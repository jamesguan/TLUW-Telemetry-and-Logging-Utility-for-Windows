//! Shared library for Telemetry and Logging Utility for Windows.
//!
//! Layout:
//! - [`identity`] — product name, registry/APPDATA paths, legacy migration
//! - [`telemetry`] — core read/apply API (CLI + GUI)
//! - [`log_cleanup`] — wipe Diagnosis / event logs / WER / CBS (CLI + GUI)
//! - [`temp_cleanup`] — clear TEMP / Windows\\Temp / Prefetch (CLI + GUI)
//! - [`cleanup_history`] — daily GB freed by clears (dashboard / CLI)
//! - [`prefs`] — GUI preferences (system tray, theme, …)
//! - [`disclaimer`] — no-warranty / liability text + acceptance marker
//! - [`gui`] — iced app (feature `gui` only); thin UI over library APIs
//!
//! Binaries (independent):
//! - `tluw` — CLI
//! - `tluw-gui` — GUI helper on top of the same API
//!
//! Prefer extending the library (+ CLI) first; the GUI should call those APIs,
//! not reimplement registry/service/task logic.

#![cfg(windows)]

pub mod cleanup_history;
pub mod disclaimer;
pub mod identity;
pub mod log_cleanup;
pub mod maintenance;
pub mod prefs;
pub mod system_links;
pub mod telemetry;
pub mod temp_cleanup;
mod win_cmd;

#[cfg(feature = "gui")]
pub mod app_icon;
#[cfg(feature = "gui")]
pub mod gui;
#[cfg(feature = "gui")]
pub mod tray;

pub use telemetry::{
    apply, apply_all, ensure_elevated, is_elevated, read_all, read_one, relaunch_elevated,
    relaunch_elevated_with_args, SettingId, SettingState,
};

/// One-time identity migration (registry, APPDATA, Run key, scheduled task).
pub fn bootstrap() {
    identity::ensure_migrated();
    maintenance::migrate_legacy_post_update();
}
