//! Optional egui front-end. Enabled with the `gui` feature.

use crate::cleanup_history::{self, DayTotals};
use crate::disclaimer;
use crate::log_cleanup::{self, LogStatus};
use crate::maintenance::{self, IntegrationState};
use crate::prefs;
use crate::system_links;
use crate::telemetry::{self, SettingId, SettingState};
use crate::temp_cleanup::{self, TempStatus};
use crate::tray::{self, TrayCommand, TrayHandle};
use eframe::egui::{self, Color32, RichText, Sense, Ui};
use std::collections::HashMap;

pub struct DiagnosticsApp {
    elevated: bool,
    settings: Vec<SettingState>,
    expanded: [bool; SettingId::ALL.len()],
    status: String,
    status_ok: bool,
    pending: Option<Pending>,
    /// When true, show the live verification table of raw values.
    show_verify: bool,
    verify_stamp: String,
    integration: IntegrationState,
    /// Cached `system_links` availability (id → available).
    link_available: Vec<(&'static str, bool)>,
    /// Cached clear-action availability.
    clear_available: Vec<(&'static str, bool)>,
    /// Per-target size/count status for Logging section.
    log_status: HashMap<&'static str, LogStatus>,
    /// Two-step: first click arms; second confirms clear.
    clear_confirm: Option<&'static str>,
    /// Last clear result line shown in the Logging panel.
    last_clear_report: String,
    temp_available: Vec<(&'static str, bool)>,
    temp_status: HashMap<&'static str, TempStatus>,
    temp_confirm: Option<&'static str>,
    last_temp_report: String,
    /// Blocks the main UI until the user accepts (first run / re-show).
    show_disclaimer: bool,
    /// Close → tray instead of exit.
    tray_enabled: bool,
    tray: Option<TrayHandle>,
    hwnd: isize,
    force_quit: bool,
    /// Last N days of freed bytes (logs + temp).
    cleanup_days: Vec<DayTotals>,
}

enum Pending {
    One { id: SettingId, active: bool },
    All { active: bool },
    /// Re-read system values and open the verify panel.
    Verify,
    SetStartup(bool),
    SetPostUpdate(bool),
    ClearLog(&'static str),
    ClearAllSafe,
    ClearAllLogs,
    ClearTemp(&'static str),
    ClearTempAll,
}

impl DiagnosticsApp {
    pub fn new(hwnd: isize, ctx: egui::Context) -> Self {
        let elevated = telemetry::is_elevated();
        let settings = telemetry::read_all();
        let tray_enabled = prefs::tray_enabled();
        let mut app = Self {
            elevated,
            settings,
            expanded: [false; SettingId::ALL.len()],
            status: if elevated {
                "Ready. Use Verify status to re-read live registry/service values.".into()
            } else {
                "Not elevated — you can Verify status (read-only); changes need Administrator."
                    .into()
            },
            status_ok: true,
            pending: None,
            show_verify: false,
            verify_stamp: String::new(),
            integration: maintenance::read_integration(),
            link_available: system_links::availability_map(),
            clear_available: log_cleanup::availability_map(),
            log_status: HashMap::new(),
            clear_confirm: None,
            last_clear_report: String::new(),
            temp_available: temp_cleanup::availability_map(),
            temp_status: HashMap::new(),
            temp_confirm: None,
            last_temp_report: String::new(),
            show_disclaimer: !disclaimer::is_accepted(),
            tray_enabled,
            tray: None,
            hwnd,
            force_quit: false,
            cleanup_days: cleanup_history::daily_totals(14),
        };
        if tray_enabled {
            match tray::create(ctx) {
                Ok(handle) => app.tray = Some(handle),
                Err(e) => {
                    app.tray_enabled = false;
                    app.status = format!("System tray unavailable: {e}");
                    app.status_ok = false;
                    let _ = prefs::set_tray_enabled(false);
                }
            }
        }
        app
    }
}

impl eframe::App for DiagnosticsApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.poll_tray(ctx);

        // Minimize-to-tray: intercept close when tray is enabled.
        // Skip interception when quitting from the tray menu (`force_quit`).
        if ctx.input(|i| i.viewport().close_requested()) {
            if self.force_quit {
                // Allow the close through; also drop tray so the icon disappears.
                self.tray = None;
            } else if self.tray_enabled && self.tray.is_some() {
                ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
                tray::win_hwnd::hide(self.hwnd);
                self.status =
                    "Running in the system tray. Right-click the tray icon for quick actions."
                        .into();
                self.status_ok = true;
            }
        }

        if self.show_disclaimer {
            self.draw_disclaimer_gate(ctx);
            return;
        }

        if let Some(pending) = self.pending.take() {
            self.run_pending(pending);
        }

        egui::TopBottomPanel::top("header").show(ctx, |ui| {
            ui.add_space(8.0);
            ui.horizontal(|ui| {
                ui.heading(RichText::new("Windows Diagnostics").strong());
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if ui
                        .small_button(RichText::new("Quit").color(Color32::from_rgb(220, 140, 120)))
                        .on_hover_text("Exit the application completely")
                        .clicked()
                    {
                        self.force_quit = true;
                        self.tray_enabled = false;
                        self.tray = None;
                        std::process::exit(0);
                    }
                    if self.tray_enabled {
                        if ui
                            .small_button("Minimize to tray")
                            .on_hover_text("Hide the window; app stays in the system tray")
                            .clicked()
                        {
                            tray::win_hwnd::hide(self.hwnd);
                        }
                    }
                    if self.elevated {
                        ui.label(
                            RichText::new("Administrator")
                                .color(Color32::from_rgb(80, 180, 120))
                                .small(),
                        );
                    } else {
                        ui.label(
                            RichText::new("Not elevated (read-only)")
                                .color(Color32::from_rgb(220, 120, 80))
                                .small(),
                        );
                    }
                });
            });
            ui.label(
                RichText::new(
                    "GUI helper over the same commands as windows-diagnostics.exe \
                     (status / disable / enable / set). Each switch is ON = collecting.",
                )
                .weak()
                .small(),
            );
            ui.add_space(6.0);
            ui.separator();
        });

        egui::TopBottomPanel::bottom("footer").show(ctx, |ui| {
            ui.add_space(6.0);
            ui.horizontal_wrapped(|ui| {
                let color = if self.status_ok {
                    Color32::from_rgb(180, 200, 180)
                } else {
                    Color32::from_rgb(220, 140, 120)
                };
                ui.label(RichText::new(&self.status).color(color).small());
            });
            ui.add_space(4.0);
            ui.horizontal_wrapped(|ui| {
                ui.label(
                    RichText::new(disclaimer::SHORT)
                        .color(Color32::from_rgb(160, 140, 100))
                        .small(),
                );
                if ui
                    .small_button(RichText::new("Full disclaimer").small())
                    .on_hover_text("Show the full no-warranty / liability waiver again")
                    .clicked()
                {
                    self.show_disclaimer = true;
                }
                if let Some(rec) = disclaimer::read_record() {
                    if !rec.accepted_at.is_empty() {
                        ui.label(
                            RichText::new(format!("Accepted: {}", rec.accepted_at))
                                .weak()
                                .small()
                                .monospace(),
                        );
                    }
                }
            });
            ui.label(
                RichText::new(
                    "CLI: windows-diagnostics status | disable | enable | set <id> on|off | disclaimer",
                )
                .weak()
                .small()
                .monospace(),
            );
            ui.add_space(6.0);
        });

        egui::CentralPanel::default().show(ctx, |ui| {
            // One scroll region for the whole body — dashboard used to live in the
            // top panel (non-scrolling), which stole height and broke this ScrollArea.
            egui::ScrollArea::vertical()
                .id_salt("main_scroll")
                .auto_shrink([false, false])
                .show(ui, |ui| {
                    self.draw_dashboard(ui);
                    ui.add_space(10.0);

                    ui.horizontal_wrapped(|ui| {
                        if ui
                            .add(
                                egui::Button::new(
                                    RichText::new("Verify status")
                                        .strong()
                                        .color(Color32::WHITE),
                                )
                                .fill(Color32::from_rgb(60, 120, 180)),
                            )
                            .on_hover_text(
                                "Re-read every registry key, service, and task this app controls \
                                 and show the live values below",
                            )
                            .clicked()
                        {
                            self.pending = Some(Pending::Verify);
                        }

                        if self.elevated {
                            if ui
                                .button(
                                    RichText::new("Turn all OFF")
                                        .color(Color32::from_rgb(220, 160, 120)),
                                )
                                .on_hover_text("Same as: windows-diagnostics disable")
                                .clicked()
                            {
                                self.pending = Some(Pending::All { active: false });
                            }
                            if ui
                                .button("Turn all ON")
                                .on_hover_text("Same as: windows-diagnostics enable")
                                .clicked()
                            {
                                self.pending = Some(Pending::All { active: true });
                            }
                        } else if ui
                            .button("Restart as Administrator")
                            .on_hover_text("Required to change settings")
                            .clicked()
                        {
                            match telemetry::relaunch_elevated() {
                                Ok(()) => std::process::exit(0),
                                Err(e) => {
                                    self.status = e;
                                    self.status_ok = false;
                                }
                            }
                        }

                        ui.label(
                            RichText::new(
                                "OFF = privacy / blocked    ·    ON = collecting / allowed",
                            )
                            .weak()
                            .small(),
                        );
                    });

                    ui.add_space(8.0);
                    self.draw_integration_row(ui);
                    ui.add_space(8.0);

                    if self.show_verify {
                        self.draw_verify_panel(ui);
                        ui.add_space(10.0);
                    }

                    for (idx, state) in self.settings.clone().into_iter().enumerate() {
                        self.draw_setting_card(ui, idx, &state);
                        ui.add_space(8.0);
                    }

                    ui.add_space(12.0);
                    self.draw_clear_logs_row(ui);
                    ui.add_space(12.0);
                    self.draw_temp_cleanup_row(ui);
                    ui.add_space(12.0);
                    self.draw_system_links_row(ui);
                    ui.add_space(8.0);
                });
        });
    }
}

impl DiagnosticsApp {
    fn refresh(&mut self) {
        self.settings = telemetry::read_all();
    }

