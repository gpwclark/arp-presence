use arp_presence::arp_listener::recv_arp;
use clap::Parser;
use log::LevelFilter;
use log::{error, info};
use std::sync::mpsc;
use std::sync::mpsc::TryRecvError;
use std::thread;

/// Simple program to listen for ARP ethernet frames
#[derive(Parser, Debug)]
#[clap(author, version, about, long_about = None)]
struct Args {
    /// Name of the person to greet
    #[clap(short, long)]
    interface: String,
}

fn main() {
    simple_logging::log_to_stderr(LevelFilter::Info);

    let args = Args::parse();
    let (tx, rx) = mpsc::channel();
    thread::spawn(|| {
        // unwrap is wrong, if something blows up, e.g. wrong interface, need a way to terminate loop
        // in main
        if let Err(e) = recv_arp(args.interface, tx) {
            error!("{}", e);
        }
    });
    loop {
        match rx.try_recv() {
            Ok(res) => {
                info!("Received Arp from MacAddr: {:?}", res.sender_hw_addr);
            }
            Err(TryRecvError::Disconnected) => {
                info!(
                    "Terminating arp printing thread! {}",
                    TryRecvError::Disconnected
                );
                break;
            }
            _ => {}
        }
    }
}
