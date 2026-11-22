//! UI-independent application runtime.
//!
//! The UI thread owns one [`AppRuntime`]. MQTT workers only push
//! [`RuntimeEvent`]s into an [`EventSink`], so all config/log mutation happens
//! on the UI thread and behaviour is identical across the egui / Iced / GPUI ports.

use std::cell::RefCell;
use std::collections::{HashMap, VecDeque};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};

use crate::cli::BenchOptions;
use crate::config::{
    self, clamp_font_scale, normalize_auto_reply_echo, AppConfig, AutoReplyRule, ConnectionConfig,
    SubscriptionConfig,
};
use crate::export;
use crate::filter::log_matches;
use crate::form::FormState;
use crate::i18n::{self, Lang};
use crate::logs::LogStore;
use crate::model::{
    clamp_log_limit, default_client_id, parse_timed_interval, ConnStatus, LogMeta, LogRecord,
    PayloadFormat, Skin,
};
use crate::mqtt::{qos_from_u8, ConnectParams, MqttHandle, ProtocolVersion, PublishOptions, UiEvent};
use crate::payload;
use crate::theme::{self, CustomPalette, Palette, PaletteField};

const PERSIST_DEBOUNCE: Duration = Duration::from_millis(400);
/// Give up after this many consecutive failures; the user can always click Connect.
const RECONNECT_MAX_ATTEMPTS: u32 = 8;
/// A broker may forward one publish once per matching subscription. Copies are
/// written back to back, so only fold an identical RECV inside this window.
const RECV_FOLD_WINDOW: Duration = Duration::from_millis(250);
/// Distinct in-flight messages remembered for copy folding.
const RECV_FOLD_SLOTS: usize = 8;

/// 1s, 2s, 4s … capped at 30s. Exposed for tests and status hints.
pub fn reconnect_delay(attempt: u32) -> Duration {
    let shift = attempt.saturating_sub(1).min(5);
    Duration::from_secs((1u64 << shift).min(30))
}

/// Worker → UI messages. `conn_id` is carried so one queue can serve every session.
#[derive(Clone, Debug)]
pub enum RuntimeEvent {
    Mqtt { conn_id: String, event: UiEvent },
    TimedTick { conn_id: String, generation: u64 },
}

/// Thread-safe queue shared by all MQTT workers and the timed-publish threads.
pub struct EventSink {
    queue: Mutex<VecDeque<RuntimeEvent>>,
    wake: Mutex<Option<Arc<dyn Fn() + Send + Sync>>>,
}

impl Default for EventSink {
    fn default() -> Self {
        Self::new()
    }
}

impl EventSink {
    pub fn new() -> Self {
        Self {
            queue: Mutex::new(VecDeque::new()),
            wake: Mutex::new(None),
        }
    }

    /// egui: `ctx.request_repaint()`. Iced/GPUI may leave this unset and tick instead.
    pub fn set_wake(&self, wake: Option<Arc<dyn Fn() + Send + Sync>>) {
        *self.wake.lock().expect("wake lock") = wake;
    }

    pub fn push(&self, event: RuntimeEvent) {
        self.queue.lock().expect("event queue").push_back(event);
        let wake = self.wake.lock().expect("wake lock").clone();
        if let Some(wake) = wake {
            wake();
        }
    }

    pub fn drain(&self) -> Vec<RuntimeEvent> {
        let mut queue = self.queue.lock().expect("event queue");
        queue.drain(..).collect()
    }
}

/// Window-wide preferences edited on the settings page.
#[derive(Clone, Debug)]
pub struct PrefsState {
    pub log_format: i32,
    pub auto_scroll: bool,
    pub log_filter: String,
    pub log_kind_filter: i32,
    pub skin_choice: i32,
    pub language: i32,
    /// Multiplier applied to every UI text size; `1.0` is standard.
    pub font_scale: f32,
    /// Keep the window above other applications.
    pub always_on_top: bool,
}

impl PrefsState {
    fn from_config(config: &AppConfig) -> Self {
        Self {
            log_format: PayloadFormat::from_key(&config.log_format).to_index(),
            auto_scroll: config.auto_scroll,
            log_filter: config.log_filter.clone(),
            log_kind_filter: config.log_kind_filter.clamp(0, 4),
            skin_choice: Skin::from_key(&config.skin).to_index(),
            language: Lang::from_key(&config.language).to_index(),
            font_scale: clamp_font_scale(config.font_scale),
            always_on_top: config.always_on_top,
        }
    }
}

/// Identifies the inputs that can change which rows pass the log filters.
#[derive(PartialEq, Eq)]
struct ViewKey {
    format: i32,
    kind_filter: i32,
    filter: String,
    topic: String,
}

#[derive(Default)]
struct ViewCache {
    key: Option<ViewKey>,
    /// Seqs of matching rows, oldest first; seqs are contiguous per store.
    seqs: Vec<u64>,
    /// Last seq already tested against the current key.
    scanned_through: Option<u64>,
}

/// Identifies one logical inbound publish. QoS and packet id are left out on
/// purpose: copies delivered for different subscriptions may use different QoS
/// and every copy gets its own packet id.
fn recv_fingerprint(topic: &str, payload: &[u8], meta: &LogMeta) -> u64 {
    use std::hash::{Hash, Hasher};
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    topic.hash(&mut hasher);
    payload.hash(&mut hasher);
    meta.retain.hash(&mut hasher);
    meta.dup.hash(&mut hasher);
    meta.props.hash(&mut hasher);
    meta.expiry.hash(&mut hasher);
    hasher.finish()
}

/// Remembers RECV rows whose broker copies have not all arrived yet.
#[derive(Default)]
struct RecvFold {
    /// Fingerprint → (first accepted, copies still expected) within
    /// [`RECV_FOLD_WINDOW`]. Retained sets can interleave topics, so several
    /// groups are tracked instead of only the latest row.
    pending: HashMap<u64, (Instant, usize)>,
}

impl RecvFold {
    fn clear(&mut self) {
        self.pending.clear();
    }

    /// Returns true when this packet is another copy of a row already logged.
    fn absorb(&mut self, fingerprint: u64, copies: usize, now: Instant) -> bool {
        self.pending
            .retain(|_, (at, _)| now.duration_since(*at) <= RECV_FOLD_WINDOW);
        if let Some((_, remaining)) = self.pending.get_mut(&fingerprint) {
            if *remaining > 0 {
                *remaining -= 1;
                return true;
            }
        }
        if copies > 1 {
            if self.pending.len() >= RECV_FOLD_SLOTS {
                if let Some(oldest) = self
                    .pending
                    .iter()
                    .min_by_key(|(_, (at, _))| *at)
                    .map(|(key, _)| *key)
                {
                    self.pending.remove(&oldest);
                }
            }
            self.pending.insert(fingerprint, (now, copies - 1));
        }
        false
    }
}

/// How many distinct saved filters match `topic`; that is the number of copies
/// the broker can deliver for one publish. Falls back to 1 when unknown.
fn matching_subscription_count(config: &AppConfig, conn_id: &str, topic: &str) -> usize {
    let Some(conn) = config.connections.iter().find(|conn| conn.id == conn_id) else {
        return 1;
    };
    let mut count = 0;
    for (index, sub) in conn.subscriptions.iter().enumerate() {
        let filter = sub.topic.trim();
        if filter.is_empty() || !crate::filter::topic_matches_filter(topic, filter) {
            continue;
        }
        let seen = conn.subscriptions[..index]
            .iter()
            .any(|prev| prev.topic.trim() == filter);
        if !seen {
            count += 1;
        }
    }
    count.max(1)
}

/// Live MQTT client, logs, and timed-publish worker for one saved connection.
pub struct ConnRuntime {
    pub mqtt: MqttHandle,
    pub logs: LogStore,
    /// Incremental filter result; rebuilt only when the filter inputs change.
    view_cache: RefCell<ViewCache>,
    pub status: ConnStatus,
    /// Bumped on stop/reconnect so the previous timed-publish thread exits.
    pub timed_generation: Arc<AtomicU64>,
    pub timed_running: bool,
    /// Empty = show every topic; otherwise filter RECV/SEND by MQTT wildcard.
    pub active_sub_topic: String,
    /// Last auto reply used to detect own echo. Manual / preset / timed publish leave this alone.
    pub last_auto_reply: Option<(String, Vec<u8>)>,
    /// Transient "freeze the log list" switch. SYS/ERR rows are still recorded.
    pub logs_paused: bool,
    /// Data rows dropped while paused, shown in the UI and reported on resume.
    pub paused_dropped: u64,
    /// RECV / SEND messages seen since the counters were last reset. Traffic
    /// keeps counting while the list is paused; SYS and ERR rows are excluded.
    pub recv_count: u64,
    pub send_count: u64,
    /// User clicked Disconnect: suppress auto reconnect until the next manual Connect.
    pub manual_stop: bool,
    /// Consecutive automatic reconnect attempts; reset on a successful handshake.
    pub reconnect_attempt: u32,
    /// When the next attempt is due (UI thread time).
    pub next_reconnect: Option<Instant>,
    /// Auto-reply hit counter per rule index (transient, reset on connect / reorder).
    pub rule_hits: Vec<u64>,
    /// Last reply time per rule index, for `min_interval_secs`.
    pub rule_last: Vec<Option<Instant>>,
    /// Broker copies of recent inbound publishes, folded into one log row.
    recv_fold: RecvFold,
}

impl Default for ConnRuntime {
    fn default() -> Self {
        Self::new()
    }
}

impl ConnRuntime {
    pub fn new() -> Self {
        Self {
            mqtt: MqttHandle::new(),
            logs: LogStore::new(),
            view_cache: RefCell::new(ViewCache::default()),
            status: ConnStatus::Disconnected,
            timed_generation: Arc::new(AtomicU64::new(0)),
            timed_running: false,
            active_sub_topic: String::new(),
            last_auto_reply: None,
            logs_paused: false,
            paused_dropped: 0,
            recv_count: 0,
            send_count: 0,
            manual_stop: false,
            reconnect_attempt: 0,
            next_reconnect: None,
            rule_hits: Vec::new(),
            rule_last: Vec::new(),
            recv_fold: RecvFold::default(),
        }
    }
}

