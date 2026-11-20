//! Shared palette → iced widget styles.
//!
//! The core owns the 6-skin palette; this module only translates its RGB
//! values into iced `Color`s and style closures so every widget matches the
//! egui shell.

use iced::widget::{button, checkbox, container, pick_list, text_editor, text_input};
use iced::{Background, Border, Color, Shadow, Theme};
use mqttc_core::theme::{Palette, Rgb};

pub fn rgb((r, g, b): Rgb) -> Color {
    Color::from_rgb8(r, g, b)
}

#[derive(Clone, Copy, Debug)]
pub struct Colors {
    pub window_bg: Color,
    pub panel_bg: Color,
    pub inset_bg: Color,
    pub hover_bg: Color,
    pub selected_bg: Color,
    pub badge_bg: Color,
    pub text_primary: Color,
    pub text_title: Color,
    pub text_muted: Color,
    pub text_faint: Color,
    pub text_body: Color,
    pub accent: Color,
    pub accent_fg: Color,
    pub recv_bg: Color,
    pub recv_topic: Color,
    pub recv_payload: Color,
    pub send_bg: Color,
    pub send_topic: Color,
    pub send_payload: Color,
    pub sys_bg: Color,
    pub sys_text: Color,
    pub err_bg: Color,
    pub err_label: Color,
    pub err_text: Color,
    pub danger: Color,
}

impl Colors {
    pub fn from_palette(p: &Palette) -> Self {
        Self {
            window_bg: rgb(p.window_bg),
            panel_bg: rgb(p.panel_bg),
            inset_bg: rgb(p.inset_bg),
            hover_bg: rgb(p.hover_bg),
            selected_bg: rgb(p.selected_bg),
            badge_bg: rgb(p.badge_bg),
            text_primary: rgb(p.text_primary),
            text_title: rgb(p.text_title),
            text_muted: rgb(p.text_muted),
            text_faint: rgb(p.text_faint),
            text_body: rgb(p.text_body),
            accent: rgb(p.accent),
            accent_fg: rgb(p.accent_fg),
            recv_bg: rgb(p.recv_bg),
            recv_topic: rgb(p.recv_topic),
            recv_payload: rgb(p.recv_payload),
            send_bg: rgb(p.send_bg),
            send_topic: rgb(p.send_topic),
            send_payload: rgb(p.send_payload),
            sys_bg: rgb(p.sys_bg),
            sys_text: rgb(p.sys_text),
            err_bg: rgb(p.err_bg),
            err_label: rgb(p.err_label),
            err_text: rgb(p.err_text),
            danger: rgb(p.danger),
        }
    }

    fn line(&self, focused: bool) -> Border {
        Border {
            color: if focused { self.accent } else { self.hover_bg },
            width: 1.0,
            radius: 6.0.into(),
        }
    }

