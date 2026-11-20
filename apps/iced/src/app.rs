//! mqttc · Iced shell.
//!
//! All business state lives in `mqttc-core`'s `AppRuntime`; this module maps
//! widget messages onto runtime actions. The sidebar holds connection/settings
//! pages; subscription, auto-reply and preset panels live in the session tabs.

use std::cell::Cell;
use std::fmt;
use std::time::{Duration, Instant};

use iced::widget::{
    button, checkbox, column, container, pick_list, row, scrollable, text, text_editor,
    text_input, Space,
};
use iced::{Alignment, Length, Task};
use mqttc_core::cli::BenchOptions;
use mqttc_core::form::FormState;
use mqttc_core::i18n::{is_en, t};
use mqttc_core::model::{default_client_id, ConnStatus, LogRecord, PayloadFormat, Skin};
use mqttc_core::payload;
use mqttc_core::runtime::AppRuntime;

use crate::style::{rgb, ButtonKind, Colors};

type El<'a> = iced::Element<'a, Msg>;

// ---------------------------------------------------------------- fields

/// Every single-line string in `FormState` that a text input can edit.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Field {
    Name,
    Host,
    Port,
    ClientId,
    Username,
    Password,
    KeepAlive,
    Timeout,
    SubTopic,
    WillTopic,
    WillPayload,
    PubTopic,
    TimedInterval,
    Ca,
    Cert,
    Key,
}

impl Field {
    fn get(self, f: &FormState) -> &str {
        match self {
            Self::Name => &f.name,
            Self::Host => &f.host,
            Self::Port => &f.port,
            Self::ClientId => &f.client_id,
            Self::Username => &f.username,
            Self::Password => &f.password,
            Self::KeepAlive => &f.keep_alive,
            Self::Timeout => &f.connect_timeout,
            Self::SubTopic => &f.sub_topic,
            Self::WillTopic => &f.will_topic,
            Self::WillPayload => &f.will_payload,
            Self::PubTopic => &f.pub_topic,
            Self::TimedInterval => &f.timed_interval,
            Self::Ca => &f.ssl_ca,
            Self::Cert => &f.ssl_cert,
            Self::Key => &f.ssl_key,
        }
    }

