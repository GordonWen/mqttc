//! Named palette for the visual skins, ported from the Slint reference.
//!
//! Shells convert these RGB tuples into their own color type once per frame.
//! Keeping the table here means the three ports cannot drift apart.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::model::Skin;

pub type Rgb = (u8, u8, u8);

#[derive(Clone, Copy, Debug)]
pub struct Palette {
    pub window_bg: Rgb,
    pub panel_bg: Rgb,
    pub inset_bg: Rgb,
    pub hover_bg: Rgb,
    pub selected_bg: Rgb,
    pub badge_bg: Rgb,
    pub text_primary: Rgb,
    pub text_title: Rgb,
    pub text_muted: Rgb,
    pub text_faint: Rgb,
    pub text_body: Rgb,
    pub accent: Rgb,
    pub accent_fg: Rgb,
    pub muted: Rgb,
    pub recv_bg: Rgb,
    pub recv_topic: Rgb,
    pub recv_payload: Rgb,
    pub send_bg: Rgb,
    pub send_topic: Rgb,
    pub send_payload: Rgb,
    pub sys_bg: Rgb,
    pub sys_text: Rgb,
    pub err_bg: Rgb,
    pub err_label: Rgb,
    pub err_text: Rgb,
    pub danger: Rgb,
    pub sub_selected: Rgb,
}

const BLACK: Palette = Palette {
    window_bg: (0x16, 0x18, 0x1d),
    panel_bg: (0x1e, 0x22, 0x29),
    inset_bg: (0x12, 0x14, 0x1a),
    hover_bg: (0x3a, 0x3f, 0x49),
    selected_bg: (0x2b, 0x32, 0x40),
    badge_bg: (0x24, 0x28, 0x30),
    text_primary: (0xf2, 0xf4, 0xf8),
    text_title: (0xc5, 0xca, 0xd3),
    text_muted: (0x8b, 0x91, 0x9a),
    text_faint: (0x6d, 0x73, 0x7c),
    text_body: (0xe4, 0xe8, 0xef),
    accent: (0x2f, 0x6f, 0xd0),
    accent_fg: (0xff, 0xff, 0xff),
    muted: (0x9a, 0xa0, 0xa6),
    recv_bg: (0x1b, 0x2f, 0x24),
    recv_topic: (0x4e, 0xcf, 0x7a),
    recv_payload: (0xd7, 0xe8, 0xdc),
    send_bg: (0x1d, 0x2f, 0x4a),
    send_topic: (0x5a, 0xa7, 0xff),
    send_payload: (0xd5, 0xe4, 0xf7),
    sys_bg: (0x24, 0x28, 0x30),
    sys_text: (0xb8, 0xc0, 0xcc),
    err_bg: (0x3a, 0x22, 0x24),
    err_label: (0xe8, 0x5d, 0x5d),
    err_text: (0xf0, 0xc4, 0xc4),
    danger: (0xe8, 0x5d, 0x5d),
    sub_selected: (0x24, 0x32, 0x48),
};

const WHITE: Palette = Palette {
    window_bg: (0xe8, 0xed, 0xf3),
    panel_bg: (0xff, 0xff, 0xff),
    inset_bg: (0xf3, 0xf6, 0xfa),
    hover_bg: (0xe4, 0xea, 0xf2),
    selected_bg: (0xd7, 0xe6, 0xf8),
    badge_bg: (0xe7, 0xec, 0xf3),
    text_primary: (0x1c, 0x24, 0x30),
    text_title: (0x33, 0x41, 0x55),
    text_muted: (0x5b, 0x65, 0x73),
    text_faint: (0x7a, 0x84, 0x91),
    text_body: (0x1c, 0x24, 0x30),
    accent: (0x2f, 0x6f, 0xd0),
    accent_fg: (0xff, 0xff, 0xff),
    muted: (0x6b, 0x72, 0x80),
    recv_bg: (0xe4, 0xf6, 0xea),
    recv_topic: (0x1f, 0x8a, 0x4c),
    recv_payload: (0x1a, 0x3d, 0x28),
    send_bg: (0xe4, 0xee, 0xfc),
    send_topic: (0x1d, 0x5f, 0xbf),
    send_payload: (0x1a, 0x2d, 0x4a),
    sys_bg: (0xe7, 0xec, 0xf3),
    sys_text: (0x3d, 0x46, 0x54),
    err_bg: (0xfd, 0xe8, 0xe8),
    err_label: (0xc6, 0x28, 0x28),
    err_text: (0x7a, 0x1f, 0x1f),
    danger: (0xc6, 0x28, 0x28),
    sub_selected: (0xd7, 0xe6, 0xf8),
};

