//! Read and write Windows diagnostic / telemetry settings.

use std::process::Stdio;
use winreg::enums::*;
use winreg::RegKey;

use crate::win_cmd;

/// Stable id for each controllable setting.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum SettingId {
    DiagnosticData,
    DiagTrack,
    CeipTasks,
    AdvertisingId,
    TailoredExperiences,
    CeipPolicy,
    AppInventory,
}

impl SettingId {
    pub const ALL: [SettingId; 7] = [
        SettingId::DiagnosticData,
        SettingId::DiagTrack,
        SettingId::CeipTasks,
        SettingId::AdvertisingId,
        SettingId::TailoredExperiences,
        SettingId::CeipPolicy,
        SettingId::AppInventory,
    ];

    pub fn title(self) -> &'static str {
        match self {
            Self::DiagnosticData => "Diagnostic data (AllowTelemetry)",
            Self::DiagTrack => "Connected User Experiences (DiagTrack)",
            Self::CeipTasks => "CEIP / feedback scheduled tasks",
            Self::AdvertisingId => "Advertising ID",
            Self::TailoredExperiences => "Tailored experiences",
            Self::CeipPolicy => "Customer Experience Improvement (CEIP)",
            Self::AppInventory => "App inventory & compatibility telemetry",
        }
    }

    pub fn explanation(self) -> &'static str {
        match self {
            Self::DiagnosticData => {
                "Controls how much diagnostic data Windows may send to Microsoft. \
                 Off sets policy to Security-only (0) — the strongest lock on Pro. \
                 On sets Required/Basic (1). Optional/Full (3) is not restored by this app."
            }
            Self::DiagTrack => {
                "The DiagTrack service hosts Connected User Experiences and Telemetry. \
                 When running, it collects and uploads diagnostic payloads. \
                 Off stops the service and sets startup to Disabled."
            }
            Self::CeipTasks => {
                "Scheduled tasks under Customer Experience Improvement Program, Application \
                 Experience, Feedback, Maps, Family Safety, and QueueReporting. \
                 Off disables them; On re-enables any that exist on this PC."
            }
            Self::AdvertisingId => {
                "A per-user ID apps can use for advertising. \
                 Off sets AdvertisingInfo\\Enabled to 0 so apps should not use it."
            }
            Self::TailoredExperiences => {
                "Lets Microsoft use diagnostic data for personalized tips and ads. \
                 Off clears TailoredExperiencesWithDiagnosticDataEnabled."
            }
            Self::CeipPolicy => {
                "Legacy SQM / CEIP policy that enrolls the PC in Microsoft’s customer \
                 experience improvement program. Off sets CEIPEnable to 0."
            }
            Self::AppInventory => {
                "Application Impact Telemetry and inventory used for compatibility \
                 assessment. Off sets AITEnable=0 and DisableInventory=1."
            }
        }
    }

    pub fn detail(self) -> &'static str {
        match self {
            Self::DiagnosticData => {
                r"HKLM\SOFTWARE\Policies\Microsoft\Windows\DataCollection\AllowTelemetry"
            }
            Self::DiagTrack => "Service: DiagTrack (Connected User Experiences and Telemetry)",
            Self::CeipTasks => "schtasks: CEIP, Compatibility Appraiser, Feedback, Maps, …",
            Self::AdvertisingId => {
                r"HKCU\SOFTWARE\Microsoft\Windows\CurrentVersion\AdvertisingInfo\Enabled"
            }
            Self::TailoredExperiences => {
                r"HKCU\…\Privacy\TailoredExperiencesWithDiagnosticDataEnabled"
            }
            Self::CeipPolicy => r"HKLM\SOFTWARE\Policies\Microsoft\SQMClient\Windows\CEIPEnable",
            Self::AppInventory => {
                r"HKLM\SOFTWARE\Policies\Microsoft\Windows\AppCompat (AITEnable, DisableInventory)"
            }
        }
    }

    /// Stable CLI id, e.g. `diagnostic-data`.
    pub fn cli_name(self) -> &'static str {
        match self {
            Self::DiagnosticData => "diagnostic-data",
            Self::DiagTrack => "diagtrack",
            Self::CeipTasks => "ceip-tasks",
            Self::AdvertisingId => "advertising-id",
            Self::TailoredExperiences => "tailored-experiences",
            Self::CeipPolicy => "ceip-policy",
            Self::AppInventory => "app-inventory",
        }
    }

    /// Parse a CLI setting name (accepts aliases).
    pub fn parse_cli(name: &str) -> Result<Self, String> {
        let n = name.trim().to_ascii_lowercase();
        match n.as_str() {
            "diagnostic-data" | "diagnostics" | "telemetry" | "allow-telemetry" => {
                Ok(Self::DiagnosticData)
            }
            "diagtrack" | "diag-track" | "connected-user-experiences" => Ok(Self::DiagTrack),
            "ceip-tasks" | "tasks" | "scheduled-tasks" => Ok(Self::CeipTasks),
            "advertising-id" | "ad-id" | "advertising" => Ok(Self::AdvertisingId),
            "tailored-experiences" | "tailored" => Ok(Self::TailoredExperiences),
            "ceip-policy" | "ceip" | "sqm" => Ok(Self::CeipPolicy),
            "app-inventory" | "inventory" | "ait" | "appcompat" => Ok(Self::AppInventory),
            _ => Err(format!(
                "unknown setting '{name}'. Try: {}",
                Self::ALL
                    .iter()
                    .map(|s| s.cli_name())
                    .collect::<Vec<_>>()
                    .join(", ")
            )),
        }
    }
}

