#![cfg_attr(all(windows, not(debug_assertions)), windows_subsystem = "windows")]

//! Desktop MQTT debug client (Slint UI + rumqttc).
//!
//! Pages: 0 = connection settings, 1 = live session, 2 = app settings, 3 = auto-reply, 4 = publish presets.
//! Slint callbacks stay on the UI thread; each connection's rumqttc eventloop
//! runs on its own worker and posts [`UiEvent`]s back with `invoke_from_event_loop`.

mod config;
mod i18n;
mod mqtt;

use base64::Engine;
use chrono::Local;
use config::{
    ensure_auto_reply_rules, ensure_publish_presets, normalize_auto_reply_echo, AppConfig,
    AutoReplyRule, ConnectionConfig, PublishPreset, SubscriptionConfig,
};
use i18n::Lang;
use mqtt::{qos_from_u8, ConnectParams, MqttHandle, ProtocolVersion, UiEvent};
use slint::{ComponentHandle, Model, VecModel};
use std::cell::RefCell;
use std::collections::HashMap;
use std::path::PathBuf;
use std::rc::Rc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::thread;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

slint::include_modules!();

/// Cap per-connection in-memory logs. Software rendering keeps every row in the scene tree.
const MAX_LOGS: usize = 200;

thread_local! {
    /// UI-thread-only. Callbacks are `FnMut`, not `Send`, so workers never lock this.
    static STATE: RefCell<Option<Rc<RefCell<AppState>>>> = RefCell::new(None);
}

/// Payload encoding for publish and log display. Combo index 0..=3 matches this order.
#[derive(Clone, Copy, Debug, PartialEq)]
enum PayloadFormat {
    Text,
    Json,
    Base64,
    Hex,
}

/// Stored preference. `System` follows the OS; Slint `Theme.skin` uses the visual index only
/// (`from_visual_index`: 0=Black … 4=Purple) because the palette has no "auto" slot.
#[derive(Clone, Copy, Debug, PartialEq)]
enum Skin {
    System,
    Black,
    White,
    Blue,
    Green,
    Purple,
}

impl Skin {
    fn from_index(index: i32) -> Self {
        // Settings combo: 0=System, 1=Black, … 5=Purple.
        match index {
            0 => Self::System,
            1 => Self::Black,
            2 => Self::White,
            3 => Self::Blue,
            4 => Self::Green,
            5 => Self::Purple,
            _ => Self::Black,
        }
    }

    fn from_visual_index(index: i32) -> Self {
        // Theme.skin has no System slot; 0 is always the resolved dark (Black) palette.
        match index {
            1 => Self::White,
            2 => Self::Blue,
            3 => Self::Green,
            4 => Self::Purple,
            _ => Self::Black,
        }
    }

    fn from_key(key: &str) -> Self {
        match key {
            "system" | "auto" | "follow" => Self::System,
            "white" => Self::White,
            "blue" => Self::Blue,
            "green" => Self::Green,
            "purple" => Self::Purple,
            _ => Self::Black,
        }
    }

    fn to_index(self) -> i32 {
        match self {
            Self::System => 0,
            Self::Black => 1,
            Self::White => 2,
            Self::Blue => 3,
            Self::Green => 4,
            Self::Purple => 5,
        }
    }

    fn to_visual_index(self) -> i32 {
        match self.resolve() {
            Self::White => 1,
            Self::Blue => 2,
            Self::Green => 3,
            Self::Purple => 4,
            _ => 0,
        }
    }

    fn to_key(self) -> &'static str {
        match self {
            Self::System => "system",
            Self::Black => "black",
            Self::White => "white",
            Self::Blue => "blue",
            Self::Green => "green",
            Self::Purple => "purple",
        }
    }

    fn resolve(self) -> Self {
        match self {
            Self::System => {
                // Only Black / White follow the OS. Colored skins stay as picked.
                if system_prefers_dark() {
                    Self::Black
                } else {
                    Self::White
                }
            }
            other => other,
        }
    }

    #[allow(dead_code)]
    fn is_light(self) -> bool {
        matches!(self.resolve(), Self::White)
    }

    #[allow(dead_code)]
    fn window_bg_rgb(self) -> (u8, u8, u8) {
        match self.resolve() {
            Self::White => (0xe8, 0xed, 0xf3),
            Self::Blue => (0x07, 0x20, 0x48),
            Self::Green => (0x06, 0x28, 0x20),
            Self::Purple => (0x2a, 0x12, 0x48),
            _ => (0x16, 0x18, 0x1d),
        }
    }

    #[allow(dead_code)]
    fn caption_text_rgb(self) -> (u8, u8, u8) {
        if self.is_light() {
            (0x1c, 0x24, 0x30)
        } else {
            (0xf2, 0xf4, 0xf8)
        }
    }
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
    fn text(self) -> String {
        match self {
            Self::Disconnected => i18n::t("未连接", "Disconnected"),
            Self::Connecting => i18n::t("连接中", "Connecting"),
            Self::Connected => i18n::t("已连接", "Connected"),
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
    /// `RECV` / `SEND` / `SYS` / `ERR`. Payload encoding applies only to RECV/SEND.
    kind: String,
    topic: String,
    payload: Vec<u8>,
}

/// Live MQTT client, logs, and timed-publish worker for one saved connection.
struct ConnRuntime {
    mqtt: MqttHandle,
    logs: Vec<LogData>,
    status: ConnStatus,
    /// Bumped on stop/reconnect so the previous timed-publish thread exits.
    timed_generation: Arc<AtomicU64>,
    timed_running: bool,
    /// Empty = show every topic; otherwise filter RECV/SEND by MQTT wildcard.
    active_sub_topic: String,
    /// Last Fixed Reply used to detect Own Echo. Manual / preset / timed publish leave this alone.
    last_auto_reply: Option<(String, Vec<u8>)>,
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
            last_auto_reply: None,
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

fn echo_policy_from_index(index: i32) -> &'static str {
    if index == 1 {
        "rematch"
    } else {
        "skip"
    }
}

fn echo_policy_to_index(key: &str) -> i32 {
    if normalize_auto_reply_echo(key) == "rematch" {
        1
    } else {
        0
    }
}

/// Run `f` on the UI-thread AppState. Callbacks and `invoke_from_event_loop` both go through here.
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
    // FemtoVG/OpenGL was dropping ~100 MB at startup; software renderer is enough for this UI.
    slint::BackendSelector::new()
        .backend_name("winit-software".into())
        .select()?;
    let ui = AppWindow::new()?;
    apply_platform_fonts(&ui);
    let state = Rc::new(RefCell::new(AppState::new()));
    STATE.with(|slot| *slot.borrow_mut() = Some(state.clone()));

    {
        let mut guard = state.borrow_mut();
        match config::load() {
            Ok(Some(cfg)) => guard.config = cfg,
            Ok(None) => {}
            Err(err) => guard.startup_error = Some(err),
        }
        i18n::set_lang(Lang::from_key(&guard.config.language));
        if guard.config.connections.is_empty() {
            let conn = ConnectionConfig::new(i18n::local_broker_name());
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
                i18n::t(
                    &format!("配置文件 {}", config::config_path().display()),
                    &format!("Config file {}", config::config_path().display()),
                )
                .into_bytes(),
            ),
        }
    }

    ui.show()?;
    schedule_window_chrome(&ui);
    start_system_skin_watch(ui.as_weak());

    let result = ui.run();
    {
        let mut guard = state.borrow_mut();
        sync_form(&ui, &mut guard);
        sync_prefs(&ui, &mut guard);
        persist(&guard);
    }
    result
}

