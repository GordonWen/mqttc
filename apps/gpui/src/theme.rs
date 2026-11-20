//! Palette → GPUI colour translation.

use gpui::Rgba;
use mqttc_core::theme::{Palette, Rgb};

pub fn c((r, g, b): Rgb) -> Rgba {
    gpui::rgb(((r as u32) << 16) | ((g as u32) << 8) | b as u32)
}

#[derive(Clone, Copy, Debug)]
#[allow(dead_code)] // a few palette entries are reserved for parity with the other shells
pub struct Colors {
    pub window_bg: Rgba,
    pub panel_bg: Rgba,
    pub inset_bg: Rgba,
    pub hover_bg: Rgba,
    pub selected_bg: Rgba,
    pub badge_bg: Rgba,
    pub text_primary: Rgba,
    pub text_title: Rgba,
    pub text_muted: Rgba,
    pub text_faint: Rgba,
    pub text_body: Rgba,
    pub accent: Rgba,
    pub accent_fg: Rgba,
    pub recv_bg: Rgba,
    pub recv_topic: Rgba,
    pub recv_payload: Rgba,
    pub send_bg: Rgba,
    pub send_topic: Rgba,
    pub send_payload: Rgba,
    pub sys_bg: Rgba,
    pub sys_text: Rgba,
    pub err_bg: Rgba,
    pub err_label: Rgba,
    pub err_text: Rgba,
    pub danger: Rgba,
}

impl Colors {
    pub fn from_palette(p: &Palette) -> Self {
        Self {
            window_bg: c(p.window_bg),
            panel_bg: c(p.panel_bg),
            inset_bg: c(p.inset_bg),
            hover_bg: c(p.hover_bg),
            selected_bg: c(p.selected_bg),
            badge_bg: c(p.badge_bg),
            text_primary: c(p.text_primary),
            text_title: c(p.text_title),
            text_muted: c(p.text_muted),
            text_faint: c(p.text_faint),
            text_body: c(p.text_body),
            accent: c(p.accent),
            accent_fg: c(p.accent_fg),
            recv_bg: c(p.recv_bg),
            recv_topic: c(p.recv_topic),
            recv_payload: c(p.recv_payload),
            send_bg: c(p.send_bg),
            send_topic: c(p.send_topic),
            send_payload: c(p.send_payload),
            sys_bg: c(p.sys_bg),
            sys_text: c(p.sys_text),
            err_bg: c(p.err_bg),
            err_label: c(p.err_label),
            err_text: c(p.err_text),
            danger: c(p.danger),
        }
    }
}
