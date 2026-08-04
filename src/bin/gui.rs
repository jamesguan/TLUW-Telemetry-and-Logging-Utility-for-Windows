//! GUI binary — thin shell over the shared telemetry library.

#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]
#![cfg(windows)]

use telemetry_logging_utility::single_instance;
use telemetry_logging_utility::telemetry;

fn main() -> iced::Result {
    telemetry_logging_utility::bootstrap();

    // Already running (window may be hidden in the tray) — just focus it.
    if single_instance::activate_existing() {
        return Ok(());
    }

    if !telemetry::is_elevated() {
        // Don't spawn a second elevated copy if the first is still coming up.
        if single_instance::activate_existing_with_retry() {
            return Ok(());
        }
        if telemetry::relaunch_elevated().is_ok() {
            return Ok(());
        }
    }

    let Some(_guard) = single_instance::try_acquire() else {
        let _ = single_instance::activate_existing_with_retry();
        return Ok(());
    };

    telemetry_logging_utility::gui::run()
}
