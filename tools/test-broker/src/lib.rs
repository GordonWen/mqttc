//! Embeddable rumqttd broker plus fixture helpers.
//!
//! Used by `core/tests/broker_roundtrip.rs` and by the benchmark scripts, so the
//! exact same broker configuration backs every measurement. `fault` adds a
//! scripted non-compliant broker for fault-injection tests.

pub mod fault;

use std::collections::HashMap;
use std::net::{SocketAddr, TcpStream};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::thread;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use rumqttd::{Broker, Config, ConnectionSettings, RouterConfig, ServerSettings, Strategy};
use rumqttc::{Client, Event, Incoming, MqttOptions, QoS};

fn connection_settings() -> ConnectionSettings {
    ConnectionSettings {
        connection_timeout_ms: 30_000,
        max_payload_size: 1024 * 1024,
        max_inflight_count: 200,
        auth: None,
        external_auth: None,
        dynamic_filters: true,
    }
}

fn server(name: &str, port: u16) -> ServerSettings {
    ServerSettings {
        name: name.to_string(),
        listen: SocketAddr::from(([127, 0, 0, 1], port)),
        tls: None,
        next_connection_delay_ms: 1,
        connections: connection_settings(),
    }
}

fn base_config() -> Config {
    Config {
        id: 0,
        router: RouterConfig {
            max_connections: 1000,
            max_outgoing_packet_count: 500,
            max_segment_size: 32 * 1024 * 1024,
            max_segment_count: 16,
            custom_segment: None,
            initialized_filters: None,
            shared_subscriptions_strategy: Strategy::default(),
        },
        v4: None,
        v5: None,
        ws: None,
        cluster: None,
        console: None,
        bridge: None,
        prometheus: None,
        metrics: None,
    }
}

/// Keeps the broker thread alive.
///
/// `rumqttd::Broker::start` joins its server threads and never returns, so the
/// broker has to run on its own thread. Dropping this handle detaches the
/// thread; the process exits when `main` returns regardless.
pub struct BrokerHandle {
    _thread: thread::JoinHandle<()>,
}

fn spawn_broker(config: Config) -> BrokerHandle {
    let mut broker = Broker::new(config);
    let thread = thread::Builder::new()
        .name("mqttc-broker".to_string())
        .spawn(move || {
            let _ = broker.start();
        })
        .expect("broker thread");
    BrokerHandle { _thread: thread }
}

/// Start a 3.1.1 broker on `port`. Wait for the port before connecting.
pub fn start_broker(port: u16) -> BrokerHandle {
    let mut config = base_config();
    let mut v4 = HashMap::new();
    v4.insert("1".to_string(), server("bench-v4", port));
    config.v4 = Some(v4);
    spawn_broker(config)
}

/// Start one broker with both a 3.1.1 and a 5.0 listener.
pub fn start_broker_dual(v4_port: u16, v5_port: u16) -> BrokerHandle {
    let mut config = base_config();
    let mut v4 = HashMap::new();
    v4.insert("1".to_string(), server("bench-v4", v4_port));
    let mut v5 = HashMap::new();
    v5.insert("1".to_string(), server("bench-v5", v5_port));
    config.v4 = Some(v4);
    config.v5 = Some(v5);
    spawn_broker(config)
}

/// Poll a TCP connect until the broker listens or `timeout` elapses.
pub fn wait_for_port(port: u16, timeout: Duration) -> bool {
    let deadline = Instant::now() + timeout;
    while Instant::now() < deadline {
        if TcpStream::connect_timeout(
            &SocketAddr::from(([127, 0, 0, 1], port)),
            Duration::from_millis(100),
        )
        .is_ok()
        {
            return true;
        }
        thread::sleep(Duration::from_millis(20));
    }
    false
}

fn client_id(prefix: &str) -> String {
    let millis = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis())
        .unwrap_or(0);
    format!("{prefix}-{millis:x}")
}

/// Publish `count` retained messages to `{prefix}/000` … and wait for every PubAck.
///
/// The fixture uses QoS 1 on purpose: returning after the acks makes the data
/// available to clients that connect immediately afterwards.
pub fn publish_retained(port: u16, prefix: &str, count: usize) -> Result<(), String> {
    let mut opts = MqttOptions::new(client_id("fixture"), "127.0.0.1", port);
    opts.set_keep_alive(Duration::from_secs(30));
    // `Client::publish` blocks once the request queue is full and nothing drives
    // the event loop, so the queue must hold every fixture publish up front.
    let (client, mut connection) = Client::new(opts, count.max(1) + 16);

    // Wait for ConnAck before publishing so the session is really up.
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        let remaining = deadline.saturating_duration_since(Instant::now());
        if remaining.is_zero() {
            return Err("timed out waiting for ConnAck".into());
        }
        match connection.recv_timeout(remaining) {
            Ok(Ok(Event::Incoming(Incoming::ConnAck(_)))) => break,
            Ok(Ok(_)) => continue,
            Ok(Err(err)) => return Err(err.to_string()),
            Err(err) => return Err(format!("{err:?}")),
        }
    }

    for i in 0..count {
        let topic = format!("{prefix}/{i:03}");
        let payload = format!("fixture message {i}");
        client
            .publish(&topic, QoS::AtLeastOnce, true, payload.into_bytes())
            .map_err(|err| err.to_string())?;
    }

    let mut acks = 0usize;
    let deadline = Instant::now() + Duration::from_secs(15);
    while acks < count {
        let remaining = deadline.saturating_duration_since(Instant::now());
        if remaining.is_zero() {
            return Err(format!("timed out after {acks}/{count} PubAcks"));
        }
        match connection.recv_timeout(remaining) {
            Ok(Ok(Event::Incoming(Incoming::PubAck(_)))) => acks += 1,
            Ok(Ok(_)) => continue,
            Ok(Err(err)) => return Err(err.to_string()),
            Err(err) => return Err(format!("{err:?}")),
        }
    }
    let _ = client.disconnect();
    Ok(())
}

