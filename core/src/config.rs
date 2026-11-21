//! Persistent app state: `mqttc.json` next to the executable (portable, no home-dir lookup).
//!
//! `#[serde(default)]` / `#[serde(default = "fn")]` keep old files loadable when a field is added.
//! Old single-connection files (flat `host` / `port` …) are upgraded on load.

use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

pub const CONFIG_FILE: &str = "mqttc.json";
pub const LOG_DIR: &str = "logs";

static ID_SEQ: AtomicU64 = AtomicU64::new(0);

/// One saved subscription. A plain string in old files is migrated to `{topic, qos: 0}`.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct SubscriptionConfig {
    #[serde(default)]
    pub topic: String,
    #[serde(default)]
    pub qos: u8,
    /// MQTT 5 "No Local": do not receive our own publications. Ignored on 3.1.1.
    #[serde(default)]
    pub no_local: bool,
    /// MQTT 5 "Retain As Published": keep the publisher's retain flag. Ignored on 3.1.1.
    #[serde(default)]
    pub retain_as_published: bool,
}

/// One-click publish row. Missing `format` becomes `"text"` on load.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct PublishPreset {
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub topic: String,
    #[serde(default)]
    pub payload: String,
    #[serde(default)]
    pub qos: u8,
    #[serde(default)]
    pub retain: bool,
    #[serde(default = "default_payload_format")]
    pub format: String,
    /// Pinned presets sort ahead of the rest in the publish strip.
    #[serde(default)]
    pub pinned: bool,
}

impl Default for PublishPreset {
    fn default() -> Self {
        Self {
            name: String::new(),
            topic: String::new(),
            payload: String::new(),
            qos: 0,
            retain: false,
            format: default_payload_format(),
            pinned: false,
        }
    }
}

/// Everything the user can edit for one Broker, including presets and auto-reply.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ConnectionConfig {
    #[serde(default)]
    pub id: String,
    #[serde(default)]
    pub name: String,
    #[serde(default = "default_host")]
    pub host: String,
    #[serde(default = "default_port")]
    pub port: String,
    #[serde(default)]
    pub client_id: String,
    #[serde(default)]
    pub username: String,
    #[serde(default)]
    pub password: String,
    #[serde(default = "default_sub_topic")]
    pub sub_topic: String,
    #[serde(default)]
    pub sub_qos: u8,
    #[serde(default = "default_pub_topic")]
    pub pub_topic: String,
    #[serde(default = "default_pub_payload")]
    pub pub_payload: String,
    #[serde(default)]
    pub pub_qos: u8,
    #[serde(default)]
    pub pub_retain: bool,
    #[serde(default = "default_payload_format")]
    pub pub_format: String,
    /// MQTT 5 publish user properties, written as `key=value;key2=value2`.
    #[serde(default)]
    pub pub_props: String,
    /// MQTT 5 message expiry interval in seconds; 0 leaves it unset.
    #[serde(default)]
    pub pub_expiry: u32,
    #[serde(default = "default_keep_alive")]
    pub keep_alive: u16,
    #[serde(default = "default_clean_session")]
    pub clean_session: bool,
    /// Retry with exponential backoff after an unexpected disconnect.
    #[serde(default = "default_true")]
    pub auto_reconnect: bool,
    #[serde(default = "default_connect_timeout")]
    pub connect_timeout: u16,
    #[serde(default)]
    pub will_topic: String,
    #[serde(default)]
    pub will_payload: String,
    #[serde(default)]
    pub will_qos: u8,
    #[serde(default)]
    pub will_retain: bool,
    #[serde(default)]
    pub ssl: bool,
    #[serde(default = "default_true")]
    pub ssl_verify: bool,
    #[serde(default)]
    pub ssl_ca: String,
    #[serde(default)]
    pub ssl_cert: String,
    #[serde(default)]
    pub ssl_key: String,
    #[serde(default = "default_protocol")]
    pub protocol: String,
    #[serde(default = "default_timed_interval")]
    pub timed_interval: u32,
    #[serde(default)]
    pub subscriptions: Vec<SubscriptionConfig>,
    #[serde(default)]
    pub publish_presets: Vec<PublishPreset>,
    #[serde(default)]
    pub auto_reply_rules: Vec<AutoReplyRule>,
    #[serde(default = "default_auto_reply_echo")]
    pub auto_reply_echo: String,
}

