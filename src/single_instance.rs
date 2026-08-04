//! Ensure only one GUI process runs; later launches focus the existing window.

use crate::identity;
use std::ffi::c_void;
use std::ptr;
use std::time::Duration;

const ERROR_ALREADY_EXISTS: u32 = 183;
const SW_RESTORE: i32 = 9;
const SW_SHOW: i32 = 5;
const ASFW_ANY: u32 = 0xFFFF_FFFF;

/// Holds the named mutex for the process lifetime.
pub struct InstanceGuard {
    handle: *mut c_void,
}

// Mutex handle is process-scoped; Send is fine for parking in main.
unsafe impl Send for InstanceGuard {}

impl Drop for InstanceGuard {
    fn drop(&mut self) {
        if !self.handle.is_null() {
            unsafe {
                CloseHandle(self.handle);
            }
        }
    }
}

#[link(name = "kernel32")]
extern "system" {
    fn CreateMutexW(
        lp_mutex_attributes: *mut c_void,
        b_initial_owner: i32,
        lp_name: *const u16,
    ) -> *mut c_void;
    fn CloseHandle(h_object: *mut c_void) -> i32;
    fn GetLastError() -> u32;
}

#[link(name = "user32")]
extern "system" {
    fn FindWindowW(lp_class_name: *const u16, lp_window_name: *const u16) -> *mut c_void;
    fn AllowSetForegroundWindow(dw_process_id: u32) -> i32;
    fn ShowWindow(hwnd: *mut c_void, n_cmd_show: i32) -> i32;
    fn SetForegroundWindow(hwnd: *mut c_void) -> i32;
    fn IsWindow(hwnd: *mut c_void) -> i32;
}

fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(std::iter::once(0)).collect()
}

/// Named mutex for this product’s GUI (per user session).
const MUTEX_NAME: &str = "Local\\TelemetryLoggingUtilityGUI";

/// Try to become the sole GUI instance. `None` means another instance owns the mutex.
pub fn try_acquire() -> Option<InstanceGuard> {
    let name = wide(MUTEX_NAME);
    let handle = unsafe { CreateMutexW(ptr::null_mut(), 1, name.as_ptr()) };
    if handle.is_null() {
        return None;
    }
    let err = unsafe { GetLastError() };
    if err == ERROR_ALREADY_EXISTS {
        unsafe {
            CloseHandle(handle);
        }
        None
    } else {
        Some(InstanceGuard { handle })
    }
}

/// Find the running main window by title (matches iced `.title(...)`).
pub fn find_main_window() -> Option<isize> {
    let title = wide(identity::PRODUCT_NAME);
    let hwnd = unsafe { FindWindowW(ptr::null(), title.as_ptr()) };
    if hwnd.is_null() {
        None
    } else {
        Some(hwnd as isize)
    }
}

fn show_window(hwnd: isize) {
    if hwnd == 0 {
        return;
    }
    unsafe {
        let h = hwnd as *mut c_void;
        if IsWindow(h) == 0 {
            return;
        }
        AllowSetForegroundWindow(ASFW_ANY);
        ShowWindow(h, SW_RESTORE);
        ShowWindow(h, SW_SHOW);
        SetForegroundWindow(h);
    }
}

/// Bring an existing GUI to the foreground (restores if hidden to tray).
pub fn activate_existing() -> bool {
    let Some(hwnd) = find_main_window() else {
        return false;
    };
    show_window(hwnd);
    true
}

/// Focus the existing window, retrying briefly while it is still creating.
pub fn activate_existing_with_retry() -> bool {
    if activate_existing() {
        return true;
    }
    for _ in 0..25 {
        std::thread::sleep(Duration::from_millis(40));
        if activate_existing() {
            return true;
        }
    }
    false
}
