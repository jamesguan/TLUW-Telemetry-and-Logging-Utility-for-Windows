//! System tray icon + context menu (GUI feature).
//!
//! Uses `tray-icon` (muda menus). Window show/hide goes through Win32 `ShowWindow`
//! because iced visibility toggles are unreliable when the window is hidden.
//! Destructive tray actions use a Win32 Yes/No confirmation before running.

use crate::app_icon;
use crate::identity;
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::Arc;
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

/// Create the tray icon. `wake` is called when a menu/tray event needs the GUI to poll.
pub fn create(
    wake: Arc<dyn Fn() + Send + Sync>,
    hwnd: isize,
) -> Result<TrayHandle, String> {
    let (tx, rx) = mpsc::channel::<TrayCommand>();

    let menu = Menu::new();
    let show = MenuItem::new("Open dashboard", true, None);
    let disable = MenuItem::new("Disable telemetry…", true, None);
    let clear_logs = MenuItem::new("Clear safe logs…", true, None);
    let quit = MenuItem::new("Quit…", true, None);

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
        .with_tooltip(identity::PRODUCT_NAME_SHORT)
        .with_icon(make_icon())
        .with_menu(Box::new(menu))
        .build()
        .map_err(|e| format!("tray icon: {e}"))?;

    let tx_menu = tx.clone();
    let wake_menu = wake.clone();
    MenuEvent::set_event_handler(Some(move |event: MenuEvent| {
        let cmd = if event.id == show_id {
            Some(TrayCommand::Show)
        } else if event.id == disable_id {
            if win_hwnd::confirm(
                "Disable telemetry?",
                "Turn OFF all telemetry / diagnostic collection settings controlled by this app?\n\n\
                 This changes registry, services, and scheduled tasks. Continue?",
            ) {
                win_hwnd::show(hwnd);
                Some(TrayCommand::DisableTelemetry)
            } else {
                None
            }
        } else if event.id == clear_id {
            if win_hwnd::confirm(
                "Clear safe logs?",
                "Clear all safe log targets now?\n\n\
                 This permanently deletes log data (Diagnosis ETL / dangerous targets are NOT included).\n\
                 Continue?",
            ) {
                win_hwnd::show(hwnd);
                Some(TrayCommand::ClearSafeLogs)
            } else {
                None
            }
        } else if event.id == quit_id {
            if win_hwnd::confirm(
                &format!("Quit {}?", identity::PRODUCT_NAME_SHORT),
                "Exit the application completely?\n\nThe system tray icon will be removed.",
            ) {
                std::process::exit(0);
            }
            None
        } else {
            None
        };
        if let Some(cmd) = cmd {
            let _ = tx_menu.send(cmd);
            wake_menu();
        }
    }));

    let tx_tray = tx.clone();
    let wake_tray = wake.clone();
    TrayIconEvent::set_event_handler(Some(move |event: TrayIconEvent| {
        let show = match &event {
            TrayIconEvent::DoubleClick { button, .. } | TrayIconEvent::Click { button, .. } => {
                *button == MouseButton::Left
            }
            _ => false,
        };
        if show {
            win_hwnd::show(hwnd);
            let _ = tx_tray.send(TrayCommand::Show);
            wake_tray();
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

/// Win32 show/hide + message boxes for the main HWND / tray confirms.
pub mod win_hwnd {
    use std::ffi::c_void;

    const SW_HIDE: i32 = 0;
    const SW_RESTORE: i32 = 9;
    const SW_SHOW: i32 = 5;
    const WM_CLOSE: u32 = 0x0010;
    const MB_OK: u32 = 0x0000_0000;
    const MB_YESNO: u32 = 0x0000_0004;
    const MB_ICONWARNING: u32 = 0x0000_0030;
    const MB_ICONINFORMATION: u32 = 0x0000_0040;
    const MB_TOPMOST: u32 = 0x0004_0000;
    const MB_SETFOREGROUND: u32 = 0x0001_0000;
    const IDYES: i32 = 6;

    #[link(name = "user32")]
    extern "system" {
        fn ShowWindow(hwnd: *mut c_void, n_cmd_show: i32) -> i32;
        fn SetForegroundWindow(hwnd: *mut c_void) -> i32;
        fn IsWindow(hwnd: *mut c_void) -> i32;
        fn PostMessageW(hwnd: *mut c_void, msg: u32, wparam: usize, lparam: isize) -> i32;
        fn DestroyWindow(hwnd: *mut c_void) -> i32;
        fn MessageBoxW(
            hwnd: *mut c_void,
            text: *const u16,
            caption: *const u16,
            utype: u32,
        ) -> i32;
    }

    fn wide(s: &str) -> Vec<u16> {
        s.encode_utf16().chain(std::iter::once(0)).collect()
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

    pub fn request_close(hwnd: isize) {
        if hwnd == 0 {
            return;
        }
        unsafe {
            let h = hwnd as *mut c_void;
            if IsWindow(h) == 0 {
                return;
            }
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

    /// Blocking Yes/No confirmation (works while the main window is hidden).
    pub fn confirm(title: &str, text: &str) -> bool {
        let title_w = wide(title);
        let text_w = wide(text);
        let flags = MB_YESNO | MB_ICONWARNING | MB_TOPMOST | MB_SETFOREGROUND;
        unsafe {
            MessageBoxW(
                std::ptr::null_mut(),
                text_w.as_ptr(),
                title_w.as_ptr(),
                flags,
            ) == IDYES
        }
    }

    /// Blocking information notice after a tray action completes.
    pub fn notify(title: &str, text: &str) {
        let title_w = wide(title);
        let text_w = wide(text);
        let flags = MB_OK | MB_ICONINFORMATION | MB_TOPMOST | MB_SETFOREGROUND;
        unsafe {
            MessageBoxW(
                std::ptr::null_mut(),
                text_w.as_ptr(),
                title_w.as_ptr(),
                flags,
            );
        }
    }
}
