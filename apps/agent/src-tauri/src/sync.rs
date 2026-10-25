use crate::sync_env::{supabase_anon_key, supabase_url};
use crate::sync_pure::{
    clamp_line_for_summary, jwt_exp, select_owned_pending_sql, truncate_tasks_for_summary,
};
use crate::vision_model::LLAMA_CHAT_MODEL_ID;
use reqwest::blocking::{Client, Response};
use rusqlite::Connection;
use serde::{Deserialize, Serialize};
use std::thread;
use std::time::Duration;

const SYNC_INTERVAL_MINS: u64 = 10;
/// Max rows per cloud upload batch (oldest unsynced first). Override with `FLOWSIGHT_SYNC_BATCH_LIMIT`.
const CLOUDSYNC_BATCH_LIMIT_DEFAULT: u64 = 500;
/// Refresh the access token when it is expired or within this many seconds of expiring.
const JWT_REFRESH_MARGIN_SECS: i64 = 300;
/// Background poll interval for proactive JWT renewal.
const TOKEN_REFRESH_POLL_SECS: u64 = 120;
pub(crate) static SESSION_REFRESH_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());
/// Max Unicode characters of TASKS text sent to the local `/v1/chat/completions` endpoint.
/// Default llama.cpp servers often use `n_ctx=2048`; prompt = instructions + tasks must stay under that.
/// Override with env `FLOWSIGHT_SUMMARY_MAX_CHARS` (same unit: Unicode chars).
const SUMMARY_MAX_TASK_CHARS: usize = 5000;
/// Avoid one verbose vision capture consuming the whole summary budget (`FLOWSIGHT_SUMMARY_MAX_LINE_CHARS` to override).
const SUMMARY_MAX_LINE_CHARS_DEFAULT: usize = 450;

