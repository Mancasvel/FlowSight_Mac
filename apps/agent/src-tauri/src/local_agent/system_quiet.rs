//! Windows' notification banner switch. This does not claim to configure
//! Focus Assist allowlists; it silences ordinary app banners and restores the
//! exact prior registry value when the requested period ends.

use chrono::{Duration, Utc};
use serde_json::{json, Value};

use super::state::{self, SystemQuiet};
use std::sync::Mutex;

static CONTROL: Mutex<()> = Mutex::new(());

#[cfg(target_os = "windows")]
const KEY: &str = r"HKCU\Software\Microsoft\Windows\CurrentVersion\Notifications\Settings";
#[cfg(target_os = "windows")]
const VALUE: &str = "NOC_GLOBAL_SETTING_TOASTS_ENABLED";
#[cfg(windows)]
const POLICY_KEY: &str = r"Software\Policies\Microsoft\Windows\CurrentVersion\PushNotifications";
#[cfg(windows)]
const POLICY_VALUE: &str = "NoToastApplicationNotification";

#[cfg(test)]
thread_local! {
    static TEST_BANNERS: std::cell::Cell<Option<Option<u32>>> = const { std::cell::Cell::new(None) };
}

#[cfg(test)]
pub fn mock_banners(value: Option<u32>) {
    TEST_BANNERS.set(Some(value));
}

#[cfg(test)]
pub fn mocked_banners() -> Option<u32> {
    TEST_BANNERS.get().flatten()
}

pub fn system_notifications_supported() -> bool {
    #[cfg(test)]
    if TEST_BANNERS.get().is_some() {
        return true;
    }
    cfg!(windows)
}

#[cfg(windows)]
fn reg(args: &[&str]) -> Result<std::process::Output, String> {
    std::process::Command::new("reg.exe")
        .args(args)
        .output()
        .map_err(|error| format!("Could not open Windows notification settings: {error}"))
}

#[cfg(windows)]
fn read_banner_setting() -> Result<Option<u32>, String> {
    #[cfg(test)]
    if let Some(value) = TEST_BANNERS.get() {
        return Ok(value);
    }
    let output = reg(&["query", KEY, "/v", VALUE])?;
    if !output.status.success() {
        return Ok(None);
    }
    let text = String::from_utf8_lossy(&output.stdout);
    let field = text
        .lines()
        .find(|line| line.contains(VALUE))
        .and_then(|line| line.split_whitespace().last())
        .ok_or("Windows returned an unreadable notification setting.")?;
    let number = field.strip_prefix("0x").unwrap_or(field);
    u32::from_str_radix(number, 16)
        .map(Some)
        .map_err(|_| "Windows returned an invalid notification setting.".into())
}

#[cfg(windows)]
fn write_banner_setting(value: Option<u32>) -> Result<(), String> {
    #[cfg(test)]
    if TEST_BANNERS.get().is_some() {
        TEST_BANNERS.set(Some(value));
        return Ok(());
    }
    let output = if let Some(value) = value {
        reg(&[
            "add",
            KEY,
            "/v",
            VALUE,
            "/t",
            "REG_DWORD",
            "/d",
            &value.to_string(),
            "/f",
        ])?
    } else {
        reg(&["delete", KEY, "/v", VALUE, "/f"])?
    };
    if !output.status.success() {
        return Err("Windows did not accept the notification setting change.".into());
    }
    Ok(())
}

#[cfg(not(windows))]
fn read_banner_setting() -> Result<Option<u32>, String> {
    #[cfg(test)]
    if let Some(value) = TEST_BANNERS.get() {
        return Ok(value);
    }
    Err("System notification control is available on Windows only.".into())
}

#[cfg(not(windows))]
fn write_banner_setting(value: Option<u32>) -> Result<(), String> {
    #[cfg(test)]
    if TEST_BANNERS.get().is_some() {
        TEST_BANNERS.set(Some(value));
        return Ok(());
    }
    let _ = value;
    Err("System notification control is available on Windows only.".into())
}

