//! Application icon (embedded PNG) for window chrome and system tray.

use egui::IconData;
use image::imageops::FilterType;
use image::RgbaImage;

const PNG: &[u8] = include_bytes!("../assets/windows-diagnostics-icon.png");

fn load_rgba(size: u32) -> (Vec<u8>, u32, u32) {
    let img = image::load_from_memory(PNG)
        .expect("embedded app icon PNG")
        .into_rgba8();
    let resized: RgbaImage = if img.width() == size && img.height() == size {
        img
    } else {
        image::imageops::resize(&img, size, size, FilterType::Lanczos3)
    };
    let (w, h) = resized.dimensions();
    (resized.into_raw(), w, h)
}

/// egui / eframe window icon (multi-size friendly: 256).
pub fn egui_icon() -> IconData {
    let (rgba, w, h) = load_rgba(256);
    IconData {
        rgba,
        width: w,
        height: h,
    }
}

/// RGBA bytes for `tray-icon` (32×32 reads cleanly in the notification area).
pub fn tray_rgba() -> (Vec<u8>, u32, u32) {
    load_rgba(32)
}
