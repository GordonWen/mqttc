//! Fault-injection tests against the scripted broker in
//! [`mqttc_test_broker::fault`].
//!
//! These cover what rumqttd never does: a slow PUBACK, a lost PUBACK, acks that
//! arrive out of order, duplicated deliveries and manual acknowledgements. The
//! 15 s ack timeout itself is unit-tested in `runtime.rs`; here the link-loss
//! path (`disconnect` → result unknown) is exercised end to end.

use std::thread;
use std::time::{Duration, Instant};

use mqttc_core::config::{AppConfig, ConnectionConfig, SubscriptionConfig};
use mqttc_core::model::{AckState, LogRecord};
use mqttc_core::runtime::AppRuntime;
use mqttc_core::ConnStatus;
use mqttc_test_broker::fault::{start_fault_broker, Fault};

fn wait_until(
    rt: &mut AppRuntime,
    timeout: Duration,
    mut done: impl FnMut(&AppRuntime) -> bool,
) -> bool {
    let deadline = Instant::now() + timeout;
    loop {
        rt.poll();
        if done(rt) {
            return true;
        }
        if Instant::now() >= deadline {
            return false;
        }
        thread::sleep(Duration::from_millis(20));
    }
}

fn config_for(port: u16, qos: u8, manual_acks: bool) -> AppConfig {
    let mut conn = ConnectionConfig::new("fault".into());
    conn.host = "127.0.0.1".into();
    conn.port = port.to_string();
    conn.protocol = "3.1.1".into();
    conn.keep_alive = 30;
    conn.auto_reconnect = false;
    conn.manual_acks = manual_acks;
    conn.sub_topic = "mqttc/fault/#".into();
    conn.sub_qos = 1;
    conn.subscriptions.push(SubscriptionConfig {
        topic: "mqttc/fault/#".into(),
        qos: 1,
        ..Default::default()
    });
    conn.pub_topic = "mqttc/fault/tx".into();
    conn.pub_payload = "ping".into();
    conn.pub_qos = qos;
    conn.pub_format = "text".into();

    let mut config = AppConfig::default();
    config.active_id = conn.id.clone();
    config.connections.push(conn);
    config
}

fn send_rows(rt: &AppRuntime) -> Vec<LogRecord> {
    rt.visible_logs()
        .into_iter()
        .filter(|log| log.kind == "SEND")
        .cloned()
        .collect()
}

fn send_state(rt: &AppRuntime) -> Option<AckState> {
    send_rows(rt)
        .first()
        .and_then(|log| log.ack.as_ref())
        .map(|ack| ack.state)
}

fn connect(rt: &mut AppRuntime) {
    assert!(rt.connect(), "connect() rejected the config");
    assert!(
        wait_until(rt, Duration::from_secs(5), |rt| rt.status()
            == ConnStatus::Connected),
        "handshake did not finish"
    );
}

#[test]
fn delayed_puback_moves_awaiting_to_acked() {
    let port = 22941;
    let _broker = start_fault_broker(port, Fault::DelayAck(Duration::from_millis(800)));
    let mut rt = AppRuntime::new(Some(config_for(port, 1, false)), None, None);
    connect(&mut rt);

    assert!(rt.publish_form());
    assert!(
        wait_until(&mut rt, Duration::from_secs(2), |rt| send_state(rt)
            == Some(AckState::AwaitingAck)),
        "SEND row should wait for the delayed PUBACK, got {:?}",
        send_state(&rt)
    );
    assert!(
        wait_until(&mut rt, Duration::from_secs(3), |rt| send_state(rt)
            == Some(AckState::Acked)),
        "delayed PUBACK never landed, got {:?}",
        send_state(&rt)
    );

    let ack = send_rows(&rt)[0].ack.clone().expect("ack info");
    assert!(ack.pkid.is_some(), "acked row keeps the broker packet id");
    assert!(
        ack.elapsed_ms.unwrap_or(0) >= 700,
        "elapsed should cover the delay, got {:?}",
        ack.elapsed_ms
    );
    rt.disconnect();
}

