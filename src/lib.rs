//! Shared library for Windows diagnostic / telemetry controls.
//!
//! Layout:
//! - [`telemetry`] — core read/apply API (CLI + GUI)
//! - [`system_links`] — open Event Viewer / Settings / log folders (CLI + GUI)
//! - [`gui`] — egui app (feature `gui` only); thin UI over library APIs
//!
//! Binaries (independent):
//! - `windows-diagnostics` — CLI
//! - `windows-diagnostics-gui` — GUI helper on top of the same API
//!
//! Prefer extending the library (+ CLI) first; the GUI should call those APIs,
//! not reimplement registry/service/task logic.

#![cfg(windows)]

pub mod maintenance;
pub mod system_links;
pub mod telemetry;
mod win_cmd;

#[cfg(feature = "gui")]
pub mod gui;

pub use telemetry::{
    apply, apply_all, ensure_elevated, is_elevated, read_all, read_one, relaunch_elevated,
    relaunch_elevated_with_args, SettingId, SettingState,
};
