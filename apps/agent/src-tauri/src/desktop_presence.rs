//! Explicit opt-in for launch at login, automatic tracking, and local focus reminders.

use rusqlite::{params, Connection};
use serde::Serialize;
use std::sync::atomic::{AtomicBool, Ordering};
use tauri::Manager;

#[cfg(desktop)]
use tauri_plugin_autostart::ManagerExt;
#[cfg(desktop)]
use tauri_plugin_notification::{NotificationExt, PermissionState};

const PROMPT_DECIDED_KEY: &str = "desktop_presence_prompt_decided";
const AUTO_MONITOR_KEY: &str = "start_monitoring_at_login";
const FOCUS_ALERTS_KEY: &str = "focus_alerts_enabled";
const CONTEXTUAL_FOCUS_ALERTS_KEY: &str = "contextual_focus_alerts_enabled";
static QUITTING: AtomicBool = AtomicBool::new(false);

fn read_bool(key: &str) -> Result<bool, String> {
    let path = crate::paths::db_path()?;
    let conn = Connection::open(path).map_err(|e| e.to_string())?;
    let value: Option<String> = conn
        .query_row("SELECT value FROM config WHERE key = ?1", [key], |row| {
            row.get(0)
        })
        .ok();
    Ok(value.as_deref() == Some("true"))
}

fn write_bool(key: &str, enabled: bool) -> Result<(), String> {
    let path = crate::paths::db_path()?;
    let conn = Connection::open(path).map_err(|e| e.to_string())?;
    conn.execute(
        "INSERT OR REPLACE INTO config (key, value) VALUES (?1, ?2)",
        params![key, if enabled { "true" } else { "false" }],
    )
    .map_err(|e| e.to_string())?;
    Ok(())
}

pub fn focus_alerts_enabled() -> bool {
    read_bool(FOCUS_ALERTS_KEY).unwrap_or(false)
}

