use crate::sync_env::{supabase_anon_key, supabase_url};
use crate::vision_model::LLAMA_CHAT_MODEL_ID;
use crate::sync_pure::{clamp_line_for_summary, jwt_exp, select_unsynced_pending_sql, truncate_tasks_for_summary};
use reqwest::blocking::{Client, Response};
use std::thread;
use std::time::Duration;
use rusqlite::Connection;
use serde::{Deserialize, Serialize};

const SYNC_INTERVAL_MINS: u64 = 10;
/// Max rows per cloud upload batch (oldest unsynced first). Override with `FLOWSIGHT_SYNC_BATCH_LIMIT`.
const CLOUDSYNC_BATCH_LIMIT_DEFAULT: u64 = 500;
/// Refresh the access token when it is expired or within this many seconds of expiring.
const JWT_REFRESH_MARGIN_SECS: i64 = 300;
/// Background poll interval for proactive JWT renewal.
const TOKEN_REFRESH_POLL_SECS: u64 = 120;
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
            if let Ok(entitlements) =
                crate::entitlements::refresh_entitlements_from_supabase(&new_session.access_token)
            {
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

// Async so Tauri dispatches it off the main/UI thread: `perform_sync` makes
// several blocking `reqwest` calls, each of which opens a real socket — the
// mechanism Windows uses to inject a broken Winsock LSP into this process
// (see crash_guard.rs module docs).
#[tauri::command]
pub async fn force_sync_now() -> Result<String, String> {
    tauri::async_runtime::spawn_blocking(force_sync_now_blocking)
        .await
        .map_err(|e| format!("Task join error: {}", e))?
}

fn force_sync_now_blocking() -> Result<String, String> {
    let db_path = crate::paths::db_path()?;
    crate::entitlements::require_feature(&db_path, "sync")?;
    match perform_sync(&db_path) {
        Ok(summary) => Ok(format!("Sync Report:\n\n{}", summary)),
        Err(e) => Err(format!("Sync failed: {}", e))
    }
}

// Get user session from local config
pub(crate) fn get_user_session_from_conn(conn: &Connection) -> Option<UserSession> {
    // 1. Try 'user_session' (Internal sync session - has team_id)
    let user_session: Option<UserSession> = conn.query_row(
        "SELECT value FROM config WHERE key = 'user_session'",
        [],
        |row| row.get::<_, String>(0)
    ).ok().and_then(|json_str| serde_json::from_str(&json_str).ok());
    
    // 2. Try 'auth_session' (OAuth session from auth.rs)
    let auth_session: Option<serde_json::Value> = conn.query_row(
        "SELECT value FROM config WHERE key = 'auth_session'",
        [],
        |row| row.get::<_, String>(0)
    ).ok().and_then(|json_str| serde_json::from_str(&json_str).ok());
    
    match (user_session, auth_session) {
        // Both exist: only merge if auth_session is from Supabase (google), NOT jira/linear
        (Some(mut us), Some(auth)) => {
            let provider = auth["provider"].as_str().unwrap_or("");
            if provider == "google" || provider == "manual" {
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
                            let _ = conn.execute(
                                "INSERT OR REPLACE INTO config (key, value) VALUES ('user_session', ?1)",
                                [&json]
                            );
                        }
                        // else: keep user_session JWT (fresher); no log — token refresh polls hit this often.
                    }
                }
            }
            // Non-Supabase auth_session (e.g. Jira): keep user_session only; no merge.
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
                println!("[Sync] auth_session is {} (not Supabase), no user_session available", provider);
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
pub fn save_user_session(user_id: String, team_id: Option<String>, access_token: String, refresh_token: Option<String>, email: String) -> Result<(), String> {
    let db_path = crate::paths::db_path()?;
    let conn = Connection::open(&db_path).map_err(|e| e.to_string())?;
    
    let session = UserSession { user_id, team_id, access_token, refresh_token, email };
    let json = serde_json::to_string(&session).map_err(|e| e.to_string())?;
    
    conn.execute(
        "INSERT OR REPLACE INTO config (key, value) VALUES ('user_session', ?1)",
        [&json]
    ).map_err(|e| e.to_string())?;
    
    println!("[Sync] User session saved for: {}", session.email);
    Ok(())
}

fn refresh_supabase_token(session: &UserSession) -> Result<UserSession, String> {
    let refresh_token = session.refresh_token.as_ref().ok_or("No refresh token available in session")?;
    
    println!("[Sync] Attempting token refresh using token starting with: {}...", &refresh_token[..10]);
    let client = Client::new();
    let url = format!("{}/auth/v1/token?grant_type=refresh_token", supabase_url());
    
    let resp = client.post(&url)
        .header("apikey", supabase_anon_key())
        .json(&serde_json::json!({ "refresh_token": refresh_token }))
        .send()
        .map_err(|e| e.to_string())?;
        
    let status = resp.status();
    if !status.is_success() {
        let err_body = resp.text().unwrap_or_default();
        println!("[Sync] Refresh failed: {}", err_body);
        return Err(format!("Refresh failed (HTTP {}): {}", status, err_body));
    }
    
    let json: serde_json::Value = resp.json().map_err(|e| e.to_string())?;
    let new_access = json["access_token"].as_str().ok_or("Missing access_token in refresh response")?;
    let new_refresh = json["refresh_token"].as_str();
    
    let mut new_session = session.clone();
    new_session.access_token = new_access.to_string();
    if let Some(r) = new_refresh {
        new_session.refresh_token = Some(r.to_string());
    }
    
    // Save updated session
    save_user_session(
        new_session.user_id.clone(),
        new_session.team_id.clone(),
        new_session.access_token.clone(),
        new_session.refresh_token.clone(),
        new_session.email.clone()
    )?;
    
    Ok(new_session)
}

// Clear user session (logout)
#[tauri::command]
pub fn clear_user_session() -> Result<(), String> {
    let db_path = crate::paths::db_path()?;
    let conn = Connection::open(&db_path).map_err(|e| e.to_string())?;
    
    conn.execute("DELETE FROM config WHERE key = 'user_session'", [])
        .map_err(|e| e.to_string())?;
    crate::entitlements::clear_entitlements(&conn)?;
    
    println!("[Sync] User session cleared");
    Ok(())
}

// Check if user is logged in
#[tauri::command]
pub fn get_current_user() -> Result<Option<UserSession>, String> {
    let db_path = crate::paths::db_path()?;
    let conn = Connection::open(&db_path).map_err(|e| e.to_string())?;
    Ok(get_user_session(&conn))
}

fn perform_sync(db_path: &std::path::PathBuf) -> Result<String, String> {
    refresh_session_if_expiring(db_path);

    let conn = Connection::open(db_path).map_err(|e| e.to_string())?;
    
    // Check if user is logged in
    let session = match get_user_session(&conn) {
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

    if let Ok(entitlements) =
        crate::entitlements::refresh_entitlements_from_supabase(&session.access_token)
    {
        let _ = crate::entitlements::save_entitlements(&conn, &entitlements);
        if !entitlements.can_sync {
            println!("[CloudSync] License inactive — sync disabled.");
            return Ok("License inactive — sync disabled".to_string());
        }
    }

    println!("[CloudSync] REST base: {}", supabase_url());
    
    let batch_limit = std::env::var("FLOWSIGHT_SYNC_BATCH_LIMIT")
        .ok()
        .and_then(|s| s.parse::<u64>().ok())
        .filter(|&n| n > 0)
        .unwrap_or(CLOUDSYNC_BATCH_LIMIT_DEFAULT)
        .min(5000) as usize;

    let total_unsynced: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM reports WHERE synced = 0",
            [],
            |r| r.get::<_, i64>(0),
        )
        .unwrap_or(0);
    println!(
        "[CloudSync] Pending unsynced reports: {} (uploading oldest up to {} rows)",
        total_unsynced, batch_limit
    );

    let sql = select_unsynced_pending_sql(batch_limit);
    let mut stmt = conn.prepare(&sql).map_err(|e| e.to_string())?;

    let rows = stmt.query_map([], |row| {
        Ok((
            row.get::<_, i64>(0)?,
            row.get::<_, String>(1)?,
            row.get::<_, String>(2)?,
            row.get::<_, i32>(3)?,
            row.get::<_, Option<String>>(4)?
        ))
    }).map_err(|e| e.to_string())?;
    
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

    for r in rows {
        if let Ok((id, desc, cat, dur, ticket)) = r {
            ids.push(id);
            let desc = clamp_line_for_summary(&desc, line_cap);
            full_text.push_str(&format!("- [{}] {}\n", cat, desc));
            total_duration += dur;
            
            // Stats
            *categories.entry(cat).or_insert(0) += dur;
            if let Some(t) = ticket {
                if !t.is_empty() {
                    *tickets.entry(t).or_insert(0) += dur;
                }
            }
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
    let summary = summarize_with_vision_model(&full_text).unwrap_or_else(|e| {
        println!("[CloudSync] Summary generation failed: {}", e);
        "Summary generation failed".to_string()
    });
    println!("[CloudSync] Summary generated ({} chars): {:.120}", summary.len(), summary);
    
    // 3. Upload to Supabase with user authentication (retry on JWT expired)
    let upload_result = upload_session(&session, total_duration, &summary, &categories, &tickets);
    let upload_result = match &upload_result {
        Err(e) if e.contains("401") || e.contains("PGRST3") => {
            println!("[CloudSync] Auth error detected ({}), attempting JWT refresh...", e);
            let conn_refresh = Connection::open(db_path).map_err(|e| e.to_string())?;
            let session_for_refresh =
                get_user_session(&conn_refresh).unwrap_or_else(|| session.clone());
            match refresh_supabase_token(&session_for_refresh) {
                Ok(refreshed) => upload_session(&refreshed, total_duration, &summary, &categories, &tickets),
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
            println!(
                "[CloudSync] Upload success for {} — in Supabase open public.work_sessions and public.activity_reports (local dev-agent.db table \"reports\" is not uploaded as raw rows).",
                session.email
            );

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
                "captured_at": chrono::Utc::now().to_rfc3339()
            });

            match post_activity_report_with_refresh(db_path, &session, &activity_body) {
                Ok(()) => println!("[CloudSync] activity_reports: AI window summary saved"),
                Err(e) => println!(
                    "[CloudSync] activity_reports insert failed (work_sessions row already saved): {}",
                    e
                ),
            }

            let id_list = ids.iter().map(|id| id.to_string()).collect::<Vec<_>>().join(",");
            let _ = conn.execute(
                &format!("UPDATE reports SET synced = 1 WHERE id IN ({})", id_list),
                []
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
            let host = base
                .trim_end_matches('/')
                .trim_start_matches("https://");
            println!(
                "[CloudSync] (testing) {} UTC | {} local capture(s) → {} | work_sessions + activity_reports",
                chrono::Utc::now().format("%Y-%m-%d %H:%M:%S"),
                ids.len(),
                host
            );
        },
        Err(e) => {
            if e.contains("License expired") || e.contains("403") {
                println!("[CloudSync] LICENSE EXPIRED - Sync blocked");
                return Err("License expired. Contact your PM to renew.".to_string());
            }
            
            println!("[CloudSync] Upload failed: {}", e);
            return Ok(format!("(Cloud Upload Failed: {})\n\nLOCAL SUMMARY:\n{}", e, summary));
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
        "Summarize the developer activity below in ONE short paragraph. Use only facts from the list; do not invent work.\n\nTASKS:\n{}",
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
                "Local AI server offline — cannot summarize (start Local AI monitoring first)".to_string()
            })?,
        )
        .json(&body)
        .send()
        .map_err(|e| e.to_string())?;
    if !resp.status().is_success() {
        let status = resp.status();
        let err_body = resp.text().unwrap_or_default();
        return Err(format!("Summary request failed ({}): {}", status, err_body));
    }

    let json: serde_json::Value = resp.json().map_err(|e| e.to_string())?;
    let content = json["choices"][0]["message"]["content"].as_str().unwrap_or("");
    if content.is_empty() {
        return Err("Model returned empty summary.".to_string());
    }

    Ok(content.to_string())
}

fn upload_session(
    session: &UserSession,
    duration: i32, 
    summary: &str, 
    categories: &std::collections::HashMap<String, i32>,
    tickets: &std::collections::HashMap<String, i32>
) -> Result<(), String> {
    let client = Client::new();
    let url = format!("{}/rest/v1/work_sessions", supabase_url()); 
    
    let body = serde_json::json!({
        "user_id": session.user_id,
        "team_id": session.team_id,
        "duration_seconds": duration,
        "summary": summary,
        "category_breakdown": categories,
        "jira_breakdown": tickets,
        "session_date": chrono::Local::now().format("%Y-%m-%d").to_string(),
        "created_at": chrono::Utc::now().to_rfc3339()
    });

    let resp = client.post(&url)
        .header("apikey", supabase_anon_key())
        .header("Authorization", format!("Bearer {}", &session.access_token))
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
        let body_text = resp.text().unwrap_or_default();
        return Err(format!("HTTP {}: {}", status, body_text));
    }
        
    Ok(())
}

fn post_activity_report_row(session: &UserSession, body: &serde_json::Value) -> Result<Response, String> {
    let client = Client::new();
    let url = format!("{}/rest/v1/activity_reports", supabase_url());
    client
        .post(&url)
        .header("apikey", supabase_anon_key())
        .header("Authorization", format!("Bearer {}", &session.access_token))
        .header("Content-Type", "application/json")
        .header("Prefer", "return=minimal")
        .json(body)
        .send()
        .map_err(|e| e.to_string())
}

/// Retries once with a refreshed JWT if the first POST returns 401/403.
fn post_activity_report_with_refresh(
    db_path: &std::path::PathBuf,
    session: &UserSession,
    body: &serde_json::Value,
) -> Result<(), String> {
    let mut resp = post_activity_report_row(session, body)?;
    if resp.status().as_u16() == 401 || resp.status().as_u16() == 403 {
        let conn = Connection::open(db_path).map_err(|e| e.to_string())?;
        if let Some(s) = get_user_session(&conn) {
            if let Ok(new_s) = refresh_supabase_token(&s) {
                resp = post_activity_report_row(&new_s, body)?;
            }
        }
    }
    let status = resp.status();
    if status.as_u16() == 403 {
        return Err("License expired or invalid".to_string());
    }
    if !status.is_success() {
        let t = resp.text().unwrap_or_default();
        return Err(format!("HTTP {}: {}", status, t));
    }
    Ok(())
}

// Upload individual activity report (for granular tracking).
// Async so Tauri dispatches it off the main/UI thread (see crash_guard.rs module docs).
#[tauri::command]
pub async fn upload_activity_report(
    description: String,
    category: String,
    jira_ticket_id: Option<String>,
    duration_seconds: i32
) -> Result<(), String> {
    tauri::async_runtime::spawn_blocking(move || {
        upload_activity_report_blocking(description, category, jira_ticket_id, duration_seconds)
    })
    .await
    .map_err(|e| format!("Task join error: {}", e))?
}

fn upload_activity_report_blocking(
    description: String,
    category: String,
    jira_ticket_id: Option<String>,
    duration_seconds: i32
) -> Result<(), String> {
    let db_path = crate::paths::db_path()?;
    crate::entitlements::require_feature(&db_path, "sync")?;
    let conn = Connection::open(&db_path).map_err(|e| e.to_string())?;
    
    let session = get_user_session(&conn)
        .ok_or("Not logged in")?;
    
    let body = serde_json::json!({
        "user_id": session.user_id,
        "team_id": session.team_id,
        "description": description,
        "category": category,
        "jira_ticket_id": jira_ticket_id,
        "duration_seconds": duration_seconds,
        "captured_at": chrono::Utc::now().to_rfc3339()
    });
    
    let resp = post_activity_report_row(&session, &body)?;
    
    if resp.status().as_u16() == 403 {
        return Err("License expired or invalid".to_string());
    }
    
    resp.error_for_status()
        .map_err(|e| e.to_string())?;
    
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
    
    let session = get_user_session(&conn)
        .ok_or("Not logged in")?;
    
    let client = Client::new();
    let mut current_token = session.access_token.clone();
    
    // Fetch team memberships from Supabase
    let url = format!(
        "{}/rest/v1/team_members?user_id=eq.{}&select=team_id,role,joined_at",
        supabase_url(), session.user_id
    );
    
    let mut resp = client.get(&url)
        .header("apikey", supabase_anon_key())
        .header("Authorization", format!("Bearer {}", current_token))
        .send()
        .map_err(|e| e.to_string())?;
    
    // Retry on 401/403
    if resp.status().as_u16() == 401 || resp.status().as_u16() == 403 {
        if let Ok(new_s) = refresh_supabase_token(&session) {
            current_token = new_s.access_token.clone();
            resp = client.get(&url)
                .header("apikey", supabase_anon_key())
                .header("Authorization", format!("Bearer {}", current_token))
                .send()
                .map_err(|e| e.to_string())?;
        }
    }
    
    let status = resp.status();
    if !status.is_success() {
        let body = resp.text().unwrap_or_default();
        return Err(format!("Failed to fetch teams (HTTP {}): {}", status, body));
    }
    
    let teams: Vec<serde_json::Value> = resp.json().map_err(|e| e.to_string())?;

    // Auto-elegir primer team si no hay uno activo persistido todavía.
    // Evita que activity_reports/work_sessions suban con team_id=NULL cuando el
    // usuario ya tiene membresía(s) pero nunca tocó el dropdown (el navegador
    // muestra la primera opción sin disparar `change`, por eso `set_active_team`
    // no se invocaba).
    let active_team_id = match session.team_id.clone() {
        Some(id) => Some(id),
        None => {
            let first = teams
                .first()
                .and_then(|t| t["team_id"].as_str().map(|s| s.to_string()));
            if let Some(id) = first.clone() {
                println!("[Team] No active team in session; auto-selecting first membership: {}", id);
                save_user_session(
                    session.user_id.clone(),
                    Some(id.clone()),
                    current_token.clone(),
                    session.refresh_token.clone(),
                    session.email.clone(),
                )?;
            }
            first
        }
    };

    println!("[Team] Found {} team memberships, active: {:?}", teams.len(), active_team_id);

    Ok(serde_json::json!({
        "teams": teams,
        "active_team_id": active_team_id
    }))
}

// Set the active team for the current user (persists to SQLite)
#[tauri::command]
pub fn set_active_team(team_id: String) -> Result<(), String> {
    let db_path = crate::paths::db_path()?;
    let conn = Connection::open(&db_path).map_err(|e| e.to_string())?;
    
    let session = get_user_session(&conn)
        .ok_or("Not logged in")?;
    
    println!("[Team] Setting active team to: {}", team_id);
    
    save_user_session(
        session.user_id,
        Some(team_id),
        session.access_token,
        session.refresh_token,
        session.email
    )
}

// Join a team using an invitation token.
// Async so Tauri dispatches it off the main/UI thread (see crash_guard.rs module docs).
#[tauri::command]
pub async fn join_team(token: String) -> Result<serde_json::Value, String> {
    tauri::async_runtime::spawn_blocking(move || join_team_blocking(token))
        .await
        .map_err(|e| format!("Task join error: {}", e))?
}

fn join_team_blocking(token: String) -> Result<serde_json::Value, String> {
    let db_path = crate::paths::db_path()?;
    crate::entitlements::require_feature(&db_path, "sync")?;
    refresh_session_if_expiring(&db_path);

    let conn = Connection::open(&db_path).map_err(|e| e.to_string())?;
    
    let mut session = get_user_session(&conn)
        .ok_or("Not logged in. Please sign in first.")?;
    
    let client = Client::new();
    let mut current_token = session.access_token.clone();
    
    // 1. Fetch current user info (Retry on 401)
    println!("[Team] Fetching user info for profile sync...");
    let mut user_resp = client.get(format!("{}/auth/v1/user", supabase_url()))
        .header("apikey", supabase_anon_key())
        .header("Authorization", format!("Bearer {}", current_token))
        .send()
        .map_err(|e| e.to_string())?;
        
    if user_resp.status().as_u16() == 401 || user_resp.status().as_u16() == 403 {
        println!("[Team] JWT might be expired (HTTP {}), attempting refresh...", user_resp.status());
        if let Ok(new_s) = refresh_supabase_token(&session) {
            session = new_s;
            current_token = session.access_token.clone();
            user_resp = client.get(format!("{}/auth/v1/user", supabase_url()))
                .header("apikey", supabase_anon_key())
                .header("Authorization", format!("Bearer {}", current_token))
                .send()
                .map_err(|e| e.to_string())?;
        }
    }
    
    if !user_resp.status().is_success() {
        let err_body = user_resp.text().unwrap_or_else(|_| "Empty body".to_string());
        return Err(format!(
            "Your session has expired. Please sign out and sign in again. ({})",
            err_body
        ));
    }
    
    let user_json: serde_json::Value = user_resp.json().map_err(|e| e.to_string())?;
    let meta = &user_json["user_metadata"];
    let display_name = meta["full_name"].as_str().or(meta["name"].as_str()).unwrap_or("User");
    let avatar_url = meta["avatar_url"].as_str();
    
    let user_id_from_jwt_owned = user_json["id"].as_str().unwrap_or(&session.user_id).to_string();
    let user_id_from_jwt = &user_id_from_jwt_owned;
    println!("[Team] Syncing profile for user {} (JWT id: {})", session.user_id, user_id_from_jwt);
    
    // 2. Ensure profile exists (Upsert)
    let profile_url = format!("{}/rest/v1/profiles", supabase_url());
    let prof_resp = client.post(&profile_url)
        .header("apikey", supabase_anon_key())
        .header("Authorization", format!("Bearer {}", current_token))
        .header("Content-Type", "application/json")
        .header("Prefer", "resolution=merge-duplicates,return=minimal")
        .json(&serde_json::json!({
            "id": user_id_from_jwt,
            "display_name": display_name,
            "avatar_url": avatar_url,
            "role": "worker"
        }))
        .send();
        
    match prof_resp {
        Ok(r) if !r.status().is_success() => {
            println!("[Team] Profile upsert failed (HTTP {}): {}", r.status(), r.text().unwrap_or_default());
        }
        Err(e) => println!("[Team] Profile upsert request error: {}", e),
        _ => println!("[Team] Profile synced successfully"),
    }

    // 3. Verify invitation (Retry on 401)
    println!("[Team] Validating invitation token: {}", token);
    let inv_url = format!("{}/rest/v1/invitations?token=eq.{}&select=team_id,expires_at,used_at,created_by,email", supabase_url(), token);
    
    let mut inv_resp = client.get(&inv_url)
        .header("apikey", supabase_anon_key())
        .header("Authorization", format!("Bearer {}", current_token))
        .send()
        .map_err(|e| e.to_string())?;
        
    if inv_resp.status().as_u16() == 401 || inv_resp.status().as_u16() == 403 {
        println!("[Team] Invitation request unauthorized, attempting refresh with latest session...");
        if let Ok(new_s) = refresh_supabase_token(&session) {
            session = new_s;
            current_token = session.access_token.clone();
            inv_resp = client.get(&inv_url)
                .header("apikey", supabase_anon_key())
                .header("Authorization", format!("Bearer {}", current_token))
                .send()
                .map_err(|e| e.to_string())?;
        }
    }
    
    let inv_status = inv_resp.status();
    if !inv_status.is_success() {
        let err_body = inv_resp.text().unwrap_or_else(|_| "Empty body".to_string());
        if err_body.contains("JWT expired") || err_body.contains("PGRST303") {
            return Err(
                "Your session has expired. Please sign out, sign in again, then join the team."
                    .to_string(),
            );
        }
        return Err(format!("Error validating invitation (HTTP {}): {}", inv_status, err_body));
    }
    
    let invitations: Vec<serde_json::Value> = inv_resp.json().map_err(|e| e.to_string())?;
    let invitation = invitations.get(0).ok_or("Invalid invitation token")?;
    
    println!("[Team] Invitation details: {:?}", invitation);
    let inv_email = invitation["email"].as_str();
    println!("[Team] Analyzing match: Session Email '{}' vs Invitation Email '{:?}'", session.email, inv_email);
    
    if !invitation["used_at"].is_null() {
        return Err("This invitation has already been used".to_string());
    }
    
    let team_id = invitation["team_id"].as_str().ok_or("Malformed invitation (missing team_id)")?;
    let _inviter_id = invitation["created_by"].as_str();
    
    // 4. Add to team_members (Retry on 401)
    println!("[Team] Adding user {} to team {} (role: member, omitting invited_by)", user_id_from_jwt, team_id);
    let member_url = format!("{}/rest/v1/team_members", supabase_url());
    let member_body = serde_json::json!({
        "team_id": team_id,
        "user_id": user_id_from_jwt,
        "role": "member",
        "joined_at": chrono::Utc::now().to_rfc3339()
    });
    
    let mut member_resp = client.post(&member_url)
        .header("apikey", supabase_anon_key())
        .header("Authorization", format!("Bearer {}", current_token))
        .header("Content-Type", "application/json")
        .header("Prefer", "return=minimal")
        .json(&member_body)
        .send()
        .map_err(|e| e.to_string())?;
    
    if member_resp.status().as_u16() == 401 || member_resp.status().as_u16() == 403 {
        if let Ok(new_s) = refresh_supabase_token(&session) {
            session = new_s;
            current_token = session.access_token.clone();
            member_resp = client.post(&member_url)
                .header("apikey", supabase_anon_key())
                .header("Authorization", format!("Bearer {}", current_token))
                .header("Content-Type", "application/json")
                .header("Prefer", "return=minimal")
                .json(&member_body)
                .send()
                .map_err(|e| e.to_string())?;
        }
    }
        
    let member_status = member_resp.status();
    if !member_status.is_success() {
        let err_text = member_resp.text().unwrap_or_else(|_| "Unknown RLS/DB error".to_string());
        if err_text.contains("unique_team_user") || err_text.contains("duplicate") {
            // Already a member
        } else {
            return Err(format!("Failed to join team: {}", err_text));
        }
    }
    
    // 5. Mark invitation as used
    let mark_url = format!("{}/rest/v1/invitations?token=eq.{}", supabase_url(), token);
    let _ = client.patch(&mark_url)
        .header("apikey", supabase_anon_key())
        .header("Authorization", format!("Bearer {}", current_token))
        .header("Content-Type", "application/json")
        .json(&serde_json::json!({ "used_at": chrono::Utc::now().to_rfc3339() }))
        .send();
    
    // 6. Get final state for local session
    let updated_session = get_user_session(&conn).unwrap_or(session);
    save_user_session(
        updated_session.user_id.clone(), 
        Some(team_id.to_string()), 
        updated_session.access_token.clone(), 
        updated_session.refresh_token.clone(), 
        updated_session.email.clone()
    )?;
    
    Ok(serde_json::json!({ "success": true, "team_id": team_id }))
}

#[cfg(test)]
mod user_session_tests {
    use super::UserSession;

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
}