/// One self-contained auto-reply rule. Unchecked rules are skipped during matching.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct AutoReplyRule {
    #[serde(default)]
    pub enabled: bool,
    #[serde(default)]
    pub match_topic: String,
    #[serde(default)]
    pub payload_contains: String,
    #[serde(default)]
    pub reply_topic: String,
    #[serde(default)]
    pub reply_payload: String,
    #[serde(default)]
    pub qos: u8,
    #[serde(default)]
    pub retain: bool,
    #[serde(default = "default_payload_format")]
    pub format: String,
    /// Minimum seconds between two replies from this rule; 0 replies to every match.
    #[serde(default)]
    pub min_interval_secs: u32,
}

impl Default for AutoReplyRule {
    fn default() -> Self {
        Self {
            enabled: false,
            match_topic: String::new(),
            payload_contains: String::new(),
            reply_topic: String::new(),
            reply_payload: String::new(),
            qos: 0,
            retain: false,
            format: default_payload_format(),
            min_interval_secs: 0,
        }
    }
}

/// Root of `mqttc.json`: the connection list plus window-wide prefs (skin, language, log filters).
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct AppConfig {
    #[serde(default)]
    pub active_id: String,
    #[serde(default)]
    pub connections: Vec<ConnectionConfig>,
    #[serde(default = "default_payload_format")]
    pub log_format: String,
    #[serde(default = "default_true")]
    pub auto_scroll: bool,
    #[serde(default)]
    pub show_advanced: bool,
    #[serde(default)]
    pub log_filter: String,
    #[serde(default)]
    pub log_kind_filter: i32,
    /// Rows kept per connection. Clamped to `[LOG_LIMIT_MIN, LOG_LIMIT_MAX]`.
    #[serde(default = "default_log_limit")]
    pub log_limit: usize,
    #[serde(default = "default_skin")]
    pub skin: String,
    /// Colors used when `skin` is `"custom"`; empty entries follow `base`.
    #[serde(default)]
    pub custom_palette: crate::theme::CustomPalette,
    #[serde(default = "default_language")]
    pub language: String,
    /// UI text scale; `1.0` is the standard size. Clamped to
    /// [`FONT_SCALE_MIN`]–[`FONT_SCALE_MAX`] when loaded or set.
    #[serde(default = "default_font_scale")]
    pub font_scale: f32,
    /// Keep the window above other applications (title-bar pin).
    #[serde(default)]
    pub always_on_top: bool,
}

/// Smallest UI text scale the settings page offers.
pub const FONT_SCALE_MIN: f32 = 0.8;
/// Largest UI text scale the settings page offers.
pub const FONT_SCALE_MAX: f32 = 1.4;

/// Keeps a hand-edited config from producing unreadable or overflowing type.
pub fn clamp_font_scale(scale: f32) -> f32 {
    if scale.is_finite() {
        scale.clamp(FONT_SCALE_MIN, FONT_SCALE_MAX)
    } else {
        1.0
    }
}

fn default_host() -> String {
    "127.0.0.1".to_string()
}

fn default_port() -> String {
    "1883".to_string()
}

fn default_sub_topic() -> String {
    "mqttc/mvp/#".to_string()
}

fn default_pub_topic() -> String {
    "mqttc/mvp/hello".to_string()
}

fn default_pub_payload() -> String {
    "hello from mqttc".to_string()
}

fn default_payload_format() -> String {
    "text".to_string()
}

fn default_true() -> bool {
    true
}

fn default_log_limit() -> usize {
    crate::model::MAX_LOGS
}

fn default_keep_alive() -> u16 {
    60
}

fn default_clean_session() -> bool {
    true
}

fn default_connect_timeout() -> u16 {
    10
}

fn default_timed_interval() -> u32 {
    5
}

fn default_skin() -> String {
    "black".to_string()
}

fn default_language() -> String {
    "zh".to_string()
}

fn default_font_scale() -> f32 {
    1.0
}

fn default_protocol() -> String {
    "3.1.1".to_string()
}

fn default_auto_reply_echo() -> String {
    "skip".to_string()
}

/// Unknown values become `"skip"` so a typo in the JSON cannot crash load.
pub fn normalize_auto_reply_echo(value: &str) -> &'static str {
    if value.trim().eq_ignore_ascii_case("rematch") {
        "rematch"
    } else {
        "skip"
    }
}