    fn run_verify(&mut self) {
        self.refresh();
        let off = self.settings.iter().filter(|s| !s.active).count();
        let on = self.settings.iter().filter(|s| s.active).count();
        self.verify_stamp = crate::win_cmd::local_stamp();
        self.show_verify = true;
        self.status = format!(
            "Verified at {} — {off} OFF (blocked), {on} ON (collecting). Values below are live from the system.",
            self.verify_stamp
        );
        self.status_ok = true;
    }

    fn run_pending(&mut self, pending: Pending) {
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
                            "{} — {}  (CLI: windows-diagnostics set {} {})",
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
                        "All ON — same as `windows-diagnostics enable`. Click Verify status to confirm."
                            .into()
                    } else {
                        "All OFF — same as `windows-diagnostics disable`. Click Verify status to confirm. Reboot recommended."
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
        self.integration = maintenance::read_integration();
        if self.show_verify {
            self.verify_stamp = crate::win_cmd::local_stamp();
        }
    }

    fn draw_disclaimer_gate(&mut self, ctx: &egui::Context) {
        let already = disclaimer::is_accepted();
        egui::CentralPanel::default().show(ctx, |ui| {
            ui.vertical_centered(|ui| {
                ui.add_space(24.0);
                ui.heading(RichText::new("Disclaimer & liability waiver").strong());
                ui.add_space(8.0);
                ui.label(
                    RichText::new(disclaimer::SHORT)
                        .color(Color32::from_rgb(200, 160, 90))
                        .strong(),
                );
                ui.add_space(12.0);
            });

            egui::ScrollArea::vertical()
                .max_height(ui.available_height() - 80.0)
                .id_salt("disclaimer_scroll")
                .show(ui, |ui| {
                    ui.add(
                        egui::Label::new(RichText::new(disclaimer::FULL).small().monospace())
                            .wrap(),
                    );
                });

            ui.add_space(12.0);
            ui.horizontal(|ui| {
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if already {
                        if ui
                            .add(
                                egui::Button::new(RichText::new("Close").strong())
                                    .min_size(egui::vec2(120.0, 32.0)),
                            )
                            .clicked()
                        {
                            self.show_disclaimer = false;
                        }
                    } else {
                        if ui
                            .add(
                                egui::Button::new(
                                    RichText::new("I understand and accept — continue")
                                        .strong()
                                        .color(Color32::WHITE),
                                )
                                .fill(Color32::from_rgb(70, 120, 90))
                                .min_size(egui::vec2(280.0, 32.0)),
                            )
                            .clicked()
                        {
                        match disclaimer::accept("gui") {
                            Ok(rec) => {
                                self.show_disclaimer = false;
                                self.status = format!(
                                    "Disclaimer accepted at {} (HKCU registry + APPDATA backup). Use at your own risk.",
                                    rec.accepted_at
                                );
                                self.status_ok = true;
                            }
                            Err(e) => {
                                self.show_disclaimer = false;
                                self.status = format!(
                                    "Accepted (could not save marker: {e}). Use at your own risk."
                                );
                                self.status_ok = false;
                            }
                        }
                        }
                        if ui
                            .button(RichText::new("Quit").color(Color32::from_rgb(220, 140, 120)))
                            .clicked()
                        {
                            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                        }
                    }
                });
            });
        });
    }

    fn draw_integration_row(&mut self, ui: &mut Ui) {
        egui::Frame::group(ui.style())
            .inner_margin(10.0)
            .corner_radius(6.0)
            .show(ui, |ui| {
                ui.label(RichText::new("Automation").strong());
                ui.label(
                    RichText::new(
                        "Optional: open the app at logon, and re-run lockdown after Windows Update \
                         (Event ID 19) with a logon backup.",
                    )
                    .small()
                    .weak(),
                );
                ui.add_space(4.0);

                let mut startup = self.integration.run_at_startup;
                if ui
                    .checkbox(&mut startup, "Run GUI when Windows starts")
                    .changed()
                {
                    self.pending = Some(Pending::SetStartup(startup));
                }

                let mut post = self.integration.post_update;
                let post_resp = ui.checkbox(
                    &mut post,
                    "Re-apply lockdown after Windows Update (scheduled task)",
                );
                if post_resp.changed() {
                    self.pending = Some(Pending::SetPostUpdate(post));
                }
                if !self.elevated {
                    ui.label(
                        RichText::new("Post-update task requires Administrator.")
                            .small()
                            .color(Color32::from_rgb(220, 140, 100)),
                    );
                }

                ui.add_space(6.0);
                ui.separator();
                ui.add_space(4.0);
                let mut tray_on = self.tray_enabled;
                let tray_resp = ui
                    .checkbox(&mut tray_on, "Keep in system tray (close hides window)")
                    .on_hover_text(
                        "Shows a tray icon with quick actions: Open, Disable telemetry, Clear safe logs, Quit",
                    );
                if tray_resp.changed() {
                    self.apply_tray_pref(tray_on, ui.ctx());
                }
                if self.tray_enabled {
                    ui.label(
                        RichText::new(
                            "Tray menu: Open dashboard · Disable telemetry · Clear safe logs · Quit",
                        )
                        .small()
                        .weak(),
                    );
                }
            });
    }

    fn apply_tray_pref(&mut self, enabled: bool, ctx: &egui::Context) {
        if let Err(e) = prefs::set_tray_enabled(enabled) {
            self.status = format!("Could not save tray preference: {e}");
            self.status_ok = false;
            return;
        }
        if enabled {
            match tray::create(ctx.clone()) {
                Ok(handle) => {
                    self.tray = Some(handle);
                    self.tray_enabled = true;
                    self.status =
                        "System tray enabled — close the window or use Minimize to tray.".into();
                    self.status_ok = true;
                }
                Err(e) => {
                    self.tray = None;
                    self.tray_enabled = false;
                    let _ = prefs::set_tray_enabled(false);
                    self.status = format!("System tray unavailable: {e}");
                    self.status_ok = false;
                }
            }
        } else {
            self.tray = None;
            self.tray_enabled = false;
            tray::win_hwnd::show(self.hwnd);
            self.status = "System tray disabled.".into();
            self.status_ok = true;
        }
    }

    fn poll_tray(&mut self, ctx: &egui::Context) {
        let cmds = self
            .tray
            .as_ref()
            .map(|t| t.poll())
            .unwrap_or_default();
        for cmd in cmds {
            match cmd {
                TrayCommand::Show => {
                    tray::win_hwnd::show(self.hwnd);
                    ctx.request_repaint();
                }
                TrayCommand::DisableTelemetry => {
                    self.pending = Some(Pending::All { active: false });
                    tray::win_hwnd::show(self.hwnd);
                }
                TrayCommand::ClearSafeLogs => {
                    self.pending = Some(Pending::ClearAllSafe);
                    tray::win_hwnd::show(self.hwnd);
                }
                TrayCommand::Quit => {
                    // Backup path if Quit ever arrives via the channel.
                    self.force_quit = true;
                    self.tray_enabled = false;
                    self.tray = None;
                    std::process::exit(0);
                }
            }
        }
    }

    fn refresh_cleanup_history(&mut self) {
        self.cleanup_days = cleanup_history::daily_totals(14);
    }

    fn draw_telemetry_chip(ui: &mut Ui, state: &SettingState) {
        let collecting = state.active;
        let badge_bg = if collecting {
            Color32::from_rgb(72, 48, 36)
        } else {
            Color32::from_rgb(32, 58, 48)
        };
        let badge_fg = if collecting {
            Color32::from_rgb(235, 160, 100)
        } else {
            Color32::from_rgb(110, 200, 155)
        };
        let card_bg = Color32::from_rgb(36, 42, 50);
        let dot = if collecting {
            Color32::from_rgb(230, 140, 80)
        } else {
            Color32::from_rgb(90, 190, 145)
        };
        let badge = if collecting { "ON" } else { "OFF" };

        let tip = format!(
            "{}\n{}\n{}",
            state.id.title(),
            if collecting {
                "Currently collecting / allowed"
            } else {
                "Blocked / privacy setting applied"
            },
            state.note
        );

        // Fixed width so horizontal_wrapped can place chips side-by-side.
        // (Using available_width() here forces each chip onto its own row.)
        const CHIP_W: f32 = 200.0;

        egui::Frame::new()
            .fill(card_bg)
            .corner_radius(8.0)
            .inner_margin(egui::Margin::symmetric(10, 8))
            .stroke(egui::Stroke::new(1.0_f32, Color32::from_rgb(52, 60, 70)))
            .show(ui, |ui| {
                ui.set_width(CHIP_W);
                ui.horizontal(|ui| {
                    egui::Frame::new()
                        .fill(badge_bg)
                        .corner_radius(5.0)
                        .inner_margin(egui::Margin::symmetric(8, 4))
                        .show(ui, |ui| {
                            ui.horizontal(|ui| {
                                ui.spacing_mut().item_spacing.x = 5.0;
                                let (resp, painter) =
                                    ui.allocate_painter(egui::vec2(8.0, 8.0), Sense::hover());
                                painter.circle_filled(resp.rect.center(), 3.5, dot);
                                ui.label(RichText::new(badge).small().strong().color(badge_fg));
                            });
                        });

                    ui.add_space(6.0);
                    ui.vertical(|ui| {
                        ui.set_max_width(CHIP_W - 64.0);
                        ui.label(
                            RichText::new(state.id.short_title())
                                .size(12.5)
                                .strong()
                                .color(Color32::from_rgb(220, 225, 230)),
                        );
                        ui.label(
                            RichText::new(state.id.cli_name())
                                .small()
                                .monospace()
                                .color(Color32::from_rgb(130, 140, 150)),
                        );
                    });
                });
            })
            .response
            .on_hover_text(tip);
    }

    fn draw_dashboard(&mut self, ui: &mut Ui) {
        egui::Frame::group(ui.style())
            .inner_margin(10.0)
            .corner_radius(6.0)
            .fill(Color32::from_rgb(28, 32, 38))
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.label(RichText::new("Dashboard").strong().size(15.0));
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if ui
                            .small_button("Refresh")
                            .on_hover_text("Re-read telemetry status and cleanup history")
                            .clicked()
                        {
                            self.refresh();
                            self.refresh_cleanup_history();
                        }
                    });
                });

                ui.add_space(6.0);
                ui.label(RichText::new("Telemetry status").small().strong());

                let off = self.settings.iter().filter(|s| !s.active).count();
                let on = self.settings.iter().filter(|s| s.active).count();
                let total = self.settings.len().max(1);
                let blocked_frac = off as f32 / total as f32;

                ui.add_space(4.0);
                let avail = ui.available_width();
                ui.horizontal_wrapped(|ui| {
                    let bar_w = (avail - 12.0).clamp(100.0, 280.0);
                    let bar_h = 10.0;
                    let (resp, painter) =
                        ui.allocate_painter(egui::vec2(bar_w, bar_h), Sense::hover());
                    let rect = resp.rect;
                    painter.rect_filled(rect, 4.0, Color32::from_rgb(42, 48, 56));
                    if blocked_frac > 0.0 {
                        let mut fill = rect;
                        fill.set_width(rect.width() * blocked_frac);
                        let fill_color = if on == 0 {
                            Color32::from_rgb(90, 180, 140)
                        } else if off == 0 {
                            Color32::from_rgb(200, 120, 70)
                        } else {
                            Color32::from_rgb(120, 170, 150)
                        };
                        painter.rect_filled(fill, 4.0, fill_color);
                    }
                    let summary = if on == 0 {
                        format!("{off}/{total} blocked — all quiet")
                    } else if off == 0 {
                        format!("{on}/{total} collecting — fully open")
                    } else {
                        format!("{off}/{total} blocked · {on} still collecting")
                    };
                    ui.add_space(8.0);
                    ui.label(
                        RichText::new(summary)
                            .small()
                            .color(if on == 0 {
                                Color32::from_rgb(120, 200, 160)
                            } else if off == 0 {
                                Color32::from_rgb(220, 150, 100)
                            } else {
                                Color32::from_rgb(190, 190, 170)
                            }),
                    );
                    resp.on_hover_text(
                        "Blocked = telemetry setting OFF. Collecting = setting still ON.",
                    );
                });

                ui.add_space(8.0);
                let settings = self.settings.clone();
                ui.horizontal_wrapped(|ui| {
                    ui.spacing_mut().item_spacing = egui::vec2(8.0, 8.0);
                    for s in &settings {
                        Self::draw_telemetry_chip(ui, s);
                    }
                });

                ui.add_space(8.0);
                ui.separator();
                ui.add_space(4.0);

                let (life_logs, life_temp) = cleanup_history::lifetime_totals();
                let today = cleanup_history::today_totals();
                ui.horizontal_wrapped(|ui| {
                    ui.label(RichText::new("Cleanup freed").small().strong());
                    ui.label(
                        RichText::new(format!(
                            "today  logs {} · temp {} · total {}",
                            cleanup_history::format_size(today.logs_bytes),
                            cleanup_history::format_size(today.temp_bytes),
                            cleanup_history::format_size(today.total_bytes()),
                        ))
                        .small()
                        .color(Color32::from_rgb(160, 200, 180))
                        .monospace(),
                    );
                });
                ui.horizontal_wrapped(|ui| {
                    ui.label(
                        RichText::new(format!(
                            "lifetime  logs {} · temp {} · total {}",
                            cleanup_history::format_size(life_logs),
                            cleanup_history::format_size(life_temp),
                            cleanup_history::format_size(life_logs.saturating_add(life_temp)),
                        ))
                        .small()
                        .weak()
                        .monospace(),
                    );
                });

                ui.add_space(4.0);
                // Always draw the chart (including 0 days) so clears are visible immediately.
                let max = self
                    .cleanup_days
                    .iter()
                    .map(|d| d.total_bytes())
                    .max()
                    .unwrap_or(0)
                    .max(1);
                let day_count = self.cleanup_days.len().max(1) as f32;
                let bar_slot = ((ui.available_width() - 4.0) / day_count).clamp(18.0, 40.0);
                ui.horizontal(|ui| {
                    for day in &self.cleanup_days {
                        ui.vertical(|ui| {
                            ui.set_width(bar_slot);
                            let h = 48.0;
                            let frac = day.total_bytes() as f32 / max as f32;
                            let bar_h = (frac * h).max(if day.total_bytes() > 0 {
                                3.0
                            } else {
                                0.0
                            });
                            let bar_w = (bar_slot - 8.0).max(10.0);
                            let (resp, painter) =
                                ui.allocate_painter(egui::vec2(bar_w, h), Sense::hover());
                            let rect = resp.rect;
                            painter.rect_filled(rect, 2.0, Color32::from_rgb(40, 44, 52));
                            if bar_h > 0.0 {
                                let logs_frac = if day.total_bytes() > 0 {
                                    day.logs_bytes as f32 / day.total_bytes() as f32
                                } else {
                                    0.0
                                };
                                let logs_h = bar_h * logs_frac;
                                let temp_h = bar_h - logs_h;
                                let top = rect.bottom() - bar_h;
                                if temp_h > 0.0 {
                                    painter.rect_filled(
                                        egui::Rect::from_min_max(
                                            egui::pos2(rect.left() + 2.0, top),
                                            egui::pos2(rect.right() - 2.0, top + temp_h),
                                        ),
                                        2.0,
                                        Color32::from_rgb(90, 140, 200),
                                    );
                                }
                                if logs_h > 0.0 {
                                    painter.rect_filled(
                                        egui::Rect::from_min_max(
                                            egui::pos2(rect.left() + 2.0, top + temp_h),
                                            egui::pos2(rect.right() - 2.0, rect.bottom()),
                                        ),
                                        2.0,
                                        Color32::from_rgb(90, 180, 140),
                                    );
                                }
                            }
                            let label = if day.date.len() >= 10 {
                                &day.date[5..]
                            } else {
                                day.date.as_str()
                            };
                            ui.label(RichText::new(label).small().weak());
                            // Always show the size under the bar (B/KB/MB/GB).
                            ui.label(
                                RichText::new(cleanup_history::format_size(day.total_bytes()))
                                    .small()
                                    .monospace()
                                    .color(if day.total_bytes() > 0 {
                                        Color32::from_rgb(180, 200, 190)
                                    } else {
                                        Color32::from_rgb(90, 95, 105)
                                    }),
                            );
                            resp.on_hover_text(format!(
                                "{} — logs {} · temp {} · total {}",
                                day.date,
                                cleanup_history::format_size(day.logs_bytes),
                                cleanup_history::format_size(day.temp_bytes),
                                cleanup_history::format_size(day.total_bytes()),
                            ));
                        });
                    }
                });
                if self.cleanup_days.iter().all(|d| d.total_bytes() == 0) {
                    ui.label(
                        RichText::new(
                            "No bytes recorded yet — clear logs or temp files; totals use B / KB / MB / GB.",
                        )
                        .small()
                        .weak(),
                    );
                } else {
                    ui.label(
                        RichText::new("Green = logs · Blue = temp · sizes under each day")
                            .small()
                            .weak(),
                    );
                }
            });
    }

    fn refresh_log_status(&mut self) {
        self.log_status.clear();
        for (action, st) in log_cleanup::inspect_all() {
            self.log_status.insert(action.id, st);
        }
    }

    fn draw_clear_logs_row(&mut self, ui: &mut Ui) {
        egui::Frame::group(ui.style())
            .inner_margin(10.0)
            .corner_radius(6.0)
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.label(RichText::new("Logging").strong());
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if ui
                            .small_button("Refresh status")
                            .on_hover_text("Count files / bytes (or event records) for each target")
                            .clicked()
                        {
                            self.clear_available = log_cleanup::availability_map();
                            self.refresh_log_status();
                            let total_files: u64 =
                                self.log_status.values().map(|s| s.files).sum();
                            let total_bytes: u64 =
                                self.log_status.values().map(|s| s.bytes).sum();
                            self.status = format!(
                                "Logging status: {} target(s), {} item(s)/records, {}",
                                self.log_status.len(),
                                total_files,
                                log_cleanup::format_bytes(total_bytes)
                            );
                            self.status_ok = true;
                        }
                    });
                });
                ui.label(
                    RichText::new(
                        "Use 📂 to open the log location. Click a clear button once to arm, again to confirm. \
                         Locked files are skipped. CLI: open-log <id> | clear <id> --confirm",
                    )
                    .small()
                    .weak(),
                );
                if !self.elevated {
                    ui.label(
                        RichText::new(
                            "Opening locations works without admin; clearing needs Administrator.",
                        )
                        .small()
                        .color(Color32::from_rgb(220, 140, 100)),
                    );
                }
                if let Some(id) = self.clear_confirm {
                    if let Some(st) = self.log_status.get(id) {
                        ui.label(
                            RichText::new(format!("Armed: {}", st.summary_line()))
                                .small()
                                .color(Color32::from_rgb(180, 200, 220)),
                        );
                    }
                    ui.horizontal(|ui| {
                        ui.label(
                            RichText::new(format!(
                                "Confirm clear for '{id}', or Cancel."
                            ))
                            .small()
                            .color(Color32::from_rgb(220, 180, 100)),
                        );
                        if ui.small_button("Cancel").clicked() {
                            self.clear_confirm = None;
                        }
                    });
                }
                if !self.last_clear_report.is_empty() {
                    ui.label(
                        RichText::new(&self.last_clear_report)
                            .small()
                            .color(Color32::from_rgb(120, 190, 140)),
                    );
                }

                // Status table
                if !self.log_status.is_empty() {
                    ui.add_space(4.0);
                    ui.separator();
                    egui::Grid::new("logging_status_grid")
                        .num_columns(3)
                        .striped(true)
                        .spacing([10.0, 4.0])
                        .show(ui, |ui| {
                            ui.label(RichText::new("Target").strong().small());
                            ui.label(RichText::new("Count").strong().small());
                            ui.label(RichText::new("Size").strong().small());
                            ui.end_row();
                            for action in log_cleanup::ALL {
                                if let Some(st) = self.log_status.get(action.id) {
                                    ui.label(RichText::new(action.id).small().monospace());
                                    ui.label(
                                        RichText::new(format!("{}", st.files)).small().monospace(),
                                    );
                                    ui.label(
                                        RichText::new(log_cleanup::format_bytes(st.bytes))
                                            .small()
                                            .monospace(),
                                    );
                                    ui.end_row();
                                }
                            }
                        });
                    ui.separator();
                }

                ui.add_space(6.0);
                ui.horizontal_wrapped(|ui| {
                    for action in log_cleanup::ALL {
                        let available = self
                            .clear_available
                            .iter()
                            .find(|(id, _)| *id == action.id)
                            .map(|(_, a)| *a)
                            .unwrap_or_else(|| action.is_available());

                        let armed = self.clear_confirm == Some(action.id);

                        ui.horizontal(|ui| {
                            ui.spacing_mut().item_spacing.x = 4.0;

                            // Open-location control (does not arm/clear)
                            let open_btn = egui::Button::new(RichText::new("📂").small())
                                .min_size(egui::vec2(22.0, 18.0));
                            let open_resp = ui
                                .add_enabled(available, open_btn)
                                .on_hover_text(if available {
                                    format!(
                                        "Open location — {} ({})",
                                        action.description, action.id
                                    )
                                } else {
                                    format!("Not available — {}", action.description)
                                });
                            if available && open_resp.clicked() {
                                let st = log_cleanup::inspect(action);
                                self.log_status.insert(action.id, st.clone());
                                match log_cleanup::open_location(action) {
                                    Ok(msg) => {
                                        self.status =
                                            format!("{msg} | {}", st.summary_line());
                                        self.status_ok = true;
                                    }
                                    Err(e) => {
                                        self.status = format!(
                                            "Open failed ({e}). Status: {}",
                                            st.summary_line()
                                        );
                                        self.status_ok = false;
                                    }
                                }
                            }

                            let label = if armed {
                                format!("Confirm: {}", action.title)
                            } else {
                                action.title.to_string()
                            };

                            let mut clear_btn = egui::Button::new(RichText::new(label).small());
                            if armed {
                                clear_btn = clear_btn.fill(Color32::from_rgb(160, 70, 50));
                            } else if action.dangerous {
                                clear_btn = clear_btn.fill(Color32::from_rgb(90, 55, 50));
                            }
                            if !available {
                                clear_btn = clear_btn
                                    .fill(Color32::from_rgb(55, 55, 60))
                                    .sense(egui::Sense::hover());
                            }

                            let clear_enabled = available && self.elevated;
                            let clear_tip = if !available {
                                format!("Not available — {}", action.description)
                            } else if !self.elevated {
                                format!("Needs Administrator — {}", action.description)
                            } else if armed {
                                format!(
                                    "Click again to confirm clear — {}",
                                    self.log_status
                                        .get(action.id)
                                        .map(|s| s.summary_line())
                                        .unwrap_or_else(|| action.description.to_string())
                                )
                            } else {
                                format!(
                                    "Arm clear (confirm on second click) — {} ({})",
                                    action.description, action.id
                                )
                            };

                            let clear_resp = ui
                                .add_enabled(clear_enabled, clear_btn)
                                .on_hover_text(clear_tip);
                            if clear_enabled && clear_resp.clicked() {
                                if self.clear_confirm == Some(action.id) {
                                    self.clear_confirm = None;
                                    self.pending = Some(Pending::ClearLog(action.id));
                                } else {
                                    let st = log_cleanup::inspect(action);
                                    self.log_status.insert(action.id, st.clone());
                                    self.clear_confirm = Some(action.id);
                                    self.status = format!(
                                        "Armed '{}'. {} — click again to confirm clear.",
                                        action.id,
                                        st.summary_line()
                                    );
                                    self.status_ok = true;
                                }
                            }
                        });
                    }
                });

                ui.add_space(6.0);
                ui.horizontal_wrapped(|ui| {
                    let clear_all_enabled = self.elevated;

                    let armed_safe = self.clear_confirm == Some("__all_safe__");
                    let safe_label = if armed_safe {
                        "Confirm: clear all safe logs"
                    } else {
                        "Clear all safe logs"
                    };
                    let mut safe_btn = egui::Button::new(safe_label);
                    if armed_safe {
                        safe_btn = safe_btn.fill(Color32::from_rgb(160, 70, 50));
                    }
                    let safe_resp = ui.add_enabled(clear_all_enabled, safe_btn).on_hover_text(
                        "Clears available non-dangerous targets (not Diagnosis wipe / Security). \
                         CLI: clear-all --confirm",
                    );
                    if clear_all_enabled && safe_resp.clicked() {
                        if armed_safe {
                            self.clear_confirm = None;
                            self.pending = Some(Pending::ClearAllSafe);
                        } else {
                            self.refresh_log_status();
                            self.clear_confirm = Some("__all_safe__");
                            let total_bytes: u64 =
                                self.log_status.values().map(|s| s.bytes).sum();
                            self.status = format!(
                                "Armed clear-all (safe). Current ~{}. Click again to confirm.",
                                log_cleanup::format_bytes(total_bytes)
                            );
                            self.status_ok = true;
                        }
                    }

                    let armed_all = self.clear_confirm == Some("__all_logs__");
                    let all_label = if armed_all {
                        "Confirm: clear ALL logs"
                    } else {
                        "Clear all logs"
                    };
                    let mut all_btn = egui::Button::new(RichText::new(all_label).color(Color32::WHITE));
                    all_btn = if armed_all {
                        all_btn.fill(Color32::from_rgb(180, 50, 40))
                    } else {
                        all_btn.fill(Color32::from_rgb(120, 55, 45))
                    };
                    let all_resp = ui.add_enabled(clear_all_enabled, all_btn).on_hover_text(
                        "Clears every available log target including Diagnosis wipe and Security. \
                         CLI: clear-all --dangerous --confirm",
                    );
                    if clear_all_enabled && all_resp.clicked() {
                        if armed_all {
                            self.clear_confirm = None;
                            self.pending = Some(Pending::ClearAllLogs);
                        } else {
                            self.refresh_log_status();
                            self.clear_confirm = Some("__all_logs__");
                            let total_bytes: u64 =
                                self.log_status.values().map(|s| s.bytes).sum();
                            self.status = format!(
                                "Armed CLEAR ALL LOGS (incl. dangerous). Current ~{}. Click again to confirm.",
                                log_cleanup::format_bytes(total_bytes)
                            );
                            self.status_ok = true;
                        }
                    }
                });
            });
    }

    fn refresh_temp_status(&mut self) {
        self.temp_status.clear();
        for (target, st) in temp_cleanup::inspect_all() {
            self.temp_status.insert(target.id, st);
        }
    }

    fn draw_temp_cleanup_row(&mut self, ui: &mut Ui) {
        egui::Frame::group(ui.style())
            .inner_margin(10.0)
            .corner_radius(6.0)
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.label(RichText::new("Temporary files").strong());
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if ui
                            .small_button("Refresh status")
                            .on_hover_text("Count files / bytes in each temp location")
                            .clicked()
                        {
                            self.temp_available = temp_cleanup::availability_map();
                            self.refresh_temp_status();
                            let total_files: u64 =
                                self.temp_status.values().map(|s| s.files).sum();
                            let total_bytes: u64 =
                                self.temp_status.values().map(|s| s.bytes).sum();
                            self.status = format!(
                                "Temp status: {} location(s), {} file(s), {}",
                                self.temp_status.len(),
                                total_files,
                                log_cleanup::format_bytes(total_bytes)
                            );
                            self.status_ok = true;
                        }
                    });
                });
                ui.label(
                    RichText::new(
                        "Use 📂 to open a temp folder. Clear buttons use confirm (click twice). \
                         Reports files + space freed after clear. CLI: temp-list | clear-temp <id> --confirm",
                    )
                    .small()
                    .weak(),
                );
                if let Some(id) = self.temp_confirm {
                    if let Some(st) = self.temp_status.get(id) {
                        ui.label(
                            RichText::new(format!("Armed: {}", st.summary_line()))
                                .small()
                                .color(Color32::from_rgb(180, 200, 220)),
                        );
                    }
                    ui.horizontal(|ui| {
                        ui.label(
                            RichText::new(format!("Confirm clear for '{id}', or Cancel."))
                                .small()
                                .color(Color32::from_rgb(220, 180, 100)),
                        );
                        if ui.small_button("Cancel").clicked() {
                            self.temp_confirm = None;
                        }
                    });
                }
                if !self.last_temp_report.is_empty() {
                    ui.label(
                        RichText::new(&self.last_temp_report)
                            .small()
                            .color(Color32::from_rgb(120, 190, 140)),
                    );
                }

                if !self.temp_status.is_empty() {
                    ui.add_space(4.0);
                    ui.separator();
                    egui::Grid::new("temp_status_grid")
                        .num_columns(3)
                        .striped(true)
                        .spacing([10.0, 4.0])
                        .show(ui, |ui| {
                            ui.label(RichText::new("Target").strong().small());
                            ui.label(RichText::new("Files").strong().small());
                            ui.label(RichText::new("Size").strong().small());
                            ui.end_row();
                            for target in temp_cleanup::ALL {
                                if let Some(st) = self.temp_status.get(target.id) {
                                    ui.label(RichText::new(target.id).small().monospace());
                                    ui.label(
                                        RichText::new(format!("{}", st.files)).small().monospace(),
                                    );
                                    ui.label(
                                        RichText::new(log_cleanup::format_bytes(st.bytes))
                                            .small()
                                            .monospace(),
                                    );
                                    ui.end_row();
                                }
                            }
                        });
                    ui.separator();
                }

                ui.add_space(6.0);
                ui.horizontal_wrapped(|ui| {
                    for target in temp_cleanup::ALL {
                        let available = self
                            .temp_available
                            .iter()
                            .find(|(id, _)| *id == target.id)
                            .map(|(_, a)| *a)
                            .unwrap_or_else(|| target.is_available());
                        // Hide duplicates / missing locations entirely (e.g. TMP == TEMP).
                        if !available {
                            continue;
                        }
                        let armed = self.temp_confirm == Some(target.id);

                        ui.horizontal(|ui| {
                            ui.spacing_mut().item_spacing.x = 4.0;

                            let open_btn = egui::Button::new(RichText::new("📂").small())
                                .min_size(egui::vec2(22.0, 18.0));
                            let open_resp = ui.add(open_btn).on_hover_text(format!(
                                "Open {} — {}",
                                target.title, target.description
                            ));
                            if open_resp.clicked() {
                                let st = temp_cleanup::inspect(target);
                                self.temp_status.insert(target.id, st.clone());
                                match temp_cleanup::open_location(target) {
                                    Ok(msg) => {
                                        self.status =
                                            format!("{msg} | {}", st.summary_line());
                                        self.status_ok = true;
                                    }
                                    Err(e) => {
                                        self.status = format!(
                                            "Open failed ({e}). Status: {}",
                                            st.summary_line()
                                        );
                                        self.status_ok = false;
                                    }
                                }
                            }

                            let label = if armed {
                                format!("Confirm: {}", target.title)
                            } else {
                                target.title.to_string()
                            };
                            let mut clear_btn = egui::Button::new(RichText::new(label).small());
                            if armed {
                                clear_btn = clear_btn.fill(Color32::from_rgb(160, 70, 50));
                            }

                            let clear_enabled = !target.needs_admin || self.elevated;
                            let tip = if target.needs_admin && !self.elevated {
                                format!("Needs Administrator — {}", target.description)
                            } else if armed {
                                format!(
                                    "Click again to confirm — {}",
                                    self.temp_status
                                        .get(target.id)
                                        .map(|s| s.summary_line())
                                        .unwrap_or_else(|| target.description.to_string())
                                )
                            } else {
                                format!(
                                    "Arm clear (confirm on second click) — {} ({})",
                                    target.description, target.id
                                )
                            };

                            let clear_resp = ui
                                .add_enabled(clear_enabled, clear_btn)
                                .on_hover_text(tip);
                            if clear_enabled && clear_resp.clicked() {
                                if self.temp_confirm == Some(target.id) {
                                    self.temp_confirm = None;
                                    self.pending = Some(Pending::ClearTemp(target.id));
                                } else {
                                    let st = temp_cleanup::inspect(target);
                                    self.temp_status.insert(target.id, st.clone());
                                    self.temp_confirm = Some(target.id);
                                    self.status = format!(
                                        "Armed '{}'. {} — click again to confirm clear.",
                                        target.id,
                                        st.summary_line()
                                    );
                                    self.status_ok = true;
                                }
                            }
                        });
                    }
                });

                ui.add_space(6.0);
                let armed_all = self.temp_confirm == Some("__all_temp__");
                let needs_admin = temp_cleanup::ALL
                    .iter()
                    .any(|t| t.is_available() && t.needs_admin);
                let all_enabled = !needs_admin || self.elevated;
                let all_label = if armed_all {
                    "Confirm: clear all temp folders"
                } else {
                    "Clear all temporary files"
                };
                let mut all_btn = egui::Button::new(all_label);
                if armed_all {
                    all_btn = all_btn.fill(Color32::from_rgb(160, 70, 50));
                }
                let all_resp = ui.add_enabled(all_enabled, all_btn).on_hover_text(
                    "Clears every available temp target (TEMP, Windows\\Temp, Prefetch, …). \
                     CLI: clear-temp-all --confirm",
                );
                if all_enabled && all_resp.clicked() {
                    if armed_all {
                        self.temp_confirm = None;
                        self.pending = Some(Pending::ClearTempAll);
                    } else {
                        self.refresh_temp_status();
                        self.temp_confirm = Some("__all_temp__");
                        let total_bytes: u64 =
                            self.temp_status.values().map(|s| s.bytes).sum();
                        self.status = format!(
                            "Armed clear-all temp. Current ~{}. Click again to confirm.",
                            log_cleanup::format_bytes(total_bytes)
                        );
                        self.status_ok = true;
                    }
                }
            });
    }

    fn draw_system_links_row(&mut self, ui: &mut Ui) {
        egui::Frame::group(ui.style())
            .inner_margin(10.0)
            .corner_radius(6.0)
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.label(RichText::new("Windows logs & tools").strong());
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if ui
                            .small_button("Refresh availability")
                            .on_hover_text("Re-check which tools/folders exist on this PC")
                            .clicked()
                        {
                            self.link_available = system_links::availability_map();
                            let n = self.link_available.iter().filter(|(_, a)| *a).count();
                            self.status = format!(
                                "Log tools: {n}/{} available on this system.",
                                system_links::ALL.len()
                            );
                            self.status_ok = true;
                        }
                    });
                });
                ui.label(
                    RichText::new(
                        "Open built-in viewers and folders. Unavailable items are grayed out. \
                         CLI: windows-diagnostics logs | open <id>",
                    )
                    .small()
                    .weak(),
                );
                ui.add_space(6.0);
                ui.horizontal_wrapped(|ui| {
                    for link in system_links::ALL {
                        let available = self
                            .link_available
                            .iter()
                            .find(|(id, _)| *id == link.id)
                            .map(|(_, a)| *a)
                            .unwrap_or_else(|| link.is_available());

                        let mut btn = egui::Button::new(link.title);
                        if !available {
                            btn = btn
                                .fill(Color32::from_rgb(55, 55, 60))
                                .sense(egui::Sense::hover());
                        }
                        let resp = ui.add_enabled(available, btn).on_hover_text(if available {
                            format!("{} ({})", link.description, link.id)
                        } else {
                            format!(
                                "Not available on this system — {} ({})",
                                link.description, link.id
                            )
                        });
                        if available && resp.clicked() {
                            match system_links::open(link) {
                                Ok(msg) => {
                                    self.status = msg;
                                    self.status_ok = true;
                                }
                                Err(e) => {
                                    self.status = format!("Open failed: {e}");
                                    self.status_ok = false;
                                }
                            }
                        }
                    }
                });
            });
    }

    fn draw_verify_panel(&mut self, ui: &mut Ui) {
        egui::Frame::group(ui.style())
            .inner_margin(12.0)
            .corner_radius(6.0)
            .fill(Color32::from_rgb(28, 32, 40))
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.heading(RichText::new("Verified values").size(16.0));
                    ui.label(
                        RichText::new(format!("as of {}", self.verify_stamp))
                            .small()
                            .weak(),
                    );
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if ui.small_button("Hide").clicked() {
                            self.show_verify = false;
                        }
                        if ui
                            .small_button("Re-check")
                            .on_hover_text("Read again from registry / services / tasks")
                            .clicked()
                        {
                            self.pending = Some(Pending::Verify);
                        }
                    });
                });
                ui.label(
                    RichText::new(
                        "These are the live values Windows reports right now for each field this app changes.",
                    )
                    .small()
                    .weak(),
                );
                ui.add_space(6.0);
                ui.separator();
                ui.add_space(4.0);

                egui::Grid::new("verify_grid")
                    .num_columns(4)
                    .spacing([12.0, 6.0])
                    .striped(true)
                    .show(ui, |ui| {
                        ui.label(RichText::new("Setting").strong().small());
                        ui.label(RichText::new("State").strong().small());
                        ui.label(RichText::new("Live value").strong().small());
                        ui.label(RichText::new("Where").strong().small());
                        ui.end_row();

                        for s in &self.settings {
                            let (state, color) = if s.active {
                                ("ON", Color32::from_rgb(220, 120, 100))
                            } else {
                                ("OFF", Color32::from_rgb(90, 180, 120))
                            };
                            ui.label(RichText::new(s.id.cli_name()).monospace().small());
                            ui.label(RichText::new(state).strong().color(color).monospace());
                            ui.label(RichText::new(&s.note).small());
                            ui.label(RichText::new(s.id.detail()).small().weak());
                            ui.end_row();
                        }
                    });
            });
    }

    fn draw_setting_card(&mut self, ui: &mut Ui, idx: usize, state: &SettingState) {
        let id = state.id;
        let mut active = state.active;
        let can_write = self.elevated;

        egui::Frame::group(ui.style())
            .inner_margin(12.0)
            .corner_radius(6.0)
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    let (badge, badge_color) = if active {
                        ("ON", Color32::from_rgb(200, 90, 70))
                    } else {
                        ("OFF", Color32::from_rgb(70, 160, 110))
                    };
                    ui.label(
                        RichText::new(badge)
                            .strong()
                            .color(badge_color)
                            .monospace(),
                    );
                    ui.vertical(|ui| {
                        ui.label(RichText::new(id.title()).strong().size(15.0));
                        ui.label(
                            RichText::new(format!("cli: {}", id.cli_name()))
                                .small()
                                .monospace()
                                .weak(),
                        );
                    });

                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if can_write {
                            let label = if active { "ON  " } else { "OFF " };
                            let btn = egui::Button::new(
                                RichText::new(label)
                                    .monospace()
                                    .strong()
                                    .color(Color32::WHITE),
                            )
                            .fill(badge_color)
                            .min_size(egui::vec2(56.0, 28.0));
                            if ui.add(btn).clicked() {
                                active = !active;
                                self.pending = Some(Pending::One { id, active });
                            }
                        }

                        if ui
                            .small_button("Verify")
                            .on_hover_text("Re-read this field’s live value from the system")
                            .clicked()
                        {
                            let fresh = telemetry::read_one(id);
                            if let Some(slot) = self.settings.iter_mut().find(|s| s.id == id) {
                                *slot = fresh.clone();
                            }
                            self.show_verify = true;
                            self.verify_stamp = crate::win_cmd::local_stamp();
                            let state_s = if fresh.active { "ON" } else { "OFF" };
                            self.status = format!(
                                "Verified {} → {state_s}: {}  ({})",
                                fresh.id.cli_name(),
                                fresh.note,
                                fresh.id.detail()
                            );
                            self.status_ok = true;
                        }

                        ui.label(if active {
                            RichText::new("Collecting").color(badge_color).small()
                        } else {
                            RichText::new("Blocked").color(badge_color).small()
                        });
                    });
                });

                ui.add_space(4.0);
                ui.horizontal(|ui| {
                    ui.label(RichText::new("Value:").small().strong());
                    ui.label(RichText::new(state.note.clone()).small().monospace());
                });

                let expand_label = if self.expanded[idx] {
                    "Hide explanation ▴"
                } else {
                    "What is this? ▾"
                };
                if ui
                    .add(
                        egui::Label::new(RichText::new(expand_label).small().weak())
                            .sense(Sense::click()),
                    )
                    .clicked()
                {
                    self.expanded[idx] = !self.expanded[idx];
                }

                if self.expanded[idx] {
                    ui.add_space(4.0);
                    ui.label(RichText::new(id.explanation()).small());
                    ui.add_space(2.0);
                    ui.label(RichText::new(id.detail()).small().monospace().weak());
                }
            });
    }
}
