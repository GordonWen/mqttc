use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;

const CONFIG_FILE: &str = "mqttc.json";
const LOG_DIR: &str = "logs";

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct AppConfig {
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
    #[serde(default = "default_pub_topic")]
    pub pub_topic: String,
    #[serde(default = "default_pub_payload")]
    pub pub_payload: String,
    #[serde(default)]
    pub subscriptions: Vec<String>,
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

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            host: default_host(),
            port: default_port(),
            client_id: String::new(),
            username: String::new(),
            password: String::new(),
            sub_topic: default_sub_topic(),
            pub_topic: default_pub_topic(),
            pub_payload: default_pub_payload(),
            subscriptions: Vec::new(),
        }
    }
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
    let config = serde_json::from_str(&text).map_err(|err| format!("解析配置失败: {err}"))?;
    Ok(Some(config))
}

pub fn save(config: &AppConfig) -> Result<(), String> {
    let path = config_path();
    let text = serde_json::to_string_pretty(config).map_err(|err| format!("序列化配置失败: {err}"))?;
    fs::write(&path, text).map_err(|err| format!("写入配置失败: {err}"))
}
