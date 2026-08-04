//! Scheduled clearing of logs and temp via Windows Task Scheduler.
//!
//! Triggers supported:
//! - Interval (hourly / 6h / daily / weekly) via TimeTrigger + Repetition or CalendarTrigger
//! - At user logon (`LogonTrigger`) — “on startup” for a per-user tool
//! - At session end (`SessionStateChangeTrigger` ConsoleDisconnect) — best-effort
//!   stand-in for shutdown/logoff (true power-off is not reliable for InteractiveToken tasks)

use std::process::Stdio;

use crate::identity::CLEANUP_TASK_NAME;
use crate::log_cleanup;
use crate::maintenance;
use crate::prefs;
use crate::temp_cleanup;
use crate::win_cmd;

/// How often the cleanup task should fire (in addition to optional logon/logoff).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CleanupInterval {
    Off,
    Hourly,
    Every6Hours,
    Daily,
    Weekly,
}

impl CleanupInterval {
    pub const ALL: [CleanupInterval; 5] = [
        CleanupInterval::Off,
        CleanupInterval::Hourly,
        CleanupInterval::Every6Hours,
        CleanupInterval::Daily,
        CleanupInterval::Weekly,
    ];

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Off => "off",
            Self::Hourly => "hourly",
            Self::Every6Hours => "6h",
            Self::Daily => "daily",
            Self::Weekly => "weekly",
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::Off => "Off",
            Self::Hourly => "Hourly",
            Self::Every6Hours => "Every 6h",
            Self::Daily => "Daily",
            Self::Weekly => "Weekly",
        }
    }

    pub fn from_str(s: &str) -> Self {
        match s.trim().to_ascii_lowercase().as_str() {
            "hourly" | "1h" | "hour" => Self::Hourly,
            "6h" | "every6hours" | "every-6-hours" => Self::Every6Hours,
            "daily" | "day" => Self::Daily,
            "weekly" | "week" => Self::Weekly,
            _ => Self::Off,
        }
    }
}

/// What to clear and when. Persisted under HKCU Prefs; Task Scheduler runs the action.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CleanupScheduleConfig {
    pub clear_safe_logs: bool,
    pub clear_all_logs: bool,
    pub clear_temp: bool,
    pub on_logon: bool,
    /// Best-effort session end (logoff / console disconnect). Not a guaranteed power-off hook.
    pub on_session_end: bool,
    pub interval: CleanupInterval,
}

impl Default for CleanupScheduleConfig {
    fn default() -> Self {
        Self {
            clear_safe_logs: true,
            clear_all_logs: false,
            clear_temp: true,
            on_logon: false,
            on_session_end: false,
            interval: CleanupInterval::Off,
        }
    }
}

impl CleanupScheduleConfig {
    pub fn has_any_trigger(&self) -> bool {
        self.on_logon || self.on_session_end || self.interval != CleanupInterval::Off
    }

    pub fn has_any_target(&self) -> bool {
        self.clear_safe_logs || self.clear_all_logs || self.clear_temp
    }

    pub fn is_active(&self) -> bool {
        self.has_any_trigger() && self.has_any_target()
    }

    pub fn summary_line(&self) -> String {
        if !self.is_active() {
            return "Cleanup schedule: OFF".into();
        }
        let mut targets = Vec::new();
        if self.clear_all_logs {
            targets.push("all logs");
        } else if self.clear_safe_logs {
            targets.push("safe logs");
        }
        if self.clear_temp {
            targets.push("temp");
        }
        let mut when = Vec::new();
        if self.interval != CleanupInterval::Off {
            when.push(self.interval.label());
        }
        if self.on_logon {
            when.push("logon");
        }
        if self.on_session_end {
            when.push("session-end");
        }
        format!(
            "Cleanup schedule: {} · {}",
            targets.join(" + "),
            when.join(", ")
        )
    }
}

/// Live status: prefs + whether the scheduled task is registered.
#[derive(Debug, Clone)]
pub struct CleanupScheduleState {
    pub config: CleanupScheduleConfig,
    pub task_registered: bool,
    /// Human line for the next interval fire (if an interval is selected).
    pub next_interval: Option<String>,
    /// Raw “Next Run Time” from `schtasks` when the task is registered (may be N/A for logon-only).
    pub scheduler_next: Option<String>,
}

