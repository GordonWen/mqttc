//! End-to-end round trips against the embedded rumqttd broker.
//!
//! These tests exercise the real `AppRuntime` (config → form → MQTT worker →
//! event queue → logs / auto reply / timed publish), not just codecs.
//! Each test owns its own port so the file can run in parallel.

use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::{Duration, Instant};

use mqttc_core::config::{AppConfig, AutoReplyRule, ConnectionConfig, SubscriptionConfig};
use mqttc_core::form::{PresetRow, RuleRow};
use mqttc_core::model::ConnStatus;
use mqttc_core::runtime::AppRuntime;
use mqttc_test_broker::{start_broker, start_broker_dual, wait_for_port};

static NEXT: AtomicUsize = AtomicUsize::new(0);

fn unique_client_id() -> String {
    let n = NEXT.fetch_add(1, Ordering::Relaxed);
    format!("mqttc-it-{}-{n}", std::process::id())
}

fn app(port: u16, protocol: &str) -> AppRuntime {
    let mut conn = ConnectionConfig::new("integration".to_string());
    conn.host = "127.0.0.1".into();
    conn.port = port.to_string();
    conn.client_id = unique_client_id();
    conn.protocol = protocol.into();
    conn.keep_alive = 30;
    conn.sub_topic = "mqttc/it/#".into();
    conn.sub_qos = 0;
    conn.pub_topic = "mqttc/it/hello".into();
    conn.pub_payload = "hello".into();
    conn.pub_qos = 0;
    conn.pub_format = "text".into();
    conn.subscriptions = vec![SubscriptionConfig {
        topic: "mqttc/it/#".into(),
        qos: 0,
        ..Default::default()
    }];
    conn.auto_reply_rules = vec![AutoReplyRule {
        enabled: true,
        match_topic: "mqttc/it/#".into(),
        payload_contains: "ping".into(),
        reply_topic: "mqttc/it/reply".into(),
        reply_payload: "pong".into(),
        qos: 0,
        retain: false,
        format: "text".into(),
        min_interval_secs: 0,
        ..Default::default()
    }];

    let mut cfg = AppConfig::default();
    cfg.active_id = conn.id.clone();
    cfg.connections = vec![conn];
    AppRuntime::new(Some(cfg), None, None)
}

fn pump_until(rt: &mut AppRuntime, secs: u64, mut done: impl FnMut(&AppRuntime) -> bool) -> bool {
    let deadline = Instant::now() + Duration::from_secs(secs);
    loop {
        rt.poll();
        if done(rt) {
            return true;
        }
        if Instant::now() >= deadline {
            return false;
        }
        std::thread::sleep(Duration::from_millis(15));
    }
}

fn has_log(rt: &AppRuntime, kind: &str, topic: &str, payload: &[u8]) -> bool {
    rt.visible_logs().iter().any(|log| {
        log.kind == kind && log.topic == topic && log.payload.as_ref() == payload
    })
}

fn connected(rt: &AppRuntime) -> bool {
    rt.status() == ConnStatus::Connected
}

#[test]
fn v311_roundtrip_preset_and_auto_reply() {
    let port = 21901;
    let _broker = start_broker(port);
    assert!(wait_for_port(port, Duration::from_secs(10)), "broker did not listen");

    let mut rt = app(port, "3.1.1");
    assert!(rt.connect());
    assert!(pump_until(&mut rt, 10, connected), "never reached Connected");

    // Plain publish → broker → subscribed client → RECV log.
    assert!(rt.publish_form(), "publish failed");
    assert!(
        pump_until(&mut rt, 10, |rt| has_log(rt, "RECV", "mqttc/it/hello", b"hello")),
        "hello was not delivered back"
    );

    // Preset row: sends its own topic/payload without touching the bottom form.
    rt.form.presets = vec![PresetRow {
        name: "preset".into(),
        topic: "mqttc/it/preset".into(),
        payload: "from-preset".into(),
        qos: 0,
        retain: false,
        format: 0,
        ..Default::default()
    }];
    assert!(rt.publish_preset(0), "preset publish failed");
    assert!(
        pump_until(&mut rt, 10, |rt| has_log(rt, "RECV", "mqttc/it/preset", b"from-preset")),
        "preset payload was not delivered"
    );

    // Auto reply: payload containing "ping" triggers one SEND to mqttc/it/reply.
    rt.form.pub_payload = "ping-42".into();
    assert!(rt.publish_form());
    assert!(
        pump_until(&mut rt, 10, |rt| has_log(rt, "SEND", "mqttc/it/reply", b"pong")),
        "auto reply was not sent"
    );
    assert!(
        pump_until(&mut rt, 10, |rt| has_log(rt, "RECV", "mqttc/it/hello", b"ping-42")),
        "ping payload was not delivered"
    );

    // The reply lands back through our own subscription; it must not re-trigger
    // the rule (otherwise the client would loop forever).
    std::thread::sleep(Duration::from_millis(400));
    rt.poll();
    let replies = rt
        .visible_logs()
        .iter()
        .filter(|log| log.kind == "SEND" && log.topic == "mqttc/it/reply")
        .count();
    assert_eq!(replies, 1, "auto reply looped on its own echo");

    rt.disconnect();
    assert_eq!(rt.status(), ConnStatus::Disconnected);
}

