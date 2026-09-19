#![cfg_attr(all(windows, not(debug_assertions)), windows_subsystem = "windows")]

mod config;
mod mqtt;

use base64::Engine;
use chrono::Local;
use config::{AppConfig, ConnectionConfig, SubscriptionConfig};
use mqtt::{qos_from_u8, ConnectParams, MqttHandle, UiEvent};
use slint::{ComponentHandle, VecModel};
use std::cell::RefCell;
use std::collections::HashMap;
use std::path::PathBuf;
use std::rc::Rc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::thread;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

slint::include_modules!();

const MAX_LOGS: usize = 500;

thread_local! {
    static STATE: RefCell<Option<Rc<RefCell<AppState>>>> = RefCell::new(None);
}

#[derive(Clone, Copy, Debug, PartialEq)]
enum PayloadFormat {
    Text,
    Json,
    Base64,
    Hex,
}

impl PayloadFormat {
    fn from_index(index: i32) -> Self {
        match index {
            1 => Self::Json,
            2 => Self::Base64,
            3 => Self::Hex,
            _ => Self::Text,
        }
    }

    fn from_key(key: &str) -> Self {
        match key {
            "json" => Self::Json,
            "base64" => Self::Base64,
            "hex" => Self::Hex,
            _ => Self::Text,
        }
    }

    fn to_index(self) -> i32 {
        match self {
            Self::Text => 0,
            Self::Json => 1,
            Self::Base64 => 2,
            Self::Hex => 3,
        }
    }

    fn to_key(self) -> &'static str {
        match self {
            Self::Text => "text",
            Self::Json => "json",
            Self::Base64 => "base64",
            Self::Hex => "hex",
        }
    }
}

#[derive(Clone, Copy, PartialEq)]
enum ConnStatus {
    Disconnected,
    Connecting,
    Connected,
}

impl ConnStatus {
    fn text(self) -> &'static str {
        match self {
            Self::Disconnected => "未连接",
            Self::Connecting => "连接中",
            Self::Connected => "已连接",
        }
    }

    fn color(self) -> slint::Color {
        match self {
            Self::Disconnected => slint::Color::from_rgb_u8(0x9a, 0xa0, 0xa6),
            Self::Connecting => slint::Color::from_rgb_u8(0xe0, 0xb4, 0x4f),
            Self::Connected => slint::Color::from_rgb_u8(0x4e, 0xcf, 0x7a),
        }
    }
}

#[derive(Clone)]
struct LogData {
    time: String,
    timestamp: String,
    kind: String,
    topic: String,
    payload: Vec<u8>,
}

struct ConnRuntime {
    mqtt: MqttHandle,
    logs: Vec<LogData>,
    status: ConnStatus,
    timed_generation: Arc<AtomicU64>,
    timed_running: bool,
    active_sub_topic: String,
}

impl ConnRuntime {
    fn new() -> Self {
        Self {
            mqtt: MqttHandle::new(),
            logs: Vec::new(),
            status: ConnStatus::Disconnected,
            timed_generation: Arc::new(AtomicU64::new(0)),
            timed_running: false,
            active_sub_topic: String::new(),
        }
    }
}

struct AppState {
    config: AppConfig,
    runtimes: HashMap<String, ConnRuntime>,
    startup_error: Option<String>,
}

impl AppState {
    fn new() -> Self {
        Self {
            config: AppConfig::default(),
            runtimes: HashMap::new(),
            startup_error: None,
        }
    }

    fn runtime_mut(&mut self, id: &str) -> &mut ConnRuntime {
        self.runtimes
            .entry(id.to_string())
            .or_insert_with(ConnRuntime::new)
    }

    fn active_index(&self) -> Option<usize> {
        self.config
            .connections
            .iter()
            .position(|conn| conn.id == self.config.active_id)
    }

    fn active_id(&self) -> Option<String> {
        self.active_index()
            .map(|idx| self.config.connections[idx].id.clone())
    }

    fn status_of(&self, id: &str) -> ConnStatus {
        self.runtimes
            .get(id)
            .map(|rt| rt.status)
            .unwrap_or(ConnStatus::Disconnected)
    }

    fn is_connected(&self, id: &str) -> bool {
        self.status_of(id) == ConnStatus::Connected
    }

    fn stop_timed(&mut self, id: &str) {
        if let Some(runtime) = self.runtimes.get_mut(id) {
            runtime.timed_generation.fetch_add(1, Ordering::Relaxed);
            runtime.timed_running = false;
        }
    }
}

fn with_state<R>(f: impl FnOnce(&mut AppState) -> R) -> R {
    STATE.with(|slot| {
        let state = slot
            .borrow()
            .clone()
            .expect("application state is not initialized");
        let mut guard = state.borrow_mut();
        f(&mut guard)
    })
}

fn main() -> Result<(), slint::PlatformError> {
    let ui = AppWindow::new()?;
    let state = Rc::new(RefCell::new(AppState::new()));
    STATE.with(|slot| *slot.borrow_mut() = Some(state.clone()));

    {
        let mut guard = state.borrow_mut();
        match config::load() {
            Ok(Some(cfg)) => guard.config = cfg,
            Ok(None) => {}
            Err(err) => guard.startup_error = Some(err),
        }
        if guard.config.connections.is_empty() {
            let conn = ConnectionConfig::new("本地 Broker".to_string());
            guard.config.active_id = conn.id.clone();
            guard.config.connections.push(conn);
        }
        if guard.active_index().is_none() {
            guard.config.active_id = guard.config.connections[0].id.clone();
        }
        let ids: Vec<String> = guard
            .config
            .connections
            .iter()
            .map(|conn| conn.id.clone())
            .collect();
        for id in ids {
            guard.runtime_mut(&id);
        }
    }

    register_callbacks(&ui);

    {
        let mut guard = state.borrow_mut();
        apply_prefs(&ui, &guard);
        refresh_all(&ui, &mut guard);
        match guard.startup_error.take() {
            Some(err) => push_log(&ui, &mut guard, "ERR", "", err.into_bytes()),
            None => push_log(
                &ui,
                &mut guard,
                "SYS",
                "",
                format!("配置文件 {}", config::config_path().display()).into_bytes(),
            ),
        }
    }

    let result = ui.run();
    {
        let mut guard = state.borrow_mut();
        sync_form(&ui, &mut guard);
        sync_prefs(&ui, &mut guard);
        persist(&guard);
    }
    result
}

