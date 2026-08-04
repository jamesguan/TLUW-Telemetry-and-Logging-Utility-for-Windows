//! Destructive log / diagnostic cleanup actions (library API for CLI + GUI).
//!
//! Workflow intended for UI:
//! 1. [`inspect`] — count files / bytes (or event-log records)
//! 2. [`open_location`] — open the folder or Event Viewer so the user can look
//! 3. [`clear`] — wipe and report how many items / bytes were freed

use std::fs;
use std::path::{Path, PathBuf};

use crate::win_cmd;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ClearKind {
    DiagnosisFolder,
    DiagnosisEtl,
    Wer,
    EventLog(&'static str),
    CbsLogs,
    DismLogs,
    WindowsUpdateLogs,
    DeliveryOptimization,
}

#[derive(Debug, Clone, Copy)]
pub struct ClearAction {
    pub id: &'static str,
    pub title: &'static str,
    pub description: &'static str,
    pub dangerous: bool,
    kind: ClearKind,
}

/// Snapshot of how much log data exists before a clear.
#[derive(Debug, Clone, Default)]
pub struct LogStatus {
    pub files: u64,
    pub bytes: u64,
    /// Human path or event channel name.
    pub location: String,
    pub note: String,
}

impl LogStatus {
    pub fn summary_line(&self) -> String {
        if self.files == 0 && self.bytes == 0 && self.note.is_empty() {
            return format!("{} — empty / unavailable", self.location);
        }
        let extra = if self.note.is_empty() {
            String::new()
        } else {
            format!(" ({})", self.note)
        };
        format!(
            "{} — {} file(s), {}{}",
            self.location,
            self.files,
            format_bytes(self.bytes),
            extra
        )
    }
}

/// Result of a clear operation.
#[derive(Debug, Clone, Default)]
pub struct ClearResult {
    pub removed_files: u64,
    pub freed_bytes: u64,
    pub skipped: u64,
    pub before: LogStatus,
    pub message: String,
}

impl ClearResult {
    pub fn summary_line(&self) -> String {
        format!(
            "{} — cleared {} item(s), freed {}, skipped {}",
            self.message,
            self.removed_files,
            format_bytes(self.freed_bytes),
            self.skipped
        )
    }
}

pub const ALL: &[ClearAction] = &[
    ClearAction {
        id: "diagnosis",
        title: "Microsoft Diagnosis folder",
        description: "%ProgramData%\\Microsoft\\Diagnosis — telemetry ETL store.",
        dangerous: true,
        kind: ClearKind::DiagnosisFolder,
    },
    ClearAction {
        id: "diagnosis-etl",
        title: "Diagnosis ETLLogs",
        description: "%ProgramData%\\Microsoft\\Diagnosis\\ETLLogs only.",
        dangerous: true,
        kind: ClearKind::DiagnosisEtl,
    },
    ClearAction {
        id: "wer",
        title: "Windows Error Reporting",
        description: "WER ReportQueue / Archive / Temp folders.",
        dangerous: false,
        kind: ClearKind::Wer,
    },
    ClearAction {
        id: "event-application",
        title: "Application event log",
        description: "wevtutil channel Application.",
        dangerous: false,
        kind: ClearKind::EventLog("Application"),
    },
    ClearAction {
        id: "event-system",
        title: "System event log",
        description: "wevtutil channel System.",
        dangerous: false,
        kind: ClearKind::EventLog("System"),
    },
    ClearAction {
        id: "event-setup",
        title: "Setup event log",
        description: "wevtutil channel Setup.",
        dangerous: false,
        kind: ClearKind::EventLog("Setup"),
    },
    ClearAction {
        id: "event-security",
        title: "Security event log",
        description: "wevtutil channel Security — erases audit trail.",
        dangerous: true,
        kind: ClearKind::EventLog("Security"),
    },
    ClearAction {
        id: "event-forwarded",
        title: "Forwarded Events log",
        description: "wevtutil channel ForwardedEvents.",
        dangerous: false,
        kind: ClearKind::EventLog("ForwardedEvents"),
    },
    ClearAction {
        id: "event-diag-perf",
        title: "Diagnostics-Performance log",
        description: "Microsoft-Windows-Diagnostics-Performance/Operational.",
        dangerous: false,
        kind: ClearKind::EventLog("Microsoft-Windows-Diagnostics-Performance/Operational"),
    },
    ClearAction {
        id: "event-diagtrack",
        title: "DiagTrack / UTC event log",
        description: "Microsoft-Windows-UniversalTelemetryClient/Operational.",
        dangerous: false,
        kind: ClearKind::EventLog("Microsoft-Windows-UniversalTelemetryClient/Operational"),
    },
    ClearAction {
        id: "cbs",
        title: "CBS component logs",
        description: "%SystemRoot%\\Logs\\CBS.",
        dangerous: false,
        kind: ClearKind::CbsLogs,
    },
    ClearAction {
        id: "dism",
        title: "DISM logs",
        description: "%SystemRoot%\\Logs\\DISM.",
        dangerous: false,
        kind: ClearKind::DismLogs,
    },
    ClearAction {
        id: "windowsupdate",
        title: "Windows Update log folder",
        description: "%SystemRoot%\\Logs\\WindowsUpdate.",
        dangerous: false,
        kind: ClearKind::WindowsUpdateLogs,
    },
    ClearAction {
        id: "delivery-optimization",
        title: "Delivery Optimization logs",
        description: "%ProgramData%\\Microsoft\\Windows\\DeliveryOptimization\\Logs.",
        dangerous: false,
        kind: ClearKind::DeliveryOptimization,
    },
];

impl ClearAction {
    pub fn find(id: &str) -> Option<&'static ClearAction> {
        let id = id.trim().to_ascii_lowercase();
        ALL.iter().find(|a| a.id == id)
    }

    pub fn is_available(self) -> bool {
        match self.kind {
            ClearKind::DiagnosisFolder => programdata_path(r"Microsoft\Diagnosis")
                .map(|p| path_present(&p))
                .unwrap_or(false),
            ClearKind::DiagnosisEtl => programdata_path(r"Microsoft\Diagnosis\ETLLogs")
                .map(|p| path_present(&p))
                .unwrap_or(false),
            ClearKind::Wer => true,
            ClearKind::EventLog(name) => event_log_exists(name),
            ClearKind::CbsLogs => system_root_path(r"Logs\CBS")
                .map(|p| path_present(&p))
                .unwrap_or(false),
            ClearKind::DismLogs => system_root_path(r"Logs\DISM")
                .map(|p| path_present(&p))
                .unwrap_or(false),
            ClearKind::WindowsUpdateLogs => system_root_path(r"Logs\WindowsUpdate")
                .map(|p| path_present(&p))
                .unwrap_or(false),
            ClearKind::DeliveryOptimization => {
                programdata_path(r"Microsoft\Windows\DeliveryOptimization\Logs")
                    .map(|p| path_present(&p))
                    .unwrap_or(false)
            }
        }
    }

    fn paths(self) -> Vec<PathBuf> {
        match self.kind {
            ClearKind::DiagnosisFolder => programdata_path(r"Microsoft\Diagnosis")
                .into_iter()
                .collect(),
            ClearKind::DiagnosisEtl => programdata_path(r"Microsoft\Diagnosis\ETLLogs")
                .into_iter()
                .collect(),
            ClearKind::Wer => wer_paths(),
            ClearKind::EventLog(_) => Vec::new(),
            ClearKind::CbsLogs => system_root_path(r"Logs\CBS").into_iter().collect(),
            ClearKind::DismLogs => system_root_path(r"Logs\DISM").into_iter().collect(),
            ClearKind::WindowsUpdateLogs => system_root_path(r"Logs\WindowsUpdate")
                .into_iter()
                .collect(),
            ClearKind::DeliveryOptimization => {
                programdata_path(r"Microsoft\Windows\DeliveryOptimization\Logs")
                    .into_iter()
                    .collect()
            }
        }
    }
}

