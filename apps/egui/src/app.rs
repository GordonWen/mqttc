//! The single window: sidebar + six pages (editor, session, settings, auto-reply,
//! presets, about), matching the earlier Slint version's window layout.

use std::time::Instant;

use eframe::egui::{
    self, Align, Align2, Button, Checkbox, ComboBox, CornerRadius, Frame, Label,
    Layout, Margin, Rect, RichText, ScrollArea, Sense, TextEdit, Ui, Vec2,
};
use mqttc_core::cli::BenchOptions;
use mqttc_core::i18n::t;
use mqttc_core::model::{default_client_id, ConnStatus, LogRecord, PayloadFormat, Skin};
use mqttc_core::payload;
use mqttc_core::runtime::AppRuntime;
use mqttc_core::theme::Palette;

use crate::bench::Bench;
use crate::style;

/// Widgets only queue actions; the runtime is mutated after the frame is laid out.
enum Action {
    Select(String),
    NewConnection,
    Delete(String),
    Page(i32),
    Connect,
    Disconnect,
    Subscribe,
    Unsubscribe(usize),
    ToggleSub(String),
    Publish,
    PublishPreset(usize),
    FormatPayload,
    ToggleTimed,
    GenerateClientId,
    SaveLogs,
    ClearLogs,
    Skin(i32),
    Language(i32),
    AddRule,
    DeleteRule(usize),
    AddPreset,
    DeletePreset(usize),
}

pub struct App {
    runtime: AppRuntime,
    page: i32,
    started: Instant,
    first_frame_ms: Option<f64>,
    bench: Option<Bench>,
    /// SSL checkbox toggles the common plain/TLS port like the Slint version.
    last_ssl: bool,
}

impl App {
    pub fn new(cc: &eframe::CreationContext<'_>, options: BenchOptions) -> Self {
        let ctx = cc.egui_ctx.clone();
        let wake = std::sync::Arc::new(move || ctx.request_repaint());
        let bench = options.bench.then(|| Bench::new(options.clone()));
        let runtime = AppRuntime::load(&options, Some(wake));
        let last_ssl = runtime.form.ssl;
        Self {
            runtime,
            page: 0,
            started: Instant::now(),
            first_frame_ms: None,
            bench,
            last_ssl,
        }
    }

    fn apply_actions(&mut self, actions: Vec<Action>) {
        for action in actions {
            match action {
                Action::Select(id) => {
                    if self.runtime.select_connection(&id) {
                        self.page = 1;
                    }
                }
                Action::NewConnection => {
                    self.runtime.new_connection();
                    self.page = 0;
                }
                Action::Delete(id) => {
                    self.runtime.delete_connection(&id);
                    self.page = 1;
                }
                Action::Page(page) => {
                    if page == 1 {
                        self.runtime.apply_form();
                        self.runtime.persist();
                    }
                    self.page = page;
                }
                Action::Connect => {
                    self.runtime.connect();
                }
                Action::Disconnect => self.runtime.disconnect(),
                Action::Subscribe => self.runtime.subscribe(),
                Action::Unsubscribe(index) => self.runtime.unsubscribe(index),
                Action::ToggleSub(topic) => self.runtime.toggle_sub_filter(&topic),
                Action::Publish => {
                    self.runtime.publish_form();
                }
                Action::PublishPreset(index) => {
                    self.runtime.publish_preset(index);
                }
                Action::FormatPayload => {
                    let _ = self.runtime.format_payload();
                }
                Action::ToggleTimed => self.runtime.toggle_timed(),
                Action::GenerateClientId => {
                    self.runtime.form.client_id = default_client_id();
                    self.runtime.apply_form();
                    self.runtime.persist();
                }
                Action::SaveLogs => {
                    let _ = self.runtime.save_logs();
                }
                Action::ClearLogs => self.runtime.clear_logs(),
                Action::Skin(index) => self.runtime.set_skin_choice(index),
                Action::Language(index) => self.runtime.set_language(index),
                Action::AddRule => {
                    self.runtime.form.add_rule();
                    self.runtime.mark_dirty();
                }
                Action::DeleteRule(index) => {
                    self.runtime.form.remove_rule(index);
                    self.runtime.mark_dirty();
                }
                Action::AddPreset => {
                    self.runtime.form.add_preset();
                    self.runtime.mark_dirty();
                }
                Action::DeletePreset(index) => {
                    self.runtime.form.remove_preset(index);
                    self.runtime.mark_dirty();
                }
            }
        }
    }

    // ------------------------------------------------------------- sidebar

