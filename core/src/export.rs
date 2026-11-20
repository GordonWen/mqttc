//! Save the log list to `logs/mqttc-YYYYMMDD-HHMMSS.log` next to the executable.

use chrono::Local;
use std::path::PathBuf;

use crate::config;
use crate::i18n;
use crate::model::{LogRecord, PayloadFormat};
use crate::payload;

fn line_for(log: &LogRecord, format: PayloadFormat) -> String {
    let body = if log.is_payload_kind() {
        payload::render_payload(&log.payload, format)
    } else {
        String::from_utf8_lossy(&log.payload).into_owned()
    };
    if log.topic.is_empty() {
        format!("[{}] [{}] {}\n", log.timestamp, log.kind, body)
    } else {
        format!(
            "[{}] [{}] {} {}\n",
            log.timestamp, log.kind, log.topic, body
        )
    }
}

pub fn save_logs_to_file(logs: &[LogRecord], format: PayloadFormat) -> Result<PathBuf, String> {
    save_logs_to_dir(logs, format, &config::log_dir())
}

pub fn save_logs_to_dir(
    logs: &[LogRecord],
    format: PayloadFormat,
    dir: &std::path::Path,
) -> Result<PathBuf, String> {
    std::fs::create_dir_all(dir).map_err(|err| {
        i18n::t(
            &format!("创建日志目录失败: {err}"),
            &format!("Failed to create log directory: {err}"),
        )
    })?;
    let path = dir.join(format!("mqttc-{}.log", Local::now().format("%Y%m%d-%H%M%S")));

    let mut content = String::new();
    for log in logs {
        content.push_str(&line_for(log, format));
    }

    std::fs::write(&path, content).map_err(|err| {
        i18n::t(
            &format!("写入日志文件失败: {err}"),
            &format!("Failed to write log file: {err}"),
        )
    })?;
    Ok(path)
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
}