/// Wire Slint callbacks. Each closure upgrades `ui_weak` then `with_state`.
fn register_callbacks(ui: &AppWindow) {
    let ui_weak = ui.as_weak();

    ui.on_new_connection_requested({
        let ui_weak = ui_weak.clone();
        move || {
            let Some(ui) = ui_weak.upgrade() else { return };
            with_state(|st| {
                sync_form(&ui, st);
                let name = i18n::new_connection_name(st.config.connections.len() + 1);
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
                    let conn = ConnectionConfig::new(i18n::local_broker_name());
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
                    push_log(
                        &ui,
                        st,
                        "ERR",
                        "",
                        i18n::t("主机不能为空", "Host is required").into_bytes(),
                    );
                    return;
                }
                let port = match cfg.port.trim().parse::<u16>() {
                    Ok(port) if port > 0 => port,
                    _ => {
                        push_log(
                            &ui,
                            st,
                            "ERR",
                            "",
                            i18n::t("端口无效，范围为 1-65535", "Invalid port, range is 1-65535")
                                .into_bytes(),
                        );
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
                    i18n::t("匿名", "anonymous")
                } else if i18n::is_en() {
                    format!("user {}", cfg.username)
                } else {
                    format!("用户 {}", cfg.username)
                };
                let id = cfg.id.clone();
                st.stop_timed(&id);
                st.runtime_mut(&id).status = ConnStatus::Connecting;
                st.runtime_mut(&id).last_auto_reply = None;
                let will_hint = if cfg.will_topic.trim().is_empty() {
                    String::new()
                } else if i18n::is_en() {
                    format!(", will {}", cfg.will_topic.trim())
                } else {
                    format!("，遗嘱 {}", cfg.will_topic.trim())
                };
                let ssl_hint = if cfg.ssl {
                    if cfg.ssl_verify {
                        i18n::t("，SSL 校验证书", ", SSL verify on")
                    } else {
                        i18n::t("，SSL 不校验证书", ", SSL verify off")
                    }
                } else {
                    String::new()
                };
                let protocol = ProtocolVersion::from_key(&cfg.protocol);
                push_log(
                    &ui,
                    st,
                    "SYS",
                    "",
                    i18n::t(
                        &format!(
                            "正在连接 {}:{} (MQTT {}，{}，{}，Keep Alive {}s，超时 {}s{}{})",
                            cfg.host,
                            cfg.port,
                            protocol.label(),
                            cfg.client_id,
                            auth_hint,
                            cfg.keep_alive,
                            cfg.connect_timeout,
                            ssl_hint,
                            will_hint
                        ),
                        &format!(
                            "Connecting {}:{} (MQTT {}, {}, {}, Keep Alive {}s, timeout {}s{}{})",
                            cfg.host,
                            cfg.port,
                            protocol.label(),
                            cfg.client_id,
                            auth_hint,
                            cfg.keep_alive,
                            cfg.connect_timeout,
                            ssl_hint,
                            will_hint
                        ),
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
                    protocol,
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
                st.runtime_mut(&id).last_auto_reply = None;
                ui.set_timed_running(false);
                push_log(
                    &ui,
                    st,
                    "SYS",
                    "",
                    i18n::t("已断开连接", "Disconnected").into_bytes(),
                );
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
                    push_log(
                        &ui,
                        st,
                        "ERR",
                        "",
                        i18n::t("请先连接 Broker", "Connect to a broker first").into_bytes(),
                    );
                    return;
                }
                let topic = ui.get_sub_topic().trim().to_string();
                if topic.is_empty() {
                    push_log(
                        &ui,
                        st,
                        "ERR",
                        "",
                        i18n::t("Topic 为空", "Topic is empty").into_bytes(),
                    );
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
                            push_log(
                                &ui,
                                st,
                                "SYS",
                                &topic,
                                i18n::t("已在订阅列表中", "Already in subscription list").into_bytes(),
                            );
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
                                i18n::t(
                                    &format!("已订阅 QoS {qos}"),
                                    &format!("Subscribed QoS {qos}"),
                                )
                                .into_bytes(),
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
                    push_log(
                        &ui,
                        st,
                        "SYS",
                        &sub.topic,
                        i18n::t("已取消订阅", "Unsubscribed").into_bytes(),
                    );
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

    ui.on_preset_publish_requested({
        let ui_weak = ui_weak.clone();
        move |index| {
            let Some(ui) = ui_weak.upgrade() else { return };
            with_state(|st| {
                // Persist preset edits first. Success must not overwrite the bottom form.
                sync_form(&ui, st);
                let _ = publish_from_preset(&ui, st, index);
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
                    push_log(
                        &ui,
                        st,
                        "SYS",
                        "",
                        i18n::t("已停止定时发布", "Timed publish stopped").into_bytes(),
                    );
                    persist(st);
                    return;
                }
                if !st.is_connected(&id) {
                    push_log(
                        &ui,
                        st,
                        "ERR",
                        "",
                        i18n::t("请先连接 Broker", "Connect to a broker first").into_bytes(),
                    );
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
                // First tick uses the bottom form, not a preset.
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
                    i18n::t(
                        &format!("已启动定时发布，每 {interval} 秒"),
                        &format!("Timed publish started every {interval}s"),
                    )
                    .into_bytes(),
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

    ui.on_skin_changed({
        let ui_weak = ui_weak.clone();
        move || {
            let Some(ui) = ui_weak.upgrade() else { return };
            apply_resolved_skin(&ui);
            with_state(|st| {
                sync_prefs(&ui, st);
                persist(st);
            });
            apply_window_chrome(&ui);
        }
    });

    ui.on_language_changed({
        let ui_weak = ui_weak.clone();
        move || {
            let Some(ui) = ui_weak.upgrade() else { return };
            with_state(|st| {
                sync_prefs(&ui, st);
                refresh_connections(&ui, st);
                refresh_status(&ui, st);
                persist(st);
            });
        }
    });

    ui.on_settings_requested({
        let ui_weak = ui_weak.clone();
        move || {
            let Some(ui) = ui_weak.upgrade() else { return };
            with_state(|st| {
                sync_form(&ui, st);
                persist(st);
                show_page(&ui, 2);
            });
        }
    });

    ui.on_auto_reply_requested({
        let ui_weak = ui_weak.clone();
        move || {
            let Some(ui) = ui_weak.upgrade() else { return };
            with_state(|st| {
                sync_form(&ui, st);
                persist(st);
                show_page(&ui, 3);
            });
        }
    });

    ui.on_publish_presets_requested({
        let ui_weak = ui_weak.clone();
        move || {
            let Some(ui) = ui_weak.upgrade() else { return };
            with_state(|st| {
                sync_form(&ui, st);
                persist(st);
                show_page(&ui, 4);
            });
        }
    });

    ui.on_add_publish_preset_requested({
        let ui_weak = ui_weak.clone();
        move || {
            let Some(ui) = ui_weak.upgrade() else { return };
            with_state(|st| {
                sync_form(&ui, st);
                let Some(idx) = st.active_index() else { return };
                st.config.connections[idx]
                    .publish_presets
                    .push(PublishPreset::default());
                refresh_publish_presets(&ui, &st.config.connections[idx]);
                persist(st);
            });
        }
    });

    ui.on_delete_publish_preset_requested({
        let ui_weak = ui_weak.clone();
        move |index| {
            let Some(ui) = ui_weak.upgrade() else { return };
            with_state(|st| {
                sync_form(&ui, st);
                let Some(idx) = st.active_index() else { return };
                let pos = index.max(0) as usize;
                if pos >= st.config.connections[idx].publish_presets.len() {
                    return;
                }
                st.config.connections[idx].publish_presets.remove(pos);
                refresh_publish_presets(&ui, &st.config.connections[idx]);
                persist(st);
            });
        }
    });

    ui.on_add_auto_reply_requested({
        let ui_weak = ui_weak.clone();
        move || {
            let Some(ui) = ui_weak.upgrade() else { return };
            with_state(|st| {
                sync_form(&ui, st);
                let Some(idx) = st.active_index() else { return };
                st.config.connections[idx]
                    .auto_reply_rules
                    .push(AutoReplyRule::default());
                refresh_auto_reply(&ui, &st.config.connections[idx]);
                persist(st);
            });
        }
    });

    ui.on_delete_auto_reply_requested({
        let ui_weak = ui_weak.clone();
        move |index| {
            let Some(ui) = ui_weak.upgrade() else { return };
            with_state(|st| {
                sync_form(&ui, st);
                let Some(idx) = st.active_index() else { return };
                let pos = index.max(0) as usize;
                if pos >= st.config.connections[idx].auto_reply_rules.len() {
                    return;
                }
                st.config.connections[idx].auto_reply_rules.remove(pos);
                refresh_auto_reply(&ui, &st.config.connections[idx]);
                persist(st);
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
                    push_log(
                        &ui,
                        st,
                        "ERR",
                        "",
                        i18n::t("当前没有可保存的日志", "No logs to save").into_bytes(),
                    );
                    return;
                }
                match save_logs_to_file(&logs, format) {
                    Ok(path) => push_log(
                        &ui,
                        st,
                        "SYS",
                        "",
                        i18n::t(
                            &format!("日志已保存到 {}", path.display()),
                            &format!("Logs saved to {}", path.display()),
                        )
                        .into_bytes(),
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

/// MQTT worker events arrive here on the UI thread, keyed by connection id.
fn handle_event(ui: &AppWindow, st: &mut AppState, conn_id: &str, event: UiEvent) {
    match event {
        UiEvent::Connected => {
            st.runtime_mut(conn_id).status = ConnStatus::Connected;
            push_log_for(
                ui,
                st,
                conn_id,
                "SYS",
                "",
                i18n::t("连接成功", "Connected").into_bytes(),
            );
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
            st.runtime_mut(conn_id).last_auto_reply = None;
            if previous != ConnStatus::Disconnected && !reason.is_empty() {
                push_log_for(
                    ui,
                    st,
                    conn_id,
                    "SYS",
                    "",
                    i18n::t(
                        &format!("连接结束: {reason}"),
                        &format!("Connection ended: {reason}"),
                    )
                    .into_bytes(),
                );
            }
            if was_timed {
                push_log_for(
                    ui,
                    st,
                    conn_id,
                    "SYS",
                    "",
                    i18n::t(
                        "连接断开，已停止定时发布",
                        "Disconnected, timed publish stopped",
                    )
                    .into_bytes(),
                );
            }
        }
        UiEvent::Recv { topic, payload } => {
            push_log_for(ui, st, conn_id, "RECV", &topic, payload.clone());
            maybe_auto_reply(ui, st, conn_id, &topic, &payload);
        }
        UiEvent::Error { message } => {
            push_log_for(ui, st, conn_id, "ERR", "", message.into_bytes());
        }
    }
    refresh_status(ui, st);
    refresh_connections(ui, st);
}

/// rumqttc does not restore subscriptions after a new CONNECT; replay the saved list.
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
                i18n::t("已重新订阅", "Resubscribed").into_bytes(),
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

fn apply_platform_fonts(ui: &AppWindow) {
    let (ui_font, mono_font) = platform_fonts();
    ui.set_ui_font(ui_font.into());
    ui.set_mono_font(mono_font.into());
}

fn platform_fonts() -> (&'static str, &'static str) {
    if cfg!(target_os = "macos") {
        ("PingFang SC", "Menlo")
    } else {
        ("Microsoft YaHei UI", "Cascadia Mono")
    }
}

fn apply_prefs(ui: &AppWindow, st: &AppState) {
    ui.set_log_format(PayloadFormat::from_key(&st.config.log_format).to_index());
    ui.set_auto_scroll(st.config.auto_scroll);
    ui.set_log_filter(st.config.log_filter.clone().into());
    ui.set_log_kind_filter(st.config.log_kind_filter.clamp(0, 4));
    let skin = Skin::from_key(&st.config.skin);
    ui.set_skin_choice(skin.to_index());
    ui.set_skin(skin.to_visual_index());
    ui.set_language(Lang::from_key(&st.config.language).to_index());
    show_page(ui, 1);
}

fn schedule_window_chrome(ui: &AppWindow) {
    apply_window_chrome(ui);
    // Win32 HWND is not always ready on first show(); retry until DWM attributes stick.
    for delay_ms in [0_u64, 16, 50, 150, 400, 1000] {
        let ui_weak = ui.as_weak();
        slint::Timer::single_shot(Duration::from_millis(delay_ms), move || {
            if let Some(ui) = ui_weak.upgrade() {
                apply_window_chrome(&ui);
            }
        });
    }
}

fn apply_resolved_skin(ui: &AppWindow) {
    let visual = Skin::from_index(ui.get_skin_choice()).to_visual_index();
    if ui.get_skin() != visual {
        ui.set_skin(visual);
    }
}

/// Poll OS light/dark once a second. Only applies when the saved skin is System.
fn start_system_skin_watch(ui_weak: slint::Weak<AppWindow>) {
    thread::spawn(move || {
        let mut last_dark: Option<bool> = None;
        loop {
            let dark = system_prefers_dark();
            if last_dark != Some(dark) {
                last_dark = Some(dark);
                let ui_weak = ui_weak.clone();
                let _ = slint::invoke_from_event_loop(move || {
                    let Some(ui) = ui_weak.upgrade() else {
                        return;
                    };
                    if Skin::from_index(ui.get_skin_choice()) != Skin::System {
                        return;
                    }
                    apply_resolved_skin(&ui);
                    apply_window_chrome(&ui);
                });
            }
            thread::sleep(Duration::from_secs(1));
        }
    });
}

fn system_prefers_dark() -> bool {
    #[cfg(windows)]
    {
        windows_apps_use_dark().unwrap_or(true)
    }
    #[cfg(target_os = "macos")]
    {
        macos_interface_dark()
    }
    #[cfg(not(any(windows, target_os = "macos")))]
    {
        linux_prefers_dark()
    }
}

#[cfg(windows)]
fn windows_apps_use_dark() -> Option<bool> {
    use std::ffi::c_void;

    type Hkey = *mut c_void;
    const HKEY_CURRENT_USER: Hkey = 0x8000_0001u32 as usize as Hkey;
    const KEY_READ: u32 = 0x20019;
    const ERROR_SUCCESS: i32 = 0;

    #[link(name = "advapi32")]
    extern "system" {
        fn RegOpenKeyExW(
            hKey: Hkey,
            lpSubKey: *const u16,
            ulOptions: u32,
            samDesired: u32,
            phkResult: *mut Hkey,
        ) -> i32;
        fn RegQueryValueExW(
            hKey: Hkey,
            lpValueName: *const u16,
            lpReserved: *mut u32,
            lpType: *mut u32,
            lpData: *mut u8,
            lpcbData: *mut u32,
        ) -> i32;
        fn RegCloseKey(hKey: Hkey) -> i32;
    }

    let subkey: Vec<u16> = "Software\\Microsoft\\Windows\\CurrentVersion\\Themes\\Personalize\0"
        .encode_utf16()
        .collect();
    let value_name: Vec<u16> = "AppsUseLightTheme\0".encode_utf16().collect();
    let mut hkey = std::ptr::null_mut();
    let status = unsafe { RegOpenKeyExW(HKEY_CURRENT_USER, subkey.as_ptr(), 0, KEY_READ, &mut hkey) };
    if status != ERROR_SUCCESS {
        return None;
    }
    let mut data: u32 = 1;
    let mut data_size = 4u32;
    let mut value_type = 0u32;
    let status = unsafe {
        RegQueryValueExW(
            hkey,
            value_name.as_ptr(),
            std::ptr::null_mut(),
            &mut value_type,
            &mut data as *mut u32 as *mut u8,
            &mut data_size,
        )
    };
    unsafe {
        let _ = RegCloseKey(hkey);
    }
    if status != ERROR_SUCCESS {
        return None;
    }
    Some(data == 0)
}

#[cfg(target_os = "macos")]
fn macos_interface_dark() -> bool {
    std::process::Command::new("defaults")
        .args(["read", "-g", "AppleInterfaceStyle"])
        .output()
        .ok()
        .filter(|out| out.status.success())
        .map(|out| {
            String::from_utf8_lossy(&out.stdout)
                .to_ascii_lowercase()
                .contains("dark")
        })
        .unwrap_or(false)
}

#[cfg(not(any(windows, target_os = "macos")))]
fn linux_prefers_dark() -> bool {
    if let Ok(out) = std::process::Command::new("gsettings")
        .args(["get", "org.gnome.desktop.interface", "color-scheme"])
        .output()
    {
        let value = String::from_utf8_lossy(&out.stdout).to_ascii_lowercase();
        if value.contains("prefer-dark") {
            return true;
        }
        if value.contains("prefer-light") {
            return false;
        }
    }
    if let Ok(out) = std::process::Command::new("gsettings")
        .args(["get", "org.gnome.desktop.interface", "gtk-theme"])
        .output()
    {
        if String::from_utf8_lossy(&out.stdout)
            .to_ascii_lowercase()
            .contains("dark")
        {
            return true;
        }
    }
    for command in [
        ["kreadconfig6", "--file", "kdeglobals", "--group", "General", "--key", "ColorScheme"],
        ["kreadconfig5", "--file", "kdeglobals", "--group", "General", "--key", "ColorScheme"],
    ] {
        if let Ok(out) = std::process::Command::new(command[0])
            .args(&command[1..])
            .output()
        {
            if String::from_utf8_lossy(&out.stdout)
                .to_ascii_lowercase()
                .contains("dark")
            {
                return true;
            }
        }
    }
    if std::env::var("GTK_THEME")
        .map(|theme| theme.to_ascii_lowercase().contains("dark"))
        .unwrap_or(false)
    {
        return true;
    }
    true
}

fn apply_window_chrome(ui: &AppWindow) {
    let skin = Skin::from_visual_index(ui.get_skin());
    apply_native_titlebar(ui, skin);
}

#[cfg(windows)]
fn apply_native_titlebar(ui: &AppWindow, skin: Skin) {
    use raw_window_handle::{HasWindowHandle, RawWindowHandle};
    use std::ffi::c_void;

    const DWMWA_USE_IMMERSIVE_DARK_MODE_OLD: u32 = 19;
    const DWMWA_USE_IMMERSIVE_DARK_MODE: u32 = 20;
    const DWMWA_CAPTION_COLOR: u32 = 35;
    const DWMWA_TEXT_COLOR: u32 = 36;
    const SWP_NOSIZE: u32 = 0x0001;
    const SWP_NOMOVE: u32 = 0x0002;
    const SWP_NOZORDER: u32 = 0x0004;
    const SWP_FRAMECHANGED: u32 = 0x0020;

    let window_handle = ui.window().window_handle();
    let Ok(handle) = window_handle.window_handle() else {
        return;
    };
    let RawWindowHandle::Win32(win) = handle.as_raw() else {
        return;
    };
    let hwnd = win.hwnd.get() as *mut c_void;
    let dark: i32 = if skin.is_light() { 0 } else { 1 };
    let caption = rgb_to_colorref(skin.window_bg_rgb());
    let text = rgb_to_colorref(skin.caption_text_rgb());
    unsafe {
        dwm_set_window_attribute(
            hwnd,
            DWMWA_USE_IMMERSIVE_DARK_MODE_OLD,
            &dark as *const i32 as *const c_void,
            std::mem::size_of::<i32>() as u32,
        );
        dwm_set_window_attribute(
            hwnd,
            DWMWA_USE_IMMERSIVE_DARK_MODE,
            &dark as *const i32 as *const c_void,
            std::mem::size_of::<i32>() as u32,
        );
        dwm_set_window_attribute(
            hwnd,
            DWMWA_CAPTION_COLOR,
            &caption as *const u32 as *const c_void,
            std::mem::size_of::<u32>() as u32,
        );
        dwm_set_window_attribute(
            hwnd,
            DWMWA_TEXT_COLOR,
            &text as *const u32 as *const c_void,
            std::mem::size_of::<u32>() as u32,
        );
        set_window_pos(
            hwnd,
            std::ptr::null_mut(),
            0,
            0,
            0,
            0,
            SWP_NOSIZE | SWP_NOMOVE | SWP_NOZORDER | SWP_FRAMECHANGED,
        );
    }
}

#[cfg(not(windows))]
fn apply_native_titlebar(_ui: &AppWindow, _skin: Skin) {}

#[allow(dead_code)]
fn rgb_to_colorref((r, g, b): (u8, u8, u8)) -> u32 {
    u32::from(r) | (u32::from(g) << 8) | (u32::from(b) << 16)
}

#[cfg(windows)]
unsafe fn dwm_set_window_attribute(
    hwnd: *mut std::ffi::c_void,
    attribute: u32,
    value: *const std::ffi::c_void,
    size: u32,
) {
    #[link(name = "dwmapi")]
    extern "system" {
        fn DwmSetWindowAttribute(
            hwnd: *mut std::ffi::c_void,
            dwAttribute: u32,
            pvAttribute: *const std::ffi::c_void,
            cbAttribute: u32,
        ) -> i32;
    }
    let _ = DwmSetWindowAttribute(hwnd, attribute, value, size);
}

#[cfg(windows)]
unsafe fn set_window_pos(
    hwnd: *mut std::ffi::c_void,
    hwnd_insert_after: *mut std::ffi::c_void,
    x: i32,
    y: i32,
    cx: i32,
    cy: i32,
    flags: u32,
) {
    #[link(name = "user32")]
    extern "system" {
        fn SetWindowPos(
            hwnd: *mut std::ffi::c_void,
            hWndInsertAfter: *mut std::ffi::c_void,
            X: i32,
            Y: i32,
            cx: i32,
            cy: i32,
            uFlags: u32,
        ) -> i32;
    }
    let _ = SetWindowPos(hwnd, hwnd_insert_after, x, y, cx, cy, flags);
}

/// 0 = connection editor, 1 = session, 2 = settings, 3 = auto-reply, 4 = publish presets.
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
    ui.set_mqtt_protocol(ProtocolVersion::from_key(&cfg.protocol).to_index());
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
    refresh_publish_presets(ui, cfg);
    refresh_auto_reply(ui, cfg);
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
        .map(|runtime| {
            runtime
                .logs
                .iter()
                .filter(|log| log_matches(log, format, &filter, kind_filter, &active_topic))
                .map(|log| render_log(log, format))
                .collect()
        })
        .unwrap_or_default();
    ui.set_logs(Rc::new(VecModel::from(entries)).into());
}

/// Copy the visible form (including preset rows) into the active connection before connect / switch / persist.
fn sync_form(ui: &AppWindow, st: &mut AppState) {
    let Some(idx) = st.active_index() else { return };
    let cfg = &mut st.config.connections[idx];
    cfg.name = ui.get_connection_name().to_string();
    cfg.host = ui.get_host().to_string();
    cfg.port = ui.get_port().to_string();
    cfg.client_id = ui.get_client_id().to_string();
    cfg.username = ui.get_username().to_string();
    cfg.password = ui.get_password().to_string();
    cfg.protocol = ProtocolVersion::from_index(ui.get_mqtt_protocol())
        .to_key()
        .to_string();
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
    sync_publish_presets(ui, cfg);
    sync_auto_reply(ui, cfg);
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
    st.config.skin = Skin::from_index(ui.get_skin_choice()).to_key().to_string();
    st.config.language = Lang::from_index(ui.get_language()).to_key().to_string();
    i18n::set_lang(Lang::from_index(ui.get_language()));
}

fn refresh_publish_presets(ui: &AppWindow, cfg: &ConnectionConfig) {
    let rows: Vec<PublishPresetRow> = cfg
        .publish_presets
        .iter()
        .map(|preset| PublishPresetRow {
            name: preset.name.clone().into(),
            topic: preset.topic.clone().into(),
            payload: preset.payload.clone().into(),
            qos: preset.qos.min(2) as i32,
            retain: preset.retain,
            format: PayloadFormat::from_key(&preset.format).to_index(),
        })
        .collect();
    ui.set_publish_presets(Rc::new(VecModel::from(rows)).into());
}

fn sync_publish_presets(ui: &AppWindow, cfg: &mut ConnectionConfig) {
    let model = ui.get_publish_presets();
    let mut presets = Vec::with_capacity(model.row_count());
    for i in 0..model.row_count() {
        let Some(row) = model.row_data(i) else {
            continue;
        };
        presets.push(PublishPreset {
            name: row.name.to_string(),
            topic: row.topic.to_string(),
            payload: row.payload.to_string(),
            qos: row.qos.clamp(0, 2) as u8,
            retain: row.retain,
            format: PayloadFormat::from_index(row.format).to_key().to_string(),
        });
    }
    cfg.publish_presets = presets;
    ensure_publish_presets(&mut cfg.publish_presets);
}

fn refresh_auto_reply(ui: &AppWindow, cfg: &ConnectionConfig) {
    let rows: Vec<AutoReplyRuleRow> = cfg
        .auto_reply_rules
        .iter()
        .map(|rule| AutoReplyRuleRow {
            enabled: rule.enabled,
            match_topic: rule.match_topic.clone().into(),
            payload_contains: rule.payload_contains.clone().into(),
            reply_topic: rule.reply_topic.clone().into(),
            reply_payload: rule.reply_payload.clone().into(),
            qos: rule.qos.min(2) as i32,
            retain: rule.retain,
            format: PayloadFormat::from_key(&rule.format).to_index(),
        })
        .collect();
    ui.set_auto_reply_rules(Rc::new(VecModel::from(rows)).into());
    ui.set_auto_reply_echo(echo_policy_to_index(&cfg.auto_reply_echo));
}

fn sync_auto_reply(ui: &AppWindow, cfg: &mut ConnectionConfig) {
    let model = ui.get_auto_reply_rules();
    let mut rules = Vec::with_capacity(model.row_count());
    for i in 0..model.row_count() {
        let Some(row) = model.row_data(i) else {
            continue;
        };
        rules.push(AutoReplyRule {
            enabled: row.enabled,
            match_topic: row.match_topic.to_string(),
            payload_contains: row.payload_contains.to_string(),
            reply_topic: row.reply_topic.to_string(),
            reply_payload: row.reply_payload.to_string(),
            qos: row.qos.clamp(0, 2) as u8,
            retain: row.retain,
            format: PayloadFormat::from_index(row.format).to_key().to_string(),
        });
    }
    cfg.auto_reply_rules = rules;
    ensure_auto_reply_rules(&mut cfg.auto_reply_rules);
    cfg.auto_reply_echo = echo_policy_from_index(ui.get_auto_reply_echo()).to_string();
}

fn inbound_payload_text(payload: &[u8]) -> String {
    String::from_utf8_lossy(payload).into_owned()
}

fn rule_matches(rule: &AutoReplyRule, topic: &str, payload_text: &str) -> bool {
    if !rule.enabled {
        return false;
    }
    let match_topic = rule.match_topic.trim();
    if match_topic.is_empty() {
        return false;
    }
    if !topic_matches_filter(topic, match_topic) {
        return false;
    }
    let needle = rule.payload_contains.as_str();
    needle.is_empty() || payload_text.contains(needle)
}

fn select_auto_reply_rule(rules: &[AutoReplyRule], topic: &str, payload_text: &str) -> Option<usize> {
    rules
        .iter()
        .enumerate()
        .find(|(_, rule)| rule_matches(rule, topic, payload_text))
        .map(|(i, _)| i)
}

fn is_own_echo(runtime: &ConnRuntime, topic: &str, payload: &[u8]) -> bool {
    runtime
        .last_auto_reply
        .as_ref()
        .map(|(echo_topic, echo_payload)| echo_topic == topic && echo_payload.as_slice() == payload)
        .unwrap_or(false)
}

fn maybe_auto_reply(
    ui: &AppWindow,
    st: &mut AppState,
    conn_id: &str,
    topic: &str,
    payload: &[u8],
) {
    if !st.is_connected(conn_id) {
        return;
    }
    if ui.get_page() == 3 && st.config.active_id == conn_id {
        sync_form(ui, st);
    }
    let Some(idx) = st
        .config
        .connections
        .iter()
        .position(|conn| conn.id == conn_id)
    else {
        return;
    };
    let echo_skip = normalize_auto_reply_echo(&st.config.connections[idx].auto_reply_echo) == "skip";
    if echo_skip {
        let runtime = st.runtime_mut(conn_id);
        if is_own_echo(runtime, topic, payload) {
            return;
        }
    }
    let payload_text = inbound_payload_text(payload);
    let Some(rule_idx) = select_auto_reply_rule(
        &st.config.connections[idx].auto_reply_rules,
        topic,
        &payload_text,
    ) else {
        return;
    };
    let rule = st.config.connections[idx].auto_reply_rules[rule_idx].clone();
    let reply_topic = rule.reply_topic.trim().to_string();
    if reply_topic.is_empty() {
        push_log_for(
            ui,
            st,
            conn_id,
            "ERR",
            "",
            i18n::t("回复 Topic 为空", "Reply topic is empty").into_bytes(),
        );
        return;
    }
    let format = PayloadFormat::from_key(&rule.format);
    let reply_bytes = match encode_payload(&rule.reply_payload, format) {
        Ok(bytes) => bytes,
        Err(err) => {
            push_log_for(ui, st, conn_id, "ERR", &reply_topic, err.into_bytes());
            return;
        }
    };
    let mqtt = st.runtime_mut(conn_id).mqtt.clone();
    match mqtt.publish(
        &reply_topic,
        qos_from_u8(rule.qos.min(2)),
        rule.retain,
        reply_bytes.clone(),
    ) {
        Ok(()) => {
            st.runtime_mut(conn_id).last_auto_reply = Some((reply_topic.clone(), reply_bytes.clone()));
            push_log_for(ui, st, conn_id, "SEND", &reply_topic, reply_bytes);
        }
        Err(err) => {
            push_log_for(ui, st, conn_id, "ERR", &reply_topic, err.into_bytes());
        }
    }
}

/// Bottom "Publish" button: read topic/payload/qos/retain/format from the form.
fn publish_from_form(ui: &AppWindow, st: &mut AppState, persist_on_ok: bool) -> Result<(), ()> {
    let Some(id) = st.active_id() else {
        return Err(());
    };
    publish_from_connection_inner(ui, st, &id, persist_on_ok, true)
}

/// Preset "Send": uses that row only. Does not copy into `pub_topic` / `pub_payload`.
fn publish_from_preset(ui: &AppWindow, st: &mut AppState, index: i32) -> Result<(), ()> {
    let Some(id) = st.active_id() else {
        return Err(());
    };
    if !st.is_connected(&id) {
        push_log_for(
            ui,
            st,
            &id,
            "ERR",
            "",
            i18n::t("请先连接 Broker", "Connect to a broker first").into_bytes(),
        );
        return Err(());
    }
    let Some(conn_idx) = st
        .config
        .connections
        .iter()
        .position(|conn| conn.id == id)
    else {
        return Err(());
    };
    let Some(preset) = st.config.connections[conn_idx]
        .publish_presets
        .get(index as usize)
        .cloned()
    else {
        return Err(());
    };
    let topic = preset.topic.trim().to_string();
    if topic.is_empty() {
        push_log_for(
            ui,
            st,
            &id,
            "ERR",
            "",
            i18n::t("Topic 为空", "Topic is empty").into_bytes(),
        );
        return Err(());
    }
    let format = PayloadFormat::from_key(&preset.format);
    let payload = match encode_payload(&preset.payload, format) {
        Ok(bytes) => bytes,
        Err(err) => {
            push_log_for(ui, st, &id, "ERR", &topic, err.into_bytes());
            return Err(());
        }
    };
    let mqtt = st.runtime_mut(&id).mqtt.clone();
    match mqtt.publish(
        &topic,
        qos_from_u8(preset.qos.min(2)),
        preset.retain,
        payload.clone(),
    ) {
        Ok(()) => {
            push_log_for(ui, st, &id, "SEND", &topic, payload);
            persist(st);
            Ok(())
        }
        Err(err) => {
            push_log_for(ui, st, &id, "ERR", &topic, err.into_bytes());
            Err(())
        }
    }
}

/// Timed publish: read the saved connection fields, even if another connection is on screen.
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
        push_log_for(
            ui,
            st,
            conn_id,
            "ERR",
            "",
            i18n::t("请先连接 Broker", "Connect to a broker first").into_bytes(),
        );
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
        push_log_for(
            ui,
            st,
            conn_id,
            "ERR",
            "",
            i18n::t("Topic 为空", "Topic is empty").into_bytes(),
        );
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

/// MQTT-style `+` / `#` match used when a subscription row is selected as the log filter.
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
        .map_err(|_| {
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

fn persist(st: &AppState) {
    if let Err(err) = config::save(&st.config) {
        eprintln!(
            "{}",
            i18n::t(
                &format!("保存配置失败: {err}"),
                &format!("Failed to save config: {err}")
            )
        );
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

/// JSON is validated then sent as the original bytes (pretty-print is a separate button).
fn encode_payload(text: &str, format: PayloadFormat) -> Result<Vec<u8>, String> {
    match format {
        PayloadFormat::Text => Ok(text.as_bytes().to_vec()),
        PayloadFormat::Json => {
            let trimmed = text.trim();
            if trimmed.is_empty() {
                return Ok(Vec::new());
            }
            serde_json::from_str::<serde_json::Value>(trimmed).map_err(|err| {
                i18n::t(
                    &format!("JSON 格式无效: {err}"),
                    &format!("Invalid JSON: {err}"),
                )
            })?;
            Ok(text.as_bytes().to_vec())
        }
        PayloadFormat::Base64 => {
            let cleaned: String = text.chars().filter(|c| !c.is_whitespace()).collect();
            base64::engine::general_purpose::STANDARD
                .decode(cleaned)
                .map_err(|err| {
                    i18n::t(
                        &format!("Base64 解码失败: {err}"),
                        &format!("Base64 decode failed: {err}"),
                    )
                })
        }
        PayloadFormat::Hex => decode_hex(text),
    }
}

fn beautify_json(text: &str) -> Result<String, String> {
    let value: serde_json::Value =
        serde_json::from_str(text.trim()).map_err(|err| {
            i18n::t(
                &format!("JSON 格式无效: {err}"),
                &format!("Invalid JSON: {err}"),
            )
        })?;
    serde_json::to_string_pretty(&value).map_err(|err| {
        i18n::t(
            &format!("JSON 格式化失败: {err}"),
            &format!("JSON format failed: {err}"),
        )
    })
}

fn encode_hex(bytes: &[u8]) -> String {
    let mut out = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        out.push_str(&format!("{byte:02X}"));
    }
    out
}

fn decode_hex(text: &str) -> Result<Vec<u8>, String> {
    // Accept `00 1A FF`, `00:1A:FF`, and `00-1A-FF` as the same payload.
    let cleaned: String = text
        .chars()
        .filter(|c| !c.is_whitespace() && *c != ',' && *c != ':' && *c != '-')
        .collect();
    if cleaned.is_empty() {
        return Ok(Vec::new());
    }
    let chars: Vec<char> = cleaned.chars().collect();
    if chars.len() % 2 != 0 {
        return Err(i18n::t("Hex 长度必须为偶数", "Hex length must be even"));
    }
    let mut out = Vec::with_capacity(chars.len() / 2);
    for pair in chars.chunks(2) {
        let token: String = pair.iter().collect();
        let byte = u8::from_str_radix(&token, 16).map_err(|_| {
            i18n::t(
                &format!("Hex 字符无效: {token}"),
                &format!("Invalid hex character: {token}"),
            )
        })?;
        out.push(byte);
    }
    Ok(out)
}

fn save_logs_to_file(logs: &[LogData], format: PayloadFormat) -> Result<PathBuf, String> {
    let dir = config::log_dir();
    std::fs::create_dir_all(&dir).map_err(|err| {
        i18n::t(
            &format!("创建日志目录失败: {err}"),
            &format!("Failed to create log directory: {err}"),
        )
    })?;
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

    std::fs::write(&path, content).map_err(|err| {
        i18n::t(
            &format!("写入日志文件失败: {err}"),
            &format!("Failed to write log file: {err}"),
        )
    })?;
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

    #[test]
    fn skin_index_and_key_round_trip() {
        let skins = [
            Skin::System,
            Skin::Black,
            Skin::White,
            Skin::Blue,
            Skin::Green,
            Skin::Purple,
        ];
        for skin in skins {
            assert_eq!(Skin::from_index(skin.to_index()), skin);
            assert_eq!(Skin::from_key(skin.to_key()), skin);
        }
        assert_eq!(Skin::from_key("unknown"), Skin::Black);
        assert_eq!(Skin::from_key("auto"), Skin::System);
        assert_eq!(Skin::from_index(99), Skin::Black);
        assert_eq!(Skin::from_index(0), Skin::System);
        assert_eq!(Skin::Black.to_visual_index(), 0);
        assert_eq!(Skin::White.to_visual_index(), 1);
        assert!(Skin::White.is_light());
        assert!(!Skin::Black.is_light());
        assert!(matches!(Skin::System.resolve(), Skin::Black | Skin::White));
        assert_eq!(Skin::Black.window_bg_rgb(), (0x16, 0x18, 0x1d));
        assert_eq!(Skin::White.window_bg_rgb(), (0xe8, 0xed, 0xf3));
        assert_eq!(Skin::Blue.window_bg_rgb(), (0x07, 0x20, 0x48));
        assert_eq!(Skin::Green.window_bg_rgb(), (0x06, 0x28, 0x20));
        assert_eq!(Skin::Purple.window_bg_rgb(), (0x2a, 0x12, 0x48));
        assert_eq!(Skin::White.caption_text_rgb(), (0x1c, 0x24, 0x30));
        assert_eq!(Skin::Black.caption_text_rgb(), (0xf2, 0xf4, 0xf8));
        assert_eq!(rgb_to_colorref((0x16, 0x18, 0x1d)), 0x001d1816);
        assert_eq!(rgb_to_colorref((0xe8, 0xed, 0xf3)), 0x00f3ede8);
    }

    #[test]
    fn protocol_index_and_key_round_trip() {
        assert_eq!(ProtocolVersion::from_index(0), ProtocolVersion::V311);
        assert_eq!(ProtocolVersion::from_index(1), ProtocolVersion::V5);
        assert_eq!(ProtocolVersion::from_key("3.1.1"), ProtocolVersion::V311);
        assert_eq!(ProtocolVersion::from_key("5.0"), ProtocolVersion::V5);
        assert_eq!(ProtocolVersion::from_key("mqtt5"), ProtocolVersion::V5);
        assert_eq!(ProtocolVersion::from_key("unknown"), ProtocolVersion::V311);
        assert_eq!(ProtocolVersion::V311.to_key(), "3.1.1");
        assert_eq!(ProtocolVersion::V5.to_key(), "5.0");
        assert_eq!(ProtocolVersion::V5.to_index(), 1);
    }

    #[test]
    fn platform_fonts_match_host_os() {
        let (ui, mono) = platform_fonts();
        if cfg!(target_os = "macos") {
            assert_eq!(ui, "PingFang SC");
            assert_eq!(mono, "Menlo");
        } else {
            assert_eq!(ui, "Microsoft YaHei UI");
            assert_eq!(mono, "Cascadia Mono");
        }
    }

    #[test]
    fn language_index_and_key_round_trip() {
        assert_eq!(Lang::from_index(Lang::Zh.to_index()), Lang::Zh);
        assert_eq!(Lang::from_index(Lang::En.to_index()), Lang::En);
        assert_eq!(Lang::from_key(Lang::Zh.to_key()), Lang::Zh);
        assert_eq!(Lang::from_key(Lang::En.to_key()), Lang::En);
        assert_eq!(Lang::from_key("unknown"), Lang::Zh);
        assert_eq!(Lang::from_index(99), Lang::Zh);
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

    fn sample_rule(enabled: bool, match_topic: &str, contains: &str) -> AutoReplyRule {
        AutoReplyRule {
            enabled,
            match_topic: match_topic.to_string(),
            payload_contains: contains.to_string(),
            reply_topic: "out".to_string(),
            reply_payload: "pong".to_string(),
            ..AutoReplyRule::default()
        }
    }

    #[test]
    fn auto_reply_selects_first_enabled_match() {
        let rules = vec![
            sample_rule(false, "a/#", ""),
            sample_rule(true, "a/#", "ping"),
            sample_rule(true, "a/#", "ping"),
        ];
        assert_eq!(select_auto_reply_rule(&rules, "a/b", "hello ping"), Some(1));
    }

    #[test]
    fn auto_reply_skips_disabled_and_empty_match_topic() {
        let rules = vec![
            sample_rule(true, "", "ping"),
            sample_rule(false, "a/#", "ping"),
            sample_rule(true, "a/#", "ping"),
        ];
        assert_eq!(select_auto_reply_rule(&rules, "a/b", "ping"), Some(2));
        assert_eq!(select_auto_reply_rule(&rules, "other", "ping"), None);
    }

    #[test]
    fn auto_reply_payload_contains_is_case_sensitive() {
        let rules = vec![sample_rule(true, "a/#", "Hello")];
        assert_eq!(select_auto_reply_rule(&rules, "a/b", "Hello"), Some(0));
        assert_eq!(select_auto_reply_rule(&rules, "a/b", "hello"), None);
    }

    #[test]
    fn auto_reply_empty_contains_matches_any_payload() {
        let rules = vec![sample_rule(true, "dev/+", "")];
        assert_eq!(select_auto_reply_rule(&rules, "dev/1", ""), Some(0));
        assert_eq!(select_auto_reply_rule(&rules, "dev/1", "anything"), Some(0));
    }

    #[test]
    fn own_echo_compares_topic_and_raw_bytes() {
        let mut runtime = ConnRuntime::new();
        runtime.last_auto_reply = Some(("out".into(), b"pong".to_vec()));
        assert!(is_own_echo(&runtime, "out", b"pong"));
        assert!(!is_own_echo(&runtime, "out", b"Pong"));
        assert!(!is_own_echo(&runtime, "other", b"pong"));
        runtime.last_auto_reply = None;
        assert!(!is_own_echo(&runtime, "out", b"pong"));
    }

    #[test]
    fn echo_policy_index_round_trip() {
        assert_eq!(echo_policy_from_index(0), "skip");
        assert_eq!(echo_policy_from_index(1), "rematch");
        assert_eq!(echo_policy_from_index(99), "skip");
        assert_eq!(echo_policy_to_index("skip"), 0);
        assert_eq!(echo_policy_to_index("rematch"), 1);
        assert_eq!(echo_policy_to_index("unknown"), 0);
    }
}