fn register_callbacks(ui: &AppWindow) {
    let ui_weak = ui.as_weak();

    ui.on_new_connection_requested({
        let ui_weak = ui_weak.clone();
        move || {
            let Some(ui) = ui_weak.upgrade() else { return };
            with_state(|st| {
                sync_form(&ui, st);
                let name = format!("连接 {}", st.config.connections.len() + 1);
                let conn = ConnectionConfig::new(name);
                st.config.active_id = conn.id.clone();
                st.config.connections.push(conn);
                let id = st.config.active_id.clone();
                st.runtime_mut(&id);
                show_page(&ui, 0);
                refresh_all(&ui, st);
                persist(st);
            });
        }
    });

    ui.on_select_connection_requested({
        let ui_weak = ui_weak.clone();
        move |id| {
            let Some(ui) = ui_weak.upgrade() else { return };
            let id = id.to_string();
            with_state(|st| {
                if st.config.active_id == id {
                    return;
                }
                sync_form(&ui, st);
                st.config.active_id = id;
                show_page(&ui, 1);
                refresh_all(&ui, st);
                persist(st);
            });
        }
    });

    ui.on_delete_connection_requested({
        let ui_weak = ui_weak.clone();
        move |id| {
            let Some(ui) = ui_weak.upgrade() else { return };
            let id = id.to_string();
            with_state(|st| {
                st.stop_timed(&id);
                if let Some(runtime) = st.runtimes.remove(&id) {
                    runtime.mqtt.disconnect();
                }
                st.config.connections.retain(|conn| conn.id != id);
                if st.config.active_id == id {
                    st.config.active_id = st
                        .config
                        .connections
                        .first()
                        .map(|conn| conn.id.clone())
                        .unwrap_or_default();
                }
                if st.config.connections.is_empty() {
                    let conn = ConnectionConfig::new("本地 Broker".to_string());
                    st.config.active_id = conn.id.clone();
                    st.config.connections.push(conn);
                }
                let active = st.config.active_id.clone();
                st.runtime_mut(&active);
                show_page(&ui, 1);
                refresh_all(&ui, st);
                persist(st);
            });
        }
    });

    ui.on_connect_requested({
        let ui_weak = ui_weak.clone();
        move || {
            let Some(ui) = ui_weak.upgrade() else { return };
            with_state(|st| {
                sync_form(&ui, st);
                let Some(idx) = st.active_index() else { return };
                let mut cfg = st.config.connections[idx].clone();

                let host = cfg.host.trim().to_string();
                if host.is_empty() {
                    push_log(&ui, st, "ERR", "", "主机不能为空".as_bytes().to_vec());
                    return;
                }
                let port = match cfg.port.trim().parse::<u16>() {
                    Ok(port) if port > 0 => port,
                    _ => {
                        push_log(&ui, st, "ERR", "", "端口无效，范围为 1-65535".as_bytes().to_vec());
                        return;
                    }
                };
                if cfg.client_id.trim().is_empty() {
                    cfg.client_id = default_client_id();
                    ui.set_client_id(cfg.client_id.clone().into());
                }
                cfg.host = host.clone();
                cfg.username = cfg.username.trim().to_string();
                st.config.connections[idx] = cfg.clone();

                let auth_hint = if cfg.username.is_empty() {
                    "匿名".to_string()
                } else {
                    format!("用户 {}", cfg.username)
                };
                let id = cfg.id.clone();
                st.stop_timed(&id);
                st.runtime_mut(&id).status = ConnStatus::Connecting;
                let will_hint = if cfg.will_topic.trim().is_empty() {
                    String::new()
                } else {
                    format!("，遗嘱 {}", cfg.will_topic.trim())
                };
                let ssl_hint = if cfg.ssl {
                    if cfg.ssl_verify {
                        "，SSL 校验证书"
                    } else {
                        "，SSL 不校验证书"
                    }
                } else {
                    ""
                };
                push_log(
                    &ui,
                    st,
                    "SYS",
                    "",
                    format!(
                        "正在连接 {}:{} ({}, {}，Keep Alive {}s，超时 {}s{}{})",
                        cfg.host,
                        cfg.port,
                        cfg.client_id,
                        auth_hint,
                        cfg.keep_alive,
                        cfg.connect_timeout,
                        ssl_hint,
                        will_hint
                    )
                    .into_bytes(),
                );

                let will_payload = cfg.will_payload.as_bytes().to_vec();
                let params = ConnectParams {
                    host: cfg.host.clone(),
                    port,
                    client_id: cfg.client_id.clone(),
                    username: cfg.username.clone(),
                    password: cfg.password.clone(),
                    keep_alive: cfg.keep_alive,
                    clean_session: cfg.clean_session,
                    connect_timeout: cfg.connect_timeout,
                    will_topic: cfg.will_topic.clone(),
                    will_payload,
                    will_qos: cfg.will_qos,
                    will_retain: cfg.will_retain,
                    ssl: cfg.ssl,
                    ssl_verify: cfg.ssl_verify,
                    ssl_ca: cfg.ssl_ca.clone(),
                    ssl_cert: cfg.ssl_cert.clone(),
                    ssl_key: cfg.ssl_key.clone(),
                };
                let ui_weak2 = ui.as_weak();
                let conn_id = id.clone();
                let mqtt = st.runtime_mut(&id).mqtt.clone();
                mqtt.connect(params, move |event| {
                    let ui_weak3 = ui_weak2.clone();
                    let conn_id = conn_id.clone();
                    let _ = slint::invoke_from_event_loop(move || {
                        if let Some(ui) = ui_weak3.upgrade() {
                            with_state(|st| handle_event(&ui, st, &conn_id, event));
                        }
                    });
                });

                show_page(&ui, 1);
                refresh_status(&ui, st);
                refresh_connections(&ui, st);
                persist(st);
            });
        }
    });

    ui.on_edit_connection_requested({
        let ui_weak = ui_weak.clone();
        move || {
            let Some(ui) = ui_weak.upgrade() else { return };
            with_state(|st| {
                sync_form(&ui, st);
                persist(st);
                show_page(&ui, 0);
            });
        }
    });

    ui.on_back_to_session_requested({
        let ui_weak = ui_weak.clone();
        move || {
            let Some(ui) = ui_weak.upgrade() else { return };
            with_state(|st| {
                sync_form(&ui, st);
                persist(st);
                show_page(&ui, 1);
            });
        }
    });

    ui.on_disconnect_requested({
        let ui_weak = ui_weak.clone();
        move || {
            let Some(ui) = ui_weak.upgrade() else { return };
            with_state(|st| {
                sync_form(&ui, st);
                let Some(id) = st.active_id() else { return };
                st.stop_timed(&id);
                st.runtime_mut(&id).mqtt.disconnect();
                st.runtime_mut(&id).status = ConnStatus::Disconnected;
                ui.set_timed_running(false);
                push_log(&ui, st, "SYS", "", "已断开连接".as_bytes().to_vec());
                refresh_status(&ui, st);
                refresh_connections(&ui, st);
                persist(st);
            });
        }
    });

    ui.on_subscribe_requested({
        let ui_weak = ui_weak.clone();
        move || {
            let Some(ui) = ui_weak.upgrade() else { return };
            with_state(|st| {
                sync_form(&ui, st);
                let Some(id) = st.active_id() else { return };
                if !st.is_connected(&id) {
                    push_log(&ui, st, "ERR", "", "请先连接 Broker".as_bytes().to_vec());
                    return;
                }
                let topic = ui.get_sub_topic().trim().to_string();
                if topic.is_empty() {
                    push_log(&ui, st, "ERR", "", "Topic 为空".as_bytes().to_vec());
                    return;
                }
                let qos = ui.get_sub_qos().clamp(0, 2) as u8;
                let mqtt = st.runtime_mut(&id).mqtt.clone();
                match mqtt.subscribe(&topic, qos_from_u8(qos)) {
                    Ok(()) => {
                        let Some(idx) = st.active_index() else { return };
                        let exists = st.config.connections[idx]
                            .subscriptions
                            .iter()
                            .any(|sub| sub.topic == topic);
                        if exists {
                            push_log(&ui, st, "SYS", &topic, "已在订阅列表中".as_bytes().to_vec());
                        } else {
                            st.config.connections[idx].subscriptions.push(SubscriptionConfig {
                                topic: topic.clone(),
                                qos,
                            });
                            push_log(
                                &ui,
                                st,
                                "SYS",
                                &topic,
                                format!("已订阅 QoS {qos}").into_bytes(),
                            );
                            refresh_subscriptions(&ui, st);
                        }
                        persist(st);
                    }
                    Err(err) => push_log(&ui, st, "ERR", &topic, err.into_bytes()),
                }
            });
        }
    });

    ui.on_unsubscribe_requested({
        let ui_weak = ui_weak.clone();
        move |index| {
            let Some(ui) = ui_weak.upgrade() else { return };
            with_state(|st| {
                let Some(id) = st.active_id() else { return };
                let Some(idx) = st.active_index() else { return };
                let pos = index.max(0) as usize;
                if pos >= st.config.connections[idx].subscriptions.len() {
                    return;
                }
                let sub = st.config.connections[idx].subscriptions[pos].clone();
                let mqtt = st.runtime_mut(&id).mqtt.clone();
                if let Err(err) = mqtt.unsubscribe(&sub.topic) {
                    push_log(&ui, st, "ERR", &sub.topic, err.into_bytes());
                } else {
                    push_log(&ui, st, "SYS", &sub.topic, "已取消订阅".as_bytes().to_vec());
                }
                if let Some(idx) = st.active_index() {
                    if pos < st.config.connections[idx].subscriptions.len() {
                        st.config.connections[idx].subscriptions.remove(pos);
                    }
                }
                if st.runtime_mut(&id).active_sub_topic == sub.topic {
                    st.runtime_mut(&id).active_sub_topic.clear();
                }
                refresh_subscriptions(&ui, st);
                refresh_logs(&ui, st);
                persist(st);
            });
        }
    });

    ui.on_publish_requested({
        let ui_weak = ui_weak.clone();
        move || {
            let Some(ui) = ui_weak.upgrade() else { return };
            with_state(|st| {
                sync_form(&ui, st);
                let _ = publish_from_form(&ui, st, true);
            });
        }
    });

    ui.on_generate_client_id_requested({
        let ui_weak = ui_weak.clone();
        move || {
            let Some(ui) = ui_weak.upgrade() else { return };
            let id = default_client_id();
            ui.set_client_id(id.clone().into());
            with_state(|st| {
                if let Some(idx) = st.active_index() {
                    st.config.connections[idx].client_id = id;
                    persist(st);
                }
            });
        }
    });

    ui.on_filter_subscription_requested({
        let ui_weak = ui_weak.clone();
        move |topic| {
            let Some(ui) = ui_weak.upgrade() else { return };
            let topic = topic.to_string();
            with_state(|st| {
                let Some(id) = st.active_id() else { return };
                let current = st.runtime_mut(&id).active_sub_topic.clone();
                st.runtime_mut(&id).active_sub_topic = if current == topic {
                    String::new()
                } else {
                    topic
                };
                refresh_subscriptions(&ui, st);
                refresh_logs(&ui, st);
            });
        }
    });

    ui.on_timed_toggle_requested({
        let ui_weak = ui_weak.clone();
        move || {
            let Some(ui) = ui_weak.upgrade() else { return };
            with_state(|st| {
                sync_form(&ui, st);
                let Some(id) = st.active_id() else { return };
                if st.runtime_mut(&id).timed_running {
                    st.stop_timed(&id);
                    ui.set_timed_running(false);
                    push_log(&ui, st, "SYS", "", "已停止定时发布".as_bytes().to_vec());
                    persist(st);
                    return;
                }
                if !st.is_connected(&id) {
                    push_log(&ui, st, "ERR", "", "请先连接 Broker".as_bytes().to_vec());
                    return;
                }
                let interval = match parse_timed_interval(&ui.get_timed_interval()) {
                    Ok(secs) => secs,
                    Err(err) => {
                        push_log(&ui, st, "ERR", "", err.into_bytes());
                        return;
                    }
                };
                if let Some(idx) = st.active_index() {
                    st.config.connections[idx].timed_interval = interval;
                }
                if publish_from_connection(&ui, st, &id).is_err() {
                    return;
                }
                let generation = {
                    let runtime = st.runtime_mut(&id);
                    let next = runtime.timed_generation.load(Ordering::Relaxed) + 1;
                    runtime.timed_generation.store(next, Ordering::Relaxed);
                    runtime.timed_running = true;
                    next
                };
                ui.set_timed_running(true);
                push_log(
                    &ui,
                    st,
                    "SYS",
                    "",
                    format!("已启动定时发布，每 {interval} 秒").into_bytes(),
                );
                persist(st);

                let ui_weak2 = ui.as_weak();
                let conn_id = id.clone();
                let gen = st.runtime_mut(&id).timed_generation.clone();
                thread::spawn(move || loop {
                    thread::sleep(Duration::from_secs(interval as u64));
                    if gen.load(Ordering::Relaxed) != generation {
                        break;
                    }
                    let ui_weak3 = ui_weak2.clone();
                    let conn_id = conn_id.clone();
                    let gen = gen.clone();
                    let _ = slint::invoke_from_event_loop(move || {
                        if gen.load(Ordering::Relaxed) != generation {
                            return;
                        }
                        let Some(ui) = ui_weak3.upgrade() else { return };
                        with_state(|st| {
                            if !st.is_connected(&conn_id) {
                                st.stop_timed(&conn_id);
                                if st.config.active_id == conn_id {
                                    ui.set_timed_running(false);
                                }
                                return;
                            }
                            if st.config.active_id == conn_id {
                                sync_form(&ui, st);
                            }
                            let _ = publish_from_connection(&ui, st, &conn_id);
                        });
                    });
                });
            });
        }
    });

    ui.on_format_payload_requested({
        let ui_weak = ui_weak.clone();
        move || {
            let Some(ui) = ui_weak.upgrade() else { return };
            let text = ui.get_pub_payload().to_string();
            if text.trim().is_empty() {
                return;
            }
            match beautify_json(&text) {
                Ok(pretty) => ui.set_pub_payload(pretty.into()),
                Err(err) => with_state(|st| push_log(&ui, st, "ERR", "", err.into_bytes())),
            }
        }
    });

    ui.on_log_format_changed({
        let ui_weak = ui_weak.clone();
        move || {
            let Some(ui) = ui_weak.upgrade() else { return };
            with_state(|st| {
                sync_prefs(&ui, st);
                refresh_logs(&ui, st);
                persist(st);
            });
        }
    });

    ui.on_log_filter_changed({
        let ui_weak = ui_weak.clone();
        move || {
            let Some(ui) = ui_weak.upgrade() else { return };
            with_state(|st| {
                sync_prefs(&ui, st);
                refresh_logs(&ui, st);
            });
        }
    });

    ui.on_save_logs_requested({
        let ui_weak = ui_weak.clone();
        move || {
            let Some(ui) = ui_weak.upgrade() else { return };
            with_state(|st| {
                let Some(id) = st.active_id() else { return };
                let format = PayloadFormat::from_index(ui.get_log_format());
                let logs = st.runtime_mut(&id).logs.clone();
                if logs.is_empty() {
                    push_log(&ui, st, "ERR", "", "当前没有可保存的日志".as_bytes().to_vec());
                    return;
                }
                match save_logs_to_file(&logs, format) {
                    Ok(path) => push_log(
                        &ui,
                        st,
                        "SYS",
                        "",
                        format!("日志已保存到 {}", path.display()).into_bytes(),
                    ),
                    Err(err) => push_log(&ui, st, "ERR", "", err.into_bytes()),
                }
            });
        }
    });

    ui.on_clear_logs_requested({
        let ui_weak = ui_weak.clone();
        move || {
            let Some(ui) = ui_weak.upgrade() else { return };
            with_state(|st| {
                let Some(id) = st.active_id() else { return };
                st.runtime_mut(&id).logs.clear();
                refresh_logs(&ui, st);
            });
        }
    });
}

