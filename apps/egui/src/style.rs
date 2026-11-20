//! Palette → egui style translation.

use eframe::egui::{
    Color32, FontFamily, FontId, Stroke, TextStyle, Visuals,
};
use mqttc_core::theme::{Palette, Rgb};

pub fn color(rgb: Rgb) -> Color32 {
    Color32::from_rgb(rgb.0, rgb.1, rgb.2)
}

/// Install the shared palette and font sizes into the egui context.
pub fn apply(ctx: &eframe::egui::Context, pal: &Palette, light: bool) {
    let mut visuals = if light { Visuals::light() } else { Visuals::dark() };
    visuals.panel_fill = color(pal.window_bg);
    visuals.window_fill = color(pal.panel_bg);
    visuals.extreme_bg_color = color(pal.inset_bg);
    visuals.faint_bg_color = color(pal.inset_bg);
    visuals.override_text_color = Some(color(pal.text_body));
    visuals.hyperlink_color = color(pal.accent);
    visuals.selection.bg_fill = color(pal.selected_bg);
    visuals.selection.stroke = Stroke::new(1.0, color(pal.text_primary));

    let widgets = &mut visuals.widgets;
    widgets.noninteractive.bg_fill = color(pal.panel_bg);
    widgets.noninteractive.weak_bg_fill = color(pal.panel_bg);
    widgets.noninteractive.bg_stroke = Stroke::new(1.0, color(pal.badge_bg));
    widgets.noninteractive.fg_stroke = Stroke::new(1.0, color(pal.text_body));

    widgets.inactive.bg_fill = color(pal.inset_bg);
    widgets.inactive.weak_bg_fill = color(pal.inset_bg);
    widgets.inactive.bg_stroke = Stroke::new(1.0, color(pal.badge_bg));
    widgets.inactive.fg_stroke = Stroke::new(1.0, color(pal.text_body));

    widgets.hovered.bg_fill = color(pal.hover_bg);
    widgets.hovered.weak_bg_fill = color(pal.hover_bg);
    widgets.hovered.bg_stroke = Stroke::new(1.0, color(pal.text_faint));
    widgets.hovered.fg_stroke = Stroke::new(1.0, color(pal.text_primary));

    widgets.active.bg_fill = color(pal.selected_bg);
    widgets.active.weak_bg_fill = color(pal.selected_bg);
    widgets.active.bg_stroke = Stroke::new(1.0, color(pal.accent));
    widgets.active.fg_stroke = Stroke::new(1.0, color(pal.text_primary));

    for w in [
        &mut widgets.inactive,
        &mut widgets.hovered,
        &mut widgets.active,
        &mut widgets.noninteractive,
        &mut widgets.open,
    ] {
        w.corner_radius = 6.into();
    }

    ctx.set_theme(if light {
        eframe::egui::ThemePreference::Light
    } else {
        eframe::egui::ThemePreference::Dark
    });
    ctx.set_visuals(visuals);
    ctx.all_styles_mut(|style| {
        style.spacing.item_spacing = eframe::egui::vec2(8.0, 6.0);
        style.spacing.button_padding = eframe::egui::vec2(10.0, 4.0);
        style.spacing.scroll.bar_width = 8.0;
        style.text_styles = [
            (TextStyle::Heading, FontId::new(16.0, FontFamily::Proportional)),
            (TextStyle::Body, FontId::new(14.0, FontFamily::Proportional)),
            (TextStyle::Button, FontId::new(14.0, FontFamily::Proportional)),
            (TextStyle::Small, FontId::new(12.0, FontFamily::Proportional)),
            (TextStyle::Monospace, FontId::new(13.0, FontFamily::Monospace)),
        ]
        .into();
    });
}

pub fn ui_font(size: f32) -> FontId {
    FontId::proportional(size)
}

pub fn mono_font(size: f32) -> FontId {
    FontId::monospace(size)
}