/// Fill empty format / clamp QoS. Length is left as-is.
pub fn ensure_auto_reply_rules(rules: &mut Vec<AutoReplyRule>) {
    for rule in rules.iter_mut() {
        if rule.format.is_empty() {
            rule.format = default_payload_format();
        }
        if rule.qos > 2 {
            rule.qos = 2;
        }
    }
}

/// Fill missing format and clamp QoS. Length is left as-is.
pub fn ensure_publish_presets(presets: &mut Vec<PublishPreset>) {
    for preset in presets.iter_mut() {
        if preset.format.is_empty() {
            preset.format = default_payload_format();
        }
        if preset.qos > 2 {
            preset.qos = 2;
        }
    }
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            active_id: String::new(),
            connections: Vec::new(),
            log_format: default_payload_format(),
            auto_scroll: true,
            show_advanced: false,
            log_filter: String::new(),
            log_kind_filter: 0,
            log_limit: default_log_limit(),
            skin: default_skin(),
            custom_palette: crate::theme::CustomPalette::default(),
            language: default_language(),
            font_scale: default_font_scale(),
            always_on_top: false,
        }
    }
}

impl ConnectionConfig {
    pub fn new(name: String) -> Self {
        Self {
            id: new_id(),
            name,
            host: default_host(),
            port: default_port(),
            client_id: String::new(),
            username: String::new(),
            password: String::new(),
            sub_topic: default_sub_topic(),
            sub_qos: 0,
            pub_topic: default_pub_topic(),
            pub_payload: default_pub_payload(),
            pub_qos: 0,
            pub_retain: false,
            pub_format: default_payload_format(),
            pub_props: String::new(),
            pub_expiry: 0,
            keep_alive: default_keep_alive(),
            clean_session: default_clean_session(),
            auto_reconnect: default_true(),
            connect_timeout: default_connect_timeout(),
            will_topic: String::new(),
            will_payload: String::new(),
            will_qos: 0,
            will_retain: false,
            ssl: false,
            ssl_verify: true,
            ssl_ca: String::new(),
            ssl_cert: String::new(),
            ssl_key: String::new(),
            protocol: default_protocol(),
            timed_interval: default_timed_interval(),
            subscriptions: Vec::new(),
            publish_presets: Vec::new(),
            auto_reply_rules: Vec::new(),
            auto_reply_echo: default_auto_reply_echo(),
        }
    }
}

/// Short hex id (`c` + millis + sequence). Unique enough for a local JSON file.
pub fn new_id() -> String {
    let millis = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis())
        .unwrap_or(0);
    let seq = ID_SEQ.fetch_add(1, Ordering::Relaxed);
    format!("c{millis:x}{seq:x}")
}

fn exe_dir() -> PathBuf {
    std::env::current_exe()
        .ok()
        .and_then(|exe| exe.parent().map(|p| p.to_path_buf()))
        .or_else(|| std::env::current_dir().ok())
        .unwrap_or_else(|| PathBuf::from("."))
}

/// Same directory as the binary so the app stays portable (USB / unzip-and-run).
pub fn config_path() -> PathBuf {
    exe_dir().join(CONFIG_FILE)
}

/// `logs/` next to the executable, same portable rule as [`config_path`].
pub fn log_dir() -> PathBuf {
    exe_dir().join(LOG_DIR)
}

/// `Ok(None)` = no file yet (first run). `Err` = file exists but could not be read / parsed.
pub fn load() -> Result<Option<AppConfig>, String> {
    load_from(&config_path())
}