/// Snapshot of one setting. `active == true` means telemetry/feature is ON (collecting).
#[derive(Debug, Clone)]
pub struct SettingState {
    pub id: SettingId,
    pub active: bool,
    pub note: String,
}

pub fn is_elevated() -> bool {
    win_cmd::command("net")
        .arg("session")
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}

/// Relaunch this exe elevated (no extra args). Does not wait — for GUI hand-off.
pub fn relaunch_elevated() -> Result<(), String> {
    relaunch_elevated_inner(&[], false)
}

/// Relaunch this exe elevated, forwarding CLI args and waiting (for UAC re-entry).
pub fn relaunch_elevated_with_args(args: &[String]) -> Result<(), String> {
    relaunch_elevated_inner(args, true)
}

fn relaunch_elevated_inner(args: &[String], wait: bool) -> Result<(), String> {
    let exe = std::env::current_exe().map_err(|e| e.to_string())?;
    let exe_str = exe.to_string_lossy().replace('\'', "''");

    let arg_list = if args.is_empty() {
        String::new()
    } else {
        let joined = args
            .iter()
            .map(|a| format!("'{}'", a.replace('\'', "''")))
            .collect::<Vec<_>>()
            .join(",");
        format!(" -ArgumentList {joined}")
    };

    let wait_flag = if wait { " -Wait" } else { "" };

    let status = win_cmd::command("powershell")
        .args([
            "-NoProfile",
            "-WindowStyle",
            "Hidden",
            "-Command",
            &format!("Start-Process -FilePath '{exe_str}' -Verb RunAs{wait_flag}{arg_list}"),
        ])
        .status()
        .map_err(|e| e.to_string())?;

    if status.success() {
        Ok(())
    } else {
        Err("UAC elevation was cancelled or failed".into())
    }
}

/// If not elevated, relaunch with the same argv and return `Ok(false)` (caller should exit).
/// If already elevated, return `Ok(true)`.
pub fn ensure_elevated() -> Result<bool, String> {
    if is_elevated() {
        return Ok(true);
    }
    let args: Vec<String> = std::env::args().skip(1).collect();
    relaunch_elevated_with_args(&args)?;
    Ok(false)
}

pub fn read_all() -> Vec<SettingState> {
    SettingId::ALL.iter().map(|id| read_one(*id)).collect()
}

pub fn read_one(id: SettingId) -> SettingState {
    match id {
        SettingId::DiagnosticData => read_diagnostic_data(),
        SettingId::DiagTrack => read_diagtrack(),
        SettingId::CeipTasks => read_ceip_tasks(),
        SettingId::AdvertisingId => read_advertising_id(),
        SettingId::TailoredExperiences => read_tailored(),
        SettingId::CeipPolicy => read_ceip_policy(),
        SettingId::AppInventory => read_app_inventory(),
    }
}

