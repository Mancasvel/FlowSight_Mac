//! Windows' notification banner switch. This does not claim to configure
//! Focus Assist allowlists; it silences ordinary app banners and restores the
//! exact prior registry value when the requested period ends.

use chrono::{Duration, Utc};
use serde_json::{json, Value};

use super::state::{self, SystemQuiet};

#[cfg(target_os = "windows")]
const KEY: &str = r"HKCU\Software\Microsoft\Windows\CurrentVersion\Notifications\Settings";
#[cfg(target_os = "windows")]
const VALUE: &str = "NOC_GLOBAL_SETTING_TOASTS_ENABLED";

#[cfg(windows)]
fn reg(args: &[&str]) -> Result<std::process::Output, String> {
    std::process::Command::new("reg.exe")
        .args(args)
        .output()
        .map_err(|error| format!("Could not open Windows notification settings: {error}"))
}

#[cfg(windows)]
fn read_banner_setting() -> Result<Option<u32>, String> {
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
    Err("System notification control is available on Windows only.".into())
}

#[cfg(not(windows))]
fn write_banner_setting(_: Option<u32>) -> Result<(), String> {
    Err("System notification control is available on Windows only.".into())
}

pub fn enable(duration_minutes: Option<i64>) -> Result<Value, String> {
    let previous = state::read()?.quiet;
    let original = if let Some(ref quiet) = previous {
        quiet.original
    } else {
        read_banner_setting()?
    };
    write_banner_setting(Some(0))?;
    let until_at =
        duration_minutes.map(|minutes| (Utc::now() + Duration::minutes(minutes)).to_rfc3339());
    if let Err(error) = state::update(|data| {
        data.quiet = Some(SystemQuiet {
            original,
            until_at: until_at.clone(),
        });
        Ok(())
    }) {
        if previous.is_none() {
            let _ = write_banner_setting(original);
        }
        return Err(error);
    }
    Ok(json!({"enabled":true,"scope":"Windows app notification banners","untilAt":until_at}))
}

pub fn disable() -> Result<Value, String> {
    let Some(quiet) = state::read()?.quiet else {
        return Ok(
            json!({"enabled":false,"scope":"Windows app notification banners","restored":false}),
        );
    };
    let original = quiet.original;
    let current = read_banner_setting()?;
    let user_changed_setting = current != Some(0);
    if !user_changed_setting {
        write_banner_setting(original)?;
    }
    state::update(|data| {
        data.quiet = None;
        Ok(())
    })?;
    Ok(
        json!({"enabled":false,"scope":"Windows app notification banners","restored":!user_changed_setting}),
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