    fn sidebar(&mut self, ui: &mut Ui, pal: &Palette, actions: &mut Vec<Action>) {
        Frame::new()
            .fill(style::color(pal.panel_bg))
            .corner_radius(10)
            .inner_margin(Margin::same(8))
            .show(ui, |ui| {
                ui.set_width(ui.available_width());
                ui.horizontal(|ui| {
                    ui.label(
                        RichText::new(t("连接", "Conns"))
                            .color(style::color(pal.text_title))
                            .strong(),
                    );
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        if ui.add_sized([30.0, 26.0], Button::new("+")).clicked() {
                            actions.push(Action::NewConnection);
                        }
                    });
                });

                let button_h = 30.0;
                let list_h = (ui.available_height() - button_h * 4.0 - 46.0).max(120.0);
                Frame::new()
                    .fill(style::color(pal.inset_bg))
                    .corner_radius(8)
                    .inner_margin(Margin::same(4))
                    .show(ui, |ui| {
                        ui.set_width(ui.available_width());
                        ScrollArea::vertical()
                            .id_salt("conn-list")
                            .max_height(list_h)
                            .auto_shrink([false, false])
                            .show(ui, |ui| {
                                ui.set_width(ui.available_width());
                                let entries: Vec<(String, String, ConnStatus)> = self
                                    .runtime
                                    .config
                                    .connections
                                    .iter()
                                    .map(|conn| {
                                        (
                                            conn.id.clone(),
                                            conn.name.clone(),
                                            self.runtime.status_of(&conn.id),
                                        )
                                    })
                                    .collect();
                                for (id, name, status) in entries {
                                    let selected = self.runtime.config.active_id == id;
                                    if let Some(action) =
                                        connection_row(ui, pal, &id, &name, status, selected)
                                    {
                                        actions.push(action);
                                    }
                                    ui.add_space(2.0);
                                }
                            });
                    });

                ui.add_space(8.0);
                for (label, page) in [
                    (t("自动回复", "Auto-reply"), 3),
                    (t("发布预设", "Presets"), 4),
                    (t("设置", "Settings"), 2),
                    (t("关于", "About"), 5),
                ] {
                    let button = Button::new(RichText::new(label).color(style::color(pal.text_body)));
                    if ui
                        .add_sized([ui.available_width(), button_h], button)
                        .clicked()
                    {
                        actions.push(Action::Page(page));
                    }
                }
            });
    }

    // --------------------------------------------------------------- pages

    fn page_editor(&mut self, ui: &mut Ui, pal: &Palette, actions: &mut Vec<Action>, dirty: &mut bool) {
        let status = self.runtime.status();
        let name = self.runtime.form.name.clone();
        Frame::new()
            .fill(style::color(pal.panel_bg))
            .corner_radius(10)
            .inner_margin(Margin::same(16))
            .show(ui, |ui| {
                ui.set_width(ui.available_width());
                ui.horizontal(|ui| {
                    ui.label(
                        RichText::new(t("连接配置", "Connection"))
                            .size(16.0)
                            .strong()
                            .color(style::color(pal.text_primary)),
                    );
                    ui.label(RichText::new(&name).color(style::color(pal.text_muted)));
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        if ui.add_sized([88.0, 28.0], Button::new(t("连接", "Connect"))).clicked()
                            && status == ConnStatus::Disconnected
                        {
                            actions.push(Action::Connect);
                        }
                        if ui.add_sized([88.0, 28.0], Button::new(t("返回会话", "Back"))).clicked() {
                            actions.push(Action::Page(1));
                        }
                    });
                });
                ui.add_space(6.0);

                let enabled = status == ConnStatus::Disconnected;
                ScrollArea::vertical()
                    .id_salt("editor")
                    .auto_shrink([false, false])
                    .show(ui, |ui| {
                        ui.set_width(ui.available_width());
                        section(ui, pal, t("基础信息", "General"));

                        ui.horizontal(|ui| {
                            *dirty |= field(ui, pal, t("名称", "Name"), &mut self.runtime.form.name, 220.0, enabled, t("连接名称", "Connection name"), false);
                            *dirty |= field(ui, pal, t("主机", "Host"), &mut self.runtime.form.host, 220.0, enabled, "127.0.0.1", false);
                            *dirty |= field(ui, pal, t("端口", "Port"), &mut self.runtime.form.port, 100.0, enabled, "1883", false);
                            ui.vertical(|ui| {
                                field_label(ui, pal, t("MQTT 协议", "MQTT protocol"));
                                let options = ["3.1.1".to_string(), "5.0".to_string()];
                                *dirty |= combo(ui, "protocol", 100.0, &mut self.runtime.form.protocol, &options, enabled);
                            });
                        });

                        ui.horizontal(|ui| {
                            ui.vertical(|ui| {
                                field_label(ui, pal, "Client ID");
                                ui.horizontal(|ui| {
                                    let width = 300.0;
                                    *dirty |= text_edit(ui, &mut self.runtime.form.client_id, width, enabled, t("空则自动生成", "Empty = auto"), false);
                                    if ui
                                        .add_enabled(enabled, Button::new(t("生成", "Gen")))
                                        .clicked()
                                    {
                                        actions.push(Action::GenerateClientId);
                                    }
                                });
                            });
                            *dirty |= field(ui, pal, t("用户名", "Username"), &mut self.runtime.form.username, 220.0, enabled, t("可选，空则匿名", "Optional, empty = anonymous"), false);
                            *dirty |= field(ui, pal, t("密码", "Password"), &mut self.runtime.form.password, 220.0, enabled, t("可选", "Optional"), true);
                        });

                        section(ui, pal, t("高级参数", "Advanced"));
                        ui.horizontal(|ui| {
                            *dirty |= field(ui, pal, t("Keep Alive (秒)", "Keep Alive (s)"), &mut self.runtime.form.keep_alive, 140.0, enabled, "60", false);
                            *dirty |= field(ui, pal, t("连接超时 (秒)", "Timeout (s)"), &mut self.runtime.form.connect_timeout, 140.0, enabled, "10", false);
                            ui.vertical(|ui| {
                                ui.add_space(18.0);
                                *dirty |= ui
                                    .add_enabled(enabled, Checkbox::new(&mut self.runtime.form.clean_session, "Clean Session"))
                                    .changed();
                            });
                        });

                        section(ui, pal, "SSL / TLS");
                        ui.horizontal(|ui| {
                            *dirty |= ui
                                .add_enabled(enabled, Checkbox::new(&mut self.runtime.form.ssl, t("启用 SSL", "Enable SSL")))
                                .changed();
                            *dirty |= ui
                                .add_enabled(
                                    self.runtime.form.ssl && enabled,
                                    Checkbox::new(&mut self.runtime.form.ssl_verify, t("校验服务器证书", "Verify certificate")),
                                )
                                .changed();
                        });
                        if self.runtime.form.ssl {
                            ui.add_space(4.0);
                            let ca_width = ui.available_width().min(520.0);
                            *dirty |= field(ui, pal, t("CA 证书路径", "CA certificate"), &mut self.runtime.form.ssl_ca, ca_width, enabled, t("可选，空则使用系统 CA", "Optional, empty = system CA"), false);
                            ui.horizontal(|ui| {
                                *dirty |= field(ui, pal, t("客户端证书", "Client cert"), &mut self.runtime.form.ssl_cert, 320.0, enabled, t("可选，双向认证时填写", "Optional, for mutual TLS"), false);
                                *dirty |= field(ui, pal, t("客户端私钥", "Client key"), &mut self.runtime.form.ssl_key, 320.0, enabled, t("可选，与证书成对", "Optional, pair with cert"), false);
                            });
                            hint(ui, pal, t("端口常用 8883。关闭校验仅用于自签证书调试。", "Port 8883 is common. Turn off verify only for self-signed certs."));
                        }

                        section(ui, pal, t("遗嘱消息 (Last Will)", "Last Will"));
                        ui.horizontal(|ui| {
                            *dirty |= field(ui, pal, t("遗嘱 Topic", "Will topic"), &mut self.runtime.form.will_topic, 360.0, enabled, t("留空则不发送遗嘱", "Empty = no will"), false);
                            *dirty |= field(ui, pal, t("遗嘱 Payload", "Will payload"), &mut self.runtime.form.will_payload, 360.0, enabled, t("异常断开时由 Broker 代发", "Sent by broker on unexpected disconnect"), false);
                        });
                        ui.horizontal(|ui| {
                            ui.label(RichText::new(t("遗嘱 QoS", "Will QoS")).size(12.0).color(style::color(pal.text_muted)));
                            let options = ["0".to_string(), "1".to_string(), "2".to_string()];
                            *dirty |= combo(ui, "will-qos", 56.0, &mut self.runtime.form.will_qos, &options, enabled);
                            *dirty |= ui
                                .add_enabled(enabled, Checkbox::new(&mut self.runtime.form.will_retain, "Retain"))
                                .changed();
                        });
                        ui.add_space(8.0);
                    });
            });

        // Toggling SSL nudges the port between the usual plain/TLS values.
        if self.runtime.form.ssl != self.last_ssl {
            if self.runtime.form.ssl && self.runtime.form.port.trim() == "1883" {
                self.runtime.form.port = "8883".into();
            } else if !self.runtime.form.ssl && self.runtime.form.port.trim() == "8883" {
                self.runtime.form.port = "1883".into();
            }
            self.last_ssl = self.runtime.form.ssl;
            *dirty = true;
        }
    }

    fn page_session(&mut self, ui: &mut Ui, pal: &Palette, actions: &mut Vec<Action>, dirty: &mut bool) {
        let status = self.runtime.status();
        let connected = status == ConnStatus::Connected;
        let connecting = status == ConnStatus::Connecting;
        let name = self.runtime.form.name.clone();
        let summary = format!("{}:{}", self.runtime.form.host, self.runtime.form.port);

        // Header card.
        Frame::new()
            .fill(style::color(pal.panel_bg))
            .corner_radius(10)
            .inner_margin(Margin::same(12))
            .show(ui, |ui| {
                ui.set_width(ui.available_width());
                ui.horizontal(|ui| {
                    ui.label(
                        RichText::new(&name)
                            .size(16.0)
                            .strong()
                            .color(style::color(pal.text_primary)),
                    );
                    ui.label(RichText::new(&summary).color(style::color(pal.text_muted)));
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        if ui
                            .add_enabled(connected || connecting, Button::new(t("断开", "Disconnect")))
                            .clicked()
                        {
                            actions.push(Action::Disconnect);
                        }
                        if ui
                            .add_enabled(!connected && !connecting, Button::new(t("连接", "Connect")))
                            .clicked()
                        {
                            actions.push(Action::Connect);
                        }
                        if ui
                            .add_enabled(!connecting, Button::new(t("编辑", "Edit")))
                            .clicked()
                        {
                            actions.push(Action::Page(0));
                        }
                        status_badge(ui, pal, status);
                    });
                });
            });
        ui.add_space(10.0);

        let avail_h = ui.available_height();
        let preset_count = self.runtime.form.presets.len();
        let publish_h = if preset_count > 0 { 190.0 } else { 145.0 };
        let logs_h = (avail_h - publish_h - 12.0).max(160.0);

        ui.horizontal_top(|ui| {
            // Subscribe card.
            ui.allocate_ui_with_layout(
                Vec2::new(220.0, avail_h),
                Layout::top_down(Align::Min),
                |ui| {
                    Frame::new()
                        .fill(style::color(pal.panel_bg))
                        .corner_radius(10)
                        .inner_margin(Margin::same(12))
                        .show(ui, |ui| {
                            ui.set_width(ui.available_width());
                            ui.label(
                                RichText::new(t("订阅", "Subscribe"))
                                    .strong()
                                    .color(style::color(pal.text_title)),
                            );
                            *dirty |= text_edit(ui, &mut self.runtime.form.sub_topic, ui.available_width(), connected, "topic/filter/#", false);
                            ui.horizontal(|ui| {
                                ui.label(RichText::new("QoS").size(12.0).color(style::color(pal.text_muted)));
                                let options = ["0".to_string(), "1".to_string(), "2".to_string()];
                                *dirty |= combo(ui, "sub-qos", 56.0, &mut self.runtime.form.sub_qos, &options, connected);
                                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                                    if ui.add_enabled(connected, Button::new(t("订阅", "Sub"))).clicked() {
                                        actions.push(Action::Subscribe);
                                    }
                                });
                            });
                            let active = self.runtime.active_sub_topic();
                            hint(
                                ui,
                                pal,
                                &if active.is_empty() {
                                    t("点击订阅可筛选消息", "Click a subscription to filter")
                                } else {
                                    format!("{}{}", t("正在筛选: ", "Filtering: "), active)
                                },
                            );
                            ui.add_space(4.0);
                            let subs: Vec<(String, i32, bool)> = self
                                .runtime
                                .subscriptions()
                                .iter()
                                .map(|sub| {
                                    (
                                        sub.topic.clone(),
                                        sub.qos as i32,
                                        !active.is_empty() && active == sub.topic,
                                    )
                                })
                                .collect();
                            Frame::new()
                                .fill(style::color(pal.inset_bg))
                                .corner_radius(8)
                                .inner_margin(Margin::same(4))
                                .show(ui, |ui| {
                                    ui.set_width(ui.available_width());
                                    ScrollArea::vertical()
                                        .id_salt("subs")
                                        .max_height(ui.available_height() - 8.0)
                                        .auto_shrink([false, false])
                                        .show(ui, |ui| {
                                            ui.set_width(ui.available_width());
                                            if subs.is_empty() {
                                                ui.add_space(8.0);
                                                ui.label(
                                                    RichText::new(t("还没有订阅", "No subscriptions yet"))
                                                        .size(12.0)
                                                        .color(style::color(pal.text_faint)),
                                                );
                                            }
                                            for (index, (topic, qos, selected)) in subs.iter().enumerate() {
                                                if subscription_row(ui, pal, index, topic, *qos, *selected, actions) {
                                                    break;
                                                }
                                            }
                                        });
                                });
                        });
                },
            );

            // Logs + publish column.
            ui.allocate_ui_with_layout(
                Vec2::new(ui.available_width(), avail_h),
                Layout::top_down(Align::Min),
                |ui| {
                    Frame::new()
                        .fill(style::color(pal.panel_bg))
                        .corner_radius(10)
                        .inner_margin(Margin::same(12))
                        .show(ui, |ui| {
                            ui.set_width(ui.available_width());
                            ui.horizontal(|ui| {
                                let mut filter = self.runtime.prefs.log_filter.clone();
                                if text_edit(ui, &mut filter, 148.0, true, t("搜索 Topic / Payload", "Search topic / payload"), false) {
                                    self.runtime.set_log_filter(filter);
                                }
                                let kind_options = vec![
                                    t("全部", "All"),
                                    t("收", "In"),
                                    t("发", "Out"),
                                    t("系统", "Sys"),
                                    t("错误", "Err"),
                                ];
                                let mut kind = self.runtime.prefs.log_kind_filter;
                                if combo(ui, "log-kind", 72.0, &mut kind, &kind_options, true) {
                                    self.runtime.set_log_kind_filter(kind);
                                }
                                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                                    if ui.add_sized([64.0, 26.0], Button::new(t("清空", "Clear"))).clicked() {
                                        actions.push(Action::ClearLogs);
                                    }
                                    if ui.add_sized([64.0, 26.0], Button::new(t("保存", "Save"))).clicked() {
                                        actions.push(Action::SaveLogs);
                                    }
                                    let mut auto = self.runtime.prefs.auto_scroll;
                                    if ui.add(Checkbox::new(&mut auto, t("自动滚动", "Auto-scroll"))).changed() {
                                        self.runtime.set_auto_scroll(auto);
                                    }
                                    let mut format = self.runtime.prefs.log_format;
                                    let options: Vec<String> = PayloadFormat::ALL.iter().map(|f| f.display()).collect();
                                    if combo(ui, "log-format", 76.0, &mut format, &options, true) {
                                        self.runtime.set_log_format(format);
                                    }
                                });
                            });
                            ui.add_space(4.0);
                            let logs_area_h = (logs_h - 48.0).max(100.0);
                            Frame::new()
                                .fill(style::color(pal.inset_bg))
                                .corner_radius(8)
                                .inner_margin(Margin::same(2))
                                .show(ui, |ui| {
                                    ui.set_min_height(logs_area_h);
                                    ui.set_width(ui.available_width());
                                    let format = PayloadFormat::from_index(self.runtime.prefs.log_format);
                                    let auto_scroll = self.runtime.prefs.auto_scroll;
                                    ScrollArea::vertical()
                                        .id_salt("logs")
                                        .stick_to_bottom(auto_scroll)
                                        .auto_shrink([false, false])
                                        .show(ui, |ui| {
                                            ui.set_width(ui.available_width());
                                            let logs: Vec<LogRecord> = self
                                                .runtime
                                                .visible_logs()
                                                .into_iter()
                                                .cloned()
                                                .collect();
                                            if logs.is_empty() {
                                                ui.add_space(20.0);
                                                ui.vertical_centered(|ui| {
                                                    ui.label(
                                                        RichText::new(t("暂无日志", "No logs yet"))
                                                            .color(style::color(pal.text_faint)),
                                                    );
                                                });
                                            }
                                            for log in &logs {
                                                log_bubble(ui, pal, log, format);
                                            }
                                        });
                                });
                        });
                    ui.add_space(8.0);

                    // Publish card.
                    Frame::new()
                        .fill(style::color(pal.panel_bg))
                        .corner_radius(10)
                        .inner_margin(Margin::same(12))
                        .show(ui, |ui| {
                            ui.set_width(ui.available_width());
                            let timed_running = self.runtime.timed_running();
                            ui.horizontal(|ui| {
                                ui.label(RichText::new(t("发布", "Publish")).strong().color(style::color(pal.text_title)));
                                let topic_w = (ui.available_width() - 450.0).max(120.0);
                                *dirty |= text_edit(ui, &mut self.runtime.form.pub_topic, topic_w, connected, "Topic", false);
                                let qos_options = ["0".to_string(), "1".to_string(), "2".to_string()];
                                *dirty |= combo(ui, "pub-qos", 52.0, &mut self.runtime.form.pub_qos, &qos_options, connected);
                                *dirty |= ui
                                    .add_enabled(connected, Checkbox::new(&mut self.runtime.form.pub_retain, "Retain"))
                                    .changed();
                                let format_options: Vec<String> = PayloadFormat::ALL.iter().map(|f| f.display()).collect();
                                *dirty |= combo(ui, "pub-format", 78.0, &mut self.runtime.form.pub_format, &format_options, connected);
                                if ui
                                    .add_enabled(connected && self.runtime.form.pub_format == 1, Button::new(t("美化", "Pretty")))
                                    .clicked()
                                {
                                    actions.push(Action::FormatPayload);
                                }
                                *dirty |= text_edit(ui, &mut self.runtime.form.timed_interval, 44.0, connected && !timed_running, t("秒", "sec"), false);
                                let timed_label = if timed_running { t("停止", "Stop") } else { t("定时", "Timed") };
                                if ui
                                    .add_enabled(connected || timed_running, Button::new(timed_label))
                                    .clicked()
                                {
                                    actions.push(Action::ToggleTimed);
                                }
                                let publish_button = Button::new(t("发布", "Pub"))
                                    .min_size(Vec2::new(64.0, 26.0));
                                if ui.add_enabled(connected, publish_button).clicked() {
                                    actions.push(Action::Publish);
                                }
                            });
                            let payload_h = 52.0;
                            *dirty |= text_area(ui, &mut self.runtime.form.pub_payload, payload_h, connected, "Payload");
                            let presets: Vec<String> = self.runtime.form.presets.iter().map(|p| p.name.clone()).collect();
                            if !presets.is_empty() {
                                ui.horizontal(|ui| {
                                    ui.label(RichText::new(t("发布预设", "Presets")).strong().color(style::color(pal.text_title)));
                                    ScrollArea::horizontal()
                                        .id_salt("preset-send")
                                        .max_width(ui.available_width())
                                        .show(ui, |ui| {
                                            ui.horizontal(|ui| {
                                                for (index, name) in presets.iter().enumerate() {
                                                    let label = if name.trim().is_empty() {
                                                        format!("{}{}", t("预设 ", "Preset "), index + 1)
                                                    } else {
                                                        name.clone()
                                                    };
                                                    if ui
                                                        .add_enabled(connected, Button::new(label))
                                                        .clicked()
                                                    {
                                                        actions.push(Action::PublishPreset(index));
                                                    }
                                                }
                                            });
                                        });
                                });
                            }
                        });
                },
            );
        });
    }

    fn page_settings(&mut self, ui: &mut Ui, pal: &Palette, actions: &mut Vec<Action>, dirty: &mut bool) {
        Frame::new()
            .fill(style::color(pal.panel_bg))
            .corner_radius(10)
            .inner_margin(Margin::same(16))
            .show(ui, |ui| {
                ui.set_width(ui.available_width());
                ui.horizontal(|ui| {
                    ui.label(
                        RichText::new(t("设置", "Settings"))
                            .size(16.0)
                            .strong()
                            .color(style::color(pal.text_primary)),
                    );
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        if ui.add_sized([88.0, 28.0], Button::new(t("返回会话", "Back"))).clicked() {
                            actions.push(Action::Page(1));
                        }
                    });
                });
                ui.add_space(12.0);
                section(ui, pal, t("外观", "Appearance"));
                ui.horizontal(|ui| {
                    ui.label(RichText::new(t("皮肤", "Skin")).color(style::color(pal.text_title)));
                    let options: Vec<String> = Skin::ALL.iter().map(|skin| skin.display()).collect();
                    let before = self.runtime.prefs.skin_choice;
                    if combo(ui, "skin", 148.0, &mut self.runtime.prefs.skin_choice, &options, true)
                        && self.runtime.prefs.skin_choice != before
                    {
                        actions.push(Action::Skin(self.runtime.prefs.skin_choice));
                    }
                });
                ui.horizontal(|ui| {
                    ui.label(RichText::new(t("界面语言", "UI language")).color(style::color(pal.text_title)));
                    let options = vec!["中文".to_string(), "English".to_string()];
                    let before = self.runtime.prefs.language;
                    if combo(ui, "language", 140.0, &mut self.runtime.prefs.language, &options, true)
                        && self.runtime.prefs.language != before
                    {
                        actions.push(Action::Language(self.runtime.prefs.language));
                    }
                });
                hint(ui, pal, t("语言切换立即生效，并写入 mqttc.json。", "Language applies immediately and is saved to mqttc.json."));
                *dirty = *dirty || false;
            });
    }

    fn page_auto_reply(&mut self, ui: &mut Ui, pal: &Palette, actions: &mut Vec<Action>, dirty: &mut bool) {
        Frame::new()
            .fill(style::color(pal.panel_bg))
            .corner_radius(10)
            .inner_margin(Margin::same(16))
            .show(ui, |ui| {
                ui.set_width(ui.available_width());
                ui.horizontal(|ui| {
                    ui.label(
                        RichText::new(t("自动回复", "Auto-reply"))
                            .size(16.0)
                            .strong()
                            .color(style::color(pal.text_primary)),
                    );
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        if ui.add_sized([88.0, 28.0], Button::new(t("返回会话", "Back"))).clicked() {
                            actions.push(Action::Page(1));
                        }
                    });
                });
                ui.horizontal(|ui| {
                    ui.label(RichText::new(t("自身回投", "Own echo")).color(style::color(pal.text_title)));
                    let options = vec![
                        t("跳过自身回投", "Skip own echo"),
                        t("再匹配自身回投", "Rematch own echo"),
                    ];
                    *dirty |= combo(ui, "echo", 220.0, &mut self.runtime.form.auto_reply_echo, &options, true);
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        if ui.add(Button::new(t("新增规则", "Add rule"))).clicked() {
                            actions.push(Action::AddRule);
                        }
                    });
                });
                hint(ui, pal, t("勾选后该条才参与匹配；同一条消息只触发第一条命中的规则。", "Only checked rules match; the first hit sends its own fixed reply."));
                ui.add_space(4.0);
                Frame::new()
                    .fill(style::color(pal.inset_bg))
                    .corner_radius(8)
                    .inner_margin(Margin::same(4))
                    .show(ui, |ui| {
                        ui.set_width(ui.available_width());
                        ScrollArea::vertical()
                            .id_salt("rules")
                            .auto_shrink([false, false])
                            .show(ui, |ui| {
                                ui.set_width(ui.available_width());
                                if self.runtime.form.rules.is_empty() {
                                    ui.add_space(20.0);
                                    ui.vertical_centered(|ui| {
                                        ui.label(
                                            RichText::new(t("还没有规则。点「新增规则」添加。", "No rules yet. Click Add rule."))
                                                .color(style::color(pal.text_muted)),
                                        );
                                    });
                                }
                                let format_options: Vec<String> = PayloadFormat::ALL.iter().map(|f| f.display()).collect();
                                let qos_options = ["0".to_string(), "1".to_string(), "2".to_string()];
                                for index in 0..self.runtime.form.rules.len() {
                                    Frame::new()
                                        .fill(style::color(pal.panel_bg))
                                        .corner_radius(8)
                                        .inner_margin(Margin::same(10))
                                        .show(ui, |ui| {
                                            ui.set_width(ui.available_width());
                                            let row = &mut self.runtime.form.rules[index];
                                            ui.horizontal(|ui| {
                                                *dirty |= ui.add(Checkbox::new(&mut row.enabled, t("启用", "On"))).changed();
                                                ui.label(RichText::new(t("规则", "Rule")).strong().color(style::color(pal.text_title)));
                                                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                                                    if ui.add_sized([64.0, 26.0], Button::new(t("删除", "Delete"))).clicked() {
                                                        actions.push(Action::DeleteRule(index));
                                                    }
                                                });
                                            });
                                            ui.horizontal(|ui| {
                                                *dirty |= field(ui, pal, t("匹配 Topic", "Match topic"), &mut row.match_topic, 320.0, true, "sensor/+/status", false);
                                                *dirty |= field(ui, pal, t("包含", "Contains"), &mut row.payload_contains, 160.0, true, t("可选", "optional"), false);
                                            });
                                            ui.horizontal(|ui| {
                                                *dirty |= field(ui, pal, t("回复 Topic", "Reply topic"), &mut row.reply_topic, 300.0, true, "Topic", false);
                                                ui.vertical(|ui| {
                                                    field_label(ui, pal, "QoS");
                                                    *dirty |= combo(ui, &format!("rule-qos-{index}"), 52.0, &mut row.qos, &qos_options, true);
                                                });
                                                ui.vertical(|ui| {
                                                    ui.add_space(18.0);
                                                    *dirty |= ui.add(Checkbox::new(&mut row.retain, "Retain")).changed();
                                                });
                                                ui.vertical(|ui| {
                                                    field_label(ui, pal, t("格式", "Format"));
                                                    *dirty |= combo(ui, &format!("rule-format-{index}"), 78.0, &mut row.format, &format_options, true);
                                                });
                                            });
                                            *dirty |= text_area(ui, &mut row.reply_payload, 40.0, true, "Payload");
                                        });
                                    ui.add_space(6.0);
                                }
                            });
                    });
            });
    }

    fn page_presets(&mut self, ui: &mut Ui, pal: &Palette, actions: &mut Vec<Action>, dirty: &mut bool) {
        Frame::new()
            .fill(style::color(pal.panel_bg))
            .corner_radius(10)
            .inner_margin(Margin::same(16))
            .show(ui, |ui| {
                ui.set_width(ui.available_width());
                ui.horizontal(|ui| {
                    ui.label(
                        RichText::new(t("发布预设", "Publish presets"))
                            .size(16.0)
                            .strong()
                            .color(style::color(pal.text_primary)),
                    );
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        if ui.add_sized([88.0, 28.0], Button::new(t("返回会话", "Back"))).clicked() {
                            actions.push(Action::Page(1));
                        }
                    });
                });
                ui.horizontal(|ui| {
                    hint(ui, pal, t("在会话页发布区下方点发送即可发出对应报文。", "Send from the session page under the publish form."));
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        if ui.add(Button::new(t("新增预设", "Add preset"))).clicked() {
                            actions.push(Action::AddPreset);
                        }
                    });
                });
                ui.add_space(4.0);
                Frame::new()
                    .fill(style::color(pal.inset_bg))
                    .corner_radius(8)
                    .inner_margin(Margin::same(4))
                    .show(ui, |ui| {
                        ui.set_width(ui.available_width());
                        ScrollArea::vertical()
                            .id_salt("presets")
                            .auto_shrink([false, false])
                            .show(ui, |ui| {
                                ui.set_width(ui.available_width());
                                if self.runtime.form.presets.is_empty() {
                                    ui.add_space(20.0);
                                    ui.vertical_centered(|ui| {
                                        ui.label(
                                            RichText::new(t("还没有预设。点「新增预设」添加。", "No presets yet. Click Add preset."))
                                                .color(style::color(pal.text_muted)),
                                        );
                                    });
                                }
                                let format_options: Vec<String> = PayloadFormat::ALL.iter().map(|f| f.display()).collect();
                                let qos_options = ["0".to_string(), "1".to_string(), "2".to_string()];
                                for index in 0..self.runtime.form.presets.len() {
                                    Frame::new()
                                        .fill(style::color(pal.panel_bg))
                                        .corner_radius(8)
                                        .inner_margin(Margin::same(10))
                                        .show(ui, |ui| {
                                            ui.set_width(ui.available_width());
                                            let row = &mut self.runtime.form.presets[index];
                                            ui.horizontal(|ui| {
                                                *dirty |= field(ui, pal, t("名称", "Name"), &mut row.name, 160.0, true, t("名称", "Name"), false);
                                                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                                                    if ui.add_sized([64.0, 26.0], Button::new(t("删除", "Delete"))).clicked() {
                                                        actions.push(Action::DeletePreset(index));
                                                    }
                                                });
                                            });
                                            ui.horizontal(|ui| {
                                                let topic_w = (ui.available_width() - 250.0).max(100.0);
                                                *dirty |= field(ui, pal, "Topic", &mut row.topic, topic_w, true, "Topic", false);
                                                ui.vertical(|ui| {
                                                    field_label(ui, pal, "QoS");
                                                    *dirty |= combo(ui, &format!("preset-qos-{index}"), 52.0, &mut row.qos, &qos_options, true);
                                                });
                                                ui.vertical(|ui| {
                                                    ui.add_space(18.0);
                                                    *dirty |= ui.add(Checkbox::new(&mut row.retain, "Retain")).changed();
                                                });
                                                ui.vertical(|ui| {
                                                    field_label(ui, pal, t("格式", "Format"));
                                                    *dirty |= combo(ui, &format!("preset-format-{index}"), 78.0, &mut row.format, &format_options, true);
                                                });
                                            });
                                            *dirty |= text_area(ui, &mut row.payload, 48.0, true, "Payload");
                                        });
                                    ui.add_space(6.0);
                                }
                            });
                    });
            });
    }

    fn page_about(&mut self, ui: &mut Ui, pal: &Palette, actions: &mut Vec<Action>) {
        Frame::new()
            .fill(style::color(pal.panel_bg))
            .corner_radius(10)
            .inner_margin(Margin::same(16))
            .show(ui, |ui| {
                ui.set_width(ui.available_width());
                ui.horizontal(|ui| {
                    ui.label(
                        RichText::new(t("关于", "About"))
                            .size(16.0)
                            .strong()
                            .color(style::color(pal.text_primary)),
                    );
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        if ui.add_sized([88.0, 28.0], Button::new(t("返回会话", "Back"))).clicked() {
                            actions.push(Action::Page(1));
                        }
                    });
                });
                ui.add_space(12.0);
                ui.label(RichText::new("mqttc").size(22.0).strong().color(style::color(pal.text_primary)));
                ui.label(
                    RichText::new(format!("{} {}", t("版本 ", "Version "), env!("CARGO_PKG_VERSION")))
                        .size(13.0)
                        .color(style::color(pal.text_muted)),
                );
                ui.add_space(6.0);
                let body = |ui: &mut Ui, text: String| {
                    ui.label(RichText::new(text).color(style::color(pal.text_body)));
                };
                body(
                    ui,
                    t(
                        "跨平台桌面 MQTT 调试客户端，界面参考 MQTTX。同一套代码覆盖 Windows、macOS 与 Linux。",
                        "A cross-platform desktop MQTT debug client inspired by MQTTX. One codebase covers Windows, macOS, and Linux.",
                    ),
                );
                section(ui, pal, t("能做什么", "What it does"));
                body(
                    ui,
                    t(
                        "同时维护多条 Broker 连接，订阅 Topic、发布消息、查看聊天气泡日志。每条连接可配置发布预设与自动回复。协议支持 MQTT 3.1.1 与 MQTT 5.0，可选 TLS。",
                        "Keep several Broker connections online at once. Subscribe, publish, and watch chat-style logs. Each connection has its own publish presets and auto-reply rules. MQTT 3.1.1 and 5.0, with optional TLS.",
                    ),
                );
                section(ui, pal, t("说明", "Notes"));
                body(
                    ui,
                    t(
                        "连接、皮肤与语言写入可执行文件同目录的 mqttc.json。日志上限每连接 200 条。本版本使用 egui/eframe 与 OpenGL（glow），不打包中文字体，按需读取系统字体。",
                        "Connections, skin, and language are saved to mqttc.json next to the executable. Each connection keeps up to 200 log rows. This build uses egui/eframe with OpenGL (glow), bundles no CJK font, and loads system fonts on demand.",
                    ),
                );
                section(ui, pal, t("开源协议", "License"));
                body(
                    ui,
                    t(
                        "MIT License。源码仓库根目录有 LICENSE 全文。Copyright (c) 2026 Gordon Wen。",
                        "MIT License. The full text is in LICENSE at the repository root. Copyright (c) 2026 Gordon Wen.",
                    ),
                );
                ui.add_space(8.0);
                ui.label(
                    RichText::new("egui / eframe · glow")
                        .size(12.0)
                        .color(style::color(pal.text_faint)),
                );
            });
    }
}