#[derive(Clone, Copy, Debug, Default)]
pub struct PollSummary {
    pub changed: bool,
    /// Set when a connection finished handshaking and the shell should show the session page.
    pub page_request: Option<i32>,
    pub status_changed: bool,
}

pub struct AppRuntime {
    pub config: AppConfig,
    pub form: FormState,
    pub prefs: PrefsState,
    runtimes: HashMap<String, ConnRuntime>,
    sink: Arc<EventSink>,
    startup_error: Option<String>,
    dirty_at: Option<Instant>,
    /// Last palette chosen while `Skin::System` was active.
    last_resolved_skin: Skin,
}

impl AppRuntime {
    /// Load `--config` or the portable `mqttc.json`, repair it, and build the first form.
    pub fn load(options: &BenchOptions, wake: Option<Arc<dyn Fn() + Send + Sync>>) -> Self {
        let (loaded, error) = match &options.config {
            Some(path) => match config::load_from(path) {
                Ok(value) => (value, None),
                Err(err) => (None, Some(err)),
            },
            None => match config::load() {
                Ok(value) => (value, None),
                Err(err) => (None, Some(err)),
            },
        };
        Self::new(loaded, error, wake)
    }

    /// Build from an already-parsed config (used by tests and the bench harness).
    pub fn new(
        config: Option<AppConfig>,
        startup_error: Option<String>,
        wake: Option<Arc<dyn Fn() + Send + Sync>>,
    ) -> Self {
        let mut config = config.unwrap_or_default();
        config.log_limit = clamp_log_limit(config.log_limit);

        // Language first: `local_broker_name` is localized.
        i18n::set_lang(Lang::from_key(&config.language));

        if config.connections.is_empty() {
            let conn = ConnectionConfig::new(i18n::local_broker_name());
            config.active_id = conn.id.clone();
            config.connections.push(conn);
        }
        if !config
            .connections
            .iter()
            .any(|conn| conn.id == config.active_id)
        {
            config.active_id = config.connections[0].id.clone();
        }

        let form = config
            .connections
            .iter()
            .find(|conn| conn.id == config.active_id)
            .map(FormState::from_config)
            .unwrap_or_default();
        let prefs = PrefsState::from_config(&config);
        let last_resolved_skin = Skin::from_index(prefs.skin_choice).resolve();
        let sink = Arc::new(EventSink::new());
        sink.set_wake(wake);

        let mut runtime = Self {
            config,
            form,
            prefs,
            runtimes: HashMap::new(),
            sink,
            startup_error,
            dirty_at: None,
            last_resolved_skin,
        };

        let active = runtime.config.active_id.clone();
        runtime.runtime_mut(&active);

        if let Some(err) = runtime.startup_error.clone() {
            runtime.push_log(&active, "ERR", "", err.into_bytes());
        }
        runtime
    }

    pub fn startup_error(&self) -> Option<&str> {
        self.startup_error.as_deref()
    }

    pub fn sink(&self) -> Arc<EventSink> {
        self.sink.clone()
    }

    /// `None` when no connection exists (should not happen after `new`, but keep it total).
    pub fn active_index(&self) -> Option<usize> {
        self.config
            .connections
            .iter()
            .position(|conn| conn.id == self.config.active_id)
    }

    pub fn active_id(&self) -> Option<String> {
        self.active_index()
            .map(|idx| self.config.connections[idx].id.clone())
    }

    pub fn active(&self) -> Option<&ConnectionConfig> {
        self.active_index().map(|idx| &self.config.connections[idx])
    }

    pub fn subscriptions(&self) -> &[SubscriptionConfig] {
        self.active()
            .map(|conn| conn.subscriptions.as_slice())
            .unwrap_or(&[])
    }

    pub fn runtime(&self, id: &str) -> Option<&ConnRuntime> {
        self.runtimes.get(id)
    }

    pub fn runtime_mut(&mut self, id: &str) -> &mut ConnRuntime {
        let limit = self.config.log_limit;
        self.runtimes.entry(id.to_string()).or_insert_with(|| {
            let mut runtime = ConnRuntime::new();
            runtime.logs.set_limit(limit);
            runtime
        })
    }

    pub fn status_of(&self, id: &str) -> ConnStatus {
        self.runtimes
            .get(id)
            .map(|rt| rt.status)
            .unwrap_or(ConnStatus::Disconnected)
    }

    pub fn status(&self) -> ConnStatus {
        self.active_id()
            .map(|id| self.status_of(&id))
            .unwrap_or(ConnStatus::Disconnected)
    }

    pub fn is_connected(&self, id: &str) -> bool {
        self.status_of(id) == ConnStatus::Connected
    }

    pub fn active_sub_topic(&self) -> String {
        self.active_id()
            .and_then(|id| self.runtimes.get(&id).map(|rt| rt.active_sub_topic.clone()))
            .unwrap_or_default()
    }

    pub fn timed_running(&self) -> bool {
        self.active_id()
            .and_then(|id| self.runtimes.get(&id).map(|rt| rt.timed_running))
            .unwrap_or(false)
    }

    pub fn skin(&self) -> Skin {
        Skin::from_index(self.prefs.skin_choice)
    }

    pub fn palette(&self) -> Palette {
        match self.skin().resolve() {
            Skin::Custom => self.config.custom_palette.resolve(),
            skin => *theme::palette(skin),
        }
    }

    // ---------------------------------------------------------------- prefs

    pub fn apply_prefs(&mut self) {
        self.config.log_format = PayloadFormat::from_index(self.prefs.log_format)
            .to_key()
            .to_string();
        self.config.auto_scroll = self.prefs.auto_scroll;
        self.config.log_filter = self.prefs.log_filter.clone();
        self.config.log_kind_filter = self.prefs.log_kind_filter.clamp(0, 4);
        self.config.skin = Skin::from_index(self.prefs.skin_choice)
            .to_key()
            .to_string();
        self.config.language = Lang::from_index(self.prefs.language).to_key().to_string();
        self.config.font_scale = clamp_font_scale(self.prefs.font_scale);
        self.config.always_on_top = self.prefs.always_on_top;
        i18n::set_lang(Lang::from_index(self.prefs.language));
    }

    pub fn set_log_format(&mut self, index: i32) {
        self.prefs.log_format = index;
        self.apply_prefs();
        self.persist();
    }

    pub fn set_auto_scroll(&mut self, on: bool) {
        self.prefs.auto_scroll = on;
        self.mark_dirty();
    }

    pub fn set_log_filter(&mut self, filter: String) {
        self.prefs.log_filter = filter;
        self.mark_dirty();
    }

    pub fn set_log_kind_filter(&mut self, kind: i32) {
        self.prefs.log_kind_filter = kind.clamp(0, 4);
        self.mark_dirty();
    }

    pub fn set_skin_choice(&mut self, choice: i32) {
        let previous = self.skin().resolve();
        self.prefs.skin_choice = choice;
        self.last_resolved_skin = self.skin().resolve();
        // First switch to “custom” seeds a full palette from the skin the user
        // was just looking at, so they tweak instead of starting from scratch.
        if self.skin() == Skin::Custom && self.config.custom_palette.is_empty() {
            let base = if previous == Skin::Custom {
                Skin::Black
            } else {
                previous
            };
            self.config.custom_palette = CustomPalette::from_base(base);
        }
        self.apply_prefs();
        self.persist();
    }

    // ------------------------------------------------------- custom palette

    pub fn custom_palette(&self) -> &CustomPalette {
        &self.config.custom_palette
    }

    /// Raw hex text for one slot, defaulting to the resolved color.
    pub fn custom_color_text(&self, field: PaletteField) -> String {
        let resolved = self.palette();
        self.config.custom_palette.text(field, field.get(&resolved))
    }

    /// Store what the user typed; invalid text simply falls back on resolve.
    pub fn set_custom_color(&mut self, field: PaletteField, value: String) {
        if self.skin() != Skin::Custom {
            return;
        }
        self.config.custom_palette.set(field, value);
        self.mark_dirty();
    }

    /// Drop one override so the slot follows the base skin again.
    pub fn reset_custom_color(&mut self, field: PaletteField) {
        if self.skin() != Skin::Custom {
            return;
        }
        self.config.custom_palette.reset(field);
        self.mark_dirty();
    }

    /// Pick the skin used for back-filling missing colors.
    pub fn set_custom_base(&mut self, skin: i32) {
        let skin = Skin::from_index(skin);
        if skin == Skin::Custom {
            return;
        }
        self.config.custom_palette.base = skin.resolve().to_key().to_string();
        self.mark_dirty();
    }

    /// Overwrite every custom color with the base skin's table.
    pub fn refill_custom_palette(&mut self) {
        self.config.custom_palette.refill();
        self.persist();
    }

    pub fn set_language(&mut self, index: i32) {
        self.prefs.language = index;
        self.apply_prefs();
        self.persist();
    }

    /// Sets the UI text scale. Out-of-range or non-finite values are clamped.
    pub fn set_font_scale(&mut self, scale: f32) {
        let scale = clamp_font_scale(scale);
        if self.prefs.font_scale == scale {
            return;
        }
        self.prefs.font_scale = scale;
        self.apply_prefs();
        self.persist();
    }

    /// Toggles the title-bar pin and persists the choice.
    pub fn set_always_on_top(&mut self, on: bool) {
        if self.prefs.always_on_top == on {
            return;
        }
        self.prefs.always_on_top = on;
        self.apply_prefs();
        self.persist();
    }

