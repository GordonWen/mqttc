//! Small shared enums and value types used by config, runtime and every UI.

use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

use chrono::Local;

use crate::i18n;
use crate::platform;

/// Default ring-buffer size per connection (users can raise it in Settings).
/// Only raw bytes are kept; display text is rendered on demand.
pub const MAX_LOGS: usize = 200;

/// Bounds accepted from `mqttc.json` / the settings picker.
pub const LOG_LIMIT_MIN: usize = 50;
pub const LOG_LIMIT_MAX: usize = 2000;

pub fn clamp_log_limit(limit: usize) -> usize {
    limit.clamp(LOG_LIMIT_MIN, LOG_LIMIT_MAX)
}

/// How a payload (or the whole log list) is displayed.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PayloadFormat {
    Text,
    Json,
    Base64,
    Hex,
}

impl PayloadFormat {
    pub fn from_index(index: i32) -> Self {
        match index {
            1 => Self::Json,
            2 => Self::Base64,
            3 => Self::Hex,
            _ => Self::Text,
        }
    }

    pub fn from_key(key: &str) -> Self {
        match key {
            "json" => Self::Json,
            "base64" => Self::Base64,
            "hex" => Self::Hex,
            _ => Self::Text,
        }
    }

    pub fn to_index(self) -> i32 {
        match self {
            Self::Text => 0,
            Self::Json => 1,
            Self::Base64 => 2,
            Self::Hex => 3,
        }
    }

    pub fn to_key(self) -> &'static str {
        match self {
            Self::Text => "text",
            Self::Json => "json",
            Self::Base64 => "base64",
            Self::Hex => "hex",
        }
    }

    /// `(zh, en)` label for combo boxes.
    pub fn label(self) -> (&'static str, &'static str) {
        match self {
            Self::Text => ("文本", "Text"),
            Self::Json => ("JSON", "JSON"),
            Self::Base64 => ("Base64", "Base64"),
            Self::Hex => ("Hex", "Hex"),
        }
    }

    pub fn display(self) -> String {
        let (zh, en) = self.label();
        i18n::t(zh, en)
    }

    pub const ALL: [PayloadFormat; 4] = [Self::Text, Self::Json, Self::Base64, Self::Hex];
}

/// Stored preference. `System` follows the OS; the palette has no "auto" slot, so
/// the shell resolves it with [`Skin::resolve`] before painting.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Skin {
    System,
    Black,
    White,
    Blue,
    Green,
    Purple,
    /// User-defined palette stored in `mqttc.json`.
    Custom,
}

impl Skin {
    /// Settings combo: 0=System, 1=Black, … 6=Custom.
    pub fn from_index(index: i32) -> Self {
        match index {
            0 => Self::System,
            1 => Self::Black,
            2 => Self::White,
            3 => Self::Blue,
            4 => Self::Green,
            5 => Self::Purple,
            6 => Self::Custom,
            _ => Self::Black,
        }
    }

    /// Palette index (0=Black … 4=Purple). No System slot.
    pub fn from_visual_index(index: i32) -> Self {
        match index {
            1 => Self::White,
            2 => Self::Blue,
            3 => Self::Green,
            4 => Self::Purple,
            _ => Self::Black,
        }
    }

    pub fn from_key(key: &str) -> Self {
        match key {
            "system" | "auto" | "follow" => Self::System,
            "white" => Self::White,
            "blue" => Self::Blue,
            "green" => Self::Green,
            "purple" => Self::Purple,
            "custom" => Self::Custom,
            _ => Self::Black,
        }
    }

    pub fn to_index(self) -> i32 {
        match self {
            Self::System => 0,
            Self::Black => 1,
            Self::White => 2,
            Self::Blue => 3,
            Self::Green => 4,
            Self::Purple => 5,
            Self::Custom => 6,
        }
    }

    pub fn to_visual_index(self) -> i32 {
        match self.resolve() {
            Self::White => 1,
            Self::Blue => 2,
            Self::Green => 3,
            Self::Purple => 4,
            _ => 0,
        }
    }

    pub fn to_key(self) -> &'static str {
        match self {
            Self::System => "system",
            Self::Black => "black",
            Self::White => "white",
            Self::Blue => "blue",
            Self::Green => "green",
            Self::Purple => "purple",
            Self::Custom => "custom",
        }
    }

    /// Only Black / White follow the OS. Colored skins stay as picked.
    pub fn resolve(self) -> Self {
        match self {
            Self::System => {
                if platform::system_prefers_dark() {
                    Self::Black
                } else {
                    Self::White
                }
            }
            other => other,
        }
    }

    pub fn is_light(self) -> bool {
        matches!(self.resolve(), Self::White)
    }

    /// `(zh, en)` label for the settings combo.
    pub fn label(self) -> (&'static str, &'static str) {
        match self {
            Self::System => ("跟随系统", "System"),
            Self::Black => ("黑色", "Black"),
            Self::White => ("白色", "White"),
            Self::Blue => ("蓝色", "Blue"),
            Self::Green => ("绿色", "Green"),
            Self::Purple => ("紫色", "Purple"),
            Self::Custom => ("自定义", "Custom"),
        }
    }

    pub fn display(self) -> String {
        let (zh, en) = self.label();
        i18n::t(zh, en)
    }

    pub const ALL: [Skin; 7] = [
        Self::System,
        Self::Black,
        Self::White,
        Self::Blue,
        Self::Green,
        Self::Purple,
        Self::Custom,
    ];
}

/// In-memory only. Never written to `mqttc.json`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ConnStatus {
    Disconnected,
    Connecting,
    Connected,
}