pub fn read_state() -> CleanupScheduleState {
    let config = load_config();
    let task_registered = maintenance::task_exists(CLEANUP_TASK_NAME);
    CleanupScheduleState {
        next_interval: describe_next_interval(config.interval),
        scheduler_next: if task_registered {
            query_scheduler_next_run()
        } else {
            None
        },
        config,
        task_registered,
    }
}

/// Cadence blurb matching the Task Scheduler XML we register.
pub fn interval_cadence(interval: CleanupInterval) -> Option<&'static str> {
    match interval {
        CleanupInterval::Off => None,
        CleanupInterval::Hourly => Some("every hour at :05"),
        CleanupInterval::Every6Hours => Some("every 6 hours at 03:00 / 09:00 / 15:00 / 21:00"),
        CleanupInterval::Daily => Some("every day at 03:30"),
        CleanupInterval::Weekly => Some("every Sunday at 03:30"),
    }
}

/// Next interval fire from our schedule anchors (same as the registered task XML).
pub fn describe_next_interval(interval: CleanupInterval) -> Option<String> {
    let cadence = interval_cadence(interval)?;
    let when = estimate_next_interval_run(interval)?;
    Some(format!("Next run: {when} ({cadence})"))
}

/// Estimate the next wall-clock time for an interval trigger.
pub fn estimate_next_interval_run(interval: CleanupInterval) -> Option<String> {
    let now = win_cmd::LocalTime::now();
    let next = match interval {
        CleanupInterval::Off => return None,
        CleanupInterval::Hourly => next_hourly_at_minute(now, 5),
        CleanupInterval::Every6Hours => next_every_n_hours(now, &[3, 9, 15, 21], 0),
        CleanupInterval::Daily => next_daily_at(now, 3, 30),
        CleanupInterval::Weekly => next_weekday_at(now, 0, 3, 30), // Sunday
    };
    Some(next.stamp_hm())
}

fn query_scheduler_next_run() -> Option<String> {
    let out = win_cmd::command("schtasks.exe")
        .args(["/Query", "/TN", CLEANUP_TASK_NAME, "/FO", "LIST", "/V"])
        .output()
        .ok()?;
    if !out.status.success() {
        return None;
    }
    let text = String::from_utf8_lossy(&out.stdout);
    for line in text.lines() {
        let line = line.trim();
        // Localized label ends with the value after a long pad; match English key prefix.
        if let Some(rest) = line
            .strip_prefix("Next Run Time:")
            .or_else(|| {
                // Some locales keep the English column via /FO LIST; also try without colon spacing.
                line.split_once("Next Run Time").map(|(_, v)| v.trim_start_matches(':'))
            })
        {
            let v = rest.trim();
            if v.is_empty() || v.eq_ignore_ascii_case("N/A") {
                return None;
            }
            return Some(v.to_string());
        }
    }
    None
}

fn is_leap(year: u16) -> bool {
    (year % 4 == 0 && year % 100 != 0) || (year % 400 == 0)
}

fn days_in_month(year: u16, month: u16) -> u16 {
    match month {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        4 | 6 | 9 | 11 => 30,
        2 => {
            if is_leap(year) {
                29
            } else {
                28
            }
        }
        _ => 30,
    }
}

fn add_days_forward(mut t: win_cmd::LocalTime, days: u32) -> win_cmd::LocalTime {
    t.day_of_week = ((u32::from(t.day_of_week) + days) % 7) as u16;
    let mut left = days;
    while left > 0 {
        let dim = u32::from(days_in_month(t.year, t.month));
        let room = dim - u32::from(t.day);
        if left <= room {
            t.day += left as u16;
            left = 0;
        } else {
            left -= room + 1;
            t.day = 1;
            t.month += 1;
            if t.month > 12 {
                t.month = 1;
                t.year = t.year.saturating_add(1);
            }
        }
    }
    t.second = 0;
    t
}

fn at_time(mut t: win_cmd::LocalTime, hour: u16, minute: u16) -> win_cmd::LocalTime {
    t.hour = hour;
    t.minute = minute;
    t.second = 0;
    t
}

fn time_tuple(t: win_cmd::LocalTime) -> (u16, u16, u16) {
    (t.hour, t.minute, t.second)
}

/// Next occurrence of `HH:mm` on an hourly cadence (minute fixed, e.g. :05).
fn next_hourly_at_minute(now: win_cmd::LocalTime, minute: u16) -> win_cmd::LocalTime {
    if now.minute < minute {
        return at_time(now, now.hour, minute);
    }
    if now.hour == 23 {
        at_time(add_days_forward(now, 1), 0, minute)
    } else {
        at_time(now, now.hour + 1, minute)
    }
}