    /// System skin: call periodically so the palette can switch with the OS.
    /// Returns true when Black/White flipped and the UI should repaint with new colors.
    pub fn refresh_system_skin(&mut self) -> bool {
        if self.skin() != Skin::System {
            return false;
        }
        let resolved = Skin::System.resolve();
        if resolved != self.last_resolved_skin {
            self.last_resolved_skin = resolved;
            true
        } else {
            false
        }
    }

    // ---------------------------------------------------------------- form

    /// Copy widgets into the active saved connection (clamping like the reference).
    pub fn apply_form(&mut self) {
        let Some(idx) = self.active_index() else { return };
        self.form.apply_to(&mut self.config.connections[idx]);
    }

    /// Mark a debounced save; text edits call this instead of writing on every keystroke.
    pub fn mark_dirty(&mut self) {
        self.dirty_at = Some(Instant::now());
    }

    pub fn persist(&mut self) {
        self.dirty_at = None;
        if let Err(err) = config::save(&self.config) {
            eprintln!("{err}");
        }
    }

    pub fn maybe_persist(&mut self) {
        if let Some(at) = self.dirty_at {
            if at.elapsed() >= PERSIST_DEBOUNCE {
                self.apply_form();
                self.apply_prefs();
                self.persist();
            }
        }
    }

    // ---------------------------------------------------------- connections

    pub fn new_connection(&mut self) {
        self.apply_form();
        let name = i18n::new_connection_name(self.config.connections.len() + 1);
        let conn = ConnectionConfig::new(name);
        self.config.active_id = conn.id.clone();
        let id = conn.id.clone();
        self.config.connections.push(conn);
        self.runtime_mut(&id);
        if let Some(idx) = self.active_index() {
            self.form = FormState::from_config(&self.config.connections[idx]);
        }
        self.persist();
    }

    pub fn select_connection(&mut self, id: &str) -> bool {
        if self.config.active_id == id {
            return false;
        }
        self.apply_form();
        self.config.active_id = id.to_string();
        self.runtime_mut(id);
        if let Some(idx) = self.active_index() {
            self.form = FormState::from_config(&self.config.connections[idx]);
        }
        self.persist();
        true
    }

    pub fn delete_connection(&mut self, id: &str) {
        self.stop_timed(id);
        if let Some(runtime) = self.runtimes.remove(id) {
            runtime.mqtt.disconnect();
        }
        self.config.connections.retain(|conn| conn.id != id);
        if self.config.active_id == id {
            self.config.active_id = self
                .config
                .connections
                .first()
                .map(|conn| conn.id.clone())
                .unwrap_or_default();
        }
        if self.config.connections.is_empty() {
            let conn = ConnectionConfig::new(i18n::local_broker_name());
            self.config.active_id = conn.id.clone();
            self.config.connections.push(conn);
        }
        let active = self.config.active_id.clone();
        self.runtime_mut(&active);
        if let Some(idx) = self.active_index() {
            self.form = FormState::from_config(&self.config.connections[idx]);
        }
        self.persist();
    }

    fn stop_timed(&mut self, id: &str) {
        if let Some(runtime) = self.runtimes.get_mut(id) {
            runtime.timed_generation.fetch_add(1, Ordering::Relaxed);
            runtime.timed_running = false;
        }
    }

    // ------------------------------------------------------------- connect

    pub fn connect(&mut self) -> bool {
        let Some(idx) = self.active_index() else {
            return false;
        };
        self.connect_at(idx, true, true)
    }

    /// Connect one saved connection. `apply_form` mirrors the editor into the
    /// active connection first; background reconnects pass `false` so they never
    /// touch whatever the user is editing.
    fn connect_at(&mut self, idx: usize, apply_form: bool, reset_backoff: bool) -> bool {
        if apply_form && self.active_index() == Some(idx) {
            self.apply_form();
        }
        let mut cfg = self.config.connections[idx].clone();
        let id = cfg.id.clone();

        let host = cfg.host.trim().to_string();
        if host.is_empty() {
            self.push_log(
                &id,
                "ERR",
                "",
                i18n::t("主机不能为空", "Host is required").into_bytes(),
            );
            return false;
        }
        let port = match cfg.port.trim().parse::<u16>() {
            Ok(port) if port > 0 => port,
            _ => {
                self.push_log(
                    &id,
                    "ERR",
                    "",
                    i18n::t(
                        "端口无效，范围为 1-65535",
                        "Invalid port, range is 1-65535",
                    )
                    .into_bytes(),
                );
                return false;
            }
        };
        if let Err(err) = validate_tls_paths(&cfg) {
            self.push_log(&id, "ERR", "", err.into_bytes());
            return false;
        }
        if cfg.client_id.trim().is_empty() {
            cfg.client_id = default_client_id();
            if self.active_index() == Some(idx) {
                self.form.client_id = cfg.client_id.clone();
            }
        }
        cfg.host = host;
        cfg.username = cfg.username.trim().to_string();
        self.config.connections[idx] = cfg.clone();

        let auth_hint = if cfg.username.is_empty() {
            i18n::t("匿名", "anonymous")
        } else if i18n::is_en() {
            format!("user {}", cfg.username)
        } else {
            format!("用户 {}", cfg.username)
        };
        self.stop_timed(&id);
        {
            let runtime = self.runtime_mut(&id);
            runtime.status = ConnStatus::Connecting;
            runtime.last_auto_reply = None;
            runtime.manual_stop = false;
            if reset_backoff {
                runtime.reconnect_attempt = 0;
                runtime.next_reconnect = None;
            }
        }

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
        self.push_log(
            &id,
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
            will_payload: cfg.will_payload.as_bytes().to_vec(),
            will_qos: cfg.will_qos,
            will_retain: cfg.will_retain,
            ssl: cfg.ssl,
            ssl_verify: cfg.ssl_verify,
            ssl_ca: cfg.ssl_ca.clone(),
            ssl_cert: cfg.ssl_cert.clone(),
            ssl_key: cfg.ssl_key.clone(),
            protocol,
        };

        let sink = self.sink.clone();
        let conn_id = id.clone();
        let mqtt = self.runtime_mut(&id).mqtt.clone();
        mqtt.connect(params, move |event| {
            sink.push(RuntimeEvent::Mqtt {
                conn_id: conn_id.clone(),
                event,
            });
        });
        self.persist();
        true
    }

    pub fn disconnect(&mut self) {
        self.apply_form();
        let Some(id) = self.active_id() else { return };
        self.stop_timed(&id);
        {
            let runtime = self.runtime_mut(&id);
            runtime.mqtt.disconnect();
            runtime.status = ConnStatus::Disconnected;
            runtime.last_auto_reply = None;
            runtime.manual_stop = true;
            runtime.reconnect_attempt = 0;
            runtime.next_reconnect = None;
        }
        self.push_log(&id, "SYS", "", i18n::t("已断开连接", "Disconnected").into_bytes());
        self.persist();
    }

    // -------------------------------------------------------- subscription

    pub fn subscribe(&mut self) {
        self.apply_form();
        let Some(id) = self.active_id() else { return };
        if !self.is_connected(&id) {
            self.push_log(
                &id,
                "ERR",
                "",
                i18n::t("请先连接 Broker", "Connect to a broker first").into_bytes(),
            );
            return;
        }
        let topic = self.form.sub_topic.trim().to_string();
        if topic.is_empty() {
            self.push_log(&id, "ERR", "", i18n::t("Topic 为空", "Topic is empty").into_bytes());
            return;
        }
        let qos = self.form.sub_qos.clamp(0, 2) as u8;
        let no_local = self.form.sub_no_local;
        let rap = self.form.sub_retain_as_published;
        let mqtt = self.runtime_mut(&id).mqtt.clone();
        match mqtt.subscribe_with_options(&topic, qos_from_u8(qos), no_local, rap) {
            Ok(()) => {
                let Some(idx) = self.active_index() else { return };
                let exists = self.config.connections[idx]
                    .subscriptions
                    .iter()
                    .any(|sub| sub.topic == topic);
                if exists {
                    self.push_log(
                        &id,
                        "SYS",
                        &topic,
                        i18n::t("已在订阅列表中", "Already in subscription list").into_bytes(),
                    );
                } else {
                    self.config.connections[idx]
                        .subscriptions
                        .push(SubscriptionConfig {
                            topic: topic.clone(),
                            qos,
                            no_local,
                            retain_as_published: rap,
                        });
                    self.push_log(
                        &id,
                        "SYS",
                        &topic,
                        i18n::t(
                            &format!("已订阅 QoS {qos}"),
                            &format!("Subscribed QoS {qos}"),
                        )
                        .into_bytes(),
                    );
                }
                self.persist();
            }
            Err(err) => self.push_log(&id, "ERR", &topic, err.into_bytes()),
        }
    }

    /// Flip one MQTT 5 option on a saved subscription. Re-subscribes when connected
    /// so the broker applies the new option immediately.
    pub fn flip_sub_option(&mut self, index: usize, no_local: bool) -> bool {
        let Some(id) = self.active_id() else { return false };
        let Some(idx) = self.active_index() else { return false };
        let Some(sub) = self.config.connections[idx].subscriptions.get_mut(index) else {
            return false;
        };
        if no_local {
            sub.no_local = !sub.no_local;
        } else {
            sub.retain_as_published = !sub.retain_as_published;
        }
        let sub = sub.clone();
        let connected = self.is_connected(&id);
        if connected {
            let mqtt = self.runtime_mut(&id).mqtt.clone();
            if let Err(err) = mqtt.subscribe_with_options(
                &sub.topic,
                qos_from_u8(sub.qos),
                sub.no_local,
                sub.retain_as_published,
            ) {
                self.push_log(&id, "ERR", &sub.topic, err.into_bytes());
                return false;
            }
        }
        self.persist();
        true
    }

