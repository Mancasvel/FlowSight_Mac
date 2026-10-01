//! Privacy controls, retention, portability, and erasure.

use chrono::{Local, Utc};
use reqwest::blocking::Client;
use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::path::Path;
use std::time::Duration;
use tauri::{Manager, State};

use crate::agent::AgentState;
use crate::sync::get_user_session_from_conn;
use crate::sync_env::{supabase_anon_key, supabase_url};

pub const PRIVACY_NOTICE_VERSION: &str = "2026-08-23";
const PRIVACY_SETTINGS_KEY: &str = "privacy_settings";
const DEFAULT_RETENTION_DAYS: u32 = 30;
const MAX_RETENTION_DAYS: u32 = 3650;
const RETENTION_CHECK_INTERVAL_SECONDS: u64 = 24 * 60 * 60;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PrivacySettings {
    #[serde(rename = "noticeVersion")]
    pub notice_version: String,
    #[serde(rename = "monitoringNoticeAcknowledged", default)]
    pub monitoring_notice_acknowledged: bool,
    #[serde(rename = "monitoringNoticeAcknowledgedAt", default)]
    pub monitoring_notice_acknowledged_at: Option<String>,
    #[serde(rename = "cloudSyncEnabled", default)]
    pub cloud_sync_enabled: bool,
    #[serde(rename = "cloudSyncEnabledAt", default)]
    pub cloud_sync_enabled_at: Option<String>,
    #[serde(rename = "cloudAiEnabled", default)]
    pub cloud_ai_enabled: bool,
    #[serde(rename = "cloudAiEnabledAt", default)]
    pub cloud_ai_enabled_at: Option<String>,
    #[serde(rename = "storeWindowTitles", default)]
    pub store_window_titles: bool,
    #[serde(
        rename = "excludedApplications",
        default = "default_excluded_applications"
    )]
    pub excluded_applications: Vec<String>,
    #[serde(rename = "retentionDays", default = "default_retention_days")]
    pub retention_days: u32,
    #[serde(rename = "updatedAt", default)]
    pub updated_at: Option<String>,
}

fn default_retention_days() -> u32 {
    DEFAULT_RETENTION_DAYS
}

fn default_excluded_applications() -> Vec<String> {
    [
        "1Password",
        "Bitwarden",
        "KeePass",
        "KeePassXC",
        "CredentialUIBroker",
        "Keychain Access",
        "SecurityAgent",
        "loginwindow",
    ]
    .into_iter()
    .map(String::from)
    .collect()
}

impl Default for PrivacySettings {
    fn default() -> Self {
        Self {
            notice_version: PRIVACY_NOTICE_VERSION.to_string(),
            monitoring_notice_acknowledged: false,
            monitoring_notice_acknowledged_at: None,
            cloud_sync_enabled: false,
            cloud_sync_enabled_at: None,
            cloud_ai_enabled: false,
            cloud_ai_enabled_at: None,
            store_window_titles: false,
            excluded_applications: default_excluded_applications(),
            retention_days: DEFAULT_RETENTION_DAYS,
            updated_at: None,
        }
    }
}

#[derive(Debug, Clone, Default, Deserialize)]
pub struct PrivacySettingsPatch {
    #[serde(rename = "monitoringNoticeAcknowledged")]
    pub monitoring_notice_acknowledged: Option<bool>,
    #[serde(rename = "cloudSyncEnabled")]
    pub cloud_sync_enabled: Option<bool>,
    #[serde(rename = "cloudAiEnabled")]
    pub cloud_ai_enabled: Option<bool>,
    #[serde(rename = "storeWindowTitles")]
    pub store_window_titles: Option<bool>,
    #[serde(rename = "excludedApplications")]
    pub excluded_applications: Option<Vec<String>>,
    #[serde(rename = "retentionDays")]
    pub retention_days: Option<u32>,
}

#[derive(Debug, Serialize)]
struct ExportReport {
    id: i64,
    description: String,
    activity_type: String,
    synced: bool,
    created_at: String,
    jira_ticket_id: Option<String>,
    duration_seconds: i64,
    active_app: Option<String>,
    window_title: Option<String>,
    capture_source: Option<String>,
    theme_hint: Option<String>,
}

