//! Business-logic handlers (library API calls) for pending GUI actions.

use crate::cleanup_history;
use crate::cleanup_schedule::{self, CleanupScheduleConfig};
use crate::log_cleanup;
use crate::maintenance;
use crate::telemetry::{self, SettingId};
use crate::temp_cleanup;

use super::App;

#[derive(Debug, Clone)]
pub enum Pending {
    One { id: SettingId, active: bool },
    All { active: bool },
    Verify,
    SetStartup(bool),
    SetPostUpdate(bool),
    ApplyCleanupSchedule(CleanupScheduleConfig),
    Quit,
    ClearLog(&'static str),
    ClearAllSafe,
    ClearAllLogs,
    ClearTemp(&'static str),
    ClearTempAll,
}

/// In-app confirmation dialog (matches iced chrome — not a Win32 MessageBox).
#[derive(Debug, Clone)]
pub struct ConfirmPrompt {
    pub title: String,
    pub body: String,
    pub action_label: String,
    pub danger: bool,
    pub pending: Pending,
}

impl App {
    pub(super) fn refresh(&mut self) {
        self.settings = telemetry::read_all();
    }

    pub(super) fn run_verify(&mut self) {
        self.refresh();
        let off = self.settings.iter().filter(|s| !s.active).count();
        let on = self.settings.iter().filter(|s| s.active).count();
        self.verify_stamp = crate::win_cmd::local_stamp();
        self.hide_verify = false;
        self.verify_highlight_until =
            Some(self.anim_now + std::time::Duration::from_millis(1800));
        self.status = format!(
            "Verified at {} — re-read {off} OFF + {on} ON from Windows (registry / services / tasks). \
             Nothing was changed — see highlighted switches and Verified values.",
            self.verify_stamp
        );
        self.status_ok = true;
    }

    pub(super) fn refresh_cleanup_history(&mut self) {
        self.cleanup_days = cleanup_history::daily_totals(14);
    }

    pub(super) fn refresh_log_status(&mut self) {
        self.log_status.clear();
        for (action, st) in log_cleanup::inspect_all() {
            self.log_status.insert(action.id, st);
        }
    }

    pub(super) fn refresh_temp_status(&mut self) {
        self.temp_status.clear();
        for (target, st) in temp_cleanup::inspect_all() {
            self.temp_status.insert(target.id, st);
        }
    }

    pub(super) fn run_pending(&mut self, pending: Pending) {
        match pending {
            Pending::Verify => {
                self.run_verify();
                self.integration = maintenance::read_integration();
                return;
            }
            Pending::SetStartup(enabled) => match maintenance::set_run_at_startup(enabled) {
                Ok(msg) => {
                    self.status = msg;
                    self.status_ok = true;
                }
                Err(e) => {
                    self.status = format!("Startup option failed: {e}");
                    self.status_ok = false;
                }
            },
            Pending::SetPostUpdate(enabled) => {
                if !self.elevated {
                    self.status =
                        "Administrator required to create the post-update scheduled task.".into();
                    self.status_ok = false;
                    return;
                }
                match maintenance::set_post_update_task(enabled) {
                    Ok(msg) => {
                        self.status = msg;
                        self.status_ok = true;
                    }
                    Err(e) => {
                        self.status = format!("Post-update task failed: {e}");
                        self.status_ok = false;
                    }
                }
            }
            Pending::ApplyCleanupSchedule(cfg) => {
                let needs_task = cfg.is_active() || self.cleanup_schedule.task_registered;
                if needs_task && !self.elevated {
                    self.status =
                        "Administrator required to create or update the cleanup scheduled task."
                            .into();
                    self.status_ok = false;
                    self.cleanup_schedule = cleanup_schedule::read_state();
                    return;
                }
                match cleanup_schedule::apply(&cfg) {
                    Ok(msg) => {
                        self.status = msg;
                        self.status_ok = true;
                        self.cleanup_schedule = cleanup_schedule::read_state();
                    }
                    Err(e) => {
                        self.status = format!("Cleanup schedule failed: {e}");
                        self.status_ok = false;
                        self.cleanup_schedule = cleanup_schedule::read_state();
                    }
                }
            }
            Pending::Quit => {
                self.force_quit = true;
                self.tray_enabled = false;
                self.tray = None;
                std::process::exit(0);
            }
            Pending::ClearLog(id) => {
                if !self.elevated {
                    self.status = "Administrator required to clear system logs.".into();
                    self.status_ok = false;
                    return;
                }
                let Some(action) = log_cleanup::ClearAction::find(id) else {
                    self.status = format!("Unknown clear target: {id}");
                    self.status_ok = false;
                    return;
                };
                match log_cleanup::clear(action) {
                    Ok(result) => {
                        self.status = result.summary_line();
                        self.last_clear_report = format!(
                            "{} — was: {}",
                            result.summary_line(),
                            result.before.summary_line()
                        );
                        self.status_ok = true;
                        self.clear_available = log_cleanup::availability_map();
                        self.refresh_log_status();
                        self.refresh_cleanup_history();
                    }
                    Err(e) => {
                        self.status = format!("Clear failed ({id}): {e}");
                        self.status_ok = false;
                    }
                }
            }
            Pending::ClearAllSafe => {
                if !self.elevated {
                    self.status = "Administrator required to clear system logs.".into();
                    self.status_ok = false;
                    return;
                }
                let results = log_cleanup::clear_all(false);
                let mut total_files = 0u64;
                let mut total_bytes = 0u64;
                let fails: Vec<_> = results
                    .iter()
                    .filter_map(|(id, r)| match r {
                        Ok(res) => {
                            total_files += res.removed_files;
                            total_bytes += res.freed_bytes;
                            None
                        }
                        Err(e) => Some(format!("{id}: {e}")),
                    })
                    .collect();
                let oks = results.iter().filter(|(_, r)| r.is_ok()).count();
                let report = format!(
                    "Cleared {oks} target(s): {} item(s), {}",
                    total_files,
                    log_cleanup::format_bytes(total_bytes)
                );
                if fails.is_empty() {
                    self.status = report.clone();
                    self.status_ok = true;
                } else {
                    self.status = format!("{report}; failures: {}", fails.join("; "));
                    self.status_ok = false;
                }
                self.last_clear_report = self.status.clone();
                self.clear_available = log_cleanup::availability_map();
                self.refresh_log_status();
                self.refresh_cleanup_history();
            }
            Pending::ClearAllLogs => {
                if !self.elevated {
                    self.status = "Administrator required to clear system logs.".into();
                    self.status_ok = false;
                    return;
                }
                let results = log_cleanup::clear_all(true);
                let mut total_files = 0u64;
                let mut total_bytes = 0u64;
                let fails: Vec<_> = results
                    .iter()
                    .filter_map(|(id, r)| match r {
                        Ok(res) => {
                            total_files += res.removed_files;
                            total_bytes += res.freed_bytes;
                            None
                        }
                        Err(e) => Some(format!("{id}: {e}")),
                    })
                    .collect();
                let oks = results.iter().filter(|(_, r)| r.is_ok()).count();
                let report = format!(
                    "Cleared ALL {oks} log target(s): {} item(s), {}",
                    total_files,
                    log_cleanup::format_bytes(total_bytes)
                );
                self.status = if fails.is_empty() {
                    report.clone()
                } else {
                    format!("{report}; failures: {}", fails.join("; "))
                };
                self.status_ok = fails.is_empty();
                self.last_clear_report = self.status.clone();
                self.clear_available = log_cleanup::availability_map();
                self.refresh_log_status();
                self.refresh_cleanup_history();
            }
            Pending::ClearTemp(id) => {
                let Some(target) = temp_cleanup::TempTarget::find(id) else {
                    self.status = format!("Unknown temp target: {id}");
                    self.status_ok = false;
                    return;
                };
                if target.needs_admin && !self.elevated {
                    self.status = "Administrator required for this temp target.".into();
                    self.status_ok = false;
                    return;
                }
                match temp_cleanup::clear(target) {
                    Ok(result) => {
                        self.status = result.summary_line();
                        self.last_temp_report = format!(
                            "{} — was: {}",
                            result.summary_line(),
                            result.before.summary_line()
                        );
                        self.status_ok = true;
                        self.temp_available = temp_cleanup::availability_map();
                        self.refresh_temp_status();
                        self.refresh_cleanup_history();
                    }
                    Err(e) => {
                        self.status = format!("Temp clear failed ({id}): {e}");
                        self.status_ok = false;
                    }
                }
            }
            Pending::ClearTempAll => {
                if temp_cleanup::ALL
                    .iter()
                    .any(|t| t.is_available() && t.needs_admin)
                    && !self.elevated
                {
                    self.status =
                        "Administrator required to clear all temp targets (includes Windows\\Temp)."
                            .into();
                    self.status_ok = false;
                    return;
                }
                let results = temp_cleanup::clear_all();
                let mut total_files = 0u64;
                let mut total_bytes = 0u64;
                let fails: Vec<_> = results
                    .iter()
                    .filter_map(|(id, r)| match r {
                        Ok(res) => {
                            total_files += res.removed_files;
                            total_bytes += res.freed_bytes;
                            None
                        }
                        Err(e) => Some(format!("{id}: {e}")),
                    })
                    .collect();
                let oks = results.iter().filter(|(_, r)| r.is_ok()).count();
                let report = format!(
                    "Cleared {oks} temp target(s): {} item(s), {}",
                    total_files,
                    log_cleanup::format_bytes(total_bytes)
                );
                self.status = if fails.is_empty() {
                    report.clone()
                } else {
                    format!("{report}; failures: {}", fails.join("; "))
                };
                self.status_ok = fails.is_empty();
                self.last_temp_report = self.status.clone();
                self.temp_available = temp_cleanup::availability_map();
                self.refresh_temp_status();
                self.refresh_cleanup_history();
            }
            Pending::One { id, active } => {
                if !self.elevated {
                    self.status = "Administrator required to change settings.".into();
                    self.status_ok = false;
                    return;
                }
                match telemetry::apply(id, active) {
                    Ok(msg) => {
                        self.status = format!(
                            "{} — {}  (CLI: tluw set {} {})",
                            id.title(),
                            msg,
                            id.cli_name(),
                            if active { "on" } else { "off" }
                        );
                        self.status_ok = true;
                    }
                    Err(e) => {
                        self.status = format!("Failed ({}): {e}", id.title());
                        self.status_ok = false;
                    }
                }
            }
            Pending::All { active } => {
                if !self.elevated {
                    self.status = "Administrator required to change settings.".into();
                    self.status_ok = false;
                    return;
                }
                let results = telemetry::apply_all(active);
                let fails: Vec<_> = results
                    .iter()
                    .filter_map(|(id, r)| r.as_ref().err().map(|e| format!("{}: {e}", id.title())))
                    .collect();
                if fails.is_empty() {
                    self.status = if active {
                        "All ON — same as `tluw enable`. Dashboard refreshed.".into()
                    } else {
                        "All OFF — same as `tluw disable`. Dashboard refreshed. Reboot recommended."
                            .into()
                    };
                    self.status_ok = true;
                } else {
                    self.status = format!("Some changes failed: {}", fails.join("; "));
                    self.status_ok = false;
                }
            }
        }
        self.refresh();
        self.refresh_cleanup_history();
        self.integration = maintenance::read_integration();
        if !self.hide_verify {
            self.verify_stamp = crate::win_cmd::local_stamp();
        }
    }

    pub(super) fn apply_tray_pref(&mut self, enabled: bool) {
        if let Err(e) = crate::prefs::set_tray_enabled(enabled) {
            self.status = format!("Could not save tray preference: {e}");
            self.status_ok = false;
            return;
        }
        if enabled {
            match self.try_create_tray() {
                Ok(()) => {
                    self.tray_enabled = true;
                    self.status =
                        "System tray icon shown — X / Minimize hide the window; Quit exits fully."
                            .into();
                    self.status_ok = true;
                }
                Err(e) => {
                    self.tray = None;
                    self.tray_enabled = false;
                    let _ = crate::prefs::set_tray_enabled(false);
                    self.status = format!("System tray unavailable: {e}");
                    self.status_ok = false;
                }
            }
        } else {
            self.tray = None;
            self.tray_enabled = false;
            crate::tray::win_hwnd::show(self.hwnd);
            self.status = "System tray disabled.".into();
            self.status_ok = true;
        }
    }

    pub(super) fn poll_tray_commands(&mut self) {
        let cmds = self
            .tray
            .as_ref()
            .map(|t| t.poll())
            .unwrap_or_default();
        for cmd in cmds {
            match cmd {
                crate::tray::TrayCommand::Show => {
                    crate::tray::win_hwnd::show(self.hwnd);
                    self.status = "Opened from system tray.".into();
                    self.status_ok = true;
                }
                crate::tray::TrayCommand::DisableTelemetry => {
                    crate::tray::win_hwnd::show(self.hwnd);
                    self.tray_action_report = true;
                    self.pending = Some(Pending::All { active: false });
                    self.status = "Disabling telemetry (from tray)…".into();
                    self.status_ok = true;
                }
                crate::tray::TrayCommand::ClearSafeLogs => {
                    crate::tray::win_hwnd::show(self.hwnd);
                    self.tray_action_report = true;
                    self.pending = Some(Pending::ClearAllSafe);
                    self.status = "Clearing safe logs (from tray)…".into();
                    self.status_ok = true;
                }
                crate::tray::TrayCommand::Quit => {
                    self.force_quit = true;
                    self.tray_enabled = false;
                    self.tray = None;
                    std::process::exit(0);
                }
            }
        }
    }
}
