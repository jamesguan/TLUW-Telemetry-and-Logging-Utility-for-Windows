//! Optional egui front-end. Enabled with the `gui` feature.

use crate::maintenance::{self, IntegrationState};
use crate::system_links;
use crate::telemetry::{self, SettingId, SettingState};
use eframe::egui::{self, Color32, RichText, Sense, Ui};

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
}

enum Pending {
    One { id: SettingId, active: bool },
    All { active: bool },
    /// Re-read system values and open the verify panel.
    Verify,
    SetStartup(bool),
    SetPostUpdate(bool),
}

impl Default for DiagnosticsApp {
    fn default() -> Self {
        let elevated = telemetry::is_elevated();
        // Status reads work without elevation for most keys; always load a snapshot.
        let settings = telemetry::read_all();
        Self {
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
        }
    }
}

impl eframe::App for DiagnosticsApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        if let Some(pending) = self.pending.take() {
            self.run_pending(pending);
        }

        egui::TopBottomPanel::top("header").show(ctx, |ui| {
            ui.add_space(8.0);
            ui.horizontal(|ui| {
                ui.heading(RichText::new("Windows Diagnostics").strong());
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
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
            ui.add_space(4.0);
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
            ui.label(
                RichText::new(
                    "CLI: windows-diagnostics status | disable | enable | set <id> on|off",
                )
                .weak()
                .small()
                .monospace(),
            );
            ui.add_space(6.0);
        });

        egui::CentralPanel::default().show(ctx, |ui| {
            ui.horizontal(|ui| {
                if ui
                    .add(
                        egui::Button::new(
                            RichText::new("Verify status").strong().color(Color32::WHITE),
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
                            RichText::new("Turn all OFF").color(Color32::from_rgb(220, 160, 120)),
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
                    RichText::new("OFF = privacy / blocked    ·    ON = collecting / allowed")
                        .weak()
                        .small(),
                );
            });

            ui.add_space(8.0);

            self.draw_integration_row(ui);
            ui.add_space(8.0);

            self.draw_system_links_row(ui);
            ui.add_space(8.0);

            if self.show_verify {
                self.draw_verify_panel(ui);
                ui.add_space(10.0);
            }

            egui::ScrollArea::vertical()
                .auto_shrink([false, false])
                .show(ui, |ui| {
                    for (idx, state) in self.settings.clone().into_iter().enumerate() {
                        self.draw_setting_card(ui, idx, &state);
                        ui.add_space(8.0);
                    }
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
            });
    }

    fn draw_system_links_row(&mut self, ui: &mut Ui) {
        egui::Frame::group(ui.style())
            .inner_margin(10.0)
            .corner_radius(6.0)
            .show(ui, |ui| {
                ui.label(RichText::new("Windows logs & tools").strong());
                ui.label(
                    RichText::new(
                        "Open built-in viewers and folders. Same as: windows-diagnostics logs | open <id>",
                    )
                    .small()
                    .weak(),
                );
                ui.add_space(6.0);
                ui.horizontal_wrapped(|ui| {
                    for link in system_links::ALL {
                        if ui
                            .button(link.title)
                            .on_hover_text(format!("{} ({})", link.description, link.id))
                            .clicked()
                        {
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