fn handle_event(ui: &AppWindow, st: &mut AppState, conn_id: &str, event: UiEvent) {
    match event {
        UiEvent::Connected => {
            st.runtime_mut(conn_id).status = ConnStatus::Connected;
            push_log_for(ui, st, conn_id, "SYS", "", "连接成功".as_bytes().to_vec());
            resubscribe_all(ui, st, conn_id);
            if st.config.active_id == conn_id {
                show_page(ui, 1);
            }
        }
        UiEvent::Disconnected { reason } => {
            let was_timed = st
                .runtimes
                .get(conn_id)
                .map(|rt| rt.timed_running)
                .unwrap_or(false);
            st.stop_timed(conn_id);
            if st.config.active_id == conn_id {
                ui.set_timed_running(false);
            }
            let previous = st.runtime_mut(conn_id).status;
            st.runtime_mut(conn_id).status = ConnStatus::Disconnected;
            if previous != ConnStatus::Disconnected && !reason.is_empty() {
                push_log_for(
                    ui,
                    st,
                    conn_id,
                    "SYS",
                    "",
                    format!("连接结束: {reason}").into_bytes(),
                );
            }
            if was_timed {
                push_log_for(
                    ui,
                    st,
                    conn_id,
                    "SYS",
                    "",
                    "连接断开，已停止定时发布".as_bytes().to_vec(),
                );
            }
        }
        UiEvent::Recv { topic, payload } => {
            push_log_for(ui, st, conn_id, "RECV", &topic, payload);
        }
        UiEvent::Error { message } => {
            push_log_for(ui, st, conn_id, "ERR", "", message.into_bytes());
        }
    }
    refresh_status(ui, st);
    refresh_connections(ui, st);
}

