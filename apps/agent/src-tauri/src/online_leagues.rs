//! Opt-in social aggregates. This service never uploads raw observations or vault keys.
use chrono::{DateTime, Duration as ChronoDuration, NaiveDate, NaiveDateTime, TimeZone, Utc};
use reqwest::blocking::Client;
use rusqlite::{params, Connection, OptionalExtension};
use serde_json::{json, Value};
use std::{
    path::Path,
    sync::{
        mpsc::{self, Receiver, RecvTimeoutError, SyncSender},
        Mutex, OnceLock,
    },
    time::Duration,
};
use tauri::Emitter;

use crate::{
    focus_semantics::{self, ActivitySample},
    sync::{get_user_session_from_conn, UserSession},
    sync_env::{supabase_anon_key, supabase_url},
};

const NOTICE: &str = "leagues-2026-10-05";
static REQUEST_LOCK: Mutex<()> = Mutex::new(());
static SYNC_WAKE: OnceLock<SyncSender<()>> = OnceLock::new();
static LAST_VIEW: Mutex<Option<(String, Value)>> = Mutex::new(None);

/// Coalesce observations/consent changes without blocking the capture pipeline.
pub fn request_sync() {
    if let Some(sender) = SYNC_WAKE.get() {
        let _ = sender.try_send(());
    }
}

pub fn ensure_schema(conn: &Connection) -> Result<(), String> {
    conn.execute_batch("CREATE TABLE IF NOT EXISTS online_local_consent (
        user_id TEXT PRIMARY KEY, enabled INTEGER NOT NULL DEFAULT 0,
        alias TEXT NOT NULL, accepted_at TEXT NOT NULL, device_id TEXT NOT NULL,
        notice_version TEXT NOT NULL, withdrawal_pending INTEGER NOT NULL DEFAULT 0);
      CREATE TABLE IF NOT EXISTS online_observation_owner (
        report_id INTEGER PRIMARY KEY, user_id TEXT NOT NULL,
        FOREIGN KEY(report_id) REFERENCES reports(id) ON DELETE CASCADE);
      CREATE INDEX IF NOT EXISTS online_observation_account ON online_observation_owner(user_id,report_id);")
      .map_err(|_| "Could not prepare local league settings".to_string())?;
    // Additive migration for an earlier local preview.
    let _ = conn.execute(
        "ALTER TABLE online_local_consent ADD COLUMN withdrawal_pending INTEGER NOT NULL DEFAULT 0",
        [],
    );
    Ok(())
}

fn enabled(conn: &Connection, uid: &str) -> bool {
    conn.query_row(
        "SELECT enabled FROM online_local_consent WHERE user_id=?1 AND notice_version=?2",
        params![uid, NOTICE],
        |r| r.get::<_, bool>(0),
    )
    .optional()
    .ok()
    .flatten()
    .unwrap_or(false)
}

/// Bind only newly captured reports to the account that opted in. No history import.
pub fn capture_owner(path: &Path) -> Option<String> {
    if !crate::entitlements::personal_features_allowed(path) {
        return None;
    }
    let conn = Connection::open(path).ok()?;
    let session = get_user_session_from_conn(&conn)?;
    enabled(&conn, &session.user_id).then_some(session.user_id)
}

pub fn bind_observation(conn: &Connection, report_id: i64, captured_owner: Option<&str>) {
    if ensure_schema(conn).is_err() {
        return;
    }
    if let Some(session) = get_user_session_from_conn(conn) {
        if crate::entitlements::profile_id(conn) == "individual"
            && captured_owner == Some(session.user_id.as_str())
            && enabled(conn, &session.user_id)
        {
            let _ = conn.execute(
                "INSERT OR IGNORE INTO online_observation_owner(report_id,user_id) VALUES(?1,?2)",
                params![report_id, session.user_id],
            );
        }
    }
}

fn account(path: &Path) -> Result<(Connection, UserSession), String> {
    let conn = Connection::open(path).map_err(|_| "Could not open FlowSight settings")?;
    ensure_schema(&conn)?;
    let session =
        get_user_session_from_conn(&conn).ok_or("Sign in to FlowSight to use online leagues")?;
    Ok((conn, session))
}

fn rpc(session: &UserSession, body: Value) -> Result<Value, String> {
    retry_authenticated_request(
        session,
        |active| rpc_once(active, &body),
        crate::sync::refresh_supabase_token,
    )
}