    pub fn unsubscribe(&mut self, index: usize) {
        let Some(id) = self.active_id() else { return };
        let Some(idx) = self.active_index() else { return };
        if index >= self.config.connections[idx].subscriptions.len() {
            return;
        }
        let sub = self.config.connections[idx].subscriptions[index].clone();
        let mqtt = self.runtime_mut(&id).mqtt.clone();
        match mqtt.unsubscribe(&sub.topic) {
            Ok(()) => self.push_log(
                &id,
                "SYS",
                &sub.topic,
                i18n::t("已取消订阅", "Unsubscribed").into_bytes(),
            ),
            Err(err) => self.push_log(&id, "ERR", &sub.topic, err.into_bytes()),
        }
        if let Some(idx) = self.active_index() {
            if index < self.config.connections[idx].subscriptions.len() {
                self.config.connections[idx].subscriptions.remove(index);
            }
        }
        if self.runtime_mut(&id).active_sub_topic == sub.topic {
            self.runtime_mut(&id).active_sub_topic.clear();
        }
        self.persist();
    }

    /// Reorder an auto-reply rule. Matching stops at the first hit, so order is significant.
    pub fn move_rule(&mut self, index: usize, delta: isize) -> bool {
        self.apply_form();
        if !self.form.move_rule(index, delta) {
            return false;
        }
        self.apply_form();
        if let Some(id) = self.active_id() {
            let runtime = self.runtime_mut(&id);
            runtime.rule_hits.clear();
            runtime.rule_last.clear();
        }
        self.persist();
        true
    }

    /// Reply count per rule for the active connection, indexed like `form.rules`.
    pub fn active_rule_hits(&self) -> Vec<u64> {
        self.active_id()
            .and_then(|id| self.runtimes.get(&id))
            .map(|rt| rt.rule_hits.clone())
            .unwrap_or_default()
    }

    pub fn reset_rule_stats(&mut self) {
        if let Some(id) = self.active_id() {
            let runtime = self.runtime_mut(&id);
            runtime.rule_hits.clear();
            runtime.rule_last.clear();
        }
    }

    /// Click a subscription row: toggle the log filter for that topic.
    pub fn toggle_sub_filter(&mut self, topic: &str) {
        let Some(id) = self.active_id() else { return };
        let current = self.runtime_mut(&id).active_sub_topic.clone();
        self.runtime_mut(&id).active_sub_topic = if current == topic {
            String::new()
        } else {
            topic.to_string()
        };
    }

    /// Clear the subscription-row topic filter.
    pub fn clear_sub_filter(&mut self) {
        if let Some(id) = self.active_id() {
            self.runtime_mut(&id).active_sub_topic.clear();
        }
    }

    // ------------------------------------------------------------- publish

    /// Bottom "Publish" button.
    pub fn publish_form(&mut self) -> bool {
        self.apply_form();
        let Some(id) = self.active_id() else { return false };
        self.publish_from_connection(&id, true, true).is_ok()
    }

    /// Preset "Send": uses that row only; does not copy into the bottom form.
    pub fn publish_preset(&mut self, index: usize) -> bool {
        self.apply_form();
        let Some(id) = self.active_id() else { return false };
        if !self.is_connected(&id) {
            self.push_log(
                &id,
                "ERR",
                "",
                i18n::t("请先连接 Broker", "Connect to a broker first").into_bytes(),
            );
            return false;
        }
        let Some(idx) = self.active_index() else { return false };
        let Some(preset) = self.config.connections[idx]
            .publish_presets
            .get(index)
            .cloned()
        else {
            return false;
        };
        let topic = preset.topic.trim().to_string();
        if topic.is_empty() {
            self.push_log(
                &id,
                "ERR",
                "",
                i18n::t("Topic 为空", "Topic is empty").into_bytes(),
            );
            return false;
        }
        let format = PayloadFormat::from_key(&preset.format);
        let payload = match payload::encode_payload(&preset.payload, format) {
            Ok(bytes) => bytes,
            Err(err) => {
                self.push_log(&id, "ERR", &topic, err.into_bytes());
                return false;
            }
        };
        let mqtt = self.runtime_mut(&id).mqtt.clone();
        match mqtt.publish(
            &topic,
            qos_from_u8(preset.qos.min(2)),
            preset.retain,
            payload.clone(),
        ) {
            Ok(()) => {
                self.push_log_meta(
                    &id,
                    "SEND",
                    &topic,
                    payload,
                    LogMeta {
                        qos: Some(preset.qos.min(2)),
                        retain: preset.retain,
                        ..LogMeta::default()
                    },
                );
                self.persist();
                true
            }
            Err(err) => {
                self.push_log(&id, "ERR", &topic, err.into_bytes());
                false
            }
        }
    }

    /// Timed publish: read the saved connection fields even if another connection is on screen.
    fn publish_from_connection(
        &mut self,
        conn_id: &str,
        persist_on_ok: bool,
        from_form: bool,
    ) -> Result<(), ()> {
        if !self.is_connected(conn_id) {
            self.push_log(
                conn_id,
                "ERR",
                "",
                i18n::t("请先连接 Broker", "Connect to a broker first").into_bytes(),
            );
            return Err(());
        }
        let Some(idx) = self
            .config
            .connections
            .iter()
            .position(|conn| conn.id == conn_id)
        else {
            return Err(());
        };
        let cfg = self.config.connections[idx].clone();
        let topic = if from_form {
            self.form.pub_topic.trim().to_string()
        } else {
            cfg.pub_topic.trim().to_string()
        };
        if topic.is_empty() {
            self.push_log(
                conn_id,
                "ERR",
                "",
                i18n::t("Topic 为空", "Topic is empty").into_bytes(),
            );
            return Err(());
        }
        let format = if from_form {
            PayloadFormat::from_index(self.form.pub_format)
        } else {
            PayloadFormat::from_key(&cfg.pub_format)
        };
        let source = if from_form {
            self.form.pub_payload.clone()
        } else {
            cfg.pub_payload.clone()
        };
        let payload = match payload::encode_payload(&source, format) {
            Ok(bytes) => bytes,
            Err(err) => {
                self.push_log(conn_id, "ERR", &topic, err.into_bytes());
                return Err(());
            }
        };
        let qos = if from_form {
            self.form.pub_qos.clamp(0, 2) as u8
        } else {
            cfg.pub_qos.min(2)
        };
        let retain = if from_form {
            self.form.pub_retain
        } else {
            cfg.pub_retain
        };
        let (props_text, expiry) = if from_form {
            (
                self.form.pub_props.clone(),
                self.form.pub_expiry.trim().parse::<u32>().unwrap_or(0),
            )
        } else {
            (cfg.pub_props.clone(), cfg.pub_expiry)
        };
        let options = publish_options(&props_text, expiry);
        let mqtt = self.runtime_mut(conn_id).mqtt.clone();
        match mqtt.publish_with_options(&topic, qos_from_u8(qos), retain, payload.clone(), &options)
        {
            Ok(()) => {
                self.push_log_meta(
                    conn_id,
                    "SEND",
                    &topic,
                    payload,
                    LogMeta {
                        qos: Some(qos),
                        retain,
                        props: options.user_properties.clone(),
                        expiry: options.expiry,
                        ..LogMeta::default()
                    },
                );
                if persist_on_ok {
                    self.persist();
                }
                Ok(())
            }
            Err(err) => {
                self.push_log(conn_id, "ERR", &topic, err.into_bytes());
                Err(())
            }
        }
    }

    /// Pretty-print the payload box. Does not send anything.
    pub fn format_payload(&mut self) -> Result<(), String> {
        if self.form.pub_payload.trim().is_empty() {
            return Ok(());
        }
        match payload::beautify_json(&self.form.pub_payload) {
            Ok(pretty) => {
                self.form.pub_payload = pretty;
                self.mark_dirty();
                Ok(())
            }
            Err(err) => {
                self.push_log_active("ERR", "", err.clone().into_bytes());
                Err(err)
            }
        }
    }

    // --------------------------------------------------------------- timed

    pub fn toggle_timed(&mut self) {
        self.apply_form();
        let Some(id) = self.active_id() else { return };
        if self
            .runtime(&id)
            .map(|rt| rt.timed_running)
            .unwrap_or(false)
        {
            self.stop_timed(&id);
            self.push_log(
                &id,
                "SYS",
                "",
                i18n::t("已停止定时发布", "Timed publish stopped").into_bytes(),
            );
            self.persist();
            return;
        }
        if !self.is_connected(&id) {
            self.push_log(
                &id,
                "ERR",
                "",
                i18n::t("请先连接 Broker", "Connect to a broker first").into_bytes(),
            );
            return;
        }
        let interval = match parse_timed_interval(&self.form.timed_interval) {
            Ok(secs) => secs,
            Err(err) => {
                self.push_log(&id, "ERR", "", err.into_bytes());
                return;
            }
        };
        if let Some(idx) = self.active_index() {
            self.config.connections[idx].timed_interval = interval;
        }
        let generation = {
            let runtime = self.runtime_mut(&id);
            let next = runtime.timed_generation.load(Ordering::Relaxed) + 1;
            runtime.timed_generation.store(next, Ordering::Relaxed);
            runtime.timed_running = true;
            next
        };
        // Log the start before the immediate first tick, so the log reads
        // SYS (started) → SEND (first message) instead of SEND first.
        self.push_log(
            &id,
            "SYS",
            "",
            i18n::t(
                &format!("已启动定时发布，每 {interval} 秒"),
                &format!("Timed publish started every {interval}s"),
            )
            .into_bytes(),
        );
        // First tick uses the bottom form, not a preset.
        if self.publish_from_connection(&id, false, true).is_err() {
            self.stop_timed(&id);
            self.push_log(
                &id,
                "SYS",
                "",
                i18n::t(
                    "首条发送失败，定时发布未启动",
                    "First send failed; timed publish not started",
                )
                .into_bytes(),
            );
            return;
        }
        self.persist();

        let sink = self.sink.clone();
        let conn_id = id.clone();
        let gen = self.runtime(&id).expect("runtime").timed_generation.clone();
        let _ = thread::Builder::new()
            .name("timed-publish".to_string())
            .stack_size(128 * 1024)
            .spawn(move || loop {
                thread::sleep(Duration::from_secs(interval as u64));
                // Stop / reconnect bumped the counter; this loop is stale.
                if gen.load(Ordering::Relaxed) != generation {
                    break;
                }
                sink.push(RuntimeEvent::TimedTick {
                    conn_id: conn_id.clone(),
                    generation,
                });
            });
    }