fn resubscribe_all(ui: &AppWindow, st: &mut AppState, conn_id: &str) {
    let subs = st
        .config
        .connections
        .iter()
        .find(|conn| conn.id == conn_id)
        .map(|conn| conn.subscriptions.clone())
        .unwrap_or_default();
    if subs.is_empty() {
        return;
    }
    let mqtt = st.runtime_mut(conn_id).mqtt.clone();
    for sub in subs {
        match mqtt.subscribe(&sub.topic, qos_from_u8(sub.qos)) {
            Ok(()) => push_log_for(
                ui,
                st,
                conn_id,
                "SYS",
                &sub.topic,
                "已重新订阅".as_bytes().to_vec(),
            ),
            Err(err) => push_log_for(ui, st, conn_id, "ERR", &sub.topic, err.into_bytes()),
        }
    }
}

fn push_log(ui: &AppWindow, st: &mut AppState, kind: &str, topic: &str, payload: Vec<u8>) {
    if let Some(id) = st.active_id() {
        push_log_for(ui, st, &id, kind, topic, payload);
    }
}

fn push_log_for(
    ui: &AppWindow,
    st: &mut AppState,
    conn_id: &str,
    kind: &str,
    topic: &str,
    payload: Vec<u8>,
) {
    let entry = LogData {
        time: now_hms(),
        timestamp: now_timestamp(),
        kind: kind.to_string(),
        topic: topic.to_string(),
        payload,
    };
    {
        let runtime = st.runtime_mut(conn_id);
        runtime.logs.push(entry);
        if runtime.logs.len() > MAX_LOGS {
            let extra = runtime.logs.len() - MAX_LOGS;
            runtime.logs.drain(0..extra);
        }
    }
    if st.config.active_id == conn_id {
        refresh_logs(ui, st);
    }
}

