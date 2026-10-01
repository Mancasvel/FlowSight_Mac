//! Explicit local reminder consent; no startup/tracking changes.
use rusqlite::{params, Connection};
use serde::Serialize;
use tauri_plugin_notification::{NotificationExt, PermissionState};

fn read_bool(key: &str) -> Result<bool, String> {
    let conn = Connection::open(crate::paths::db_path()?).map_err(|e| e.to_string())?;
    Ok(conn
        .query_row::<String, _, _>("SELECT value FROM config WHERE key=?1", [key], |r| r.get(0))
        .ok()
        .as_deref()
        == Some("true"))
}
fn write_bool(key: &str, enabled: bool) -> Result<(), String> {
    let conn = Connection::open(crate::paths::db_path()?).map_err(|e| e.to_string())?;
    conn.execute(
        "INSERT OR REPLACE INTO config (key,value) VALUES (?1,?2)",
        params![key, if enabled { "true" } else { "false" }],
    )
    .map_err(|e| e.to_string())?;
    Ok(())
}
pub fn focus_alerts_enabled() -> bool {
    read_bool("focus_alerts_enabled").unwrap_or(false)
}
pub fn contextual_focus_alerts_enabled() -> bool {
    read_bool("contextual_focus_alerts_enabled").unwrap_or(false)
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DesktopPreferences {
    focus_alerts_enabled: bool,
    contextual_focus_alerts_enabled: bool,
}
#[tauri::command]
pub fn get_desktop_preferences() -> DesktopPreferences {
    DesktopPreferences {
        focus_alerts_enabled: focus_alerts_enabled(),
        contextual_focus_alerts_enabled: contextual_focus_alerts_enabled(),
    }
}
#[tauri::command]
pub fn set_focus_alerts_enabled(
    app: tauri::AppHandle,
    state: tauri::State<'_, crate::agent::AgentState>,
    enabled: bool,
) -> Result<bool, String> {
    if enabled
        && app
            .notification()
            .request_permission()
            .map_err(|e| e.to_string())?
            != PermissionState::Granted
    {
        return Err("Allow FlowSight notifications in your system settings first.".into());
    }
    write_bool("focus_alerts_enabled", enabled)?;
    if !enabled {
        write_bool("contextual_focus_alerts_enabled", false)?;
    }
    crate::focus_alerts::set_enabled(enabled);
    if enabled
        && state
            .lock()
            .map_err(|e| e.to_string())?
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
    write_bool("contextual_focus_alerts_enabled", enabled)?;
    Ok(enabled)
}
