use arp_presence::arp_listener::recv_arp;
use clap::lazy_static::lazy_static;
use clap::Parser;
use futures_util::{SinkExt, StreamExt, TryFutureExt};
use log::{error, warn};
use pnet::packet::arp::Arp;
use pnet::util::MacAddr;
use serde::Serialize;
use std::collections::HashMap;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::thread;
use std::time::{SystemTime, UNIX_EPOCH};
use tera::{Context, Tera};
use tokio::sync::broadcast;
use tokio::sync::broadcast::error::{RecvError, TryRecvError};
use tokio::sync::broadcast::Receiver;
use warp::ws::{Message, WebSocket};
use warp::Filter;

static INDEX_HTML_FILE: &str = "index.html";
static PARTIAL_HTML_FILE: &str = "partial.html";

lazy_static! {
    pub static ref TEMPLATES: Tera = {
        let mut tera = match Tera::new("examples/templates/**/*") {
            Ok(t) => t,
            Err(e) => {
                error!("Parsing error(s): {}", e);
                ::std::process::exit(1);
            }
        };
        tera.autoescape_on(vec![".html", ".sql"]);
        //tera.register_filter("do_nothing", do_nothing_filter);
        tera
    };
}

static NEXT_CONNECTION_ID: AtomicUsize = AtomicUsize::new(1);

/// Simple program to listen for ARP ethernet frames
#[derive(Parser, Debug)]
#[clap(author, version, about, long_about = None)]
struct Args {
    /// Name of the person to greet
    #[clap(short, long)]
    interface: String,

    #[clap(short, long)]
    port: u16,
}

#[tokio::main]
async fn main() {
    pretty_env_logger::init();

    let args = Args::parse();

    let (tx, _) = broadcast::channel(256);
    let tx2 = tx.clone();
    thread::spawn(|| {
        if let Err(e) = recv_arp(args.interface, tx2) {
            error!("{}", e);
        }
    });

    let arps = warp::path("arps")
        .and(warp::ws())
        .map(move |ws: warp::ws::Ws| {
            // This will call our function if the handshake succeeds.
            let rx = tx.subscribe();
            ws.on_upgrade(move |socket| new_connection(socket, rx))
        });

    let tmpl = TEMPLATES
        .render(INDEX_HTML_FILE, &Context::new())
        .unwrap_or_default();

    // hit root, return index.html
    let index = warp::path::end().map(move || warp::reply::html(tmpl.clone()));

    let routes = index.or(arps);

    warp::serve(routes).run(([127, 0, 0, 1], args.port)).await;
}

#[derive(Serialize)]
struct LastHeardFrom {
    pub addr: String,
    pub count: u32,
    pub duration_since: u64,
    pub last_heard_from: SystemTime,
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
        let duration_since = last_heard_from
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis() as u64;
        LastHeardFrom {
            addr,
            count,
            last_heard_from,
            duration_since,
        }
    }
    fn update(&mut self) {
        let now = SystemTime::now();
        self.count = self.count + 1;
        self.duration_since = now
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis() as u64;
        self.last_heard_from = now;
    }
}

async fn new_connection(ws: WebSocket, mut receiver: Receiver<Arp>) {
    let curr_id = NEXT_CONNECTION_ID.fetch_add(1, Ordering::Relaxed);

    let (mut ws_tx, mut ws_rx) = ws.split();

    tokio::task::spawn(async move {
        let mut map: HashMap<MacAddr, LastHeardFrom> = HashMap::new();
        let mut tx_broken = false;
        loop {
            match receiver.recv().await {
                Ok(res) => {
                    let mac_addr = res.sender_hw_addr;

                    map.entry(mac_addr)
                        .or_insert_with(|| LastHeardFrom::new(mac_addr.to_string()))
                        .update();

                    let mut context = Context::new();
                    let mut entries = map.values().collect::<Vec<&LastHeardFrom>>();
                    entries.sort();
                    context.insert("entries", &entries);
                    let tmpl = TEMPLATES
                        .render(PARTIAL_HTML_FILE, &context)
                        .unwrap_or_default();
                    ws_tx
                        .send(Message::text(tmpl))
                        .unwrap_or_else(|e| {
                            error!("Failed to send message, closing socket {}: {}", curr_id, e);
                            tx_broken = true;
                        })
                        .await;
                }
                Err(RecvError::Closed) => {
                    warn!("Terminating arp printing thread! {}", TryRecvError::Closed);
                    break;
                }
                _ => {}
            }
            if tx_broken {
                break;
            }
        }
    });

    while let Some(result) = ws_rx.next().await {
        match result {
            Ok(_) => {}
            Err(e) => {
                error!("websocket error(cid={}): {}", curr_id, e);
                break;
            }
        };
    }
}