impl eframe::App for App {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();
        if self.first_frame_ms.is_none() {
            let ms = self.started.elapsed().as_secs_f64() * 1000.0;
            self.first_frame_ms = Some(ms);
            println!("FIRST_FRAME_MS {ms:.1}");
        }

        let summary = self.runtime.poll();
        if let Some(page) = summary.page_request {
            self.page = page;
        }
        if self.runtime.refresh_system_skin() {
            ctx.request_repaint();
        }
        if let Some(bench) = &mut self.bench {
            bench.tick(&ctx, &mut self.runtime);
        }

        let skin = self.runtime.skin();
        let pal = self.runtime.palette();
        style::apply(&ctx, &pal, skin.is_light());

        let mut actions = Vec::new();
        let mut dirty = false;

        egui::Panel::left("sidebar")
            .exact_size(200.0)
            .resizable(false)
            .frame(
                Frame::new()
                    .fill(style::color(pal.window_bg))
                    .inner_margin(Margin::same(10)),
            )
            .show(ui, |ui| {
                self.sidebar(ui, &pal, &mut actions);
            });

        egui::CentralPanel::default()
            .frame(
                Frame::new()
                    .fill(style::color(pal.window_bg))
                    .inner_margin(Margin::symmetric(12, 10)),
            )
            .show(ui, |ui| match self.page {
                0 => self.page_editor(ui, &pal, &mut actions, &mut dirty),
                1 => self.page_session(ui, &pal, &mut actions, &mut dirty),
                2 => self.page_settings(ui, &pal, &mut actions, &mut dirty),
                3 => self.page_auto_reply(ui, &pal, &mut actions, &mut dirty),
                4 => self.page_presets(ui, &pal, &mut actions, &mut dirty),
                _ => self.page_about(ui, &pal, &mut actions),
            });