pub fn availability_map() -> Vec<(&'static str, bool)> {
    ALL.iter().map(|a| (a.id, a.is_available())).collect()
}

pub fn format_bytes(bytes: u64) -> String {
    const KB: f64 = 1024.0;
    const MB: f64 = KB * 1024.0;
    const GB: f64 = MB * 1024.0;
    let b = bytes as f64;
    if b >= GB {
        format!("{:.2} GB", b / GB)
    } else if b >= MB {
        format!("{:.2} MB", b / MB)
    } else if b >= KB {
        format!("{:.1} KB", b / KB)
    } else {
        format!("{bytes} B")
    }
}

/// Count files / bytes (or event records) for a target.
pub fn inspect(action: &ClearAction) -> LogStatus {
    match action.kind {
        ClearKind::EventLog(name) => inspect_event_log(name),
        _ => {
            let paths = action.paths();
            let mut files = 0u64;
            let mut bytes = 0u64;
            for p in &paths {
                let (f, b) = dir_stats(p, true);
                files += f;
                bytes += b;
            }
            let location = paths
                .first()
                .map(|p| p.to_string_lossy().into_owned())
                .unwrap_or_else(|| action.id.to_string());
            LogStatus {
                files,
                bytes,
                location,
                note: if paths.len() > 1 {
                    format!("{} folders", paths.len())
                } else {
                    String::new()
                },
            }
        }
    }
}

