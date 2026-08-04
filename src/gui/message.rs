//! UI messages for the iced GUI.

use crate::prefs::ThemePref;
use crate::telemetry::SettingId;
use iced::time::Instant;
use iced::window;

#[derive(Debug, Clone)]
pub enum Message {
    PollTray,
    AnimTick(Instant),
    WindowClose,
    WindowOpened(window::Id),
    HwndResolved(isize),
    Verify,
    TurnAllOff,
    TurnAllOn,
    RestartElevated,
    ShowSettings,
    CloseSettings,
    SetTheme(ThemePref),
    SetStartup(bool),
    SetPostUpdate(bool),
    SetTrayEnabled(bool),
    MinimizeToTray,
    Quit,
    ShowDisclaimer,
    AcceptDisclaimer,
    CloseDisclaimer,
    RefreshDashboard,
    RefreshLogStatus,
    RefreshTempStatus,
    RefreshLinks,
    /// User flipped a setting switch to `active` (ON = collecting).
    ToggleSetting { id: SettingId, active: bool },
    VerifyOne(SettingId),
    ToggleExpand(usize),
    HideVerify(bool),
    HideLogDetails(bool),
    HideTempDetails(bool),
    OpenLog(&'static str),
    /// Ask to clear one log target (opens confirm modal).
    RequestClearLog(&'static str),
    RequestClearAllSafe,
    RequestClearAllLogs,
    OpenTemp(&'static str),
    RequestClearTemp(&'static str),
    RequestClearTempAll,
    OpenLink(&'static str),
    ConfirmAccept,
    ConfirmCancel,
}
