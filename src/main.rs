#![cfg_attr(all(windows, not(debug_assertions)), windows_subsystem = "windows")]

mod config;
mod mqtt;

use chrono::Local;
use config::AppConfig;
use mqtt::{MqttHandle, UiEvent};
use slint::{ComponentHandle, Model, ModelRc, VecModel};
use std::rc::Rc;
use std::time::{SystemTime, UNIX_EPOCH};

slint::include_modules!();

fn main() -> Result<(), slint::PlatformError> {
    let ui = AppWindow::new()?;
    let mqtt = MqttHandle::new();
    let ui_weak = ui.as_weak();
    apply_startup_config(&ui);

    ui.on_connect_requested({
        let ui_weak = ui_weak.clone();
        let mqtt = mqtt.clone();
        move || {
            let ui = ui_weak.unwrap();
            let host = ui.get_host().trim().to_string();
            if host.is_empty() {
                append_log(&ui, "ERR", "", "主机不能为空");
                return;
            }
            let port = match ui.get_port().trim().parse::<u16>() {
                Ok(port) if port > 0 => port,
                _ => {
                    append_log(&ui, "ERR", "", "端口无效，范围为 1-65535");
                    return;
                }
            };
            let mut client_id = ui.get_client_id().trim().to_string();
            if client_id.is_empty() {
                client_id = default_client_id();
                ui.set_client_id(client_id.clone().into());
            }
            let username = ui.get_username().trim().to_string();
            let password = ui.get_password().to_string();
            let auth_hint = if username.is_empty() {
                "匿名".to_string()
            } else {
                format!("用户 {username}")
            };

            ui.set_connecting(true);
            ui.set_connected(false);
            ui.set_status("连接中".into());
            ui.set_status_color(slint::Color::from_rgb_u8(0xe0, 0xb4, 0x4f));
            append_log(
                &ui,
                "SYS",
                "",
                &format!("正在连接 {host}:{port} ({client_id}, {auth_hint})"),
            );

            let ui_weak = ui_weak.clone();
            persist_config(&ui);
            let mqtt_events = mqtt.clone();
            mqtt.connect(host, port, client_id, username, password, move |event| {
                let ui_weak = ui_weak.clone();
                let mqtt_events = mqtt_events.clone();
                let _ = slint::invoke_from_event_loop(move || {
                    if let Some(ui) = ui_weak.upgrade() {
                        apply_event(&ui, event, &mqtt_events);
                    }
                });
            });
        }
    });

    ui.on_disconnect_requested({
        let ui_weak = ui_weak.clone();
        let mqtt = mqtt.clone();
        move || {
            mqtt.disconnect();
            let ui = ui_weak.unwrap();
            set_disconnected(&ui, "未连接");
            append_log(&ui, "SYS", "", "已断开连接");
        }
    });

    ui.on_subscribe_requested({
        let ui_weak = ui_weak.clone();
        let mqtt = mqtt.clone();
        move || {
            let ui = ui_weak.unwrap();
            if !ui.get_connected() {
                append_log(&ui, "ERR", "", "请先连接 Broker");
                return;
            }
            let topic = ui.get_sub_topic().trim().to_string();
            if topic.is_empty() {
                append_log(&ui, "ERR", "", "Topic 为空");
                return;
            }
            match mqtt.subscribe(&topic) {
                Ok(()) => {
                    let mut subs = collect_subscriptions(&ui);
                    if subs.iter().any(|item| item == &topic) {
                        append_log(&ui, "SYS", &topic, "已在订阅列表中");
                    } else {
                        subs.push(topic.clone());
                        set_subscriptions(&ui, subs);
                        append_log(&ui, "SYS", &topic, "已发送订阅");
                    }
                    persist_config(&ui);
                }
                Err(err) => append_log(&ui, "ERR", &topic, &err),
            }
        }
    });

    ui.on_unsubscribe_requested({
        let ui_weak = ui_weak.clone();
        let mqtt = mqtt.clone();
        move |index| {
            let ui = ui_weak.unwrap();
            let mut subs = collect_subscriptions(&ui);
            let idx = index.max(0) as usize;
            if idx >= subs.len() {
                return;
            }
            let topic = subs[idx].clone();
            match mqtt.unsubscribe(&topic) {
                Ok(()) => {
                    subs.remove(idx);
                    set_subscriptions(&ui, subs);
                    persist_config(&ui);
                    append_log(&ui, "SYS", &topic, "已发送取消订阅");
                }
                Err(err) => append_log(&ui, "ERR", &topic, &err),
            }
        }
    });

    ui.on_publish_requested({
        let ui_weak = ui_weak.clone();
        let mqtt = mqtt.clone();
        move || {
            let ui = ui_weak.unwrap();
            if !ui.get_connected() {
                append_log(&ui, "ERR", "", "请先连接 Broker");
                return;
            }
            let topic = ui.get_pub_topic().trim().to_string();
            if topic.is_empty() {
                append_log(&ui, "ERR", "", "Topic 为空");
                return;
            }
            let payload = ui.get_pub_payload().to_string();
            match mqtt.publish(&topic, &payload) {
                Ok(()) => {
                    persist_config(&ui);
                    append_log(&ui, "SEND", &topic, &payload);
                }
                Err(err) => append_log(&ui, "ERR", &topic, &err),
            }
        }
    });

    ui.on_save_logs_requested({
        let ui_weak = ui_weak.clone();
        move || {
            let ui = ui_weak.unwrap();
            let model = ui.get_logs();
            let entries: Vec<LogEntry> = (0..model.row_count())
                .filter_map(|i| model.row_data(i))
                .collect();
            if entries.is_empty() {
                append_log(&ui, "ERR", "", "当前没有可保存的日志");
                return;
            }
            match save_logs_to_file(&entries) {
                Ok(path) => append_log(&ui, "SYS", "", &format!("日志已保存到 {}", path.display())),
                Err(err) => append_log(&ui, "ERR", "", &err),
            }
        }
    });

    ui.on_clear_logs_requested({
        let ui_weak = ui_weak.clone();
        move || {
            let ui = ui_weak.unwrap();
            ui.set_logs(ModelRc::new(VecModel::<LogEntry>::default()));
        }
    });

    let result = ui.run();
    persist_config(&ui);
    result
}