#[derive(Debug, Serialize)]
struct ExportPrivacyEvent {
    purpose: String,
    granted: bool,
    notice_version: String,
    created_at: String,
}

fn config_value(conn: &Connection, key: &str) -> Option<String> {
    conn.query_row(
        "SELECT value FROM config WHERE key = ?1",
        params![key],
        |row| row.get(0),
    )
    .optional()
    .ok()
    .flatten()
}

pub fn ensure_schema(conn: &Connection) -> Result<(), String> {
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS privacy_events (
            id INTEGER PRIMARY KEY,
            purpose TEXT NOT NULL,
            granted INTEGER NOT NULL,
            notice_version TEXT NOT NULL,
            created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP
         );
         CREATE INDEX IF NOT EXISTS privacy_events_created_idx
           ON privacy_events(created_at);",
    )
    .map_err(|error| error.to_string())
}

pub fn load_privacy_settings(db_path: &Path) -> Result<PrivacySettings, String> {
    let conn = Connection::open(db_path).map_err(|error| error.to_string())?;
    ensure_schema(&conn)?;
    let raw: Option<String> = conn
        .query_row(
            "SELECT value FROM config WHERE key = ?1",
            params![PRIVACY_SETTINGS_KEY],
            |row| row.get(0),
        )
        .optional()
        .map_err(|error| error.to_string())?;
    let mut settings: PrivacySettings = raw
        .as_deref()
        .map(serde_json::from_str)
        .transpose()
        .map_err(|error| format!("Invalid privacy settings: {error}"))?
        .unwrap_or_default();
    settings.retention_days = settings.retention_days.clamp(1, MAX_RETENTION_DAYS);
    Ok(settings)
}

fn save_privacy_settings(conn: &Connection, settings: &PrivacySettings) -> Result<(), String> {
    let encoded = serde_json::to_string(settings).map_err(|error| error.to_string())?;
    conn.execute(
        "INSERT OR REPLACE INTO config (key, value) VALUES (?1, ?2)",
        params![PRIVACY_SETTINGS_KEY, encoded],
    )
    .map_err(|error| error.to_string())?;
    Ok(())
}

fn record_choice(conn: &Connection, purpose: &str, granted: bool) -> Result<(), String> {
    conn.execute(
        "INSERT INTO privacy_events (purpose, granted, notice_version) VALUES (?1, ?2, ?3)",
        params![purpose, i32::from(granted), PRIVACY_NOTICE_VERSION],
    )
    .map_err(|error| error.to_string())?;
    Ok(())
}

pub fn cloud_sync_enabled(db_path: &Path) -> bool {
    load_privacy_settings(db_path)
        .map(|settings| settings.cloud_sync_enabled)
        .unwrap_or(false)
}

pub fn require_cloud_ai(db_path: &Path) -> Result<(), String> {
    let settings = load_privacy_settings(db_path)?;
    if settings.cloud_ai_enabled {
        Ok(())
    } else {
        Err("Cloud AI sharing is off. Enable it in Privacy & data before sending activity or messages to the AI coach.".to_string())
    }
}

pub fn store_window_titles(db_path: &Path) -> bool {
    load_privacy_settings(db_path)
        .map(|settings| settings.store_window_titles)
        .unwrap_or(false)
}

pub(crate) fn normalized_application(value: &str) -> String {
    let name = value
        .trim()
        .rsplit(['/', '\\'])
        .next()
        .unwrap_or(value)
        .to_lowercase();
    name.trim_end_matches(".exe")
        .trim_end_matches(".app")
        .trim()
        .to_string()
}

pub fn application_is_excluded(db_path: &Path, application: Option<&str>) -> bool {
    let Some(application) = application.map(normalized_application) else {
        return true;
    };
    if application.is_empty()
        || [
            "1password",
            "bitwarden",
            "keepass",
            "lastpass",
            "keychain access",
            "securityagent",
            "loginwindow",
        ]
        .iter()
        .any(|excluded| application.contains(excluded))
    {
        return true;
    }
    load_privacy_settings(db_path)
        .map(|settings| {
            settings
                .excluded_applications
                .iter()
                .map(|value| normalized_application(value))
                .any(|excluded| excluded == application)
        })
        .unwrap_or(true)
}

