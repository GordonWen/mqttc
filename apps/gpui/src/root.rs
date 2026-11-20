//! mqttc · GPUI shell.
//!
//! The runtime, config and MQTT workers all live in `mqttc-core`; this module
//! maps GPUI clicks onto runtime actions and paints the six reference pages.

use std::collections::HashMap;
use std::time::{Duration, Instant};

use gpui::prelude::*;
use gpui::{
    div, px, uniform_list, ClickEvent, Context, Div, Entity, SharedString, UniformListScrollHandle,
    Window, ScrollStrategy,
};
use mqttc_core::cli::BenchOptions;
use mqttc_core::i18n::t;
use mqttc_core::model::{default_client_id, ConnStatus, PayloadFormat};
use mqttc_core::payload;
use mqttc_core::runtime::AppRuntime;

use crate::text_input::TextInput;
use crate::theme::Colors;
use crate::widgets::{button, card, checkbox, chip, dot, hint, section, well, Kind};

type El = gpui::AnyElement;

// ---------------------------------------------------------------- fields

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
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
    PubPayload,
    TimedInterval,
    Ca,
    Cert,
    Key,
}

impl Field {
    const ALL: [Field; 17] = [
        Field::Name,
        Field::Host,
        Field::Port,
        Field::ClientId,
        Field::Username,
        Field::Password,
        Field::KeepAlive,
        Field::Timeout,
        Field::SubTopic,
        Field::WillTopic,
        Field::WillPayload,
        Field::PubTopic,
        Field::PubPayload,
        Field::TimedInterval,
        Field::Ca,
        Field::Cert,
        Field::Key,
    ];

    fn get(self, f: &mqttc_core::form::FormState) -> &str {
        use Field::*;
        match self {
            Name => &f.name,
            Host => &f.host,
            Port => &f.port,
            ClientId => &f.client_id,
            Username => &f.username,
            Password => &f.password,
            KeepAlive => &f.keep_alive,
            Timeout => &f.connect_timeout,
            SubTopic => &f.sub_topic,
            WillTopic => &f.will_topic,
            WillPayload => &f.will_payload,
            PubTopic => &f.pub_topic,
            PubPayload => &f.pub_payload,
            TimedInterval => &f.timed_interval,
            Ca => &f.ssl_ca,
            Cert => &f.ssl_cert,
            Key => &f.ssl_key,
        }
    }

    fn set(self, f: &mut mqttc_core::form::FormState, value: String) {
        use Field::*;
        match self {
            Name => f.name = value,
            Host => f.host = value,
            Port => f.port = value,
            ClientId => f.client_id = value,
            Username => f.username = value,
            Password => f.password = value,
            KeepAlive => f.keep_alive = value,
            Timeout => f.connect_timeout = value,
            SubTopic => f.sub_topic = value,
            WillTopic => f.will_topic = value,
            WillPayload => f.will_payload = value,
            PubTopic => f.pub_topic = value,
            PubPayload => f.pub_payload = value,
            TimedInterval => f.timed_interval = value,
            Ca => f.ssl_ca = value,
            Cert => f.ssl_cert = value,
            Key => f.ssl_key = value,
        }
    }

    fn id(self) -> &'static str {
        match self {
            Field::Name => "in-name",
            Field::Host => "in-host",
            Field::Port => "in-port",
            Field::ClientId => "in-client-id",
            Field::Username => "in-user",
            Field::Password => "in-pass",
            Field::KeepAlive => "in-keepalive",
            Field::Timeout => "in-timeout",
            Field::SubTopic => "in-sub-topic",
            Field::WillTopic => "in-will-topic",
            Field::WillPayload => "in-will-payload",
            Field::PubTopic => "in-pub-topic",
            Field::PubPayload => "in-pub-payload",
            Field::TimedInterval => "in-timed",
            Field::Ca => "in-ca",
            Field::Cert => "in-cert",
            Field::Key => "in-key",
        }
    }
}

/// Cycle-able single-choice fields (form values plus app preferences).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Opt {
    Protocol,
    SubQos,
    PubQos,
    PubFormat,
    WillQos,
    Echo,
    Kind,
    LogFormat,
    Skin,
    Lang,
}

// ---------------------------------------------------------------- bench

struct Bench {
    options: BenchOptions,
    quit_at: Option<Instant>,
    announced: bool,
    timed_started: bool,
    finished: bool,
}

// ---------------------------------------------------------------- app

pub struct Root {
    rt: AppRuntime,
    page: i32,
    inputs: HashMap<Field, Entity<TextInput>>,
    filter_input: Entity<TextInput>,
    started: Instant,
    first_frame_ms: Option<f64>,
    log_scroll: UniformListScrollHandle,
    bench: Option<Bench>,
    ticking: bool,
    dirty_at: Option<Instant>,
}

impl Root {
    pub fn new(cx: &mut Context<Self>, options: BenchOptions) -> Self {
        let rt = AppRuntime::load(&options, None);
        let started = Instant::now();
        let quit_at = options
            .quit_after
            .map(|secs| started + Duration::from_secs(secs));
        let bench = options.bench.then(|| Bench {
            options,
            quit_at,
            announced: false,
            timed_started: false,
            finished: false,
        });
        let colors = Colors::from_palette(rt.palette());

        let mut inputs = HashMap::new();
        for field in Field::ALL {
            let text = field.get(&rt.form).to_owned();
            let placeholder = field.id();
            let weak = cx.weak_entity();
            let entity = cx.new(|cx| {
                TextInput::new(
                    cx.focus_handle(),
                    placeholder,
                    &text,
                    matches!(field, Field::Password),
                    colors.text_faint,
                    Some(Box::new(move |value, app| {
                        let _ = weak.update(app, |this, cx| {
                            field.set(&mut this.rt.form, value.to_owned());
                            this.mark_dirty(cx);
                        });
                    })),
                    None,
                )
            });
            inputs.insert(field, entity);
        }

        let filter_text = rt.prefs.log_filter.clone();
        let weak = cx.weak_entity();
        let filter_input = cx.new(|cx| {
            TextInput::new(
                cx.focus_handle(),
                "搜索 Topic / Payload",
                &filter_text,
                false,
                colors.text_faint,
                Some(Box::new(move |value, app| {
                    let _ = weak.update(app, |this, cx| {
                        this.rt.set_log_filter(value.to_owned());
                        this.mark_dirty(cx);
                    });
                })),
                None,
            )
        });

        let mut root = Self {
            rt,
            page: 0,
            inputs,
            filter_input,
            started,
            first_frame_ms: None,
            log_scroll: UniformListScrollHandle::new(),
            bench,
            ticking: false,
            dirty_at: None,
        };
        root.ensure_ticker(cx);
        root
    }