/// Next slot among fixed local hours (e.g. 3/9/15/21), at `minute`.
fn next_every_n_hours(now: win_cmd::LocalTime, hours: &[u16], minute: u16) -> win_cmd::LocalTime {
    for &h in hours {
        let candidate = at_time(now, h, minute);
        if time_tuple(now) < time_tuple(candidate) {
            return candidate;
        }
    }
    // Tomorrow, first slot
    at_time(add_days_forward(now, 1), hours[0], minute)
}

fn next_daily_at(now: win_cmd::LocalTime, hour: u16, minute: u16) -> win_cmd::LocalTime {
    let today = at_time(now, hour, minute);
    if time_tuple(now) < time_tuple(today) {
        today
    } else {
        at_time(add_days_forward(now, 1), hour, minute)
    }
}

/// `weekday`: 0 = Sunday … 6 = Saturday (Win32).
fn next_weekday_at(
    now: win_cmd::LocalTime,
    weekday: u16,
    hour: u16,
    minute: u16,
) -> win_cmd::LocalTime {
    let today = at_time(now, hour, minute);
    if now.day_of_week == weekday && time_tuple(now) < time_tuple(today) {
        return today;
    }
    let days_ahead = (u32::from(weekday) + 7 - u32::from(now.day_of_week)) % 7;
    let days = if days_ahead == 0 { 7 } else { days_ahead };
    at_time(add_days_forward(now, days), hour, minute)
}

pub fn load_config() -> CleanupScheduleConfig {
    CleanupScheduleConfig {
        clear_safe_logs: prefs::cleanup_bool("CleanupClearSafeLogs", true),
        clear_all_logs: prefs::cleanup_bool("CleanupClearAllLogs", false),
        clear_temp: prefs::cleanup_bool("CleanupClearTemp", true),
        on_logon: prefs::cleanup_bool("CleanupOnLogon", false),
        on_session_end: prefs::cleanup_bool("CleanupOnSessionEnd", false),
        interval: CleanupInterval::from_str(&prefs::cleanup_string(
            "CleanupInterval",
            CleanupInterval::Off.as_str(),
        )),
    }
}

fn save_config(cfg: &CleanupScheduleConfig) -> Result<(), String> {
    prefs::set_cleanup_bool("CleanupClearSafeLogs", cfg.clear_safe_logs)?;
    prefs::set_cleanup_bool("CleanupClearAllLogs", cfg.clear_all_logs)?;
    prefs::set_cleanup_bool("CleanupClearTemp", cfg.clear_temp)?;
    prefs::set_cleanup_bool("CleanupOnLogon", cfg.on_logon)?;
    prefs::set_cleanup_bool("CleanupOnSessionEnd", cfg.on_session_end)?;
    prefs::set_cleanup_string("CleanupInterval", cfg.interval.as_str())?;
    Ok(())
}

/// Persist prefs and create/update/remove the Task Scheduler entry.
pub fn apply(cfg: &CleanupScheduleConfig) -> Result<String, String> {
    save_config(cfg)?;

    if !cfg.is_active() {
        maintenance::delete_task(CLEANUP_TASK_NAME);
        return Ok("Cleanup schedule disabled (task removed)".into());
    }

    create_task(cfg)?;
    let mut msg = format!(
        "Cleanup schedule applied — task '{}' ({})",
        CLEANUP_TASK_NAME,
        cfg.summary_line()
    );
    if let Some(next) = describe_next_interval(cfg.interval) {
        msg.push_str(" · ");
        msg.push_str(&next);
    }
    Ok(msg)
}

/// Disable schedule: clear triggers in prefs and remove the task.
pub fn disable() -> Result<String, String> {
    let mut cfg = load_config();
    cfg.on_logon = false;
    cfg.on_session_end = false;
    cfg.interval = CleanupInterval::Off;
    apply(&cfg)
}