pub fn contextual_focus_alerts_enabled() -> bool {
    read_bool(CONTEXTUAL_FOCUS_ALERTS_KEY).unwrap_or(false)
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DesktopPreferences {
    launch_at_login: bool,
    start_monitoring_at_login: bool,
    focus_alerts_enabled: bool,
    contextual_focus_alerts_enabled: bool,
    prompt_decided: bool,
    autostart_launch: bool,
    development_build: bool,
}

#[tauri::command]
pub fn get_desktop_preferences(app: tauri::AppHandle) -> Result<DesktopPreferences, String> {
    #[cfg(desktop)]
    let launch_at_login = app.autolaunch().is_enabled().map_err(|e| e.to_string())?;
    #[cfg(not(desktop))]
    let launch_at_login = false;
    #[cfg(not(desktop))]
    let _ = app;

    Ok(DesktopPreferences {
        launch_at_login,
        start_monitoring_at_login: launch_at_login && read_bool(AUTO_MONITOR_KEY)?,
        focus_alerts_enabled: read_bool(FOCUS_ALERTS_KEY)?,
        contextual_focus_alerts_enabled: read_bool(CONTEXTUAL_FOCUS_ALERTS_KEY)?,
        prompt_decided: read_bool(PROMPT_DECIDED_KEY)?,
        autostart_launch: std::env::args().any(|arg| arg == "--flowsight-autostart"),
        development_build: tauri::is_dev(),
    })
}

#[tauri::command]
pub fn set_launch_at_login(app: tauri::AppHandle, enabled: bool) -> Result<bool, String> {
    #[cfg(desktop)]
    {
        if enabled && tauri::is_dev() {
            return Err("Install the release build before enabling launch at login.".into());
        }
        if enabled {
            app.autolaunch().enable().map_err(|e| e.to_string())?;
        } else {
            app.autolaunch().disable().map_err(|e| e.to_string())?;
            write_bool(AUTO_MONITOR_KEY, false)?;
        }
        write_bool(PROMPT_DECIDED_KEY, true)?;
        app.autolaunch().is_enabled().map_err(|e| e.to_string())
    }
    #[cfg(not(desktop))]
    {
        let _ = (app, enabled);
        Err("Launch at login is available on desktop only.".into())
    }
}

#[tauri::command]
pub fn set_start_monitoring_at_login(app: tauri::AppHandle, enabled: bool) -> Result<bool, String> {
    #[cfg(desktop)]
    if enabled {
        if !app.autolaunch().is_enabled().map_err(|e| e.to_string())? {
            return Err("Enable launch at login first.".into());
        }
        crate::privacy::require_monitoring_acknowledgement(&crate::paths::db_path()?)?;
    }
    #[cfg(not(desktop))]
    if enabled {
        let _ = app;
        return Err("Automatic tracking is available on desktop only.".into());
    }
    write_bool(AUTO_MONITOR_KEY, enabled)?;
    Ok(enabled)
}

#[tauri::command]
pub fn set_focus_alerts_enabled(
    app: tauri::AppHandle,
    state: tauri::State<'_, crate::agent::AgentState>,
    enabled: bool,
) -> Result<bool, String> {
    #[cfg(desktop)]
    if enabled
        && app
            .notification()
            .request_permission()
            .map_err(|e| e.to_string())?
            != PermissionState::Granted
    {
        return Err("Allow FlowSight notifications in your system settings first.".into());
    }
    #[cfg(not(desktop))]
    let _ = app;
    write_bool(FOCUS_ALERTS_KEY, enabled)?;
    crate::focus_alerts::set_enabled(enabled);
    if enabled
        && state
            .lock()
            .unwrap()
            .as_ref()
            .is_some_and(|agent| agent.is_running)
    {
        crate::focus_alerts::start_monitoring(&crate::paths::db_path()?);
    }
    Ok(enabled)
}

#[tauri::command]
pub fn set_contextual_focus_alerts_enabled(enabled: bool) -> Result<bool, String> {
    if enabled && !focus_alerts_enabled() {
        return Err("Enable focus reminders first.".into());
    }
    write_bool(CONTEXTUAL_FOCUS_ALERTS_KEY, enabled)?;
    Ok(enabled)
}

#[tauri::command]
pub fn dismiss_desktop_prompt() -> Result<bool, String> {
    write_bool(PROMPT_DECIDED_KEY, true)?;
    Ok(true)
}

#[cfg(desktop)]
pub fn setup_tray(app: &mut tauri::App) -> tauri::Result<()> {
    use tauri::menu::{Menu, MenuItem};
    use tauri::tray::{TrayIconBuilder, TrayIconEvent};

    let open = MenuItem::with_id(
        app,
        "open",
        crate::language::copy("Open FlowSight", "Abrir FlowSight"),
        true,
        None::<&str>,
    )?;
    let quit = MenuItem::with_id(
        app,
        "quit",
        crate::language::copy("Quit FlowSight", "Salir de FlowSight"),
        true,
        None::<&str>,
    )?;
    let menu = Menu::with_items(app, &[&open, &quit])?;
    let icon = app
        .default_window_icon()
        .ok_or(tauri::Error::WindowNotFound)?
        .clone();
    TrayIconBuilder::with_id("flowsight-tray")
        .icon(icon)
        .tooltip("FlowSight")
        .menu(&menu)
        .show_menu_on_left_click(false)
        .on_menu_event(|app, event| match event.id().0.as_str() {
            "open" => show_main_window(app),
            "quit" => {
                QUITTING.store(true, Ordering::Relaxed);
                crate::local_agent::restore_on_exit();
                app.exit(0);
            }
            _ => {}
        })
        .on_tray_icon_event(|tray, event| {
            if matches!(event, TrayIconEvent::DoubleClick { .. }) {
                show_main_window(tray.app_handle());
            }
        })
        .build(app)?;
    Ok(())
}

#[cfg(desktop)]
fn show_main_window(app: &tauri::AppHandle) {
    if let Some(window) = app.get_webview_window("main") {
        let _ = window.show();
        let _ = window.unminimize();
        let _ = window.set_focus();
    }
}

#[cfg(desktop)]
pub fn should_hide_on_close() -> bool {
    !QUITTING.load(Ordering::Relaxed)
}