pub fn load_from(path: &Path) -> Result<Option<AppConfig>, String> {
    if !path.exists() {
        return Ok(None);
    }
    let text = fs::read_to_string(path).map_err(|err| {
        crate::i18n::t(
            &format!("读取配置失败: {err}"),
            &format!("Failed to read config: {err}"),
        )
    })?;
    let value: serde_json::Value = serde_json::from_str(&text).map_err(|err| {
        crate::i18n::t(
            &format!("解析配置失败: {err}"),
            &format!("Failed to parse config: {err}"),
        )
    })?;
    let mut config: AppConfig = serde_json::from_value(value.clone()).map_err(|err| {
        crate::i18n::t(
            &format!("解析配置失败: {err}"),
            &format!("Failed to parse config: {err}"),
        )
    })?;
    config.log_limit = crate::model::clamp_log_limit(config.log_limit);
    config.font_scale = clamp_font_scale(config.font_scale);

    if config.connections.is_empty() {
        if let Some(conn) = legacy_connection(&value) {
            config.connections.push(conn);
        }
    }

    for conn in config.connections.iter_mut() {
        if conn.id.is_empty() {
            conn.id = new_id();
        }
        if conn.name.is_empty() {
            conn.name = format!("{}:{}", conn.host, conn.port);
        }
        ensure_publish_presets(&mut conn.publish_presets);
        ensure_auto_reply_rules(&mut conn.auto_reply_rules);
        conn.auto_reply_echo = normalize_auto_reply_echo(&conn.auto_reply_echo).to_string();
    }

    if !config
        .connections
        .iter()
        .any(|conn| conn.id == config.active_id)
    {
        config.active_id = config
            .connections
            .first()
            .map(|conn| conn.id.clone())
            .unwrap_or_default();
    }

    Ok(Some(config))
}

/// Pre-multi-connection `mqttc.json`: a single flat object with `host` at the root.
fn legacy_connection(value: &serde_json::Value) -> Option<ConnectionConfig> {
    let host = value.get("host")?.as_str()?.to_string();
    let mut conn = ConnectionConfig::new(host.clone());
    conn.host = host;
    if let Some(port) = value.get("port").and_then(|v| v.as_str()) {
        conn.port = port.to_string();
    }
    if let Some(v) = value.get("client_id").and_then(|v| v.as_str()) {
        conn.client_id = v.to_string();
    }
    if let Some(v) = value.get("username").and_then(|v| v.as_str()) {
        conn.username = v.to_string();
    }
    if let Some(v) = value.get("password").and_then(|v| v.as_str()) {
        conn.password = v.to_string();
    }
    if let Some(v) = value.get("sub_topic").and_then(|v| v.as_str()) {
        conn.sub_topic = v.to_string();
    }
    if let Some(v) = value.get("pub_topic").and_then(|v| v.as_str()) {
        conn.pub_topic = v.to_string();
    }
    if let Some(v) = value.get("pub_payload").and_then(|v| v.as_str()) {
        conn.pub_payload = v.to_string();
    }
    if let Some(list) = value.get("subscriptions").and_then(|v| v.as_array()) {
        for item in list {
            if let Some(topic) = item.as_str() {
                conn.subscriptions.push(SubscriptionConfig {
                    topic: topic.to_string(),
                    qos: 0,
                    ..SubscriptionConfig::default()
                });
            }
        }
    }
    conn.name = format!("{}:{}", conn.host, conn.port);
    ensure_publish_presets(&mut conn.publish_presets);
    Some(conn)
}

/// Pretty-printed JSON so users can edit `mqttc.json` by hand if they want.
pub fn save(config: &AppConfig) -> Result<(), String> {
    save_to(&config_path(), config)
}