        if dirty {
            self.runtime.mark_dirty();
        }
        if !actions.is_empty() {
            ctx.request_repaint();
            self.apply_actions(actions);
        }

        // The System skin needs a slow heartbeat; everything else is event driven
        // (the MQTT workers call `request_repaint` through the runtime wake hook).
        if skin == Skin::System {
            ctx.request_repaint_after(std::time::Duration::from_secs(1));
        }
    }
}

// ---------------------------------------------------------------- widgets

fn field_label(ui: &mut Ui, pal: &Palette, text: impl AsRef<str>) {
    ui.label(
        RichText::new(text.as_ref())
            .size(12.0)
            .color(style::color(pal.text_muted)),
    );
}

fn section(ui: &mut Ui, pal: &Palette, text: impl AsRef<str>) {
    ui.add_space(4.0);
    ui.label(
        RichText::new(text.as_ref())
            .strong()
            .color(style::color(pal.text_title)),
    );
}

fn hint(ui: &mut Ui, pal: &Palette, text: impl AsRef<str>) {
    ui.label(
        RichText::new(text.as_ref())
            .size(12.0)
            .color(style::color(pal.text_faint)),
    );
}

fn text_edit(
    ui: &mut Ui,
    value: &mut String,
    width: f32,
    enabled: bool,
    hint_text: impl Into<egui::WidgetText>,
    password: bool,
) -> bool {
    let mut edit = TextEdit::singleline(value)
        .desired_width(width)
        .hint_text(hint_text);
    if password {
        edit = edit.password(true);
    }
    ui.add_enabled(enabled, edit).changed()
}