#[test]
fn dropped_puback_survives_until_link_loss_marks_it_unknown() {
    let port = 22942;
    let _broker = start_fault_broker(port, Fault::DropAck);
    let mut rt = AppRuntime::new(Some(config_for(port, 1, false)), None, None);
    connect(&mut rt);

    assert!(rt.publish_form());
    assert!(
        wait_until(&mut rt, Duration::from_secs(2), |rt| send_state(rt)
            == Some(AckState::AwaitingAck)),
        "SEND row should stay awaiting the missing PUBACK, got {:?}",
        send_state(&rt)
    );
    // Still waiting after a moment: no ack was invented locally.
    thread::sleep(Duration::from_millis(300));
    rt.poll();
    assert_eq!(send_state(&rt), Some(AckState::AwaitingAck));

    rt.disconnect();
    rt.poll();
    assert_eq!(
        send_state(&rt),
        Some(AckState::Timeout),
        "link loss must not leave the row waiting forever"
    );
}

#[test]
fn out_of_order_pubacks_land_on_the_right_rows() {
    let port = 22943;
    let _broker = start_fault_broker(port, Fault::OutOfOrder);
    let mut rt = AppRuntime::new(Some(config_for(port, 1, false)), None, None);
    connect(&mut rt);

    assert!(rt.publish_form());
    assert!(rt.publish_form());
    assert!(
        wait_until(&mut rt, Duration::from_secs(3), |rt| {
            let rows = send_rows(rt);
            rows.len() == 2
                && rows
                    .iter()
                    .all(|row| row.ack.as_ref().map(|ack| ack.state) == Some(AckState::Acked))
        }),
        "both rows should be acked even though the broker acked newest first: {:?}",
        send_rows(&rt)
    );

    let pkids: Vec<u16> = send_rows(&rt)
        .iter()
        .filter_map(|row| row.ack.as_ref().and_then(|ack| ack.pkid))
        .collect();
    assert_eq!(pkids.len(), 2);
    assert_ne!(pkids[0], pkids[1], "each SEND row keeps its own packet id");
    rt.disconnect();
}

#[test]
fn duplicate_delivery_waits_for_manual_ack() {
    let port = 22944;
    let _broker = start_fault_broker(port, Fault::Duplicate);
    let mut rt = AppRuntime::new(Some(config_for(port, 1, true)), None, None);
    connect(&mut rt);

    assert!(rt.publish_form());
    assert!(
        wait_until(&mut rt, Duration::from_secs(3), |rt| {
            rt.visible_logs()
                .iter()
                .filter(|log| {
                    log.kind == "RECV"
                        && log.ack.as_ref().map(|ack| ack.state) == Some(AckState::PendingAck)
                })
                .count()
                == 2
        }),
        "the duplicate echo should leave two rows waiting for a manual ack: {:?}",
        rt.visible_logs()
    );
    assert_eq!(send_state(&rt), Some(AckState::Acked));

    let pending: Vec<u64> = rt
        .visible_logs()
        .iter()
        .filter(|log| log.ack.as_ref().map(|ack| ack.state) == Some(AckState::PendingAck))
        .map(|log| log.seq)
        .collect();
    for seq in pending {
        assert!(rt.manual_ack(seq), "manual ack #{seq} was rejected");
    }
    rt.poll();
    let sent = rt
        .visible_logs()
        .iter()
        .filter(|log| log.ack.as_ref().map(|ack| ack.state) == Some(AckState::AckSent))
        .count();
    assert_eq!(sent, 2, "both inbound rows should be manually acked");
    rt.disconnect();
}

#[test]
fn qos2_walks_pubrec_to_pubcomp() {
    let port = 22945;
    let _broker = start_fault_broker(port, Fault::Happy);
    let mut rt = AppRuntime::new(Some(config_for(port, 2, false)), None, None);
    connect(&mut rt);

    assert!(rt.publish_form());
    assert!(
        wait_until(&mut rt, Duration::from_secs(3), |rt| send_state(rt)
            == Some(AckState::Acked)),
        "QoS 2 should finish after PUBREC + PUBCOMP, got {:?}",
        send_state(&rt)
    );
    let ack = send_rows(&rt)[0].ack.clone().expect("ack info");
    assert!(ack.pkid.is_some());
    assert!(ack.elapsed_ms.is_some());
    rt.disconnect();
}
