use arp_presence::arp_listener::recv_arp;
use clap::Parser;
use log::{error, info};
use std::thread;
use tokio::sync::broadcast;
use tokio::sync::broadcast::error::TryRecvError;

/// Simple program to listen for ARP ethernet frames
#[derive(Parser, Debug)]
#[clap(author, version, about, long_about = None)]
struct Args {
    /// Name of the person to greet
    #[clap(short, long)]
    interface: String,
}

fn main() {
    pretty_env_logger::init();

    let args = Args::parse();
    let (tx, mut rx) = broadcast::channel(256);
    thread::spawn(|| {
        if let Err(e) = recv_arp(args.interface, tx) {
            error!("{}", e);
        }
    });
    loop {
        match rx.try_recv() {
            Ok(res) => {
                info!("Received Arp from MacAddr: {:?}", res.sender_hw_addr);
            }
            Err(TryRecvError::Closed) => {
                info!("Terminating arp printing thread! {}", TryRecvError::Closed);
                break;
            }
            _ => {}
        }
    }
}