fn text_area(ui: &mut Ui, value: &mut String, height: f32, enabled: bool, hint_text: &str) -> bool {
    let edit = TextEdit::multiline(value)
        .desired_rows(2)
        .desired_width(ui.available_width())
        .hint_text(hint_text)
        .font(style::mono_font(13.0));
    let response = ui.add_enabled(enabled, edit);
    let _ = height;
    response.changed()
}

fn field(
    ui: &mut Ui,
    pal: &Palette,
    label: impl AsRef<str>,
    value: &mut String,
    width: f32,
    enabled: bool,
    hint_text: impl Into<egui::WidgetText>,
    password: bool,
) -> bool {
    ui.vertical(|ui| {
        field_label(ui, pal, label.as_ref());
        text_edit(ui, value, width, enabled, hint_text, password)
    })
    .inner
}

fn combo(
    ui: &mut Ui,
    id: &str,
    width: f32,
    current: &mut i32,
    options: &[String],
    enabled: bool,
) -> bool {
    let mut changed = false;
    ui.add_enabled_ui(enabled, |ui| {
        ComboBox::from_id_salt(id)
            .width(width)
            .selected_text(options.get(*current as usize).cloned().unwrap_or_default())
            .show_ui(ui, |ui| {
                for (index, label) in options.iter().enumerate() {
                    if ui
                        .selectable_value(current, index as i32, label.as_str())
                        .changed()
                    {
                        changed = true;
                    }
                }
            });
    });
    changed
}