/// `active = true` turns the telemetry feature on; `false` turns it off.
pub fn apply(id: SettingId, active: bool) -> Result<String, String> {
    match id {
        SettingId::DiagnosticData => set_diagnostic_data(active),
        SettingId::DiagTrack => set_diagtrack(active),
        SettingId::CeipTasks => set_ceip_tasks(active),
        SettingId::AdvertisingId => set_advertising_id(active),
        SettingId::TailoredExperiences => set_tailored(active),
        SettingId::CeipPolicy => set_ceip_policy(active),
        SettingId::AppInventory => set_app_inventory(active),
    }
}

pub fn apply_all(active: bool) -> Vec<(SettingId, Result<String, String>)> {
    SettingId::ALL
        .iter()
        .map(|id| (*id, apply(*id, active)))
        .collect()
}

fn dword(hive: &RegKey, path: &str, name: &str) -> Option<u32> {
    let key = hive.open_subkey(path).ok()?;
    key.get_value(name).ok()
}

fn set_dword(hive: &RegKey, path: &str, name: &str, value: u32) -> Result<(), String> {
    let (key, _) = hive
        .create_subkey(path)
        .map_err(|e| format!("create {path}: {e}"))?;
    key.set_value(name, &value)
        .map_err(|e| format!("set {path}\\{name}: {e}"))
}

fn read_diagnostic_data() -> SettingState {
    let hklm = RegKey::predef(HKEY_LOCAL_MACHINE);
    let policy = dword(
        &hklm,
        r"SOFTWARE\Policies\Microsoft\Windows\DataCollection",
        "AllowTelemetry",
    );
    let current = dword(
        &hklm,
        r"SOFTWARE\Microsoft\Windows\CurrentVersion\Policies\DataCollection",
        "AllowTelemetry",
    );
    let level = policy.or(current).unwrap_or(1);
    SettingState {
        id: SettingId::DiagnosticData,
        active: level > 0,
        note: format!("AllowTelemetry = {level} (0 = Security-only / off)"),
    }
}

fn set_diagnostic_data(active: bool) -> Result<String, String> {
    let level: u32 = if active { 1 } else { 0 };
    let hklm = RegKey::predef(HKEY_LOCAL_MACHINE);
    for path in [
        r"SOFTWARE\Policies\Microsoft\Windows\DataCollection",
        r"SOFTWARE\Microsoft\Windows\CurrentVersion\Policies\DataCollection",
    ] {
        set_dword(&hklm, path, "AllowTelemetry", level)?;
        let _ = set_dword(&hklm, path, "MaxTelemetryAllowed", level);
    }
    Ok(format!("AllowTelemetry set to {level}"))
}

fn read_diagtrack() -> SettingState {
    let output = win_cmd::command("sc.exe")
        .args(["qc", "DiagTrack"])
        .output();
    let text = output
        .map(|o| String::from_utf8_lossy(&o.stdout).into_owned())
        .unwrap_or_default();
    let disabled = text.to_ascii_lowercase().contains("disabled");
    let running = win_cmd::command("sc.exe")
        .args(["query", "DiagTrack"])
        .output()
        .map(|o| {
            String::from_utf8_lossy(&o.stdout)
                .to_ascii_lowercase()
                .contains("running")
        })
        .unwrap_or(false);

    let note = if disabled {
        "Startup: Disabled".into()
    } else if running {
        "Startup: enabled, currently Running".into()
    } else {
        "Startup: not Disabled (service may start later)".into()
    };

    SettingState {
        id: SettingId::DiagTrack,
        active: !disabled,
        note,
    }
}