/// Entry point for the scheduled task (`tluw scheduled-cleanup --no-elevate`).
///
/// Runs clears immediately with **no confirmation prompts** — this is intentional
/// for Task Scheduler / unattended execution.
pub fn run_now() -> Result<String, String> {
    let cfg = load_config();
    if !cfg.has_any_target() {
        return Ok("Scheduled cleanup: nothing selected to clear (check prefs)".into());
    }

    let mut parts = Vec::new();

    if cfg.clear_all_logs || cfg.clear_safe_logs {
        let include_dangerous = cfg.clear_all_logs;
        let results = log_cleanup::clear_all(include_dangerous);
        let mut files = 0u64;
        let mut bytes = 0u64;
        let mut fails = 0usize;
        for (_, r) in &results {
            match r {
                Ok(res) => {
                    files += res.removed_files;
                    bytes += res.freed_bytes;
                }
                Err(_) => fails += 1,
            }
        }
        parts.push(format!(
            "logs: {} target(s), {} item(s), {}{}",
            results.iter().filter(|(_, r)| r.is_ok()).count(),
            files,
            log_cleanup::format_bytes(bytes),
            if fails > 0 {
                format!(", {fails} failed")
            } else {
                String::new()
            }
        ));
    }

    if cfg.clear_temp {
        let results = temp_cleanup::clear_all();
        let mut files = 0u64;
        let mut bytes = 0u64;
        let mut fails = 0usize;
        for (_, r) in &results {
            match r {
                Ok(res) => {
                    files += res.removed_files;
                    bytes += res.freed_bytes;
                }
                Err(_) => fails += 1,
            }
        }
        parts.push(format!(
            "temp: {} target(s), {} item(s), {}{}",
            results.iter().filter(|(_, r)| r.is_ok()).count(),
            files,
            log_cleanup::format_bytes(bytes),
            if fails > 0 {
                format!(", {fails} failed")
            } else {
                String::new()
            }
        ));
    }

    Ok(format!("Scheduled cleanup — {}", parts.join("; ")))
}

