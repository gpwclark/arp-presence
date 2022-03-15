use pnet::datalink::{self, Channel, NetworkInterface};
use pnet::packet::arp::ArpPacket;
use pnet::packet::ethernet::{EtherTypes, EthernetPacket};
use pnet::packet::{FromPacket, Packet};
use std::io::ErrorKind;

fn main() {
    let interfaces = datalink::interfaces();

    let interfaces_name_match = |iface: &NetworkInterface| iface.name == "wlp4s0";
    let interface = interfaces.into_iter().find(interfaces_name_match).unwrap();

    let (_, mut rx) = match datalink::channel(&interface, Default::default()) {
        Ok(Channel::Ethernet(tx, rx)) => (tx, rx),
        Ok(_) => panic!("Unknown channel type"),
        Err(e) => panic!("Error happened {}", e),
    };
    loop {
        let arp_packet = match rx.next() {
            Ok(frame) => {
                let pkt = EthernetPacket::new(frame).unwrap();
                match pkt.get_ethertype() {
                    EtherTypes::Arp => match ArpPacket::new(pkt.payload()) {
                        Some(arp) => Ok(Some(arp.from_packet())),
                        None => Ok(None),
                    },
                    _ => Ok(None),
                }
            }
            Err(e) => {
                if e.kind() == ErrorKind::TimedOut {
                    Ok(None)
                } else {
                    Err(e)
                }
            }
        };
        // some arp: Arp { hardware_type: ArpHardwareType(1),
        // protocol_type: EtherType(2048),
        // hw_addr_len: 6,
        // proto_addr_len: 4,
        // operation: ArpOperation(1),
        // sender_hw_addr: 18:1b:53:91:51:18,
        // sender_proto_addr: 10.0.0.4,
        // target_hw_addr: 00:00:00:00:00:00,
        // target_proto_addr: 10.0.0.23,
        // payload: [] }.
        if let Ok(arp_packet) = arp_packet {
            if let Some(arp_packet) = arp_packet {
                println! {"some arp: {:?}.", arp_packet};
            } else {
                println! {"none arp: {:?}.", arp_packet};
            }
        } else {
            println! {"err arp: {:?}.", arp_packet};
        }
    }
}
