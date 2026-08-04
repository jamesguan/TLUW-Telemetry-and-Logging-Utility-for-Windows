//! GUI binary — thin shell over the shared telemetry library.

#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]
#![cfg(windows)]

use eframe::egui;
use raw_window_handle::{HasWindowHandle, RawWindowHandle};
use windows_diagnostics::app_icon;
use windows_diagnostics::gui::DiagnosticsApp;
use windows_diagnostics::telemetry;

fn resolve_hwnd(cc: &eframe::CreationContext<'_>) -> isize {
    let Ok(handle) = cc.window_handle() else {
        return 0;
    };
    match handle.as_raw() {
        RawWindowHandle::Win32(h) => h.hwnd.get() as isize,
        _ => 0,
    }
}

fn main() -> eframe::Result<()> {
    if !telemetry::is_elevated() {
        // Auto-elevate; if user cancels UAC, still open GUI with a prompt.
        if telemetry::relaunch_elevated().is_ok() {
            return Ok(());
        }
    }

    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([760.0, 900.0])
            .with_min_inner_size([600.0, 560.0])
            .with_title("Windows Diagnostics")
            .with_icon(app_icon::egui_icon()),
        ..Default::default()
    };

    eframe::run_native(
        "Windows Diagnostics",
        options,
        Box::new(|cc| {
            let mut style = (*cc.egui_ctx.style()).clone();
            style.spacing.item_spacing = egui::vec2(8.0, 6.0);
            cc.egui_ctx.set_style(style);
            let hwnd = resolve_hwnd(&cc);
            Ok(Box::new(DiagnosticsApp::new(hwnd, cc.egui_ctx.clone())))
        }),
    )
}
