//! mqttc · Iced shell.
//!
//! Thin UI over `mqttc-core`: the runtime owns config, MQTT workers and logs;
//! this crate only paints widgets and queues actions. Rendering uses iced's
//! `tiny-skia` software backend (no wgpu) to keep both binary size and idle
//! GPU usage down.

#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod app;
mod style;

use std::io::Cursor;

use iced::{window, Size};

/// Decodes `assets/icons/mqttc-256.png` into the RGBA buffer shared by the
/// window icon and the in-app mark.
pub fn icon_rgba() -> Option<(u32, u32, Vec<u8>)> {
    let bytes = include_bytes!("../../../assets/icons/mqttc-256.png");
    let decoder = png::Decoder::new(Cursor::new(&bytes[..]));
    let mut reader = decoder.read_info().ok()?;
    let mut buf = vec![0; reader.output_buffer_size()?];
    let info = reader.next_frame(&mut buf).ok()?;
    let raw = &buf[..info.buffer_size()];
    let rgba = match info.color_type {
        png::ColorType::Rgba => raw.to_vec(),
        png::ColorType::Rgb => raw
            .chunks_exact(3)
            .flat_map(|px| [px[0], px[1], px[2], 255])
            .collect(),
        png::ColorType::Grayscale => raw.iter().flat_map(|g| [*g, *g, *g, 255]).collect(),
        png::ColorType::GrayscaleAlpha => raw
            .chunks_exact(2)
            .flat_map(|px| [px[0], px[0], px[0], px[1]])
            .collect(),
        png::ColorType::Indexed => return None,
    };
    Some((info.width, info.height, rgba))
}

fn load_icon() -> Option<window::Icon> {
    let (width, height, rgba) = icon_rgba()?;
    iced::window::icon::from_rgba(rgba, width, height).ok()
}

fn main() -> iced::Result {
    let mut settings = window::Settings {
        size: Size::new(1360.0, 860.0),
        min_size: Some(Size::new(980.0, 640.0)),
        // The shell draws its own title strip (brand + window buttons) and
        // resize grips, so the OS caption bar is disabled on every platform.
        decorations: false,
        ..window::Settings::default()
    };
    if let Some(icon) = load_icon() {
        settings.icon = Some(icon);
    }

    iced::application(app::App::boot, app::App::update, app::App::view)
        .title("MQTTC")
        .theme(app::App::theme)
        .subscription(app::App::subscription)
        .default_font(app::ui_font())
        .window(settings)
        .centered()
        .run()
}
