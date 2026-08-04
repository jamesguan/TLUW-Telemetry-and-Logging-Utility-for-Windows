//! Product identity, paths, and one-time migration from the former "Windows Diagnostics" names.
//!
//! Migration is idempotent and only touches keys/folders this app previously owned.

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Once;
use winreg::enums::*;
use winreg::RegKey;

/// Full product name (UI, MSI, PE metadata).
pub const PRODUCT_NAME: &str = "Telemetry and Logging Utility for Windows";

/// Shorter label for tight UI (tray tooltip, quit dialogs).
pub const PRODUCT_NAME_SHORT: &str = "Telemetry Logging Utility";

/// Publisher / copyright holder (company).
pub const COMPANY: &str = "Chillcoders LLC";

/// Primary author.
pub const AUTHOR: &str = "James Guan";

/// CLI binary / clap command name.
pub const CLI_BIN: &str = "tluw";

/// GUI binary name (without `.exe`).
pub const GUI_BIN: &str = "tluw-gui";

/// HKCU / HKLM software key (no `Software\` prefix).
pub const REG_KEY: &str = "TelemetryLoggingUtility";

pub const REG_ROOT: &str = r"Software\TelemetryLoggingUtility";
pub const REG_DISCLAIMER: &str = r"Software\TelemetryLoggingUtility\Disclaimer";
pub const REG_PREFS: &str = r"Software\TelemetryLoggingUtility\Prefs";

/// `%APPDATA%` / `%LOCALAPPDATA%` folder name.
pub const APPDATA_DIR: &str = "TelemetryLoggingUtility";

/// HKCU Run value name.
pub const RUN_VALUE: &str = "TelemetryLoggingUtility";

/// Scheduled task name for post-update lockdown.
pub const TASK_NAME: &str = "TelemetryLoggingUtilityPostUpdate";

// --- Legacy (pre-rename) ---

const LEGACY_REG_KEY: &str = "WindowsDiagnostics";
const LEGACY_REG_ROOT: &str = r"Software\WindowsDiagnostics";
const LEGACY_APPDATA_DIR: &str = "WindowsDiagnostics";
const LEGACY_RUN_VALUE: &str = "WindowsDiagnostics";

/// Former scheduled task name (cleanup lives in [`crate::maintenance`]).
pub const LEGACY_TASK_NAME: &str = "WindowsDiagnosticsPostUpdate";

static MIGRATE: Once = Once::new();

/// Run legacy → current migration once per process (best-effort).
pub fn ensure_migrated() {
    MIGRATE.call_once(|| {
        let _ = migrate_all();
    });
}

fn migrate_all() -> Result<(), String> {
    migrate_hkcu_tree()?;
    migrate_hklm_tree();
    migrate_appdata();
    migrate_run_key()?;
    Ok(())
}

fn looks_like_our_legacy_key(key: &RegKey) -> bool {
    // Only delete if the key looks like ours (avoid nuking unrelated Software\WindowsDiagnostics).
    const MARKERS: &[&str] = &[
        "Disclaimer",
        "Prefs",
        "StartMenuShortcut",
        "DesktopShortcut",
        "StartupShortcut",
        "Installed",
        "PostUpdateEnabled",
        "TrayEnabled",
    ];
    if key.enum_keys().filter_map(|k| k.ok()).any(|n| {
        n.eq_ignore_ascii_case("Disclaimer") || n.eq_ignore_ascii_case("Prefs")
    }) {
        return true;
    }
    for name in MARKERS {
        if key.get_value::<u32, _>(name).is_ok() || key.get_value::<String, _>(name).is_ok() {
            return true;
        }
    }
    // Empty leftover from uninstall still safe to remove if the key name matches our legacy brand.
    key.enum_keys().filter_map(|k| k.ok()).next().is_none()
        && key.enum_values().filter_map(|v| v.ok()).next().is_none()
}

fn copy_value(src: &RegKey, dst: &RegKey, name: &str) {
    if dst.get_raw_value(name).is_ok() {
        return;
    }
    if let Ok(raw) = src.get_raw_value(name) {
        let _ = dst.set_raw_value(name, &raw);
    }
}

fn copy_key_values(src: &RegKey, dst: &RegKey) {
    for item in src.enum_values().filter_map(|v| v.ok()) {
        let (name, _) = item;
        copy_value(src, dst, &name);
    }
}

fn migrate_subkey(hkcu: &RegKey, sub: &str) -> Result<(), String> {
    let legacy_path = format!("{LEGACY_REG_ROOT}\\{sub}");
    let new_path = format!("{REG_ROOT}\\{sub}");
    let Ok(src) = hkcu.open_subkey(&legacy_path) else {
        return Ok(());
    };
    let (dst, _) = hkcu
        .create_subkey(&new_path)
        .map_err(|e| format!("create {new_path}: {e}"))?;
    copy_key_values(&src, &dst);
    Ok(())
}