    // ------------------------------------------------------------- polling

    /// Drain worker events. Call once per frame (or per 100 ms tick) on the UI thread.
    pub fn poll(&mut self) -> PollSummary {
        let mut summary = PollSummary::default();
        for event in self.sink.drain() {
            summary.changed = true;
            match event {
                RuntimeEvent::Mqtt { conn_id, event } => {
                    let status_before = self.status_of(&conn_id);
                    let page = self.handle_mqtt_event(&conn_id, event);
                    if page.is_some() {
                        summary.page_request = page;
                    }
                    if self.status_of(&conn_id) != status_before {
                        summary.status_changed = true;
                    }
                }
                RuntimeEvent::TimedTick { conn_id, generation } => {
                    self.handle_timed_tick(&conn_id, generation);
                }
            }
        }
        self.maybe_persist();
        self.reconnect_tick();
        summary
    }

    /// Queue an exponential-backoff retry after an unexpected disconnect.
    fn schedule_reconnect(&mut self, conn_id: &str, previous: ConnStatus) {
        if previous == ConnStatus::Disconnected {
            return;
        }
        let Some(cfg) = self
            .config
            .connections
            .iter()
            .find(|conn| conn.id == conn_id)
        else {
            return;
        };
        if !cfg.auto_reconnect {
            return;
        }
        let Some(runtime) = self.runtimes.get(conn_id) else {
            return;
        };
        if runtime.manual_stop {
            return;
        }
        if runtime.reconnect_attempt >= RECONNECT_MAX_ATTEMPTS {
            self.push_log(
                conn_id,
                "SYS",
                "",
                i18n::t(
                    &format!("已连续重试 {} 次，停止自动重连", RECONNECT_MAX_ATTEMPTS),
                    &format!(
                        "Auto reconnect stopped after {} attempts",
                        RECONNECT_MAX_ATTEMPTS
                    ),
                )
                .into_bytes(),
            );
            return;
        }
        let attempt = runtime.reconnect_attempt + 1;
        let delay = reconnect_delay(attempt);
        let runtime = self.runtime_mut(conn_id);
        runtime.reconnect_attempt = attempt;
        runtime.next_reconnect = Some(Instant::now() + delay);
        self.push_log(
            conn_id,
            "SYS",
            "",
            i18n::t(
                &format!(
                    "{} 秒后自动重连（第 {} 次）",
                    delay.as_secs(),
                    attempt
                ),
                &format!(
                    "Reconnecting in {}s (attempt {})",
                    delay.as_secs(),
                    attempt
                ),
            )
            .into_bytes(),
        );
    }

    /// Called from `poll()`: start the next attempt once its backoff has elapsed.
    /// Runs for every connection, not just the active one, so background
    /// sessions recover while the user is editing another connection.
    fn reconnect_tick(&mut self) {
        let now = Instant::now();
        let due: Vec<String> = self
            .runtimes
            .iter()
            .filter(|(_, rt)| rt.status == ConnStatus::Disconnected)
            .filter(|(_, rt)| matches!(rt.next_reconnect, Some(at) if at <= now))
            .map(|(id, _)| id.clone())
            .collect();
        for id in due {
            let Some(idx) = self.config.connections.iter().position(|c| c.id == id) else {
                let runtime = self.runtime_mut(&id);
                runtime.next_reconnect = None;
                runtime.reconnect_attempt = 0;
                continue;
            };
            if !self.config.connections[idx].auto_reconnect {
                let runtime = self.runtime_mut(&id);
                runtime.next_reconnect = None;
                runtime.reconnect_attempt = 0;
                continue;
            }
            self.runtime_mut(&id).next_reconnect = None;
            let _ = self.connect_at(idx, false, false);
        }
    }

    /// `(attempt, remaining delay)` while an automatic retry is queued.
    pub fn reconnect_state(&self) -> Option<(u32, Duration)> {
        let id = self.active_id()?;
        let runtime = self.runtimes.get(&id)?;
        let at = runtime.next_reconnect?;
        Some((
            runtime.reconnect_attempt,
            at.saturating_duration_since(Instant::now()),
        ))
    }

    pub fn reconnect_pending(&self) -> bool {
        self.reconnect_state().is_some()
    }

    /// Brokers forward one publish once per matching subscription. Fold the
    /// extra copies so the log row, the counter and auto reply see it once.
    fn absorb_subscription_copy(
        &mut self,
        conn_id: &str,
        topic: &str,
        payload: &[u8],
        meta: &LogMeta,
    ) -> bool {
        let copies = matching_subscription_count(&self.config, conn_id, topic);
        // Single-filter connections never fold, so skip hashing their payloads.
        let has_pending = self
            .runtime(conn_id)
            .map(|rt| !rt.recv_fold.pending.is_empty())
            .unwrap_or(false);
        if copies < 2 && !has_pending {
            return false;
        }
        let fingerprint = recv_fingerprint(topic, payload, meta);
        self.runtime_mut(conn_id)
            .recv_fold
            .absorb(fingerprint, copies, Instant::now())
    }

