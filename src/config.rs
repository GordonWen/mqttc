use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

const CONFIG_FILE: &str = "mqttc.json";
const LOG_DIR: &str = "logs";

static ID_SEQ: AtomicU64 = AtomicU64::new(0);

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct SubscriptionConfig {
    #[serde(default)]
    pub topic: String,
    #[serde(default)]
    pub qos: u8,
}

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
    #[serde(default = "default_keep_alive")]
    pub keep_alive: u16,
    #[serde(default = "default_clean_session")]
    pub clean_session: bool,
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
    #[serde(default = "default_timed_interval")]
    pub timed_interval: u32,
    #[serde(default)]
    pub subscriptions: Vec<SubscriptionConfig>,
}

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
            keep_alive: default_keep_alive(),
            clean_session: default_clean_session(),
            connect_timeout: default_connect_timeout(),
            will_topic: String::new(),
            will_payload: String::new(),
            will_qos: 0,
            will_retain: false,
            timed_interval: default_timed_interval(),
            subscriptions: Vec::new(),
        }
    }
}

pub fn new_id() -> String {
    let millis = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis())
        .unwrap_or(0);
    let seq = ID_SEQ.fetch_add(1, Ordering::Relaxed);
    format!("c{millis:x}{seq:x}")
}

pub fn config_path() -> PathBuf {
    let dir = std::env::current_exe()
        .ok()
        .and_then(|exe| exe.parent().map(|p| p.to_path_buf()))
        .or_else(|| std::env::current_dir().ok())
        .unwrap_or_else(|| PathBuf::from("."));
    dir.join(CONFIG_FILE)
}

pub fn log_dir() -> PathBuf {
    let dir = std::env::current_exe()
        .ok()
        .and_then(|exe| exe.parent().map(|p| p.to_path_buf()))
        .or_else(|| std::env::current_dir().ok())
        .unwrap_or_else(|| PathBuf::from("."));
    dir.join(LOG_DIR)
}

pub fn load() -> Result<Option<AppConfig>, String> {
    let path = config_path();
    if !path.exists() {
        return Ok(None);
    }
    let text = fs::read_to_string(&path).map_err(|err| format!("读取配置失败: {err}"))?;
    let value: serde_json::Value =
        serde_json::from_str(&text).map_err(|err| format!("解析配置失败: {err}"))?;
    let mut config: AppConfig =
        serde_json::from_value(value.clone()).map_err(|err| format!("解析配置失败: {err}"))?;

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

fn legacy_connection(value: &serde_json::Value) -> Option<ConnectionConfig> {
    let host = value.get("host")?.as_str()?.to_string();
    let mut conn = ConnectionConfig::new(format!("{host}"));
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
                });
            }
        }
    }
    conn.name = format!("{}:{}", conn.host, conn.port);
    Some(conn)
}

pub fn save(config: &AppConfig) -> Result<(), String> {
    let path = config_path();
    let text =
        serde_json::to_string_pretty(config).map_err(|err| format!("序列化配置失败: {err}"))?;
    fs::write(&path, text).map_err(|err| format!("写入配置失败: {err}"))
}

#[cfg(test)]
mod tests {
    use super::*;

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
    }

    #[test]
    fn app_config_round_trip_keeps_subscriptions_and_prefs() {
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
        conn.timed_interval = 12;
        conn.subscriptions.push(SubscriptionConfig {
            topic: "a/#".to_string(),
            qos: 2,
        });
        config.active_id = conn.id.clone();
        config.connections.push(conn);
        config.log_format = "base64".to_string();
        config.auto_scroll = false;

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
        assert_eq!(back.connections[0].timed_interval, 12);
        assert_eq!(back.connections[0].subscriptions[0].qos, 2);
        assert_eq!(back.log_format, "base64");
        assert!(!back.auto_scroll);
    }
}
