//! Topic wildcards and log-list filtering. Pure functions; never mutate stored logs.

use crate::model::{LogRecord, PayloadFormat};
use crate::payload;

/// MQTT-style `+` (one level) / `#` (rest of the tree). Also used by auto-reply match_topic.
pub fn topic_matches_filter(topic: &str, filter: &str) -> bool {
    if filter.is_empty() || topic == filter {
        return true;
    }
    let topic_parts: Vec<&str> = topic.split('/').collect();
    let filter_parts: Vec<&str> = filter.split('/').collect();
    let mut ti = 0;
    for (fi, part) in filter_parts.iter().enumerate() {
        if *part == "#" {
            return fi == filter_parts.len() - 1;
        }
        if ti >= topic_parts.len() {
            return false;
        }
        if *part != "+" && *part != topic_parts[ti] {
            return false;
        }
        ti += 1;
    }
    ti == topic_parts.len()
}

pub fn kind_matches(kind: &str, filter: i32) -> bool {
    match filter {
        1 => kind == "RECV",
        2 => kind == "SEND",
        3 => kind == "SYS",
        4 => kind == "ERR",
        _ => true,
    }
}

/// Keyword / kind / subscription-row filters. `filter` must already be lowercase.
pub fn log_matches(
    log: &LogRecord,
    format: PayloadFormat,
    filter: &str,
    kind_filter: i32,
    active_topic: &str,
) -> bool {
    if !kind_matches(&log.kind, kind_filter) {
        return false;
    }
    if !active_topic.is_empty() && log.is_payload_kind() && !topic_matches_filter(&log.topic, active_topic)
    {
        return false;
    }
    if filter.is_empty() {
        return true;
    }
    let payload_text = if log.is_payload_kind() {
        payload::render_payload(&log.payload, format)
    } else {
        String::from_utf8_lossy(&log.payload).into_owned()
    };
    let payload_lower = payload_text.to_lowercase();
    log.topic.to_lowercase().contains(filter)
        || payload_lower.contains(filter)
        || log.kind.to_lowercase().contains(filter)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::now_hms;
    use std::sync::Arc;

    fn rec(kind: &str, topic: &str, payload: &[u8]) -> LogRecord {
        LogRecord {
            seq: 0,
            time: now_hms(),
            timestamp: String::new(),
            kind: kind.to_string(),
            topic: topic.to_string(),
            payload: Arc::from(payload),
        }
    }

    #[test]
    fn wildcards_match() {
        assert!(topic_matches_filter("a/b/c", "a/#"));
        assert!(topic_matches_filter("a/b/c", "a/+/c"));
        assert!(topic_matches_filter("a/b", "a/+"));
        assert!(!topic_matches_filter("a/b/c", "a/+"));
        assert!(!topic_matches_filter("a/b/c", "b/#"));
        assert!(topic_matches_filter("anything", ""));
        assert!(topic_matches_filter("a/b", "a/b"));
        assert!(!topic_matches_filter("a/b", "a/#/c"));
    }

    #[test]
    fn kind_filter_works() {
        assert!(kind_matches("ERR", 4));
        assert!(!kind_matches("ERR", 1));
        assert!(kind_matches("SYS", 0));
    }

    #[test]
    fn keyword_filter_checks_topic_payload_kind() {
        let log = rec("SEND", "dev/hb", b"ping");
        assert!(log_matches(&log, PayloadFormat::Text, "dev", 0, ""));
        assert!(log_matches(&log, PayloadFormat::Text, "ping", 0, ""));
        assert!(log_matches(&log, PayloadFormat::Text, "send", 0, ""));
        assert!(!log_matches(&log, PayloadFormat::Text, "pong", 0, ""));
        assert!(!log_matches(&log, PayloadFormat::Text, "dev", 4, ""));
        assert!(log_matches(&log, PayloadFormat::Text, "", 2, ""));
        assert!(!log_matches(&log, PayloadFormat::Text, "", 2, "other/#"));
        assert!(log_matches(&log, PayloadFormat::Text, "", 2, "dev/#"));
    }
}