fn create_task(cfg: &CleanupScheduleConfig) -> Result<(), String> {
    let cli = maintenance::cli_exe_path()?;
    let cli_str = cli.to_string_lossy().replace('&', "&amp;");

    let mut triggers = String::new();
    match cfg.interval {
        CleanupInterval::Off => {}
        CleanupInterval::Hourly => {
            triggers.push_str(
                r#"    <TimeTrigger>
      <Repetition>
        <Interval>PT1H</Interval>
        <StopAtDurationEnd>false</StopAtDurationEnd>
      </Repetition>
      <StartBoundary>2020-01-01T00:05:00</StartBoundary>
      <Enabled>true</Enabled>
    </TimeTrigger>
"#,
            );
        }
        CleanupInterval::Every6Hours => {
            triggers.push_str(
                r#"    <TimeTrigger>
      <Repetition>
        <Interval>PT6H</Interval>
        <StopAtDurationEnd>false</StopAtDurationEnd>
      </Repetition>
      <StartBoundary>2020-01-01T03:00:00</StartBoundary>
      <Enabled>true</Enabled>
    </TimeTrigger>
"#,
            );
        }
        CleanupInterval::Daily => {
            triggers.push_str(
                r#"    <CalendarTrigger>
      <StartBoundary>2020-01-01T03:30:00</StartBoundary>
      <Enabled>true</Enabled>
      <ScheduleByDay>
        <DaysInterval>1</DaysInterval>
      </ScheduleByDay>
    </CalendarTrigger>
"#,
            );
        }
        CleanupInterval::Weekly => {
            triggers.push_str(
                r#"    <CalendarTrigger>
      <StartBoundary>2020-01-05T03:30:00</StartBoundary>
      <Enabled>true</Enabled>
      <ScheduleByWeek>
        <WeeksInterval>1</WeeksInterval>
        <DaysOfWeek>
          <Sunday />
        </DaysOfWeek>
      </ScheduleByWeek>
    </CalendarTrigger>
"#,
            );
        }
    }

    if cfg.on_logon {
        triggers.push_str(
            r#"    <LogonTrigger>
      <Enabled>true</Enabled>
      <Delay>PT3M</Delay>
    </LogonTrigger>
"#,
        );
    }

    if cfg.on_session_end {
        // ConsoleDisconnect fires on interactive logoff / session teardown.
        // This is the practical stand-in for “on shutdown” for a user-scoped task.
        triggers.push_str(
            r#"    <SessionStateChangeTrigger>
      <Enabled>true</Enabled>
      <StateChange>ConsoleDisconnect</StateChange>
    </SessionStateChangeTrigger>
"#,
        );
    }

    let mut xml = String::from(r#"<?xml version="1.0" encoding="UTF-16"?>"#);
    xml.push('\n');
    xml.push_str(
        r#"<Task version="1.4" xmlns="http://schemas.microsoft.com/windows/2004/02/mit/task">
  <RegistrationInfo>
    <Description>Telemetry Logging Utility — clear selected logs and/or temp folders on a schedule, at logon, or at session end.</Description>
  </RegistrationInfo>
  <Triggers>
"#,
    );
    xml.push_str(&triggers);
    xml.push_str(
        r#"  </Triggers>
  <Principals>
    <Principal id="Author">
      <LogonType>InteractiveToken</LogonType>
      <RunLevel>HighestAvailable</RunLevel>
    </Principal>
  </Principals>
  <Settings>
    <MultipleInstancesPolicy>IgnoreNew</MultipleInstancesPolicy>
    <DisallowStartIfOnBatteries>false</DisallowStartIfOnBatteries>
    <StopIfGoingOnBatteries>false</StopIfGoingOnBatteries>
    <AllowHardTerminate>true</AllowHardTerminate>
    <StartWhenAvailable>true</StartWhenAvailable>
    <RunOnlyIfNetworkAvailable>false</RunOnlyIfNetworkAvailable>
    <AllowStartOnDemand>true</AllowStartOnDemand>
    <Enabled>true</Enabled>
    <Hidden>false</Hidden>
    <ExecutionTimeLimit>PT30M</ExecutionTimeLimit>
    <Priority>7</Priority>
  </Settings>
  <Actions Context="Author">
    <Exec>
"#,
    );
    xml.push_str("      <Command>");
    xml.push_str(&cli_str);
    xml.push_str("</Command>\n");
    xml.push_str("      <Arguments>scheduled-cleanup --no-elevate</Arguments>\n");
    xml.push_str(
        r#"    </Exec>
  </Actions>
</Task>
"#,
    );

    let tmp = std::env::temp_dir().join("tluw-cleanup-schedule.xml");
    let mut utf16: Vec<u8> = vec![0xFF, 0xFE];
    for u in xml.encode_utf16() {
        utf16.extend_from_slice(&u.to_le_bytes());
    }
    std::fs::write(&tmp, &utf16).map_err(|e| e.to_string())?;

    maintenance::delete_task(CLEANUP_TASK_NAME);

    let out = win_cmd::command("schtasks.exe")
        .args([
            "/Create",
            "/TN",
            CLEANUP_TASK_NAME,
            "/XML",
            tmp.to_str().unwrap_or_default(),
            "/F",
        ])
        .output()
        .map_err(|e| e.to_string())?;

    let _ = std::fs::remove_file(&tmp);

    if out.status.success() {
        Ok(())
    } else {
        let msg = format!(
            "{}{}",
            String::from_utf8_lossy(&out.stderr).trim(),
            String::from_utf8_lossy(&out.stdout).trim()
        );
        // Fall back to a simple ONLOGON or HOURLY task when XML schema is rejected.
        create_simple_fallback(cfg, &cli_str).map_err(|e| {
            format!("cleanup schedule XML failed ({msg}); fallback also failed: {e}")
        })?;
        Ok(())
    }
}

fn create_simple_fallback(cfg: &CleanupScheduleConfig, cli: &str) -> Result<(), String> {
    maintenance::delete_task(CLEANUP_TASK_NAME);
    let tr = format!("\"{cli}\" scheduled-cleanup --no-elevate");

    let (sc, mo): (&str, Option<&str>) = if cfg.on_logon && cfg.interval == CleanupInterval::Off {
        ("ONLOGON", None)
    } else {
        match cfg.interval {
            CleanupInterval::Hourly => ("HOURLY", None),
            CleanupInterval::Every6Hours => ("HOURLY", Some("6")),
            CleanupInterval::Daily => ("DAILY", None),
            CleanupInterval::Weekly => ("WEEKLY", None),
            CleanupInterval::Off => ("ONLOGON", None),
        }
    };

    let mut cmd = win_cmd::command("schtasks.exe");
    cmd.args([
        "/Create",
        "/TN",
        CLEANUP_TASK_NAME,
        "/TR",
        &tr,
        "/SC",
        sc,
        "/RL",
        "HIGHEST",
        "/F",
    ]);
    if let Some(m) = mo {
        cmd.args(["/MO", m]);
    }

    let out = cmd
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .map_err(|e| e.to_string())?;

    if out.status.success() {
        Ok(())
    } else {
        Err(format!(
            "{}{}",
            String::from_utf8_lossy(&out.stderr).trim(),
            String::from_utf8_lossy(&out.stdout).trim()
        ))
    }
}