fn apply_prefs(ui: &AppWindow, st: &AppState) {
    ui.set_log_format(PayloadFormat::from_key(&st.config.log_format).to_index());
    ui.set_auto_scroll(st.config.auto_scroll);
    ui.set_log_filter(st.config.log_filter.clone().into());
    ui.set_log_kind_filter(st.config.log_kind_filter.clamp(0, 4));
    show_page(ui, 1);
}

fn show_page(ui: &AppWindow, page: i32) {
    ui.set_page(page);
}

fn refresh_all(ui: &AppWindow, st: &mut AppState) {
    refresh_connections(ui, st);
    refresh_active_form(ui, st);
    refresh_status(ui, st);
    refresh_subscriptions(ui, st);
    refresh_logs(ui, st);
}

fn refresh_connections(ui: &AppWindow, st: &AppState) {
    let entries: Vec<ConnectionEntry> = st
        .config
        .connections
        .iter()
        .map(|conn| {
            let status = st.status_of(&conn.id);
            ConnectionEntry {
                id: conn.id.clone().into(),
                name: conn.name.clone().into(),
                status: status.text().into(),
                status_color: status.color(),
            }
        })
        .collect();
    ui.set_active_connection_id(st.config.active_id.clone().into());
    ui.set_connections(Rc::new(VecModel::from(entries)).into());
}

fn refresh_active_form(ui: &AppWindow, st: &AppState) {
    let Some(idx) = st.active_index() else { return };
    let cfg = &st.config.connections[idx];
    ui.set_connection_name(cfg.name.clone().into());
    ui.set_host(cfg.host.clone().into());
    ui.set_port(cfg.port.clone().into());
    ui.set_client_id(cfg.client_id.clone().into());
    ui.set_username(cfg.username.clone().into());
    ui.set_password(cfg.password.clone().into());
    ui.set_keep_alive(cfg.keep_alive.to_string().into());
    ui.set_clean_session(cfg.clean_session);
    ui.set_connect_timeout(cfg.connect_timeout.to_string().into());
    ui.set_will_topic(cfg.will_topic.clone().into());
    ui.set_will_payload(cfg.will_payload.clone().into());
    ui.set_will_qos(cfg.will_qos.min(2) as i32);
    ui.set_will_retain(cfg.will_retain);
    ui.set_ssl(cfg.ssl);
    ui.set_ssl_verify(cfg.ssl_verify);
    ui.set_ssl_ca(cfg.ssl_ca.clone().into());
    ui.set_ssl_cert(cfg.ssl_cert.clone().into());
    ui.set_ssl_key(cfg.ssl_key.clone().into());
    ui.set_connection_summary(format!("{}:{}", cfg.host, cfg.port).into());
    ui.set_timed_interval(cfg.timed_interval.max(1).to_string().into());
    ui.set_timed_running(
        st.runtimes
            .get(&cfg.id)
            .map(|rt| rt.timed_running)
            .unwrap_or(false),
    );
    ui.set_sub_topic(cfg.sub_topic.clone().into());
    ui.set_sub_qos(cfg.sub_qos.min(2) as i32);
    ui.set_pub_topic(cfg.pub_topic.clone().into());
    ui.set_pub_payload(cfg.pub_payload.clone().into());
    ui.set_pub_qos(cfg.pub_qos.min(2) as i32);
    ui.set_pub_retain(cfg.pub_retain);
    ui.set_pub_format(PayloadFormat::from_key(&cfg.pub_format).to_index());
}

fn refresh_status(ui: &AppWindow, st: &AppState) {
    let status = st
        .active_id()
        .map(|id| st.status_of(&id))
        .unwrap_or(ConnStatus::Disconnected);
    ui.set_status(status.text().into());
    ui.set_status_color(status.color());
    ui.set_connected(status == ConnStatus::Connected);
    ui.set_connecting(status == ConnStatus::Connecting);
}

