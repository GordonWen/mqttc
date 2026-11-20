//! Per-connection ring buffer for log rows.
//!
//! Raw bytes only (`Arc<[u8]>`); the UI renders text for the visible rows with
//! the current display format. Two bounds apply: a row count (user-configurable,
//! default 200) and a byte budget so a few huge payloads cannot exhaust RAM.

use std::collections::VecDeque;
use std::sync::Arc;

use crate::model::{
    clamp_log_limit, now_hms, now_timestamp, LogMeta, LogRecord, PayloadFormat, MAX_LOGS,
};
use crate::payload;

/// Per-connection memory budget for raw payload bytes. The newest row is always
/// kept even if it alone exceeds the budget.
pub const LOG_BYTE_BUDGET: usize = 16 * 1024 * 1024;

#[derive(Debug)]
pub struct LogStore {
    items: VecDeque<LogRecord>,
    next_seq: u64,
    limit: usize,
    bytes: usize,
    byte_budget: usize,
}

impl Default for LogStore {
    fn default() -> Self {
        Self {
            items: VecDeque::new(),
            next_seq: 0,
            limit: MAX_LOGS,
            bytes: 0,
            byte_budget: LOG_BYTE_BUDGET,
        }
    }
}

impl LogStore {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn push(&mut self, kind: &str, topic: &str, payload: Vec<u8>) {
        self.push_meta(kind, topic, payload, LogMeta::default());
    }

    /// Push with MQTT metadata (QoS / retain / dup / MQTT 5 properties).
    pub fn push_meta(&mut self, kind: &str, topic: &str, payload: Vec<u8>, meta: LogMeta) {
        let seq = self.next_seq;
        self.next_seq += 1;
        self.bytes += payload.len();
        self.items.push_back(LogRecord {
            seq,
            time: now_hms(),
            timestamp: now_timestamp(),
            kind: kind.to_string(),
            topic: topic.to_string(),
            payload: Arc::from(payload),
            qos: meta.qos,
            retain: meta.retain,
            dup: meta.dup,
            props: meta.props,
            expiry: meta.expiry,
        });
        self.trim();
    }

    /// Change the row cap and drop the oldest rows immediately.
    pub fn set_limit(&mut self, limit: usize) {
        self.limit = clamp_log_limit(limit);
        self.trim();
    }

    pub fn limit(&self) -> usize {
        self.limit
    }

    pub fn clear(&mut self) {
        self.items.clear();
        self.bytes = 0;
    }

    pub fn is_empty(&self) -> bool {
        self.items.is_empty()
    }

    pub fn len(&self) -> usize {
        self.items.len()
    }

    pub fn iter(&self) -> impl DoubleEndedIterator<Item = &LogRecord> {
        self.items.iter()
    }

    /// Lookup by the stable per-connection sequence number (detail view).
    pub fn find(&self, seq: u64) -> Option<&LogRecord> {
        self.items.iter().find(|log| log.seq == seq)
    }

    /// Cheap snapshot for export; payloads are shared `Arc`s.
    pub fn to_vec(&self) -> Vec<LogRecord> {
        self.items.iter().cloned().collect()
    }

    /// Display text for one row with the given format. Allocates; call only for rows on screen.
    pub fn render(&self, index: usize, format: PayloadFormat) -> Option<String> {
        let log = self.items.get(index)?;
        Some(if log.is_payload_kind() {
            payload::render_payload(&log.payload, format)
        } else {
            String::from_utf8_lossy(&log.payload).into_owned()
        })
    }

    fn trim(&mut self) {
        while self.items.len() > self.limit {
            self.pop_front();
        }
        while self.items.len() > 1 && self.bytes > self.byte_budget {
            self.pop_front();
        }
    }

    fn pop_front(&mut self) {
        if let Some(log) = self.items.pop_front() {
            self.bytes = self.bytes.saturating_sub(log.payload.len());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ring_keeps_last_200() {
        let mut store = LogStore::new();
        for i in 0..250 {
            store.push("RECV", "t", format!("{i}").into_bytes());
        }
        assert_eq!(store.len(), MAX_LOGS);
        let first = store.iter().next().unwrap();
        assert_eq!(first.payload.as_ref(), b"50");
        let last = store.iter().last().unwrap();
        assert_eq!(last.payload.as_ref(), b"249");
        assert_eq!(last.seq, 249);
    }

    #[test]
    fn limit_can_be_raised_and_lowered() {
        let mut store = LogStore::new();
        store.set_limit(500);
        assert_eq!(store.limit(), 500);
        for i in 0..600 {
            store.push("SYS", "", format!("{i}").into_bytes());
        }
        assert_eq!(store.len(), 500);
        assert_eq!(store.iter().next().unwrap().payload.as_ref(), b"100");

        store.set_limit(80);
        assert_eq!(store.len(), 80);
        assert_eq!(store.iter().last().unwrap().payload.as_ref(), b"599");

        // Values from a hand-edited JSON file are clamped, not trusted.
        store.set_limit(0);
        assert_eq!(store.limit(), crate::model::LOG_LIMIT_MIN);
    }

    #[test]
    fn byte_budget_drops_oldest_but_keeps_newest() {
        let mut store = LogStore::new();
        store.byte_budget = 12;
        store.push("RECV", "a", vec![b'a'; 8]);
        store.push("RECV", "b", vec![b'b'; 8]);
        assert_eq!(store.len(), 1);
        assert_eq!(store.iter().next().unwrap().payload.as_ref(), &[b'b'; 8]);

        // A single oversized row is still kept.
        store.push("RECV", "c", vec![b'c'; 32]);
        assert_eq!(store.len(), 1);
        assert_eq!(store.iter().next().unwrap().topic, "c");
    }

    #[test]
    fn render_respects_format() {
        let mut store = LogStore::new();
        store.push("RECV", "t", vec![0x00, 0xff]);
        assert_eq!(store.render(0, PayloadFormat::Hex).unwrap(), "00FF");
        store.push("SYS", "", "hi".as_bytes().to_vec());
        assert_eq!(store.render(1, PayloadFormat::Hex).unwrap(), "hi");
        assert!(store.render(2, PayloadFormat::Text).is_none());
    }

    #[test]
    fn clear_resets_len_but_not_seq() {
        let mut store = LogStore::new();
        store.push("SYS", "", b"a".to_vec());
        store.clear();
        assert!(store.is_empty());
        store.push("SYS", "", b"b".to_vec());
        assert_eq!(store.iter().next().unwrap().seq, 1);
    }
}
