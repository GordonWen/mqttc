//! Local benchmark broker.
//!
//! ```text
//! mqttc-test-broker --port 1883 --prepare 200
//! mqttc-test-broker --port 1883 --stream 10
//! ```
//!
//! Starts a rumqttd server, optionally publishes 200 retained fixture messages,
//! optionally streams at N msg/s, prints `READY`, then runs until killed.

use std::io::Write;
use std::thread;
use std::time::Duration;

use mqttc_test_broker::{probe, publish_retained, spawn_stream, start_broker, wait_for_port};

struct Args {
    port: u16,
    v5_port: Option<u16>,
    prepare: usize,
    stream: u32,
    stream_topic: String,
    probe: bool,
    probe_only: bool,
    probe_topic: String,
    probe_wait: u64,
    fixture_only: bool,
    fixture_prefix: String,
}

fn parse_args() -> Args {
    let mut args = Args {
        port: 1883,
        v5_port: None,
        prepare: 0,
        stream: 0,
        stream_topic: "mqttc/mvp/stream".to_string(),
        probe: false,
        probe_only: false,
        probe_topic: "mqttc/mvp/#".to_string(),
        probe_wait: 3,
        fixture_only: false,
        fixture_prefix: "mqttc/mvp".to_string(),
    };
    let argv: Vec<String> = std::env::args().skip(1).collect();
    let mut i = 0;
    while i < argv.len() {
        let value = |i: usize| argv.get(i + 1).cloned();
        match argv[i].as_str() {
            "--port" => {
                if let Some(v) = value(i) {
                    if let Ok(p) = v.parse() {
                        args.port = p;
                    }
                    i += 1;
                }
            }
            "--v5-port" => {
                if let Some(v) = value(i) {
                    args.v5_port = v.parse().ok();
                    i += 1;
                }
            }
            "--prepare" => {
                if let Some(v) = value(i) {
                    if let Ok(n) = v.parse() {
                        args.prepare = n;
                    }
                    i += 1;
                }
            }
            "--stream" => {
                if let Some(v) = value(i) {
                    if let Ok(rate) = v.parse() {
                        args.stream = rate;
                    }
                    i += 1;
                }
            }
            "--stream-topic" => {
                if let Some(v) = value(i) {
                    args.stream_topic = v;
                    i += 1;
                }
            }
            "--probe" => args.probe = true,
            "--fixture-only" => args.fixture_only = true,
            "--fixture-prefix" => {
                if let Some(v) = value(i) {
                    args.fixture_prefix = v;
                    i += 1;
                }
            }
            "--probe-only" => {
                args.probe = true;
                args.probe_only = true;
            }
            "--probe-topic" => {
                if let Some(v) = value(i) {
                    args.probe_topic = v;
                    i += 1;
                }
            }
            "--probe-wait" => {
                if let Some(v) = value(i) {
                    if let Ok(secs) = v.parse() {
                        args.probe_wait = secs;
                    }
                    i += 1;
                }
            }
            other => eprintln!("unknown argument: {other}"),
        }
        i += 1;
    }
    args
}

fn main() {
    let args = parse_args();

    if args.probe_only {
        match probe(
            args.port,
            &args.probe_topic,
            Duration::from_secs(args.probe_wait),
        ) {
            Ok(count) => {
                println!("PROBE topic={} count={count}", args.probe_topic);
                let _ = std::io::stdout().flush();
            }
            Err(err) => {
                eprintln!("probe failed: {err}");
                std::process::exit(1);
            }
        }
        return;
    }

    if args.fixture_only {
        match publish_retained(args.port, &args.fixture_prefix, args.prepare) {
            Ok(()) => {
                println!(
                    "FIXTURE retained={} topic={}/#",
                    args.prepare, args.fixture_prefix
                );
                let _ = std::io::stdout().flush();
            }
            Err(err) => {
                eprintln!("fixture failed: {err}");
                std::process::exit(1);
            }
        }
        return;
    }

    let _broker = match args.v5_port {
        Some(v5) => {
            println!("BROKER v4=127.0.0.1:{} v5=127.0.0.1:{v5}", args.port);
            mqttc_test_broker::start_broker_dual(args.port, v5)
        }
        None => {
            println!("BROKER v4=127.0.0.1:{}", args.port);
            start_broker(args.port)
        }
    };

    if !wait_for_port(args.port, Duration::from_secs(10)) {
        eprintln!("broker did not start on port {}", args.port);
        std::process::exit(1);
    }

    if args.prepare > 0 {
        match publish_retained(args.port, &args.fixture_prefix, args.prepare) {
            Ok(()) => println!(
                "FIXTURE retained={} topic={}/#",
                args.prepare, args.fixture_prefix
            ),
            Err(err) => {
                eprintln!("fixture failed: {err}");
                std::process::exit(1);
            }
        }
    }

    let _stream = if args.stream > 0 {
        match spawn_stream(args.port, &args.stream_topic, args.stream) {
            Ok(handles) => {
                println!(
                    "STREAM rate={} topic={}",
                    args.stream, args.stream_topic
                );
                Some(handles)
            }
            Err(err) => {
                eprintln!("stream failed: {err}");
                None
            }
        }
    } else {
        None
    };

    if args.probe {
        match probe(
            args.port,
            &args.probe_topic,
            Duration::from_secs(args.probe_wait),
        ) {
            Ok(count) => println!("PROBE topic={} count={count}", args.probe_topic),
            Err(err) => eprintln!("probe failed: {err}"),
        }
    }

    println!("READY");
    let _ = std::io::stdout().flush();
    loop {
        thread::sleep(Duration::from_secs(3600));
    }
}