fn retry_authenticated_request(
    session: &UserSession,
    mut request: impl FnMut(&UserSession) -> Result<Value, String>,
    renew: impl FnOnce(&UserSession) -> Result<UserSession, String>,
) -> Result<Value, String> {
    match request(session) {
        Err(error) if error == "Sign in again to use online leagues" => {
            let renewed = renew(session)?;
            request(&renewed)
        }
        result => result,
    }
}

fn rpc_once(session: &UserSession, body: &Value) -> Result<Value, String> {
    let response = Client::builder()
        .timeout(Duration::from_secs(20))
        .build()
        .map_err(|_| "Could not connect to leagues")?
        .post(format!(
            "{}/rest/v1/rpc/online_leagues_service",
            supabase_url()
        ))
        .header("apikey", supabase_anon_key())
        .bearer_auth(&session.access_token)
        .json(&json!({"p_request":body}))
        .send()
        .map_err(|_| "Online leagues could not connect. Check your connection and retry")?;
    let status = response.status();
    let value: Value = response.json().unwrap_or(Value::Null);
    if !status.is_success() {
        if status.as_u16() == 401 {
            return Err("Sign in again to use online leagues".into());
        }
        if value["code"] == "PGRST202" {
            return Err("The online league service is not deployed yet".into());
        }
        let message = value["message"].as_str().unwrap_or("");
        for expected in [
            "Invitation expired or unavailable",
            "Another device is scoring. Disable leagues on that device first",
            "This group already has 8 friends",
            "Only the group owner can invite",
            "Group limit reached",
            "Membership limit reached",
            "Choose a display name of 2 to 32 characters",
            "Invalid invitation code",
            "Scoring device does not match",
            "Accept the online league service first",
            "Username already taken. Choose another",
            "Use 3 to 20 letters, numbers, dots or underscores for your username",
        ] {
            if message.contains(expected) {
                return Err(expected.into());
            }
        }
        return Err("The league request could not be completed. Retry or sign in again".into());
    }
    Ok(value)
}

pub fn points(minutes: i64) -> i64 {
    if minutes < 25 {
        0
    } else {
        let m = minutes.min(75);
        m + 5 * (m / 25) + 10
    }
}

fn normalize_username(value: &str) -> Result<String, String> {
    let value = value.trim().to_ascii_lowercase();
    if !(3..=20).contains(&value.len())
        || !value
            .bytes()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == b'_' || c == b'.')
        || !value
            .as_bytes()
            .first()
            .is_some_and(|c| c.is_ascii_alphanumeric())
    {
        return Err("Use 3 to 20 letters, numbers, dots or underscores for your username".into());
    }
    Ok(value)
}