    fn handle_mqtt_event(&mut self, conn_id: &str, event: UiEvent) -> Option<i32> {
        match event {
            UiEvent::Connected => {
                {
                    let runtime = self.runtime_mut(conn_id);
                    runtime.status = ConnStatus::Connected;
                    runtime.reconnect_attempt = 0;
                    runtime.next_reconnect = None;
                    runtime.manual_stop = false;
                    runtime.rule_hits.clear();
                    runtime.rule_last.clear();
                    runtime.recv_fold.clear();
                }
                self.push_log(
                    conn_id,
                    "SYS",
                    "",
                    i18n::t("连接成功", "Connected").into_bytes(),
                );
                self.resubscribe_all(conn_id);
                if self.config.active_id == conn_id {
                    return Some(1);
                }
            }
            UiEvent::Disconnected { reason } => {
                let was_timed = self
                    .runtimes
                    .get(conn_id)
                    .map(|rt| rt.timed_running)
                    .unwrap_or(false);
                self.stop_timed(conn_id);
                let previous = self.runtime_mut(conn_id).status;
                {
                    let runtime = self.runtime_mut(conn_id);
                    runtime.status = ConnStatus::Disconnected;
                    runtime.last_auto_reply = None;
                    runtime.recv_fold.clear();
                }
                if previous != ConnStatus::Disconnected && !reason.is_empty() {
                    self.push_log(
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
                    self.push_log(
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
                self.schedule_reconnect(conn_id, previous);
            }
            UiEvent::Recv {
                topic,
                payload,
                qos,
                retain,
                dup,
                props,
                expiry,
            } => {
                let meta = LogMeta {
                    qos: Some(qos),
                    retain,
                    dup,
                    props,
                    expiry,
                };
                if self.absorb_subscription_copy(conn_id, &topic, &payload, &meta) {
                    return None;
                }
                self.push_log_meta(conn_id, "RECV", &topic, payload.clone(), meta);
                self.maybe_auto_reply(conn_id, &topic, &payload);
            }
            UiEvent::Reason {
                name,
                pkid,
                code,
                ok,
            } => {
                let message = if ok {
                    format!("{name} #{pkid}: {code}")
                } else {
                    format!("{name} #{pkid} failed: {code}")
                };
                self.push_log(conn_id, if ok { "SYS" } else { "ERR" }, "", message.into_bytes());
            }
            UiEvent::Error { message } => {
                self.push_log(conn_id, "ERR", "", message.into_bytes());
            }
        }
        None
    }

    fn handle_timed_tick(&mut self, conn_id: &str, generation: u64) {
        let current = self
            .runtime(conn_id)
            .map(|rt| {
                (
                    rt.timed_generation.load(Ordering::Relaxed),
                    rt.timed_running,
                )
            })
            .unwrap_or((0, false));
        if current.0 != generation || !current.1 {
            return;
        }
        if !self.is_connected(conn_id) {
            self.stop_timed(conn_id);
            return;
        }
        if self.config.active_id == conn_id {
            self.apply_form();
        }
        let _ = self.publish_from_connection(conn_id, false, false);
    }

    /// rumqttc does not restore subscriptions after a new CONNECT; replay the saved list.
    fn resubscribe_all(&mut self, conn_id: &str) {
        let subs = self
            .config
            .connections
            .iter()
            .find(|conn| conn.id == conn_id)
            .map(|conn| conn.subscriptions.clone())
            .unwrap_or_default();
        if subs.is_empty() {
            return;
        }
        let mqtt = self.runtime_mut(conn_id).mqtt.clone();
        for sub in subs {
            match mqtt.subscribe_with_options(
                &sub.topic,
                qos_from_u8(sub.qos),
                sub.no_local,
                sub.retain_as_published,
            ) {
                Ok(()) => self.push_log(
                    conn_id,
                    "SYS",
                    &sub.topic,
                    i18n::t("已重新订阅", "Resubscribed").into_bytes(),
                ),
                Err(err) => self.push_log(conn_id, "ERR", &sub.topic, err.into_bytes()),
            }
        }
    }

    fn maybe_auto_reply(&mut self, conn_id: &str, topic: &str, payload: &[u8]) {
        if !self.is_connected(conn_id) {
            return;
        }
        let Some(idx) = self
            .config
            .connections
            .iter()
            .position(|conn| conn.id == conn_id)
        else {
            return;
        };
        let echo_skip =
            normalize_auto_reply_echo(&self.config.connections[idx].auto_reply_echo) == "skip";
        if echo_skip {
            let echo = self
                .runtime(conn_id)
                .and_then(|rt| rt.last_auto_reply.as_ref())
                .map(|(echo_topic, echo_payload)| {
                    echo_topic == topic && echo_payload.as_slice() == payload
                })
                .unwrap_or(false);
            if echo {
                return;
            }
        }
        let payload_text = String::from_utf8_lossy(payload).into_owned();
        let rules = self.config.connections[idx].auto_reply_rules.clone();
        let now = Instant::now();
        let mut rule_idx = None;
        for (i, rule) in rules.iter().enumerate() {
            if !rule_matches(rule, topic, &payload_text) {
                continue;
            }
            // A throttled rule lets the next matching rule take the message.
            let ready = rule.min_interval_secs == 0
                || self
                    .runtime(conn_id)
                    .and_then(|rt| rt.rule_last.get(i).copied().flatten())
                    .map(|last| {
                        now.duration_since(last).as_secs() >= rule.min_interval_secs as u64
                    })
                    .unwrap_or(true);
            if ready {
                rule_idx = Some(i);
                break;
            }
        }
        let Some(rule_idx) = rule_idx else {
            return;
        };
        let rule = self.config.connections[idx].auto_reply_rules[rule_idx].clone();
        let reply_topic = rule.reply_topic.trim().to_string();
        if reply_topic.is_empty() {
            self.push_log(
                conn_id,
                "ERR",
                "",
                i18n::t("回复 Topic 为空", "Reply topic is empty").into_bytes(),
            );
            return;
        }
        let format = PayloadFormat::from_key(&rule.format);
        let reply_bytes = match payload::encode_payload(&rule.reply_payload, format) {
            Ok(bytes) => bytes,
            Err(err) => {
                self.push_log(conn_id, "ERR", &reply_topic, err.into_bytes());
                return;
            }
        };
        let mqtt = self.runtime_mut(conn_id).mqtt.clone();
        match mqtt.publish(
            &reply_topic,
            qos_from_u8(rule.qos.min(2)),
            rule.retain,
            reply_bytes.clone(),
        ) {
            Ok(()) => {
                {
                    let runtime = self.runtime_mut(conn_id);
                    runtime.last_auto_reply = Some((reply_topic.clone(), reply_bytes.clone()));
                    if runtime.rule_hits.len() <= rule_idx {
                        runtime.rule_hits.resize(rule_idx + 1, 0);
                    }
                    if runtime.rule_last.len() <= rule_idx {
                        runtime.rule_last.resize(rule_idx + 1, None);
                    }
                    runtime.rule_hits[rule_idx] = runtime.rule_hits[rule_idx].saturating_add(1);
                    runtime.rule_last[rule_idx] = Some(now);
                }
                self.push_log_meta(
                    conn_id,
                    "SEND",
                    &reply_topic,
                    reply_bytes,
                    LogMeta {
                        qos: Some(rule.qos.min(2)),
                        retain: rule.retain,
                        ..LogMeta::default()
                    },
                );
            }
            Err(err) => self.push_log(conn_id, "ERR", &reply_topic, err.into_bytes()),
        }
    }

    // ---------------------------------------------------------------- logs

    pub fn push_log(&mut self, conn_id: &str, kind: &str, topic: &str, payload: Vec<u8>) {
        self.push_log_meta(conn_id, kind, topic, payload, LogMeta::default());
    }

    /// Variant carrying MQTT metadata; paused capture still drops only RECV/SEND.
    pub fn push_log_meta(
        &mut self,
        conn_id: &str,
        kind: &str,
        topic: &str,
        payload: Vec<u8>,
        meta: LogMeta,
    ) {
        let runtime = self.runtime_mut(conn_id);
        match kind {
            "RECV" => runtime.recv_count = runtime.recv_count.saturating_add(1),
            "SEND" => runtime.send_count = runtime.send_count.saturating_add(1),
            _ => {}
        }
        if runtime.logs_paused && (kind == "RECV" || kind == "SEND") {
            runtime.paused_dropped = runtime.paused_dropped.saturating_add(1);
            return;
        }
        runtime.logs.push_meta(kind, topic, payload, meta);
    }

    /// Whether the active connection is freezing data rows. Transient (not persisted).
    pub fn logs_paused(&self) -> bool {
        self.active_id()
            .and_then(|id| self.runtimes.get(&id).map(|rt| rt.logs_paused))
            .unwrap_or(false)
    }

    pub fn paused_dropped(&self) -> u64 {
        self.active_id()
            .and_then(|id| self.runtimes.get(&id).map(|rt| rt.paused_dropped))
            .unwrap_or(0)
    }

    /// `(received, sent)` messages counted for the active connection.
    pub fn active_counts(&self) -> (u64, u64) {
        self.active_id()
            .and_then(|id| self.runtimes.get(&id))
            .map(|rt| (rt.recv_count, rt.send_count))
            .unwrap_or((0, 0))
    }

    pub fn reset_active_counts(&mut self) {
        let Some(id) = self.active_id() else { return };
        let runtime = self.runtime_mut(&id);
        runtime.recv_count = 0;
        runtime.send_count = 0;
    }

    /// Freeze / resume RECV+SEND capture. SYS and ERR rows are always recorded.
    pub fn toggle_logs_pause(&mut self) -> bool {
        let Some(id) = self.active_id() else { return false };
        let (paused, dropped) = {
            let runtime = self.runtime_mut(&id);
            runtime.logs_paused = !runtime.logs_paused;
            let dropped = runtime.paused_dropped;
            if !runtime.logs_paused {
                runtime.paused_dropped = 0;
            }
            (runtime.logs_paused, dropped)
        };
        let message = if paused {
            i18n::t(
                "日志已暂停：数据报文不再记录，连接与自动回复不受影响",
                "Log capture paused: data messages are dropped; connection and auto reply keep running",
            )
        } else if dropped > 0 {
            i18n::t(
                &format!("日志已恢复，暂停期间丢弃 {dropped} 条"),
                &format!("Log capture resumed; {dropped} messages were dropped while paused"),
            )
        } else {
            i18n::t("日志已恢复", "Log capture resumed")
        };
        self.push_log(&id, "SYS", "", message.into_bytes());
        paused
    }

    /// Apply a new row cap to every live connection and persist the choice.
    pub fn set_log_limit(&mut self, limit: usize) {
        let limit = clamp_log_limit(limit);
        if self.config.log_limit == limit {
            return;
        }
        self.config.log_limit = limit;
        for runtime in self.runtimes.values_mut() {
            runtime.logs.set_limit(limit);
        }
        self.persist();
    }

    /// One row from the active connection, by its stable sequence number.
    pub fn active_log(&self, seq: u64) -> Option<&LogRecord> {
        let id = self.active_id()?;
        self.runtimes.get(&id)?.logs.find(seq)
    }

    /// Most recent SEND row of the active connection (↑ recall, copy last row).
    pub fn last_sent(&self) -> Option<LogRecord> {
        let id = self.active_id()?;
        self.runtimes
            .get(&id)?
            .logs
            .iter()
            .rev()
            .find(|log| log.kind == "SEND")
            .cloned()
    }

    /// Most recent row of any kind, for "copy whole row" without a selection.
    pub fn last_log(&self) -> Option<LogRecord> {
        let id = self.active_id()?;
        self.runtimes.get(&id)?.logs.iter().rev().next().cloned()
    }

    fn push_log_active(&mut self, kind: &str, topic: &str, payload: Vec<u8>) {
        if let Some(id) = self.active_id() {
            self.push_log(&id, kind, topic, payload);
        }
    }

    /// Logs that pass the keyword / kind / subscription filters, oldest first.
    ///
    /// The result is cached incrementally: a new row is tested once, and the
    /// full history is only rescanned when the format / kind / keyword / topic
    /// filters change. Without this, a keyword filter re-rendered and
    /// lowercased every retained payload on each 100 ms tick.
    pub fn visible_logs(&self) -> Vec<&LogRecord> {
        let Some(id) = self.active_id() else {
            return Vec::new();
        };
        let Some(runtime) = self.runtimes.get(&id) else {
            return Vec::new();
        };
        let format = PayloadFormat::from_index(self.prefs.log_format);
        let kind_filter = self.prefs.log_kind_filter;
        let filter = self.prefs.log_filter.trim().to_lowercase();
        let topic = runtime.active_sub_topic.clone();
        let key = ViewKey {
            format: self.prefs.log_format,
            kind_filter,
            filter: filter.clone(),
            topic: topic.clone(),
        };

        let mut cache = runtime.view_cache.borrow_mut();
        if cache.key.as_ref() != Some(&key) {
            cache.key = Some(key);
            cache.seqs.clear();
            cache.scanned_through = None;
        }

        let first = runtime.logs.first_seq();
        let last = runtime.logs.last_seq();
        match first {
            Some(first_seq) => cache.seqs.retain(|&seq| seq >= first_seq),
            None => cache.seqs.clear(),
        }

        if let Some(last_seq) = last {
            let start = cache
                .scanned_through
                .map_or(first.unwrap_or(0), |seq| seq.saturating_add(1))
                .max(first.unwrap_or(0));
            if start <= last_seq {
                let mut matched = Vec::new();
                for seq in start..=last_seq {
                    let Some(log) = runtime.logs.get_by_seq(seq) else {
                        continue;
                    };
                    if log_matches(log, format, &filter, kind_filter, &topic) {
                        matched.push(seq);
                    }
                }
                cache.seqs.extend(matched);
                cache.scanned_through = Some(last_seq);
            }
        } else {
            cache.scanned_through = None;
        }

        let seqs = cache.seqs.clone();
        drop(cache);
        seqs.into_iter()
            .filter_map(|seq| runtime.logs.get_by_seq(seq))
            .collect()
    }

    pub fn clear_logs(&mut self) {
        if let Some(id) = self.active_id() {
            self.runtime_mut(&id).logs.clear();
        }
    }

    /// Save every buffered row of the active connection.
    pub fn save_logs(&mut self) -> Result<PathBuf, String> {
        self.save_logs_scoped(false)
    }

    /// Save only the rows that pass the current keyword / kind / topic filters.
    pub fn save_visible_logs(&mut self) -> Result<PathBuf, String> {
        self.save_logs_scoped(true)
    }

    fn save_logs_scoped(&mut self, only_visible: bool) -> Result<PathBuf, String> {
        let Some(id) = self.active_id() else {
            return Err(i18n::t("当前没有可保存的日志", "No logs to save"));
        };
        let name = self.active().map(|conn| conn.name.clone()).unwrap_or_default();
        let logs: Vec<LogRecord> = if only_visible {
            self.visible_logs().into_iter().cloned().collect()
        } else {
            self.runtimes
                .get(&id)
                .map(|rt| rt.logs.to_vec())
                .unwrap_or_default()
        };
        if logs.is_empty() {
            let err = if only_visible {
                i18n::t(
                    "当前筛选没有日志，清空搜索或切换筛选后再试",
                    "No logs match the filter; clear the search or switch filters",
                )
            } else {
                i18n::t("当前没有可保存的日志", "No logs to save")
            };
            self.push_log(&id, "ERR", "", err.clone().into_bytes());
            return Err(err);
        }
        let format = PayloadFormat::from_index(self.prefs.log_format);
        match export::save_logs_to_file_named(&logs, format, &name) {
            Ok(path) => {
                self.push_log(
                    &id,
                    "SYS",
                    "",
                    i18n::t(
                        &format!("日志已保存到 {}", path.display()),
                        &format!("Logs saved to {}", path.display()),
                    )
                    .into_bytes(),
                );
                Ok(path)
            }
            Err(err) => {
                self.push_log(&id, "ERR", "", err.clone().into_bytes());
                Err(err)
            }
        }
    }
}

/// Reject obviously wrong cert paths before spawning the worker. The TLS builder
/// still parses contents; this only catches typos / removed files.
fn validate_tls_paths(cfg: &ConnectionConfig) -> Result<(), String> {
    if !cfg.ssl {
        return Ok(());
    }
    for (label, path) in [
        (i18n::t("CA 证书", "CA certificate"), &cfg.ssl_ca),
        (i18n::t("客户端证书", "Client certificate"), &cfg.ssl_cert),
        (i18n::t("客户端私钥", "Client key"), &cfg.ssl_key),
    ] {
        let path = path.trim();
        if !path.is_empty() && !Path::new(path).is_file() {
            return Err(i18n::t(
                &format!("{label}文件不存在: {path}"),
                &format!("{label} file not found: {path}"),
            ));
        }
    }
    Ok(())
}

/// `key=value;key2=value2` → MQTT 5 user properties. Bare keys get an empty value.
fn parse_user_properties(text: &str) -> Vec<(String, String)> {
    text.split([';', '\n'])
        .filter_map(|pair| {
            let pair = pair.trim();
            if pair.is_empty() {
                return None;
            }
            let (key, value) = pair.split_once('=').unwrap_or((pair, ""));
            let key = key.trim();
            if key.is_empty() {
                None
            } else {
                Some((key.to_string(), value.trim().to_string()))
            }
        })
        .collect()
}

fn publish_options(props: &str, expiry: u32) -> PublishOptions {
    PublishOptions {
        user_properties: parse_user_properties(props),
        expiry: (expiry > 0).then_some(expiry),
    }
}

/// First matching enabled rule wins.
pub fn select_auto_reply_rule(
    rules: &[AutoReplyRule],
    topic: &str,
    payload_text: &str,
) -> Option<usize> {
    rules
        .iter()
        .enumerate()
        .find(|(_, rule)| rule_matches(rule, topic, payload_text))
        .map(|(i, _)| i)
}

pub fn rule_matches(rule: &AutoReplyRule, topic: &str, payload_text: &str) -> bool {
    if !rule.enabled {
        return false;
    }
    let match_topic = rule.match_topic.trim();
    if match_topic.is_empty() {
        return false;
    }
    if !crate::filter::topic_matches_filter(topic, match_topic) {
        return false;
    }
    let needle = rule.payload_contains.as_str();
    needle.is_empty() || payload_text.contains(needle)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::PublishPreset;

    fn runtime_with_conn() -> AppRuntime {
        let mut config = AppConfig::default();
        let conn = ConnectionConfig::new("local".into());
        config.active_id = conn.id.clone();
        config.connections.push(conn);
        AppRuntime::new(Some(config), None, None)
    }

    fn rule(enabled: bool, topic: &str, needle: &str) -> AutoReplyRule {
        AutoReplyRule {
            enabled,
            match_topic: topic.into(),
            payload_contains: needle.into(),
            reply_topic: "out/x".into(),
            reply_payload: "pong".into(),
            ..AutoReplyRule::default()
        }
    }

    fn subscribe(rt: &mut AppRuntime, filters: &[(&str, u8)]) {
        rt.config.connections[0].subscriptions = filters
            .iter()
            .map(|(topic, qos)| SubscriptionConfig {
                topic: (*topic).into(),
                qos: *qos,
                ..SubscriptionConfig::default()
            })
            .collect();
    }

    fn recv(rt: &mut AppRuntime, topic: &str, payload: &[u8], qos: u8) {
        let id = rt.active_id().unwrap();
        rt.handle_mqtt_event(
            &id,
            UiEvent::Recv {
                topic: topic.into(),
                payload: payload.to_vec(),
                qos,
                retain: false,
                dup: false,
                props: Vec::new(),
                expiry: None,
            },
        );
    }

    #[test]
    fn overlapping_subscriptions_fold_broker_copies() {
        let mut rt = runtime_with_conn();
        subscribe(&mut rt, &[("a/#", 0), ("a/b", 1)]);

        recv(&mut rt, "a/b", b"x", 0);
        // The copy for the QoS 1 filter differs only in QoS; it still folds.
        recv(&mut rt, "a/b", b"x", 1);
        assert_eq!(rt.visible_logs().len(), 1);
        assert_eq!(rt.active_counts(), (1, 0));

        // A different payload opens a new row with its own single copy.
        recv(&mut rt, "a/b", b"y", 0);
        recv(&mut rt, "a/b", b"y", 1);
        assert_eq!(rt.visible_logs().len(), 2);
        assert_eq!(rt.active_counts(), (2, 0));
    }

    #[test]
    fn three_matching_filters_fold_two_copies() {
        let mut rt = runtime_with_conn();
        subscribe(&mut rt, &[("a/#", 0), ("a/+", 0), ("a/b", 0)]);
        recv(&mut rt, "a/b", b"x", 0);
        recv(&mut rt, "a/b", b"x", 0);
        recv(&mut rt, "a/b", b"x", 0);
        assert_eq!(rt.visible_logs().len(), 1);
        assert_eq!(rt.active_counts(), (1, 0));
    }

    #[test]
    fn single_subscription_keeps_identical_publishes() {
        let mut rt = runtime_with_conn();
        subscribe(&mut rt, &[("a/#", 0)]);
        recv(&mut rt, "a/b", b"x", 0);
        recv(&mut rt, "a/b", b"x", 0);
        assert_eq!(rt.visible_logs().len(), 2);
        assert_eq!(rt.active_counts(), (2, 0));
    }

    #[test]
    fn stale_copy_budget_expires() {
        let mut rt = runtime_with_conn();
        subscribe(&mut rt, &[("a/#", 0), ("a/b", 0)]);
        recv(&mut rt, "a/b", b"x", 0);

        let id = rt.active_id().unwrap();
        for (at, _) in rt
            .runtimes
            .get_mut(&id)
            .unwrap()
            .recv_fold
            .pending
            .values_mut()
        {
            *at = Instant::now() - Duration::from_secs(1);
        }
        recv(&mut rt, "a/b", b"x", 0);
        assert_eq!(rt.active_counts(), (2, 0));
    }

    #[test]
    fn auto_reply_selects_first_enabled_match() {
        let rules = vec![
            rule(false, "dev/#", ""),
            rule(true, "dev/in", ""),
            rule(true, "dev/#", "ping"),
        ];
        assert_eq!(select_auto_reply_rule(&rules, "dev/in", "hello"), Some(1));
        assert_eq!(select_auto_reply_rule(&rules, "dev/other", "ping"), Some(2));
        assert_eq!(select_auto_reply_rule(&rules, "dev/other", "nope"), None);
        assert_eq!(select_auto_reply_rule(&[], "a", "b"), None);
    }

    #[test]
    fn reconnect_backoff_doubles_then_caps() {
        assert_eq!(reconnect_delay(0), Duration::from_secs(1));
        assert_eq!(reconnect_delay(1), Duration::from_secs(1));
        assert_eq!(reconnect_delay(2), Duration::from_secs(2));
        assert_eq!(reconnect_delay(3), Duration::from_secs(4));
        assert_eq!(reconnect_delay(4), Duration::from_secs(8));
        assert_eq!(reconnect_delay(5), Duration::from_secs(16));
        assert_eq!(reconnect_delay(6), Duration::from_secs(30));
        assert_eq!(reconnect_delay(64), Duration::from_secs(30));
    }

    #[test]
    fn user_properties_parse_repeated_and_blank_keys() {
        assert_eq!(parse_user_properties(""), Vec::new());
        assert_eq!(parse_user_properties("a=1;;b=2;flag"), vec![
            ("a".into(), "1".into()),
            ("b".into(), "2".into()),
            ("flag".into(), String::new()),
        ]);
        let opts = publish_options("k=v", 0);
        assert!(!opts.is_empty());
        assert_eq!(opts.expiry, None);
        assert_eq!(publish_options("", 60).expiry, Some(60));
        assert!(publish_options("", 0).is_empty());
    }

    #[test]
    fn empty_rule_is_not_a_match() {
        let r = rule(true, "", "");
        assert!(!rule_matches(&r, "a/b", "x"));
        let mut r = rule(true, "a/#", "needle");
        assert!(rule_matches(&r, "a/b", "has needle"));
        r.payload_contains.clear();
        assert!(rule_matches(&r, "a/b", "anything"));
    }

    #[test]
    fn ring_stays_capped_through_runtime() {
        let mut rt = runtime_with_conn();
        let id = rt.active_id().unwrap();
        for i in 0..230 {
            rt.push_log(&id, "SYS", "", format!("{i}").into_bytes());
        }
        assert_eq!(rt.visible_logs().len(), crate::model::MAX_LOGS);
    }

    #[test]
    fn filters_apply_to_visible_logs() {
        let mut rt = runtime_with_conn();
        let id = rt.active_id().unwrap();
        rt.push_log(&id, "RECV", "a/b", b"hello".to_vec());
        rt.push_log(&id, "SEND", "c/d", b"world".to_vec());
        rt.set_log_kind_filter(1);
        assert_eq!(rt.visible_logs().len(), 1);
        rt.set_log_kind_filter(0);
        rt.set_log_filter("world".into());
        assert_eq!(rt.visible_logs().len(), 1);
        rt.set_log_filter(String::new());
        rt.toggle_sub_filter("a/#");
        assert_eq!(rt.visible_logs().len(), 1);
        rt.clear_sub_filter();
        assert_eq!(rt.visible_logs().len(), 2);
        rt.toggle_sub_filter("a/#");
        rt.toggle_sub_filter("a/#");
        assert_eq!(rt.visible_logs().len(), 2);
    }

    #[test]
    fn visible_cache_tracks_appends_and_eviction() {
        let mut rt = runtime_with_conn();
        let id = rt.active_id().unwrap();
        rt.set_log_limit(4);
        for (kind, payload) in [
            ("RECV", "match-1"),
            ("RECV", "other"),
            ("RECV", "match-2"),
        ] {
            rt.push_log(&id, kind, "t", payload.as_bytes().to_vec());
        }
        rt.set_log_filter("match".into());
        assert_eq!(rt.visible_logs().len(), 2);

        // A cached view must notice rows appended after the last scan.
        rt.push_log(&id, "RECV", "t", b"match-3".to_vec());
        assert_eq!(rt.visible_logs().len(), 3);

        // The store clamps the limit to 50, so fill it before forcing eviction.
        let limit = rt.config.log_limit;
        for i in 0..limit - 4 {
            rt.push_log(&id, "RECV", "t", format!("filler-{i}").into_bytes());
        }
        assert_eq!(rt.visible_logs().len(), 3);

        // Eviction only drops from the front; cached seqs below it disappear.
        rt.push_log(&id, "RECV", "t", b"other-2".to_vec());
        let logs = rt.visible_logs();
        assert_eq!(logs.len(), 2);
        assert_eq!(logs[0].payload.as_ref(), b"match-2");
        assert_eq!(logs[1].payload.as_ref(), b"match-3");

        rt.push_log(&id, "RECV", "t", b"match-4".to_vec());
        let logs = rt.visible_logs();
        assert_eq!(logs.len(), 3);
        assert_eq!(logs[2].payload.as_ref(), b"match-4");
    }

    #[test]
    fn switching_connection_saves_and_loads_form() {
        let mut rt = runtime_with_conn();
        let first = rt.active_id().unwrap();
        rt.form.host = "first".into();
        rt.new_connection();
        let second = rt.active_id().unwrap();
        assert_ne!(first, second);
        assert_eq!(rt.form.host, "127.0.0.1");
        assert!(rt.select_connection(&first));
        assert_eq!(rt.form.host, "first");
        assert!(!rt.select_connection(&first));
    }

    #[test]
    fn deleting_last_connection_creates_local() {
        let mut rt = runtime_with_conn();
        let id = rt.active_id().unwrap();
        rt.delete_connection(&id);
        assert_eq!(rt.config.connections.len(), 1);
        assert_ne!(rt.active_id().unwrap(), id);
    }

    #[test]
    fn publish_without_connection_logs_error() {
        let mut rt = runtime_with_conn();
        assert!(!rt.publish_form());
        let logs = rt.visible_logs();
        assert!(logs.iter().any(|log| log.kind == "ERR"));
    }

    #[test]
    fn preset_publish_without_connection_logs_error() {
        let mut rt = runtime_with_conn();
        let idx = rt.active_index().unwrap();
        rt.config.connections[idx]
            .publish_presets
            .push(PublishPreset {
                name: "p".into(),
                topic: "t".into(),
                payload: "x".into(),
                ..PublishPreset::default()
            });
        assert!(!rt.publish_preset(0));
        assert!(!rt.publish_preset(9));
    }

    #[test]
    fn prefs_apply_and_round_trip() {
        let mut rt = runtime_with_conn();
        rt.set_log_format(3);
        assert_eq!(rt.config.log_format, "hex");
        rt.set_auto_scroll(false);
        rt.set_log_filter("abc".into());
        rt.set_log_kind_filter(2);
        rt.set_skin_choice(4);
        assert_eq!(rt.config.skin, "green");
        rt.set_language(1);
        assert_eq!(rt.config.language, "en");
        assert!(i18n::is_en());
        rt.set_language(0);

        // Custom palette: switching seeds from the skin just shown, then single
        // slots can be overridden without touching the rest.
        rt.set_skin_choice(Skin::Custom.to_index());
        assert_eq!(rt.skin(), Skin::Custom);
        assert_eq!(
            PaletteField::WindowBg.get(&rt.palette()),
            PaletteField::WindowBg.get(theme::palette(Skin::Green))
        );
        rt.set_custom_color(PaletteField::WindowBg, "#123456".into());
        assert_eq!(rt.custom_color_text(PaletteField::WindowBg), "#123456");
        assert_eq!(
            PaletteField::WindowBg.get(&rt.palette()),
            (0x12, 0x34, 0x56)
        );
        assert_eq!(
            rt.custom_color_text(PaletteField::PanelBg),
            theme::to_hex(PaletteField::PanelBg.get(theme::palette(Skin::Green)))
        );
        // Invalid text stays in the box but resolves to the base color.
        rt.set_custom_color(PaletteField::WindowBg, "#12".into());
        assert_eq!(rt.custom_color_text(PaletteField::WindowBg), "#12");
        assert_eq!(
            PaletteField::WindowBg.get(&rt.palette()),
            PaletteField::WindowBg.get(theme::palette(Skin::Green))
        );
    }

    #[test]
    fn save_logs_requires_entries() {
        let mut rt = runtime_with_conn();
        assert!(rt.save_logs().is_err());
        // The failed attempt appended an ERR row; clear it before checking the
        // filtered export path (otherwise that row is visible and gets saved).
        rt.clear_logs();
        assert!(rt.save_visible_logs().is_err());
    }

    #[test]
    fn pause_drops_data_rows_but_keeps_system_rows() {
        let mut rt = runtime_with_conn();
        let id = rt.active_id().unwrap();
        assert!(!rt.logs_paused());

        assert!(rt.toggle_logs_pause());
        assert!(rt.logs_paused());
        rt.push_log(&id, "RECV", "a/b", b"recv".to_vec());
        rt.push_log(&id, "SEND", "a/b", b"send".to_vec());
        rt.push_log(&id, "SYS", "", b"system".to_vec());
        assert_eq!(rt.paused_dropped(), 2);
        assert!(rt
            .visible_logs()
            .iter()
            .all(|log| log.kind == "SYS"));

        assert!(!rt.toggle_logs_pause());
        assert!(!rt.logs_paused());
        assert_eq!(rt.paused_dropped(), 0);
        rt.push_log(&id, "RECV", "a/b", b"after".to_vec());
        assert_eq!(
            rt.visible_logs().iter().filter(|log| log.kind == "RECV").count(),
            1
        );
    }

    #[test]
    fn traffic_counters_keep_counting_while_paused() {
        let mut rt = runtime_with_conn();
        let id = rt.active_id().unwrap();
        rt.push_log(&id, "RECV", "a/b", b"1".to_vec());
        rt.push_log(&id, "SEND", "a/b", b"2".to_vec());
        rt.push_log(&id, "RECV", "a/b", b"3".to_vec());
        rt.push_log(&id, "SYS", "", b"system".to_vec());
        assert_eq!(rt.active_counts(), (2, 1));

        rt.toggle_logs_pause();
        rt.push_log(&id, "RECV", "a/b", b"paused".to_vec());
        rt.push_log(&id, "SEND", "a/b", b"paused".to_vec());
        assert_eq!(rt.active_counts(), (3, 2));
        assert_eq!(rt.paused_dropped(), 2);

        rt.reset_active_counts();
        assert_eq!(rt.active_counts(), (0, 0));
    }

    #[test]
    fn log_limit_resizes_existing_buffers() {
        let mut rt = runtime_with_conn();
        let id = rt.active_id().unwrap();
        for i in 0..300 {
            rt.push_log(&id, "SYS", "", format!("{i}").into_bytes());
        }
        assert_eq!(rt.visible_logs().len(), crate::model::MAX_LOGS);
        rt.set_log_limit(100);
        assert_eq!(rt.config.log_limit, 100);
        assert_eq!(rt.visible_logs().len(), 100);
        assert_eq!(
            rt.visible_logs().last().unwrap().payload.as_ref(),
            b"299"
        );
    }

    #[test]
    fn active_log_lookup_is_scoped_to_the_active_connection() {
        let mut rt = runtime_with_conn();
        let first = rt.active_id().unwrap();
        rt.push_log(&first, "RECV", "a", b"one".to_vec());
        let seq = rt.visible_logs()[0].seq;
        assert_eq!(rt.active_log(seq).unwrap().topic, "a");
        rt.new_connection();
        assert!(rt.active_log(seq).is_none());
    }
}