fn refresh_subscriptions(ui: &AppWindow, st: &AppState) {
    let active_topic = st
        .active_id()
        .and_then(|id| st.runtimes.get(&id).map(|rt| rt.active_sub_topic.clone()))
        .unwrap_or_default();
    let items: Vec<Subscription> = st
        .active_index()
        .map(|idx| st.config.connections[idx].subscriptions.clone())
        .unwrap_or_default()
        .into_iter()
        .map(|sub| {
            let selected = !active_topic.is_empty() && sub.topic == active_topic;
            Subscription {
                topic: sub.topic.into(),
                qos: sub.qos.min(2) as i32,
                selected,
            }
        })
        .collect();
    ui.set_active_sub_topic(active_topic.into());
    ui.set_subscriptions(Rc::new(VecModel::from(items)).into());
}

fn refresh_logs(ui: &AppWindow, st: &AppState) {
    let format = PayloadFormat::from_index(ui.get_log_format());
    let filter = ui.get_log_filter().trim().to_lowercase();
    let kind_filter = ui.get_log_kind_filter();
    let active_topic = st
        .active_id()
        .and_then(|id| st.runtimes.get(&id).map(|rt| rt.active_sub_topic.clone()))
        .unwrap_or_default();
    let entries: Vec<LogEntry> = st
        .active_id()
        .and_then(|id| st.runtimes.get(&id))
        .map(|runtime| runtime.logs.clone())
        .unwrap_or_default()
        .iter()
        .filter(|log| log_matches(log, format, &filter, kind_filter, &active_topic))
        .map(|log| render_log(log, format))
        .collect();
    ui.set_logs(Rc::new(VecModel::from(entries)).into());
}

fn sync_form(ui: &AppWindow, st: &mut AppState) {
    let Some(idx) = st.active_index() else { return };
    let cfg = &mut st.config.connections[idx];
    cfg.name = ui.get_connection_name().to_string();
    cfg.host = ui.get_host().to_string();
    cfg.port = ui.get_port().to_string();
    cfg.client_id = ui.get_client_id().to_string();
    cfg.username = ui.get_username().to_string();
    cfg.password = ui.get_password().to_string();
    cfg.keep_alive = parse_u16_clamped(&ui.get_keep_alive(), 1, 65535, 60);
    cfg.clean_session = ui.get_clean_session();
    cfg.connect_timeout = parse_u16_clamped(&ui.get_connect_timeout(), 1, 300, 10);
    cfg.will_topic = ui.get_will_topic().to_string();
    cfg.will_payload = ui.get_will_payload().to_string();
    cfg.will_qos = ui.get_will_qos().clamp(0, 2) as u8;
    cfg.will_retain = ui.get_will_retain();
    cfg.ssl = ui.get_ssl();
    cfg.ssl_verify = ui.get_ssl_verify();
    cfg.ssl_ca = ui.get_ssl_ca().to_string();
    cfg.ssl_cert = ui.get_ssl_cert().to_string();
    cfg.ssl_key = ui.get_ssl_key().to_string();
    if let Ok(interval) = parse_timed_interval(&ui.get_timed_interval()) {
        cfg.timed_interval = interval;
    }
    cfg.sub_topic = ui.get_sub_topic().to_string();
    cfg.sub_qos = ui.get_sub_qos().clamp(0, 2) as u8;
    cfg.pub_topic = ui.get_pub_topic().to_string();
    cfg.pub_payload = ui.get_pub_payload().to_string();
    cfg.pub_qos = ui.get_pub_qos().clamp(0, 2) as u8;
    cfg.pub_retain = ui.get_pub_retain();
    cfg.pub_format = PayloadFormat::from_index(ui.get_pub_format())
        .to_key()
        .to_string();
    if cfg.name.trim().is_empty() {
        cfg.name = format!("{}:{}", cfg.host.trim(), cfg.port.trim());
    }
}

fn sync_prefs(ui: &AppWindow, st: &mut AppState) {
    st.config.log_format = PayloadFormat::from_index(ui.get_log_format())
        .to_key()
        .to_string();
    st.config.auto_scroll = ui.get_auto_scroll();
    st.config.log_filter = ui.get_log_filter().to_string();
    st.config.log_kind_filter = ui.get_log_kind_filter().clamp(0, 4);
}

fn publish_from_form(ui: &AppWindow, st: &mut AppState, persist_on_ok: bool) -> Result<(), ()> {
    let Some(id) = st.active_id() else {
        return Err(());
    };
    publish_from_connection_inner(ui, st, &id, persist_on_ok, true)
}

fn publish_from_connection(ui: &AppWindow, st: &mut AppState, conn_id: &str) -> Result<(), ()> {
    publish_from_connection_inner(ui, st, conn_id, false, false)
}

fn publish_from_connection_inner(
    ui: &AppWindow,
    st: &mut AppState,
    conn_id: &str,
    persist_on_ok: bool,
    from_form: bool,
) -> Result<(), ()> {
    if !st.is_connected(conn_id) {
        push_log_for(ui, st, conn_id, "ERR", "", "请先连接 Broker".as_bytes().to_vec());
        return Err(());
    }
    let Some(idx) = st
        .config
        .connections
        .iter()
        .position(|conn| conn.id == conn_id)
    else {
        return Err(());
    };
    let cfg = &st.config.connections[idx];
    let topic = if from_form {
        ui.get_pub_topic().trim().to_string()
    } else {
        cfg.pub_topic.trim().to_string()
    };
    if topic.is_empty() {
        push_log_for(ui, st, conn_id, "ERR", "", "Topic 为空".as_bytes().to_vec());
        return Err(());
    }
    let format = if from_form {
        PayloadFormat::from_index(ui.get_pub_format())
    } else {
        PayloadFormat::from_key(&cfg.pub_format)
    };
    let source = if from_form {
        ui.get_pub_payload().to_string()
    } else {
        cfg.pub_payload.clone()
    };
    let payload = match encode_payload(&source, format) {
        Ok(bytes) => bytes,
        Err(err) => {
            push_log_for(ui, st, conn_id, "ERR", &topic, err.into_bytes());
            return Err(());
        }
    };
    let qos = if from_form {
        ui.get_pub_qos().clamp(0, 2) as u8
    } else {
        cfg.pub_qos.min(2)
    };
    let retain = if from_form {
        ui.get_pub_retain()
    } else {
        cfg.pub_retain
    };
    let mqtt = st.runtime_mut(conn_id).mqtt.clone();
    match mqtt.publish(&topic, qos_from_u8(qos), retain, payload.clone()) {
        Ok(()) => {
            push_log_for(ui, st, conn_id, "SEND", &topic, payload);
            if persist_on_ok {
                persist(st);
            }
            Ok(())
        }
        Err(err) => {
            push_log_for(ui, st, conn_id, "ERR", &topic, err.into_bytes());
            Err(())
        }
    }
}

