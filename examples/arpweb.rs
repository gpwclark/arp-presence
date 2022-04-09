use arp_presence::arp_listener::recv_arp;
use clap::lazy_static::lazy_static;
use clap::Parser;
use futures_util::{SinkExt, StreamExt, TryFutureExt};
use log::{debug, error, info, warn};
use pnet::util::MacAddr;
use serde::Serialize;
use std::collections::HashMap;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use tera::{Context, Tera};
use tokio::sync::{mpsc, RwLock};
use tokio::task;
use tokio::time;
use warp::ws::{Message, WebSocket};
use warp::Filter;

static INDEX_HTML_FILE: &str = "index.html";
static PARTIAL_HTML_FILE: &str = "partial.html";

type KnownMacAddrs = Arc<RwLock<HashMap<MacAddr, LastHeardFrom>>>;

lazy_static! {
    pub static ref TEMPLATES: Tera = {
        let mut tera = match Tera::new("examples/templates/**/*") {
            Ok(t) => t,
            Err(e) => {
                error!("Parsing error(s): {}", e);
                ::std::process::exit(1);
            }
        };
        tera.autoescape_on(vec![".html"]);
        tera
    };
}

static NEXT_CONNECTION_ID: AtomicUsize = AtomicUsize::new(1);

/// Simple program to listen for ARP ethernet frames
#[derive(Parser, Debug)]
#[clap(author, version, about, long_about = None)]
struct Args {
    /// Name of interface to listen for ARPs on
    #[clap(short, long)]
    interface: String,

    /// port to host web server
    #[clap(short, long)]
    port: u16,

    /// ms timer to use for red/green row coloring
    #[clap(short, long)]
    threshold_ms: u64,

    /// how frequently to send clients current ARPs
    #[clap(short, long)]
    update_freq_ms: u64,
}

#[tokio::main]
async fn main() {
    pretty_env_logger::init();

    let args = Args::parse();
    let known_mac_addrs = KnownMacAddrs::default();

    update_mac_addrs_task(known_mac_addrs.clone(), args.threshold_ms, args.interface);
    info!("Clients will be notified every {} ms", args.update_freq_ms);

    let known_mac_addrs = warp::any().map(move || known_mac_addrs.clone());

    let arps = warp::path("arps").and(warp::ws()).and(known_mac_addrs).map(
        move |ws: warp::ws::Ws, known_mac_addrs| {
            ws.on_upgrade(move |socket| {
                new_connection(socket, args.update_freq_ms, known_mac_addrs)
            })
        },
    );

    let mut context = Context::new();
    context.insert("threshold_ms", &args.threshold_ms);
    let tmpl = TEMPLATES
        .render(INDEX_HTML_FILE, &context)
        .unwrap_or_default();

    // hit root, return index.html
    let index = warp::path::end().map(move || warp::reply::html(tmpl.clone()));

    let routes = index.or(arps);

    warp::serve(routes).run(([0, 0, 0, 0], args.port)).await;
}

fn update_mac_addrs_task(
    known_mac_addrs: Arc<RwLock<HashMap<MacAddr, LastHeardFrom>>>,
    threshold: u64,
    interface: String,
) {
    let (tx, mut rx) = mpsc::unbounded_channel();
    task::spawn_blocking(|| {
        if let Err(e) = recv_arp(interface, tx) {
            error!("{}", e);
        }
    });
    tokio::task::spawn(async move {
        loop {
            match rx.recv().await {
                Some(res) => {
                    let mac_addr = res.sender_hw_addr;
                    debug!("Arp packet from sender: {}.", mac_addr);
                    known_mac_addrs
                        .write()
                        .await
                        .entry(mac_addr)
                        .or_insert_with(|| LastHeardFrom::new(mac_addr.to_string()))
                        .update(threshold);
                }
                None => {
                    warn!("Terminating arp printing thread!");
                    break;
                }
            }
        }
    });
}

#[derive(Serialize)]
struct LastHeardFrom {
    pub addr: String,
    pub count: u32,
    pub duration_since: u64,
    pub last_heard_from: SystemTime,
    pub threshold: u64,
}

impl PartialEq for LastHeardFrom {
    fn eq(&self, other: &Self) -> bool {
        self.duration_since.eq(&other.duration_since)
    }
}

impl Eq for LastHeardFrom {}

impl PartialOrd<Self> for LastHeardFrom {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        self.duration_since.partial_cmp(&other.duration_since)
    }
}

impl Ord for LastHeardFrom {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        self.duration_since.cmp(&other.duration_since)
    }
}

impl LastHeardFrom {
    pub fn new(addr: String) -> Self {
        let count = 1;
        let last_heard_from = SystemTime::now();
        let threshold = last_heard_from
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis() as u64;
        let duration_since = last_heard_from
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis() as u64;
        LastHeardFrom {
            addr,
            count,
            last_heard_from,
            duration_since,
            threshold,
        }
    }

    fn update(&mut self, threshold: u64) {
        let now = SystemTime::now();
        self.count += 1;
        self.duration_since = now
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis() as u64;
        let last_heard_epoch = self
            .last_heard_from
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis() as u64;
        let diff = self.duration_since - last_heard_epoch;
        if diff >= threshold {
            self.threshold = now
                .duration_since(UNIX_EPOCH)
                .unwrap_or_default()
                .as_millis() as u64;
        }
        self.last_heard_from = now;
    }
}

async fn new_connection(ws: WebSocket, update_freq: u64, connected_clients: KnownMacAddrs) {
    let mut interval = time::interval(Duration::from_millis(update_freq));
    let curr_id = NEXT_CONNECTION_ID.fetch_add(1, Ordering::Relaxed);

    debug!("spawn socket#={}", curr_id);
    let (mut ws_tx, mut ws_rx) = ws.split();

    tokio::task::spawn(async move {
        let mut tx_broken = false;
        loop {
            interval.tick().await;
            let mut context = Context::new();
            let entries = connected_clients.read().await;
            let mut entries = entries.values().collect::<Vec<&LastHeardFrom>>();
            entries.sort();
            entries.reverse();
            context.insert("entries", &entries);
            let tmpl = TEMPLATES
                .render(PARTIAL_HTML_FILE, &context)
                .unwrap_or_default();
            ws_tx
                .send(Message::text(tmpl))
                .unwrap_or_else(|e| {
                    error!("Failed to send message, closing socket# {}: {}", curr_id, e);
                    tx_broken = true;
                })
                .await;
            if tx_broken {
                break;
            }
        }
    });

    while let Some(result) = ws_rx.next().await {
        match result {
            Ok(_) => {}
            Err(e) => {
                error!("websocket error(socket#={}): {}", curr_id, e);
                break;
            }
        };
    }
}