pub fn require_monitoring_acknowledgement(db_path: &Path) -> Result<(), String> {
    let settings = load_privacy_settings(db_path)?;
    if settings.monitoring_notice_acknowledged && settings.notice_version == PRIVACY_NOTICE_VERSION
    {
        Ok(())
    } else {
        Err("Review the local monitoring notice before starting tracking.".to_string())
    }
}

pub fn enforce_local_retention(db_path: &Path) -> Result<usize, String> {
    let settings = load_privacy_settings(db_path)?;
    let conn = Connection::open(db_path).map_err(|error| error.to_string())?;
    if !settings.store_window_titles {
        conn.execute(
            "UPDATE reports SET window_title = NULL WHERE window_title IS NOT NULL",
            [],
        )
        .map_err(|error| error.to_string())?;
    }
    let modifier = format!("-{} days", settings.retention_days);
    let deleted = conn
        .execute(
            "DELETE FROM reports WHERE datetime(created_at) < datetime('now', ?1)",
            params![modifier],
        )
        .map_err(|error| error.to_string())?;
    Ok(deleted)
}

pub fn start_local_retention_thread(db_path: std::path::PathBuf) {
    std::thread::spawn(move || loop {
        std::thread::sleep(Duration::from_secs(RETENTION_CHECK_INTERVAL_SECONDS));
        match enforce_local_retention(&db_path) {
            Ok(deleted) if deleted > 0 => {
                log::info!("[Privacy] Removed {deleted} expired local activity row(s)")
            }
            Ok(_) => {}
            Err(error) => log::warn!("[Privacy] Scheduled local retention failed: {error}"),
        }
    });
}

fn sync_server_privacy_settings(conn: &Connection, settings: &PrivacySettings) {
    let Some(session) = get_user_session_from_conn(conn) else {
        return;
    };
    let response = Client::builder()
        .timeout(Duration::from_secs(20))
        .build()
        .and_then(|client| {
            client
                .post(format!("{}/functions/v1/privacy-rights", supabase_url()))
                .header("apikey", supabase_anon_key())
                .bearer_auth(session.access_token)
                .json(&json!({
                    "action": "update_preferences",
                    "notice_version": settings.notice_version,
                    "cloud_sync_enabled": settings.cloud_sync_enabled,
                    "cloud_ai_enabled": settings.cloud_ai_enabled,
                }))
                .send()
        });
    match response {
        Ok(result) if result.status().is_success() => {}
        _ => log::warn!("[Privacy] Could not mirror privacy choices to the account service"),
    }
}

#[tauri::command]
pub fn get_privacy_settings() -> Result<PrivacySettings, String> {
    let db_path = crate::paths::db_path()?;
    let settings = load_privacy_settings(&db_path)?;
    if let Ok(conn) = Connection::open(&db_path) {
        sync_server_privacy_settings(&conn, &settings);
    }
    Ok(settings)
}