impl ConnStatus {
    pub fn text(self) -> String {
        match self {
            Self::Disconnected => i18n::t("未连接", "Disconnected"),
            Self::Connecting => i18n::t("连接中", "Connecting"),
            Self::Connected => i18n::t("已连接", "Connected"),
        }
    }

    pub fn rgb(self) -> (u8, u8, u8) {
        match self {
            Self::Disconnected => (0x9a, 0xa0, 0xa6),
            Self::Connecting => (0xe0, 0xb4, 0x4f),
            Self::Connected => (0x4e, 0xcf, 0x7a),
        }
    }
}

/// MQTT metadata attached to one RECV/SEND row. Empty/default for SYS/ERR.
#[derive(Clone, Debug, Default)]
pub struct LogMeta {
    pub qos: Option<u8>,
    pub retain: bool,
    pub dup: bool,
    /// MQTT 5 user properties (and a few well-known ones) in display order.
    pub props: Vec<(String, String)>,
    /// MQTT 5 message expiry interval, seconds.
    pub expiry: Option<u32>,
}

/// One log row. Payload stays as raw bytes so switching display format is lossless.
#[derive(Clone, Debug)]
pub struct LogRecord {
    /// Monotonic per-connection sequence, useful as a stable list key.
    pub seq: u64,
    /// `HH:MM:SS.mmm` — milliseconds keep bursts ordered.
    pub time: String,
    /// `YYYY-MM-DD HH:MM:SS.mmm` for exports.
    pub timestamp: String,
    /// `RECV` / `SEND` / `SYS` / `ERR`. Payload encoding applies only to RECV/SEND.
    pub kind: String,
    pub topic: String,
    pub payload: Arc<[u8]>,
    pub qos: Option<u8>,
    pub retain: bool,
    pub dup: bool,
    pub props: Vec<(String, String)>,
    pub expiry: Option<u32>,
}

impl LogRecord {
    pub fn is_payload_kind(&self) -> bool {
        self.kind == "RECV" || self.kind == "SEND"
    }

    /// Compact flag text for bubble headers / exports, e.g. `QoS1 · R · DUP`.
    pub fn flags(&self) -> String {
        let mut parts = Vec::new();
        if let Some(qos) = self.qos {
            parts.push(format!("QoS{qos}"));
        }
        if self.retain {
            parts.push(i18n::t("保留", "retain").to_string());
        }
        if self.dup {
            parts.push("DUP".to_string());
        }
        if let Some(expiry) = self.expiry {
            parts.push(i18n::t(
                &format!("过期 {expiry}s"),
                &format!("expiry {expiry}s"),
            ));
        }
        parts.join(" · ")
    }

    /// `k=v, k2=v2` for MQTT 5 user properties; empty when none.
    pub fn props_text(&self) -> String {
        self.props
            .iter()
            .map(|(key, value)| format!("{key}={value}"))
            .collect::<Vec<_>>()
            .join(", ")
    }
}

pub fn now_hms() -> String {
    Local::now().format("%H:%M:%S%.3f").to_string()
}

pub fn now_timestamp() -> String {
    Local::now().format("%Y-%m-%d %H:%M:%S%.3f").to_string()
}

/// Unique-enough Client ID when the form is left blank (`mqttc-` + unix millis in hex).
pub fn default_client_id() -> String {
    let millis = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis())
        .unwrap_or(0);
    format!("mqttc-{millis:x}")
}

/// Keep-alive / timeout text fields: parse, clamp, fall back when invalid.
pub fn parse_u16_clamped(text: &str, min: u16, max: u16, fallback: u16) -> u16 {
    text.trim()
        .parse::<u16>()
        .ok()
        .filter(|v| *v >= min && *v <= max)
        .unwrap_or(fallback)
}

/// Timed publish interval: 1-3600 seconds. Error text is localized.
pub fn parse_timed_interval(text: &str) -> Result<u32, String> {
    let value = text.trim().parse::<u32>().map_err(|_| {
        i18n::t(
            "定时间隔无效，范围为 1-3600 秒",
            "Invalid interval, range is 1-3600 seconds",
        )
    })?;
    if (1..=3600).contains(&value) {
        Ok(value)
    } else {
        Err(i18n::t(
            "定时间隔无效，范围为 1-3600 秒",
            "Invalid interval, range is 1-3600 seconds",
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn skin_index_and_key_round_trip() {
        for skin in Skin::ALL {
            assert_eq!(Skin::from_index(skin.to_index()), skin);
            assert_eq!(Skin::from_key(skin.to_key()), skin);
        }
        assert_eq!(Skin::from_key("auto"), Skin::System);
        assert_eq!(Skin::from_key("nope"), Skin::Black);
        assert_eq!(Skin::from_index(99), Skin::Black);
    }

    #[test]
    fn payload_format_round_trip() {
        for format in PayloadFormat::ALL {
            assert_eq!(PayloadFormat::from_index(format.to_index()), format);
            assert_eq!(PayloadFormat::from_key(format.to_key()), format);
        }
        assert_eq!(PayloadFormat::from_index(42), PayloadFormat::Text);
    }

    #[test]
    fn parse_helpers_clamp() {
        assert_eq!(parse_u16_clamped("60", 1, 65535, 10), 60);
        assert_eq!(parse_u16_clamped("0", 1, 65535, 10), 10);
        assert_eq!(parse_u16_clamped("x", 1, 300, 10), 10);
        assert_eq!(parse_timed_interval("5"), Ok(5));
        assert!(parse_timed_interval("0").is_err());
        assert!(parse_timed_interval("3601").is_err());
        assert!(parse_timed_interval("abc").is_err());
    }
}
