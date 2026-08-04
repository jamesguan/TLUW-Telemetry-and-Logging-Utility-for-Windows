//! Optional iced front-end. Enabled with the `gui` feature.

mod logic;
mod message;
mod theme;
mod view;

use crate::cleanup_history::{self, DayTotals};
use crate::cleanup_schedule::{self, CleanupScheduleState};
use crate::disclaimer;
use crate::identity;
use crate::log_cleanup::{self, LogStatus};
use crate::maintenance;
use crate::maintenance::IntegrationState;
use crate::prefs::{self, ThemePref};
use crate::system_links;
use crate::telemetry::{self, SettingId, SettingState};
use crate::temp_cleanup::{self, TempStatus};
use crate::tray::{self, TrayHandle};
use iced::time::{self, Instant};
use iced::window;
use iced::{Animation, Element, Subscription, Task, Theme};
use iced::animation::Easing;
use logic::{ConfirmPrompt, Pending};
use message::Message;
use raw_window_handle::RawWindowHandle;
use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;

pub struct App {
    window_id: Option<window::Id>,
    elevated: bool,
    settings: Vec<SettingState>,
    expanded: [bool; SettingId::ALL.len()],
    status: String,
    status_ok: bool,
    pending: Option<Pending>,
    verify_stamp: String,
    /// Brief highlight after Verify refreshes live values.
    verify_highlight_until: Option<Instant>,
    /// When true, the verified-values table is collapsed.
    hide_verify: bool,
    /// When true, logging size/count table is collapsed.
    hide_log_details: bool,
    /// When true, temp size/count table is collapsed.
    hide_temp_details: bool,
    setting_anims: [Animation<bool>; SettingId::ALL.len()],
    anim_now: Instant,
    integration: IntegrationState,
    cleanup_schedule: CleanupScheduleState,
    link_available: Vec<(&'static str, bool)>,
    clear_available: Vec<(&'static str, bool)>,
    log_status: HashMap<&'static str, LogStatus>,
    last_clear_report: String,
    temp_available: Vec<(&'static str, bool)>,
    temp_status: HashMap<&'static str, TempStatus>,
    last_temp_report: String,
    show_disclaimer: bool,
    tray_enabled: bool,
    tray: Option<TrayHandle>,
    hwnd: isize,
    force_quit: bool,
    cleanup_days: Vec<DayTotals>,
    tray_action_report: bool,
    show_settings: bool,
    /// Shared confirm modal for toggles, clears, and bulk actions.
    confirm: Option<ConfirmPrompt>,
    theme_pref: ThemePref,
    wake_flag: Arc<AtomicBool>,
}

impl App {
    fn accordion_anim(open: bool) -> Animation<bool> {
        Animation::new(open)
            .duration(Duration::from_millis(280))
            .easing(Easing::EaseOutCubic)
    }

    fn any_accordion_animating(&self) -> bool {
        let now = self.anim_now;
        self.setting_anims.iter().any(|a| a.is_animating(now))
    }

    pub(crate) fn verify_flash(&self) -> f32 {
        let Some(until) = self.verify_highlight_until else {
            return 0.0;
        };
        if self.anim_now >= until {
            return 0.0;
        }
        let left = (until - self.anim_now).as_secs_f32();
        (left / 1.8).clamp(0.0, 1.0)
    }

    fn verify_highlight_active(&self) -> bool {
        self.verify_flash() > 0.01
    }

    fn ask_confirm(
        &mut self,
        title: impl Into<String>,
        body: impl Into<String>,
        action_label: impl Into<String>,
        danger: bool,
        pending: Pending,
    ) {
        self.confirm = Some(ConfirmPrompt {
            title: title.into(),
            body: body.into(),
            action_label: action_label.into(),
            danger,
            pending,
        });
    }

    fn queue_cleanup_schedule(&mut self, cfg: cleanup_schedule::CleanupScheduleConfig) {
        self.cleanup_schedule.next_interval =
            cleanup_schedule::describe_next_interval(cfg.interval);
        self.cleanup_schedule.config = cfg.clone();
        self.pending = Some(Pending::ApplyCleanupSchedule(cfg));
    }

    /// Hide the main window and ensure a tray icon remains so the app stays running.
    fn hide_to_tray(&mut self) -> Task<Message> {
        if self.hwnd == 0 {
            self.status = "Window handle not ready yet — try again in a moment.".into();
            self.status_ok = false;
            return Task::none();
        }
        if self.tray.is_none() {
            match self.try_create_tray() {
                Ok(()) => {
                    self.tray_enabled = true;
                    let _ = prefs::set_tray_enabled(true);
                }
                Err(e) => {
                    self.status = format!(
                        "Could not keep running in the tray ({e}). Use Quit to exit, or enable tray in Settings."
                    );
                    self.status_ok = false;
                    return Task::none();
                }
            }
        }
        tray::win_hwnd::hide(self.hwnd);
        self.status =
            "Running in the system tray. Open it from the tray icon, or Quit from the tray menu."
                .into();
        self.status_ok = true;
        Task::none()
    }

    fn new() -> Self {
        let elevated = telemetry::is_elevated();
        let settings = telemetry::read_all();
        let tray_enabled = prefs::tray_enabled();
        let mut app = Self {
            window_id: None,
            elevated,
            settings,
            expanded: [false; SettingId::ALL.len()],
            status: if elevated {
                "Ready. Verify status re-reads live ON/OFF from Windows (does not change anything)."
                    .into()
            } else {
                "Not elevated — Verify status is read-only; changes need Administrator.".into()
            },
            status_ok: true,
            pending: None,
            verify_stamp: crate::win_cmd::local_stamp(),
            verify_highlight_until: None,
            hide_verify: false,
            hide_log_details: false,
            hide_temp_details: false,
            setting_anims: std::array::from_fn(|_| Self::accordion_anim(false)),
            anim_now: Instant::now(),
            integration: maintenance::read_integration(),
            cleanup_schedule: cleanup_schedule::read_state(),
            link_available: system_links::availability_map(),
            clear_available: log_cleanup::availability_map(),
            log_status: HashMap::new(),
            last_clear_report: String::new(),
            temp_available: temp_cleanup::availability_map(),
            temp_status: HashMap::new(),
            last_temp_report: String::new(),
            show_disclaimer: !disclaimer::is_accepted(),
            tray_enabled,
            tray: None,
            hwnd: 0,
            force_quit: false,
            cleanup_days: cleanup_history::daily_totals(14),
            tray_action_report: false,
            show_settings: false,
            confirm: None,
            theme_pref: prefs::theme_pref(),
            wake_flag: Arc::new(AtomicBool::new(false)),
        };
        app.refresh_log_status();
        app.refresh_temp_status();
        app
    }

    fn is_dark(&self) -> bool {
        prefs::effective_dark(self.theme_pref)
    }

    fn try_create_tray(&mut self) -> Result<(), String> {
        let wake = {
            let flag = self.wake_flag.clone();
            Arc::new(move || flag.store(true, Ordering::Relaxed)) as Arc<dyn Fn() + Send + Sync>
        };
        let handle = tray::create(wake, self.hwnd)?;
        self.tray = Some(handle);
        Ok(())
    }

    fn apply_hwnd(&mut self, hwnd: isize) {
        if hwnd == 0 {
            return;
        }
        self.hwnd = hwnd;
        if self.tray_enabled && self.tray.is_none() {
            if let Err(e) = self.try_create_tray() {
                self.tray_enabled = false;
                self.status = format!("System tray unavailable: {e}");
                self.status_ok = false;
                let _ = prefs::set_tray_enabled(false);
            }
        }
    }

    fn hwnd_from_window(win: &dyn window::Window) -> isize {
        let Ok(handle) = win.window_handle() else {
            return 0;
        };
        match handle.as_raw() {
            RawWindowHandle::Win32(h) => h.hwnd.get() as isize,
            _ => 0,
        }
    }

    fn update(&mut self, message: Message) -> Task<Message> {
        if self.wake_flag.swap(false, Ordering::Relaxed) {
            self.poll_tray_commands();
        }

        if let Some(pending) = self.pending.take() {
            let from_tray = self.tray_action_report;
            self.run_pending(pending);
            if from_tray {
                self.tray_action_report = false;
                tray::win_hwnd::show(self.hwnd);
                let title = if self.status_ok {
                    format!("{} — done", identity::PRODUCT_NAME_SHORT)
                } else {
                    format!("{} — finished with errors", identity::PRODUCT_NAME_SHORT)
                };
                tray::win_hwnd::notify(&title, &self.status);
            }
        }

        match message {
            Message::PollTray => self.poll_tray_commands(),
            Message::AnimTick(now) => {
                self.anim_now = now;
            }
            Message::WindowOpened(id) => {
                self.window_id = Some(id);
                return window::run(id, Self::hwnd_from_window).map(Message::HwndResolved);
            }
            Message::HwndResolved(hwnd) => self.apply_hwnd(hwnd),
            Message::WindowClose => {
                if self.force_quit {
                    self.tray = None;
                    if let Some(id) = self.window_id {
                        return window::close(id);
                    }
                    std::process::exit(0);
                }
                // X / Alt+F4: hide the window, keep the process + system tray.
                return self.hide_to_tray();
            }
            Message::Verify => self.pending = Some(Pending::Verify),
            Message::TurnAllOff => self.ask_confirm(
                "Turn all telemetry OFF?",
                "Block every telemetry / diagnostic setting this app controls.\n\n\
                 Each switch will be set to OFF (not collecting).",
                "Turn all OFF",
                true,
                Pending::All { active: false },
            ),
            Message::TurnAllOn => self.ask_confirm(
                "Turn all telemetry ON?",
                "Allow every telemetry / diagnostic setting this app controls.\n\n\
                 Each switch will be set to ON (collecting / allowed).",
                "Turn all ON",
                false,
                Pending::All { active: true },
            ),
            Message::RestartElevated => match telemetry::relaunch_elevated() {
                Ok(()) => std::process::exit(0),
                Err(e) => {
                    self.status = e;
                    self.status_ok = false;
                }
            },
            Message::ShowSettings => {
                self.show_settings = true;
                self.integration = maintenance::read_integration();
                self.cleanup_schedule = cleanup_schedule::read_state();
            }
            Message::CloseSettings => self.show_settings = false,
            Message::SetTheme(pref) => {
                if let Err(e) = prefs::set_theme_pref(pref) {
                    self.status = format!("Could not save theme: {e}");
                    self.status_ok = false;
                } else {
                    self.theme_pref = pref;
                    self.status = format!("Theme set to {}.", pref.label());
                    self.status_ok = true;
                }
            }
            Message::SetStartup(v) => self.pending = Some(Pending::SetStartup(v)),
            Message::SetPostUpdate(v) => self.pending = Some(Pending::SetPostUpdate(v)),
            Message::SetTrayEnabled(v) => self.apply_tray_pref(v),
            Message::SetCleanupClearSafe(v) => {
                let mut cfg = self.cleanup_schedule.config.clone();
                cfg.clear_safe_logs = v;
                if v {
                    // Safe is implied by all-logs; leaving both on is fine.
                } else if !cfg.clear_all_logs {
                    // ok
                }
                self.queue_cleanup_schedule(cfg);
            }
            Message::SetCleanupClearAll(v) => {
                let mut cfg = self.cleanup_schedule.config.clone();
                cfg.clear_all_logs = v;
                if v {
                    cfg.clear_safe_logs = true;
                }
                self.queue_cleanup_schedule(cfg);
            }
            Message::SetCleanupClearTemp(v) => {
                let mut cfg = self.cleanup_schedule.config.clone();
                cfg.clear_temp = v;
                self.queue_cleanup_schedule(cfg);
            }
            Message::SetCleanupOnLogon(v) => {
                let mut cfg = self.cleanup_schedule.config.clone();
                cfg.on_logon = v;
                self.queue_cleanup_schedule(cfg);
            }
            Message::SetCleanupOnSessionEnd(v) => {
                let mut cfg = self.cleanup_schedule.config.clone();
                cfg.on_session_end = v;
                self.queue_cleanup_schedule(cfg);
            }
            Message::SetCleanupInterval(interval) => {
                let mut cfg = self.cleanup_schedule.config.clone();
                cfg.interval = interval;
                self.queue_cleanup_schedule(cfg);
            }
            Message::DisableCleanupSchedule => {
                let mut cfg = self.cleanup_schedule.config.clone();
                cfg.on_logon = false;
                cfg.on_session_end = false;
                cfg.interval = cleanup_schedule::CleanupInterval::Off;
                self.queue_cleanup_schedule(cfg);
            }
            Message::MinimizeToTray => {
                return self.hide_to_tray();
            }
            Message::Quit => self.ask_confirm(
                format!("Quit {}?", identity::PRODUCT_NAME_SHORT),
                "Exit the application completely?\n\n\
                 The window and system tray icon will both close.",
                "Quit",
                true,
                Pending::Quit,
            ),
            Message::ShowDisclaimer => self.show_disclaimer = true,
            Message::AcceptDisclaimer => match disclaimer::accept("gui") {
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
            },
            Message::CloseDisclaimer => self.show_disclaimer = false,
            Message::HideVerify(hide) => self.hide_verify = hide,
            Message::HideLogDetails(hide) => self.hide_log_details = hide,
            Message::HideTempDetails(hide) => self.hide_temp_details = hide,
            Message::RefreshDashboard => {
                self.refresh();
                self.refresh_cleanup_history();
            }
            Message::RefreshLogStatus => {
                self.clear_available = log_cleanup::availability_map();
                self.refresh_log_status();
                self.hide_log_details = false;
                let total_files: u64 = self.log_status.values().map(|s| s.files).sum();
                let total_bytes: u64 = self.log_status.values().map(|s| s.bytes).sum();
                self.status = format!(
                    "Logging status: {} target(s), {} item(s)/records, {}",
                    self.log_status.len(),
                    total_files,
                    log_cleanup::format_bytes(total_bytes)
                );
                self.status_ok = true;
            }
            Message::RefreshTempStatus => {
                self.temp_available = temp_cleanup::availability_map();
                self.refresh_temp_status();
                self.hide_temp_details = false;
                let total_files: u64 = self.temp_status.values().map(|s| s.files).sum();
                let total_bytes: u64 = self.temp_status.values().map(|s| s.bytes).sum();
                self.status = format!(
                    "Temp status: {} location(s), {} file(s), {}",
                    self.temp_status.len(),
                    total_files,
                    log_cleanup::format_bytes(total_bytes)
                );
                self.status_ok = true;
            }
            Message::RefreshLinks => {
                self.link_available = system_links::availability_map();
                let n = self.link_available.iter().filter(|(_, a)| *a).count();
                self.status = format!(
                    "Log tools: {n}/{} available on this system.",
                    system_links::ALL.len()
                );
                self.status_ok = true;
            }
            Message::ToggleSetting { id, active } => {
                if active {
                    self.ask_confirm(
                        format!("Turn ON — {}?", id.title()),
                        format!(
                            "Allow this setting to collect / run again?\n\n\
                             Setting: {}\nCLI: tluw set {} on\n\n\
                             ON = collecting / allowed.",
                            id.title(),
                            id.cli_name()
                        ),
                        "Turn ON",
                        false,
                        Pending::One { id, active: true },
                    );
                } else {
                    self.ask_confirm(
                        format!("Turn OFF — {}?", id.title()),
                        format!(
                            "Block this telemetry / diagnostic setting?\n\n\
                             Setting: {}\nCLI: tluw set {} off\n\n\
                             OFF = not collecting / blocked.",
                            id.title(),
                            id.cli_name()
                        ),
                        "Turn OFF",
                        true,
                        Pending::One { id, active: false },
                    );
                }
            }
            Message::VerifyOne(id) => {
                let fresh = telemetry::read_one(id);
                if let Some(slot) = self.settings.iter_mut().find(|s| s.id == id) {
                    *slot = fresh.clone();
                }
                self.hide_verify = false;
                self.verify_stamp = crate::win_cmd::local_stamp();
                self.verify_highlight_until =
                    Some(self.anim_now + Duration::from_millis(1800));
                let state_s = if fresh.active { "ON" } else { "OFF" };
                self.status = format!(
                    "Verified {} → {state_s}: {}  (live re-read; nothing changed)",
                    fresh.id.cli_name(),
                    fresh.note
                );
                self.status_ok = true;
            }
            Message::ToggleExpand(idx) => {
                let now = Instant::now();
                let next = !self.expanded[idx];
                for i in 0..SettingId::ALL.len() {
                    if i == idx {
                        self.expanded[i] = next;
                        self.setting_anims[i].go_mut(next, now);
                    } else if self.expanded[i] {
                        self.expanded[i] = false;
                        self.setting_anims[i].go_mut(false, now);
                    }
                }
            }
            Message::OpenLog(id) => {
                if let Some(action) = log_cleanup::ClearAction::find(id) {
                    let st = log_cleanup::inspect(action);
                    self.log_status.insert(action.id, st.clone());
                    match log_cleanup::open_location(action) {
                        Ok(msg) => {
                            self.status = format!("{msg} | {}", st.summary_line());
                            self.status_ok = true;
                        }
                        Err(e) => {
                            self.status = format!("Open failed ({e}). Status: {}", st.summary_line());
                            self.status_ok = false;
                        }
                    }
                }
            }
            Message::RequestClearLog(id) => {
                if let Some(action) = log_cleanup::ClearAction::find(id) {
                    let st = log_cleanup::inspect(action);
                    self.log_status.insert(action.id, st.clone());
                    self.ask_confirm(
                        format!("Clear {}?", action.title),
                        format!(
                            "Permanently clear this log target.\n\n\
                             Target: {} ({})\nCurrent: {}\n\n\
                             Locked files are skipped. This cannot be undone.",
                            action.title,
                            action.id,
                            st.summary_line()
                        ),
                        "Clear",
                        true,
                        Pending::ClearLog(id),
                    );
                }
            }
            Message::RequestClearAllSafe => {
                self.refresh_log_status();
                let total_bytes: u64 = self.log_status.values().map(|s| s.bytes).sum();
                self.ask_confirm(
                    "Clear all safe logs?",
                    format!(
                        "Clear every available non-dangerous log target.\n\n\
                         Diagnosis wipe / Security logs are NOT included.\n\
                         Current total ~{}.\n\n\
                         Locked files are skipped. This cannot be undone.",
                        log_cleanup::format_bytes(total_bytes)
                    ),
                    "Clear safe logs",
                    true,
                    Pending::ClearAllSafe,
                );
            }
            Message::RequestClearAllLogs => {
                self.refresh_log_status();
                let total_bytes: u64 = self.log_status.values().map(|s| s.bytes).sum();
                self.ask_confirm(
                    "Clear ALL logs?",
                    format!(
                        "Clear every available log target, including dangerous ones \
                         (Diagnosis wipe / Security).\n\n\
                         Current total ~{}.\n\n\
                         Locked files are skipped. This cannot be undone.",
                        log_cleanup::format_bytes(total_bytes)
                    ),
                    "Clear ALL logs",
                    true,
                    Pending::ClearAllLogs,
                );
            }
            Message::OpenTemp(id) => {
                if let Some(target) = temp_cleanup::TempTarget::find(id) {
                    let st = temp_cleanup::inspect(target);
                    self.temp_status.insert(target.id, st.clone());
                    match temp_cleanup::open_location(target) {
                        Ok(msg) => {
                            self.status = format!("{msg} | {}", st.summary_line());
                            self.status_ok = true;
                        }
                        Err(e) => {
                            self.status = format!("Open failed ({e}). Status: {}", st.summary_line());
                            self.status_ok = false;
                        }
                    }
                }
            }
            Message::RequestClearTemp(id) => {
                if let Some(target) = temp_cleanup::TempTarget::find(id) {
                    let st = temp_cleanup::inspect(target);
                    self.temp_status.insert(target.id, st.clone());
                    self.ask_confirm(
                        format!("Clear {}?", target.title),
                        format!(
                            "Permanently clear this temporary files location.\n\n\
                             Target: {} ({})\nCurrent: {}\n\n\
                             This cannot be undone.",
                            target.title,
                            target.id,
                            st.summary_line()
                        ),
                        "Clear",
                        true,
                        Pending::ClearTemp(id),
                    );
                }
            }
            Message::RequestClearTempAll => {
                self.refresh_temp_status();
                let total_bytes: u64 = self.temp_status.values().map(|s| s.bytes).sum();
                self.ask_confirm(
                    "Clear all temporary files?",
                    format!(
                        "Clear every available temp target (TEMP, Windows\\Temp, Prefetch, …).\n\n\
                         Current total ~{}.\n\n\
                         This cannot be undone.",
                        log_cleanup::format_bytes(total_bytes)
                    ),
                    "Clear all temp",
                    true,
                    Pending::ClearTempAll,
                );
            }
            Message::ConfirmAccept => {
                if let Some(prompt) = self.confirm.take() {
                    self.pending = Some(prompt.pending);
                }
            }
            Message::ConfirmCancel => {
                self.confirm = None;
            }
            Message::OpenLink(id) => {
                if let Some(link) = system_links::find(id) {
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
        }

        Task::none()
    }

    fn subscription(&self) -> Subscription<Message> {
        let mut subs = vec![
            time::every(Duration::from_millis(200)).map(|_| Message::PollTray),
            window::close_requests().map(|_| Message::WindowClose),
            window::open_events().map(Message::WindowOpened),
        ];
        if self.any_accordion_animating() || self.verify_highlight_active() {
            subs.push(window::frames().map(Message::AnimTick));
        }
        Subscription::batch(subs)
    }

    fn theme(&self) -> Theme {
        if self.is_dark() {
            Theme::Dark
        } else {
            Theme::Light
        }
    }

    fn view(&self) -> Element<'_, Message> {
        view::view(self)
    }
}

/// Run the iced GUI application.
pub fn run() -> iced::Result {
    let (rgba, w, h) = crate::app_icon::window_rgba();
    let icon = window::icon::from_rgba(rgba, w, h).ok();

    fn boot() -> (App, Task<Message>) {
        (App::new(), Task::none())
    }

    iced::application(boot, App::update, App::view)
        .subscription(App::subscription)
        .theme(App::theme)
        .window(window::Settings {
            size: iced::Size::new(760.0, 900.0),
            min_size: Some(iced::Size::new(420.0, 520.0)),
            icon,
            // Handle X ourselves so we can hide to tray instead of exiting.
            exit_on_close_request: false,
            ..Default::default()
        })
        .title(identity::PRODUCT_NAME)
        .run()
}
