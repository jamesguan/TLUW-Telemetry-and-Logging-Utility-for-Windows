//! Disclaimer, no-warranty, and limitation of liability text + acceptance gate.
//!
//! Acceptance is stored **locally on the user's machine** (not uploaded):
//! - **Primary:** `HKCU\Software\TelemetryLoggingUtility\Disclaimer` (survives TEMP/log wipes)
//! - **Backup:** `%APPDATA%\TelemetryLoggingUtility\` (roaming; not a temp clear target)
//!
//! Canonical human copy: `DISCLAIMER.md`.

use crate::identity;
use crate::win_cmd;
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::PathBuf;
use winreg::enums::*;
use winreg::RegKey;

/// One-line notice for footers / help blurbs.
pub const SHORT: &str = "USE AT YOUR OWN RISK. Provided AS IS with NO WARRANTY. Authors accept NO LIABILITY.";

/// Full disclaimer shown before use (GUI) and via `tluw disclaimer`.
pub const FULL: &str = "\
DISCLAIMER — NO WARRANTY — LIMITATION OF LIABILITY — ASSUMPTION OF RISK

By downloading, installing, copying, or using Telemetry and Logging Utility for Windows (the \"Software\"), \
including the CLI, GUI, and installer, you acknowledge and agree to all of the following.

1. NO WARRANTY. The Software is provided \"AS IS\" and \"AS AVAILABLE\", without warranty \
of any kind, express or implied, including but not limited to warranties of \
merchantability, fitness for a particular purpose, title, non-infringement, accuracy, \
or uninterrupted/error-free operation. You use it entirely at your own risk.

2. DANGEROUS OPERATIONS. The Software can change Windows registry keys and policies, \
start/stop services, modify scheduled tasks, delete log and temporary files, and \
otherwise alter your system. Those actions may cause data loss, application or OS \
instability, loss of diagnostic information, security or compliance issues, boot or \
update problems, or other harm. You are solely responsible for backups, testing, and \
deciding whether any action is appropriate for your machine.

3. NO LIABILITY. To the maximum extent permitted by law, the copyright holders, \
authors, contributors, and distributors (collectively, the \"Authors\") shall not be \
liable for any claim, damages, or other liability — whether in contract, tort \
(including negligence), strict liability, or otherwise — arising from or related to \
the Software or its use or inability to use, including but not limited to direct, \
indirect, incidental, special, consequential, punitive, or exemplary damages, loss of \
data, loss of profits, business interruption, system failure, or cost of substitute \
goods or services, even if advised of the possibility of such damages.

4. INDEMNITY. You agree to defend, indemnify, and hold the Authors harmless from and \
against any claims, losses, damages, liabilities, costs, and expenses (including \
reasonable attorneys' fees) arising out of your use of the Software, your violation \
of these terms or of applicable law, or your distribution of the Software.

5. NOT LEGAL, SECURITY, OR COMPLIANCE ADVICE. The Software does not constitute legal, \
privacy, security, IT, or regulatory advice. Disabling telemetry or deleting logs may \
conflict with employer policy, school policy, or legal/regulatory obligations. You \
alone are responsible for compliance.

6. NO SUPPORT OBLIGATION. The Authors have no duty to provide support, updates, fixes, \
or continued availability of the Software.

7. ACCEPTANCE. If you do not agree, do not install or use the Software. Continued use \
constitutes acceptance. These terms supplement the PolyForm Noncommercial License 1.0.0 \
(see LICENSE); if conflict exists on liability/warranty, the stricter limitation in \
favor of the Authors controls to the extent allowed by law.

Some jurisdictions do not allow certain exclusions; in those places, liability is \
limited to the fullest extent the law permits (which may be zero monetary liability \
where allowed).
";

/// Snapshot of a local acceptance record.
#[derive(Debug, Clone, Default)]
pub struct AcceptanceRecord {
    pub accepted_at: String,
    pub user: String,
    pub computer: String,
    pub version: String,
    pub source: String,
    pub disclaimer_bytes: usize,
}

fn env_or(key: &str, fallback: &str) -> String {
    std::env::var(key).unwrap_or_else(|_| fallback.to_string())
}

/// App data under **roaming** APPDATA — never targeted by our TEMP cleaners.
fn roaming_dir() -> Option<PathBuf> {
    identity::roaming_dir()
}

fn marker_path() -> Option<PathBuf> {
    Some(roaming_dir()?.join("disclaimer_accepted"))
}

fn log_path() -> Option<PathBuf> {
    Some(roaming_dir()?.join("disclaimer_acceptance.log"))
}

/// LocalAppData marker path (same product folder name).
fn local_marker_path() -> Option<PathBuf> {
    let base = std::env::var_os("LOCALAPPDATA")?;
    Some(
        PathBuf::from(base)
            .join(identity::APPDATA_DIR)
            .join("disclaimer_accepted"),
    )
}

fn parse_file_record(text: &str) -> AcceptanceRecord {
    let mut rec = AcceptanceRecord::default();
    for line in text.lines() {
        if let Some((k, v)) = line.split_once('=') {
            match k.trim() {
                "accepted_at" => rec.accepted_at = v.trim().to_string(),
                "user" => rec.user = v.trim().to_string(),
                "computer" => rec.computer = v.trim().to_string(),
                "version" => rec.version = v.trim().to_string(),
                "source" => rec.source = v.trim().to_string(),
                "disclaimer_bytes" => {
                    rec.disclaimer_bytes = v.trim().parse().unwrap_or(0);
                }
                _ => {}
            }
        } else if line.trim() == "accepted" && rec.accepted_at.is_empty() {
            rec.accepted_at = "(unknown — accepted before timestamped records)".into();
        }
    }
    if rec.accepted_at.is_empty() && !text.trim().is_empty() {
        rec.accepted_at = "(unknown — accepted before timestamped records)".into();
    }
    rec
}

fn read_registry() -> Option<AcceptanceRecord> {
    let hkcu = RegKey::predef(HKEY_CURRENT_USER);
    let key = hkcu.open_subkey(identity::REG_DISCLAIMER).ok()?;
    let accepted: u32 = key.get_value("Accepted").ok()?;
    if accepted == 0 {
        return None;
    }
    Some(AcceptanceRecord {
        accepted_at: key.get_value("AcceptedAt").unwrap_or_default(),
        user: key.get_value("User").unwrap_or_default(),
        computer: key.get_value("Computer").unwrap_or_default(),
        version: key.get_value("Version").unwrap_or_default(),
        source: key.get_value("Source").unwrap_or_default(),
        disclaimer_bytes: key
            .get_value::<u32, _>("DisclaimerBytes")
            .unwrap_or(0) as usize,
    })
}

fn write_registry(rec: &AcceptanceRecord) -> Result<(), String> {
    let hkcu = RegKey::predef(HKEY_CURRENT_USER);
    let (key, _) = hkcu
        .create_subkey(identity::REG_DISCLAIMER)
        .map_err(|e| format!("Could not open registry key: {e}"))?;
    key.set_value("Accepted", &1u32)
        .map_err(|e| format!("registry Accepted: {e}"))?;
    key.set_value("AcceptedAt", &rec.accepted_at)
        .map_err(|e| format!("registry AcceptedAt: {e}"))?;
    key.set_value("User", &rec.user)
        .map_err(|e| format!("registry User: {e}"))?;
    key.set_value("Computer", &rec.computer)
        .map_err(|e| format!("registry Computer: {e}"))?;
    key.set_value("Version", &rec.version)
        .map_err(|e| format!("registry Version: {e}"))?;
    key.set_value("Source", &rec.source)
        .map_err(|e| format!("registry Source: {e}"))?;
    key.set_value("DisclaimerBytes", &(rec.disclaimer_bytes as u32))
        .map_err(|e| format!("registry DisclaimerBytes: {e}"))?;
    key.set_value("ShortNotice", &SHORT.to_string())
        .map_err(|e| format!("registry ShortNotice: {e}"))?;
    Ok(())
}

fn write_file_backup(rec: &AcceptanceRecord) -> Result<(), String> {
    let dir = roaming_dir().ok_or_else(|| "APPDATA is not set".to_string())?;
    fs::create_dir_all(&dir).map_err(|e| format!("Could not create config dir: {e}"))?;

    let body = format!(
        "accepted=yes\n\
         accepted_at={}\n\
         user={}\n\
         computer={}\n\
         version={}\n\
         source={}\n\
         disclaimer_bytes={}\n\
         short={}\n\
         storage=HKCU\\\\Software\\\\TelemetryLoggingUtility\\\\Disclaimer + APPDATA backup\n",
        rec.accepted_at,
        rec.user,
        rec.computer,
        rec.version,
        rec.source,
        rec.disclaimer_bytes,
        SHORT
    );

    let marker = dir.join("disclaimer_accepted");
    fs::write(&marker, &body).map_err(|e| format!("Could not save acceptance file: {e}"))?;

    if let Some(log) = log_path() {
        let line = format!(
            "{} | user={} | computer={} | version={} | source={} | bytes={}\n",
            rec.accepted_at,
            rec.user,
            rec.computer,
            rec.version,
            rec.source,
            rec.disclaimer_bytes
        );
        let _ = OpenOptions::new()
            .create(true)
            .append(true)
            .open(log)
            .and_then(|mut f| f.write_all(line.as_bytes()));
    }
    Ok(())
}

/// Whether the user has accepted (registry first, then file backups).
pub fn is_accepted() -> bool {
    read_record().is_some()
}

/// Read the latest acceptance record, if present.
pub fn read_record() -> Option<AcceptanceRecord> {
    if let Some(rec) = read_registry() {
        if !rec.accepted_at.is_empty() || rec.disclaimer_bytes > 0 {
            return Some(rec);
        }
        // Accepted=1 with empty fields still counts.
        return Some(rec);
    }
    for path in [marker_path(), local_marker_path()].into_iter().flatten() {
        if let Ok(text) = fs::read_to_string(&path) {
            let rec = parse_file_record(&text);
            if !rec.accepted_at.is_empty() || !text.trim().is_empty() {
                return Some(rec);
            }
        }
    }
    None
}

/// Persist acceptance with timestamp / user / machine.
///
/// Writes **HKCU registry** (primary — survives TEMP/log clears) and an
/// **%APPDATA%** file backup. `source` should be `"gui"` or `"cli"`.
pub fn accept(source: &str) -> Result<AcceptanceRecord, String> {
    let rec = AcceptanceRecord {
        accepted_at: win_cmd::local_stamp(),
        user: env_or("USERNAME", "(unknown)"),
        computer: env_or("COMPUTERNAME", "(unknown)"),
        version: env!("CARGO_PKG_VERSION").to_string(),
        source: source.to_string(),
        disclaimer_bytes: FULL.len(),
    };

    write_registry(&rec)?;
    // File backup is best-effort; registry already succeeded.
    let _ = write_file_backup(&rec);

    Ok(rec)
}
