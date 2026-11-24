//! Scripted misbehaving MQTT 3.1.1 broker for fault-injection tests.
//!
//! rumqttd is spec-correct, which is exactly what fault tests must not be. This
//! listener speaks enough MQTT to finish a handshake and then breaks one rule on
//! purpose: delay or drop PUBACK, echo publishes twice, or acknowledge in reverse
//! order. It is deliberately tiny — no routing, no sessions, no retained store.
//!
//! Only 3.1.1 is scripted. The MQTT 5 paths (reason codes, CONNACK capabilities)
//! are covered at the runtime level in `core/src/runtime.rs` tests.

use std::io::{Read, Write};
use std::net::{SocketAddr, TcpListener, TcpStream};
use std::thread;
use std::time::Duration;

use bytes::BytesMut;
use rumqttc::mqttbytes::v4::{
    ConnAck, ConnectReturnCode, Packet, PubAck, PubComp, PubRec, Publish, SubAck,
    SubscribeReasonCode, UnsubAck,
};
use rumqttc::mqttbytes::Error as CodecError;
use rumqttc::QoS;

/// One way for the broker to misbehave.
#[derive(Clone, Copy, Debug)]
pub enum Fault {
    /// Correct QoS 1/2 broker: ack immediately, complete PUBREL.
    Happy,
    /// Hold every PUBACK for `Duration` before sending it.
    DelayAck(Duration),
    /// Never acknowledge a PUBLISH; the TCP link stays up.
    DropAck,
    /// Echo each PUBLISH back twice (QoS 1, second copy with DUP set).
    Duplicate,
    /// Buffer PUBACKs until two publishes are pending, then ack newest first.
    OutOfOrder,
}

/// Keeps the listener thread alive; dropping it detaches the thread.
pub struct FaultBroker {
    _listener: thread::JoinHandle<()>,
}

/// Start the scripted broker on `port` (loopback) with one fault mode.
pub fn start_fault_broker(port: u16, fault: Fault) -> FaultBroker {
    let listener = TcpListener::bind(SocketAddr::from(([127, 0, 0, 1], port)))
        .unwrap_or_else(|err| panic!("bind 127.0.0.1:{port}: {err}"));
    let thread = thread::Builder::new()
        .name(format!("fault-broker-{port}"))
        .spawn(move || {
            for stream in listener.incoming() {
                let Ok(stream) = stream else { continue };
                let fault = fault;
                let _ = thread::Builder::new()
                    .name("fault-conn".to_string())
                    .spawn(move || serve(stream, fault));
            }
        })
        .expect("listener thread");
    FaultBroker { _listener: thread }
}

const MAX_PACKET: usize = 1024 * 1024;

struct Session {
    stream: TcpStream,
    out: BytesMut,
    /// PUBLISH packet ids waiting for an ack in `OutOfOrder` mode.
    held: Vec<u16>,
    next_pkid: u16,
}

impl Session {
    fn send(&mut self, packet: Packet) {
        self.out.clear();
        if packet.write(&mut self.out, MAX_PACKET).is_ok() {
            let _ = self.stream.write_all(&self.out);
        }
    }

    /// Ack one QoS 1 row; `OutOfOrder` holds it until a second one is pending.
    fn puback(&mut self, pkid: u16, fault: Fault) {
        match fault {
            Fault::DropAck => {}
            Fault::DelayAck(delay) => {
                thread::sleep(delay);
                self.send(Packet::PubAck(PubAck::new(pkid)));
            }
            Fault::OutOfOrder => {
                self.held.push(pkid);
                if self.held.len() >= 2 {
                    while let Some(pkid) = self.held.pop() {
                        self.send(Packet::PubAck(PubAck::new(pkid)));
                    }
                }
            }
            _ => self.send(Packet::PubAck(PubAck::new(pkid))),
        }
    }

    fn echo_twice(&mut self, publish: &Publish) {
        let first = self.next_pkid();
        let second = self.next_pkid();
        let topic = publish.topic.clone();
        let mut copy = Publish::from_bytes(topic.clone(), QoS::AtLeastOnce, publish.payload.clone());
        copy.pkid = first;
        self.send(Packet::Publish(copy));
        let mut dup = Publish::from_bytes(topic, QoS::AtLeastOnce, publish.payload.clone());
        dup.pkid = second;
        dup.dup = true;
        self.send(Packet::Publish(dup));
    }

    fn next_pkid(&mut self) -> u16 {
        // Server-originated ids live in the upper half to stay clear of client ids.
        self.next_pkid = self.next_pkid.wrapping_add(1).max(0x8000);
        self.next_pkid
    }
}

fn serve(stream: TcpStream, fault: Fault) {
    let _ = stream.set_nodelay(true);
    let mut session = Session {
        stream,
        out: BytesMut::with_capacity(1024),
        held: Vec::new(),
        next_pkid: 0x8000,
    };
    let mut buffer = BytesMut::with_capacity(4096);
    let mut chunk = [0u8; 4096];

    loop {
        loop {
            match Packet::read(&mut buffer, MAX_PACKET) {
                Ok(packet) => {
                    if !handle(packet, &mut session, fault) {
                        return;
                    }
                }
                Err(CodecError::InsufficientBytes(_)) => break,
                Err(_) => return,
            }
        }
        match session.stream.read(&mut chunk) {
            Ok(0) | Err(_) => return,
            Ok(n) => buffer.extend_from_slice(&chunk[..n]),
        }
    }
}

/// Handle one client packet. Returns `false` when the connection should close.
fn handle(packet: Packet, session: &mut Session, fault: Fault) -> bool {
    match packet {
        Packet::Connect(_) => {
            session.send(Packet::ConnAck(ConnAck::new(
                ConnectReturnCode::Success,
                false,
            )));
        }
        Packet::Subscribe(subscribe) => {
            let codes = subscribe
                .filters
                .iter()
                .map(|filter| SubscribeReasonCode::Success(filter.qos))
                .collect();
            session.send(Packet::SubAck(SubAck::new(subscribe.pkid, codes)));
        }
        Packet::Unsubscribe(unsubscribe) => {
            session.send(Packet::UnsubAck(UnsubAck::new(unsubscribe.pkid)));
        }
        Packet::Publish(publish) => match publish.qos {
            QoS::AtMostOnce => {}
            QoS::AtLeastOnce => {
                if matches!(fault, Fault::Duplicate) {
                    session.echo_twice(&publish);
                }
                session.puback(publish.pkid, fault);
            }
            QoS::ExactlyOnce => session.send(Packet::PubRec(PubRec::new(publish.pkid))),
        },
        Packet::PubRel(pubrel) => session.send(Packet::PubComp(PubComp::new(pubrel.pkid))),
        // PUBACK for broker-originated rows (manual ACK mode) needs no reply.
        Packet::PubAck(_) | Packet::PubRec(_) | Packet::PubComp(_) => {}
        Packet::PingReq => session.send(Packet::PingResp),
        Packet::Disconnect => return false,
        _ => {}
    }
    true
}