#[test]
fn overlapping_subscriptions_log_one_recv() {
    let port = 21909;
    let _broker = start_broker(port);
    assert!(wait_for_port(port, Duration::from_secs(10)), "broker did not listen");

    let mut rt = app(port, "3.1.1");
    // A second filter that also matches mqttc/it/hello, at a different QoS.
    rt.config.connections[0]
        .subscriptions
        .push(SubscriptionConfig {
            topic: "mqttc/it/hello".into(),
            qos: 1,
            ..SubscriptionConfig::default()
        });
    assert!(rt.connect());
    assert!(pump_until(&mut rt, 10, connected), "never reached Connected");

    assert!(rt.publish_form(), "publish failed");
    assert!(
        pump_until(&mut rt, 10, |rt| has_log(rt, "RECV", "mqttc/it/hello", b"hello")),
        "hello was not delivered"
    );
    // Let the broker's second copy arrive and drain it.
    std::thread::sleep(Duration::from_millis(400));
    rt.poll();

    let rows = rt
        .visible_logs()
        .iter()
        .filter(|log| {
            log.kind == "RECV" && log.topic == "mqttc/it/hello" && log.payload.as_ref() == b"hello"
        })
        .count();
    assert_eq!(rows, 1, "one publish must log one RECV row");
    assert_eq!(rt.active_counts().0, 1, "one publish must count once");
    rt.disconnect();
}

#[test]
fn v5_connect_publish_receive() {
    let broker = start_broker_dual(21902, 21903);
    assert!(wait_for_port(21903, Duration::from_secs(10)), "v5 listener missing");

    let mut rt = app(21903, "5.0");
    assert!(rt.connect());
    assert!(pump_until(&mut rt, 10, connected), "MQTT 5.0 never connected");

    assert!(rt.publish_form());
    assert!(
        pump_until(&mut rt, 10, |rt| has_log(rt, "RECV", "mqttc/it/hello", b"hello")),
        "MQTT 5.0 round trip failed"
    );
    rt.disconnect();
    drop(broker);
}

#[test]
fn v5_preset_and_reply_carry_properties() {
    let broker = start_broker_dual(21907, 21908);
    assert!(wait_for_port(21908, Duration::from_secs(10)), "v5 listener missing");

    let mut rt = app(21908, "5.0");
    assert!(rt.connect());
    assert!(pump_until(&mut rt, 10, connected), "MQTT 5.0 never connected");

    // Preset rows send their own user properties and expiry.
    rt.form.presets = vec![PresetRow {
        name: "props".into(),
        topic: "mqttc/it/props".into(),
        payload: "with-props".into(),
        qos: 1,
        props: "k=v".into(),
        expiry: "30".into(),
        ..Default::default()
    }];
    assert!(rt.publish_preset(0), "preset publish failed");
    assert!(
        pump_until(&mut rt, 10, |rt| {
            rt.visible_logs().iter().any(|log| {
                log.kind == "SEND"
                    && log.topic == "mqttc/it/props"
                    && log.props.iter().any(|(k, v)| k == "k" && v == "v")
                    && log.expiry == Some(30)
            })
        }),
        "preset properties missing from the SEND row"
    );

    // Auto replies use the same options path.
    rt.form.rules = vec![RuleRow {
        enabled: true,
        match_topic: "mqttc/it/ping".into(),
        reply_topic: "mqttc/it/rule-reply".into(),
        reply_payload: "pong5".into(),
        props: "role=echo".into(),
        expiry: "15".into(),
        ..Default::default()
    }];
    rt.form.pub_topic = "mqttc/it/ping".into();
    rt.form.pub_payload = "ping5".into();
    assert!(rt.publish_form());
    assert!(
        pump_until(&mut rt, 10, |rt| {
            rt.visible_logs().iter().any(|log| {
                log.kind == "SEND"
                    && log.topic == "mqttc/it/rule-reply"
                    && log.props.iter().any(|(k, v)| k == "role" && v == "echo")
                    && log.expiry == Some(15)
            })
        }),
        "reply properties missing from the SEND row"
    );

    rt.disconnect();
    drop(broker);
}