fn status_badge(ui: &mut Ui, pal: &Palette, status: ConnStatus) {
    Frame::new()
        .fill(style::color(pal.badge_bg))
        .corner_radius(11)
        .inner_margin(Margin::symmetric(10, 4))
        .show(ui, |ui| {
            ui.horizontal(|ui| {
                let (rect, _) = ui.allocate_exact_size(Vec2::splat(8.0), Sense::hover());
                ui.painter()
                    .circle_filled(rect.center(), 4.0, style::color(status.rgb()));
                ui.label(
                    RichText::new(status.text())
                        .strong()
                        .color(style::color(status.rgb())),
                );
            });
        });
}

fn connection_row(
    ui: &mut Ui,
    pal: &Palette,
    id: &str,
    name: &str,
    status: ConnStatus,
    selected: bool,
) -> Option<Action> {
    let width = ui.available_width();
    let (rect, _) = ui.allocate_exact_size(Vec2::new(width, 50.0), Sense::hover());
    let painter = ui.painter().clone();
    if selected {
        painter.rect_filled(rect, CornerRadius::same(8), style::color(pal.selected_bg));
    }
    painter.circle_filled(
        egui::pos2(rect.left() + 14.0, rect.center().y),
        4.0,
        style::color(status.rgb()),
    );
    let clip = Rect::from_min_max(rect.min, egui::pos2(rect.right() - 30.0, rect.max.y));
    let painter = painter.with_clip_rect(clip);
    painter.text(
        egui::pos2(rect.left() + 24.0, rect.center().y - 9.0),
        Align2::LEFT_TOP,
        name,
        style::ui_font(14.0),
        style::color(pal.text_body),
    );
    painter.text(
        egui::pos2(rect.left() + 24.0, rect.center().y + 2.0),
        Align2::LEFT_TOP,
        status.text(),
        style::ui_font(12.0),
        style::color(status.rgb()),
    );

    let del_rect = Rect::from_center_size(
        egui::pos2(rect.right() - 16.0, rect.center().y),
        Vec2::splat(24.0),
    );
    let del = ui.interact(del_rect, ui.scope_id().with(("del", id)), Sense::click());
    let painter = ui.painter().clone();
    if del.hovered() {
        painter.rect_filled(del_rect, CornerRadius::same(12), style::color(pal.hover_bg));
    }
    painter.text(
        del_rect.center(),
        Align2::CENTER_CENTER,
        "×",
        style::ui_font(15.0),
        style::color(pal.danger),
    );
    if del.clicked() {
        return Some(Action::Delete(id.to_string()));
    }

    let select_rect = Rect::from_min_max(rect.min, egui::pos2(rect.right() - 30.0, rect.max.y));
    let select = ui.interact(select_rect, ui.scope_id().with(("sel", id)), Sense::click());
    if select.hovered() {
        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
    }
    if select.clicked() {
        return Some(Action::Select(id.to_string()));
    }
    None
}