    pub fn input_style(&self) -> impl Fn(&Theme, text_input::Status) -> text_input::Style + 'static {
        let c = *self;
        move |_, status| {
            let focused = matches!(status, text_input::Status::Focused { .. });
            let disabled = matches!(status, text_input::Status::Disabled);
            text_input::Style {
                background: Background::Color(if disabled { c.panel_bg } else { c.inset_bg }),
                border: c.line(focused),
                icon: c.text_muted,
                placeholder: c.text_faint,
                value: if disabled { c.text_faint } else { c.text_body },
                selection: c.selected_bg,
            }
        }
    }

    pub fn editor_style(
        &self,
    ) -> impl Fn(&Theme, text_editor::Status) -> text_editor::Style + 'static {
        let c = *self;
        move |_, status| {
            let focused = matches!(status, text_editor::Status::Focused { .. });
            text_editor::Style {
                background: Background::Color(c.inset_bg),
                border: c.line(focused),
                placeholder: c.text_faint,
                value: c.text_body,
                selection: c.selected_bg,
            }
        }
    }

    pub fn primary_button(&self) -> impl Fn(&Theme, button::Status) -> button::Style + 'static {
        let c = *self;
        move |_, status| primary_style(c, status)
    }

    pub fn ghost_button(&self) -> impl Fn(&Theme, button::Status) -> button::Style + 'static {
        let c = *self;
        move |_, status| ghost_style(c, status)
    }

    /// One entry point for conditional styles: `if/else` branches over two
    /// different `impl Fn` returns would otherwise need distinct opaque types.
    pub fn button_kind(
        &self,
        kind: ButtonKind,
    ) -> impl Fn(&Theme, button::Status) -> button::Style + 'static {
        let c = *self;
        move |_, status| match kind {
            ButtonKind::Ghost => ghost_style(c, status),
            ButtonKind::Danger => danger_style(c, status),
            ButtonKind::Selected => selected_style(c, status),
        }
    }

    pub fn danger_button(&self) -> impl Fn(&Theme, button::Status) -> button::Style + 'static {
        let c = *self;
        move |_, status| danger_style(c, status)
    }

    pub fn checkbox_style(&self) -> impl Fn(&Theme, checkbox::Status) -> checkbox::Style + 'static {
        let c = *self;
        move |_, _status| checkbox::Style {
            background: Background::Color(c.inset_bg),
            icon_color: c.accent,
            border: Border {
                color: c.hover_bg,
                width: 1.0,
                radius: 4.0.into(),
            },
            text_color: Some(c.text_body),
        }
    }

    pub fn pick_list_style(&self) -> impl Fn(&Theme, pick_list::Status) -> pick_list::Style + 'static {
        let c = *self;
        move |_, status| {
            let hovered = matches!(
                status,
                pick_list::Status::Hovered | pick_list::Status::Opened { .. }
            );
            pick_list::Style {
                text_color: c.text_body,
                placeholder_color: c.text_faint,
                handle_color: c.text_muted,
                background: Background::Color(if hovered { c.hover_bg } else { c.inset_bg }),
                border: c.line(hovered),
            }
        }
    }

    /// Filled card used for the sidebar, log panel and editor sections.
    pub fn card(&self) -> impl Fn(&Theme) -> container::Style + 'static {
        let c = *self;
        move |_| container::Style {
            text_color: Some(c.text_body),
            background: Some(Background::Color(c.panel_bg)),
            border: Border {
                color: c.hover_bg,
                width: 1.0,
                radius: 8.0.into(),
            },
            shadow: Shadow::default(),
            snap: true,
        }
    }

    /// Inset area inside a card (input wells, log background).
    pub fn inset(&self) -> impl Fn(&Theme) -> container::Style + 'static {
        let c = *self;
        move |_| container::Style {
            text_color: Some(c.text_body),
            background: Some(Background::Color(c.inset_bg)),
            border: Border {
                color: c.hover_bg,
                width: 1.0,
                radius: 6.0.into(),
            },
            shadow: Shadow::default(),
            snap: true,
        }
    }

    pub fn bubble(&self, background: Color) -> impl Fn(&Theme) -> container::Style + 'static {
        let c = *self;
        move |_| container::Style {
            text_color: Some(c.text_body),
            background: Some(Background::Color(background)),
            border: Border {
                color: background,
                width: 0.0,
                radius: 8.0.into(),
            },
            shadow: Shadow::default(),
            snap: true,
        }
    }

    /// Iced theme so default widgets (menus, text) pick up the palette.
    pub fn theme(&self) -> Theme {
        Theme::custom(
            "mqttc",
            iced::theme::Palette {
                background: self.window_bg,
                text: self.text_body,
                primary: self.accent,
                success: rgb((78, 207, 122)),
                warning: rgb((224, 180, 79)),
                danger: self.danger,
            },
        )
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ButtonKind {
    Ghost,
    Danger,
    Selected,
}

fn primary_style(c: Colors, status: button::Status) -> button::Style {
    let (bg, text_color) = match status {
        button::Status::Active | button::Status::Pressed => (c.accent, c.accent_fg),
        button::Status::Hovered => (c.selected_bg, c.text_primary),
        button::Status::Disabled => (c.badge_bg, c.text_faint),
    };
    button::Style {
        background: Some(Background::Color(bg)),
        text_color,
        border: Border {
            color: bg,
            width: 1.0,
            radius: 6.0.into(),
        },
        shadow: Shadow::default(),
        snap: true,
    }
}

fn ghost_style(c: Colors, status: button::Status) -> button::Style {
    let (bg, text_color, line) = match status {
        button::Status::Active => (None, c.text_body, c.hover_bg),
        button::Status::Hovered | button::Status::Pressed => {
            (Some(c.hover_bg), c.text_primary, c.hover_bg)
        }
        button::Status::Disabled => (None, c.text_faint, c.panel_bg),
    };
    button::Style {
        background: bg.map(Background::Color),
        text_color,
        border: Border {
            color: line,
            width: 1.0,
            radius: 6.0.into(),
        },
        shadow: Shadow::default(),
        snap: true,
    }
}

fn selected_style(c: Colors, status: button::Status) -> button::Style {
    let (bg, text_color) = match status {
        button::Status::Active | button::Status::Pressed => (c.selected_bg, c.text_primary),
        button::Status::Hovered => (c.hover_bg, c.text_primary),
        button::Status::Disabled => (c.badge_bg, c.text_faint),
    };
    button::Style {
        background: Some(Background::Color(bg)),
        text_color,
        border: Border {
            color: bg,
            width: 1.0,
            radius: 6.0.into(),
        },
        shadow: Shadow::default(),
        snap: true,
    }
}

fn danger_style(c: Colors, status: button::Status) -> button::Style {
    let (bg, text_color) = match status {
        button::Status::Active => (None, c.danger),
        button::Status::Hovered | button::Status::Pressed => (Some(c.danger), c.accent_fg),
        button::Status::Disabled => (None, c.text_faint),
    };
    button::Style {
        background: bg.map(Background::Color),
        text_color,
        border: Border {
            color: if bg.is_some() { c.danger } else { c.panel_bg },
            width: 1.0,
            radius: 6.0.into(),
        },
        shadow: Shadow::default(),
        snap: true,
    }
}