fn eligible_minutes(conn: &Connection, uid: &str, day: &str) -> Result<i64, String> {
    let date = NaiveDate::parse_from_str(day, "%Y-%m-%d").map_err(|_| "Invalid league date")?;
    let zone = chrono_tz::Europe::Madrid;
    let start = zone
        .from_local_datetime(&date.and_hms_opt(0, 0, 0).unwrap())
        .single()
        .ok_or("Invalid league date")?
        .with_timezone(&Utc);
    let day_end = zone
        .from_local_datetime(
            &(date + ChronoDuration::days(1))
                .and_hms_opt(0, 0, 0)
                .unwrap(),
        )
        .single()
        .ok_or("Invalid league date")?
        .with_timezone(&Utc);
    let mut stmt=conn.prepare("SELECT r.created_at,r.duration_seconds,r.activity_type,r.capture_source,r.theme_hint
      FROM reports r JOIN online_observation_owner o ON o.report_id=r.id
      JOIN online_local_consent c ON c.user_id=o.user_id
      WHERE o.user_id=?1 AND c.enabled=1 AND datetime(r.created_at)>=datetime(c.accepted_at)
        AND datetime(r.created_at)>=datetime(?2) AND datetime(r.created_at)<=datetime(?3) ORDER BY datetime(r.created_at),r.id")
      .map_err(|_|"Could not read eligible focus")?;
    let accepted: String = conn
        .query_row(
            "SELECT accepted_at FROM online_local_consent WHERE user_id=?1",
            [uid],
            |r| r.get(0),
        )
        .map_err(|_| "League acceptance unavailable")?;
    let parse = |s: &str| {
        DateTime::parse_from_rfc3339(s)
            .map(|v| v.with_timezone(&Utc))
            .ok()
            .or_else(|| {
                NaiveDateTime::parse_from_str(s, "%Y-%m-%d %H:%M:%S")
                    .ok()
                    .map(|v| Utc.from_utc_datetime(&v))
            })
    };
    let accepted = parse(&accepted).ok_or("League acceptance unavailable")?;
    let rows = stmt
        .query_map(
            params![uid, start.to_rfc3339(), day_end.to_rfc3339()],
            |r| {
                Ok((
                    r.get::<_, String>(0)?,
                    r.get::<_, i64>(1).unwrap_or(0),
                    r.get::<_, String>(2).unwrap_or_default(),
                    r.get::<_, String>(3).unwrap_or_default(),
                    r.get::<_, Option<String>>(4).unwrap_or(None),
                ))
            },
        )
        .map_err(|_| "Could not read eligible focus")?;
    let mut samples = Vec::new();
    let mut focus_seconds = 0;
    let summarize_block = |block: &mut Vec<ActivitySample>| {
        focus_semantics::summarize(std::mem::take(block))
            .sessions
            .iter()
            .filter(|s| s.focus_seconds >= 1500)
            .map(|s| s.focus_seconds)
            .sum::<i64>()
    };
    for row in rows {
        let (end, seconds, category, source, theme) =
            row.map_err(|_| "Could not read eligible focus")?;
        if seconds <= 0 {
            continue;
        }
        // Only periodic vision owns elapsed time in the desktop capture pipeline.
        // Manual/imported/unknown observations are hard boundaries.
        if seconds > 600 || source != "periodic_vision" {
            focus_seconds += summarize_block(&mut samples);
            continue;
        }
        let Some(report_end) = parse(&end) else {
            focus_seconds += summarize_block(&mut samples);
            continue;
        };
        if report_end > Utc::now() {
            continue;
        }
        let report_start = (report_end - ChronoDuration::seconds(seconds))
            .max(start)
            .max(accepted);
        let report_end = report_end.min(day_end);
        let seconds = (report_end - report_start).num_seconds();
        if seconds <= 0 {
            continue;
        }
        samples.push(ActivitySample {
            start: report_start.naive_utc(),
            duration_seconds: seconds,
            category,
            description: String::new(),
            ticket: None,
            theme_hint: theme,
            app_name: None,
            window_title: None,
        });
    }
    focus_seconds += summarize_block(&mut samples);
    Ok((focus_seconds / 60).min(75))
}

fn local_view(path: &Path) -> Result<Value, String> {
    let conn = Connection::open(path).map_err(|_| "Could not open league settings")?;
    ensure_schema(&conn)?;
    let Some(session) = get_user_session_from_conn(&conn) else {
        return Ok(json!({"signed_in":false,"enabled":false,"groups":[]}));
    };
    let day = Utc::now()
        .with_timezone(&chrono_tz::Europe::Madrid)
        .date_naive()
        .to_string();
    let active = enabled(&conn, &session.user_id);
    let minutes = if active {
        eligible_minutes(&conn, &session.user_id, &day)?
    } else {
        0
    };
    let alias: Option<String> = conn
        .query_row(
            "SELECT alias FROM online_local_consent WHERE user_id=?1",
            [&session.user_id],
            |r| r.get(0),
        )
        .optional()
        .unwrap_or(None);
    let pending = conn
        .query_row(
            "SELECT withdrawal_pending FROM online_local_consent WHERE user_id=?1",
            [&session.user_id],
            |r| r.get::<_, bool>(0),
        )
        .optional()
        .unwrap_or(None)
        .unwrap_or(false);
    Ok(
        json!({"signed_in":true,"enabled":active,"alias":alias,"account_id":session.user_id,"day":day,"today_points":0,"synced_at":null,"eligible_minutes":minutes,"local_points":points(minutes),"groups":[],"notice_version":NOTICE,"withdrawal_pending":pending}),
    )
}

fn sync_inner(path: &Path) -> Result<Value, String> {
    if !crate::entitlements::personal_features_allowed(path) {
        let (conn, session) = account(path)?;
        conn.execute("UPDATE online_local_consent SET enabled=0,withdrawal_pending=1 WHERE user_id=?1 AND enabled=1",[&session.user_id]).map_err(|_| "Could not stop league sharing")?;
        conn.execute(
            "DELETE FROM online_observation_owner WHERE user_id=?1",
            [&session.user_id],
        )
        .map_err(|_| "Could not clear league ownership")?;
    }
    let mut view = local_view(path)?;
    if view["withdrawal_pending"] == true {
        crate::sync::refresh_session_if_expiring(&path.to_path_buf());
        let (conn, session) = account(path)?;
        rpc(&session, json!({"action":"withdraw"}))?;
        conn.execute(
            "UPDATE online_local_consent SET withdrawal_pending=0 WHERE user_id=?1",
            [&session.user_id],
        )
        .map_err(|_| "Could not save league withdrawal")?;
        view["withdrawal_pending"] = json!(false);
    }
    if view["enabled"] != true {
        return Ok(view);
    }
    crate::sync::refresh_session_if_expiring(&path.to_path_buf());
    let (conn, session) = account(path)?;
    let device: String = conn
        .query_row(
            "SELECT device_id FROM online_local_consent WHERE user_id=?1",
            [&session.user_id],
            |r| r.get(0),
        )
        .map_err(|_| "Scoring device unavailable")?;
    let mut remote = rpc(&session, json!({"action":"status"}))?;
    if remote["enabled"] != true || remote["device_id"].as_str() != Some(&device) {
        conn.execute(
            "UPDATE online_local_consent SET enabled=0 WHERE user_id=?1",
            [&session.user_id],
        )
        .map_err(|_| "Could not update league choice")?;
        view["enabled"] = json!(false);
        view["cloud_error"] = json!("Leagues were disabled or another device is scoring");
        return Ok(view);
    }
    let minutes = eligible_minutes(
        &conn,
        &session.user_id,
        remote["day"].as_str().ok_or("League date unavailable")?,
    )?;
    if points(minutes) > remote["today_points"].as_i64().unwrap_or(0) {
        remote = rpc(
            &session,
            json!({"action":"submit","device_id":device,"day":remote["day"],"eligible_minutes":minutes,
          "scoring_version":"friends-v1","evidence_profile":"windows-observed-v1","receipt_id":uuid::Uuid::new_v4().to_string()}),
        )?;
    }
    remote["signed_in"] = json!(true);
    remote["account_id"] = json!(session.user_id);
    remote["eligible_minutes"] = json!(minutes);
    remote["local_points"] = json!(points(minutes));
    remote["synced_at"] = json!(Utc::now().to_rfc3339());
    Ok(remote)
}

fn remember_view(view: &Value) {
    if let Some(uid) = view["account_id"].as_str() {
        if let Ok(mut cached) = LAST_VIEW.lock() {
            *cached = Some((uid.to_string(), view.clone()));
        }
    }
}

/// Read the latest background result. Opening Online never controls uploading.
fn current_view(path: &Path) -> Result<Value, String> {
    let local = local_view(path)?;
    let cached = LAST_VIEW.lock().map_err(|_| "League status unavailable")?;
    Ok(merge_current_view(local, cached.as_ref()))
}

fn merge_current_view(local: Value, cached: Option<&(String, Value)>) -> Value {
    if local["enabled"] != true && local["withdrawal_pending"] != true {
        return local;
    }
    if let Some((_, remote)) =
        cached.filter(|(uid, _)| Some(uid.as_str()) == local["account_id"].as_str())
    {
        let mut view = remote.clone();
        if view["day"] != local["day"] {
            view["today_points"] = json!(0);
            view["synced_at"] = Value::Null;
        }
        if let Some(object) = local.as_object() {
            for (key, value) in object {
                if !matches!(key.as_str(), "groups" | "today_points" | "synced_at") {
                    view[key] = value.clone();
                }
            }
        }
        view
    } else {
        local
    }
}

fn perform_sync(path: &Path) -> Result<Value, String> {
    let view = match sync_inner(path) {
        Ok(view) => view,
        Err(error) => {
            let mut view = current_view(path)?;
            view["cloud_error"] = json!(error);
            view
        }
    };
    remember_view(&view);
    Ok(view)
}

#[tauri::command]
pub async fn get_online_leagues() -> Result<Value, String> {
    tauri::async_runtime::spawn_blocking(|| {
        let path = crate::paths::db_path()?;
        current_view(&path)
    })
    .await
    .map_err(|_| "League request interrupted")?
}

#[tauri::command]
pub async fn set_online_league_consent(accept: bool, alias: String) -> Result<Value, String> {
    let result = tauri::async_runtime::spawn_blocking(move|| -> Result<Value, String> {
        let _guard=REQUEST_LOCK.lock().map_err(|_|"League request unavailable")?;
        let path=crate::paths::db_path()?;
        let (conn,session)=account(&path)?;
        if !accept {
            // Stop local sharing immediately even when the cloud is unreachable.
            conn.execute("UPDATE online_local_consent SET enabled=0,withdrawal_pending=1 WHERE user_id=?1",[&session.user_id]).map_err(|_|"Could not disable leagues")?;
            crate::sync::refresh_session_if_expiring(&path);
            let (_,session)=account(&path)?;
            let remote=rpc(&session,json!({"action":"withdraw"}));
            conn.execute("DELETE FROM online_observation_owner WHERE user_id=?1",[&session.user_id]).map_err(|_|"Could not clear local league data")?;
            if remote.is_ok(){conn.execute("UPDATE online_local_consent SET withdrawal_pending=0 WHERE user_id=?1",[&session.user_id]).map_err(|_|"Could not save league withdrawal")?;}
            let mut view=local_view(&path)?;
            if let Err(error)=remote{view["cloud_error"]=json!(error);}
            return Ok(view);
        }
        if !crate::entitlements::personal_features_allowed(&path) { return Err("Online leagues are available in the Individual profile".into()); }
        let alias=normalize_username(&alias)?;
        crate::sync::refresh_session_if_expiring(&path);
        let (_,session)=account(&path)?;
        // Complete an earlier offline withdrawal before starting a new participation.
        if conn.query_row("SELECT withdrawal_pending FROM online_local_consent WHERE user_id=?1",[&session.user_id],|r|r.get::<_,bool>(0)).optional().unwrap_or(None).unwrap_or(false){rpc(&session,json!({"action":"withdraw"}))?;}
        let saved:Option<String>=conn.query_row("SELECT device_id FROM online_local_consent WHERE user_id=?1",[&session.user_id],|r|r.get(0)).optional().unwrap_or(None);
        let device=saved.unwrap_or_else(||uuid::Uuid::new_v4().to_string());
        let mut remote=rpc(&session,json!({"action":"accept","device_id":device,"alias":alias,"notice_version":NOTICE}))?;
        if !enabled(&conn,&session.user_id){
            conn.execute("DELETE FROM online_observation_owner WHERE user_id=?1",[&session.user_id]).map_err(|_|"Could not prepare new participation")?;
        }
        conn.execute("INSERT INTO online_local_consent(user_id,enabled,alias,accepted_at,device_id,notice_version) VALUES(?1,1,?2,?3,?4,?5)
          ON CONFLICT(user_id) DO UPDATE SET enabled=1,withdrawal_pending=0,alias=excluded.alias,accepted_at=excluded.accepted_at,device_id=excluded.device_id,notice_version=excluded.notice_version",
          params![session.user_id,alias,remote["accepted_at"].as_str().ok_or("League acceptance not confirmed")?,device,NOTICE]).map_err(|_|"Could not save league acceptance")?;
        remote["signed_in"]=json!(true);remote["account_id"]=json!(session.user_id);remote["local_points"]=json!(0);remote["eligible_minutes"]=json!(0);Ok(remote)
    }).await.map_err(|_|"League request interrupted")??;
    remember_view(&result);
    request_sync();
    Ok(result)
}

#[tauri::command]
pub async fn online_league_action(request: Value) -> Result<Value, String> {
    let result = tauri::async_runtime::spawn_blocking(move || -> Result<Value, String> {
        let _guard = REQUEST_LOCK
            .lock()
            .map_err(|_| "League request unavailable")?;
        if !matches!(
            request["action"].as_str(),
            Some("create" | "join" | "invite" | "leave" | "export")
        ) {
            return Err("Unsupported league action".into());
        }
        let path = crate::paths::db_path()?;
        if request["action"] != "export" && !crate::entitlements::personal_features_allowed(&path) {
            return Err("Online leagues are available in the Individual profile".into());
        }
        crate::sync::refresh_session_if_expiring(&path);
        let (conn, session) = account(&path)?;
        if !enabled(&conn, &session.user_id) {
            return Err("Accept the online league service first".into());
        }
        let mut result = rpc(&session, request)?;
        result["signed_in"] = json!(true);
        result["account_id"] = json!(session.user_id);
        Ok(result)
    })
    .await
    .map_err(|_| "League request interrupted")??;
    remember_view(&result);
    request_sync();
    Ok(result)
}

fn run_sync_worker(receiver: Receiver<()>, interval: Duration, mut sync: impl FnMut()) {
    sync(); // Restore accepted participation immediately after app startup.
    loop {
        match receiver.recv_timeout(interval) {
            Ok(()) | Err(RecvTimeoutError::Timeout) => {
                while receiver.try_recv().is_ok() {}
                sync();
            }
            Err(RecvTimeoutError::Disconnected) => return,
        }
    }
}

pub fn start_sync_thread(path: std::path::PathBuf, app: tauri::AppHandle) {
    let (sender, receiver) = mpsc::sync_channel(1);
    if SYNC_WAKE.set(sender).is_err() {
        return;
    }
    std::thread::spawn(move || {
        run_sync_worker(receiver, Duration::from_secs(60), || {
            if let Ok(_guard) = REQUEST_LOCK.lock() {
                if perform_sync(&path).is_ok() {
                    // No account or work details in events; the UI reads current status.
                    let _ = app.emit("online-leagues-updated", ());
                }
            }
        });
    });
}

pub(crate) fn export_data(conn: &Connection, include_cloud: bool) -> Result<Value, String> {
    ensure_schema(conn)?;
    let Some(session) = get_user_session_from_conn(conn) else {
        return Ok(Value::Null);
    };
    let consent:Option<Value>=conn.query_row("SELECT enabled,alias,accepted_at,withdrawal_pending FROM online_local_consent WHERE user_id=?1",[&session.user_id],|r|Ok(json!({"enabled":r.get::<_,bool>(0)?,"alias":r.get::<_,String>(1)?,"accepted_at":r.get::<_,String>(2)?,"withdrawal_pending":r.get::<_,bool>(3)?}))).optional().map_err(|_|"Could not export league settings")?;
    let cloud = if include_cloud && consent.is_some() {
        Some(rpc(&session, json!({"action":"export"}))?)
    } else {
        None
    };
    Ok(json!({"local_consent":consent,"cloud":cloud}))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn an_unauthorized_league_request_renews_once_and_retries_with_the_rotated_session() {
        let session = UserSession {
            user_id: "alice".into(),
            team_id: None,
            access_token: "expired".into(),
            refresh_token: Some("refresh".into()),
            email: "fixture@example.test".into(),
        };
        let mut calls = 0;
        let value = retry_authenticated_request(
            &session,
            |s| {
                calls += 1;
                if s.access_token == "expired" {
                    Err("Sign in again to use online leagues".into())
                } else {
                    Ok(json!({"ok":true}))
                }
            },
            |s| {
                let mut next = s.clone();
                next.access_token = "rotated".into();
                Ok(next)
            },
        )
        .unwrap();
        assert_eq!(calls, 2);
        assert_eq!(value["ok"], true);
        calls = 0;
        assert!(retry_authenticated_request(
            &session,
            |_| {
                calls += 1;
                Err("Sign in again to use online leagues".into())
            },
            |s| Ok(s.clone())
        )
        .is_err());
        assert_eq!(calls, 2);
        assert!(retry_authenticated_request(
            &session,
            |_| Err("Connection unavailable".into()),
            |_| panic!("Network failures must not rotate tokens")
        )
        .is_err());
    }
    #[test]
    fn background_worker_starts_without_ui_and_wakes_and_retries() {
        let (sender, receiver) = mpsc::sync_channel(1);
        let (completed, results) = mpsc::channel();
        let worker = std::thread::spawn(move || {
            let mut attempts = 0;
            run_sync_worker(receiver, Duration::from_millis(30), || {
                attempts += 1;
                // A failed first network attempt does not stop the worker.
                completed.send(attempts > 1).unwrap();
            });
        });
        assert!(!results.recv_timeout(Duration::from_secs(1)).unwrap());
        sender.send(()).unwrap(); // New observation or accepted consent.
        assert!(results.recv_timeout(Duration::from_secs(1)).unwrap());
        assert!(results.recv_timeout(Duration::from_secs(1)).unwrap()); // Offline retry interval.
        drop(sender);
        worker.join().unwrap();
    }
    #[test]
    fn cached_ranking_is_account_bound_and_cannot_restore_withdrawn_consent() {
        let cache = (
            "alice".into(),
            json!({"enabled":true,"day":"2026-10-05","today_points":40,"groups":[{"name":"Friends"}],"synced_at":"2026-10-05T10:00:00Z"}),
        );
        let local = json!({"enabled":true,"account_id":"alice","day":"2026-10-05","today_points":0,"eligible_minutes":25,"groups":[]});
        let view = merge_current_view(local.clone(), Some(&cache));
        assert_eq!(view["today_points"], 40);
        assert_eq!(view["groups"].as_array().unwrap().len(), 1);
        let mut other_account = local.clone();
        other_account["account_id"] = json!("bob");
        assert_eq!(
            merge_current_view(other_account, Some(&cache))["groups"],
            json!([])
        );
        let mut withdrawn = local.clone();
        withdrawn["enabled"] = json!(false);
        assert_eq!(
            merge_current_view(withdrawn, Some(&cache))["enabled"],
            false
        );
        let mut next_day = local;
        next_day["day"] = json!("2026-10-06");
        let view = merge_current_view(next_day, Some(&cache));
        assert_eq!(view["today_points"], 0);
        assert!(view["synced_at"].is_null());
    }
    #[test]
    fn limits_and_thresholds() {
        assert_eq!(points(0), 0);
        assert_eq!(points(24), 0);
        assert_eq!(points(25), 40);
        assert_eq!(points(50), 70);
        assert_eq!(points(75), 100);
        assert_eq!(points(600), 100);
    }
    #[test]
    fn usernames_are_normalized_and_have_a_bounded_safe_format() {
        assert_eq!(normalize_username(" Alex.Foco_1 ").unwrap(), "alex.foco_1");
        for invalid in [
            "a",
            "ab",
            "álex",
            "alex foco",
            "@alex",
            ".alex",
            "<script>",
            "123456789012345678901",
        ] {
            assert!(normalize_username(invalid).is_err(), "{invalid}");
        }
    }
    #[test]
    fn new_account_defaults_to_disabled() {
        let c = Connection::open_in_memory().unwrap();
        ensure_schema(&c).unwrap();
        assert!(!enabled(&c, "test"));
    }
    fn fixture() -> Connection {
        let c = Connection::open_in_memory().unwrap();
        c.execute_batch("CREATE TABLE reports(id INTEGER PRIMARY KEY,created_at TEXT,duration_seconds INTEGER,activity_type TEXT,capture_source TEXT,theme_hint TEXT);CREATE TABLE config(key TEXT PRIMARY KEY,value TEXT);").unwrap();
        ensure_schema(&c).unwrap();
        c.execute("INSERT INTO online_local_consent(user_id,enabled,alias,accepted_at,device_id,notice_version) VALUES('alice',1,'Alice','2026-10-01T08:00:00Z','device',?1)",[NOTICE]).unwrap();
        c
    }
    fn observations(c: &Connection, uid: &str, start: &str, count: i64, source: &str) {
        let start = NaiveDateTime::parse_from_str(start, "%Y-%m-%d %H:%M:%S").unwrap();
        for i in 1..=count {
            c.execute("INSERT INTO reports(created_at,duration_seconds,activity_type,capture_source) VALUES(?1,60,'Research',?2)",params![(start+ChronoDuration::seconds(i*60)).to_string(),source]).unwrap();
            c.execute(
                "INSERT INTO online_observation_owner(report_id,user_id) VALUES(?1,?2)",
                params![c.last_insert_rowid(), uid],
            )
            .unwrap();
        }
    }
    #[test]
    fn only_post_acceptance_observed_minutes_for_this_account_count() {
        let c = fixture();
        observations(&c, "alice", "2026-10-01 07:00:00", 45, "periodic_vision");
        observations(&c, "bob", "2026-10-01 08:00:00", 45, "periodic_vision");
        observations(&c, "alice", "2026-10-01 10:00:00", 90, "manual");
        assert_eq!(eligible_minutes(&c, "alice", "2026-10-01").unwrap(), 0);
        observations(&c, "alice", "2026-10-01 13:00:00", 50, "periodic_vision");
        assert_eq!(eligible_minutes(&c, "alice", "2026-10-01").unwrap(), 50);
        assert_eq!(eligible_minutes(&c, "alice", "2026-10-01").unwrap(), 50);
        c.execute("UPDATE online_local_consent SET enabled=0", [])
            .unwrap();
        assert_eq!(eligible_minutes(&c, "alice", "2026-10-01").unwrap(), 0);
    }
    #[test]
    fn clips_the_first_interval_at_acceptance_and_uses_madrid_midnight() {
        let c = fixture();
        c.execute(
            "UPDATE online_local_consent SET accepted_at='2026-10-01T08:00:30Z'",
            [],
        )
        .unwrap();
        observations(&c, "alice", "2026-10-01 08:00:00", 25, "periodic_vision");
        assert_eq!(eligible_minutes(&c, "alice", "2026-10-01").unwrap(), 0);
        observations(&c, "alice", "2026-10-01 08:25:00", 1, "periodic_vision");
        assert_eq!(eligible_minutes(&c, "alice", "2026-10-01").unwrap(), 25);
        observations(&c, "alice", "2026-10-01 21:30:00", 60, "periodic_vision");
        assert_eq!(eligible_minutes(&c, "alice", "2026-10-01").unwrap(), 55);
        assert_eq!(eligible_minutes(&c, "alice", "2026-10-02").unwrap(), 30);
    }
    #[test]
    fn manual_intervals_break_focus_and_daily_total_is_capped() {
        let c = fixture();
        observations(&c, "alice", "2026-10-01 08:00:00", 20, "periodic_vision");
        observations(&c, "alice", "2026-10-01 08:20:00", 1, "manual");
        observations(&c, "alice", "2026-10-01 08:21:00", 20, "periodic_vision");
        assert_eq!(eligible_minutes(&c, "alice", "2026-10-01").unwrap(), 0);
        observations(&c, "alice", "2026-10-01 10:00:00", 90, "periodic_vision");
        assert_eq!(eligible_minutes(&c, "alice", "2026-10-01").unwrap(), 75);
    }
    #[test]
    fn no_session_never_contacts_cloud() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("local.db");
        let c = Connection::open(&path).unwrap();
        c.execute_batch("CREATE TABLE config(key TEXT PRIMARY KEY,value TEXT);")
            .unwrap();
        assert_eq!(sync_inner(&path).unwrap()["signed_in"], false);
    }
    #[test]
    fn a_capture_is_bound_only_to_the_same_still_opted_in_account() {
        let c = fixture();
        let session = UserSession {
            user_id: "alice".into(),
            team_id: None,
            access_token: "test-token".into(),
            refresh_token: None,
            email: "alice@example.invalid".into(),
        };
        crate::secure_config::save_secret(
            &c,
            "user_session",
            &serde_json::to_string(&session).unwrap(),
        )
        .unwrap();
        crate::entitlements::save_entitlements(
            &c,
            &crate::entitlements::Entitlements {
                owner_user_id: Some("alice".into()),
                usage_profile: Some(json!({"id":"individual"})),
                ..Default::default()
            },
        )
        .unwrap();
        c.execute("INSERT INTO reports(id,created_at,duration_seconds,activity_type,capture_source) VALUES(1,'2026-10-01 09:00:00',60,'Research','periodic_vision')",[]).unwrap();
        bind_observation(&c, 1, None);
        bind_observation(&c, 1, Some("bob"));
        assert_eq!(
            c.query_row("SELECT count(*) FROM online_observation_owner", [], |r| r
                .get::<_, i64>(
                0
            ))
            .unwrap(),
            0
        );
        bind_observation(&c, 1, Some("alice"));
        assert_eq!(
            c.query_row(
                "SELECT user_id FROM online_observation_owner WHERE report_id=1",
                [],
                |r| r.get::<_, String>(0)
            )
            .unwrap(),
            "alice"
        );
        c.execute("DELETE FROM online_observation_owner", [])
            .unwrap();
        c.execute("UPDATE online_local_consent SET enabled=0", [])
            .unwrap();
        bind_observation(&c, 1, Some("alice"));
        assert_eq!(
            c.query_row("SELECT count(*) FROM online_observation_owner", [], |r| r
                .get::<_, i64>(
                0
            ))
            .unwrap(),
            0
        );
    }
}
