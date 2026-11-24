//! Save the log list to `logs/mqttc-YYYYMMDD-HHMMSS.log` next to the executable.

use chrono::Local;
use std::path::PathBuf;

use crate::config;
use crate::i18n;
use crate::model::{LogRecord, PayloadFormat};
use crate::payload;

/// One display line for a record, without a trailing newline. Shared by the
/// file exporter and the UI's "copy row" action so both read identically.
pub fn log_line(log: &LogRecord, format: PayloadFormat) -> String {
    let body = if log.is_payload_kind() {
        payload::render_payload(&log.payload, format)
    } else {
        String::from_utf8_lossy(&log.payload).into_owned()
    };
    let flags = log.flags();
    let props = log.props_text();
    let meta = match (flags.is_empty(), props.is_empty()) {
        (true, true) => String::new(),
        (false, true) => format!("({flags})"),
        (true, false) => format!("<{props}>"),
        (false, false) => format!("({flags}) <{props}>"),
    };
    let head = if log.topic.is_empty() {
        format!("[{}] [{}]", log.timestamp, log.kind)
    } else {
        format!("[{}] [{}] {}", log.timestamp, log.kind, log.topic)
    };
    if meta.is_empty() {
        format!("{head} {body}")
    } else {
        format!("{head} {meta} {body}")
    }
}

pub fn save_logs_to_file(logs: &[LogRecord], format: PayloadFormat) -> Result<PathBuf, String> {
    save_logs_to_file_named(logs, format, "")
}

/// Same as [`save_logs_to_file`] but includes the connection name in the file name.
pub fn save_logs_to_file_named(
    logs: &[LogRecord],
    format: PayloadFormat,
    label: &str,
) -> Result<PathBuf, String> {
    save_logs_to_dir_named(logs, format, &config::log_dir(), label)
}

pub fn save_logs_to_dir(
    logs: &[LogRecord],
    format: PayloadFormat,
    dir: &std::path::Path,
) -> Result<PathBuf, String> {
    save_logs_to_dir_named(logs, format, dir, "")
}

fn save_logs_to_dir_named(
    logs: &[LogRecord],
    format: PayloadFormat,
    dir: &std::path::Path,
    label: &str,
) -> Result<PathBuf, String> {
    std::fs::create_dir_all(dir).map_err(|err| {
        i18n::t(
            &format!("创建日志目录失败: {err}"),
            &format!("Failed to create log directory: {err}"),
        )
    })?;
    // Milliseconds keep back-to-back saves from colliding; the label makes
    // multi-connection captures identifiable at a glance.
    let stamp = Local::now().format("%Y%m%d-%H%M%S-%3f");
    let slug = sanitize_label(label);
    let stem = if slug.is_empty() {
        format!("mqttc-{stamp}")
    } else {
        format!("mqttc-{slug}-{stamp}")
    };
    let path = unique_path(dir, &stem);

    let mut content = String::new();
    for log in logs {
        content.push_str(&log_line(log, format));
        content.push('\n');
    }

    std::fs::write(&path, content).map_err(|err| {
        i18n::t(
            &format!("写入日志文件失败: {err}"),
            &format!("Failed to write log file: {err}"),
        )
    })?;
    Ok(path)
}

/// Keep the label readable while removing characters Windows forbids in names.
fn sanitize_label(label: &str) -> String {
    let mut out = String::new();
    let mut count = 0;
    for ch in label.chars() {
        if count >= 40 {
            break;
        }
        count += 1;
        if ch.is_control()
            || ch.is_whitespace()
            || matches!(ch, '<' | '>' | ':' | '"' | '/' | '\\' | '|' | '?' | '*')
        {
            out.push('_');
        } else {
            out.push(ch);
        }
    }
    out.trim_matches(|c| c == '.' || c == '_' || c == ' ')
        .to_string()
}