#[tauri::command]
pub fn update_privacy_settings(patch: PrivacySettingsPatch) -> Result<PrivacySettings, String> {
    let db_path = crate::paths::db_path()?;
    let conn = Connection::open(&db_path).map_err(|error| error.to_string())?;
    ensure_schema(&conn)?;
    let mut settings = load_privacy_settings(&db_path)?;
    let now = Utc::now().to_rfc3339();

    if let Some(value) = patch.monitoring_notice_acknowledged {
        if value != settings.monitoring_notice_acknowledged
            || settings.notice_version != PRIVACY_NOTICE_VERSION
        {
            record_choice(&conn, "local_monitoring_notice", value)?;
        }
        settings.monitoring_notice_acknowledged = value;
        settings.monitoring_notice_acknowledged_at = value.then(|| now.clone());
    }
    if let Some(value) = patch.cloud_sync_enabled {
        if value != settings.cloud_sync_enabled {
            record_choice(&conn, "cloud_activity_sync", value)?;
        }
        settings.cloud_sync_enabled = value;
        settings.cloud_sync_enabled_at = value.then(|| now.clone());
    }
    if let Some(value) = patch.cloud_ai_enabled {
        if value != settings.cloud_ai_enabled {
            record_choice(&conn, "cloud_ai", value)?;
        }
        settings.cloud_ai_enabled = value;
        settings.cloud_ai_enabled_at = value.then(|| now.clone());
    }
    if let Some(value) = patch.store_window_titles {
        if value != settings.store_window_titles {
            record_choice(&conn, "store_window_titles", value)?;
        }
        settings.store_window_titles = value;
        if !value {
            conn.execute("UPDATE reports SET window_title = NULL", [])
                .map_err(|error| error.to_string())?;
        }
    }
    if let Some(values) = patch.excluded_applications {
        let mut cleaned = Vec::new();
        for value in values.into_iter().take(50) {
            let value = value.trim();
            if value.is_empty() || value.chars().count() > 80 {
                continue;
            }
            if !cleaned
                .iter()
                .any(|existing: &String| existing.eq_ignore_ascii_case(value))
            {
                cleaned.push(value.to_string());
            }
        }
        if cleaned != settings.excluded_applications {
            record_choice(&conn, "excluded_applications_changed", true)?;
        }
        settings.excluded_applications = cleaned;
    }
    if let Some(value) = patch.retention_days {
        let value = value.clamp(1, MAX_RETENTION_DAYS);
        if value != settings.retention_days {
            record_choice(&conn, "local_retention_changed", true)?;
        }
        settings.retention_days = value;
    }

    settings.notice_version = PRIVACY_NOTICE_VERSION.to_string();
    settings.updated_at = Some(now);
    save_privacy_settings(&conn, &settings)?;
    drop(conn);
    crate::telemetry::refresh_privacy_filter();
    enforce_local_retention(&db_path)?;
    if let Ok(conn) = Connection::open(&db_path) {
        sync_server_privacy_settings(&conn, &settings);
    }
    Ok(settings)
}

fn fetch_cloud_export(conn: &Connection, db_path: &Path) -> Result<Option<Value>, String> {
    let Some(session) = get_user_session_from_conn(conn) else {
        return Ok(None);
    };
    let analytics_credentials = crate::anonymous_analytics::analytics_export_credentials(db_path)?;
    let response = Client::builder()
        .timeout(Duration::from_secs(60))
        .build()
        .map_err(|error| error.to_string())?
        .post(format!("{}/functions/v1/privacy-rights", supabase_url()))
        .header("apikey", supabase_anon_key())
        .bearer_auth(session.access_token)
        .json(&json!({
            "action": "export",
            "anonymous_id": analytics_credentials.as_ref().map(|value| &value.0),
            "anonymous_secret": analytics_credentials.as_ref().map(|value| &value.1),
        }))
        .send()
        .map_err(|error| format!("Could not request the cloud export: {error}"))?;
    let status = response.status();
    let payload: Value = response
        .json()
        .map_err(|_| "The cloud export service returned an invalid response.".to_string())?;
    if !status.is_success() {
        return Err(payload["error"]
            .as_str()
            .unwrap_or("The cloud export failed.")
            .to_string());
    }
    Ok(Some(payload))
}