/// Atomic save: write a sibling temp file, then rename over the real one.
pub fn save_to(path: &Path, config: &AppConfig) -> Result<(), String> {
    let text = serde_json::to_string_pretty(config).map_err(|err| {
        crate::i18n::t(
            &format!("序列化配置失败: {err}"),
            &format!("Failed to serialize config: {err}"),
        )
    })?;
    let tmp = path.with_extension("json.tmp");
    fs::write(&tmp, text.as_bytes()).map_err(|err| {
        crate::i18n::t(
            &format!("写入配置失败: {err}"),
            &format!("Failed to write config: {err}"),
        )
    })?;
    // Windows `rename` refuses to replace an existing file; fall back to a direct write.
    if fs::rename(&tmp, path).is_err() {
        let _ = fs::remove_file(&tmp);
        fs::write(path, text.as_bytes()).map_err(|err| {
            crate::i18n::t(
                &format!("写入配置失败: {err}"),
                &format!("Failed to write config: {err}"),
            )
        })?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn font_scale_defaults_and_clamps() {
        assert_eq!(AppConfig::default().font_scale, 1.0);
        let parsed: AppConfig = serde_json::from_str("{}").expect("defaults");
        assert_eq!(parsed.font_scale, 1.0);
        assert_eq!(clamp_font_scale(0.1), FONT_SCALE_MIN);
        assert_eq!(clamp_font_scale(9.0), FONT_SCALE_MAX);
        assert_eq!(clamp_font_scale(f32::NAN), 1.0);
    }

    #[test]
    fn legacy_flat_config_migrates_to_connection() {
        let value: serde_json::Value = serde_json::from_str(
            r#"{
                "host": "10.0.0.5",
                "port": "1884",
                "pub_topic": "a/b",
                "subscriptions": ["x/#", "y/+"]
            }"#,
        )
        .expect("valid json");

        let conn = legacy_connection(&value).expect("legacy connection");
        assert_eq!(conn.host, "10.0.0.5");
        assert_eq!(conn.port, "1884");
        assert_eq!(conn.pub_topic, "a/b");
        assert_eq!(conn.subscriptions.len(), 2);
        assert_eq!(conn.subscriptions[1].topic, "y/+");
        assert!(conn.publish_presets.is_empty());
    }

    #[test]
    fn app_config_round_trip_keeps_every_field() {
        let mut config = AppConfig::default();
        let mut conn = ConnectionConfig::new("测试".to_string());
        conn.pub_qos = 1;
        conn.pub_retain = true;
        conn.pub_format = "hex".to_string();
        conn.keep_alive = 45;
        conn.clean_session = false;
        conn.connect_timeout = 8;
        conn.will_topic = "status/lwt".to_string();
        conn.will_payload = "offline".to_string();
        conn.will_qos = 1;
        conn.will_retain = true;
        conn.ssl = true;
        conn.ssl_verify = false;
        conn.ssl_ca = "/tmp/ca.pem".to_string();
        conn.ssl_cert = "/tmp/client.pem".to_string();
        conn.ssl_key = "/tmp/client.key".to_string();
        conn.protocol = "5.0".to_string();
        conn.timed_interval = 12;
        conn.subscriptions.push(SubscriptionConfig {
            topic: "a/#".to_string(),
            qos: 2,
            no_local: true,
            retain_as_published: true,
        });
        conn.publish_presets.push(PublishPreset {
            name: "heartbeat".to_string(),
            topic: "dev/hb".to_string(),
            payload: "ping".to_string(),
            qos: 1,
            retain: true,
            format: "hex".to_string(),
            pinned: true,
        });
        conn.auto_reply_echo = "rematch".to_string();
        conn.auto_reply_rules.push(AutoReplyRule {
            enabled: true,
            match_topic: "dev/in".to_string(),
            payload_contains: "ping".to_string(),
            reply_topic: "dev/out".to_string(),
            reply_payload: "pong".to_string(),
            qos: 1,
            retain: false,
            format: "text".to_string(),
            min_interval_secs: 3,
        });
        config.active_id = conn.id.clone();
        config.connections.push(conn);
        config.log_format = "base64".to_string();
        config.auto_scroll = false;
        config.skin = "green".to_string();
        config.language = "en".to_string();
        config.always_on_top = true;

        let text = serde_json::to_string(&config).expect("serialize");
        let back: AppConfig = serde_json::from_str(&text).expect("deserialize");

        assert_eq!(back.active_id, config.active_id);
        assert_eq!(back.connections.len(), 1);
        assert_eq!(back.connections[0].pub_qos, 1);
        assert!(back.connections[0].pub_retain);
        assert_eq!(back.connections[0].pub_format, "hex");
        assert_eq!(back.connections[0].keep_alive, 45);
        assert!(!back.connections[0].clean_session);
        assert_eq!(back.connections[0].connect_timeout, 8);
        assert_eq!(back.connections[0].will_topic, "status/lwt");
        assert_eq!(back.connections[0].will_payload, "offline");
        assert_eq!(back.connections[0].will_qos, 1);
        assert!(back.connections[0].will_retain);
        assert!(back.connections[0].ssl);
        assert!(!back.connections[0].ssl_verify);
        assert_eq!(back.connections[0].ssl_ca, "/tmp/ca.pem");
        assert_eq!(back.connections[0].ssl_cert, "/tmp/client.pem");
        assert_eq!(back.connections[0].ssl_key, "/tmp/client.key");
        assert_eq!(back.connections[0].protocol, "5.0");
        assert_eq!(back.connections[0].timed_interval, 12);
        assert_eq!(back.connections[0].subscriptions[0].qos, 2);
        assert_eq!(back.connections[0].publish_presets.len(), 1);
        assert_eq!(back.connections[0].publish_presets[0].name, "heartbeat");
        assert_eq!(back.connections[0].publish_presets[0].topic, "dev/hb");
        assert_eq!(back.connections[0].publish_presets[0].payload, "ping");
        assert_eq!(back.connections[0].publish_presets[0].qos, 1);
        assert!(back.connections[0].publish_presets[0].retain);
        assert_eq!(back.connections[0].publish_presets[0].format, "hex");
        assert!(back.connections[0].publish_presets[0].pinned);
        assert_eq!(back.connections[0].auto_reply_echo, "rematch");
        assert_eq!(back.connections[0].auto_reply_rules.len(), 1);
        assert!(back.connections[0].auto_reply_rules[0].enabled);
        assert_eq!(back.connections[0].auto_reply_rules[0].match_topic, "dev/in");
        assert_eq!(back.connections[0].auto_reply_rules[0].payload_contains, "ping");
        assert_eq!(back.connections[0].auto_reply_rules[0].reply_topic, "dev/out");
        assert_eq!(back.connections[0].auto_reply_rules[0].reply_payload, "pong");
        assert_eq!(back.log_format, "base64");
        assert!(!back.auto_scroll);
        assert_eq!(back.skin, "green");
        assert_eq!(back.language, "en");
        assert!(back.always_on_top);
    }

    #[test]
    fn ensure_publish_presets_keeps_length_and_clamps_qos() {
        let mut empty = Vec::new();
        ensure_publish_presets(&mut empty);
        assert!(empty.is_empty());

        let mut presets = vec![
            PublishPreset {
                name: "a".into(),
                topic: "t1".into(),
                payload: "p1".into(),
                qos: 9,
                retain: false,
                format: String::new(),
                pinned: false,
            },
            PublishPreset {
                name: "b".into(),
                topic: "t2".into(),
                ..PublishPreset::default()
            },
        ];
        ensure_publish_presets(&mut presets);
        assert_eq!(presets.len(), 2);
        assert_eq!(presets[0].name, "a");
        assert_eq!(presets[0].qos, 2);
        assert_eq!(presets[0].format, "text");
        assert_eq!(presets[1].topic, "t2");
    }

    #[test]
    fn missing_fields_fall_back_to_defaults() {
        let value: serde_json::Value = serde_json::from_str(
            r#"{
                "active_id": "c1",
                "connections": [{
                    "id": "c1",
                    "name": "x",
                    "publish_presets": [{"name": "p", "qos": 7}],
                    "auto_reply_rules": [{"match_topic": "a/#"}]
                }],
                "log_kind_filter": 3
            }"#,
        )
        .expect("valid json");
        let mut config: AppConfig = serde_json::from_value(value).expect("deserialize");
        for conn in config.connections.iter_mut() {
            ensure_publish_presets(&mut conn.publish_presets);
            ensure_auto_reply_rules(&mut conn.auto_reply_rules);
        }
        let conn = &config.connections[0];
        assert_eq!(conn.host, "127.0.0.1");
        assert_eq!(conn.port, "1883");
        assert_eq!(conn.keep_alive, 60);
        assert!(conn.clean_session);
        assert_eq!(conn.protocol, "3.1.1");
        assert_eq!(conn.pub_format, "text");
        assert_eq!(conn.publish_presets[0].qos, 2);
        assert_eq!(conn.publish_presets[0].format, "text");
        assert_eq!(conn.auto_reply_rules[0].format, "text");
        assert_eq!(conn.auto_reply_echo, "skip");
        assert_eq!(config.skin, "black");
        assert_eq!(config.language, "zh");
        assert_eq!(config.log_kind_filter, 3);
    }

    #[test]
    fn save_and_load_round_trip_file() {
        let dir = std::env::temp_dir().join(format!("mqttc-cfg-{}", std::process::id()));
        let _ = fs::create_dir_all(&dir);
        let path = dir.join("mqttc.json");
        let mut config = AppConfig::default();
        let conn = ConnectionConfig::new("local".into());
        config.active_id = conn.id.clone();
        config.connections.push(conn);
        save_to(&path, &config).expect("save");
        let back = load_from(&path).expect("load").expect("present");
        assert_eq!(back.active_id, config.active_id);
        assert_eq!(back.connections.len(), 1);
        let _ = fs::remove_dir_all(&dir);
    }
}