    fn colors(&self) -> Colors {
        Colors::from_palette(self.rt.palette())
    }

    // ------------------------------------------------------------ state

    fn busy(&self) -> bool {
        self.bench.as_ref().map(|b| !b.finished).unwrap_or(false)
            || self.rt.timed_running()
            || self.dirty_at.is_some()
            || self
                .rt
                .active_id()
                .map(|id| self.rt.status_of(&id) != ConnStatus::Disconnected)
                .unwrap_or(false)
    }

    fn needs_tick(&self) -> bool {
        self.busy() || self.rt.prefs.skin_choice == 0
    }

    fn ensure_ticker(&mut self, cx: &mut Context<Self>) {
        if self.ticking || !self.needs_tick() {
            return;
        }
        self.ticking = true;
        cx.spawn(async move |weak, cx| loop {
            let interval = weak
                .update(cx, |this, _| if this.busy() { 100 } else { 1000 })
                .unwrap_or(100);
            cx.background_executor()
                .timer(Duration::from_millis(interval))
                .await;
            let keep = weak
                .update(cx, |this, cx| {
                    this.tick(cx);
                    this.needs_tick()
                })
                .unwrap_or(false);
            if !keep {
                let _ = weak.update(cx, |this, _| this.ticking = false);
                break;
            }
        })
        .detach();
    }

    fn mark_dirty(&mut self, cx: &mut Context<Self>) {
        self.dirty_at = Some(Instant::now() + Duration::from_millis(420));
        self.rt.mark_dirty();
        self.ensure_ticker(cx);
        cx.notify();
    }

    fn sync_inputs(&mut self, cx: &mut Context<Self>) {
        let texts: Vec<(Field, String)> = self
            .inputs
            .keys()
            .map(|field| (*field, field.get(&self.rt.form).to_owned()))
            .collect();
        for (field, text) in texts {
            if let Some(input) = self.inputs.get(&field) {
                input.update(cx, |input, cx| input.set_text(&text, cx));
            }
        }
        let filter = self.rt.prefs.log_filter.clone();
        self.filter_input
            .update(cx, |input, cx| input.set_text(&filter, cx));
    }

    fn reload_form(&mut self, cx: &mut Context<Self>) {
        self.rt.apply_form();
        self.sync_inputs(cx);
    }

    fn select_connection(&mut self, id: &str, cx: &mut Context<Self>) {
        if self.rt.select_connection(id) {
            self.page = 1;
            self.reload_form(cx);
        }
    }

    fn new_connection(&mut self, cx: &mut Context<Self>) {
        self.rt.new_connection();
        self.page = 0;
        self.rt.persist();
        self.reload_form(cx);
    }

    fn delete_connection(&mut self, id: &str, cx: &mut Context<Self>) {
        self.rt.delete_connection(id);
        self.page = 0;
        self.rt.persist();
        self.reload_form(cx);
    }

    fn go(&mut self, page: i32, cx: &mut Context<Self>) {
        if page == 1 {
            self.reload_form(cx);
        }
        self.page = page;
        cx.notify();
    }

    fn cycle(&mut self, opt: Opt, cx: &mut Context<Self>) {
        use Opt::*;
        let f = &mut self.rt.form;
        match opt {
            Protocol => f.protocol = (f.protocol + 1) % 2,
            SubQos => f.sub_qos = (f.sub_qos + 1) % 3,
            PubQos => f.pub_qos = (f.pub_qos + 1) % 3,
            PubFormat => f.pub_format = (f.pub_format + 1) % 4,
            WillQos => f.will_qos = (f.will_qos + 1) % 3,
            Echo => f.auto_reply_echo = (f.auto_reply_echo + 1) % 3,
            Kind => {
                let next = (self.rt.prefs.log_kind_filter + 1) % 5;
                self.rt.set_log_kind_filter(next);
                cx.notify();
                return;
            }
            LogFormat => {
                let next = (self.rt.prefs.log_format + 1) % 4;
                self.rt.set_log_format(next);
                cx.notify();
                return;
            }
            Skin => {
                let next = (self.rt.prefs.skin_choice + 1) % mqttc_core::model::Skin::ALL.len() as i32;
                self.rt.set_skin_choice(next);
                cx.notify();
                self.ensure_ticker(cx);
                return;
            }
            Lang => {
                let next = (self.rt.prefs.language + 1) % 2;
                self.rt.set_language(next);
                cx.notify();
                return;
            }
        }
        self.rt.apply_form();
        self.mark_dirty(cx);
    }

    /// Cycle a rule's QoS / format without text editing.
    fn cycle_rule(&mut self, index: usize, qos: bool, cx: &mut Context<Self>) {
        if let Some(rule) = self.rt.form.rules.get_mut(index) {
            if qos {
                rule.qos = (rule.qos + 1) % 3;
            } else {
                rule.format = (rule.format + 1) % 4;
            }
        }
        self.rt.apply_form();
        self.mark_dirty(cx);
    }

