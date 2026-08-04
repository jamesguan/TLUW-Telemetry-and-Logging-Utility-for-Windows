//! Open Windows logging / diagnostics tools (shared by CLI + GUI).

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
    /// `start "" <target>` (MSC, exe args, ms-settings URI, folder path)
    Start(&'static str),
}

pub const ALL: &[SystemLink] = &[
    SystemLink {
        id: "event-viewer",
        title: "Event Viewer",
        description: "System, Application, and Security event logs (eventvwr.msc).",
        kind: LinkKind::Start("eventvwr.msc"),
    },
    SystemLink {
        id: "event-viewer-wu",
        title: "Windows Update events",
        description: "Event Viewer channel: Microsoft-Windows-WindowsUpdateClient/Operational.",
        kind: LinkKind::Start(
            "eventvwr.exe /c:Microsoft-Windows-WindowsUpdateClient/Operational",
        ),
    },
    SystemLink {
        id: "reliability",
        title: "Reliability Monitor",
        description: "Crash / failure timeline (perfmon /rel).",
        kind: LinkKind::Start("perfmon.exe /rel"),
    },
    SystemLink {
        id: "perfmon",
        title: "Performance Monitor",
        description: "perfmon.msc — counters and trace sessions.",
        kind: LinkKind::Start("perfmon.msc"),
    },
    SystemLink {
        id: "task-scheduler",
        title: "Task Scheduler",
        description: "Scheduled tasks including CEIP / post-update automation.",
        kind: LinkKind::Start("taskschd.msc"),
    },
    SystemLink {
        id: "services",
        title: "Services",
        description: "services.msc — includes DiagTrack and related services.",
        kind: LinkKind::Start("services.msc"),
    },
    SystemLink {
        id: "privacy-feedback",
        title: "Settings: Diagnostics & feedback",
        description: "ms-settings:privacy-feedback — diagnostic data UI.",
        kind: LinkKind::Start("ms-settings:privacy-feedback"),
    },
    SystemLink {
        id: "privacy-general",
        title: "Settings: Privacy general",
        description: "ms-settings:privacy-general — advertising ID and related toggles.",
        kind: LinkKind::Start("ms-settings:privacy-general"),
    },
    SystemLink {
        id: "wu-history",
        title: "Settings: Windows Update history",
        description: "ms-settings:windowsupdate-history.",
        kind: LinkKind::Start("ms-settings:windowsupdate-history"),
    },
    SystemLink {
        id: "evt-logs-folder",
        title: "Event log files folder",
        description: "Explorer → %SystemRoot%\\System32\\winevt\\Logs (.evtx files).",
        kind: LinkKind::Start(r"%SystemRoot%\System32\winevt\Logs"),
    },
    SystemLink {
        id: "diagnosis-folder",
        title: "Diagnosis data folder",
        description: "Explorer → %ProgramData%\\Microsoft\\Diagnosis (ETL / telemetry store).",
        kind: LinkKind::Start(r"%ProgramData%\Microsoft\Diagnosis"),
    },
    SystemLink {
        id: "diagtrack-log",
        title: "DiagTrack ETL folder",
        description: "Explorer → %ProgramData%\\Microsoft\\Diagnosis\\ETLLogs.",
        kind: LinkKind::Start(r"%ProgramData%\Microsoft\Diagnosis\ETLLogs"),
    },
];

pub fn find(id: &str) -> Option<&'static SystemLink> {
    let id = id.trim().to_ascii_lowercase();
    ALL.iter().find(|l| l.id == id)
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
    match link.kind {
        LinkKind::Start(target) => start_detached(target)?,
    }
    Ok(format!("Opened: {}", link.title))
}

fn start_detached(target: &str) -> Result<(), String> {
    // Hidden cmd + `start` launches the real UI without a lingering console.
    // Empty title argument after `start` is required when the target is quoted.
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