    fn set(self, f: &mut FormState, value: String) {
        match self {
            Self::Name => f.name = value,
            Self::Host => f.host = value,
            Self::Port => f.port = value,
            Self::ClientId => f.client_id = value,
            Self::Username => f.username = value,
            Self::Password => f.password = value,
            Self::KeepAlive => f.keep_alive = value,
            Self::Timeout => f.connect_timeout = value,
            Self::SubTopic => f.sub_topic = value,
            Self::WillTopic => f.will_topic = value,
            Self::WillPayload => f.will_payload = value,
            Self::PubTopic => f.pub_topic = value,
            Self::TimedInterval => f.timed_interval = value,
            Self::Ca => f.ssl_ca = value,
            Self::Cert => f.ssl_cert = value,
            Self::Key => f.ssl_key = value,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Num {
    Protocol,
    SubQos,
    PubQos,
    PubFormat,
    WillQos,
    Echo,
}

impl Num {
    fn set(self, f: &mut FormState, value: i32) {
        match self {
            Self::Protocol => f.protocol = value,
            Self::SubQos => f.sub_qos = value,
            Self::PubQos => f.pub_qos = value,
            Self::PubFormat => f.pub_format = value,
            Self::WillQos => f.will_qos = value,
            Self::Echo => f.auto_reply_echo = value,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Flag {
    CleanSession,
    Ssl,
    SslVerify,
    WillRetain,
    PubRetain,
}

impl Flag {
    fn set(self, f: &mut FormState, value: bool) {
        match self {
            Self::CleanSession => f.clean_session = value,
            Self::Ssl => f.ssl = value,
            Self::SslVerify => f.ssl_verify = value,
            Self::WillRetain => f.will_retain = value,
            Self::PubRetain => f.pub_retain = value,
        }
    }
}

#[derive(Debug, Clone)]
pub enum Msg {
    Select(String),
    New,
    Delete(String),
    Page(i32),
    SessionTab(SessionTab),
    Connect,
    Disconnect,
    Subscribe,
    Unsubscribe(usize),
    ToggleSub(String),
    Publish,
    PublishPreset(usize),
    Format,
    Timed,
    GenId,
    SaveLogs,
    ClearLogs,
    Skin(i32),
    Language(i32),
    AddRule,
    DelRule(usize),
    RuleEnabled(usize, bool),
    RuleRetain(usize, bool),
    RuleNum(usize, i32),
    RuleFormat(usize, i32),
    RuleText(usize, RuleTextField, String),
    AddPreset,
    DelPreset(usize),
    PresetRetain(usize, bool),
    PresetNum(usize, i32),
    PresetFormat(usize, i32),
    PresetText(usize, PresetTextField, String),
    LogFilter(String),
    LogKind(i32),
    LogFormat(i32),
    AutoScroll(bool),
    Field(Field, String),
    Num(Num, i32),
    Flag(Flag, bool),
    PubEdit(text_editor::Action),
    Tick,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RuleTextField {
    MatchTopic,
    Contains,
    ReplyTopic,
    ReplyPayload,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PresetTextField {
    Name,
    Topic,
    Payload,
}

/// Connection-scoped panels shown below the session header.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SessionTab {
    Logs,
    Subs,
    Rules,
    Presets,
}

// ---------------------------------------------------------------- options

#[derive(Clone, Copy, PartialEq, Eq)]
struct Opt {
    value: i32,
    label: &'static str,
}

impl fmt::Display for Opt {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.label)
    }
}

fn qos_opts() -> Vec<Opt> {
    vec![
        Opt { value: 0, label: "QoS 0" },
        Opt { value: 1, label: "QoS 1" },
        Opt { value: 2, label: "QoS 2" },
    ]
}

fn format_opts() -> Vec<Opt> {
    PayloadFormat::ALL
        .iter()
        .map(|f| {
            let label = match f {
                PayloadFormat::Text => "Text",
                PayloadFormat::Json => "JSON",
                PayloadFormat::Base64 => "Base64",
                PayloadFormat::Hex => "Hex",
            };
            Opt { value: f.to_index(), label }
        })
        .collect()
}

fn proto_opts() -> Vec<Opt> {
    vec![
        Opt { value: 0, label: "3.1.1" },
        Opt { value: 1, label: "5.0" },
    ]
}

fn skin_opts() -> Vec<Opt> {
    Skin::ALL
        .iter()
        .map(|s| Opt {
            value: s.to_index(),
            label: match s {
                Skin::System => "跟随系统",
                Skin::Black => "黑",
                Skin::White => "白",
                Skin::Blue => "蓝",
                Skin::Green => "绿",
                Skin::Purple => "紫",
            },
        })
        .collect()
}

fn lang_opts() -> Vec<Opt> {
    vec![
        Opt { value: 0, label: "中文" },
        Opt { value: 1, label: "English" },
    ]
}

fn kind_opts() -> Vec<Opt> {
    vec![
        Opt { value: 0, label: "全部" },
        Opt { value: 1, label: "收" },
        Opt { value: 2, label: "发" },
        Opt { value: 3, label: "系统" },
        Opt { value: 4, label: "错误" },
    ]
}

fn echo_opts() -> Vec<Opt> {
    vec![
        Opt { value: 0, label: "显示" },
        Opt { value: 1, label: "隐藏" },
        Opt { value: 2, label: "跳过" },
    ]
}

fn pick<'a, F>(c: &Colors, options: &[Opt], current: i32, on: F) -> El<'a>
where
    F: Fn(i32) -> Msg + 'a,
{
    let selected = options.iter().copied().find(|o| o.value == current);
    pick_list(options.to_vec(), selected, move |o: Opt| on(o.value))
        .text_size(13.0)
        .padding([6, 8])
        .style(c.pick_list_style())
        .into()
}

// ---------------------------------------------------------------- app

struct Bench {
    options: BenchOptions,
    quit_at: Option<Instant>,
    timed_started: bool,
    finished: bool,
    announced: bool,
}

pub struct App {
    rt: AppRuntime,
    page: i32,
    session_tab: SessionTab,
    editor: text_editor::Content,
    started: Instant,
    first_frame_ms: Cell<Option<f64>>,
    bench: Option<Bench>,
    /// UI-side debounce; set on every edit so we only tick briefly after typing.
    dirty_at: Option<Instant>,
}

impl App {
    pub fn new() -> Self {
        let options = mqttc_core::cli::parse(std::env::args().skip(1));
        let rt = AppRuntime::load(&options, None);
        let editor = text_editor::Content::with_text(&rt.form.pub_payload);
        let started = Instant::now();
        let bench = options.bench.then(|| Bench {
            quit_at: options
                .quit_after
                .map(|secs| started + Duration::from_secs(secs)),
            options,
            timed_started: false,
            finished: false,
            announced: false,
        });
        Self {
            rt,
            page: 0,
            session_tab: SessionTab::Logs,
            editor,
            started,
            first_frame_ms: Cell::new(None),
            bench,
            dirty_at: None,
        }
    }

    pub fn theme(&self) -> iced::Theme {
        Colors::from_palette(self.rt.palette()).theme()
    }

    pub fn subscription(&self) -> iced::Subscription<Msg> {
        let busy = self.bench.is_some()
            || self.rt.timed_running()
            || self
                .rt
                .active_id()
                .map(|id| self.rt.status_of(&id) != ConnStatus::Disconnected)
                .unwrap_or(false);
        let every = if busy || self.dirty_at.is_some() {
            Some(Duration::from_millis(100))
        } else if self.rt.prefs.skin_choice == 0 {
            Some(Duration::from_secs(1))
        } else {
            None
        };
        match every {
            Some(period) => iced::time::every(period).map(|_| Msg::Tick),
            None => iced::Subscription::none(),
        }
    }

    fn touched(&mut self) -> Task<Msg> {
        self.dirty_at = Some(Instant::now() + Duration::from_millis(420));
        self.rt.mark_dirty();
        Task::none()
    }

    fn reload_form(&mut self) {
        self.rt.apply_form();
        self.editor = text_editor::Content::with_text(&self.rt.form.pub_payload);
    }

    pub fn update(&mut self, msg: Msg) -> Task<Msg> {
        match msg {
            Msg::Tick => return self.on_tick(),
            Msg::Select(id) => {
                if self.rt.select_connection(&id) {
                    self.page = 1;
                    self.session_tab = SessionTab::Logs;
                    self.reload_form();
                }
            }
            Msg::New => {
                self.rt.new_connection();
                self.page = 0;
                self.rt.persist();
                self.reload_form();
            }
            Msg::Delete(id) => {
                self.rt.delete_connection(&id);
                self.page = 0;
                self.rt.persist();
                self.reload_form();
            }
            Msg::Page(p) => {
                if p == 1 {
                    self.reload_form();
                    self.rt.persist();
                }
                self.page = p;
            }
            Msg::SessionTab(tab) => self.session_tab = tab,
            Msg::Connect => {
                self.rt.apply_form();
                if self.rt.connect() {
                    self.page = 1;
                    self.session_tab = SessionTab::Logs;
                }
            }
            Msg::Disconnect => self.rt.disconnect(),
            Msg::Subscribe => {
                self.rt.apply_form();
                self.rt.subscribe();
            }
            Msg::Unsubscribe(index) => self.rt.unsubscribe(index),
            Msg::ToggleSub(topic) => self.rt.toggle_sub_filter(&topic),
            Msg::Publish => {
                self.rt.apply_form();
                self.rt.publish_form();
            }
            Msg::PublishPreset(index) => {
                self.rt.apply_form();
                self.rt.publish_preset(index);
            }
            Msg::Format => {
                let _ = self.rt.format_payload();
                self.editor = text_editor::Content::with_text(&self.rt.form.pub_payload);
            }
            Msg::Timed => self.rt.toggle_timed(),
            Msg::GenId => {
                self.rt.form.client_id = default_client_id();
                return self.touched();
            }
            Msg::SaveLogs => {
                let _ = self.rt.save_logs();
            }
            Msg::ClearLogs => self.rt.clear_logs(),
            Msg::Skin(choice) => self.rt.set_skin_choice(choice),
            Msg::Language(index) => self.rt.set_language(index),
            Msg::AddRule => {
                self.rt.form.add_rule();
                self.rt.apply_form();
                self.rt.persist();
            }
            Msg::DelRule(index) => {
                self.rt.form.remove_rule(index);
                self.rt.apply_form();
                self.rt.persist();
            }
            Msg::RuleEnabled(index, on) => {
                if let Some(rule) = self.rt.form.rules.get_mut(index) {
                    rule.enabled = on;
                }
                self.rt.apply_form();
            }
            Msg::RuleRetain(index, on) => {
                if let Some(rule) = self.rt.form.rules.get_mut(index) {
                    rule.retain = on;
                }
                self.rt.apply_form();
            }
            Msg::RuleNum(index, value) => {
                if let Some(rule) = self.rt.form.rules.get_mut(index) {
                    rule.qos = value;
                }
                self.rt.apply_form();
            }
            Msg::RuleFormat(index, value) => {
                if let Some(rule) = self.rt.form.rules.get_mut(index) {
                    rule.format = value;
                }
                self.rt.apply_form();
            }
            Msg::RuleText(index, field, value) => {
                if let Some(rule) = self.rt.form.rules.get_mut(index) {
                    match field {
                        RuleTextField::MatchTopic => rule.match_topic = value,
                        RuleTextField::Contains => rule.payload_contains = value,
                        RuleTextField::ReplyTopic => rule.reply_topic = value,
                        RuleTextField::ReplyPayload => rule.reply_payload = value,
                    }
                }
                self.rt.apply_form();
            }
            Msg::AddPreset => {
                self.rt.form.add_preset();
                self.rt.apply_form();
                self.rt.persist();
            }
            Msg::DelPreset(index) => {
                self.rt.form.remove_preset(index);
                self.rt.apply_form();
                self.rt.persist();
            }
            Msg::PresetRetain(index, on) => {
                if let Some(preset) = self.rt.form.presets.get_mut(index) {
                    preset.retain = on;
                }
                self.rt.apply_form();
            }
            Msg::PresetNum(index, value) => {
                if let Some(preset) = self.rt.form.presets.get_mut(index) {
                    preset.qos = value;
                }
                self.rt.apply_form();
            }
            Msg::PresetFormat(index, value) => {
                if let Some(preset) = self.rt.form.presets.get_mut(index) {
                    preset.format = value;
                }
                self.rt.apply_form();
            }
            Msg::PresetText(index, field, value) => {
                if let Some(preset) = self.rt.form.presets.get_mut(index) {
                    match field {
                        PresetTextField::Name => preset.name = value,
                        PresetTextField::Topic => preset.topic = value,
                        PresetTextField::Payload => preset.payload = value,
                    }
                }
                self.rt.apply_form();
            }
            Msg::LogFilter(value) => self.rt.set_log_filter(value),
            Msg::LogKind(value) => self.rt.set_log_kind_filter(value),
            Msg::LogFormat(value) => self.rt.set_log_format(value),
            Msg::AutoScroll(on) => self.rt.set_auto_scroll(on),
            Msg::Field(field, value) => {
                field.set(&mut self.rt.form, value);
                return self.touched();
            }
            Msg::Num(field, value) => {
                field.set(&mut self.rt.form, value);
                return self.touched();
            }
            Msg::Flag(flag, on) => {
                flag.set(&mut self.rt.form, on);
                return self.touched();
            }
            Msg::PubEdit(action) => {
                let is_edit = action.is_edit();
                self.editor.perform(action);
                if is_edit {
                    self.rt.form.pub_payload = self.editor.text();
                    return self.touched();
                }
            }
        }
        self.rt.maybe_persist();
        Task::none()
    }

    fn on_tick(&mut self) -> Task<Msg> {
        let summary = self.rt.poll();
        if let Some(page) = summary.page_request {
            self.page = page;
            self.reload_form();
        }

        if let Some(at) = self.dirty_at {
            if Instant::now() >= at {
                self.dirty_at = None;
                self.rt.apply_form();
                self.rt.persist();
            }
        }
        self.rt.maybe_persist();

        if self.rt.prefs.skin_choice == 0 && self.rt.refresh_system_skin() {
            // Palette changed; nothing else to do until the next frame.
        }

        let Some(bench) = self.bench.as_mut() else {
            return Task::none();
        };

        if !bench.announced {
            bench.announced = true;
            if bench.options.auto_connect() {
                self.rt.apply_form();
                self.rt.connect();
            }
            println!(
                "BENCH scenario={} connect={} timed={}",
                bench.options.scenario.as_deref().unwrap_or("s1"),
                bench.options.auto_connect(),
                bench.options.wants_timed()
            );
        }

        if bench.options.wants_timed() && !bench.timed_started {
            let connected = self
                .rt
                .active_id()
                .map(|id| self.rt.is_connected(&id))
                .unwrap_or(false);
            if connected {
                self.rt.toggle_timed();
                bench.timed_started = true;
            }
        }

        if !bench.finished {
            if let Some(quit_at) = bench.quit_at {
                if Instant::now() >= quit_at {
                    bench.finished = true;
                    let logs = self.rt.visible_logs();
                    let recv = logs.iter().filter(|log| log.kind == "RECV").count();
                    let send = logs.iter().filter(|log| log.kind == "SEND").count();
                    println!(
                        "BENCH_SUMMARY logs={} recv={} send={} status={:?}",
                        logs.len(),
                        recv,
                        send,
                        self.rt.status()
                    );
                    println!(
                        "BENCH_QUIT {:.2}",
                        self.started.elapsed().as_secs_f64()
                    );
                    return iced::exit();
                }
            }
        }
        Task::none()
    }

    // ------------------------------------------------------------ view

    pub fn view(&self) -> El<'_> {
        if self.first_frame_ms.get().is_none() {
            let ms = self.started.elapsed().as_secs_f64() * 1000.0;
            self.first_frame_ms.set(Some(ms));
            println!("FIRST_FRAME_MS {ms:.1}");
        }
        let c = Colors::from_palette(self.rt.palette());
        row![self.sidebar(&c), self.main_area(&c)]
            .spacing(12)
            .padding(12)
            .width(Length::Fill)
            .height(Length::Fill)
            .into()
    }

    fn sidebar(&self, c: &Colors) -> El<'_> {
        let mut list = column![].spacing(3);
        let active = self.rt.active_id();
        for conn in &self.rt.config.connections {
            let status = self.rt.status_of(&conn.id);
            let is_active = active.as_deref() == Some(conn.id.as_str());
            let dot = text("●").size(9.0).color(rgb(status.rgb()));
            let name = text(conn.name.as_str())
                .size(13.0)
                .color(if is_active { c.text_primary } else { c.text_body });
            let label = row![dot, name]
                .spacing(6)
                .align_y(Alignment::Center)
                .width(Length::Fill);
            let pick = button(label)
                .width(Length::Fill)
                .padding([7, 8])
                .style(c.button_kind(if is_active {
                    ButtonKind::Selected
                } else {
                    ButtonKind::Ghost
                }))
                .on_press(Msg::Select(conn.id.clone()));
            let del = button(text("×").size(14.0))
                .padding([4, 6])
                .style(c.danger_button())
                .on_press(Msg::Delete(conn.id.clone()));
            list = list.push(row![pick, del].spacing(2));
        }

        let nav = [
            (0, t("连接编辑", "Connection")),
            (1, t("会话", "Session")),
            (2, t("设置", "Settings")),
            (5, t("关于", "About")),
        ];
        let mut links = column![].spacing(2);
        for (page, label) in nav {
            links = links.push(
                button(text(label).size(13.0))
                    .width(Length::Fill)
                    .padding([6, 8])
                    .style(c.button_kind(if self.page == page {
                        ButtonKind::Selected
                    } else {
                        ButtonKind::Ghost
                    }))
                    .on_press(Msg::Page(page)),
            );
        }

        container(
            column![
                text("mqttc").size(20.0).color(c.text_title),
                text("iced · MQTT debug client").size(11.0).color(c.text_faint),
                Space::new().height(10),
                scrollable(list).height(Length::FillPortion(3)),
                button(text(format!("＋ {}", t("新建连接", "New connection"))).size(13.0))
                    .width(Length::Fill)
                    .padding([6, 8])
                    .style(c.ghost_button())
                    .on_press(Msg::New),
                Space::new().height(8),
                column![
                    text(t("页面", "Pages")).size(11.0).color(c.text_faint),
                    links
                ]
                .spacing(4),
            ]
            .spacing(6)
            .padding(12)
            .height(Length::Fill),
        )
        .width(216)
        .height(Length::Fill)
        .style(c.card())
        .into()
    }

    fn main_area(&self, c: &Colors) -> El<'_> {
        let body: El<'_> = match self.page {
            0 => self.page_editor(c),
            1 => self.page_session(c),
            2 => self.page_settings(c),
            _ => self.page_about(c),
        };
        container(body)
            .width(Length::Fill)
            .height(Length::Fill)
            .into()
    }

    fn section(&self, c: &Colors, title: &str) -> El<'static> {
        text(title.to_string()).size(12.0).color(c.text_muted).into()
    }