    fn toggle_rule(&mut self, index: usize, enabled: bool, cx: &mut Context<Self>) {
        if let Some(rule) = self.rt.form.rules.get_mut(index) {
            rule.enabled = enabled;
        }
        self.rt.apply_form();
        self.mark_dirty(cx);
    }

    // ------------------------------------------------------------ tick

    fn tick(&mut self, cx: &mut Context<Self>) {
        let summary = self.rt.poll();
        if let Some(page) = summary.page_request {
            self.page = page;
            self.reload_form(cx);
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
            cx.notify();
        }

        if summary.changed && self.rt.prefs.auto_scroll {
            let count = self.rt.visible_logs().len();
            if count > 0 {
                self.log_scroll.scroll_to_item(count - 1, ScrollStrategy::Bottom);
            }
        }

        if self.bench.is_some() {
            self.tick_bench(cx);
        }

        if summary.changed || summary.status_changed || summary.page_request.is_some() {
            cx.notify();
        }
    }

    fn tick_bench(&mut self, cx: &mut Context<Self>) {
        let Some(bench) = self.bench.as_mut() else {
            return;
        };
        if bench.finished {
            return;
        }
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
                bench.timed_started = true;
                self.rt.toggle_timed();
            }
        }

        if bench.quit_at.map(|at| Instant::now() >= at).unwrap_or(false) {
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
            println!("BENCH_QUIT {:.2}", self.started.elapsed().as_secs_f64());
            cx.quit();
        }
    }

    // ------------------------------------------------------------ helpers

    fn input(&self, field: Field) -> Entity<TextInput> {
        self.inputs
            .get(&field)
            .cloned()
            .expect("every Field has a text input")
    }

    fn field_row(&self, c: &Colors, label: &str, field: Field) -> Div {
        div()
            .flex()
            .items_center()
            .gap(px(8.))
            .child(
                div()
                    .w(px(92.))
                    .flex_shrink_0()
                    .text_size(px(12.))
                    .text_color(c.text_muted)
                    .child(label.to_owned()),
            )
            .child(
                well(c)
                    .flex_1()
                    .text_color(c.text_body)
                    .child(self.input(field)),
            )
    }

    fn select(&self, c: &Colors, id: &'static str, label: impl Into<SharedString>) -> gpui::Stateful<Div> {
        let _ = id;
        chip(c, SharedString::from(format!("chip-{id}")), label.into())
    }

    fn opt_text(&self, opt: Opt) -> String {
        use Opt::*;
        use mqttc_core::i18n::is_en;
        match opt {
            Protocol => {
                if self.rt.form.protocol == 1 { "5.0" } else { "3.1.1" }.to_string()
            }
            SubQos => format!("QoS {}", self.rt.form.sub_qos),
            PubQos => format!("QoS {}", self.rt.form.pub_qos),
            PubFormat => fmt_name(PayloadFormat::from_index(self.rt.form.pub_format)).to_string(),
            WillQos => format!("QoS {}", self.rt.form.will_qos),
            Echo => {
                let names = if is_en() {
                    ["Show", "Hide", "Skip"]
                } else {
                    ["显示", "隐藏", "跳过"]
                };
                names[(self.rt.form.auto_reply_echo.clamp(0, 2)) as usize].to_string()
            }
            Kind => {
                let names = if is_en() {
                    ["All", "Recv", "Send", "System", "Errors"]
                } else {
                    ["全部", "收", "发", "系统", "错误"]
                };
                names[(self.rt.prefs.log_kind_filter.clamp(0, 4)) as usize].to_string()
            }
            LogFormat => fmt_name(PayloadFormat::from_index(self.rt.prefs.log_format)).to_string(),
            Skin => {
                let names = [
                    "跟随系统", "黑", "白", "蓝", "绿", "紫",
                ];
                let names_en = ["System", "Black", "White", "Blue", "Green", "Purple"];
                let i = (self.rt.prefs.skin_choice.clamp(0, 5)) as usize;
                if is_en() { names_en[i] } else { names[i] }.to_string()
            }
            Lang => if self.rt.prefs.language == 1 { "English" } else { "中文" }.to_string(),
        }
    }
}

fn fmt_name(f: PayloadFormat) -> &'static str {
    match f {
        PayloadFormat::Text => "Text",
        PayloadFormat::Json => "JSON",
        PayloadFormat::Base64 => "Base64",
        PayloadFormat::Hex => "Hex",
    }
}

// ---------------------------------------------------------------- render

impl Render for Root {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        if self.first_frame_ms.is_none() {
            let ms = self.started.elapsed().as_secs_f64() * 1000.0;
            self.first_frame_ms = Some(ms);
            println!("FIRST_FRAME_MS {ms:.1}");
        }
        let c = self.colors();
        let page: El = match self.page {
            0 => self.page_editor(&c, cx).into_any_element(),
            1 => self.page_session(&c, cx).into_any_element(),
            2 => self.page_settings(&c, cx).into_any_element(),
            3 => self.page_rules(&c, cx).into_any_element(),
            4 => self.page_presets(&c, cx).into_any_element(),
            _ => self.page_about(&c),
        };

        div()
            .size_full()
            .flex()
            .bg(c.window_bg)
            .text_color(c.text_body)
            .text_size(px(13.))
            .child(self.sidebar(&c, cx))
            .child(
                div()
                    .flex_1()
                    .flex()
                    .flex_col()
                    .p(px(10.))
                    .gap(px(10.))
                    .overflow_hidden()
                    .child(page),
            )
    }
}