fn log_matches(
    log: &LogData,
    format: PayloadFormat,
    filter: &str,
    kind_filter: i32,
    active_topic: &str,
) -> bool {
    if !kind_matches(&log.kind, kind_filter) {
        return false;
    }
    if !active_topic.is_empty()
        && (log.kind == "RECV" || log.kind == "SEND")
        && !topic_matches_filter(&log.topic, active_topic)
    {
        return false;
    }
    if filter.is_empty() {
        return true;
    }
    let payload = log_payload_text(log, format).to_lowercase();
    log.topic.to_lowercase().contains(filter)
        || payload.contains(filter)
        || log.kind.to_lowercase().contains(filter)
}

fn topic_matches_filter(topic: &str, filter: &str) -> bool {
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

fn kind_matches(kind: &str, filter: i32) -> bool {
    match filter {
        1 => kind == "RECV",
        2 => kind == "SEND",
        3 => kind == "SYS",
        4 => kind == "ERR",
        _ => true,
    }
}

fn parse_u16_clamped(text: &str, min: u16, max: u16, fallback: u16) -> u16 {
    text.trim()
        .parse::<u16>()
        .ok()
        .filter(|v| *v >= min && *v <= max)
        .unwrap_or(fallback)
}

fn parse_timed_interval(text: &str) -> Result<u32, String> {
    let value = text
        .trim()
        .parse::<u32>()
        .map_err(|_| "定时间隔无效，范围为 1-3600 秒".to_string())?;
    if (1..=3600).contains(&value) {
        Ok(value)
    } else {
        Err("定时间隔无效，范围为 1-3600 秒".to_string())
    }
}

fn persist(st: &AppState) {
    if let Err(err) = config::save(&st.config) {
        eprintln!("保存配置失败: {err}");
    }
}

fn render_log(log: &LogData, format: PayloadFormat) -> LogEntry {
    LogEntry {
        time: log.time.clone().into(),
        timestamp: log.timestamp.clone().into(),
        kind: log.kind.clone().into(),
        topic: log.topic.clone().into(),
        payload: log_payload_text(log, format).into(),
    }
}

fn log_payload_text(log: &LogData, format: PayloadFormat) -> String {
    if log.kind == "RECV" || log.kind == "SEND" {
        render_payload(&log.payload, format)
    } else {
        String::from_utf8_lossy(&log.payload).into_owned()
    }
}

fn render_payload(bytes: &[u8], format: PayloadFormat) -> String {
    match format {
        PayloadFormat::Text => String::from_utf8_lossy(bytes).into_owned(),
        PayloadFormat::Json => match serde_json::from_slice::<serde_json::Value>(bytes) {
            Ok(value) => serde_json::to_string_pretty(&value)
                .unwrap_or_else(|_| String::from_utf8_lossy(bytes).into_owned()),
            Err(_) => String::from_utf8_lossy(bytes).into_owned(),
        },
        PayloadFormat::Base64 => base64::engine::general_purpose::STANDARD.encode(bytes),
        PayloadFormat::Hex => encode_hex(bytes),
    }
}

fn encode_payload(text: &str, format: PayloadFormat) -> Result<Vec<u8>, String> {
    match format {
        PayloadFormat::Text => Ok(text.as_bytes().to_vec()),
        PayloadFormat::Json => {
            let trimmed = text.trim();
            if trimmed.is_empty() {
                return Ok(Vec::new());
            }
            serde_json::from_str::<serde_json::Value>(trimmed)
                .map_err(|err| format!("JSON 格式无效: {err}"))?;
            Ok(text.as_bytes().to_vec())
        }
        PayloadFormat::Base64 => {
            let cleaned: String = text.chars().filter(|c| !c.is_whitespace()).collect();
            base64::engine::general_purpose::STANDARD
                .decode(cleaned)
                .map_err(|err| format!("Base64 解码失败: {err}"))
        }
        PayloadFormat::Hex => decode_hex(text),
    }
}

fn beautify_json(text: &str) -> Result<String, String> {
    let value: serde_json::Value =
        serde_json::from_str(text.trim()).map_err(|err| format!("JSON 格式无效: {err}"))?;
    serde_json::to_string_pretty(&value).map_err(|err| format!("JSON 格式化失败: {err}"))
}

fn encode_hex(bytes: &[u8]) -> String {
    let mut out = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        out.push_str(&format!("{byte:02X}"));
    }
    out
}

fn decode_hex(text: &str) -> Result<Vec<u8>, String> {
    let cleaned: String = text
        .chars()
        .filter(|c| !c.is_whitespace() && *c != ',' && *c != ':' && *c != '-')
        .collect();
    if cleaned.is_empty() {
        return Ok(Vec::new());
    }
    let chars: Vec<char> = cleaned.chars().collect();
    if chars.len() % 2 != 0 {
        return Err("Hex 长度必须为偶数".to_string());
    }
    let mut out = Vec::with_capacity(chars.len() / 2);
    for pair in chars.chunks(2) {
        let token: String = pair.iter().collect();
        let byte = u8::from_str_radix(&token, 16).map_err(|_| format!("Hex 字符无效: {token}"))?;
        out.push(byte);
    }
    Ok(out)
}

