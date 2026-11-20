//! Per-connection ring buffer for log rows.
//!
//! Raw bytes only (`Arc<[u8]>`); the UI renders text for the visible rows with
//! the current display format. The cap is the reference implementation's 200.

use std::collections::VecDeque;
use std::sync::Arc;

use crate::model::{now_hms, now_timestamp, LogRecord, PayloadFormat, MAX_LOGS};
use crate::payload;

#[derive(Debug, Default)]
pub struct LogStore {
    items: VecDeque<LogRecord>,
    next_seq: u64,
}

impl LogStore {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn push(&mut self, kind: &str, topic: &str, payload: Vec<u8>) {
        let seq = self.next_seq;
        self.next_seq += 1;
        self.items.push_back(LogRecord {
            seq,
            time: now_hms(),
            timestamp: now_timestamp(),
            kind: kind.to_string(),
            topic: topic.to_string(),
            payload: Arc::from(payload),
        });
        while self.items.len() > MAX_LOGS {
            self.items.pop_front();
        }
    }

    pub fn clear(&mut self) {
        self.items.clear();
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
