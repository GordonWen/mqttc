//! mqttc · egui shell.
//!
//! Thin UI over `mqttc-core`: the runtime owns config, MQTT workers and logs;
//! this crate only paints widgets and queues actions.

#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod app;
mod bench;
mod style;

use eframe::egui;

/// Decode `assets/icon.png` into the RGBA buffer egui wants for the window icon.
fn load_icon() -> Option<egui::IconData> {
    let bytes = include_bytes!("../../../assets/icon.png");
    let decoder = png::Decoder::new(std::io::Cursor::new(&bytes[..]));
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
    Some(egui::IconData {
        rgba,
        width: info.width,
        height: info.height,
    })
}

fn main() -> eframe::Result {
    let options = mqttc_core::cli::parse(std::env::args().skip(1));

    let mut viewport = egui::ViewportBuilder::default()
        .with_title("mqttc · egui")
        .with_inner_size([1360.0, 860.0])
        .with_min_inner_size([1180.0, 720.0]);
    if let Some(icon) = load_icon() {
        viewport = viewport.with_icon(icon);
    }

    let native_options = eframe::NativeOptions {
        viewport,
        renderer: eframe::Renderer::Glow,
        // CJK / Arabic glyphs are loaded from installed system fonts on demand;
        // no font bytes are bundled, which keeps the portable exe small.
        system_font_fallback: true,
        centered: true,
        ..Default::default()
    };

    eframe::run_native(
        "mqttc",
        native_options,
        Box::new(move |cc| Ok(Box::new(app::App::new(cc, options)))),
    )
}
