//! Open Windows logging / diagnostics tools (shared by CLI + GUI).

use std::path::{Path, PathBuf};

use crate::win_cmd;

/// A built-in link to a Windows logging or diagnostics surface.
#[derive(Debug, Clone, Copy)]
pub struct SystemLink {
    pub id: &'static str,
    pub title: &'static str,
    pub description: &'static str,
    kind: LinkKind,
}

#[derive(Debug, Clone, Copy)]
enum LinkKind {
    /// Snap-in under System32 (e.g. eventvwr.msc).
    Msc(&'static str),
    /// Executable on PATH / System32, optional args (e.g. perfmon.exe /rel).
    Exe {
        name: &'static str,
        args: &'static str,
    },
    /// `ms-settings:…` URI (Windows 10+ Settings).
    SettingsUri(&'static str),
    /// Folder path that may contain `%ENV%` variables.
    Folder(&'static str),
}

pub const ALL: &[SystemLink] = &[
    SystemLink {
        id: "event-viewer",
        title: "Event Viewer",
        description: "System, Application, and Security event logs (eventvwr.msc).",
        kind: LinkKind::Msc("eventvwr.msc"),
    },
    SystemLink {
        id: "event-viewer-wu",
        title: "Windows Update events",
        description: "Event Viewer channel: Microsoft-Windows-WindowsUpdateClient/Operational.",
        kind: LinkKind::Exe {
            name: "eventvwr.exe",
            args: "/c:Microsoft-Windows-WindowsUpdateClient/Operational",
        },
    },
    SystemLink {
        id: "reliability",
        title: "Reliability Monitor",
        description: "Crash / failure timeline (perfmon /rel).",
        kind: LinkKind::Exe {
            name: "perfmon.exe",
            args: "/rel",
        },
    },
    SystemLink {
        id: "perfmon",
        title: "Performance Monitor",
        description: "perfmon.msc — counters and trace sessions.",
        kind: LinkKind::Msc("perfmon.msc"),
    },
    SystemLink {
        id: "task-scheduler",
        title: "Task Scheduler",
        description: "Scheduled tasks including CEIP / post-update automation.",
        kind: LinkKind::Msc("taskschd.msc"),
    },
    SystemLink {
        id: "services",
        title: "Services",
        description: "services.msc — includes DiagTrack and related services.",
        kind: LinkKind::Msc("services.msc"),
    },
    SystemLink {
        id: "privacy-feedback",
        title: "Settings: Diagnostics & feedback",
        description: "ms-settings:privacy-feedback — diagnostic data UI.",
        kind: LinkKind::SettingsUri("ms-settings:privacy-feedback"),
    },
    SystemLink {
        id: "privacy-general",
        title: "Settings: Privacy general",
        description: "ms-settings:privacy-general — advertising ID and related toggles.",
        kind: LinkKind::SettingsUri("ms-settings:privacy-general"),
    },
    SystemLink {
        id: "wu-history",
        title: "Settings: Windows Update history",
        description: "ms-settings:windowsupdate-history.",
        kind: LinkKind::SettingsUri("ms-settings:windowsupdate-history"),
    },
    SystemLink {
        id: "evt-logs-folder",
        title: "Event log files folder",
        description: "Explorer → %SystemRoot%\\System32\\winevt\\Logs (.evtx files).",
        kind: LinkKind::Folder(r"%SystemRoot%\System32\winevt\Logs"),
    },
    SystemLink {
        id: "diagnosis-folder",
        title: "Diagnosis data folder",
        description: "Explorer → %ProgramData%\\Microsoft\\Diagnosis (ETL / telemetry store).",
        kind: LinkKind::Folder(r"%ProgramData%\Microsoft\Diagnosis"),
    },
    SystemLink {
        id: "diagtrack-log",
        title: "DiagTrack ETL folder",
        description: "Explorer → %ProgramData%\\Microsoft\\Diagnosis\\ETLLogs.",
        kind: LinkKind::Folder(r"%ProgramData%\Microsoft\Diagnosis\ETLLogs"),
    },
];

impl SystemLink {
    /// Whether this tool/path looks present on the current machine.
    pub fn is_available(self) -> bool {
        match self.kind {
            LinkKind::Msc(name) => system32_file(name).is_some(),
            LinkKind::Exe { name, .. } => resolve_exe(name).is_some(),
            LinkKind::SettingsUri(_) => settings_app_available(),
            LinkKind::Folder(pattern) => expand_env_path(pattern)
                .map(|p| path_present(&p))
                .unwrap_or(false),
        }
    }

    fn start_target(self) -> String {
        match self.kind {
            LinkKind::Msc(name) => system32_file(name)
                .map(|p| p.to_string_lossy().into_owned())
                .unwrap_or_else(|| name.to_string()),
            LinkKind::Exe { name, args } => {
                let exe = resolve_exe(name)
                    .map(|p| p.to_string_lossy().into_owned())
                    .unwrap_or_else(|| name.to_string());
                if args.is_empty() {
                    exe
                } else {
                    format!("{exe} {args}")
                }
            }
            LinkKind::SettingsUri(uri) => uri.to_string(),
            LinkKind::Folder(pattern) => expand_env_path(pattern)
                .map(|p| p.to_string_lossy().into_owned())
                .unwrap_or_else(|| pattern.to_string()),
        }
    }
}

pub fn find(id: &str) -> Option<&'static SystemLink> {
    let id = id.trim().to_ascii_lowercase();
    ALL.iter().find(|l| l.id == id)
}

/// Snapshot of availability for GUI (compute once, not every frame).
pub fn availability_map() -> Vec<(&'static str, bool)> {
    ALL.iter().map(|l| (l.id, l.is_available())).collect()
}

/// Open a link by id.
pub fn open_id(id: &str) -> Result<String, String> {
    let link = find(id).ok_or_else(|| {
        format!(
            "unknown log link '{id}'. Try: {}",
            ALL.iter()
                .map(|l| l.id)
                .collect::<Vec<_>>()
                .join(", ")
        )
    })?;
    open(link)
}

pub fn open(link: &SystemLink) -> Result<String, String> {
    if !link.is_available() {
        return Err(format!("'{}' is not available on this system", link.title));
    }
    start_detached(&link.start_target())?;
    Ok(format!("Opened: {}", link.title))
}

fn start_detached(target: &str) -> Result<(), String> {
    // Hidden cmd + `start` launches the real UI without a lingering console.
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

fn system32() -> Option<PathBuf> {
    std::env::var_os("SystemRoot").map(|root| PathBuf::from(root).join("System32"))
}

fn system32_file(name: &str) -> Option<PathBuf> {
    let path = system32()?.join(name);
    path.is_file().then_some(path)
}

fn resolve_exe(name: &str) -> Option<PathBuf> {
    if let Some(p) = system32_file(name) {
        return Some(p);
    }
    // Fall back to PATH search without spawning a console.
    let path = std::env::var_os("PATH")?;
    for dir in std::env::split_paths(&path) {
        let candidate = dir.join(name);
        if candidate.is_file() {
            return Some(candidate);
        }
    }
    None
}

fn settings_app_available() -> bool {
    // `ms-settings:` is handled by the shell on Windows 10/11; SystemSettings.exe is often
    // not a plain System32 binary (Store / packaged path), so don't require that file.
    if let Ok(ver) = std::env::var("OS") {
        if ver.eq_ignore_ascii_case("Windows_NT") {
            return windows_10_or_later();
        }
    }
    windows_10_or_later()
}

fn windows_10_or_later() -> bool {
    use winreg::enums::*;
    use winreg::RegKey;
    let hklm = RegKey::predef(HKEY_LOCAL_MACHINE);
    let key = match hklm.open_subkey(r"SOFTWARE\Microsoft\Windows NT\CurrentVersion") {
        Ok(k) => k,
        Err(_) => return true, // assume modern Windows for this app
    };
    if let Ok(major) = key.get_value::<u32, _>("CurrentMajorVersionNumber") {
        return major >= 10;
    }
    if let Ok(build) = key.get_value::<String, _>("CurrentBuild") {
        if let Ok(n) = build.parse::<u32>() {
            return n >= 10240;
        }
    }
    true
}

fn expand_env_path(pattern: &str) -> Option<PathBuf> {
    let mut out = String::new();
    let bytes = pattern.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' {
            if let Some(end) = pattern[i + 1..].find('%') {
                let name = &pattern[i + 1..i + 1 + end];
                let val = std::env::var(name).ok()?;
                out.push_str(&val);
                i += name.len() + 2;
                continue;
            }
        }
        out.push(bytes[i] as char);
        i += 1;
    }
    Some(PathBuf::from(out))
}

fn path_present(path: &Path) -> bool {
    // Permission-denied usually means the path exists but is locked down (e.g. ETLLogs).
    match path.try_exists() {
        Ok(exists) => exists,
        Err(_) => true,
    }
}
