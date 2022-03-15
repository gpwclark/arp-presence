use arp_presence::arp_listener::recv_arp;
use clap::Parser;
use std::sync::mpsc;
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
    let args = Args::parse();
    let (tx, rx) = mpsc::channel();
    thread::spawn(|| {
        recv_arp(args.interface, tx).unwrap();
    });
    loop {
        if let Ok(res) = rx.try_recv() {
            println!("Received Arp from MacAddr: {:?}", res.sender_hw_addr);
        }
    }
}
