//! GUI binary — thin shell over the shared telemetry library.

#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]
#![cfg(windows)]

use telemetry_logging_utility::telemetry;

fn main() -> iced::Result {
    telemetry_logging_utility::bootstrap();

    if !telemetry::is_elevated() {
        if telemetry::relaunch_elevated().is_ok() {
            return Ok(());
        }
    }

    telemetry_logging_utility::gui::run()
}