#[cfg(windows)]
fn read_policy_setting() -> Result<Option<u32>, String> {
    #[cfg(test)]
    if let Some(value) = TEST_BANNERS.get() {
        return Ok(value);
    }
    use winreg::{enums::HKEY_CURRENT_USER, RegKey};
    match RegKey::predef(HKEY_CURRENT_USER).open_subkey(POLICY_KEY) {
        Ok(key) => match key.get_value(POLICY_VALUE) {
            Ok(value) => Ok(Some(value)),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
            Err(error) => Err(format!("Could not read notification policy: {error}")),
        },
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(format!("Could not read notification policy: {error}")),
    }
}

#[cfg(windows)]
fn write_policy_setting(value: Option<u32>) -> Result<(), String> {
    #[cfg(test)]
    if TEST_BANNERS.get().is_some() {
        TEST_BANNERS.set(Some(value));
        return Ok(());
    }
    use winreg::{enums::HKEY_CURRENT_USER, RegKey};
    let (key, _) = RegKey::predef(HKEY_CURRENT_USER)
        .create_subkey(POLICY_KEY)
        .map_err(|error| format!("Could not open notification policy: {error}"))?;
    if let Some(value) = value {
        key.set_value(POLICY_VALUE, &value)
            .map_err(|error| error.to_string())?;
    } else if let Err(error) = key.delete_value(POLICY_VALUE) {
        if error.kind() != std::io::ErrorKind::NotFound {
            return Err(error.to_string());
        }
    }
    // Notify the shell/broker; writing policy alone leaves its cached setting unchanged.
    unsafe {
        use windows::{
            core::w,
            Win32::{
                Foundation::{LPARAM, WPARAM},
                UI::WindowsAndMessaging::{SendNotifyMessageW, HWND_BROADCAST, WM_SETTINGCHANGE},
            },
        };
        let _ = SendNotifyMessageW(
            HWND_BROADCAST,
            WM_SETTINGCHANGE,
            WPARAM(0),
            LPARAM(w!("Policy").as_ptr() as isize),
        );
    }
    Ok(())
}

#[cfg(not(windows))]
fn read_policy_setting() -> Result<Option<u32>, String> {
    read_banner_setting()
}
#[cfg(not(windows))]
fn write_policy_setting(value: Option<u32>) -> Result<(), String> {
    write_banner_setting(value)
}

fn confirm_silence() -> Result<(), String> {
    #[cfg(test)]
    if TEST_BANNERS.get().is_some() {
        return Ok(());
    }
    // WinRT queried through the .NET host reflects the user policy immediately;
    // a notifier activated by the running desktop app can retain Enabled.
    let mut command = std::process::Command::new("powershell.exe");
    command.args(["-NoLogo", "-NoProfile", "-NonInteractive", "-Command",
        "[Windows.UI.Notifications.NotificationSetting, Windows.UI.Notifications, ContentType = WindowsRuntime] | Out-Null; $focusNotifier = [Windows.UI.Notifications.ToastNotificationManager, Windows.UI.Notifications, ContentType = WindowsRuntime]::CreateToastNotifier('ai.flowsight.agent'); if ($focusNotifier.Setting -eq 'DisabledByGroupPolicy' -or $focusNotifier.Setting -eq 'DisabledForUser') { exit 0 }; exit 1"]);
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        command.creation_flags(0x08000000); // CREATE_NO_WINDOW
    }
    let status = command
        .status()
        .map_err(|error| format!("Could not confirm Windows notification silence: {error}"))?;
    if status.success() {
        Ok(())
    } else {
        Err("Windows did not confirm notification silence.".into())
    }
}

pub fn enable(duration_minutes: Option<i64>) -> Result<Value, String> {
    enable_for(false, duration_minutes)
}

pub fn enable_total_focus(duration_minutes: i64) -> Result<Value, String> {
    enable_for(true, Some(duration_minutes))
}