    fn hint(&self, c: &Colors, text_value: String) -> El<'static> {
        text(text_value).size(11.0).color(c.text_faint).into()
    }

    fn input(&self, c: &Colors, field: Field, placeholder: &'static str) -> El<'_> {
        text_input(placeholder, field.get(&self.rt.form))
            .size(13.0)
            .padding([7, 9])
            .width(Length::Fill)
            .style(c.input_style())
            .on_input(move |v| Msg::Field(field, v))
            .into()
    }

    fn labelled<'a>(&'a self, c: &Colors, label: &str, input: El<'a>) -> El<'a> {
        column![text(label.to_string()).size(11.0).color(c.text_faint), input]
            .spacing(3)
            .into()
    }

    fn page_editor(&self, c: &Colors) -> El<'_> {
        let f = &self.rt.form;
        let connected = self
            .rt
            .active_id()
            .map(|id| self.rt.status_of(&id) != ConnStatus::Disconnected)
            .unwrap_or(false);

        let general = column![
            self.section(c, &t("常规", "General")),
            row![
                self.labelled(c, &t("名称", "Name"), self.input(c, Field::Name, "Local bench")),
                self.labelled(c, &t("协议", "Protocol"), pick(c, &proto_opts(), f.protocol, |v| Msg::Num(Num::Protocol, v))),
            ]
            .spacing(10),
            row![
                self.labelled(c, &t("主机", "Host"), self.input(c, Field::Host, "127.0.0.1")),
                column![
                    text(t("端口", "Port")).size(11.0).color(c.text_faint),
                    text_input("1883", &f.port)
                        .size(13.0)
                        .padding([7, 9])
                        .width(120)
                        .style(c.input_style())
                        .on_input(|v| Msg::Field(Field::Port, v)),
                ]
                .spacing(3)
                .width(120),
            ]
            .spacing(10),
            row![
                self.labelled(c, &t("客户端 ID", "Client ID"), self.input(c, Field::ClientId, "auto")),
                button(text(t("生成", "Generate")).size(12.0))
                    .padding([7, 10])
                    .style(c.ghost_button())
                    .on_press(Msg::GenId),
            ]
            .spacing(8)
            .align_y(Alignment::End),
            row![
                self.labelled(c, &t("用户名", "Username"), self.input(c, Field::Username, "")),
                self.labelled(c, &t("密码", "Password"), {
                    text_input("", &f.password)
                        .secure(true)
                        .size(13.0)
                        .padding([7, 9])
                        .width(Length::Fill)
                        .style(c.input_style())
                        .on_input(|v| Msg::Field(Field::Password, v))
                        .into()
                }),
            ]
            .spacing(10),
        ]
        .spacing(10);

        let advanced = column![
            self.section(c, &t("高级", "Advanced")),
            row![
                self.labelled(c, &t("Keep Alive (s)", "Keep alive (s)"), self.input(c, Field::KeepAlive, "60")),
                self.labelled(c, &t("连接超时 (s)", "Connect timeout (s)"), self.input(c, Field::Timeout, "10")),
                column![
                    text(t("Clean session", "Clean session")).size(11.0).color(c.text_faint),
                    checkbox(f.clean_session)
                        .size(16.0)
                        .style(c.checkbox_style())
                        .on_toggle(|v| Msg::Flag(Flag::CleanSession, v)),
                ]
                .spacing(3),
            ]
            .spacing(10),
            row![
                self.labelled(c, &t("订阅 Topic", "Subscribe topic"), self.input(c, Field::SubTopic, "mqttc/mvp/#")),
                column![
                    text(t("订阅 QoS", "Subscribe QoS")).size(11.0).color(c.text_faint),
                    pick(c, &qos_opts(), f.sub_qos, |v| Msg::Num(Num::SubQos, v)),
                ]
                .spacing(3)
                .width(110),
                self.labelled(c, &t("定时间隔 (s)", "Timed interval (s)"), self.input(c, Field::TimedInterval, "5")),
            ]
            .spacing(10),
        ]
        .spacing(10);

        let ssl = column![
            self.section(c, &t("SSL / TLS", "SSL / TLS")),
            row![
                checkbox(f.ssl)
                    .label(t("启用 SSL", "Enable SSL"))
                    .size(16.0)
                    .style(c.checkbox_style())
                    .on_toggle(|v| Msg::Flag(Flag::Ssl, v)),
                checkbox(f.ssl_verify)
                    .label(t("校验服务器证书", "Verify certificate"))
                    .size(16.0)
                    .style(c.checkbox_style())
                    .on_toggle(|v| Msg::Flag(Flag::SslVerify, v)),
            ]
            .spacing(16),
            self.labelled(c, &t("CA 证书", "CA certificate"), self.input(c, Field::Ca, "path/to/ca.pem")),
            row![
                self.labelled(c, &t("客户端证书", "Client certificate"), self.input(c, Field::Cert, "path/to/cert.pem")),
                self.labelled(c, &t("客户端私钥", "Client key"), self.input(c, Field::Key, "path/to/key.pem")),
            ]
            .spacing(10),
        ]
        .spacing(10);

        let will = column![
            self.section(c, &t("遗嘱消息", "Last will")),
            row![
                self.labelled(c, &t("Will Topic", "Will topic"), self.input(c, Field::WillTopic, "mqttc/status")),
                self.labelled(c, &t("Will Payload", "Will payload"), self.input(c, Field::WillPayload, "offline")),
                column![
                    text(t("Will QoS", "Will QoS")).size(11.0).color(c.text_faint),
                    pick(c, &qos_opts(), f.will_qos, |v| Msg::Num(Num::WillQos, v)),
                ]
                .spacing(3)
                .width(100),
            ]
            .spacing(10),
            checkbox(f.will_retain)
                .label(t("Retain", "Retain"))
                .size(16.0)
                .style(c.checkbox_style())
                .on_toggle(|v| Msg::Flag(Flag::WillRetain, v)),
        ]
        .spacing(10);

        let actions = row![
            button(text(t("连接", "Connect")).size(13.0))
                .padding([8, 18])
                .style(c.primary_button())
                .on_press(Msg::Connect),
            button(text(t("会话页", "Session")).size(13.0))
                .padding([8, 14])
                .style(c.ghost_button())
                .on_press(Msg::Page(1)),
            self.hint(
                c,
                if connected {
                    t("连接中编辑不会生效，请先断开", "Editing while connected has no effect; disconnect first")
                } else {
                    t("修改会保存到 exe 同目录的 mqttc.json", "Changes are saved to mqttc.json next to the exe")
                },
            ),
        ]
        .spacing(10)
        .align_y(Alignment::Center);

        scrollable(
            column![general, advanced, ssl, will, actions]
                .spacing(14)
                .padding(14),
        )
        .height(Length::Fill)
        .into()
    }

    fn page_session(&self, c: &Colors) -> El<'_> {
        let Some(conn) = self.rt.active() else {
            return self.hint(c, t("还没有连接，请先新建", "No connection yet — create one first"));
        };
        let status = self.rt.status();
        let connected = status == ConnStatus::Connected;
        let connecting = status == ConnStatus::Connecting;

        let header = container(
            row![
                text(conn.name.as_str()).size(18.0).color(c.text_title),
                text("●").size(10.0).color(rgb(status.rgb())),
                text(status.text()).size(12.0).color(c.text_muted),
                text(format!("{}:{}", conn.host, conn.port))
                    .size(12.0)
                    .color(c.text_faint),
                Space::new().width(Length::Fill),
                button(text(t("编辑", "Edit")).size(12.0))
                    .padding([6, 12])
                    .style(c.ghost_button())
                    .on_press(Msg::Page(0)),
                if connected || connecting {
                    button(text(t("断开", "Disconnect")).size(12.0))
                        .padding([6, 12])
                        .style(c.danger_button())
                        .on_press(Msg::Disconnect)
                } else {
                    button(text(t("连接", "Connect")).size(12.0))
                        .padding([6, 12])
                        .style(c.primary_button())
                        .on_press(Msg::Connect)
                },
            ]
            .spacing(10)
            .align_y(Alignment::Center),
        )
        .padding([12, 12])
        .style(c.card());

        let tabs = row![
            self.tab_button(c, SessionTab::Logs, t("日志", "Logs"), None),
            self.tab_button(
                c,
                SessionTab::Subs,
                t("订阅", "Subscriptions"),
                Some(self.rt.subscriptions().len())
            ),
            self.tab_button(
                c,
                SessionTab::Rules,
                t("自动回复", "Auto reply"),
                Some(self.rt.form.rules.len())
            ),
            self.tab_button(
                c,
                SessionTab::Presets,
                t("预设", "Presets"),
                Some(self.rt.form.presets.len())
            ),
        ]
        .spacing(4);

        let body: El<'_> = match self.session_tab {
            SessionTab::Logs => column![self.logs_card(c), self.publish_card(c)]
                .spacing(10)
                .height(Length::Fill)
                .into(),
            SessionTab::Subs => self.subs_panel(c),
            SessionTab::Rules => self.page_rules(c),
            SessionTab::Presets => self.page_presets(c),
        };

        column![header, tabs, body]
            .spacing(10)
            .height(Length::Fill)
            .into()
    }

    fn tab_button(
        &self,
        c: &Colors,
        tab: SessionTab,
        label: String,
        count: Option<usize>,
    ) -> El<'_> {
        let caption = match count {
            Some(n) if n > 0 => format!("{label} · {n}"),
            _ => label,
        };
        button(text(caption).size(12.0))
            .padding([6, 12])
            .style(c.button_kind(if self.session_tab == tab {
                ButtonKind::Selected
            } else {
                ButtonKind::Ghost
            }))
            .on_press(Msg::SessionTab(tab))
            .into()
    }

    fn subs_panel(&self, c: &Colors) -> El<'_> {
        let connected = self.rt.status() == ConnStatus::Connected;
        let active = self.rt.active_sub_topic();
        let subs = self.rt.subscriptions();

        let mut list = column![].spacing(3);
        for (index, sub) in subs.iter().enumerate() {
            let selected = !active.is_empty() && active == sub.topic;
            let label = text(sub.topic.as_str())
                .size(13.0)
                .color(if selected { c.accent } else { c.text_body });
            let pick = button(label)
                .width(Length::Fill)
                .padding([7, 8])
                .style(c.button_kind(if selected {
                    ButtonKind::Selected
                } else {
                    ButtonKind::Ghost
                }))
                .on_press(Msg::ToggleSub(sub.topic.clone()));
            let qos = text(format!("QoS {}", sub.qos))
                .size(11.0)
                .color(c.text_faint);
            let del = button(text("×").size(14.0))
                .padding([4, 8])
                .style(c.danger_button())
                .on_press(Msg::Unsubscribe(index));
            list = list.push(row![pick, qos, del].spacing(8).align_y(Alignment::Center));
        }
        if subs.is_empty() {
            list = list.push(self.hint(c, t("尚未订阅", "No subscriptions yet")));
        }

        let subscribe = button(text(t("订阅", "Subscribe")).size(12.0))
            .padding([6, 16])
            .style(c.primary_button())
            .on_press_maybe(connected.then_some(Msg::Subscribe));

        let status_hint = if connected {
            t("点击条目筛选 RECV/SEND", "Click a row to filter RECV/SEND")
        } else {
            t("未连接，连接后即可订阅", "Connect first to subscribe")
        };

        container(
            column![
                row![
                    self.section(c, &t("订阅", "Subscriptions")),
                    text(subs.len().to_string()).size(11.0).color(c.text_faint),
                    Space::new().width(Length::Fill),
                    self.hint(c, status_hint),
                ]
                .spacing(8)
                .align_y(Alignment::Center),
                row![
                    text_input("mqttc/mvp/#", &self.rt.form.sub_topic)
                        .size(13.0)
                        .padding([7, 9])
                        .width(Length::Fill)
                        .style(c.input_style())
                        .on_input(|v| Msg::Field(Field::SubTopic, v)),
                    pick(c, &qos_opts(), self.rt.form.sub_qos, |v| Msg::Num(Num::SubQos, v)),
                    subscribe,
                ]
                .spacing(6)
                .align_y(Alignment::Center),
                scrollable(list).height(Length::Fill),
                self.hint(
                    c,
                    t(
                        "点击条目会筛选日志，× 取消订阅",
                        "Click a row to filter logs; × unsubscribes"
                    )
                ),
            ]
            .spacing(10)
            .height(Length::Fill),
        )
        .padding(12)
        .height(Length::Fill)
        .style(c.card())
        .into()
    }

    fn logs_card(&self, c: &Colors) -> El<'_> {
        let prefs = &self.rt.prefs;
        let toolbar = row![
            text_input(
                l("搜索 Topic / Payload", "Search topic / payload"),
                &prefs.log_filter
            )
            .size(12.0)
            .padding([5, 7])
            .width(180)
            .style(c.input_style())
            .on_input(Msg::LogFilter),
            pick(c, &kind_opts(), prefs.log_kind_filter, Msg::LogKind),
            pick(c, &format_opts(), prefs.log_format, Msg::LogFormat),
            checkbox(prefs.auto_scroll)
                .label(t("自动滚动", "Auto-scroll"))
                .size(15.0)
                .text_size(12.0)
                .style(c.checkbox_style())
                .on_toggle(Msg::AutoScroll),
            Space::new().width(Length::Fill),
            button(text(t("保存", "Save")).size(12.0))
                .padding([5, 10])
                .style(c.ghost_button())
                .on_press(Msg::SaveLogs),
            button(text(t("清空", "Clear")).size(12.0))
                .padding([5, 10])
                .style(c.ghost_button())
                .on_press(Msg::ClearLogs),
        ]
        .spacing(6)
        .align_y(Alignment::Center);

        let logs = self.rt.visible_logs();
        let mut list = column![].spacing(5);
        if logs.is_empty() {
            list = list.push(
                container(self.hint(c, t("还没有日志", "No logs yet")))
                    .padding(12)
                    .center_x(Length::Fill),
            );
        } else {
            for log in logs {
                list = list.push(self.log_bubble(c, log));
            }
        }

        container(
            column![
                toolbar,
                scrollable(list)
                    .height(Length::Fill)
                    .anchor_bottom()
                    .auto_scroll(prefs.auto_scroll),
            ]
            .spacing(6)
            .height(Length::Fill),
        )
        .padding(10)
        .height(Length::FillPortion(3))
        .style(c.card())
        .into()
    }

    fn log_bubble(&self, c: &Colors, log: &LogRecord) -> El<'_> {
        let fmt = PayloadFormat::from_index(self.rt.prefs.log_format);
        let (bg, topic_color, body_color, right) = match log.kind.as_str() {
            "RECV" => (c.recv_bg, c.recv_topic, c.recv_payload, false),
            "SEND" => (c.send_bg, c.send_topic, c.send_payload, true),
            "ERR" => (c.err_bg, c.err_label, c.err_text, false),
            _ => (c.sys_bg, c.sys_text, c.sys_text, false),
        };
        let body = if log.is_payload_kind() {
            payload::render_payload(&log.payload, fmt)
        } else {
            String::from_utf8_lossy(&log.payload).into_owned()
        };
        let topic_line = if log.topic.is_empty() {
            format!("{} · {}", log.time, log.kind)
        } else {
            format!("{} · {} · {}", log.time, log.kind, log.topic)
        };
        let bubble = container(
            column![
                text(topic_line).size(10.5).color(topic_color),
                text(body).size(12.5).font(mono_font()).color(body_color),
            ]
            .spacing(2),
        )
        .padding([7, 9])
        .max_width(520)
        .style(c.bubble(bg));

        let mut row_children: Vec<El> = Vec::new();
        if right {
            row_children.push(Space::new().width(Length::Fill).into());
            row_children.push(bubble.into());
        } else {
            row_children.push(bubble.into());
            row_children.push(Space::new().width(Length::Fill).into());
        }
        row(row_children).width(Length::Fill).into()
    }

    fn publish_card(&self, c: &Colors) -> El<'_> {
        let f = &self.rt.form;
        let timed = self.rt.timed_running();
        let presets = self
            .rt
            .form
            .presets
            .iter()
            .enumerate()
            .fold(row![].spacing(4), |acc, (i, p)| {
                acc.push(
                    button(text(p.name.as_str()).size(11.0))
                        .padding([4, 8])
                        .style(c.ghost_button())
                        .on_press(Msg::PublishPreset(i)),
                )
            });

        let topic_row = row![
            text_input("mqttc/mvp/hello", &f.pub_topic)
                .size(12.0)
                .padding([5, 7])
                .width(Length::Fill)
                .style(c.input_style())
                .on_input(|v| Msg::Field(Field::PubTopic, v)),
            pick(c, &qos_opts(), f.pub_qos, |v| Msg::Num(Num::PubQos, v)),
            checkbox(f.pub_retain)
                .label("Retain")
                .size(15.0)
                .text_size(12.0)
                .style(c.checkbox_style())
                .on_toggle(|v| Msg::Flag(Flag::PubRetain, v)),
            pick(c, &format_opts(), f.pub_format, |v| Msg::Num(Num::PubFormat, v)),
            button(text(t("美化", "Pretty")).size(11.0))
                .padding([4, 8])
                .style(c.ghost_button())
                .on_press(Msg::Format),
        ]
        .spacing(5)
        .align_y(Alignment::Center);

        let timed_row = row![
            text_input("5", &f.timed_interval)
                .size(12.0)
                .padding([5, 7])
                .width(70)
                .style(c.input_style())
                .on_input(|v| Msg::Field(Field::TimedInterval, v)),
            text(t("秒一次", "sec interval")).size(11.0).color(c.text_faint),
            button(
                text(if timed {
                    t("停止定时", "Stop timed")
                } else {
                    t("开始定时", "Start timed")
                })
                .size(11.0)
            )
            .padding([4, 9])
            .style(c.button_kind(if timed {
                ButtonKind::Danger
            } else {
                ButtonKind::Ghost
            }))
            .on_press(Msg::Timed),
            button(text(t("发布", "Publish")).size(12.0))
                .padding([5, 14])
                .style(c.primary_button())
                .on_press(Msg::Publish),
        ]
        .spacing(6)
        .align_y(Alignment::Center);

        container(
            column![
                row![
                    self.section(c, &t("发布", "Publish")),
                    Space::new().width(Length::Fill),
                    scrollable(presets)
                        .direction(scrollable::Direction::Horizontal(
                            scrollable::Scrollbar::new()
                        ))
                        .height(26),
                ]
                .align_y(Alignment::Center),
                topic_row,
                text_editor(&self.editor)
                    .height(84)
                    .padding(8)
                    .size(12.5)
                    .font(mono_font())
                    .style(c.editor_style())
                    .on_action(Msg::PubEdit),
                timed_row,
            ]
            .spacing(6),
        )
        .padding(10)
        .style(c.card())
        .into()
    }

    fn page_settings(&self, c: &Colors) -> El<'_> {
        container(
            column![
                self.section(c, &t("外观", "Appearance")),
                row![
                    text(t("皮肤", "Skin")).size(13.0).color(c.text_body),
                    pick(c, &skin_opts(), self.rt.prefs.skin_choice, Msg::Skin),
                ]
                .spacing(12)
                .align_y(Alignment::Center),
                row![
                    text(t("语言", "Language")).size(13.0).color(c.text_body),
                    pick(c, &lang_opts(), self.rt.prefs.language, Msg::Language),
                ]
                .spacing(12)
                .align_y(Alignment::Center),
                self.hint(
                    c,
                    t(
                        "跟随系统会按 Windows/macOS/Linux 的深浅色自动切换",
                        "System follows the OS light/dark setting on Windows/macOS/Linux"
                    )
                ),
                Space::new().height(8),
                self.section(c, &t("配置文件", "Configuration")),
                self.hint(
                    c,
                    t(
                        "所有设置写入 exe 同目录的 mqttc.json，复制整个目录即可迁移",
                        "Everything is stored in mqttc.json next to the exe; copy the folder to migrate"
                    )
                ),
            ]
            .spacing(12)
            .padding(14),
        )
        .style(c.card())
        .into()
    }

    fn page_rules(&self, c: &Colors) -> El<'_> {
        let mut list = column![].spacing(8);
        for (index, rule) in self.rt.form.rules.iter().enumerate() {
            let card = container(
                column![
                    row![
                        checkbox(rule.enabled)
                            .label(t("启用", "Enabled"))
                            .size(15.0)
                            .text_size(12.0)
                            .style(c.checkbox_style())
                            .on_toggle(move |v| Msg::RuleEnabled(index, v)),
                        text(t("匹配 Topic", "Match topic")).size(11.0).color(c.text_faint),
                        text_input("mqttc/in/#", &rule.match_topic)
                            .size(12.0)
                            .padding([5, 7])
                            .width(Length::Fill)
                            .style(c.input_style())
                            .on_input(move |v| Msg::RuleText(index, RuleTextField::MatchTopic, v)),
                        button(text("×").size(13.0))
                            .padding([3, 7])
                            .style(c.danger_button())
                            .on_press(Msg::DelRule(index)),
                    ]
                    .spacing(6)
                    .align_y(Alignment::Center),
                    row![
                        text(t("包含文本", "Contains")).size(11.0).color(c.text_faint),
                        text_input("", &rule.payload_contains)
                            .size(12.0)
                            .padding([5, 7])
                            .width(Length::Fill)
                            .style(c.input_style())
                            .on_input(move |v| Msg::RuleText(index, RuleTextField::Contains, v)),
                        text(t("回复 Topic", "Reply topic")).size(11.0).color(c.text_faint),
                        text_input("", &rule.reply_topic)
                            .size(12.0)
                            .padding([5, 7])
                            .width(Length::Fill)
                            .style(c.input_style())
                            .on_input(move |v| Msg::RuleText(index, RuleTextField::ReplyTopic, v)),
                    ]
                    .spacing(6)
                    .align_y(Alignment::Center),
                    row![
                        text(t("回复内容", "Reply payload")).size(11.0).color(c.text_faint),
                        text_input("", &rule.reply_payload)
                            .size(12.0)
                            .padding([5, 7])
                            .width(Length::Fill)
                            .style(c.input_style())
                            .on_input(move |v| Msg::RuleText(index, RuleTextField::ReplyPayload, v)),
                        pick(c, &qos_opts(), rule.qos, move |v| Msg::RuleNum(index, v)),
                        checkbox(rule.retain)
                            .label("Retain")
                            .size(15.0)
                            .text_size(12.0)
                            .style(c.checkbox_style())
                            .on_toggle(move |v| Msg::RuleRetain(index, v)),
                        pick(c, &format_opts(), rule.format, move |v| Msg::RuleFormat(index, v)),
                    ]
                    .spacing(6)
                    .align_y(Alignment::Center),
                ]
                .spacing(6),
            )
            .padding(10)
            .style(c.inset());
            list = list.push(card);
        }
        if self.rt.form.rules.is_empty() {
            list = list.push(self.hint(
                c,
                t("尚无规则，点击右上角新增", "No rules yet — use Add rule"),
            ));
        }

        scrollable(
            column![
                row![
                    self.section(c, &t("自动回复", "Auto reply")),
                    Space::new().width(Length::Fill),
                    text(t("自身回投", "Own echo")).size(11.0).color(c.text_faint),
                    pick(c, &echo_opts(), self.rt.form.auto_reply_echo, |v| {
                        Msg::Num(Num::Echo, v)
                    }),
                    button(text(t("新增规则", "Add rule")).size(12.0))
                        .padding([5, 10])
                        .style(c.primary_button())
                        .on_press(Msg::AddRule),
                ]
                .spacing(8)
                .align_y(Alignment::Center),
                list,
                self.hint(
                    c,
                    t(
                        "收到消息后按规则顺序匹配，首条命中的规则回复一次",
                        "Rules are evaluated in order; the first match replies once"
                    )
                ),
            ]
            .spacing(10)
            .padding(12),
        )
        .height(Length::Fill)
        .into()
    }

    fn page_presets(&self, c: &Colors) -> El<'_> {
        let mut list = column![].spacing(8);
        for (index, preset) in self.rt.form.presets.iter().enumerate() {
            let card = container(
                column![
                    row![
                        text_input(l("名称", "Name"), &preset.name)
                            .size(12.0)
                            .padding([5, 7])
                            .width(140)
                            .style(c.input_style())
                            .on_input(move |v| Msg::PresetText(index, PresetTextField::Name, v)),
                        text_input("mqttc/mvp/hello", &preset.topic)
                            .size(12.0)
                            .padding([5, 7])
                            .width(Length::Fill)
                            .style(c.input_style())
                            .on_input(move |v| Msg::PresetText(index, PresetTextField::Topic, v)),
                        pick(c, &qos_opts(), preset.qos, move |v| Msg::PresetNum(index, v)),
                        checkbox(preset.retain)
                            .label("Retain")
                            .size(15.0)
                            .text_size(12.0)
                            .style(c.checkbox_style())
                            .on_toggle(move |v| Msg::PresetRetain(index, v)),
                        pick(c, &format_opts(), preset.format, move |v| Msg::PresetFormat(index, v)),
                        button(text(t("发送", "Send")).size(11.0))
                            .padding([4, 8])
                            .style(c.primary_button())
                            .on_press(Msg::PublishPreset(index)),
                        button(text("×").size(13.0))
                            .padding([3, 7])
                            .style(c.danger_button())
                            .on_press(Msg::DelPreset(index)),
                    ]
                    .spacing(5)
                    .align_y(Alignment::Center),
                    text_input(l("内容", "Payload"), &preset.payload)
                        .size(12.0)
                        .padding([5, 7])
                        .width(Length::Fill)
                        .style(c.input_style())
                        .on_input(move |v| Msg::PresetText(index, PresetTextField::Payload, v)),
                ]
                .spacing(5),
            )
            .padding(10)
            .style(c.inset());
            list = list.push(card);
        }
        if self.rt.form.presets.is_empty() {
            list = list.push(self.hint(
                c,
                t("尚无预设，点击右上角新增", "No presets yet — use Add preset"),
            ));
        }

        scrollable(
            column![
                row![
                    self.section(c, &t("发送预设", "Publish presets")),
                    Space::new().width(Length::Fill),
                    button(text(t("新增预设", "Add preset")).size(12.0))
                        .padding([5, 10])
                        .style(c.primary_button())
                        .on_press(Msg::AddPreset),
                ]
                .spacing(8)
                .align_y(Alignment::Center),
                list,
                self.hint(
                    c,
                    t(
                        "日志页发布框上方也有一键发送按钮",
                        "The Logs tab also shows one-click buttons for every preset"
                    )
                ),
            ]
            .spacing(10)
            .padding(12),
        )
        .height(Length::Fill)
        .into()
    }

    fn page_about(&self, c: &Colors) -> El<'_> {
        container(
            column![
                text("mqttc · iced").size(22.0).color(c.text_title),
                text(format!("v{}", env!("CARGO_PKG_VERSION")))
                    .size(12.0)
                    .color(c.text_faint),
                Space::new().height(8),
                text(t(
                    "轻量级跨平台 MQTT 调试客户端，核心逻辑与 egui / GPUI 版本共用。",
                    "A lightweight cross-platform MQTT debug client sharing its core with the egui / GPUI builds."
                ))
                .size(13.0)
                .color(c.text_body),
                text(t(
                    "渲染：iced + tiny-skia 软件光栅；字体使用系统 CJK 回退。",
                    "Rendering: iced + tiny-skia software rasterizer; CJK glyphs come from system fonts."
                ))
                .size(12.0)
                .color(c.text_muted),
                Space::new().height(8),
                self.hint(c, "MQTT 3.1.1 / 5.0 · TLS · Last Will · Auto reply · Timed publish".to_string()),
                self.hint(c, "License: MIT".to_string()),
            ]
            .spacing(8)
            .padding(16),
        )
        .style(c.card())
        .into()
    }
}