impl Root {
    fn sidebar(&self, c: &Colors, cx: &mut Context<Self>) -> Div {
        let mut list = div().flex().flex_col().gap(px(3.));
        let active = self.rt.active_id();
        for conn in &self.rt.config.connections {
            let status = self.rt.status_of(&conn.id);
            let is_active = active.as_deref() == Some(conn.id.as_str());
            let id = conn.id.clone();
            let pick = button(
                c,
                SharedString::from(format!("conn-{}", conn.id)),
                div()
                    .flex()
                    .items_center()
                    .gap(px(6.))
                    .child(dot(theme_rgb(status.rgb())))
                    .child(
                        div()
                            .text_size(px(13.))
                            .text_color(if is_active { c.text_primary } else { c.text_body })
                            .child(conn.name.clone()),
                    ),
                if is_active { Kind::Selected } else { Kind::Ghost },
            )
            .flex_1()
            .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                this.select_connection(&id, cx);
            }));
            let del_id = conn.id.clone();
            let del = button(
                c,
                SharedString::from(format!("del-{}", conn.id)),
                "×",
                Kind::Danger,
            )
            .px(px(6.))
            .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                this.delete_connection(&del_id, cx);
            }));
            list = list.child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(2.))
                    .child(pick)
                    .child(del),
            );
        }

        let nav = [
            (0, t("连接编辑", "Connection")),
            (1, t("会话", "Session")),
            (2, t("设置", "Settings")),
            (3, t("自动回复", "Auto reply")),
            (4, t("预设", "Presets")),
            (5, t("关于", "About")),
        ];
        let mut links = div().flex().flex_col().gap(px(2.));
        for (page, label) in nav {
            links = links.child(
                button(
                    c,
                    SharedString::from(format!("nav-{page}")),
                    div().text_size(px(13.)).child(label),
                    if self.page == page { Kind::Selected } else { Kind::Ghost },
                )
                .w_full()
                .justify_start()
                .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| this.go(page, cx))),
            );
        }

        card(c)
            .w(px(220.))
            .flex_shrink_0()
            .h_full()
            .flex()
            .flex_col()
            .p(px(10.))
            .gap(px(8.))
            .child(
                div()
                    .flex()
                    .items_center()
                    .justify_between()
                    .child(
                        div()
                            .text_size(px(15.))
                            .text_color(c.text_title)
                            .child("mqttc · gpui"),
                    )
                    .child(
                        button(c, "new-conn", t("新建", "New"), Kind::Ghost).on_click(
                            cx.listener(|this, _: &ClickEvent, _, cx| this.new_connection(cx)),
                        ),
                    ),
            )
            .child(div().flex().flex_col().gap(px(3.)).child(list))
            .child(div().h(px(1.)).bg(c.hover_bg))
            .child(links)
            .child(div().flex_1())
            .child(hint(c, t("GPUI 0.2 · 原生 GPU", "GPUI 0.2 · native GPU")))
    }
}

fn theme_rgb((r, g, b): (u8, u8, u8)) -> gpui::Rgba {
    crate::theme::c((r, g, b))
}

// ---------------------------------------------------------------- pages

impl Root {
    fn page_editor(&self, c: &Colors, cx: &mut Context<Self>) -> gpui::Stateful<Div> {
        let f = &self.rt.form;
        let connecting = self
            .rt
            .active_id()
            .map(|id| self.rt.status_of(&id) != ConnStatus::Disconnected)
            .unwrap_or(false);

        div()
            .id("editor-scroll")
            .flex_1()
            .flex()
            .flex_col()
            .gap(px(10.))
            .overflow_y_scroll()
            .child(
                card(c).p(px(12.)).flex().flex_col().gap(px(8.))
                    .child(section(c, &t("连接", "Connection")))
                    .child(self.field_row(c, &t("名称", "Name"), Field::Name))
                    .child(
                        div().flex().gap(px(8.))
                            .child(div().flex_1().child(self.field_row(c, "Host", Field::Host)))
                            .child(div().w(px(130.)).child(self.field_row(c, "Port", Field::Port))),
                    )
                    .child(
                        div().flex().gap(px(8.))
                            .child(div().flex_1().child(self.field_row(c, &t("客户端 ID", "Client ID"), Field::ClientId)))
                            .child(
                                button(c, "gen-id", t("生成", "Generate"), Kind::Ghost)
                                    .on_click(cx.listener(|this, _: &ClickEvent, _, cx| {
                                        this.rt.form.client_id = default_client_id();
                                        this.rt.apply_form();
                                        this.mark_dirty(cx);
                                        this.sync_inputs(cx);
                                    })),
                            ),
                    )
                    .child(
                        div().flex().gap(px(8.))
                            .child(div().flex_1().child(self.field_row(c, &t("用户名", "Username"), Field::Username)))
                            .child(div().flex_1().child(self.field_row(c, &t("密码", "Password"), Field::Password))),
                    ),
            )
            .child(
                card(c).p(px(12.)).flex().flex_col().gap(px(8.))
                    .child(section(c, &t("高级", "Advanced")))
                    .child(
                        div().flex().items_center().gap(px(8.))
                            .child(self.select(c, "protocol", format!("协议 {}", self.opt_text(Opt::Protocol))))
                            .on_mouse_up(gpui::MouseButton::Left, cx.listener(|this, _, _, cx| this.cycle(Opt::Protocol, cx)))
                            .child(checkbox(c, "clean", &t("清理会话", "Clean session"), f.clean_session)
                                .on_click(cx.listener(|this, _: &ClickEvent, _, cx| {
                                    this.rt.form.clean_session = !this.rt.form.clean_session;
                                    this.rt.apply_form();
                                    this.mark_dirty(cx);
                                }))),
                    )
                    .child(
                        div().flex().gap(px(8.))
                            .child(div().flex_1().child(self.field_row(c, &t("Keep Alive", "Keep alive"), Field::KeepAlive)))
                            .child(div().flex_1().child(self.field_row(c, &t("超时（秒）", "Timeout (s)"), Field::Timeout))),
                    ),
            )
            .child(
                card(c).p(px(12.)).flex().flex_col().gap(px(8.))
                    .child(
                        div().flex().items_center().gap(px(10.))
                            .child(section(c, &t("TLS", "TLS")))
                            .child(checkbox(c, "ssl", "SSL/TLS", f.ssl)
                                .on_click(cx.listener(|this, _: &ClickEvent, _, cx| {
                                    this.rt.form.ssl = !this.rt.form.ssl;
                                    if this.rt.form.ssl && this.rt.form.port == "1883" {
                                        this.rt.form.port = "8883".to_owned();
                                    }
                                    this.rt.apply_form();
                                    this.mark_dirty(cx);
                                    this.sync_inputs(cx);
                                })))
                            .child(checkbox(c, "verify", &t("校验证书", "Verify"), f.ssl_verify)
                                .on_click(cx.listener(|this, _: &ClickEvent, _, cx| {
                                    this.rt.form.ssl_verify = !this.rt.form.ssl_verify;
                                    this.rt.apply_form();
                                    this.mark_dirty(cx);
                                }))),
                    )
                    .child(self.field_row(c, "CA", Field::Ca))
                    .child(self.field_row(c, &t("客户端证书", "Client cert"), Field::Cert))
                    .child(self.field_row(c, &t("客户端私钥", "Client key"), Field::Key)),
            )
            .child(
                card(c).p(px(12.)).flex().flex_col().gap(px(8.))
                    .child(section(c, &t("遗嘱消息", "Last will")))
                    .child(self.field_row(c, &t("Topic", "Topic"), Field::WillTopic))
                    .child(self.field_row(c, "Payload", Field::WillPayload))
                    .child(
                        div().flex().items_center().gap(px(8.))
                            .child(self.select(c, "will-qos", self.opt_text(Opt::WillQos)))
                            .on_mouse_up(gpui::MouseButton::Left, cx.listener(|this, _, _, cx| this.cycle(Opt::WillQos, cx)))
                            .child(checkbox(c, "will-retain", "Retain", f.will_retain)
                                .on_click(cx.listener(|this, _: &ClickEvent, _, cx| {
                                    this.rt.form.will_retain = !this.rt.form.will_retain;
                                    this.rt.apply_form();
                                    this.mark_dirty(cx);
                                }))),
                    ),
            )
            .child(
                div().flex().items_center().gap(px(8.))
                    .child(if connecting {
                        button(c, "disconnect", t("断开", "Disconnect"), Kind::Danger)
                            .on_click(cx.listener(|this, _: &ClickEvent, _, cx| {
                                this.rt.apply_form();
                                this.rt.disconnect();
                                cx.notify();
                            }))
                    } else {
                        button(c, "connect", t("连接", "Connect"), Kind::Primary)
                            .on_click(cx.listener(|this, _: &ClickEvent, _, cx| {
                                this.rt.apply_form();
                                this.rt.persist();
                                this.rt.connect();
                                this.ensure_ticker(cx);
                                cx.notify();
                            }))
                    })
                    .child(
                        button(c, "to-session", t("打开会话", "Open session"), Kind::Ghost)
                            .on_click(cx.listener(|this, _: &ClickEvent, _, cx| this.go(1, cx))),
                    )
                    .child(hint(c, t("编辑会自动保存（400ms 防抖）", "Edits save automatically (400ms debounce)"))),
            )
    }