fn unique_path(dir: &std::path::Path, stem: &str) -> PathBuf {
    let first = dir.join(format!("{stem}.log"));
    if !first.exists() {
        return first;
    }
    for n in 1..100 {
        let path = dir.join(format!("{stem}-{n}.log"));
        if !path.exists() {
            return path;
        }
    }
    dir.join(format!("{stem}-{}.log", std::process::id()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;

    fn sample() -> LogRecord {
        LogRecord {
            seq: 0,
            time: "12:00:00".into(),
            timestamp: "2026-01-01 12:00:00".into(),
            kind: "SEND".into(),
            topic: "a/b".into(),
            payload: Arc::from(&b"hi"[..]),
            qos: None,
            retain: false,
            dup: false,
            props: Vec::new(),
            expiry: None,
            ack: None,
        }
    }

    #[test]
    fn writes_one_line_per_record() {
        let dir = std::env::temp_dir().join(format!("mqttc-logs-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let path = save_logs_to_dir(&[sample()], PayloadFormat::Text, &dir).expect("save");
        let text = std::fs::read_to_string(&path).expect("read");
        assert_eq!(text, "[2026-01-01 12:00:00] [SEND] a/b hi\n");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn named_file_has_label_millis_and_safe_chars() {
        let dir = std::env::temp_dir().join(format!("mqttc-logs3-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let path =
            save_logs_to_dir_named(&[sample()], PayloadFormat::Text, &dir, "Local demo")
                .expect("save");
        let name = path.file_name().unwrap().to_string_lossy().into_owned();
        assert!(name.starts_with("mqttc-Local_demo-"), "{name}");
        assert!(name.ends_with(".log"), "{name}");
        // 8 date + 1 separator + 6 time + 1 separator + 3 millis = 19 digits.
        let stamp = name
            .trim_start_matches("mqttc-Local_demo-")
            .trim_end_matches(".log");
        assert_eq!(stamp.len(), 19, "{stamp}");

        let unsafe_path = save_logs_to_dir_named(
            &[sample()],
            PayloadFormat::Text,
            &dir,
            "a/b:c *?\"<>|",
        )
        .expect("save");
        let unsafe_name = unsafe_path.file_name().unwrap().to_string_lossy().into_owned();
        assert!(!unsafe_name
            .chars()
            .any(|c| matches!(c, '/' | ':' | '*' | '?' | '"' | '<' | '>' | '|')));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn same_millisecond_falls_back_to_suffix() {
        let dir = std::env::temp_dir().join(format!("mqttc-logs4-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let base = "mqttc-x-20260101-120000-123";
        std::fs::write(dir.join(format!("{base}.log")), b"").unwrap();
        let path = unique_path(&dir, base);
        assert!(path.to_string_lossy().ends_with("-1.log"));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn sys_lines_have_no_topic() {
        let mut log = sample();
        log.kind = "SYS".into();
        log.topic.clear();
        let dir = std::env::temp_dir().join(format!("mqttc-logs2-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let path = save_logs_to_dir(&[log], PayloadFormat::Hex, &dir).expect("save");
        let text = std::fs::read_to_string(&path).expect("read");
        assert_eq!(text, "[2026-01-01 12:00:00] [SYS] hi\n");
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// Copying a row uses the same formatter as the saved log, including MQTT 5 props.
    #[test]
    fn row_line_has_flags_and_properties() {
        let mut log = sample();
        log.qos = Some(1);
        log.retain = true;
        log.expiry = Some(30);
        log.props = vec![("k".into(), "v".into()), ("k2".into(), "v2".into())];
        let line = log_line(&log, PayloadFormat::Text);
        assert!(line.starts_with("[2026-01-01 12:00:00] [SEND] a/b (QoS1"), "{line}");
        assert!(line.contains("<k=v, k2=v2>"), "{line}");
        assert!(line.ends_with(" hi"), "{line}");
        assert!(!line.contains('\n'));
    }
}
