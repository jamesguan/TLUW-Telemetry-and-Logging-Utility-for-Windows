//! Windows subprocess helpers: hide console windows and format local time.

use std::os::windows::process::CommandExt;
use std::process::Command;

/// `CREATE_NO_WINDOW` — prevents console flashes for CLI helpers (sc, schtasks, net, …).
const CREATE_NO_WINDOW: u32 = 0x0800_0000;

/// Build a [`Command`] that does not show a console window.
pub fn command(program: &str) -> Command {
    let mut cmd = Command::new(program);
    cmd.creation_flags(CREATE_NO_WINDOW);
    cmd
}

/// Local wall-clock stamp `yyyy-MM-dd HH:mm:ss` via Win32 (no PowerShell spawn).
pub fn local_stamp() -> String {
    #[repr(C)]
    struct SystemTime {
        year: u16,
        month: u16,
        day_of_week: u16,
        day: u16,
        hour: u16,
        minute: u16,
        second: u16,
        milliseconds: u16,
    }

    #[link(name = "kernel32")]
    extern "system" {
        fn GetLocalTime(lp_system_time: *mut SystemTime);
    }

    let mut st = SystemTime {
        year: 0,
        month: 0,
        day_of_week: 0,
        day: 0,
        hour: 0,
        minute: 0,
        second: 0,
        milliseconds: 0,
    };
    unsafe {
        GetLocalTime(&mut st);
    }
    format!(
        "{:04}-{:02}-{:02} {:02}:{:02}:{:02}",
        st.year, st.month, st.day, st.hour, st.minute, st.second
    )
}
