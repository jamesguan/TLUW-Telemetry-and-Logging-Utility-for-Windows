//! Persisted GUI / app preferences (HKCU — survives TEMP clears).

use crate::identity;
use winreg::enums::*;
use winreg::RegKey;

fn open(write: bool) -> Result<RegKey, String> {
    let hkcu = RegKey::predef(HKEY_CURRENT_USER);
    if write {
        hkcu.create_subkey(identity::REG_PREFS)
            .map(|(k, _)| k)
            .map_err(|e| e.to_string())
    } else {
        hkcu.open_subkey(identity::REG_PREFS).map_err(|e| e.to_string())
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

/// Appearance preference for the GUI.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ThemePref {
    /// Follow Windows light/dark app mode.
    System,
    Light,
    Dark,
}

impl ThemePref {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::System => "system",
            Self::Light => "light",
            Self::Dark => "dark",
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::System => "System",
            Self::Light => "Light",
            Self::Dark => "Dark",
        }
    }

    pub fn from_str(s: &str) -> Self {
        match s.trim().to_ascii_lowercase().as_str() {
            "light" => Self::Light,
            "dark" => Self::Dark,
            _ => Self::System,
        }
    }

    pub const ALL: [ThemePref; 3] = [ThemePref::System, ThemePref::Light, ThemePref::Dark];
}

pub fn theme_pref() -> ThemePref {
    open(false)
        .ok()
        .and_then(|k| k.get_value::<String, _>("Theme").ok())
        .map(|s| ThemePref::from_str(&s))
        .unwrap_or(ThemePref::System)
}

pub fn set_theme_pref(pref: ThemePref) -> Result<(), String> {
    let key = open(true)?;
    key.set_value("Theme", &pref.as_str().to_string())
        .map_err(|e| e.to_string())
}

/// Windows “Apps use light theme” — `false` means dark mode apps.
pub fn system_apps_dark() -> bool {
    let hkcu = RegKey::predef(HKEY_CURRENT_USER);
    let Ok(key) =
        hkcu.open_subkey(r"Software\Microsoft\Windows\CurrentVersion\Themes\Personalize")
    else {
        return true; // default to dark if unknown
    };
    match key.get_value::<u32, _>("AppsUseLightTheme") {
        Ok(0) => true,
        Ok(_) => false,
        Err(_) => true,
    }
}

pub fn effective_dark(pref: ThemePref) -> bool {
    match pref {
        ThemePref::System => system_apps_dark(),
        ThemePref::Light => false,
        ThemePref::Dark => true,
    }
}