/// DingTalk blue: every surface is a darkened shade of the brand azure
/// (#0089FF), so the whole window — not just the buttons — carries the hue.
const BLUE: Palette = Palette {
    window_bg: (0x07, 0x1c, 0x30),
    panel_bg: (0x0a, 0x2a, 0x45),
    inset_bg: (0x04, 0x16, 0x26),
    hover_bg: (0x0f, 0x3a, 0x5c),
    selected_bg: (0x11, 0x52, 0x7f),
    badge_bg: (0x0c, 0x33, 0x52),
    text_primary: (0xee, 0xf6, 0xff),
    text_title: (0xd7, 0xe8, 0xf7),
    text_muted: (0x93, 0xb4, 0xcf),
    text_faint: (0x73, 0x96, 0xb0),
    text_body: (0xdc, 0xeb, 0xf8),
    accent: (0x00, 0x89, 0xff),
    accent_fg: (0x04, 0x18, 0x28),
    muted: (0x8f, 0xb2, 0xcf),
    recv_bg: (0x0a, 0x35, 0x27),
    recv_topic: (0x3d, 0xdc, 0x84),
    recv_payload: (0xd9, 0xf7, 0xe5),
    send_bg: (0x0a, 0x31, 0x52),
    send_topic: (0x6c, 0xc4, 0xff),
    send_payload: (0xd8, 0xec, 0xff),
    sys_bg: (0x0c, 0x33, 0x52),
    sys_text: (0xb6, 0xd2, 0xe8),
    err_bg: (0x3a, 0x1f, 0x2a),
    err_label: (0xff, 0x7a, 0x8a),
    err_text: (0xff, 0xd6, 0xdc),
    danger: (0xff, 0x5f, 0x73),
    sub_selected: (0x11, 0x52, 0x7f),
};

/// WeChat green: the deep surfaces are shades of the icon green (#07C160),
/// with a pale green-tinted foreground instead of neutral grey.
const GREEN: Palette = Palette {
    window_bg: (0x06, 0x28, 0x1b),
    panel_bg: (0x0a, 0x3b, 0x28),
    inset_bg: (0x04, 0x20, 0x16),
    hover_bg: (0x0f, 0x52, 0x37),
    selected_bg: (0x12, 0x70, 0x43),
    badge_bg: (0x0c, 0x49, 0x2f),
    text_primary: (0xee, 0xff, 0xf6),
    text_title: (0xd3, 0xf4, 0xe2),
    text_muted: (0x90, 0xc2, 0xa6),
    text_faint: (0x6f, 0xa8, 0x89),
    text_body: (0xd9, 0xf3, 0xe4),
    accent: (0x07, 0xc1, 0x60),
    accent_fg: (0x03, 0x20, 0x15),
    muted: (0x8a, 0xbd, 0x9f),
    recv_bg: (0x0b, 0x3f, 0x2a),
    recv_topic: (0x22, 0xd9, 0x7a),
    recv_payload: (0xd9, 0xf9, 0xe7),
    send_bg: (0x08, 0x35, 0x3c),
    send_topic: (0x5b, 0xe0, 0xd0),
    send_payload: (0xd5, 0xf8, 0xf3),
    sys_bg: (0x0c, 0x49, 0x2f),
    sys_text: (0xbf, 0xe6, 0xcf),
    err_bg: (0x3d, 0x21, 0x21),
    err_label: (0xff, 0x7a, 0x70),
    err_text: (0xff, 0xd7, 0xd2),
    danger: (0xff, 0x64, 0x5a),
    sub_selected: (0x12, 0x70, 0x43),
};