/// Open Explorer / Event Viewer for this target so the user can see the logs.
pub fn open_location(action: &ClearAction) -> Result<String, String> {
    match action.kind {
        ClearKind::EventLog(name) => {
            // Open Event Viewer; channel focus when possible.
            let target = if name.contains('/') {
                format!("eventvwr.exe /c:{name}")
            } else {
                "eventvwr.msc".to_string()
            };
            start_detached(&target)?;
            Ok(format!("Opened Event Viewer for {name}"))
        }
        _ => {
            let paths = action.paths();
            let path = paths
                .into_iter()
                .find(|p| path_present(p))
                .ok_or_else(|| "log location not found".to_string())?;
            // Prefer opening the folder itself in Explorer.
            start_detached(&path.to_string_lossy())?;
            Ok(format!("Opened {}", path.display()))
        }
    }
}

/// Wipe the target and report how much was cleared.
pub fn clear(action: &ClearAction) -> Result<ClearResult, String> {
    if !action.is_available() {
        return Err(format!("'{}' is not available on this system", action.title));
    }
    let before = inspect(action);

    let result = match action.kind {
        ClearKind::DiagnosisFolder | ClearKind::DiagnosisEtl => {
            stop_diagtrack_best_effort();
            let path = action
                .paths()
                .into_iter()
                .next()
                .ok_or_else(|| "path missing".to_string())?;
            let cleared = clear_directory_contents(&path, true)?;
            ClearResult {
                removed_files: cleared.removed,
                freed_bytes: cleared.bytes,
                skipped: cleared.skipped,
                before: before.clone(),
                message: format!("{} wiped", action.title),
            }
        }
        ClearKind::Wer => {
            let mut removed = 0u64;
            let mut bytes = 0u64;
            let mut skipped = 0u64;
            for p in wer_paths() {
                if !path_present(&p) {
                    continue;
                }
                let c = clear_directory_contents(&p, true)?;
                removed += c.removed;
                bytes += c.bytes;
                skipped += c.skipped;
            }
            ClearResult {
                removed_files: removed,
                freed_bytes: bytes,
                skipped,
                before: before.clone(),
                message: "WER queues cleared".into(),
            }
        }
        ClearKind::EventLog(name) => {
            clear_event_log(name)?;
            ClearResult {
                removed_files: before.files,
                freed_bytes: before.bytes,
                skipped: 0,
                before: before.clone(),
                message: format!("Event log cleared: {name}"),
            }
        }
        ClearKind::CbsLogs => {
            let path = system_root_path(r"Logs\CBS")
                .ok_or_else(|| "SystemRoot not set".to_string())?;
            // Non-recursive: only top-level files (CBS often locks subdirs less relevant)
            let c = clear_directory_contents(&path, false)?;
            ClearResult {
                removed_files: c.removed,
                freed_bytes: c.bytes,
                skipped: c.skipped,
                before: before.clone(),
                message: "CBS logs cleared".into(),
            }
        }
        ClearKind::DismLogs
        | ClearKind::WindowsUpdateLogs
        | ClearKind::DeliveryOptimization => {
            let path = action
                .paths()
                .into_iter()
                .next()
                .ok_or_else(|| "path missing".to_string())?;
            let c = clear_directory_contents(&path, true)?;
            ClearResult {
                removed_files: c.removed,
                freed_bytes: c.bytes,
                skipped: c.skipped,
                before: before.clone(),
                message: format!("{} cleared", action.title),
            }
        }
    };

    crate::cleanup_history::record_clear(
        crate::cleanup_history::Category::Logs,
        result.freed_bytes,
        result.removed_files,
    );
    Ok(result)
}

pub fn clear_all(include_dangerous: bool) -> Vec<(String, Result<ClearResult, String>)> {
    ALL.iter()
        .filter(|a| a.is_available())
        .filter(|a| include_dangerous || !a.dangerous)
        .map(|a| (a.id.to_string(), clear(a)))
        .collect()
}

/// Inspect every available target (for Logging status panel).
pub fn inspect_all() -> Vec<(&'static ClearAction, LogStatus)> {
    ALL.iter()
        .filter(|a| a.is_available())
        .map(|a| (a, inspect(a)))
        .collect()
}

