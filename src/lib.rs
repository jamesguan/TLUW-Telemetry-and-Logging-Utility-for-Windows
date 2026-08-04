//! Shared library for Windows diagnostic / telemetry controls.
//!
//! Layout:
//! - [`telemetry`] — core read/apply API (CLI + GUI)
//! - [`gui`] — egui app (feature `gui` only)
//!
//! Binaries:
//! - `windows-diagnostics` — CLI
//! - `windows-diagnostics-gui` — GUI helper on top of the same API

#![cfg(windows)]

pub mod telemetry;

#[cfg(feature = "gui")]
pub mod gui;

pub use telemetry::{
    apply, apply_all, ensure_elevated, is_elevated, read_all, read_one, relaunch_elevated,
    relaunch_elevated_with_args, SettingId, SettingState,
};