/// Subscribe to `topic` on a running broker, count messages for `wait`, then disconnect.
///
/// Used by the benchmark scripts to prove the retained fixture really exists
/// before blaming a UI for receiving nothing.
pub fn probe(port: u16, topic: &str, wait: Duration) -> Result<usize, String> {
    let mut opts = MqttOptions::new(client_id("probe"), "127.0.0.1", port);
    opts.set_keep_alive(Duration::from_secs(10));
    let (client, mut connection) = Client::new(opts, 64);

    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        let remaining = deadline.saturating_duration_since(Instant::now());
        if remaining.is_zero() {
            return Err("timed out waiting for ConnAck".into());
        }
        match connection.recv_timeout(remaining) {
            Ok(Ok(Event::Incoming(Incoming::ConnAck(_)))) => break,
            Ok(Ok(_)) => continue,
            Ok(Err(err)) => return Err(err.to_string()),
            Err(err) => return Err(format!("{err:?}")),
        }
    }

    client
        .subscribe(topic, QoS::AtMostOnce)
        .map_err(|err| err.to_string())?;

    let deadline = Instant::now() + wait;
    let mut count = 0usize;
    while Instant::now() < deadline {
        match connection.recv_timeout(Duration::from_millis(200)) {
            Ok(Ok(Event::Incoming(Incoming::Publish(_)))) => count += 1,
            Ok(Ok(_)) => {}
            Ok(Err(err)) => return Err(err.to_string()),
            Err(_) => {}
        }
    }
    let _ = client.disconnect();
    Ok(count)
}

pub struct StreamHandles {
    pub stop: Arc<AtomicBool>,
    pub workers: Vec<thread::JoinHandle<()>>,
}

/// Publish to `topic` at `rate_per_sec` until both handles are dropped / stopped.
pub fn spawn_stream(port: u16, topic: &str, rate_per_sec: u32) -> Result<StreamHandles, String> {
    let rate = rate_per_sec.max(1);
    let mut opts = MqttOptions::new(client_id("stream"), "127.0.0.1", port);
    opts.set_keep_alive(Duration::from_secs(30));
    let (client, mut connection) = Client::new(opts, 64);

    let stop = Arc::new(AtomicBool::new(false));
    let stop_worker = stop.clone();
    let worker = thread::spawn(move || {
        for event in connection.iter() {
            if stop_worker.load(Ordering::Relaxed) {
                break;
            }
            if event.is_err() {
                break;
            }
        }
    });

    let stop_pub = stop.clone();
    let topic = topic.to_string();
    let publisher = thread::spawn(move || {
        let interval = Duration::from_micros(1_000_000 / rate as u64);
        let mut seq = 0u64;
        while !stop_pub.load(Ordering::Relaxed) {
            let payload = format!("{{\"seq\":{seq},\"value\":\"streaming\"}}");
            if client
                .publish(&topic, QoS::AtMostOnce, false, payload.into_bytes())
                .is_err()
            {
                break;
            }
            seq += 1;
            thread::sleep(interval);
        }
    });

    Ok(StreamHandles {
        stop,
        workers: vec![worker, publisher],
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn broker_starts_and_accepts_retained_fixture() {
        let port = 21886;
        let _broker = start_broker(port);
        assert!(wait_for_port(port, Duration::from_secs(5)));
        publish_retained(port, "test/fixture", 200).expect("fixture");

        let mut opts = MqttOptions::new(client_id("check"), "127.0.0.1", port);
        opts.set_keep_alive(Duration::from_secs(10));
        let (client, mut connection) = Client::new(opts, 32);
        // QoS 0: rumqttd 0.20 truncates retained forwarding to the QoS>0
        // inflight window (100) and drops the rest, so QoS 0 is required to
        // receive a 200-message retained fixture.
        client.subscribe("test/fixture/#", QoS::AtMostOnce).unwrap();
        let deadline = Instant::now() + Duration::from_secs(10);
        let mut received = 0;
        while received < 200 && Instant::now() < deadline {
            if let Ok(Ok(Event::Incoming(Incoming::Publish(_)))) =
                connection.recv_timeout(Duration::from_millis(500))
            {
                received += 1;
            }
        }
        assert_eq!(received, 200);
    }
}