/// Obsidian purple: violet-tinted surfaces built around the Obsidian accent
/// (#7C3AED), separated from the grey-blue and neutral skins at a glance.
const PURPLE: Palette = Palette {
    window_bg: (0x1a, 0x0f, 0x33),
    panel_bg: (0x26, 0x15, 0x47),
    inset_bg: (0x12, 0x0a, 0x24),
    hover_bg: (0x35, 0x20, 0x5c),
    selected_bg: (0x43, 0x2a, 0x79),
    badge_bg: (0x2e, 0x1b, 0x55),
    text_primary: (0xf7, 0xf3, 0xff),
    text_title: (0xe3, 0xd8, 0xfb),
    text_muted: (0xb3, 0xa3, 0xd6),
    text_faint: (0x8f, 0x7e, 0xb1),
    text_body: (0xed, 0xe6, 0xfb),
    accent: (0x7c, 0x3a, 0xed),
    accent_fg: (0xff, 0xff, 0xff),
    muted: (0xab, 0x9c, 0xc9),
    recv_bg: (0x17, 0x3a, 0x2b),
    recv_topic: (0x4a, 0xde, 0x80),
    recv_payload: (0xdc, 0xfc, 0xe7),
    send_bg: (0x31, 0x20, 0x5a),
    send_topic: (0xc4, 0xb5, 0xfd),
    send_payload: (0xf0, 0xea, 0xff),
    sys_bg: (0x2e, 0x1b, 0x55),
    sys_text: (0xcd, 0xbd, 0xea),
    err_bg: (0x3c, 0x1f, 0x33),
    err_label: (0xfb, 0x71, 0x85),
    err_text: (0xff, 0xd6, 0xe0),
    danger: (0xe1, 0x1d, 0x48),
    sub_selected: (0x43, 0x2a, 0x79),
};

/// Resolve System to Black/White and return the matching table.
/// `Custom` is resolved by [`CustomPalette::resolve`] before this is called;
/// falling back to Black keeps every caller total.
pub fn palette(skin: Skin) -> &'static Palette {
    match skin.resolve() {
        Skin::White => &WHITE,
        Skin::Blue => &BLUE,
        Skin::Green => &GREEN,
        Skin::Purple => &PURPLE,
        Skin::Custom => &BLACK,
        _ => &BLACK,
    }
}

// ------------------------------------------------------------ custom palette

/// One color slot in [`Palette`], with stable JSON keys and settings labels.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PaletteField {
    WindowBg,
    PanelBg,
    InsetBg,
    HoverBg,
    SelectedBg,
    BadgeBg,
    TextPrimary,
    TextTitle,
    TextMuted,
    TextFaint,
    TextBody,
    Accent,
    AccentFg,
    Muted,
    RecvBg,
    RecvTopic,
    RecvPayload,
    SendBg,
    SendTopic,
    SendPayload,
    SysBg,
    SysText,
    ErrBg,
    ErrLabel,
    ErrText,
    Danger,
    SubSelected,
}

impl PaletteField {
    pub const ALL: [Self; 27] = [
        Self::WindowBg,
        Self::PanelBg,
        Self::InsetBg,
        Self::HoverBg,
        Self::SelectedBg,
        Self::BadgeBg,
        Self::TextPrimary,
        Self::TextTitle,
        Self::TextMuted,
        Self::TextFaint,
        Self::TextBody,
        Self::Accent,
        Self::AccentFg,
        Self::Muted,
        Self::RecvBg,
        Self::RecvTopic,
        Self::RecvPayload,
        Self::SendBg,
        Self::SendTopic,
        Self::SendPayload,
        Self::SysBg,
        Self::SysText,
        Self::ErrBg,
        Self::ErrLabel,
        Self::ErrText,
        Self::Danger,
        Self::SubSelected,
    ];

    /// Always-visible colors on the settings page.
    pub const COMMON: [Self; 10] = [
        Self::WindowBg,
        Self::PanelBg,
        Self::InsetBg,
        Self::Accent,
        Self::TextBody,
        Self::TextMuted,
        Self::RecvBg,
        Self::RecvTopic,
        Self::SendBg,
        Self::SendTopic,
    ];

