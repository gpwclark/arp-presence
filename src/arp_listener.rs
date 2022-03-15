use log::info;
use pnet::datalink::{self, Channel, DataLinkReceiver, NetworkInterface};
use pnet::packet::arp::{Arp, ArpPacket};
use pnet::packet::ethernet::{EtherTypes, EthernetPacket};
use pnet::packet::{FromPacket, Packet};
use std::io;
use std::io::{Error, ErrorKind};
use std::sync::mpsc::Sender;

fn recv(mut rx: Box<dyn DataLinkReceiver>, tx: Sender<Arp>) {
    loop {
        if let Ok(frame) = rx.next() {
            if let Some(pkt) = EthernetPacket::new(frame) {
                if pkt.get_ethertype() == EtherTypes::Arp {
                    if let Some(arp) = ArpPacket::new(pkt.payload()) {
                        let arp_packet = arp.from_packet();
                        match tx.send(arp_packet) {
                            Ok(_) => {}
                            Err(e) => {
                                info!("Terminating arp listening thread! {}", e);
                                break;
                            }
                        }
                    }
                }
            }
        };
    }
}

pub fn recv_arp(interface: String, tx: Sender<Arp>) -> io::Result<()> {
    let interfaces = datalink::interfaces();
    let interfaces_name_match = |iface: &NetworkInterface| iface.name == interface;
    if let Some(interface) = interfaces.into_iter().find(interfaces_name_match) {
        match datalink::channel(&interface, Default::default()) {
            Ok(Channel::Ethernet(_, rx)) => {
                recv(rx, tx);
                Ok(())
            }
            Ok(_) => Err(Error::new(ErrorKind::Other, "Unknown channel type")),
            Err(e) => Err(e),
        }
    } else {
        Err(Error::new(
            ErrorKind::Other,
            format!("Invalid interface: {}", interface),
        ))
    }
}