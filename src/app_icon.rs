//! Application icon (embedded PNG) for window chrome and system tray.

use image::imageops::FilterType;
use image::RgbaImage;

const PNG: &[u8] = include_bytes!("../assets/tluw-icon.png");

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

/// RGBA bytes for window icon (256×256).
pub fn window_rgba() -> (Vec<u8>, u32, u32) {
    load_rgba(256)
}

/// Build an iced window icon from embedded PNG.
#[cfg(feature = "gui")]
pub fn iced_window_icon() -> iced::window::Icon {
    let (rgba, w, h) = window_rgba();
    iced::window::icon::from_rgba(rgba, w, h).expect("window icon")
}

/// RGBA bytes for `tray-icon` (32×32 reads cleanly in the notification area).
pub fn tray_rgba() -> (Vec<u8>, u32, u32) {
    load_rgba(32)
}
