//! End-to-end runtime tests against the embedded rumqttd broker.
//!
//! These cover the parts unit tests cannot: the real MQTT worker threads,
//! ConnAck → resubscribe, retained fixtures, auto-reply and timed publish.
//!
//! Subscriptions use QoS 0 on purpose: rumqttd 0.20 truncates retained
//! forwarding to the QoS>0 inflight window (100) and drops the rest
//! (`router/routing.rs` `forward_retained`), so a 200-message retained fixture
//! is only complete on a QoS 0 subscription.

use std::thread;
use std::time::{Duration, Instant};

use mqttc_core::config::{
    AppConfig, AutoReplyRule, ConnectionConfig, SubscriptionConfig,
};
use mqttc_core::runtime::AppRuntime;
use mqttc_core::ConnStatus;
use mqttc_test_broker::{publish_retained, start_broker, start_broker_dual, wait_for_port};
use rumqttc::{Client, MqttOptions, QoS};

fn wait_until(mut f: impl FnMut() -> bool, timeout: Duration) -> bool {
    let deadline = Instant::now() + timeout;
    while Instant::now() < deadline {
        if f() {
            return true;
        }
        thread::sleep(Duration::from_millis(20));
    }
    f()
}

fn config_for(port: u16, protocol: &str) -> AppConfig {
    let mut conn = ConnectionConfig::new("bench".into());
    conn.host = "127.0.0.1".into();
    conn.port = port.to_string();
    conn.protocol = protocol.into();
    conn.sub_topic = "mqttc/mvp/#".into();
    conn.subscriptions.push(SubscriptionConfig {
        topic: "mqttc/mvp/#".into(),
        qos: 0,
        ..Default::default()
    });
    let mut config = AppConfig::default();
    config.active_id = conn.id.clone();
    config.connections.push(conn);
    config
}

fn publish_once(port: u16, topic: &str, payload: &str, v5: bool) {
    if v5 {
        let mut opts =
            rumqttc::v5::MqttOptions::new(format!("probe-{}", std::process::id()), "127.0.0.1", port);
        opts.set_keep_alive(Duration::from_secs(10));
        let (client, mut connection) = rumqttc::v5::Client::new(opts, 16);
        let deadline = Instant::now() + Duration::from_secs(5);
        while Instant::now() < deadline {
            match connection.recv_timeout(Duration::from_millis(200)) {
                Ok(Ok(rumqttc::v5::Event::Incoming(
                    rumqttc::v5::mqttbytes::v5::Packet::ConnAck(_),
                ))) => break,
                Ok(_) => continue,
                Err(_) => continue,
            }
        }
        client
            .publish(
                topic,
                rumqttc::v5::mqttbytes::QoS::AtLeastOnce,
                false,
                payload.as_bytes().to_vec(),
            )
            .expect("publish");
        let _ = connection.recv_timeout(Duration::from_millis(300));
        let _ = client.disconnect();
        return;
    }

    let mut opts = MqttOptions::new(format!("probe-{}", std::process::id()), "127.0.0.1", port);
    opts.set_keep_alive(Duration::from_secs(10));
    let (client, mut connection) = Client::new(opts, 16);
    let deadline = Instant::now() + Duration::from_secs(5);
    while Instant::now() < deadline {
        match connection.recv_timeout(Duration::from_millis(200)) {
            Ok(Ok(rumqttc::Event::Incoming(rumqttc::Incoming::ConnAck(_)))) => break,
            Ok(_) => continue,
            Err(_) => continue,
        }
    }
    client
        .publish(topic, QoS::AtLeastOnce, false, payload.as_bytes().to_vec())
        .expect("publish");
    // Give the broker a moment to route before disconnecting.
    let _ = connection.recv_timeout(Duration::from_millis(300));
    let _ = client.disconnect();
}

#[test]
fn retained_fixture_fills_ring_buffer() {
    let port = 22911;
    let _broker = start_broker(port);
    assert!(wait_for_port(port, Duration::from_secs(5)));
    publish_retained(port, "mqttc/mvp", 200).expect("fixture");

    let mut rt = AppRuntime::new(Some(config_for(port, "3.1.1")), None, None);
    assert!(rt.connect());
    assert!(wait_until(
        || {
            rt.poll();
            rt.visible_logs().iter().filter(|log| log.kind == "RECV").count() == 200
        },
        Duration::from_secs(15)
    ));

    let recv = rt
        .visible_logs()
        .iter()
        .filter(|log| log.kind == "RECV")
        .count();
    // With QoS 0 all 200 retained messages arrive; the three SYS rows
    // (connecting / connected / resubscribed) are pushed first and fall off the
    // 200-row cap, leaving exactly the 200 RECV rows.
    assert_eq!(recv, 200, "all retained messages should arrive");
    assert_eq!(rt.visible_logs().len(), 200);
    rt.disconnect();
}