fn set_diagtrack(active: bool) -> Result<String, String> {
    if active {
        let config = win_cmd::command("sc.exe")
            .args(["config", "DiagTrack", "start=", "auto"])
            .output()
            .map_err(|e| e.to_string())?;
        if !config.status.success() {
            return Err(sc_err(&config));
        }
        let _ = win_cmd::command("sc.exe")
            .args(["start", "DiagTrack"])
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status();
        Ok("DiagTrack set to Automatic and start requested".into())
    } else {
        let _ = win_cmd::command("sc.exe")
            .args(["stop", "DiagTrack"])
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status();
        let config = win_cmd::command("sc.exe")
            .args(["config", "DiagTrack", "start=", "disabled"])
            .output()
            .map_err(|e| e.to_string())?;
        if !config.status.success() {
            return Err(sc_err(&config));
        }
        Ok("DiagTrack stopped and Disabled".into())
    }
}

fn sc_err(out: &std::process::Output) -> String {
    let a = String::from_utf8_lossy(&out.stderr);
    let b = String::from_utf8_lossy(&out.stdout);
    format!("{}{}", a.trim(), b.trim())
}

const CEIP_TASKS: &[&str] = &[
    r"\Microsoft\Windows\Customer Experience Improvement Program\Consolidator",
    r"\Microsoft\Windows\Customer Experience Improvement Program\UsbCeip",
    r"\Microsoft\Windows\Application Experience\Microsoft Compatibility Appraiser",
    r"\Microsoft\Windows\Application Experience\ProgramDataUpdater",
    r"\Microsoft\Windows\Application Experience\PcaPatchDbTask",
    r"\Microsoft\Windows\Application Experience\StartupAppTask",
    r"\Microsoft\Windows\Feedback\Siuf\DmClient",
    r"\Microsoft\Windows\Feedback\Siuf\DmClientOnScenarioDownload",
    r"\Microsoft\Windows\Maps\MapsToastTask",
    r"\Microsoft\Windows\Maps\MapsUpdateTask",
    r"\Microsoft\Windows\Shell\FamilySafetyMonitor",
    r"\Microsoft\Windows\Shell\FamilySafetyRefreshTask",
    r"\Microsoft\Windows\Windows Error Reporting\QueueReporting",
];

fn task_enabled(path: &str) -> Option<bool> {
    // LIST without /V is enough (Status line) and lighter than verbose queries.
    let output = win_cmd::command("schtasks.exe")
        .args(["/Query", "/TN", path, "/FO", "LIST"])
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let text = String::from_utf8_lossy(&output.stdout).to_ascii_lowercase();
    for line in text.lines() {
        let line = line.trim();
        if line.starts_with("status:") {
            return Some(!line.contains("disabled"));
        }
    }
    // Fallback if locale layout differs
    Some(!text.contains("disabled"))
}

fn read_ceip_tasks() -> SettingState {
    let mut found = 0usize;
    let mut enabled = 0usize;
    for t in CEIP_TASKS {
        if let Some(on) = task_enabled(t) {
            found += 1;
            if on {
                enabled += 1;
            }
        }
    }
    SettingState {
        id: SettingId::CeipTasks,
        active: enabled > 0,
        note: format!("{enabled} enabled / {found} found on this PC"),
    }
}

fn set_ceip_tasks(active: bool) -> Result<String, String> {
    let flag = if active { "/Enable" } else { "/Disable" };
    let mut changed = 0usize;
    let mut missing = 0usize;
    for t in CEIP_TASKS {
        let output = win_cmd::command("schtasks.exe")
            .args(["/Change", "/TN", t, flag])
            .output()
            .map_err(|e| e.to_string())?;
        if output.status.success() {
            changed += 1;
        } else {
            missing += 1;
        }
    }
    Ok(format!(
        "{} {changed} tasks ({} not present)",
        if active { "Enabled" } else { "Disabled" },
        missing
    ))
}

fn read_advertising_id() -> SettingState {
    let hkcu = RegKey::predef(HKEY_CURRENT_USER);
    let v = dword(
        &hkcu,
        r"SOFTWARE\Microsoft\Windows\CurrentVersion\AdvertisingInfo",
        "Enabled",
    )
    .unwrap_or(1);
    SettingState {
        id: SettingId::AdvertisingId,
        active: v != 0,
        note: format!("Enabled = {v}"),
    }
}