/// `t()` returns an owned `String`; `text_input` placeholders must borrow a
/// `&'static str`, so the small helper keeps both strings in the binary.
fn l(zh: &'static str, en: &'static str) -> &'static str {
    if is_en() {
        en
    } else {
        zh
    }
}

pub fn mono_font() -> iced::Font {
    iced::Font {
        family: iced::font::Family::Monospace,
        ..iced::Font::DEFAULT
    }
}

pub fn ui_font() -> iced::Font {
    #[cfg(target_os = "windows")]
    const FAMILY: &str = "Microsoft YaHei UI";
    #[cfg(target_os = "macos")]
    const FAMILY: &str = "PingFang SC";
    #[cfg(all(unix, not(target_os = "macos")))]
    const FAMILY: &str = "Noto Sans CJK SC";
    #[cfg(not(any(target_os = "windows", target_os = "macos", unix)))]
    const FAMILY: &str = "sans-serif";

    iced::Font {
        family: iced::font::Family::Name(FAMILY),
        ..iced::Font::DEFAULT
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Widget construction has no renderer dependency; building every page
    /// catches layout/borrow panics that the bench (always on page 0) misses.
    #[test]
    fn every_page_builds() {
        let mut app = App::new();
        for page in [0, 1, 2, 5] {
            app.page = page;
            let _page = app.view();
        }
    }

    /// Each session tab has its own body builder; exercise all of them with
    /// non-empty connection-level lists so the list branches are covered.
    #[test]
    fn every_session_tab_builds() {
        use mqttc_core::config::{AutoReplyRule, ConnectionConfig, SubscriptionConfig};
        use mqttc_core::form::FormState;

        let mut app = App::new();
        let mut conn = ConnectionConfig::new("test".into());
        conn.subscriptions.push(SubscriptionConfig {
            topic: "mqttc/test/#".into(),
            qos: 1,
        });
        conn.publish_presets.push(Default::default());
        conn.auto_reply_rules.push(AutoReplyRule::default());
        app.rt.config.active_id = conn.id.clone();
        app.rt.config.connections.push(conn);
        app.rt.form = FormState::from_config(app.rt.active().unwrap());
        app.page = 1;
        for tab in [
            SessionTab::Logs,
            SessionTab::Subs,
            SessionTab::Rules,
            SessionTab::Presets,
        ] {
            app.session_tab = tab;
            let _page = app.view();
        }
    }
}