    /// Folded under “更多颜色”.
    pub const MORE: [Self; 17] = [
        Self::HoverBg,
        Self::SelectedBg,
        Self::BadgeBg,
        Self::TextPrimary,
        Self::TextTitle,
        Self::TextFaint,
        Self::AccentFg,
        Self::Muted,
        Self::RecvPayload,
        Self::SendPayload,
        Self::SysBg,
        Self::SysText,
        Self::ErrBg,
        Self::ErrLabel,
        Self::ErrText,
        Self::Danger,
        Self::SubSelected,
    ];

    /// Stable key used in `mqttc.json`.
    pub fn key(self) -> &'static str {
        match self {
            Self::WindowBg => "window_bg",
            Self::PanelBg => "panel_bg",
            Self::InsetBg => "inset_bg",
            Self::HoverBg => "hover_bg",
            Self::SelectedBg => "selected_bg",
            Self::BadgeBg => "badge_bg",
            Self::TextPrimary => "text_primary",
            Self::TextTitle => "text_title",
            Self::TextMuted => "text_muted",
            Self::TextFaint => "text_faint",
            Self::TextBody => "text_body",
            Self::Accent => "accent",
            Self::AccentFg => "accent_fg",
            Self::Muted => "muted",
            Self::RecvBg => "recv_bg",
            Self::RecvTopic => "recv_topic",
            Self::RecvPayload => "recv_payload",
            Self::SendBg => "send_bg",
            Self::SendTopic => "send_topic",
            Self::SendPayload => "send_payload",
            Self::SysBg => "sys_bg",
            Self::SysText => "sys_text",
            Self::ErrBg => "err_bg",
            Self::ErrLabel => "err_label",
            Self::ErrText => "err_text",
            Self::Danger => "danger",
            Self::SubSelected => "sub_selected",
        }
    }

    pub fn label(self) -> (&'static str, &'static str) {
        match self {
            Self::WindowBg => ("窗口底色", "Window bg"),
            Self::PanelBg => ("面板底色", "Panel bg"),
            Self::InsetBg => ("输入框底色", "Input bg"),
            Self::HoverBg => ("悬停底色", "Hover bg"),
            Self::SelectedBg => ("选中底色", "Selected bg"),
            Self::BadgeBg => ("徽标底色", "Badge bg"),
            Self::TextPrimary => ("高亮文字", "Primary text"),
            Self::TextTitle => ("标题文字", "Title text"),
            Self::TextMuted => ("次要文字", "Muted text"),
            Self::TextFaint => ("弱化文字", "Faint text"),
            Self::TextBody => ("正文文字", "Body text"),
            Self::Accent => ("强调色", "Accent"),
            Self::AccentFg => ("按钮文字", "Button text"),
            Self::Muted => ("系统标签", "System label"),
            Self::RecvBg => ("接收气泡底色", "Recv bubble bg"),
            Self::RecvTopic => ("接收主题色", "Recv topic"),
            Self::RecvPayload => ("接收正文", "Recv payload"),
            Self::SendBg => ("发送气泡底色", "Send bubble bg"),
            Self::SendTopic => ("发送主题色", "Send topic"),
            Self::SendPayload => ("发送正文", "Send payload"),
            Self::SysBg => ("系统气泡底色", "Sys bubble bg"),
            Self::SysText => ("系统气泡文字", "Sys bubble text"),
            Self::ErrBg => ("错误气泡底色", "Err bubble bg"),
            Self::ErrLabel => ("错误标签", "Err label"),
            Self::ErrText => ("错误正文", "Err text"),
            Self::Danger => ("危险色", "Danger"),
            Self::SubSelected => ("订阅选中底色", "Subscription selected"),
        }
    }

    pub fn get(self, p: &Palette) -> Rgb {
        match self {
            Self::WindowBg => p.window_bg,
            Self::PanelBg => p.panel_bg,
            Self::InsetBg => p.inset_bg,
            Self::HoverBg => p.hover_bg,
            Self::SelectedBg => p.selected_bg,
            Self::BadgeBg => p.badge_bg,
            Self::TextPrimary => p.text_primary,
            Self::TextTitle => p.text_title,
            Self::TextMuted => p.text_muted,
            Self::TextFaint => p.text_faint,
            Self::TextBody => p.text_body,
            Self::Accent => p.accent,
            Self::AccentFg => p.accent_fg,
            Self::Muted => p.muted,
            Self::RecvBg => p.recv_bg,
            Self::RecvTopic => p.recv_topic,
            Self::RecvPayload => p.recv_payload,
            Self::SendBg => p.send_bg,
            Self::SendTopic => p.send_topic,
            Self::SendPayload => p.send_payload,
            Self::SysBg => p.sys_bg,
            Self::SysText => p.sys_text,
            Self::ErrBg => p.err_bg,
            Self::ErrLabel => p.err_label,
            Self::ErrText => p.err_text,
            Self::Danger => p.danger,
            Self::SubSelected => p.sub_selected,
        }
    }

    fn set(self, p: &mut Palette, value: Rgb) {
        match self {
            Self::WindowBg => p.window_bg = value,
            Self::PanelBg => p.panel_bg = value,
            Self::InsetBg => p.inset_bg = value,
            Self::HoverBg => p.hover_bg = value,
            Self::SelectedBg => p.selected_bg = value,
            Self::BadgeBg => p.badge_bg = value,
            Self::TextPrimary => p.text_primary = value,
            Self::TextTitle => p.text_title = value,
            Self::TextMuted => p.text_muted = value,
            Self::TextFaint => p.text_faint = value,
            Self::TextBody => p.text_body = value,
            Self::Accent => p.accent = value,
            Self::AccentFg => p.accent_fg = value,
            Self::Muted => p.muted = value,
            Self::RecvBg => p.recv_bg = value,
            Self::RecvTopic => p.recv_topic = value,
            Self::RecvPayload => p.recv_payload = value,
            Self::SendBg => p.send_bg = value,
            Self::SendTopic => p.send_topic = value,
            Self::SendPayload => p.send_payload = value,
            Self::SysBg => p.sys_bg = value,
            Self::SysText => p.sys_text = value,
            Self::ErrBg => p.err_bg = value,
            Self::ErrLabel => p.err_label = value,
            Self::ErrText => p.err_text = value,
            Self::Danger => p.danger = value,
            Self::SubSelected => p.sub_selected = value,
        }
    }
}

