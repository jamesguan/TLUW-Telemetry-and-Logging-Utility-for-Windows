//! Persisted GUI / app preferences (HKCU — survives TEMP clears).

use winreg::enums::*;
use winreg::RegKey;

const REG_PATH: &str = r"Software\WindowsDiagnostics\Prefs";

fn open(write: bool) -> Result<RegKey, String> {
    let hkcu = RegKey::predef(HKEY_CURRENT_USER);
    if write {
        hkcu.create_subkey(REG_PATH)
            .map(|(k, _)| k)
            .map_err(|e| e.to_string())
    } else {
        hkcu.open_subkey(REG_PATH).map_err(|e| e.to_string())
    }
}

/// When true, closing the main window hides to the tray instead of exiting.
pub fn tray_enabled() -> bool {
    open(false)
        .ok()
        .and_then(|k| k.get_value::<u32, _>("TrayEnabled").ok())
        .map(|v| v != 0)
        .unwrap_or(false)
}

pub fn set_tray_enabled(enabled: bool) -> Result<(), String> {
    let key = open(true)?;
    key.set_value("TrayEnabled", &(if enabled { 1u32 } else { 0u32 }))
        .map_err(|e| e.to_string())
}