fn apply_event(ui: &AppWindow, event: UiEvent, mqtt: &MqttHandle) {
    match event {
        UiEvent::Connected => {
            ui.set_connecting(false);
            ui.set_connected(true);
            ui.set_status("已连接".into());
            ui.set_status_color(slint::Color::from_rgb_u8(0x4e, 0xcf, 0x7a));
            append_log(ui, "SYS", "", "连接成功");
            resubscribe_all(ui, mqtt);
        }
        UiEvent::Disconnected { reason } => {
            if ui.get_connected() || ui.get_connecting() {
                set_disconnected(ui, "未连接");
                if !reason.is_empty() {
                    append_log(ui, "SYS", "", &format!("连接结束: {reason}"));
                }
            }
        }
        UiEvent::Recv { topic, payload } => {
            append_log(ui, "RECV", &topic, &payload);
        }
        UiEvent::Error { message } => {
            append_log(ui, "ERR", "", &message);
        }
    }
}

fn set_disconnected(ui: &AppWindow, status: &str) {
    ui.set_connecting(false);
    ui.set_connected(false);
    ui.set_status(status.into());
    ui.set_status_color(slint::Color::from_rgb_u8(0x9a, 0xa0, 0xa6));
}

fn collect_subscriptions(ui: &AppWindow) -> Vec<String> {
    let model = ui.get_subscriptions();
    (0..model.row_count())
        .filter_map(|i| model.row_data(i))
        .map(|item| item.topic.to_string())
        .collect()
}

fn set_subscriptions(ui: &AppWindow, topics: Vec<String>) {
    let items: Vec<Subscription> = topics
        .into_iter()
        .map(|topic| Subscription { topic: topic.into() })
        .collect();
    ui.set_subscriptions(Rc::new(VecModel::from(items)).into());
}