/// `#rrggbb` (the leading `#` is optional). Anything else is ignored.
pub fn parse_hex(text: &str) -> Option<Rgb> {
    let s = text.trim().trim_start_matches('#');
    if s.len() != 6 || !s.is_ascii() {
        return None;
    }
    Some((
        u8::from_str_radix(&s[0..2], 16).ok()?,
        u8::from_str_radix(&s[2..4], 16).ok()?,
        u8::from_str_radix(&s[4..6], 16).ok()?,
    ))
}

pub fn to_hex((r, g, b): Rgb) -> String {
    format!("#{r:02x}{g:02x}{b:02x}")
}

/// User-defined palette persisted in `mqttc.json`. Missing or unparsable
/// entries fall back to `base`, so partial files and mid-typing values are safe.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct CustomPalette {
    /// Skin whose colors seed and back-fill this palette.
    #[serde(default)]
    pub base: String,
    /// `PaletteField::key()` → raw hex text (may be invalid while editing).
    #[serde(default)]
    pub colors: BTreeMap<String, String>,
}

impl CustomPalette {
    pub fn from_base(skin: Skin) -> Self {
        let p = palette(skin);
        let mut colors = BTreeMap::new();
        for field in PaletteField::ALL {
            colors.insert(field.key().to_string(), to_hex(field.get(p)));
        }
        Self {
            base: skin.to_key().to_string(),
            colors,
        }
    }

    pub fn is_empty(&self) -> bool {
        self.colors.is_empty()
    }