fn subscription_row(
    ui: &mut Ui,
    pal: &Palette,
    index: usize,
    topic: &str,
    qos: i32,
    selected: bool,
    actions: &mut Vec<Action>,
) -> bool {
    let width = ui.available_width();
    let (rect, _) = ui.allocate_exact_size(Vec2::new(width, 44.0), Sense::hover());
    let painter = ui.painter().clone();
    if selected {
        painter.rect_filled(rect, CornerRadius::same(6), style::color(pal.sub_selected));
    }
    let clip = Rect::from_min_max(rect.min, egui::pos2(rect.right() - 28.0, rect.max.y));
    let painter = painter.with_clip_rect(clip);
    painter.text(
        egui::pos2(rect.left() + 10.0, rect.center().y - 9.0),
        Align2::LEFT_TOP,
        topic,
        style::mono_font(13.0),
        style::color(if selected { pal.send_topic } else { pal.text_body }),
    );
    painter.text(
        egui::pos2(rect.left() + 10.0, rect.center().y + 3.0),
        Align2::LEFT_TOP,
        format!("QoS {qos}"),
        style::ui_font(12.0),
        style::color(pal.text_muted),
    );

    let cancel_rect = Rect::from_center_size(
        egui::pos2(rect.right() - 16.0, rect.center().y),
        Vec2::splat(22.0),
    );
    let cancel = ui.interact(cancel_rect, ui.scope_id().with(("unsub", index)), Sense::click());
    let painter = ui.painter().clone();
    if cancel.hovered() {
        painter.rect_filled(cancel_rect, CornerRadius::same(11), style::color(pal.hover_bg));
    }
    painter.text(
        cancel_rect.center(),
        Align2::CENTER_CENTER,
        "×",
        style::ui_font(15.0),
        style::color(pal.danger),
    );
    if cancel.clicked() {
        actions.push(Action::Unsubscribe(index));
        return true;
    }

    let select_rect = Rect::from_min_max(rect.min, egui::pos2(rect.right() - 28.0, rect.max.y));
    let select = ui.interact(select_rect, ui.scope_id().with(("sub", index)), Sense::click());
    if select.hovered() {
        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
    }
    if select.clicked() {
        actions.push(Action::ToggleSub(topic.to_string()));
    }
    false
}