struct Cleared {
    removed: u64,
    bytes: u64,
    skipped: u64,
}

fn stop_diagtrack_best_effort() {
    let _ = win_cmd::command("sc.exe")
        .args(["stop", "DiagTrack"])
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status();
}

fn clear_event_log(name: &str) -> Result<(), String> {
    let out = win_cmd::command("wevtutil.exe")
        .args(["cl", name])
        .output()
        .map_err(|e| e.to_string())?;
    if out.status.success() {
        Ok(())
    } else {
        let msg = format!(
            "{}{}",
            String::from_utf8_lossy(&out.stderr).trim(),
            String::from_utf8_lossy(&out.stdout).trim()
        );
        Err(if msg.is_empty() {
            format!("wevtutil failed for {name}")
        } else {
            msg
        })
    }
}

fn event_log_exists(name: &str) -> bool {
    win_cmd::command("wevtutil.exe")
        .args(["gl", name])
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}

fn inspect_event_log(name: &str) -> LogStatus {
    let out = win_cmd::command("wevtutil.exe")
        .args(["gli", name])
        .output()
        .ok();
    let text = out
        .map(|o| String::from_utf8_lossy(&o.stdout).into_owned())
        .unwrap_or_default();
    let records = parse_wevtutil_u64(&text, "numberOfLogRecords").unwrap_or(0);
    let file_size = parse_wevtutil_u64(&text, "fileSize").unwrap_or(0);
    LogStatus {
        files: records,
        bytes: file_size,
        location: format!("Event log: {name}"),
        note: "records".into(),
    }
}

fn parse_wevtutil_u64(text: &str, key: &str) -> Option<u64> {
    for line in text.lines() {
        let line = line.trim();
        if let Some(rest) = line
            .strip_prefix(key)
            .or_else(|| line.strip_prefix(&format!("{key}:")))
        {
            let digits: String = rest
                .chars()
                .skip_while(|c| !c.is_ascii_digit())
                .take_while(|c| c.is_ascii_digit())
                .collect();
            if !digits.is_empty() {
                return digits.parse().ok();
            }
        }
        // "numberOfLogRecords: 1234" style
        if line.to_ascii_lowercase().starts_with(&key.to_ascii_lowercase()) {
            let digits: String = line
                .chars()
                .skip_while(|c| !c.is_ascii_digit())
                .take_while(|c| c.is_ascii_digit())
                .collect();
            if !digits.is_empty() {
                return digits.parse().ok();
            }
        }
    }
    None
}

fn wer_paths() -> Vec<PathBuf> {
    let mut paths = Vec::new();
    if let Some(p) = programdata_path(r"Microsoft\Windows\WER\ReportQueue") {
        paths.push(p);
    }
    if let Some(p) = programdata_path(r"Microsoft\Windows\WER\ReportArchive") {
        paths.push(p);
    }
    if let Some(p) = programdata_path(r"Microsoft\Windows\WER\Temp") {
        paths.push(p);
    }
    if let Ok(local) = std::env::var("LOCALAPPDATA") {
        paths.push(PathBuf::from(&local).join(r"Microsoft\Windows\WER\ReportQueue"));
        paths.push(PathBuf::from(&local).join(r"Microsoft\Windows\WER\ReportArchive"));
        paths.push(PathBuf::from(&local).join(r"Microsoft\Windows\WER\Temp"));
    }
    paths
}

fn dir_stats(dir: &Path, recursive: bool) -> (u64, u64) {
    let mut files = 0u64;
    let mut bytes = 0u64;
    let walker = match fs::read_dir(dir) {
        Ok(w) => w,
        Err(_) => return (0, 0),
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
    let entries = match fs::read_dir(dir) {
        Ok(e) => e,
        Err(e) => return Err(format!("cannot list {}: {e}", dir.display())),
    };
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
    let status = win_cmd::command("cmd.exe")
        .args(["/C", "start", "", "/B", target])
        .status()
        .map_err(|e| e.to_string())?;
    if status.success() {
        Ok(())
    } else {
        Err(format!("failed to open '{target}'"))
    }
}

fn programdata_path(relative: &str) -> Option<PathBuf> {
    std::env::var_os("ProgramData").map(|p| PathBuf::from(p).join(relative))
}

fn system_root_path(relative: &str) -> Option<PathBuf> {
    std::env::var_os("SystemRoot").map(|p| PathBuf::from(p).join(relative))
}

fn path_present(path: &Path) -> bool {
    match path.try_exists() {
        Ok(exists) => exists,
        Err(_) => true,
    }
}
