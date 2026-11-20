//! Small widget helpers shared by the GPUI pages.
//!
//! GPUI ships divs, text and lists but no widget kit, so buttons, toggles and
//! select chips are composed here. Dialogue-style dropdowns are replaced by
//! click-to-cycle chips (see docs/PARITY.md).

use gpui::prelude::*;
use gpui::{div, px, Div, ElementId, IntoElement, Rgba, Stateful};

use crate::theme::Colors;

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    Primary,
    Ghost,
    Danger,
    Selected,
}

pub fn button(
    c: &Colors,
    id: impl Into<ElementId>,
    label: impl IntoElement,
    kind: Kind,
) -> Stateful<Div> {
    let (bg, fg, border) = match kind {
        Kind::Primary => (c.accent, c.accent_fg, c.accent),
        Kind::Ghost => (c.inset_bg, c.text_body, c.hover_bg),
        Kind::Danger => (c.inset_bg, c.danger, c.hover_bg),
        Kind::Selected => (c.selected_bg, c.text_primary, c.selected_bg),
    };
    let hover = match kind {
        Kind::Primary => c.selected_bg,
        Kind::Ghost | Kind::Danger => c.hover_bg,
        Kind::Selected => c.hover_bg,
    };
    div()
        .id(id)
        .flex()
        .items_center()
        .justify_center()
        .px(px(10.))
        .py(px(4.))
        .rounded(px(6.))
        .border_1()
        .border_color(border)
        .bg(bg)
        .text_color(fg)
        .text_size(px(12.))
        .cursor_pointer()
        .hover(move |s| s.bg(hover))
        .child(label)
}

/// Click-to-cycle selector: renders `label ▾` and advances to the next option.
pub fn chip(c: &Colors, id: impl Into<ElementId>, label: impl IntoElement) -> Stateful<Div> {
    div()
        .id(id)
        .flex()
        .items_center()
        .gap(px(4.))
        .px(px(8.))
        .py(px(4.))
        .rounded(px(6.))
        .border_1()
        .border_color(c.hover_bg)
        .bg(c.inset_bg)
        .text_color(c.text_body)
        .text_size(px(12.))
        .cursor_pointer()
        .hover(move |s| s.bg(c.hover_bg))
        .child(label)
        .child(div().text_size(px(9.)).text_color(c.text_faint).child("▾"))
}

pub fn checkbox(
    c: &Colors,
    id: impl Into<ElementId>,
    label: &str,
    on: bool,
) -> Stateful<Div> {
    let box_bg = if on { c.accent } else { c.inset_bg };
    let mark = if on { "✓" } else { "" };
    div()
        .id(id)
        .flex()
        .items_center()
        .gap(px(5.))
        .cursor_pointer()
        .child(
            div()
                .flex()
                .items_center()
                .justify_center()
                .size(px(14.))
                .rounded(px(3.))
                .border_1()
                .border_color(if on { c.accent } else { c.hover_bg })
                .bg(box_bg)
                .text_color(c.accent_fg)
                .text_size(px(10.))
                .child(mark),
        )
        .child(div().text_size(px(12.)).text_color(c.text_body).child(label.to_string()))
}

pub fn section(c: &Colors, title: &str) -> Div {
    div()
        .text_size(px(14.))
        .text_color(c.text_title)
        .child(title.to_string())
}

pub fn hint(c: &Colors, text: impl Into<String>) -> Div {
    div()
        .text_size(px(11.))
        .text_color(c.text_faint)
        .child(text.into())
}

/// Panel chrome: solid fill, hairline border, rounded corners.
pub fn card(c: &Colors) -> Div {
    div()
        .bg(c.panel_bg)
        .border_1()
        .border_color(c.hover_bg)
        .rounded(px(8.))
}

pub fn well(c: &Colors) -> Div {
    div()
        .bg(c.inset_bg)
        .border_1()
        .border_color(c.hover_bg)
        .rounded(px(6.))
}

pub fn dot(color: Rgba) -> Div {
    div().text_size(px(9.)).text_color(color).child("●")
}