fn migrate_hkcu_tree() -> Result<(), String> {
    let hkcu = RegKey::predef(HKEY_CURRENT_USER);
    let Ok(legacy) = hkcu.open_subkey(LEGACY_REG_ROOT) else {
        return Ok(());
    };
    if !looks_like_our_legacy_key(&legacy) {
        return Ok(());
    }

    // Root-level MSI marker values (shortcuts, etc.).
    let (dst_root, _) = hkcu
        .create_subkey(REG_ROOT)
        .map_err(|e| format!("create {REG_ROOT}: {e}"))?;
    copy_key_values(&legacy, &dst_root);

    migrate_subkey(&hkcu, "Disclaimer")?;
    migrate_subkey(&hkcu, "Prefs")?;

    // Remove legacy tree only after copy succeeded.
    let software = hkcu
        .open_subkey_with_flags("Software", KEY_WRITE)
        .map_err(|e| format!("open Software: {e}"))?;
    let _ = software.delete_subkey_all(LEGACY_REG_KEY);
    Ok(())
}

fn migrate_hklm_tree() {
    // Best-effort (needs admin). Safe to ignore failures.
    let hklm = RegKey::predef(HKEY_LOCAL_MACHINE);
    let Ok(legacy) = hklm.open_subkey(LEGACY_REG_ROOT) else {
        return;
    };
    if !looks_like_our_legacy_key(&legacy) {
        return;
    }
    if let Ok((dst, _)) = hklm.create_subkey(REG_ROOT) {
        copy_key_values(&legacy, &dst);
    }
    if let Ok(software) = hklm.open_subkey_with_flags("Software", KEY_WRITE) {
        let _ = software.delete_subkey_all(LEGACY_REG_KEY);
    }
}

fn roaming_legacy() -> Option<PathBuf> {
    Some(PathBuf::from(std::env::var_os("APPDATA")?).join(LEGACY_APPDATA_DIR))
}

fn roaming_new() -> Option<PathBuf> {
    Some(PathBuf::from(std::env::var_os("APPDATA")?).join(APPDATA_DIR))
}

fn local_legacy() -> Option<PathBuf> {
    Some(PathBuf::from(std::env::var_os("LOCALAPPDATA")?).join(LEGACY_APPDATA_DIR))
}

fn local_new() -> Option<PathBuf> {
    Some(PathBuf::from(std::env::var_os("LOCALAPPDATA")?).join(APPDATA_DIR))
}

fn migrate_dir(from: &Path, to: &Path) {
    if !from.is_dir() {
        return;
    }
    let _ = fs::create_dir_all(to);
    if let Ok(entries) = fs::read_dir(from) {
        for entry in entries.flatten() {
            let src = entry.path();
            let dst = to.join(entry.file_name());
            if dst.exists() {
                continue;
            }
            if fs::rename(&src, &dst).is_err() {
                if src.is_file() {
                    let _ = fs::copy(&src, &dst);
                    let _ = fs::remove_file(&src);
                }
            }
        }
    }
    // Remove legacy dir if empty (or wipe leftovers after copy).
    if let Ok(mut rd) = fs::read_dir(from) {
        if rd.next().is_none() {
            let _ = fs::remove_dir(from);
        } else {
            // Non-empty leftovers: try recursive remove of known app files only.
            for name in [
                "disclaimer_accepted",
                "disclaimer_acceptance.log",
                "cleanup_history.log",
            ] {
                let p = from.join(name);
                let _ = fs::remove_file(p);
            }
            if let Ok(mut rd2) = fs::read_dir(from) {
                if rd2.next().is_none() {
                    let _ = fs::remove_dir(from);
                }
            }
        }
    }
}

fn migrate_appdata() {
    if let (Some(from), Some(to)) = (roaming_legacy(), roaming_new()) {
        migrate_dir(&from, &to);
    }
    if let (Some(from), Some(to)) = (local_legacy(), local_new()) {
        migrate_dir(&from, &to);
    }
}

fn migrate_run_key() -> Result<(), String> {
    let hkcu = RegKey::predef(HKEY_CURRENT_USER);
    let Ok(key) = hkcu.open_subkey_with_flags(
        r"Software\Microsoft\Windows\CurrentVersion\Run",
        KEY_READ | KEY_WRITE,
    ) else {
        return Ok(());
    };

    let legacy: Option<String> = key.get_value(LEGACY_RUN_VALUE).ok();
    let current: Option<String> = key.get_value(RUN_VALUE).ok();

    match (legacy, current) {
        (Some(val), None) => {
            // Point at new GUI name if the old path still references the old exe.
            let updated = val
                .replace("tluw-gui.exe", "tluw-gui.exe")
                .replace("tluw.exe", "tluw.exe");
            let _ = key.set_value(RUN_VALUE, &updated);
            let _ = key.delete_value(LEGACY_RUN_VALUE);
        }
        (Some(_), Some(_)) => {
            let _ = key.delete_value(LEGACY_RUN_VALUE);
        }
        _ => {}
    }
    Ok(())
}

/// Roaming config directory for this product.
pub fn roaming_dir() -> Option<PathBuf> {
    Some(PathBuf::from(std::env::var_os("APPDATA")?).join(APPDATA_DIR))
}