    fn page_session(&self, c: &Colors, cx: &mut Context<Self>) -> Div {
        let connected = self
            .rt
            .active_id()
            .map(|id| self.rt.is_connected(&id))
            .unwrap_or(false);
        let status = self.rt.status();

        let header = card(c)
            .p(px(8.))
            .flex()
            .items_center()
            .gap(px(10.))
            .child(
                div()
                    .flex_1()
                    .flex()
                    .items_center()
                    .gap(px(8.))
                    .child(dot(theme_rgb(status.rgb())))
                    .child(
                        div()
                            .text_size(px(16.))
                            .text_color(c.text_title)
                            .child(self.rt.active().map(|c| c.name.clone()).unwrap_or_default()),
                    )
                    .child(hint(c, format!("{} · {}", status.text(), self.rt.active_sub_topic()))),
            )
            .child(
                button(c, "edit-conn", t("编辑", "Edit"), Kind::Ghost)
                    .on_click(cx.listener(|this, _: &ClickEvent, _, cx| this.go(0, cx))),
            )
            .child(if connected {
                button(c, "disc", t("断开", "Disconnect"), Kind::Danger).on_click(
                    cx.listener(|this, _: &ClickEvent, _, cx| {
                        this.rt.disconnect();
                        cx.notify();
                    }),
                )
            } else {
                button(c, "conn", t("连接", "Connect"), Kind::Primary).on_click(
                    cx.listener(|this, _: &ClickEvent, _, cx| {
                        this.rt.connect();
                        this.ensure_ticker(cx);
                        cx.notify();
                    }),
                )
            });

        div()
            .flex_1()
            .flex()
            .flex_col()
            .gap(px(10.))
            .overflow_hidden()
            .child(header)
            .child(
                div()
                    .flex_1()
                    .flex()
                    .gap(px(10.))
                    .overflow_hidden()
                    .child(self.subscribe_card(c, cx))
                    .child(
                        div().flex_1().flex().flex_col().gap(px(10.)).overflow_hidden()
                            .child(self.logs_card(c, cx))
                            .child(self.publish_card(c, cx)),
                    ),
            )
    }