#[tauri::command]
pub fn export_personal_data(include_cloud: bool) -> Result<String, String> {
    let db_path = crate::paths::db_path()?;
    let conn = Connection::open(&db_path).map_err(|error| error.to_string())?;
    ensure_schema(&conn)?;
    let mut stmt = conn
        .prepare(
            "SELECT id, COALESCE(description,''), COALESCE(activity_type,''), COALESCE(synced,0),
                    COALESCE(created_at,''), jira_ticket_id, COALESCE(duration_seconds,0),
                    active_app, window_title, capture_source, theme_hint
             FROM reports ORDER BY datetime(created_at) ASC, id ASC",
        )
        .map_err(|error| error.to_string())?;
    let reports = stmt
        .query_map([], |row| {
            Ok(ExportReport {
                id: row.get(0)?,
                description: row.get(1)?,
                activity_type: row.get(2)?,
                synced: row.get::<_, i64>(3)? != 0,
                created_at: row.get(4)?,
                jira_ticket_id: row.get(5)?,
                duration_seconds: row.get(6)?,
                active_app: row.get(7)?,
                window_title: row.get(8)?,
                capture_source: row.get(9)?,
                theme_hint: row.get(10)?,
            })
        })
        .map_err(|error| error.to_string())?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|error| error.to_string())?;
    drop(stmt);

    let mut event_stmt = conn
        .prepare(
            "SELECT purpose, granted, notice_version, created_at
             FROM privacy_events ORDER BY datetime(created_at) ASC, id ASC",
        )
        .map_err(|error| error.to_string())?;
    let privacy_events = event_stmt
        .query_map([], |row| {
            Ok(ExportPrivacyEvent {
                purpose: row.get(0)?,
                granted: row.get::<_, i64>(1)? != 0,
                notice_version: row.get(2)?,
                created_at: row.get(3)?,
            })
        })
        .map_err(|error| error.to_string())?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|error| error.to_string())?;
    drop(event_stmt);

    let privacy_settings = load_privacy_settings(&db_path)?;
    let analytics =
        crate::anonymous_analytics::load_analytics_consent(&db_path).unwrap_or_default();
    let user_preferences =
        crate::user_preferences::load_user_preferences(&db_path).unwrap_or_default();
    let weekly_report_schedule = crate::report_schedule::get_weekly_report_schedule()?;
    let local_agent_data = crate::local_agent::state::read()?;
    let coach_messages = crate::coach_chat::load_messages(&conn).unwrap_or_default();
    let application_settings = json!({
        "display_name": config_value(&conn, "dev_name"),
        "vision_model": config_value(&conn, "vision_model"),
        "gpu_layers": config_value(&conn, "gpu_layers"),
        "daily_goal_hours": config_value(&conn, "daily_goal_hours"),
        "last_cloud_sync": config_value(&conn, "last_cloud_sync")
            .and_then(|value| serde_json::from_str::<Value>(&value).ok()),
        "entitlements": crate::entitlements::load_entitlements(&conn),
    });
    let account = get_user_session_from_conn(&conn).map(|session| {
        json!({
            "user_id": session.user_id,
            "email": session.email,
            "team_id": session.team_id,
        })
    });
    let cloud = if include_cloud {
        fetch_cloud_export(&conn, &db_path)?
    } else {
        None
    };
    let export = json!({
        "format": "FlowSight GDPR data export",
        "format_version": 1,
        "generated_at": Utc::now().to_rfc3339(),
        "account": account,
        "privacy_settings": privacy_settings,
        "analytics_preference": {
            "decided": analytics.decided,
            "consented": analytics.consented,
            "anonymous_id": analytics.anonymous_id,
            "decided_at": analytics.decided_at,
            "withdrawal_pending": analytics.withdrawal_pending,
        },
        "user_preferences": user_preferences,
        "weekly_report_schedule": weekly_report_schedule,
        "local_agent_data": local_agent_data,
        "application_settings": application_settings,
        "local_activity_reports": reports,
        "local_coach_messages": coach_messages,
        "privacy_choice_history": privacy_events,
        "cloud_data": cloud,
        "excluded_security_data": ["access tokens", "refresh tokens", "OAuth state", "encryption material"],
    });
    let bytes = serde_json::to_vec_pretty(&export).map_err(|error| error.to_string())?;
    let filename = format!(
        "FlowSight-data-export-{}.json",
        Local::now().format("%Y-%m-%d")
    );
    crate::paths::save_bytes_to_downloads(&filename, &bytes)
}