    /// Base skin used for fallback; `custom` or unknown keys become Black.
    pub fn base_skin(&self) -> Skin {
        let skin = Skin::from_key(&self.base).resolve();
        if skin == Skin::Custom {
            Skin::Black
        } else {
            skin
        }
    }

    /// Full palette with every valid override applied.
    pub fn resolve(&self) -> Palette {
        let mut p = *palette(self.base_skin());
        for field in PaletteField::ALL {
            if let Some(rgb) = self.colors.get(field.key()).and_then(|hex| parse_hex(hex)) {
                field.set(&mut p, rgb);
            }
        }
        p
    }

    /// Raw stored text, or the fallback color formatted as hex.
    pub fn text(&self, field: PaletteField, fallback: Rgb) -> String {
        self.colors
            .get(field.key())
            .cloned()
            .unwrap_or_else(|| to_hex(fallback))
    }

    pub fn set(&mut self, field: PaletteField, value: String) {
        self.colors.insert(field.key().to_string(), value);
    }

    /// Drop one override; the slot falls back to the base skin.
    pub fn reset(&mut self, field: PaletteField) {
        self.colors.remove(field.key());
    }

    /// Replace every entry with the base skin's colors, keeping the base key.
    pub fn refill(&mut self) {
        let base = self.base_skin();
        *self = Self::from_base(base);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hex_round_trip_and_validation() {
        assert_eq!(parse_hex("#2F6FD0"), Some((0x2f, 0x6f, 0xd0)));
        assert_eq!(parse_hex("2f6fd0"), Some((0x2f, 0x6f, 0xd0)));
        assert_eq!(parse_hex("  #ffffff  "), Some((0xff, 0xff, 0xff)));
        assert_eq!(parse_hex("#fff"), None);
        assert_eq!(parse_hex("#gggggg"), None);
        assert_eq!(parse_hex("2f6fd0ff"), None);
        assert_eq!(to_hex((0x0a, 0x33, 0x2a)), "#0a332a");
    }

    #[test]
    fn from_base_fills_every_slot_and_matches_base() {
        let custom = CustomPalette::from_base(Skin::White);
        assert_eq!(custom.colors.len(), PaletteField::ALL.len());
        assert_eq!(custom.base_skin(), Skin::White);
        let resolved = custom.resolve();
        let base = *palette(Skin::White);
        for field in PaletteField::ALL {
            assert_eq!(field.get(&resolved), field.get(&base), "{:?}", field);
        }
    }

    #[test]
    fn resolve_ignores_invalid_values_and_reset_falls_back() {
        let mut custom = CustomPalette::from_base(Skin::Black);
        custom.set(PaletteField::Accent, "#123456".into());
        assert_eq!(
            PaletteField::Accent.get(&custom.resolve()),
            (0x12, 0x34, 0x56)
        );
        custom.set(PaletteField::Accent, "#12".into());
        assert_eq!(
            PaletteField::Accent.get(&custom.resolve()),
            PaletteField::Accent.get(palette(Skin::Black))
        );
        custom.set(PaletteField::Accent, "#123456".into());
        custom.reset(PaletteField::Accent);
        assert!(!custom.colors.contains_key("accent"));
        assert_eq!(
            PaletteField::Accent.get(&custom.resolve()),
            PaletteField::Accent.get(palette(Skin::Black))
        );
    }

    #[test]
    fn unknown_base_falls_back_to_black() {
        let mut custom = CustomPalette {
            base: "custom".into(),
            ..CustomPalette::default()
        };
        assert_eq!(custom.base_skin(), Skin::Black);
        custom.base = "nope".into();
        assert_eq!(custom.base_skin(), Skin::Black);
        assert_eq!(
            PaletteField::WindowBg.get(&custom.resolve()),
            PaletteField::WindowBg.get(palette(Skin::Black))
        );
    }

    #[test]
    fn more_and_common_cover_every_field_once() {
        let mut seen = Vec::new();
        seen.extend(PaletteField::COMMON);
        seen.extend(PaletteField::MORE);
        seen.sort_by_key(|f| f.key());
        let mut all = PaletteField::ALL.to_vec();
        all.sort_by_key(|f| f.key());
        assert_eq!(seen, all);
    }
}