// User session stored locally after login
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UserSession {
    pub user_id: String,
    pub team_id: Option<String>,
    pub access_token: String,
    pub refresh_token: Option<String>,
    pub email: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct PublicUserSession {
    pub user_id: String,
    pub team_id: Option<String>,
    pub email: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct CloudCaptureOwner {
    pub user_id: String,
    pub team_id: String,
    pub sharing_epoch: String,
}

pub(crate) fn ensure_capture_schema(conn: &Connection) -> Result<(), String> {
    // Legacy report tables can lack the modern id alias. Retention/erasure
    // explicitly prune bindings instead of requiring a new foreign key.
    conn.execute_batch("CREATE TABLE IF NOT EXISTS cloud_observation_owner(report_id INTEGER PRIMARY KEY,user_id TEXT NOT NULL,team_id TEXT NOT NULL,sharing_epoch TEXT NOT NULL); CREATE INDEX IF NOT EXISTS cloud_owner_scope ON cloud_observation_owner(user_id,team_id,sharing_epoch,report_id);").map_err(|e| e.to_string())
}

fn cloud_owner_from_conn(conn: &Connection) -> Option<CloudCaptureOwner> {
    let session = get_user_session_from_conn(conn)?;
    let entitlements = crate::entitlements::load_entitlements(conn);
    if !entitlements.can_sync || entitlements.usage_profile.is_none() {
        return None;
    }
    let raw = conn
        .query_row(
            "SELECT value FROM config WHERE key='privacy_settings'",
            [],
            |row| row.get::<_, String>(0),
        )
        .ok()?;
    let settings: crate::privacy::PrivacySettings = serde_json::from_str(&raw).ok()?;
    if !settings.cloud_sync_enabled {
        return None;
    }
    Some(CloudCaptureOwner {
        user_id: session.user_id,
        team_id: session.team_id?,
        sharing_epoch: settings.cloud_sync_enabled_at?,
    })
}

pub(crate) fn cloud_capture_owner(path: &std::path::Path) -> Option<CloudCaptureOwner> {
    cloud_owner_from_conn(&Connection::open(path).ok()?)
}

pub(crate) fn bind_cloud_observation(
    conn: &Connection,
    report_id: i64,
    captured: Option<&CloudCaptureOwner>,
) {
    if ensure_capture_schema(conn).is_err() {
        return;
    }
    if let Some(owner) =
        captured.filter(|owner| cloud_owner_from_conn(conn).as_ref() == Some(*owner))
    {
        let _=conn.execute("INSERT OR IGNORE INTO cloud_observation_owner(report_id,user_id,team_id,sharing_epoch) VALUES(?1,?2,?3,?4)",rusqlite::params![report_id,owner.user_id,owner.team_id,owner.sharing_epoch]);
    }
}

pub fn start_sync_thread(db_path: std::path::PathBuf) {
    let path_clone = db_path.clone();
    thread::spawn(move || {
        // Run once immediately so the first cloud batch is not delayed by SYNC_INTERVAL_MINS.
        let _ = perform_sync(&path_clone);
        loop {
            thread::sleep(Duration::from_secs(SYNC_INTERVAL_MINS * 60));
            let _ = perform_sync(&path_clone);
        }
    });
}

/// Proactively refreshes the Supabase session when the access token is missing, expired,
/// or close to expiry. Safe to call from a background thread.
pub(crate) fn refresh_session_if_expiring(db_path: &std::path::PathBuf) {
    let Ok(conn) = Connection::open(db_path) else {
        return;
    };
    let Some(session) = get_user_session(&conn) else {
        return;
    };
    if session.refresh_token.is_none() {
        return;
    }

    let exp = jwt_exp(&session.access_token);
    let now = chrono::Utc::now().timestamp();
    if exp > 0 && exp - now > JWT_REFRESH_MARGIN_SECS {
        return;
    }

    match refresh_supabase_token(&session) {
        Ok(new_session) => {
            println!(
                "[Sync] Proactive JWT refresh OK (previous access exp: {})",
                exp
            );
            if let Ok(entitlements) = crate::entitlements::refresh_entitlements_from_supabase(
                &new_session.access_token,
                &new_session.user_id,
            ) {
                let _ = crate::entitlements::save_entitlements(&conn, &entitlements);
            }
        }
        Err(e) => println!("[Sync] Proactive JWT refresh failed: {}", e),
    }
}

pub fn start_token_refresh_thread(db_path: std::path::PathBuf) {
    thread::spawn(move || {
        refresh_session_if_expiring(&db_path);
        loop {
            thread::sleep(Duration::from_secs(TOKEN_REFRESH_POLL_SECS));
            refresh_session_if_expiring(&db_path);
        }
    });
}

// Get user session from local config
pub(crate) fn get_user_session_from_conn(conn: &Connection) -> Option<UserSession> {
    // 1. Try 'user_session' (Internal sync session - has team_id)
    let user_session: Option<UserSession> = crate::secure_config::load_secret(conn, "user_session")
        .ok()
        .flatten()
        .and_then(|json_str| serde_json::from_str(&json_str).ok());

    // 2. Try 'auth_session' (OAuth session from auth.rs)
    let auth_session: Option<serde_json::Value> =
        crate::secure_config::load_secret(conn, "auth_session")
            .ok()
            .flatten()
            .and_then(|json_str| serde_json::from_str(&json_str).ok());

    match (user_session, auth_session) {
        // Both exist: merge only the same Supabase identity. A stale auth_session
        // from another FlowSight account must never donate its JWT to this user.
        (Some(mut us), Some(auth)) => {
            let provider = auth["provider"].as_str().unwrap_or("");
            let same_user = auth["user"]["id"].as_str() == Some(us.user_id.as_str());
            if same_user && (provider == "google" || provider == "manual") {
                if let Some(auth_token) = auth["access_token"].as_str() {
                    if auth_token != us.access_token {
                        // Only merge if auth_session token is actually newer (decode JWT exp)
                        let auth_exp = jwt_exp(auth_token);
                        let us_exp = jwt_exp(&us.access_token);
                        if auth_exp > us_exp {
                            println!("[Sync] Merging fresher Supabase tokens into user_session (auth_exp={} > us_exp={})", auth_exp, us_exp);
                            us.access_token = auth_token.to_string();
                            if let Some(rt) = auth["refresh_token"].as_str() {
                                us.refresh_token = Some(rt.to_string());
                            }
                            let json = serde_json::to_string(&us).unwrap_or_default();
                            let _ = crate::secure_config::save_secret(conn, "user_session", &json);
                        }
                        // else: keep user_session JWT (fresher); no log — token refresh polls hit this often.
                    }
                }
            }
            // Non-Supabase or different-user auth_session: keep user_session only.
            Some(us)
        }
        // Only user_session exists: use as-is
        (Some(us), None) => Some(us),
        // Only auth_session exists: build UserSession from it (only if Supabase)
        (None, Some(v)) => {
            let provider = v["provider"].as_str().unwrap_or("");
            if provider == "google" || provider == "manual" {
                Some(UserSession {
                    user_id: v["user"]["id"].as_str()?.to_string(),
                    team_id: None,
                    access_token: v["access_token"].as_str()?.to_string(),
                    refresh_token: v["refresh_token"].as_str().map(|t| t.to_string()),
                    email: v["user"]["email"].as_str()?.to_string(),
                })
            } else {
                println!(
                    "[Sync] auth_session is {} (not Supabase), no user_session available",
                    provider
                );
                None
            }
        }
        // Neither exists
        (None, None) => None,
    }
}

fn get_user_session(conn: &Connection) -> Option<UserSession> {
    get_user_session_from_conn(conn)
}

// Save user session to local config
#[tauri::command]
pub fn save_user_session(
    app: tauri::AppHandle,
    user_id: String,
    team_id: Option<String>,
    access_token: String,
    refresh_token: Option<String>,
    email: String,
) -> Result<(), String> {
    use tauri::Manager;
    let conn = Connection::open(crate::paths::db_path()?).map_err(|e| e.to_string())?;
    let previous = get_user_session_from_conn(&conn);
    if previous.as_ref().map(|s| s.user_id.as_str()) != Some(user_id.as_str())
        || previous.as_ref().and_then(|s| s.team_id.as_ref()) != team_id.as_ref()
    {
        crate::agent::stop_monitoring(app.state::<crate::agent::AgentState>())?;
        crate::local_agent::end_personal_context()?;
    }
    save_user_session_local(user_id, team_id, access_token, refresh_token, email)
}

fn save_user_session_local(
    user_id: String,
    team_id: Option<String>,
    access_token: String,
    refresh_token: Option<String>,
    email: String,
) -> Result<(), String> {
    let _guard = SESSION_REFRESH_LOCK
        .lock()
        .map_err(|_| "Session unavailable")?;
    let db_path = crate::paths::db_path()?;
    let conn = Connection::open(&db_path).map_err(|e| e.to_string())?;
    if get_user_session_from_conn(&conn)
        .as_ref()
        .map(|s| s.user_id.as_str())
        != Some(user_id.as_str())
    {
        crate::privacy::disable_cloud_for_context_change(&conn)?;
        crate::privacy::forget_monitoring_acknowledgement(&conn)?;
    }

    let session = UserSession {
        user_id,
        team_id,
        access_token,
        refresh_token,
        email,
    };
    if let Some(previous) = get_user_session_from_conn(&conn) {
        if previous.user_id != session.user_id || previous.team_id != session.team_id {
            crate::privacy::disable_cloud_for_context_change(&conn)?;
        }
    }
    let json = serde_json::to_string(&session).map_err(|e| e.to_string())?;

    crate::secure_config::save_secret(&conn, "user_session", &json)?;

    println!("[Sync] User session saved");
    Ok(())
}

pub(crate) fn refresh_supabase_token(session: &UserSession) -> Result<UserSession, String> {
    let _guard = SESSION_REFRESH_LOCK
        .lock()
        .map_err(|_| "Session unavailable")?;
    let conn = Connection::open(crate::paths::db_path()?).map_err(|e| e.to_string())?;
    recover_session(&conn, session, request_refreshed_session)
}

/// An in-flight request cannot restore an account after logout or donate a token
/// to another account. Concurrent callers reuse an already rotated session.
fn recover_session(
    conn: &Connection,
    rejected: &UserSession,
    refresh: impl FnOnce(&UserSession) -> Result<UserSession, String>,
) -> Result<UserSession, String> {
    let current = get_user_session_from_conn(conn).ok_or("Sign in again to use online leagues")?;
    if current.user_id != rejected.user_id {
        return Err("The FlowSight account changed. Retry with the current account".into());
    }
    if current.access_token != rejected.access_token {
        return Ok(current);
    }
    let refreshed = refresh(&current)?;
    if refreshed.user_id != current.user_id {
        return Err("Session identity could not be verified".into());
    }
    crate::secure_config::save_secret(
        conn,
        "user_session",
        &serde_json::to_string(&refreshed).map_err(|e| e.to_string())?,
    )?;
    Ok(refreshed)
}

fn request_refreshed_session(session: &UserSession) -> Result<UserSession, String> {
    let refresh_token = session
        .refresh_token
        .as_ref()
        .ok_or("No refresh token available in session")?;

    println!("[Sync] Attempting token refresh");
    let client = Client::builder()
        .timeout(Duration::from_secs(20))
        .build()
        .map_err(|_| "Could not reconnect to FlowSight. Check your connection and retry")?;
    let url = format!("{}/auth/v1/token?grant_type=refresh_token", supabase_url());

    let resp = client
        .post(&url)
        .header("apikey", supabase_anon_key())
        .json(&serde_json::json!({ "refresh_token": refresh_token }))
        .send()
        .map_err(|_| "Could not reconnect to FlowSight. Check your connection and retry")?;

    let status = resp.status();
    if !status.is_success() {
        println!("[Sync] Refresh failed with HTTP {}", status);
        return Err(if status.as_u16() == 400 || status.as_u16() == 401 {
            "Sign in again to use online leagues".to_string()
        } else {
            "FlowSight could not renew the session. Retry shortly".to_string()
        });
    }

    let json: serde_json::Value = resp.json().map_err(|e| e.to_string())?;
    if json["user"]["id"].as_str() != Some(session.user_id.as_str()) {
        return Err("Session identity could not be verified".into());
    }
    let new_access = json["access_token"]
        .as_str()
        .ok_or("Missing access_token in refresh response")?;
    let new_refresh = json["refresh_token"].as_str();

    let mut new_session = session.clone();
    new_session.access_token = new_access.to_string();
    if let Some(r) = new_refresh {
        new_session.refresh_token = Some(r.to_string());
    }

    Ok(new_session)
}

// Clear user session (logout)
#[tauri::command]
pub fn clear_user_session(state: tauri::State<'_, crate::agent::AgentState>) -> Result<(), String> {
    let team = !crate::entitlements::personal_features_allowed(&crate::paths::db_path()?);
    crate::agent::stop_monitoring(state)?;
    crate::local_agent::end_personal_context()?;
    crate::calendar_companion::on_cloud_logout();
    let _guard = SESSION_REFRESH_LOCK
        .lock()
        .map_err(|_| "Session unavailable")?;
    let db_path = crate::paths::db_path()?;
    let conn = Connection::open(&db_path).map_err(|e| e.to_string())?;
    crate::privacy::disable_cloud_for_context_change(&conn)?;
    if team {
        crate::privacy::forget_monitoring_acknowledgement(&conn)?;
    }

    crate::secure_config::delete_secret(&conn, "user_session")?;
    crate::secure_config::delete_secret(&conn, "auth_session")?;
    crate::entitlements::clear_entitlements(&conn)?;

    println!("[Sync] User session cleared");
    Ok(())
}

// Check if user is logged in
#[tauri::command]
pub fn get_current_user() -> Result<Option<PublicUserSession>, String> {
    let db_path = crate::paths::db_path()?;
    let conn = Connection::open(&db_path).map_err(|e| e.to_string())?;
    Ok(get_user_session(&conn).map(|session| PublicUserSession {
        user_id: session.user_id,
        team_id: session.team_id,
        email: session.email,
    }))
}

fn perform_sync(db_path: &std::path::PathBuf) -> Result<String, String> {
    if !crate::privacy::cloud_sync_enabled(db_path) {
        return Ok("Cloud activity sync is off in Privacy & data.".to_string());
    }

    refresh_session_if_expiring(db_path);

    let conn = Connection::open(db_path).map_err(|e| e.to_string())?;

    // Check if user is logged in
    let mut session = match get_user_session(&conn) {
        Some(s) => s,
        None => {
            println!("[CloudSync] No user session found. Sync disabled.");
            return Ok("Not logged in - sync disabled".to_string());
        }
    };

    if let Err(reason) = crate::entitlements::require_feature(db_path, "sync") {
        println!("[CloudSync] {}", reason);
        return Ok(reason);
    }

    let entitlements = crate::entitlements::refresh_entitlements_from_supabase(
        &session.access_token,
        &session.user_id,
    )?;
    {
        crate::entitlements::save_entitlements(&conn, &entitlements)?;
        if !entitlements.can_sync {
            println!("[CloudSync] License inactive — sync disabled.");
            return Ok("License inactive — sync disabled".to_string());
        }
    }
    session = get_user_session(&conn).ok_or("Account signed out while preparing sync")?;
    ensure_capture_schema(&conn)?;
    let Some(owner) = cloud_owner_from_conn(&conn) else {
        return Ok("Sharing is off for the current profile".into());
    };

    let batch_limit = std::env::var("FLOWSIGHT_SYNC_BATCH_LIMIT")
        .ok()
        .and_then(|s| s.parse::<u64>().ok())
        .filter(|&n| n > 0)
        .unwrap_or(CLOUDSYNC_BATCH_LIMIT_DEFAULT)
        .min(5000) as usize;

    let total_unsynced: i64 = conn
        .query_row("SELECT COUNT(*) FROM reports r JOIN cloud_observation_owner o ON o.report_id=r.id WHERE r.synced=0 AND o.user_id=?1 AND o.team_id=?2 AND o.sharing_epoch=?3", rusqlite::params![owner.user_id,owner.team_id,owner.sharing_epoch], |r| {
            r.get::<_, i64>(0)
        })
        .unwrap_or(0);
    println!(
        "[CloudSync] Pending unsynced reports: {} (uploading oldest up to {} rows)",
        total_unsynced, batch_limit
    );

    let session_date: String=conn.query_row("SELECT date(r.created_at) FROM reports r JOIN cloud_observation_owner o ON o.report_id=r.id WHERE r.synced=0 AND o.user_id=?1 AND o.team_id=?2 AND o.sharing_epoch=?3 ORDER BY r.id LIMIT 1",rusqlite::params![owner.user_id,owner.team_id,owner.sharing_epoch],|row| row.get(0)).unwrap_or_default();
    let captured_at: String=conn.query_row("SELECT r.created_at FROM reports r JOIN cloud_observation_owner o ON o.report_id=r.id WHERE r.synced=0 AND o.user_id=?1 AND o.team_id=?2 AND o.sharing_epoch=?3 ORDER BY r.id LIMIT 1",rusqlite::params![owner.user_id,owner.team_id,owner.sharing_epoch],|row| row.get(0)).unwrap_or_default();
    let sql = select_owned_pending_sql(batch_limit);
    let mut stmt = conn.prepare(&sql).map_err(|e| e.to_string())?;

    let rows = stmt
        .query_map(
            rusqlite::params![
                owner.user_id,
                owner.team_id,
                owner.sharing_epoch,
                session_date
            ],
            |row| {
                Ok((
                    row.get::<_, i64>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, i32>(3)?,
                    row.get::<_, Option<String>>(4)?,
                ))
            },
        )
        .map_err(|e| e.to_string())?;

    let mut ids = Vec::new();
    let mut full_text = String::new();
    let mut total_duration = 0;

    // Aggregations
    let mut categories = std::collections::HashMap::new();
    let mut tickets = std::collections::HashMap::new();

    let line_cap = std::env::var("FLOWSIGHT_SUMMARY_MAX_LINE_CHARS")
        .ok()
        .and_then(|s| s.parse::<usize>().ok())
        .filter(|&n| n >= 80)
        .unwrap_or(SUMMARY_MAX_LINE_CHARS_DEFAULT);

    for (id, desc, cat, dur, ticket) in rows.flatten() {
        ids.push(id);
        let desc = clamp_line_for_summary(&desc, line_cap);
        full_text.push_str(&format!("- [{}] {}\n", cat, desc));
        total_duration += dur;

        // Stats
        *categories.entry(cat).or_insert(0) += dur;
        if let Some(ticket) = crate::focus_semantics::canonical_ticket_value(ticket.as_deref()) {
            *tickets.entry(ticket).or_insert(0) += dur;
        }
    }

    if (total_unsynced as usize) > ids.len() {
        println!(
            "[CloudSync] {} more unsynced report(s) queued after this batch; will upload on the next run.",
            total_unsynced as usize - ids.len()
        );
    }

    if ids.is_empty() {
        println!("[CloudSync] No new reports to sync.");
        return Ok("No new activity to report.".to_string());
    }

    // 2. Generate summary with local vision model
    println!("[CloudSync] Summarizing {} reports...", ids.len());
    let personal = crate::entitlements::personal_features_allowed(db_path);
    if !personal {
        categories = team_category_totals(&categories);
        let allowed = entitlements
            .usage_profile
            .as_ref()
            .and_then(|p| p["allowed_project_keys"].as_array());
        tickets.retain(|ticket, _| {
            allowed.is_some_and(|keys| {
                keys.iter()
                    .any(|key| key.as_str() == ticket.split_once('-').map(|(project, _)| project))
            })
        });
    }
    let summary = if personal {
        summarize_with_vision_model(&full_text).unwrap_or_else(|e| {
            println!("[CloudSync] Summary generation failed: {}", e);
            "Summary generation failed".to_string()
        })
    } else {
        "Shared work totals. Detailed activity stays on the device.".into()
    };
    println!("[CloudSync] Summary generated ({} chars)", summary.len());

    // Recheck consent and the capture context after any local inference work.
    if cloud_owner_from_conn(&conn).as_ref() != Some(&owner) {
        return Ok("Sharing context changed; upload cancelled".into());
    }
    // 3. Upload to Supabase with user authentication (retry on JWT expired)
    let upload_result = upload_session(
        db_path,
        &owner,
        &session,
        &session_date,
        total_duration,
        &summary,
        &categories,
        &tickets,
    );
    let upload_result = match &upload_result {
        Err(e) if e.contains("401") || e.contains("PGRST3") => {
            println!("[CloudSync] Authentication failed; attempting JWT refresh");
            let conn_refresh = Connection::open(db_path).map_err(|e| e.to_string())?;
            require_cloud_owner(&conn_refresh, &owner, &session)?;
            match refresh_supabase_token(&session) {
                Ok(refreshed) => upload_session(
                    db_path,
                    &owner,
                    &refreshed,
                    &session_date,
                    total_duration,
                    &summary,
                    &categories,
                    &tickets,
                ),
                Err(ref_err) => {
                    println!("[CloudSync] Token refresh failed: {}", ref_err);
                    upload_result
                }
            }
        }
        _ => upload_result,
    };

    match upload_result {
        Ok(_) => {
            println!("[CloudSync] Aggregate upload succeeded");

            let primary_category = categories
                .iter()
                .max_by_key(|(_, &secs)| secs)
                .map(|(c, _)| c.as_str())
                .unwrap_or("mixed")
                .to_string();

            let primary_jira = tickets
                .iter()
                .max_by_key(|(_, &secs)| secs)
                .map(|(t, _)| t.clone());

            // Same AI summary as work_sessions — not the raw SQLite log (that stays local only).
            let activity_body = serde_json::json!({
                "user_id": session.user_id,
                "team_id": session.team_id,
                "description": summary.clone(),
                "category": primary_category,
                "jira_ticket_id": primary_jira,
                "duration_seconds": total_duration,
                "captured_at": chrono::NaiveDateTime::parse_from_str(&captured_at,"%Y-%m-%d %H:%M:%S").map(|date| date.and_utc().to_rfc3339()).unwrap_or_else(|_| chrono::Utc::now().to_rfc3339())
            });

            match post_activity_report_with_refresh(db_path, &owner, &session, &activity_body) {
                Ok(()) => println!("[CloudSync] activity_reports: AI window summary saved"),
                Err(_) => println!(
                    "[CloudSync] activity_reports insert failed after work_sessions was saved"
                ),
            }

            let id_list = ids
                .iter()
                .map(|id| id.to_string())
                .collect::<Vec<_>>()
                .join(",");
            let _ = conn.execute(
                &format!("UPDATE reports SET synced = 1 WHERE id IN ({})", id_list),
                [],
            );

            let sync_meta = serde_json::json!({
                "at": chrono::Utc::now().to_rfc3339(),
                "rows_marked_synced": ids.len(),
                "supabase_project_host": supabase_url()
                    .trim_end_matches('/')
                    .trim_start_matches("https://"),
                "tables": "work_sessions, activity_reports",
            });
            let _ = conn.execute(
                "INSERT OR REPLACE INTO config (key, value) VALUES ('last_cloud_sync', ?1)",
                [sync_meta.to_string()],
            );
            let base = supabase_url();
            let host = base.trim_end_matches('/').trim_start_matches("https://");
            println!(
                "[CloudSync] (testing) {} UTC | {} local capture(s) → {} | work_sessions + activity_reports",
                chrono::Utc::now().format("%Y-%m-%d %H:%M:%S"),
                ids.len(),
                host
            );
        }
        Err(e) => {
            if e.contains("License expired") || e.contains("403") {
                println!("[CloudSync] LICENSE EXPIRED - Sync blocked");
                return Err("License expired. Contact your PM to renew.".to_string());
            }

            println!("[CloudSync] Upload failed");
            return Ok(format!(
                "(Cloud Upload Failed: {})\n\nLOCAL SUMMARY:\n{}",
                e, summary
            ));
        }
    }

    println!("[CloudSync] Processed {} reports.", ids.len());
    Ok(summary)
}

fn summarize_with_vision_model(text: &str) -> Result<String, String> {
    let client = Client::builder()
        .timeout(Duration::from_secs(60))
        .build()
        .map_err(|e| e.to_string())?;

    let max_chars = std::env::var("FLOWSIGHT_SUMMARY_MAX_CHARS")
        .ok()
        .and_then(|s| s.parse::<usize>().ok())
        .unwrap_or(SUMMARY_MAX_TASK_CHARS);

    let n_chars = text.chars().count();
    if n_chars > max_chars {
        println!(
            "[CloudSync] Truncating summary TASKS from {} to {} Unicode chars (local n_ctx limit)",
            n_chars, max_chars
        );
    }
    let tasks = truncate_tasks_for_summary(text, max_chars);

    // Keep instructions short to preserve token budget for TASKS.
    let prompt = format!(
        "Summarize the observed knowledge-work activity below in ONE short paragraph. Use only facts from the list; do not invent completion, productivity, or intent. Preserve useful research, writing, design, planning, communication, administrative, sales, analysis, and software work.\n\nACTIVITY:\n{}",
        tasks
    );

    let body = serde_json::json!({
        "model": LLAMA_CHAT_MODEL_ID,
        "messages": [{ "role": "user", "content": prompt }],
        "temperature": 0.3,
        "max_tokens": 384
    });

    let resp = client
        .post(
            crate::llama_port::managed_chat_completions_url().ok_or_else(|| {
                "Local AI server offline — cannot summarize (start Local AI monitoring first)"
                    .to_string()
            })?,
        )
        .json(&body)
        .send()
        .map_err(|e| e.to_string())?;
    if !resp.status().is_success() {
        let status = resp.status();
        return Err(format!("Summary request failed ({}).", status));
    }

    let json: serde_json::Value = resp.json().map_err(|e| e.to_string())?;
    let content = json["choices"][0]["message"]["content"]
        .as_str()
        .unwrap_or("");
    if content.is_empty() {
        return Err("Model returned empty summary.".to_string());
    }

    Ok(content.to_string())
}

fn require_cloud_owner(
    conn: &Connection,
    owner: &CloudCaptureOwner,
    session: &UserSession,
) -> Result<(), String> {
    if session.user_id != owner.user_id
        || session.team_id.as_deref() != Some(owner.team_id.as_str())
        || cloud_owner_from_conn(conn).as_ref() != Some(owner)
    {
        return Err("Sharing context changed; upload cancelled".into());
    }
    Ok(())
}

#[allow(clippy::too_many_arguments)] // Explicit capture/recipient boundary, rechecked before every retry.
fn upload_session(
    path: &std::path::Path,
    owner: &CloudCaptureOwner,
    session: &UserSession,
    session_date: &str,
    duration: i32,
    summary: &str,
    categories: &std::collections::HashMap<String, i32>,
    tickets: &std::collections::HashMap<String, i32>,
) -> Result<(), String> {
    require_cloud_owner(
        &Connection::open(path).map_err(|e| e.to_string())?,
        owner,
        session,
    )?;
    let client = Client::new();
    let url = format!("{}/rest/v1/work_sessions", supabase_url());

    let body = serde_json::json!({
        "user_id": session.user_id,
        "team_id": session.team_id,
        "duration_seconds": duration,
        "summary": summary,
        "category_breakdown": categories,
        "jira_breakdown": tickets,
        "session_date": session_date,
        "created_at": chrono::Utc::now().to_rfc3339()
    });

    let resp = client
        .post(&url)
        .header("apikey", supabase_anon_key())
        .header("Authorization", format!("Bearer {}", session.access_token))
        .header("Content-Type", "application/json")
        .header("Prefer", "return=minimal")
        .json(&body)
        .send()
        .map_err(|e| e.to_string())?;

    let status = resp.status();

    if status.as_u16() == 403 {
        return Err("License expired or invalid".to_string());
    }

    if !status.is_success() {
        return Err(format!("HTTP {}", status));
    }

    Ok(())
}

fn post_activity_report_row(
    session: &UserSession,
    body: &serde_json::Value,
) -> Result<Response, String> {
    let client = Client::new();
    let url = format!("{}/rest/v1/activity_reports", supabase_url());
    client
        .post(&url)
        .header("apikey", supabase_anon_key())
        .header("Authorization", format!("Bearer {}", session.access_token))
        .header("Content-Type", "application/json")
        .header("Prefer", "return=minimal")
        .json(body)
        .send()
        .map_err(|e| e.to_string())
}

/// Retries once with a refreshed JWT if the first POST returns 401/403.
fn post_activity_report_with_refresh(
    db_path: &std::path::PathBuf,
    owner: &CloudCaptureOwner,
    session: &UserSession,
    body: &serde_json::Value,
) -> Result<(), String> {
    let conn = Connection::open(db_path).map_err(|e| e.to_string())?;
    require_cloud_owner(&conn, owner, session)?;
    let mut resp = post_activity_report_row(session, body)?;
    if resp.status().as_u16() == 401 || resp.status().as_u16() == 403 {
        require_cloud_owner(&conn, owner, session)?;
        if let Ok(new_s) = refresh_supabase_token(session) {
            require_cloud_owner(&conn, owner, &new_s)?;
            resp = post_activity_report_row(&new_s, body)?;
        }
    }
    let status = resp.status();
    if status.as_u16() == 403 {
        return Err("License expired or invalid".to_string());
    }
    if !status.is_success() {
        return Err(format!("HTTP {}", status));
    }
    Ok(())
}

// Get all teams the current user belongs to.
// Async so Tauri dispatches it off the main/UI thread (see crash_guard.rs module docs).
#[tauri::command]
pub async fn get_user_teams() -> Result<serde_json::Value, String> {
    tauri::async_runtime::spawn_blocking(get_user_teams_blocking)
        .await
        .map_err(|e| format!("Task join error: {}", e))?
}

fn get_user_teams_blocking() -> Result<serde_json::Value, String> {
    let db_path = crate::paths::db_path()?;
    let conn = Connection::open(&db_path).map_err(|e| e.to_string())?;

    let session = get_user_session(&conn).ok_or("Not logged in")?;

    let client = Client::new();
    let mut current_token = session.access_token.clone();

    // Fetch team memberships from Supabase
    let url = format!(
        "{}/rest/v1/team_members?user_id=eq.{}&select=team_id,role,joined_at",
        supabase_url(),
        session.user_id
    );

    let mut resp = client
        .get(&url)
        .header("apikey", supabase_anon_key())
        .header("Authorization", format!("Bearer {}", current_token))
        .send()
        .map_err(|e| e.to_string())?;

    // Retry on 401/403
    if resp.status().as_u16() == 401 || resp.status().as_u16() == 403 {
        if let Ok(new_s) = refresh_supabase_token(&session) {
            current_token = new_s.access_token.clone();
            resp = client
                .get(&url)
                .header("apikey", supabase_anon_key())
                .header("Authorization", format!("Bearer {}", current_token))
                .send()
                .map_err(|e| e.to_string())?;
        }
    }

    let status = resp.status();
    if !status.is_success() {
        return Err(format!("Failed to fetch teams (HTTP {}).", status));
    }

    let teams: Vec<serde_json::Value> = resp.json().map_err(|e| e.to_string())?;

    // Auto-elegir primer team si no hay uno activo persistido todavía.
    // Evita que activity_reports/work_sessions suban con team_id=NULL cuando el
    // usuario ya tiene membresía(s) pero nunca tocó el dropdown (el navegador
    // muestra la primera opción sin disparar `change`, por eso `set_active_team`
    // no se invocaba).
    let active_team_id = session.team_id.clone();

    println!("[Team] Found {} team memberships", teams.len());

    Ok(serde_json::json!({
        "teams": teams,
        "active_team_id": active_team_id
    }))
}

// Set the active team for the current user (persists to SQLite)
#[tauri::command]
pub async fn set_active_team(team_id: String, app: tauri::AppHandle) -> Result<(), String> {
    tauri::async_runtime::spawn_blocking(move || {
        crate::entitlements::set_usage_profile_blocking("team".into(), Some(team_id), &app)
            .map(|_| ())
    })
    .await
    .map_err(|_| "Team change interrupted".to_string())?
}

// Join a team using an invitation token.
// Async so Tauri dispatches it off the main/UI thread (see crash_guard.rs module docs).
#[tauri::command]
pub async fn join_team(token: String, app: tauri::AppHandle) -> Result<serde_json::Value, String> {
    tauri::async_runtime::spawn_blocking(move || join_team_blocking(token, &app))
        .await
        .map_err(|e| format!("Task join error: {}", e))?
}

fn join_team_blocking(token: String, app: &tauri::AppHandle) -> Result<serde_json::Value, String> {
    use tauri::Manager;
    let path = crate::paths::db_path()?;
    refresh_session_if_expiring(&path);
    let conn = Connection::open(&path).map_err(|e| e.to_string())?;
    let session = get_user_session(&conn).ok_or("Sign in before joining a team")?;
    let send = |active: &UserSession| -> Result<(reqwest::StatusCode, serde_json::Value), String> {
        let response = Client::builder()
            .timeout(Duration::from_secs(20))
            .build()
            .map_err(|e| e.to_string())?
            .post(format!(
                "{}/rest/v1/rpc/join_flowsight_team",
                supabase_url()
            ))
            .header("apikey", supabase_anon_key())
            .bearer_auth(&active.access_token)
            .json(&serde_json::json!({"p_token":token}))
            .send()
            .map_err(|_| "Could not connect to FlowSight. Check your connection and retry")?;
        let status = response.status();
        Ok((
            status,
            response
                .json()
                .map_err(|_| "Team invitation response unavailable")?,
        ))
    };
    let (mut status, mut result) = send(&session)?;
    if status.as_u16() == 401 {
        (status, result) = send(&refresh_supabase_token(&session)?)?;
    }
    if !status.is_success() {
        return Err(result["message"]
            .as_str()
            .unwrap_or("Team invitation could not be accepted")
            .to_string());
    }
    if get_user_session(&conn).as_ref().map(|s| s.user_id.as_str())
        != Some(session.user_id.as_str())
    {
        return Err("Account changed while joining the team".into());
    }
    crate::agent::stop_monitoring(app.state::<crate::agent::AgentState>())?;
    crate::local_agent::end_personal_context()?;
    crate::entitlements::refresh_entitlements()?;
    Ok(result)
}

fn team_category_totals(
    categories: &std::collections::HashMap<String, i32>,
) -> std::collections::HashMap<String, i32> {
    let mut totals = std::collections::HashMap::new();
    for (category, seconds) in categories {
        let bucket = match category.to_ascii_lowercase().as_str() {
            "meeting" | "meetings" | "clientcalls" => "meetings",
            "coding" | "debugging" | "testing" | "devops" | "database" | "design"
            | "prototyping" | "uxdesign" | "writing" | "editing" | "contentcreation"
            | "copywriting" | "campaignmanagement" | "seo" | "documentation" | "deep_work"
            | "deep work" | "focus work" | "work" => "work",
            _ => "other",
        };
        let count: &mut i32 = totals.entry(bucket.to_string()).or_insert(0);
        *count = count.saturating_add((*seconds).max(0));
    }
    totals
}

#[cfg(test)]
mod user_session_tests {
    use super::UserSession;

    #[cfg(any(windows, target_os = "macos"))]
    use super::get_user_session_from_conn;

    #[cfg(any(windows, target_os = "macos"))]
    fn session_fixture() -> (rusqlite::Connection, UserSession) {
        let conn = rusqlite::Connection::open_in_memory().unwrap();
        conn.execute_batch("CREATE TABLE config(key TEXT PRIMARY KEY,value TEXT); CREATE TABLE reports(id INTEGER PRIMARY KEY,description TEXT,activity_type TEXT,duration_seconds INTEGER,jira_ticket_id TEXT,created_at TEXT,synced INTEGER);").unwrap();
        let user = UserSession {
            user_id: "owner-a".into(),
            team_id: Some("team-a".into()),
            access_token: "old-access".into(),
            refresh_token: Some("old-refresh".into()),
            email: "fixture@example.test".into(),
        };
        crate::secure_config::save_secret(
            &conn,
            "user_session",
            &serde_json::to_string(&user).unwrap(),
        )
        .unwrap();
        (conn, user)
    }

    #[cfg(any(windows, target_os = "macos"))]
    #[test]
    fn session_recovery_reuses_rotation_and_never_restores_logged_out_or_other_account() {
        let (conn, user) = session_fixture();
        let mut rotated = user.clone();
        rotated.access_token = "rotated-access".into();
        rotated.refresh_token = Some("rotated-refresh".into());
        crate::secure_config::save_secret(
            &conn,
            "user_session",
            &serde_json::to_string(&rotated).unwrap(),
        )
        .unwrap();
        let reused =
            super::recover_session(&conn, &user, |_| panic!("Must reuse the rotated token"))
                .unwrap();
        assert_eq!(reused.refresh_token, rotated.refresh_token);
        let mut other = rotated.clone();
        other.user_id = "owner-b".into();
        crate::secure_config::save_secret(
            &conn,
            "user_session",
            &serde_json::to_string(&other).unwrap(),
        )
        .unwrap();
        assert!(super::recover_session(&conn, &user, |_| panic!(
            "Must not refresh another identity"
        ))
        .is_err());
        crate::secure_config::delete_secret(&conn, "user_session").unwrap();
        assert!(
            super::recover_session(&conn, &user, |_| panic!("Must not restore logout")).is_err()
        );
    }

    #[cfg(any(windows, target_os = "macos"))]
    #[test]
    fn session_recovery_persists_the_rotated_pair() {
        let (conn, user) = session_fixture();
        let recovered = super::recover_session(&conn, &user, |s| {
            let mut next = s.clone();
            next.access_token = "new-access".into();
            next.refresh_token = Some("new-refresh".into());
            Ok(next)
        })
        .unwrap();
        let saved = get_user_session_from_conn(&conn).unwrap();
        assert_eq!(saved.access_token, recovered.access_token);
        assert_eq!(saved.refresh_token, recovered.refresh_token);
    }

    #[cfg(any(windows, target_os = "macos"))]
    #[test]
    fn cloud_queue_is_scoped_and_retries_reject_changed_recipient_or_grant() {
        let (conn, user) = session_fixture();
        let entitlements = crate::entitlements::Entitlements {
            owner_user_id: Some(user.user_id.clone()),
            plan: Some("team".into()),
            status: "active".into(),
            active_team_id: user.team_id.clone(),
            can_sync: true,
            usage_profile: Some(serde_json::json!({"id":"team_employee"})),
            ..Default::default()
        };
        crate::entitlements::save_entitlements(&conn, &entitlements).unwrap();
        conn.execute("INSERT INTO config VALUES('privacy_settings',?1)",[serde_json::json!({"noticeVersion":"2026-08-23","cloudSyncEnabled":true,"cloudSyncEnabledAt":"grant-a"}).to_string()]).unwrap();
        super::ensure_capture_schema(&conn).unwrap();
        conn.execute_batch("INSERT INTO reports VALUES(1,'old private','Coding',60,NULL,'2026-10-01 12:00:00',0),(2,'shared','Coding',60,NULL,'2026-10-02 12:00:00',0),(3,'other day','Coding',60,NULL,'2026-10-03 12:00:00',0),(4,'other user','Coding',60,NULL,'2026-10-02 12:00:00',0),(5,'old grant','Coding',60,NULL,'2026-10-02 12:00:00',0); INSERT INTO cloud_observation_owner VALUES(2,'owner-a','team-a','grant-a'),(3,'owner-a','team-a','grant-a'),(4,'owner-b','team-a','grant-a'),(5,'owner-a','team-a','grant-old');").unwrap();
        let owner = super::cloud_owner_from_conn(&conn).unwrap();
        let day: String = conn
            .query_row("SELECT date(created_at) FROM reports WHERE id=2", [], |r| {
                r.get(0)
            })
            .unwrap();
        let batch: Vec<i64> = conn
            .prepare(&crate::sync_pure::select_owned_pending_sql(500))
            .unwrap()
            .query_map(
                rusqlite::params![owner.user_id, owner.team_id, owner.sharing_epoch, day],
                |r| r.get(0),
            )
            .unwrap()
            .collect::<Result<_, _>>()
            .unwrap();
        assert_eq!(batch, [2]);
        assert!(super::require_cloud_owner(&conn, &owner, &user).is_ok());
        let mut changed = user.clone();
        changed.team_id = Some("team-b".into());
        assert!(super::require_cloud_owner(&conn, &owner, &changed).is_err());
        conn.execute("UPDATE config SET value=?1 WHERE key='privacy_settings'",[serde_json::json!({"noticeVersion":"2026-08-23","cloudSyncEnabled":true,"cloudSyncEnabledAt":"grant-b"}).to_string()]).unwrap();
        assert!(super::require_cloud_owner(&conn, &owner, &user).is_err());
        super::bind_cloud_observation(&conn, 1, Some(&owner));
        assert_eq!(
            conn.query_row(
                "SELECT count(*) FROM cloud_observation_owner WHERE report_id=1",
                [],
                |r| r.get::<_, i64>(0)
            )
            .unwrap(),
            0
        );
    }

    #[test]
    fn team_categories_are_neutral_and_preserve_recorded_duration() {
        let source = std::collections::HashMap::from([
            ("Coding".into(), 300),
            ("Meetings".into(), 180),
            ("Browsing".into(), 60),
            ("Private window name".into(), 60),
        ]);
        let totals = super::team_category_totals(&source);
        assert_eq!(
            totals,
            std::collections::HashMap::from([
                ("work".into(), 300),
                ("meetings".into(), 180),
                ("other".into(), 120)
            ])
        );
    }

    #[test]
    fn user_session_json_roundtrip() {
        let s = UserSession {
            user_id: "u1".into(),
            team_id: Some("t1".into()),
            access_token: "at".into(),
            refresh_token: Some("rt".into()),
            email: "a@b.c".into(),
        };
        let j = serde_json::to_string(&s).unwrap();
        let back: UserSession = serde_json::from_str(&j).unwrap();
        assert_eq!(back.email, s.email);
        assert_eq!(back.team_id, s.team_id);
    }

    #[cfg(any(windows, target_os = "macos"))]
    #[test]
    fn never_merge_supabase_tokens_from_a_different_user() {
        let conn = rusqlite::Connection::open_in_memory().unwrap();
        conn.execute("CREATE TABLE config (key TEXT PRIMARY KEY, value TEXT)", [])
            .unwrap();
        let user = UserSession {
            user_id: "owner-a".into(),
            team_id: None,
            access_token: "owner-a-access".into(),
            refresh_token: Some("owner-a-refresh".into()),
            email: "a@example.com".into(),
        };
        crate::secure_config::save_secret(
            &conn,
            "user_session",
            &serde_json::to_string(&user).unwrap(),
        )
        .unwrap();
        crate::secure_config::save_secret(
            &conn,
            "auth_session",
            &serde_json::json!({
                "provider": "google",
                "user": { "id": "owner-b", "email": "b@example.com" },
                "access_token": "eyJhbGciOiJub25lIn0.eyJleHAiOjk5OTk5OTk5OTl9.sig",
                "refresh_token": "owner-b-refresh"
            })
            .to_string(),
        )
        .unwrap();

        let resolved = get_user_session_from_conn(&conn).unwrap();
        assert_eq!(resolved.user_id, "owner-a");
        assert_eq!(resolved.access_token, "owner-a-access");
        assert_eq!(resolved.refresh_token.as_deref(), Some("owner-a-refresh"));
    }
}