fn set_advertising_id(active: bool) -> Result<String, String> {
    let hkcu = RegKey::predef(HKEY_CURRENT_USER);
    let v = if active { 1u32 } else { 0 };
    set_dword(
        &hkcu,
        r"SOFTWARE\Microsoft\Windows\CurrentVersion\AdvertisingInfo",
        "Enabled",
        v,
    )?;
    Ok(format!("AdvertisingInfo\\Enabled = {v}"))
}

fn read_tailored() -> SettingState {
    let hkcu = RegKey::predef(HKEY_CURRENT_USER);
    let v = dword(
        &hkcu,
        r"SOFTWARE\Microsoft\Windows\CurrentVersion\Privacy",
        "TailoredExperiencesWithDiagnosticDataEnabled",
    )
    .unwrap_or(1);
    SettingState {
        id: SettingId::TailoredExperiences,
        active: v != 0,
        note: format!("TailoredExperiencesWithDiagnosticDataEnabled = {v}"),
    }
}

fn set_tailored(active: bool) -> Result<String, String> {
    let hkcu = RegKey::predef(HKEY_CURRENT_USER);
    let v = if active { 1u32 } else { 0 };
    set_dword(
        &hkcu,
        r"SOFTWARE\Microsoft\Windows\CurrentVersion\Privacy",
        "TailoredExperiencesWithDiagnosticDataEnabled",
        v,
    )?;
    Ok(format!("TailoredExperiences = {v}"))
}

fn read_ceip_policy() -> SettingState {
    let hklm = RegKey::predef(HKEY_LOCAL_MACHINE);
    let v = dword(
        &hklm,
        r"SOFTWARE\Policies\Microsoft\SQMClient\Windows",
        "CEIPEnable",
    )
    .unwrap_or(1);
    SettingState {
        id: SettingId::CeipPolicy,
        active: v != 0,
        note: format!("CEIPEnable = {v}"),
    }
}

fn set_ceip_policy(active: bool) -> Result<String, String> {
    let hklm = RegKey::predef(HKEY_LOCAL_MACHINE);
    let v = if active { 1u32 } else { 0 };
    set_dword(
        &hklm,
        r"SOFTWARE\Policies\Microsoft\SQMClient\Windows",
        "CEIPEnable",
        v,
    )?;
    Ok(format!("CEIPEnable = {v}"))
}

fn read_app_inventory() -> SettingState {
    let hklm = RegKey::predef(HKEY_LOCAL_MACHINE);
    let ait = dword(
        &hklm,
        r"SOFTWARE\Policies\Microsoft\Windows\AppCompat",
        "AITEnable",
    );
    let inv = dword(
        &hklm,
        r"SOFTWARE\Policies\Microsoft\Windows\AppCompat",
        "DisableInventory",
    );
    // Active (telemetry on) if AIT is non-zero OR inventory not disabled
    let active = match (ait, inv) {
        (Some(0), Some(1)) => false,
        (None, None) => true,
        (a, i) => a.unwrap_or(1) != 0 || i.unwrap_or(0) != 1,
    };
    SettingState {
        id: SettingId::AppInventory,
        active,
        note: format!(
            "AITEnable = {}, DisableInventory = {}",
            ait.map(|v| v.to_string()).unwrap_or_else(|| "unset".into()),
            inv.map(|v| v.to_string()).unwrap_or_else(|| "unset".into())
        ),
    }
}

fn set_app_inventory(active: bool) -> Result<String, String> {
    let hklm = RegKey::predef(HKEY_LOCAL_MACHINE);
    if active {
        set_dword(
            &hklm,
            r"SOFTWARE\Policies\Microsoft\Windows\AppCompat",
            "AITEnable",
            1,
        )?;
        set_dword(
            &hklm,
            r"SOFTWARE\Policies\Microsoft\Windows\AppCompat",
            "DisableInventory",
            0,
        )?;
        Ok("App inventory telemetry re-enabled".into())
    } else {
        set_dword(
            &hklm,
            r"SOFTWARE\Policies\Microsoft\Windows\AppCompat",
            "AITEnable",
            0,
        )?;
        set_dword(
            &hklm,
            r"SOFTWARE\Policies\Microsoft\Windows\AppCompat",
            "DisableInventory",
            1,
        )?;
        Ok("App inventory / AIT disabled".into())
    }
}