fn remove_runtime_artifacts(app: &tauri::AppHandle) -> Vec<String> {
    let mut warnings = Vec::new();
    if let Ok(dir) = crate::paths::screenshots_tmp_dir() {
        if let Ok(entries) = std::fs::read_dir(dir) {
            for entry in entries.filter_map(Result::ok) {
                if entry.path().is_file() && std::fs::remove_file(entry.path()).is_err() {
                    warnings.push("A temporary screenshot could not be removed.".to_string());
                }
            }
        }
    }
    for path in [
        crate::paths::server_log_path().ok(),
        crate::paths::auth_log_path().ok(),
        crate::paths::agent_error_log_path().ok(),
        Some(crate::paths::crash_log_path_or_fallback()),
    ]
    .into_iter()
    .flatten()
    {
        if path.exists() && std::fs::write(&path, []).is_err() {
            warnings.push(format!(
                "Could not clear {} while it is in use.",
                path.display()
            ));
        }
    }
    if let Ok(log_dir) = app.path().app_log_dir() {
        if let Ok(entries) = std::fs::read_dir(log_dir) {
            for entry in entries.filter_map(Result::ok) {
                let path = entry.path();
                if path.is_file() && std::fs::write(&path, []).is_err() {
                    warnings.push(format!(
                        "Could not clear application log {} while it is in use.",
                        path.display()
                    ));
                }
            }
        }
    }
    warnings
}

fn erase_local_database(conn: &Connection) -> Result<(), String> {
    let transaction = conn
        .unchecked_transaction()
        .map_err(|error| error.to_string())?;
    transaction
        .execute("DELETE FROM reports", [])
        .map_err(|error| error.to_string())?;
    transaction
        .execute("DELETE FROM privacy_events", [])
        .map_err(|error| error.to_string())?;
    transaction
        .execute("DELETE FROM config", [])
        .map_err(|error| error.to_string())?;
    transaction.commit().map_err(|error| error.to_string())?;
    conn.execute_batch("PRAGMA wal_checkpoint(TRUNCATE); VACUUM;")
        .map_err(|error| error.to_string())?;
    Ok(())
}

#[tauri::command]
pub fn delete_local_data(
    confirmation: String,
    state: State<'_, AgentState>,
    app: tauri::AppHandle,
) -> Result<Value, String> {
    if confirmation != "DELETE" {
        return Err("Type DELETE to confirm local erasure.".to_string());
    }
    crate::local_agent::restore_on_exit();
    crate::local_agent::clear_pending_after_data_deletion();
    crate::local_agent::browser_bridge::queue_unblock_all();
    crate::report_schedule::clear_weekly_report_schedule()?;
    crate::telemetry::set_running(false);
    if let Some(agent) = state.lock().unwrap().as_mut() {
        agent.is_running = false;
        agent.reports_sent = 0;
    }
    let db_path = crate::paths::db_path()?;
    let conn = Connection::open(&db_path).map_err(|error| error.to_string())?;
    ensure_schema(&conn)?;
    erase_local_database(&conn)?;
    let warnings = remove_runtime_artifacts(&app);
    Ok(json!({ "deleted": true, "warnings": warnings }))
}

