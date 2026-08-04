//! System tray icon + context menu (GUI feature).
//!
//! Uses `tray-icon` (muda menus). Window show/hide goes through Win32 `ShowWindow`
//! because eframe `ViewportCommand::Visible` is unreliable when the window is hidden.

use crate::app_icon;
use egui::Context;
use std::sync::mpsc::{self, Receiver, Sender};
use tray_icon::menu::{Menu, MenuEvent, MenuItem, PredefinedMenuItem};
use tray_icon::{Icon, MouseButton, TrayIcon, TrayIconBuilder, TrayIconEvent};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TrayCommand {
    Show,
    DisableTelemetry,
    ClearSafeLogs,
    Quit,
}

pub struct TrayHandle {
    _tray: TrayIcon,
    rx: Receiver<TrayCommand>,
    _tx: Sender<TrayCommand>,
}

fn make_icon() -> Icon {
    let (rgba, w, h) = app_icon::tray_rgba();
    Icon::from_rgba(rgba, w, h).expect("tray icon")
}

pub fn create(ctx: Context) -> Result<TrayHandle, String> {
    let (tx, rx) = mpsc::channel::<TrayCommand>();

    let menu = Menu::new();
    let show = MenuItem::new("Open dashboard", true, None);
    let disable = MenuItem::new("Disable telemetry", true, None);
    let clear_logs = MenuItem::new("Clear safe logs", true, None);
    let quit = MenuItem::new("Quit", true, None);

    menu.append(&show)
        .map_err(|e| format!("tray menu: {e}"))?;
    menu.append(&PredefinedMenuItem::separator())
        .map_err(|e| format!("tray menu: {e}"))?;
    menu.append(&disable)
        .map_err(|e| format!("tray menu: {e}"))?;
    menu.append(&clear_logs)
        .map_err(|e| format!("tray menu: {e}"))?;
    menu.append(&PredefinedMenuItem::separator())
        .map_err(|e| format!("tray menu: {e}"))?;
    menu.append(&quit)
        .map_err(|e| format!("tray menu: {e}"))?;

    let show_id = show.id().clone();
    let disable_id = disable.id().clone();
    let clear_id = clear_logs.id().clone();
    let quit_id = quit.id().clone();

    let tray = TrayIconBuilder::new()
        .with_tooltip("Windows Diagnostics")
        .with_icon(make_icon())
        .with_menu(Box::new(menu))
        .build()
        .map_err(|e| format!("tray icon: {e}"))?;

    let tx_menu = tx.clone();
    let ctx_menu = ctx.clone();
    MenuEvent::set_event_handler(Some(move |event: MenuEvent| {
        let cmd = if event.id == show_id {
            Some(TrayCommand::Show)
        } else if event.id == disable_id {
            Some(TrayCommand::DisableTelemetry)
        } else if event.id == clear_id {
            Some(TrayCommand::ClearSafeLogs)
        } else if event.id == quit_id {
            // Exit here — do not wait for egui's update loop. When the main
            // window is SW_HIDE'd, eframe often never runs another frame, so a
            // queued Quit / ViewportCommand::Close never executes.
            std::process::exit(0);
        } else {
            None
        };
        if let Some(cmd) = cmd {
            let _ = tx_menu.send(cmd);
            ctx_menu.request_repaint();
        }
    }));

    let tx_tray = tx.clone();
    let ctx_tray = ctx.clone();
    TrayIconEvent::set_event_handler(Some(move |event: TrayIconEvent| {
        let show = match &event {
            TrayIconEvent::DoubleClick { button, .. } | TrayIconEvent::Click { button, .. } => {
                *button == MouseButton::Left
            }
            _ => false,
        };
        if show {
            let _ = tx_tray.send(TrayCommand::Show);
            ctx_tray.request_repaint();
        }
    }));

    Ok(TrayHandle {
        _tray: tray,
        rx,
        _tx: tx,
    })
}

impl TrayHandle {
    pub fn poll(&self) -> Vec<TrayCommand> {
        let mut out = Vec::new();
        while let Ok(cmd) = self.rx.try_recv() {
            out.push(cmd);
        }
        out
    }
}

/// Win32 show/hide for the egui HWND (works when window is fully hidden).
pub mod win_hwnd {
    use std::ffi::c_void;

    const SW_HIDE: i32 = 0;
    const SW_RESTORE: i32 = 9;
    const SW_SHOW: i32 = 5;
    const WM_CLOSE: u32 = 0x0010;

    #[link(name = "user32")]
    extern "system" {
        fn ShowWindow(hwnd: *mut c_void, n_cmd_show: i32) -> i32;
        fn SetForegroundWindow(hwnd: *mut c_void) -> i32;
        fn IsWindow(hwnd: *mut c_void) -> i32;
        fn PostMessageW(hwnd: *mut c_void, msg: u32, wparam: usize, lparam: isize) -> i32;
        fn DestroyWindow(hwnd: *mut c_void) -> i32;
    }

    pub fn hide(hwnd: isize) {
        if hwnd == 0 {
            return;
        }
        unsafe {
            ShowWindow(hwnd as *mut c_void, SW_HIDE);
        }
    }

    pub fn show(hwnd: isize) {
        if hwnd == 0 {
            return;
        }
        unsafe {
            let h = hwnd as *mut c_void;
            if IsWindow(h) == 0 {
                return;
            }
            ShowWindow(h, SW_RESTORE);
            ShowWindow(h, SW_SHOW);
            SetForegroundWindow(h);
        }
    }

    /// Ask the window to close (works even if it was hidden with `ShowWindow`).
    pub fn request_close(hwnd: isize) {
        if hwnd == 0 {
            return;
        }
        unsafe {
            let h = hwnd as *mut c_void;
            if IsWindow(h) == 0 {
                return;
            }
            // Restore first — some hosts ignore WM_CLOSE while SW_HIDE.
            ShowWindow(h, SW_SHOW);
            PostMessageW(h, WM_CLOSE, 0, 0);
        }
    }

    pub fn destroy(hwnd: isize) {
        if hwnd == 0 {
            return;
        }
        unsafe {
            let h = hwnd as *mut c_void;
            if IsWindow(h) != 0 {
                DestroyWindow(h);
            }
        }
    }
}