fn resubscribe_all(ui: &AppWindow, mqtt: &MqttHandle) {
    for topic in collect_subscriptions(ui) {
        match mqtt.subscribe(&topic) {
            Ok(()) => append_log(ui, "SYS", &topic, "已重新订阅"),
            Err(err) => append_log(ui, "ERR", &topic, &err),
        }
    }
}

fn append_log(ui: &AppWindow, kind: &str, topic: &str, payload: &str) {
    let model = ui.get_logs();
    let mut entries: Vec<LogEntry> = (0..model.row_count())
        .filter_map(|i| model.row_data(i))
        .collect();
    entries.push(LogEntry {
        time: now_hms().into(),
        timestamp: now_timestamp().into(),
        kind: kind.into(),
        topic: topic.into(),
        payload: payload.into(),
    });
    if entries.len() > 500 {
        let extra = entries.len() - 500;
        entries.drain(0..extra);
    }
    ui.set_logs(Rc::new(VecModel::from(entries)).into());
}

fn apply_startup_config(ui: &AppWindow) {
    match config::load() {
        Ok(Some(cfg)) => {
            apply_config(ui, &cfg);
            append_log(
                ui,
                "SYS",
                "",
                &format!("已加载配置 {}", config::config_path().display()),
            );
        }
        Ok(None) => {}
        Err(err) => append_log(ui, "ERR", "", &err),
    }
}

fn collect_config(ui: &AppWindow) -> AppConfig {
    AppConfig {
        host: ui.get_host().to_string(),
        port: ui.get_port().to_string(),
        client_id: ui.get_client_id().to_string(),
        username: ui.get_username().to_string(),
        password: ui.get_password().to_string(),
        sub_topic: ui.get_sub_topic().to_string(),
        pub_topic: ui.get_pub_topic().to_string(),
        pub_payload: ui.get_pub_payload().to_string(),
        subscriptions: collect_subscriptions(ui),
    }
}

fn apply_config(ui: &AppWindow, cfg: &AppConfig) {
    ui.set_host(cfg.host.clone().into());
    ui.set_port(cfg.port.clone().into());
    ui.set_client_id(cfg.client_id.clone().into());
    ui.set_username(cfg.username.clone().into());
    ui.set_password(cfg.password.clone().into());
    ui.set_sub_topic(cfg.sub_topic.clone().into());
    ui.set_pub_topic(cfg.pub_topic.clone().into());
    ui.set_pub_payload(cfg.pub_payload.clone().into());
    set_subscriptions(ui, cfg.subscriptions.clone());
}

fn persist_config(ui: &AppWindow) {
    if let Err(err) = config::save(&collect_config(ui)) {
        append_log(ui, "ERR", "", &err);
    }
}

fn now_hms() -> String {
    Local::now().format("%H:%M:%S").to_string()
}

fn now_timestamp() -> String {
    Local::now().format("%Y-%m-%d %H:%M:%S").to_string()
}

fn save_logs_to_file(entries: &[LogEntry]) -> Result<std::path::PathBuf, String> {
    let dir = config::log_dir();
    std::fs::create_dir_all(&dir).map_err(|err| format!("创建日志目录失败: {err}"))?;
    let file_name = format!("mqttc-{}.log", Local::now().format("%Y%m%d-%H%M%S"));
    let path = dir.join(file_name);

    let mut content = String::new();
    for entry in entries {
        if entry.topic.is_empty() {
            content.push_str(&format!("[{}] [{}] {}\n", entry.timestamp, entry.kind, entry.payload));
        } else {
            content.push_str(&format!(
                "[{}] [{}] {} {}\n",
                entry.timestamp, entry.kind, entry.topic, entry.payload
            ));
        }
    }

    std::fs::write(&path, content).map_err(|err| format!("写入日志文件失败: {err}"))?;
    Ok(path)
}

fn default_client_id() -> String {
    let millis = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis())
        .unwrap_or(0);
    format!("mqttc-{millis:x}")
}