#[tauri::command]
pub fn delete_cloud_account(
    confirmation: String,
    state: State<'_, AgentState>,
    app: tauri::AppHandle,
) -> Result<Value, String> {
    if confirmation != "DELETE" {
        return Err("Type DELETE to confirm account erasure.".to_string());
    }
    let db_path = crate::paths::db_path()?;
    let conn = Connection::open(&db_path).map_err(|error| error.to_string())?;
    let session = get_user_session_from_conn(&conn).ok_or("No cloud account is signed in.")?;
    let analytics_credentials = crate::anonymous_analytics::analytics_export_credentials(&db_path)?;
    let response = Client::builder()
        .timeout(Duration::from_secs(60))
        .build()
        .map_err(|error| error.to_string())?
        .post(format!("{}/functions/v1/privacy-rights", supabase_url()))
        .header("apikey", supabase_anon_key())
        .bearer_auth(session.access_token)
        .json(&json!({
            "action": "delete_account",
            "confirmation": "DELETE",
            "anonymous_id": analytics_credentials.as_ref().map(|value| &value.0),
            "anonymous_secret": analytics_credentials.as_ref().map(|value| &value.1),
        }))
        .send()
        .map_err(|error| format!("Could not request account deletion: {error}"))?;
    let status = response.status();
    let payload: Value = response.json().unwrap_or_else(|_| json!({}));
    if !status.is_success() {
        return Err(payload["error"]
            .as_str()
            .unwrap_or("Cloud account deletion failed.")
            .to_string());
    }
    drop(conn);
    delete_local_data(confirmation, state, app)?;
    Ok(json!({ "deleted": true }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    fn test_db() -> (tempfile::TempDir, std::path::PathBuf) {
        let dir = tempdir().unwrap();
        let path = dir.path().join("privacy.db");
        let conn = Connection::open(&path).unwrap();
        conn.execute_batch(
            "CREATE TABLE config (key TEXT PRIMARY KEY, value TEXT);
             CREATE TABLE reports (
               id INTEGER PRIMARY KEY, description TEXT, activity_type TEXT,
               synced INTEGER DEFAULT 0, created_at TEXT DEFAULT CURRENT_TIMESTAMP,
               jira_ticket_id TEXT, duration_seconds INTEGER DEFAULT 30,
               active_app TEXT, window_title TEXT, capture_source TEXT, theme_hint TEXT
             );",
        )
        .unwrap();
        ensure_schema(&conn).unwrap();
        (dir, path)
    }

    #[test]
    fn privacy_defaults_disable_all_optional_transfers() {
        let (_dir, path) = test_db();
        let settings = load_privacy_settings(&path).unwrap();
        assert!(!settings.cloud_sync_enabled);
        assert!(!settings.cloud_ai_enabled);
        assert!(!settings.store_window_titles);
        assert_eq!(settings.retention_days, 30);
        assert_eq!(settings.notice_version, PRIVACY_NOTICE_VERSION);
        assert_eq!(settings.excluded_applications.len(), 8);
        for application in [
            "Bitwarden",
            "Keychain Access",
            "SecurityAgent",
            "loginwindow",
        ] {
            assert!(settings
                .excluded_applications
                .contains(&application.to_string()));
        }
    }

    #[test]
    fn excluded_application_matching_is_case_insensitive_and_ignores_exe_suffix() {
        let (_dir, path) = test_db();
        assert!(application_is_excluded(&path, Some("bitwarden.EXE")));
        assert!(application_is_excluded(&path, Some("  KeePassXC.exe  ")));
        assert!(!application_is_excluded(&path, Some("code.exe")));
        assert!(application_is_excluded(&path, None));
    }

    #[test]
    fn retention_removes_only_expired_reports() {
        let (_dir, path) = test_db();
        let conn = Connection::open(&path).unwrap();
        conn.execute(
            "INSERT INTO reports (description, activity_type, created_at) VALUES ('old','Coding',datetime('now','-31 days'))",
            [],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO reports (description, activity_type, created_at) VALUES ('new','Coding',datetime('now','-1 day'))",
            [],
        )
        .unwrap();
        drop(conn);
        assert_eq!(enforce_local_retention(&path).unwrap(), 1);
    }

    #[test]
    fn retention_clears_titles_unless_storage_is_explicitly_enabled() {
        let (_dir, path) = test_db();
        let conn = Connection::open(&path).unwrap();
        conn.execute(
            "INSERT INTO reports (description, activity_type, window_title) VALUES ('new','Coding','Sensitive title')",
            [],
        )
        .unwrap();
        drop(conn);

        enforce_local_retention(&path).unwrap();
        let conn = Connection::open(&path).unwrap();
        let cleared: Option<String> = conn
            .query_row("SELECT window_title FROM reports LIMIT 1", [], |row| {
                row.get(0)
            })
            .unwrap();
        assert!(cleared.is_none());

        let mut settings = load_privacy_settings(&path).unwrap();
        settings.store_window_titles = true;
        save_privacy_settings(&conn, &settings).unwrap();
        conn.execute("UPDATE reports SET window_title = 'Allowed title'", [])
            .unwrap();
        drop(conn);

        enforce_local_retention(&path).unwrap();
        let conn = Connection::open(&path).unwrap();
        let retained: Option<String> = conn
            .query_row("SELECT window_title FROM reports LIMIT 1", [], |row| {
                row.get(0)
            })
            .unwrap();
        assert_eq!(retained.as_deref(), Some("Allowed title"));
    }
}
