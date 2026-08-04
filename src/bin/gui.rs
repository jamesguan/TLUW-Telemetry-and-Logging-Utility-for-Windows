//! GUI binary — thin shell over the shared telemetry library.

#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]
#![cfg(windows)]

use eframe::egui;
use windows_diagnostics::gui::DiagnosticsApp;
use windows_diagnostics::telemetry;

fn main() -> eframe::Result<()> {
    if !telemetry::is_elevated() {
        // Auto-elevate; if user cancels UAC, still open GUI with a prompt.
        if telemetry::relaunch_elevated().is_ok() {
            return Ok(());
        }
    }

    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([720.0, 780.0])
            .with_min_inner_size([560.0, 480.0])
            .with_title("Windows Diagnostics"),
        ..Default::default()
    };

    eframe::run_native(
        "Windows Diagnostics",
        options,
        Box::new(|cc| {
            let mut style = (*cc.egui_ctx.style()).clone();
            style.spacing.item_spacing = egui::vec2(8.0, 6.0);
            cc.egui_ctx.set_style(style);
            Ok(Box::new(DiagnosticsApp::default()))
        }),
    )
}