#[test]
fn log_buffer_caps_at_200() {
    let mut rt = app(21904, "3.1.1"); // no broker needed
    let id = rt.active_id().expect("active connection");
    for i in 0..250 {
        rt.push_log(&id, "SYS", "", format!("line {i}").into_bytes());
    }
    let logs = rt.visible_logs();
    assert_eq!(logs.len(), 200, "log ring should keep the newest 200");
    assert_eq!(logs.first().unwrap().payload.as_ref(), b"line 50");
    assert_eq!(logs.last().unwrap().payload.as_ref(), b"line 249");
}

#[test]
fn timed_publish_sends_and_stops() {
    let port = 21905;
    let _broker = start_broker(port);
    assert!(wait_for_port(port, Duration::from_secs(10)), "broker did not listen");

    let mut rt = app(port, "3.1.1");
    assert!(rt.connect());
    assert!(pump_until(&mut rt, 10, connected), "never reached Connected");

    rt.form.pub_topic = "mqttc/it/timed".into();
    rt.form.pub_payload = "tick".into();
    rt.form.timed_interval = "1".into();
    rt.toggle_timed();
    assert!(rt.timed_running(), "timed publish did not start");

    // The SYS "started" row must precede the immediate first SEND, so the log
    // reads start → first message instead of the first message arriving first.
    {
        let logs = rt.visible_logs();
        let first_send = logs
            .iter()
            .position(|log| log.kind == "SEND" && log.topic == "mqttc/it/timed")
            .expect("first timed send missing");
        assert!(first_send > 0, "first timed send has no preceding row");
        let start_row = &logs[first_send - 1];
        assert_eq!(start_row.kind, "SYS", "SYS start row must precede the first SEND");
        let text = String::from_utf8_lossy(&start_row.payload);
        assert!(
            text.contains("定时发布") || text.contains("Timed publish"),
            "unexpected row before first SEND: {text}"
        );
    }

    assert!(
        pump_until(&mut rt, 6, |rt| {
            rt.visible_logs()
                .iter()
                .filter(|log| log.kind == "SEND" && log.topic == "mqttc/it/timed")
                .count()
                >= 3
        }),
        "expected at least three timed sends (first is immediate)"
    );

    rt.toggle_timed();
    assert!(!rt.timed_running(), "timed publish did not stop");
    rt.disconnect();
}

#[test]
fn reconnect_after_disconnect() {
    let port = 21906;
    let _broker = start_broker(port);
    assert!(wait_for_port(port, Duration::from_secs(10)), "broker did not listen");

    let mut rt = app(port, "3.1.1");
    assert!(rt.connect());
    assert!(pump_until(&mut rt, 10, connected), "first connect failed");

    rt.disconnect();
    assert_eq!(rt.status(), ConnStatus::Disconnected);

    assert!(rt.connect(), "second connect refused");
    assert!(pump_until(&mut rt, 10, connected), "reconnect failed");

    // The saved subscription list is replayed after CONNACK.
    assert!(
        pump_until(&mut rt, 10, |rt| {
            rt.visible_logs()
                .iter()
                .any(|log| log.kind == "SYS" && log.topic == "mqttc/it/#")
        }),
        "subscriptions were not replayed"
    );
    rt.disconnect();
}

/// TLS against a broker with a self-signed certificate is not part of the
/// embedded broker (rumqttd runs plaintext). Covered manually against the
/// external test servers instead; see docs/BENCHMARK.md.
#[test]
#[ignore = "needs a TLS listener + test certificate"]
fn tls_roundtrip() {}
