//! Tiny argument parser shared by the three shells.
//!
//! `--bench` is used by `tools/bench`: load a fixed config, optionally connect,
//! and quit after N seconds so the sampler gets a clean process.
//!
//! ```text
//! mqttc-egui --bench --bench-scenario s2 --bench-quit-after 75 --config fixture.json
//! ```

use std::path::PathBuf;

#[derive(Clone, Debug, Default)]
pub struct BenchOptions {
    pub bench: bool,
    pub scenario: Option<String>,
    pub quit_after: Option<u64>,
    pub config: Option<PathBuf>,
}

impl BenchOptions {
    /// True when this scenario should connect the first saved connection on startup.
    pub fn auto_connect(&self) -> bool {
        matches!(
            self.scenario.as_deref(),
            Some("s2") | Some("s3") | Some("s4")
        )
    }

    /// S3: the broker streams; the app only waits. S4: timed publish uses the config interval.
    pub fn wants_stream(&self) -> bool {
        matches!(self.scenario.as_deref(), Some("s3"))
    }

    pub fn wants_timed(&self) -> bool {
        matches!(self.scenario.as_deref(), Some("s4"))
    }
}

fn value_of(args: &[String], index: usize) -> Option<&String> {
    args.get(index + 1).filter(|v| !v.starts_with("--"))
}

pub fn parse<I, S>(args: I) -> BenchOptions
where
    I: IntoIterator<Item = S>,
    S: Into<String>,
{
    let args: Vec<String> = args.into_iter().map(Into::into).collect();
    let mut out = BenchOptions::default();
    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "--bench" => out.bench = true,
            "--bench-scenario" => {
                if let Some(value) = value_of(&args, i) {
                    out.scenario = Some(value.to_lowercase());
                    i += 1;
                }
            }
            "--bench-quit-after" => {
                if let Some(value) = value_of(&args, i) {
                    out.quit_after = value.parse::<u64>().ok();
                    i += 1;
                }
            }
            "--config" => {
                if let Some(value) = value_of(&args, i) {
                    out.config = Some(PathBuf::from(value));
                    i += 1;
                }
            }
            _ => {}
        }
        i += 1;
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_full_command_line() {
        let opts = parse([
            "app.exe",
            "--bench",
            "--bench-scenario",
            "S2",
            "--bench-quit-after",
            "70",
            "--config",
            "fixtures/mqttc.json",
        ]);
        assert!(opts.bench);
        assert_eq!(opts.scenario.as_deref(), Some("s2"));
        assert_eq!(opts.quit_after, Some(70));
        assert_eq!(opts.config.as_deref(), Some(std::path::Path::new("fixtures/mqttc.json")));
        assert!(opts.auto_connect());
        assert!(!opts.wants_timed());
    }

    #[test]
    fn defaults_are_inert() {
        let opts = parse(["app.exe"]);
        assert!(!opts.bench);
        assert!(!opts.auto_connect());
        assert_eq!(opts.quit_after, None);
        assert!(parse(["app", "--bench-scenario"]).scenario.is_none());
        assert!(parse(["app", "--bench-quit-after", "--bench"]).quit_after.is_none());
    }

    #[test]
    fn scenario_helpers() {
        assert!(parse(["x", "--bench-scenario", "s3"]).wants_stream());
        assert!(parse(["x", "--bench-scenario", "s4"]).wants_timed());
        assert!(!parse(["x", "--bench-scenario", "s1"]).auto_connect());
    }
}
