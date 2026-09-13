#![cfg_attr(all(windows, not(debug_assertions)), windows_subsystem = "windows")]

mod mqtt;

use chrono::Local;
use mqtt::{MqttHandle, UiEvent};
use slint::{ComponentHandle, Model, ModelRc, VecModel};
use std::rc::Rc;
use std::time::{SystemTime, UNIX_EPOCH};

slint::include_modules!();

fn main() -> Result<(), slint::PlatformError> {
    let ui = AppWindow::new()?;
    let mqtt = MqttHandle::new();
    let ui_weak = ui.as_weak();

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

            ui.set_connecting(true);
            ui.set_connected(false);
            ui.set_status("连接中".into());
            ui.set_status_color(slint::Color::from_rgb_u8(0xe0, 0xb4, 0x4f));
            append_log(
                &ui,
                "SYS",
                "",
                &format!("正在连接 {host}:{port} ({client_id})"),
            );

            let ui_weak = ui_weak.clone();
            mqtt.connect(host, port, client_id, move |event| {
                let ui_weak = ui_weak.clone();
                let _ = slint::invoke_from_event_loop(move || {
                    if let Some(ui) = ui_weak.upgrade() {
                        apply_event(&ui, event);
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
                Ok(()) => append_log(&ui, "SYS", &topic, "已发送订阅"),
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
                Ok(()) => append_log(&ui, "SEND", &topic, &payload),
                Err(err) => append_log(&ui, "ERR", &topic, &err),
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

    ui.run()
}

fn apply_event(ui: &AppWindow, event: UiEvent) {
    match event {
        UiEvent::Connected => {
            ui.set_connecting(false);
            ui.set_connected(true);
            ui.set_status("已连接".into());
            ui.set_status_color(slint::Color::from_rgb_u8(0x4e, 0xcf, 0x7a));
            append_log(ui, "SYS", "", "连接成功");
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

fn append_log(ui: &AppWindow, kind: &str, topic: &str, payload: &str) {
    let model = ui.get_logs();
    let mut entries: Vec<LogEntry> = (0..model.row_count())
        .filter_map(|i| model.row_data(i))
        .collect();
    entries.push(LogEntry {
        time: now_hms().into(),
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

fn now_hms() -> String {
    Local::now().format("%H:%M:%S").to_string()
}

fn default_client_id() -> String {
    let millis = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis())
        .unwrap_or(0);
    format!("mqttc-{millis:x}")
}