fn log_bubble(ui: &mut Ui, pal: &Palette, log: &LogRecord, format: PayloadFormat) {
    let text = if log.is_payload_kind() {
        payload::render_payload(&log.payload, format)
    } else {
        String::from_utf8_lossy(&log.payload).into_owned()
    };
    let max_w = (ui.available_width() - 24.0).clamp(160.0, 520.0);

    match log.kind.as_str() {
        "RECV" | "SEND" => {
            let send = log.kind == "SEND";
            let (bg, topic_color, payload_color, layout) = if send {
                (pal.send_bg, pal.send_topic, pal.send_payload, Layout::right_to_left(Align::TOP))
            } else {
                (pal.recv_bg, pal.recv_topic, pal.recv_payload, Layout::left_to_right(Align::TOP))
            };
            ui.with_layout(layout, |ui| {
                Frame::new()
                    .fill(style::color(bg))
                    .corner_radius(10)
                    .inner_margin(Margin::same(8))
                    .show(ui, |ui| {
                        ui.set_max_width(max_w);
                        ui.horizontal(|ui| {
                            ui.add(
                                Label::new(
                                    RichText::new(&log.topic)
                                        .font(style::mono_font(13.0))
                                        .color(style::color(topic_color)),
                                )
                                .wrap(),
                            );
                            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                                ui.label(
                                    RichText::new(&log.time)
                                        .font(style::mono_font(12.0))
                                        .color(style::color(pal.text_faint)),
                                );
                            });
                        });
                        ui.add(
                            Label::new(
                                RichText::new(text)
                                    .font(style::mono_font(13.0))
                                    .color(style::color(payload_color)),
                            )
                            .wrap(),
                        );
                    });
            });
        }
        _ => {
            let err = log.kind == "ERR";
            let (bg, label_color, text_color) = if err {
                (pal.err_bg, pal.err_label, pal.err_text)
            } else {
                (pal.sys_bg, pal.muted, pal.sys_text)
            };
            ui.with_layout(Layout::top_down(Align::Center), |ui| {
                Frame::new()
                    .fill(style::color(bg))
                    .corner_radius(8)
                    .inner_margin(Margin::symmetric(10, 6))
                    .show(ui, |ui| {
                        ui.set_max_width((ui.available_width() - 8.0).min(720.0));
                        ui.horizontal(|ui| {
                            ui.label(
                                RichText::new(&log.time)
                                    .font(style::mono_font(12.0))
                                    .color(style::color(pal.text_faint)),
                            );
                            ui.label(
                                RichText::new(if err { t("错误", "Error") } else { t("系统", "System") })
                                    .font(style::ui_font(12.0))
                                    .strong()
                                    .color(style::color(label_color)),
                            );
                            let body = if log.topic.is_empty() {
                                text
                            } else {
                                format!("{}  {}", log.topic, text)
                            };
                            ui.add(
                                Label::new(
                                    RichText::new(body)
                                        .font(style::mono_font(13.0))
                                        .color(style::color(text_color)),
                                )
                                .wrap(),
                            );
                        });
                    });
            });
        }
    }
}