fn save_logs_to_file(logs: &[LogData], format: PayloadFormat) -> Result<PathBuf, String> {
    let dir = config::log_dir();
    std::fs::create_dir_all(&dir).map_err(|err| format!("创建日志目录失败: {err}"))?;
    let path = dir.join(format!("mqttc-{}.log", Local::now().format("%Y%m%d-%H%M%S")));

    let mut content = String::new();
    for log in logs {
        let payload = log_payload_text(log, format);
        if log.topic.is_empty() {
            content.push_str(&format!("[{}] [{}] {}\n", log.timestamp, log.kind, payload));
        } else {
            content.push_str(&format!(
                "[{}] [{}] {} {}\n",
                log.timestamp, log.kind, log.topic, payload
            ));
        }
    }

    std::fs::write(&path, content).map_err(|err| format!("写入日志文件失败: {err}"))?;
    Ok(path)
}

fn now_hms() -> String {
    Local::now().format("%H:%M:%S").to_string()
}

fn now_timestamp() -> String {
    Local::now().format("%Y-%m-%d %H:%M:%S").to_string()
}

fn default_client_id() -> String {
    let millis = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis())
        .unwrap_or(0);
    format!("mqttc-{millis:x}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn text_payload_is_verbatim() {
        let payload = encode_payload("line1\nline2", PayloadFormat::Text).unwrap();
        assert_eq!(payload, b"line1\nline2");
    }

    #[test]
    fn json_payload_is_validated() {
        assert!(encode_payload("{\"a\":1}", PayloadFormat::Json).is_ok());
        assert!(encode_payload("{oops", PayloadFormat::Json).is_err());
    }

    #[test]
    fn json_can_be_beautified() {
        let pretty = beautify_json("{\"a\":[1,2]}").unwrap();
        assert!(pretty.contains('\n'));
        assert!(pretty.contains("\"a\""));
    }

    #[test]
    fn base64_payload_round_trip() {
        let bytes = encode_payload("aGVsbG8=", PayloadFormat::Base64).unwrap();
        assert_eq!(bytes, b"hello");
        assert_eq!(render_payload(&bytes, PayloadFormat::Base64), "aGVsbG8=");
    }

    #[test]
    fn hex_payload_round_trip() {
        let bytes = vec![0x00, 0x1a, 0xff];
        assert_eq!(render_payload(&bytes, PayloadFormat::Hex), "001AFF");
        assert_eq!(decode_hex("001AFF").unwrap(), bytes);
        assert_eq!(decode_hex("00 1A FF").unwrap(), bytes);
    }

    #[test]
    fn hex_rejects_invalid_input() {
        assert!(decode_hex("ABC").is_err());
        assert!(decode_hex("ZZ").is_err());
    }

    #[test]
    fn json_render_falls_back_on_plain_text() {
        assert_eq!(render_payload(b"not json", PayloadFormat::Json), "not json");
    }

    #[test]
    fn format_index_and_key_round_trip() {
        let formats = [
            PayloadFormat::Text,
            PayloadFormat::Json,
            PayloadFormat::Base64,
            PayloadFormat::Hex,
        ];
        for format in formats {
            assert_eq!(PayloadFormat::from_index(format.to_index()), format);
            assert_eq!(PayloadFormat::from_key(format.to_key()), format);
        }
    }

    fn sample_log(kind: &str, topic: &str, payload: &str) -> LogData {
        LogData {
            time: "12:00:00".to_string(),
            timestamp: "2026-09-17 12:00:00".to_string(),
            kind: kind.to_string(),
            topic: topic.to_string(),
            payload: payload.as_bytes().to_vec(),
        }
    }

    #[test]
    fn log_filter_matches_topic_and_payload() {
        let log = sample_log("RECV", "sensors/temp", "hello world");
        assert!(log_matches(&log, PayloadFormat::Text, "temp", 0, ""));
        assert!(log_matches(&log, PayloadFormat::Text, "hello", 0, ""));
        assert!(!log_matches(&log, PayloadFormat::Text, "missing", 0, ""));
        assert!(!log_matches(&log, PayloadFormat::Text, "hello", 2, ""));
        assert!(log_matches(&log, PayloadFormat::Text, "hello", 1, ""));
        assert!(log_matches(&log, PayloadFormat::Text, "", 0, "sensors/temp"));
        assert!(!log_matches(&log, PayloadFormat::Text, "", 0, "other"));
        assert!(log_matches(&log, PayloadFormat::Text, "", 0, "sensors/#"));
        assert!(log_matches(&log, PayloadFormat::Text, "", 0, "sensors/+"));
        assert!(!log_matches(&log, PayloadFormat::Text, "", 0, "home/#"));
    }

    #[test]
    fn kind_filter_selects_send_recv_sys_err() {
        assert!(kind_matches("RECV", 0));
        assert!(kind_matches("RECV", 1));
        assert!(!kind_matches("SEND", 1));
        assert!(kind_matches("SEND", 2));
        assert!(kind_matches("SYS", 3));
        assert!(kind_matches("ERR", 4));
        assert!(!kind_matches("SYS", 4));
    }

    #[test]
    fn timed_interval_rejects_out_of_range() {
        assert_eq!(parse_timed_interval("5").unwrap(), 5);
        assert!(parse_timed_interval("0").is_err());
        assert!(parse_timed_interval("3601").is_err());
        assert!(parse_timed_interval("abc").is_err());
        assert_eq!(parse_u16_clamped("0", 1, 300, 10), 10);
        assert_eq!(parse_u16_clamped("8", 1, 300, 10), 8);
    }

    #[test]
    fn mqtt_wildcard_filter_matches_topic_levels() {
        assert!(topic_matches_filter("a/b/c", "a/b/c"));
        assert!(topic_matches_filter("a/b/c", "a/+/c"));
        assert!(topic_matches_filter("a/b/c", "a/#"));
        assert!(topic_matches_filter("a/b/c", "#"));
        assert!(!topic_matches_filter("a/b/c", "a/+"));
        assert!(!topic_matches_filter("a/b/c", "b/#"));
    }
}