fn enable_for(total: bool, duration_minutes: Option<i64>) -> Result<Value, String> {
    let _guard = CONTROL.lock().map_err(|error| error.to_string())?;
    let data = state::read()?;
    if total
        && !data
            .total_focus
            .as_ref()
            .is_some_and(|session| session.is_active())
    {
        return Err("Total focus ended before notification protection could be applied.".into());
    }
    let previous = data
        .quiet
        .or(data.total_focus_quiet)
        .filter(|quiet| quiet.policy_based);
    let original = if let Some(ref quiet) = previous {
        quiet.original
    } else {
        read_policy_setting()?
    };
    let until_at =
        duration_minutes.map(|minutes| (Utc::now() + Duration::minutes(minutes)).to_rfc3339());
    // Journal restoration before touching Windows, including crash recovery.
    state::update(|data| {
        if total
            && !data
                .total_focus
                .as_ref()
                .is_some_and(|session| session.is_active())
        {
            return Err(
                "Total focus ended before notification protection could be applied.".into(),
            );
        }
        let quiet = Some(SystemQuiet {
            original,
            until_at: until_at.clone(),
            policy_based: true,
        });
        if total {
            data.total_focus_quiet = quiet;
        } else {
            data.quiet = quiet;
        }
        Ok(())
    })?;
    if let Err(error) = write_policy_setting(Some(1)).and_then(|()| confirm_silence()) {
        // An already active owner still keeps its protection.
        if previous.is_none() {
            let _ = write_policy_setting(original);
        }
        state::update(|data| {
            if total {
                data.total_focus_quiet = None;
            } else {
                data.quiet = None;
            }
            Ok(())
        })?;
        return Err(error);
    }
    Ok(json!({"enabled":true,"scope":"Windows app notification banners","untilAt":until_at}))
}

pub fn total_focus_confirmed() -> bool {
    state::read().is_ok_and(|data| data.total_focus_quiet.is_some())
        && read_policy_setting().is_ok_and(|value| value == Some(1))
}

pub fn disable() -> Result<Value, String> {
    disable_for(false)
}

pub fn disable_total_focus() -> Result<Value, String> {
    disable_for(true)
}

fn disable_for(total: bool) -> Result<Value, String> {
    let _guard = CONTROL.lock().map_err(|error| error.to_string())?;
    let data = state::read()?;
    let other_active = if total {
        data.quiet.is_some()
    } else {
        data.total_focus_quiet.is_some()
    };
    let Some(quiet) = (if total {
        data.total_focus_quiet
    } else {
        data.quiet
    }) else {
        return Ok(
            json!({"enabled":false,"scope":"Windows app notification banners","restored":false}),
        );
    };
    let original = quiet.original;
    let current = if quiet.policy_based {
        read_policy_setting()?
    } else {
        read_banner_setting()?
    };
    let user_changed_setting = current != Some(if quiet.policy_based { 1 } else { 0 });
    if !user_changed_setting && !other_active {
        if quiet.policy_based {
            write_policy_setting(original)?;
        } else {
            write_banner_setting(original)?;
        }
    }
    state::update(|data| {
        if total {
            data.total_focus_quiet = None;
        } else {
            data.quiet = None;
        }
        Ok(())
    })?;
    Ok(
        json!({"enabled":other_active,"scope":"Windows app notification banners","restored":!user_changed_setting && !other_active}),
    )
}

pub fn restore_if_expired() -> Result<bool, String> {
    let Some(quiet) = state::read()?.quiet else {
        return Ok(false);
    };
    let Some(until_at) = quiet.until_at else {
        return Ok(false);
    };
    let until = chrono::DateTime::parse_from_rfc3339(&until_at)
        .map_err(|_| "Saved notification expiry is invalid.".to_string())?;
    if until > Utc::now() {
        return Ok(false);
    }
    disable()?;
    Ok(true)
}
