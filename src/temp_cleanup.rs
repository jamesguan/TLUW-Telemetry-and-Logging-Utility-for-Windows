//! Clear Windows temporary folders (library API for CLI + GUI).

use std::fs;
use std::path::{Path, PathBuf};

use crate::log_cleanup::format_bytes;

#[derive(Debug, Clone, Copy)]
pub struct TempTarget {
    pub id: &'static str,
    pub title: &'static str,
    pub description: &'static str,
    /// Needs Administrator (e.g. Windows\\Temp, Prefetch).
    pub needs_admin: bool,
    kind: TempKind,
}

#[derive(Debug, Clone, Copy)]
enum TempKind {
    EnvVar(&'static str),
    SystemRelative(&'static str),
    LocalAppDataRelative(&'static str),
    Recent,
}

#[derive(Debug, Clone, Default)]
pub struct TempStatus {
    pub files: u64,
    pub bytes: u64,
    pub location: String,
}

impl TempStatus {
    pub fn summary_line(&self) -> String {
        format!(
            "{} — {} file(s), {}",
            self.location,
            self.files,
            format_bytes(self.bytes)
        )
    }
}

#[derive(Debug, Clone, Default)]
pub struct TempClearResult {
    pub removed_files: u64,
    pub freed_bytes: u64,
    pub skipped: u64,
    pub before: TempStatus,
    pub message: String,
}

impl TempClearResult {
    pub fn summary_line(&self) -> String {
        format!(
            "{} — cleared {} file(s)/item(s), freed {}, skipped {}",
            self.message,
            self.removed_files,
            format_bytes(self.freed_bytes),
            self.skipped
        )
    }
}

pub const ALL: &[TempTarget] = &[
    // Note: never add whole %LOCALAPPDATA% or %APPDATA% — disclaimer acceptance
    // lives in HKCU + %APPDATA%\WindowsDiagnostics and must survive clears.
    TempTarget {
        id: "user-temp",
        title: "User TEMP folder",
        description: "%TEMP% — current user temporary files.",
        needs_admin: false,
        kind: TempKind::EnvVar("TEMP"),
    },
    TempTarget {
        id: "user-tmp",
        title: "User TMP folder",
        description: "%TMP% (hidden if same as TEMP).",
        needs_admin: false,
        kind: TempKind::EnvVar("TMP"),
    },
    TempTarget {
        id: "local-temp",
        title: "LocalAppData\\Temp",
        description: "%LOCALAPPDATA%\\Temp (hidden if same as TEMP).",
        needs_admin: false,
        kind: TempKind::LocalAppDataRelative("Temp"),
    },
    TempTarget {
        id: "windows-temp",
        title: "Windows\\Temp",
        description: "%SystemRoot%\\Temp — system temporary files.",
        needs_admin: true,
        kind: TempKind::SystemRelative("Temp"),
    },
    TempTarget {
        id: "prefetch",
        title: "Prefetch",
        description: "%SystemRoot%\\Prefetch — app launch traces (rebuilds after clear).",
        needs_admin: true,
        kind: TempKind::SystemRelative("Prefetch"),
    },
    TempTarget {
        id: "recent",
        title: "Recent items",
        description: "%APPDATA%\\Microsoft\\Windows\\Recent shortcuts.",
        needs_admin: false,
        kind: TempKind::Recent,
    },
];

impl TempTarget {
    pub fn find(id: &str) -> Option<&'static TempTarget> {
        let id = id.trim().to_ascii_lowercase();
        ALL.iter().find(|t| t.id == id)
    }

    pub fn resolve_path(self) -> Option<PathBuf> {
        match self.kind {
            TempKind::Recent => std::env::var_os("APPDATA")
                .map(|p| PathBuf::from(p).join(r"Microsoft\Windows\Recent")),
            TempKind::EnvVar(name) => std::env::var_os(name).map(PathBuf::from),
            TempKind::SystemRelative(rel) => {
                std::env::var_os("SystemRoot").map(|p| PathBuf::from(p).join(rel))
            }
            TempKind::LocalAppDataRelative(rel) => {
                std::env::var_os("LOCALAPPDATA").map(|p| PathBuf::from(p).join(rel))
            }
        }
    }

    pub fn is_available(self) -> bool {
        let Some(path) = self.resolve_path() else {
            return false;
        };
        if matches!(self.id, "user-tmp" | "local-temp") {
            if let (Ok(temp), Some(p)) = (std::env::var("TEMP"), self.resolve_path()) {
                if same_path(&PathBuf::from(temp), &p) {
                    return false;
                }
            }
        }
        path_present(&path)
    }
}

pub fn availability_map() -> Vec<(&'static str, bool)> {
    ALL.iter().map(|t| (t.id, t.is_available())).collect()
}

pub fn inspect(target: &TempTarget) -> TempStatus {
    let location = target
        .resolve_path()
        .map(|p| p.to_string_lossy().into_owned())
        .unwrap_or_else(|| target.id.to_string());
    let (files, bytes) = target
        .resolve_path()
        .map(|p| dir_stats(&p, true))
        .unwrap_or((0, 0));
    TempStatus {
        files,
        bytes,
        location,
    }
}

pub fn inspect_all() -> Vec<(&'static TempTarget, TempStatus)> {
    ALL.iter()
        .filter(|t| t.is_available())
        .map(|t| (t, inspect(t)))
        .collect()
}

pub fn open_location(target: &TempTarget) -> Result<String, String> {
    let path = target
        .resolve_path()
        .filter(|p| path_present(p))
        .ok_or_else(|| "temp location not found".to_string())?;
    start_detached(&path.to_string_lossy())?;
    Ok(format!("Opened {}", path.display()))
}

pub fn clear(target: &TempTarget) -> Result<TempClearResult, String> {
    if !target.is_available() {
        return Err(format!("'{}' is not available", target.title));
    }
    let before = inspect(target);
    let path = target
        .resolve_path()
        .ok_or_else(|| "path missing".to_string())?;
    let cleared = clear_directory_contents(&path, true)?;
    let result = TempClearResult {
        removed_files: cleared.removed,
        freed_bytes: cleared.bytes,
        skipped: cleared.skipped,
        before,
        message: format!("{} cleared", target.title),
    };
    crate::cleanup_history::record_clear(
        crate::cleanup_history::Category::Temp,
        result.freed_bytes,
        result.removed_files,
    );
    Ok(result)
}

pub fn clear_all() -> Vec<(String, Result<TempClearResult, String>)> {
    ALL.iter()
        .filter(|t| t.is_available())
        .map(|t| (t.id.to_string(), clear(t)))
        .collect()
}

struct Cleared {
    removed: u64,
    bytes: u64,
    skipped: u64,
}

fn same_path(a: &Path, b: &Path) -> bool {
    let a = a.canonicalize().unwrap_or_else(|_| a.to_path_buf());
    let b = b.canonicalize().unwrap_or_else(|_| b.to_path_buf());
    a == b
}

fn path_present(path: &Path) -> bool {
    match path.try_exists() {
        Ok(exists) => exists,
        Err(_) => true,
    }
}

fn dir_stats(dir: &Path, recursive: bool) -> (u64, u64) {
    let mut files = 0u64;
    let mut bytes = 0u64;
    let Ok(walker) = fs::read_dir(dir) else {
        return (0, 0);
    };
    for entry in walker.flatten() {
        let path = entry.path();
        if path.is_dir() {
            if recursive {
                let (f, b) = dir_stats(&path, true);
                files += f;
                bytes += b;
            }
        } else if let Ok(meta) = path.metadata() {
            files += 1;
            bytes += meta.len();
        }
    }
    (files, bytes)
}

fn clear_directory_contents(dir: &Path, recursive: bool) -> Result<Cleared, String> {
    if !dir.exists() {
        return Ok(Cleared {
            removed: 0,
            bytes: 0,
            skipped: 0,
        });
    }
    let entries = fs::read_dir(dir).map_err(|e| format!("cannot list {}: {e}", dir.display()))?;
    let mut removed = 0u64;
    let mut bytes = 0u64;
    let mut skipped = 0u64;
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            if recursive {
                let (_, b) = dir_stats(&path, true);
                match fs::remove_dir_all(&path) {
                    Ok(()) => {
                        removed += 1;
                        bytes += b;
                    }
                    Err(_) => skipped += 1,
                }
            } else {
                skipped += 1;
            }
        } else {
            let size = path.metadata().map(|m| m.len()).unwrap_or(0);
            match fs::remove_file(&path) {
                Ok(()) => {
                    removed += 1;
                    bytes += size;
                }
                Err(_) => skipped += 1,
            }
        }
    }
    Ok(Cleared {
        removed,
        bytes,
        skipped,
    })
}

fn start_detached(target: &str) -> Result<(), String> {
    let status = crate::win_cmd::command("cmd.exe")
        .args(["/C", "start", "", "/B", target])
        .status()
        .map_err(|e| e.to_string())?;
    if status.success() {
        Ok(())
    } else {
        Err(format!("failed to open '{target}'"))
    }
}
