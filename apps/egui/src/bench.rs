//! Automation hooks used by `tools/bench`.
//!
//! `--bench` makes the app deterministically load a fixed config, optionally
//! connect / start timed publish on the first frame, and quit after N seconds.

use std::time::{Duration, Instant};

use eframe::egui;
use mqttc_core::cli::BenchOptions;
use mqttc_core::runtime::AppRuntime;

pub struct Bench {
    options: BenchOptions,
    started: Instant,
    quit_at: Option<Instant>,
    first_tick: bool,
    finished: bool,
    timed_started: bool,
}

impl Bench {
    pub fn new(options: BenchOptions) -> Self {
        let started = Instant::now();
        let quit_at = options
            .quit_after
            .map(|secs| started + Duration::from_secs(secs));
        Self {
            options,
            started,
            quit_at,
            first_tick: true,
            finished: false,
            timed_started: false,
        }
    }

    /// Called once per frame before the UI is drawn.
    pub fn tick(&mut self, ctx: &egui::Context, runtime: &mut AppRuntime) {
        if self.first_tick {
            self.first_tick = false;
            if self.options.auto_connect() {
                runtime.connect();
            }
            println!(
                "BENCH scenario={} connect={} timed={}",
                self.options.scenario.as_deref().unwrap_or("s1"),
                self.options.auto_connect(),
                self.options.wants_timed()
            );
        }

        // Timed publish needs a live session, so wait for ConnAck instead of
        // racing the worker on the first frame.
        if self.options.wants_timed() && !self.timed_started {
            if let Some(id) = runtime.active_id() {
                if runtime.is_connected(&id) {
                    runtime.toggle_timed();
                    self.timed_started = true;
                }
            }
        }

        let now = Instant::now();
        if !self.finished && self.quit_at.map(|at| now >= at).unwrap_or(false) {
            self.finished = true;
            let logs = runtime.visible_logs();
            let recv = logs.iter().filter(|log| log.kind == "RECV").count();
            let send = logs.iter().filter(|log| log.kind == "SEND").count();
            println!(
                "BENCH_SUMMARY logs={} recv={} send={} status={:?}",
                logs.len(),
                recv,
                send,
                runtime.status()
            );
            println!("BENCH_QUIT {:.2}", self.started.elapsed().as_secs_f64());
            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
        } else {
            // Keep the frame clock alive so the quit deadline is observed even
            // when the app is otherwise idle (scenario S1).
            ctx.request_repaint_after(Duration::from_millis(100));
        }
    }
}