#[test]
fn auto_reply_and_timed_publish() {
    let port = 22912;
    let _broker = start_broker(port);
    assert!(wait_for_port(port, Duration::from_secs(5)));

    let mut config = config_for(port, "3.1.1");
    let conn = &mut config.connections[0];
    conn.sub_topic = "mqttc/in/#".into();
    conn.subscriptions = vec![SubscriptionConfig {
        topic: "mqttc/in/#".into(),
        qos: 1,
        ..Default::default()
    }];
    conn.pub_topic = "mqttc/timed".into();
    conn.pub_payload = "tick".into();
    conn.timed_interval = 1;
    conn.auto_reply_rules.push(AutoReplyRule {
        enabled: true,
        match_topic: "mqttc/in/#".into(),
        payload_contains: "ping".into(),
        reply_topic: "mqttc/out".into(),
        reply_payload: "pong".into(),
        format: "text".into(),
        ..AutoReplyRule::default()
    });

    let mut rt = AppRuntime::new(Some(config), None, None);
    assert!(rt.connect());
    assert!(wait_until(
        || {
            rt.poll();
            rt.status() == ConnStatus::Connected
        },
        Duration::from_secs(10)
    ));

    publish_once(port, "mqttc/in/ping", "ping hello", false);
    assert!(wait_until(
        || {
            rt.poll();
            rt.visible_logs()
                .iter()
                .any(|log| log.kind == "SEND" && log.topic == "mqttc/out")
        },
        Duration::from_secs(10)
    ));
    let reply = rt
        .visible_logs()
        .iter()
        .find(|log| log.kind == "SEND" && log.topic == "mqttc/out")
        .map(|log| log.payload.to_vec())
        .unwrap();
    assert_eq!(reply, b"pong");

    rt.toggle_timed();
    assert!(wait_until(
        || {
            rt.poll();
            rt.visible_logs()
                .iter()
                .filter(|log| log.kind == "SEND" && log.topic == "mqttc/timed")
                .count()
                >= 2
        },
        Duration::from_secs(6)
    ));

    rt.toggle_timed();
    thread::sleep(Duration::from_millis(200));
    rt.poll();
    let stopped_at = rt
        .visible_logs()
        .iter()
        .filter(|log| log.kind == "SEND" && log.topic == "mqttc/timed")
        .count();
    thread::sleep(Duration::from_millis(1500));
    rt.poll();
    let after = rt
        .visible_logs()
        .iter()
        .filter(|log| log.kind == "SEND" && log.topic == "mqttc/timed")
        .count();
    assert_eq!(after, stopped_at, "timed publish must stop on toggle");
    rt.disconnect();
}

#[test]
fn mqtt5_connects_subscribes_and_publishes() {
    let v4 = 22913;
    let v5 = 22914;
    let _broker = start_broker_dual(v4, v5);
    assert!(wait_for_port(v5, Duration::from_secs(5)));

    let mut config = config_for(v5, "5.0");
    config.connections[0].pub_topic = "mqttc/mvp/from-v5".into();
    config.connections[0].pub_payload = "hello v5".into();

    let mut rt = AppRuntime::new(Some(config), None, None);
    assert!(rt.connect());
    assert!(wait_until(
        || {
            rt.poll();
            rt.status() == ConnStatus::Connected
        },
        Duration::from_secs(10)
    ));

    assert!(rt.publish_form());
    assert!(wait_until(
        || {
            rt.poll();
            rt.visible_logs()
                .iter()
                .any(|log| log.kind == "SEND" && log.topic == "mqttc/mvp/from-v5")
        },
        Duration::from_secs(5)
    ));

    publish_once(v5, "mqttc/mvp/from-peer", "peer v5", true);
    assert!(wait_until(
        || {
            rt.poll();
            rt.visible_logs()
                .iter()
                .any(|log| log.kind == "RECV" && log.topic == "mqttc/mvp/from-peer")
        },
        Duration::from_secs(10)
    ));
    rt.disconnect();
}