    fn subscribe_card(&self, c: &Colors, cx: &mut Context<Self>) -> Div {
        let active_topic = self.rt.active_sub_topic();
        let subs: Vec<(usize, String, i32)> = self
            .rt
            .subscriptions()
            .iter()
            .enumerate()
            .map(|(i, s)| (i, s.topic.clone(), s.qos as i32))
            .collect();

        let mut list = div().flex().flex_col().gap(px(3.));
        for (index, topic, _qos) in subs {
            let selected = !active_topic.is_empty() && active_topic == topic;
            let t_for_click = topic.clone();
            let row = div()
                .flex()
                .items_center()
                .gap(px(4.))
                .child(
                    button(
                        c,
                        SharedString::from(format!("sub-{index}")),
                        div().text_size(px(11.5)).child(topic),
                        if selected { Kind::Selected } else { Kind::Ghost },
                    )
                    .flex_1()
                    .justify_start()
                    .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                        this.rt.toggle_sub_filter(&t_for_click);
                        cx.notify();
                    })),
                )
                .child(
                    button(c, SharedString::from(format!("unsub-{index}")), "×", Kind::Danger)
                        .px(px(6.))
                        .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                            this.rt.unsubscribe(index);
                            cx.notify();
                        })),
                );
            list = list.child(row);
        }

        card(c)
            .w(px(232.))
            .flex_shrink_0()
            .flex()
            .flex_col()
            .gap(px(6.))
            .p(px(10.))
            .child(section(c, &t("订阅", "Subscriptions")))
            .child(well(c).text_color(c.text_body).child(self.input(Field::SubTopic)))
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(5.))
                    .child(self.select(c, "sub-qos", self.opt_text(Opt::SubQos)))
                    .on_mouse_up(gpui::MouseButton::Left, cx.listener(|this, _, _, cx| this.cycle(Opt::SubQos, cx)))
                    .child(
                        button(c, "subscribe", t("订阅", "Subscribe"), Kind::Primary).on_click(
                            cx.listener(|this, _: &ClickEvent, _, cx| {
                                this.rt.apply_form();
                                this.rt.subscribe();
                                this.rt.persist();
                                cx.notify();
                            }),
                        ),
                    ),
            )
            .child(div().h(px(1.)).bg(c.hover_bg))
            .child(list)
    }

    fn logs_card(&self, c: &Colors, cx: &mut Context<Self>) -> Div {
        let count = self.rt.visible_logs().len();
        let list = uniform_list(
            "logs",
            count,
            cx.processor(|this: &mut Root, range: std::ops::Range<usize>, _window: &mut Window, _cx: &mut Context<Root>| {
                let logs = this.rt.visible_logs();
                let c = this.colors();
                let fmt = PayloadFormat::from_index(this.rt.prefs.log_format);
                range
                    .map(|ix| {
                        let log = logs[ix];
                        let (bg, line_color) = match log.kind.as_str() {
                            "RECV" => (c.recv_bg, c.recv_topic),
                            "SEND" => (c.send_bg, c.send_topic),
                            "ERR" => (c.err_bg, c.err_label),
                            _ => (c.sys_bg, c.sys_text),
                        };
                        let body = if log.is_payload_kind() {
                            payload::render_payload(&log.payload, fmt)
                        } else {
                            String::from_utf8_lossy(&log.payload).into_owned()
                        };
                        let line = if log.topic.is_empty() {
                            format!("{} · {} · {}", log.time, log.kind, body)
                        } else {
                            format!("{} · {} · {} · {}", log.time, log.kind, log.topic, body)
                        };
                        div()
                            .h(px(18.))
                            .w_full()
                            .px(px(6.))
                            .bg(bg)
                            .text_size(px(11.5))
                            .text_color(line_color)
                            .whitespace_nowrap()
                            .overflow_hidden()
                            .child(line)
                            .into_any_element()
                    })
                    .collect::<Vec<_>>()
            }),
        )
        .h_full()
        .track_scroll(self.log_scroll.clone());

        card(c)
            .flex_1()
            .flex()
            .flex_col()
            .gap(px(6.))
            .p(px(10.))
            .overflow_hidden()
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(6.))
                    .child(section(c, &t("日志", "Logs")))
                    .child(well(c).w(px(180.)).text_color(c.text_body).child(self.filter_input.clone()))
                    .child(self.select(c, "kind", self.opt_text(Opt::Kind)))
                    .on_mouse_up(gpui::MouseButton::Left, cx.listener(|this, _, _, cx| this.cycle(Opt::Kind, cx)))
                    .child(self.select(c, "log-format", self.opt_text(Opt::LogFormat)))
                    .on_mouse_up(gpui::MouseButton::Left, cx.listener(|this, _, _, cx| this.cycle(Opt::LogFormat, cx)))
                    .child(checkbox(c, "autoscroll", &t("自动滚动", "Auto-scroll"), self.rt.prefs.auto_scroll)
                        .on_click(cx.listener(|this, _: &ClickEvent, _, cx| {
                            this.rt.set_auto_scroll(!this.rt.prefs.auto_scroll);
                            cx.notify();
                        })))
                    .child(div().flex_1())
                    .child(
                        button(c, "save-logs", t("保存", "Save"), Kind::Ghost).on_click(
                            cx.listener(|this, _: &ClickEvent, _, _| {
                                let _ = this.rt.save_logs();
                            }),
                        ),
                    )
                    .child(
                        button(c, "clear-logs", t("清空", "Clear"), Kind::Ghost).on_click(
                            cx.listener(|this, _: &ClickEvent, _, cx| {
                                this.rt.clear_logs();
                                cx.notify();
                            }),
                        ),
                    ),
            )
            .child(
                div()
                    .flex_1()
                    .overflow_hidden()
                    .rounded(px(6.))
                    .child(list),
            )
    }

    fn publish_card(&self, c: &Colors, cx: &mut Context<Self>) -> Div {
        let timed = self.rt.timed_running();
        let presets: Vec<(usize, String)> = self
            .rt
            .form
            .presets
            .iter()
            .enumerate()
            .map(|(i, p)| (i, if p.name.is_empty() { p.topic.clone() } else { p.name.clone() }))
            .collect();
        let mut preset_row = div().flex().items_center().gap(px(4.));
        for (index, name) in presets {
            preset_row = preset_row.child(
                button(c, SharedString::from(format!("preset-{index}")), div().text_size(px(11.)).child(name), Kind::Ghost)
                    .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                        this.rt.publish_preset(index);
                        this.rt.persist();
                        cx.notify();
                    })),
            );
        }

        card(c)
            .flex_shrink_0()
            .flex()
            .flex_col()
            .gap(px(6.))
            .p(px(10.))
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(6.))
                    .child(section(c, &t("发布", "Publish")))
                    .child(div().flex_1())
                    .child(preset_row),
            )
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(5.))
                    .child(well(c).flex_1().text_color(c.text_body).child(self.input(Field::PubTopic)))
                    .child(self.select(c, "pub-qos", self.opt_text(Opt::PubQos)))
                    .on_mouse_up(gpui::MouseButton::Left, cx.listener(|this, _, _, cx| this.cycle(Opt::PubQos, cx)))
                    .child(checkbox(c, "pub-retain", "Retain", self.rt.form.pub_retain)
                        .on_click(cx.listener(|this, _: &ClickEvent, _, cx| {
                            this.rt.form.pub_retain = !this.rt.form.pub_retain;
                            this.rt.apply_form();
                            this.mark_dirty(cx);
                        })))
                    .child(self.select(c, "pub-format", self.opt_text(Opt::PubFormat)))
                    .on_mouse_up(gpui::MouseButton::Left, cx.listener(|this, _, _, cx| this.cycle(Opt::PubFormat, cx))),
            )
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(6.))
                    .child(
                        well(c)
                            .flex_1()
                            .text_color(c.text_body)
                            .child(self.input(Field::PubPayload)),
                    )
                    .child(
                        button(c, "pretty", t("美化", "Pretty"), Kind::Ghost).on_click(
                            cx.listener(|this, _: &ClickEvent, _, cx| {
                                let _ = this.rt.format_payload();
                                this.sync_inputs(cx);
                                cx.notify();
                            }),
                        ),
                    ),
            )
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(6.))
                    .child(well(c).w(px(70.)).text_color(c.text_body).child(self.input(Field::TimedInterval)))
                    .child(hint(c, t("秒一次", "sec interval")))
                    .child(
                        button(
                            c,
                            "timed",
                            if timed {
                                t("停止定时", "Stop timed")
                            } else {
                                t("开始定时", "Start timed")
                            },
                            if timed { Kind::Danger } else { Kind::Ghost },
                        )
                        .on_click(cx.listener(|this, _: &ClickEvent, _, cx| {
                            this.rt.apply_form();
                            this.rt.toggle_timed();
                            this.ensure_ticker(cx);
                            cx.notify();
                        })),
                    )
                    .child(div().flex_1())
                    .child(
                        button(c, "publish", t("发布", "Publish"), Kind::Primary).on_click(
                            cx.listener(|this, _: &ClickEvent, _, cx| {
                                this.rt.apply_form();
                                this.rt.publish_form();
                                this.rt.persist();
                                cx.notify();
                            }),
                        ),
                    ),
            )
    }

    fn page_settings(&self, c: &Colors, cx: &mut Context<Self>) -> Div {
        card(c)
            .p(px(16.))
            .flex()
            .flex_col()
            .gap(px(10.))
            .child(section(c, &t("外观", "Appearance")))
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(8.))
                    .child(div().w(px(80.)).text_size(px(12.)).text_color(c.text_muted).child(t("皮肤", "Skin")))
                    .child(self.select(c, "skin", self.opt_text(Opt::Skin)))
                    .on_mouse_up(gpui::MouseButton::Left, cx.listener(|this, _, _, cx| this.cycle(Opt::Skin, cx))),
            )
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(8.))
                    .child(div().w(px(80.)).text_size(px(12.)).text_color(c.text_muted).child(t("语言", "Language")))
                    .child(self.select(c, "lang", self.opt_text(Opt::Lang)))
                    .on_mouse_up(gpui::MouseButton::Left, cx.listener(|this, _, _, cx| this.cycle(Opt::Lang, cx))),
            )
            .child(hint(
                c,
                t(
                    "跟随系统皮肤仅在系统主题变化时刷新（1s 轮询）。",
                    "The System skin refreshes on a 1s poll.",
                ),
            ))
    }

    fn page_rules(&self, c: &Colors, cx: &mut Context<Self>) -> gpui::Stateful<Div> {
        let mut list = div().flex().flex_col().gap(px(8.));
        let rules: Vec<(usize, bool, String, String, String, String, i32, i32, bool)> = self
            .rt
            .form
            .rules
            .iter()
            .enumerate()
            .map(|(i, r)| {
                (
                    i,
                    r.enabled,
                    r.match_topic.clone(),
                    r.payload_contains.clone(),
                    r.reply_topic.clone(),
                    r.reply_payload.clone(),
                    r.qos,
                    r.format,
                    r.retain,
                )
            })
            .collect();
        for (i, enabled, match_topic, contains, reply_topic, reply_payload, qos, format, retain) in rules {
            let card_row = card(c)
                .p(px(10.))
                .flex()
                .flex_col()
                .gap(px(6.))
                .child(
                    div()
                        .flex()
                        .items_center()
                        .gap(px(8.))
                        .child(checkbox(c, SharedString::from(format!("rule-{i}")), "", enabled)
                            .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                                let on = !this.rt.form.rules[i].enabled;
                                this.toggle_rule(i, on, cx);
                            })))
                        .child(
                            div()
                                .text_size(px(12.))
                                .text_color(c.text_primary)
                                .child(if match_topic.is_empty() { "*".to_string() } else { match_topic }),
                        )
                        .child(div().flex_1())
                        .child(self.select(c, "rule-qos", format!("QoS {qos}")))
                        .on_mouse_up(gpui::MouseButton::Left, cx.listener(move |this, _, _, cx| this.cycle_rule(i, true, cx)))
                        .child(self.select(c, "rule-format", fmt_name(PayloadFormat::from_index(format))))
                        .on_mouse_up(gpui::MouseButton::Left, cx.listener(move |this, _, _, cx| this.cycle_rule(i, false, cx)))
                        .child(checkbox(c, SharedString::from(format!("rule-retain-{i}")), "Retain", retain)
                            .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                                if let Some(r) = this.rt.form.rules.get_mut(i) {
                                    r.retain = !r.retain;
                                }
                                this.rt.apply_form();
                                this.mark_dirty(cx);
                            })))
                        .child(
                            button(c, SharedString::from(format!("rule-del-{i}")), "×", Kind::Danger)
                                .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                                    this.rt.form.remove_rule(i);
                                    this.rt.apply_form();
                                    this.rt.persist();
                                    cx.notify();
                                })),
                        ),
                )
                .child(
                    div()
                        .flex()
                        .items_center()
                        .gap(px(8.))
                        .child(hint(c, format!("contains: {contains}")))
                        .child(hint(c, format!("→ {reply_topic}  {reply_payload}"))),
                );
            list = list.child(card_row);
        }

        div()
            .id("rules-scroll")
            .flex_1()
            .flex()
            .flex_col()
            .gap(px(10.))
            .overflow_y_scroll()
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(8.))
                    .child(section(c, &t("自动回复", "Auto reply")))
                    .child(div().flex_1())
                    .child(hint(c, t("自身回投", "Own echo")))
                    .child(self.select(c, "echo", self.opt_text(Opt::Echo)))
                    .on_mouse_up(gpui::MouseButton::Left, cx.listener(|this, _, _, cx| this.cycle(Opt::Echo, cx)))
                    .child(
                        button(c, "add-rule", t("新增规则", "Add rule"), Kind::Primary).on_click(
                            cx.listener(|this, _: &ClickEvent, _, cx| {
                                this.rt.form.add_rule();
                                this.rt.apply_form();
                                this.rt.persist();
                                cx.notify();
                            }),
                        ),
                    ),
            )
            .child(list)
            .child(hint(
                c,
                t(
                    "规则文本在 GPUI 版只读，可在 egui/iced/早期 Slint 版中编辑。",
                    "Rule text is read-only in the GPUI build; edit it in the egui/iced/reference builds.",
                ),
            ))
    }

    fn page_presets(&self, c: &Colors, cx: &mut Context<Self>) -> gpui::Stateful<Div> {
        let mut list = div().flex().flex_col().gap(px(8.));
        let presets: Vec<(usize, String, String, String, i32, i32, bool)> = self
            .rt
            .form
            .presets
            .iter()
            .enumerate()
            .map(|(i, p)| (i, p.name.clone(), p.topic.clone(), p.payload.clone(), p.qos, p.format, p.retain))
            .collect();
        for (i, name, topic, payload, qos, format, retain) in presets {
            list = list.child(
                card(c)
                    .p(px(10.))
                    .flex()
                    .flex_col()
                    .gap(px(6.))
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap(px(8.))
                            .child(
                                div()
                                    .text_size(px(13.))
                                    .text_color(c.text_primary)
                                    .child(if name.is_empty() { topic.clone() } else { name }),
                            )
                            .child(hint(c, format!("{} · QoS {qos} · {} · retain={retain}", topic, fmt_name(PayloadFormat::from_index(format)))))
                            .child(div().flex_1())
                            .child(
                                button(c, SharedString::from(format!("send-{i}")), t("发送", "Send"), Kind::Primary)
                                    .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                                        this.rt.publish_preset(i);
                                        this.rt.persist();
                                        cx.notify();
                                    })),
                            )
                            .child(
                                button(c, SharedString::from(format!("preset-del-{i}")), "×", Kind::Danger)
                                    .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                                        this.rt.form.remove_preset(i);
                                        this.rt.apply_form();
                                        this.rt.persist();
                                        cx.notify();
                                    })),
                            ),
                    )
                    .child(hint(c, payload)),
            );
        }

        div()
            .id("presets-scroll")
            .flex_1()
            .flex()
            .flex_col()
            .gap(px(10.))
            .overflow_y_scroll()
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(8.))
                    .child(section(c, &t("发送预设", "Publish presets")))
                    .child(div().flex_1())
                    .child(
                        button(c, "add-preset", t("新增预设", "Add preset"), Kind::Primary).on_click(
                            cx.listener(|this, _: &ClickEvent, _, cx| {
                                this.rt.form.add_preset();
                                this.rt.apply_form();
                                this.rt.persist();
                                cx.notify();
                            }),
                        ),
                    ),
            )
            .child(list)
            .child(hint(
                c,
                t(
                    "预设内容在 GPUI 版只读；会话页底部有发送按钮。",
                    "Preset contents are read-only in GPUI; the session page has send buttons.",
                ),
            ))
    }

    fn page_about(&self, c: &Colors) -> El {
        card(c)
            .p(px(16.))
            .flex()
            .flex_col()
            .gap(px(8.))
            .child(div().text_size(px(22.)).text_color(c.text_title).child("mqttc · gpui"))
            .child(div().text_size(px(12.)).text_color(c.text_faint).child(format!("v{}", env!("CARGO_PKG_VERSION"))))
            .child(
                div()
                    .text_size(px(13.))
                    .child(t(
                        "轻量级跨平台 MQTT 调试客户端，核心逻辑与 egui / iced 版本共用。",
                        "A lightweight cross-platform MQTT debug client sharing its core with the egui / iced builds.",
                    )),
            )
            .child(div().text_size(px(12.)).text_color(c.text_muted).child(
                t(
                    "渲染：GPUI 原生 GPU；组件为自绘（无第三方控件库）。",
                    "Rendering: native GPUI GPU; widgets are hand-drawn (no widget kit).",
                ),
            ))
            .child(hint(c, "MQTT 3.1.1 / 5.0 · TLS · Last Will · Auto reply · Timed publish".to_string()))
            .child(hint(c, "License: MIT".to_string()))
            .into_any_element()
    }
}
