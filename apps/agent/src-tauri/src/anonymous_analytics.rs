//! Opt-in anonymous product analytics (local-first; Supabase only after explicit consent).
//!
//! Collects only aggregate usage: daily minutes and weekly primary activity category.
//! No account, email, or other personally identifiable information is sent.

use chrono::{Datelike, Local, NaiveDate};
use reqwest::blocking::Client;
use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::thread;
use std::time::Duration;
use uuid::Uuid;

use crate::sync_env::{supabase_anon_key, supabase_url};

const CONSENT_KEY: &str = "anonymous_analytics_consent";
const ANALYTICS_SYNC_INTERVAL_HOURS: u64 = 6;

#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq, Eq)]
pub struct AnalyticsConsent {
    #[serde(rename = "decided", default)]
    pub decided: bool,
    #[serde(rename = "consented", default)]
    pub consented: bool,
    #[serde(rename = "anonymousId", default)]
    pub anonymous_id: Option<String>,
    #[serde(rename = "decidedAt", default)]
    pub decided_at: Option<String>,
}

#[derive(Serialize, Debug, Clone, PartialEq, Eq)]
struct DailyUsageEntry {
    date: String,
    minutes: i32,
}

pub fn load_analytics_consent(db_path: &Path) -> Result<AnalyticsConsent, String> {
    let conn = Connection::open(db_path).map_err(|e| e.to_string())?;
    let raw: Option<String> = conn
        .query_row(
            "SELECT value FROM config WHERE key = ?1",
            params![CONSENT_KEY],
            |row| row.get(0),
        )
        .ok();

    match raw {
        Some(json) => {
            serde_json::from_str(&json).map_err(|e| format!("Invalid analytics consent JSON: {e}"))
        }
        None => Ok(AnalyticsConsent::default()),
    }
}

pub fn save_analytics_consent(
    db_path: &Path,
    consent: AnalyticsConsent,
) -> Result<AnalyticsConsent, String> {
    let json = serde_json::to_string(&consent).map_err(|e| e.to_string())?;
    let conn = Connection::open(db_path).map_err(|e| e.to_string())?;
    conn.execute(
        "INSERT OR REPLACE INTO config (key, value) VALUES (?1, ?2)",
        params![CONSENT_KEY, json],
    )
    .map_err(|e| e.to_string())?;
    Ok(consent)
}

pub fn compute_daily_usage(
    conn: &Connection,
    days: i32,
) -> Result<Vec<DailyUsageEntry>, String> {
    if days <= 0 {
        return Ok(Vec::new());
    }

    let today = Local::now().date_naive();
    let start = today - chrono::Duration::days((days - 1) as i64);
    let start_str = start.format("%Y-%m-%d").to_string();
    let end_str = today.format("%Y-%m-%d").to_string();

    let mut stmt = conn
        .prepare(
            "SELECT date(created_at, 'localtime') as d, SUM(duration_seconds) as total
             FROM reports
             WHERE date(created_at, 'localtime') >= ?1 AND date(created_at, 'localtime') <= ?2
             GROUP BY d",
        )
        .map_err(|e| e.to_string())?;

    let mut totals: std::collections::HashMap<String, i32> = std::collections::HashMap::new();
    let rows = stmt
        .query_map(params![start_str, end_str], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, i32>(1).unwrap_or(0),
            ))
        })
        .map_err(|e| e.to_string())?;

    for row in rows.filter_map(|r| r.ok()) {
        totals.insert(row.0, row.1);
    }

    let mut entries = Vec::with_capacity(days as usize);
    for offset in 0..days {
        let day = start + chrono::Duration::days(offset as i64);
        let date_str = day.format("%Y-%m-%d").to_string();
        let seconds = totals.get(&date_str).copied().unwrap_or(0);
        entries.push(DailyUsageEntry {
            date: date_str,
            minutes: seconds.div_euclid(60),
        });
    }

    Ok(entries)
}

pub fn compute_weekly_primary_activity(conn: &Connection) -> Result<Option<String>, String> {
    let today = Local::now().date_naive();
    let weekday = today.weekday().num_days_from_monday();
    let week_start = today - chrono::Duration::days(weekday as i64);
    let week_end = week_start + chrono::Duration::days(6);
    let start_str = week_start.format("%Y-%m-%d").to_string();
    let end_str = week_end.format("%Y-%m-%d").to_string();

    let mut stmt = conn
        .prepare(
            "SELECT activity_type, SUM(duration_seconds) as total
             FROM reports
             WHERE date(created_at, 'localtime') >= ?1 AND date(created_at, 'localtime') <= ?2
             GROUP BY activity_type
             ORDER BY total DESC
             LIMIT 1",
        )
        .map_err(|e| e.to_string())?;

    let row = stmt
        .query_row(params![start_str, end_str], |row| {
            let category: String = row.get(0)?;
            let total: i32 = row.get(1)?;
            Ok((category, total))
        })
        .optional()
        .map_err(|e| e.to_string())?;

    Ok(row.filter(|(_, total)| *total > 0).map(|(category, _)| category))
}

fn upsert_anonymous_analytics_on_supabase(
    anonymous_id: &str,
    consented: bool,
    daily_usage: &[DailyUsageEntry],
    weekly_primary_activity: Option<&str>,
) -> Result<(), String> {
    let client = Client::builder()
        .timeout(Duration::from_secs(20))
        .build()
        .map_err(|e| e.to_string())?;

    let url = format!(
        "{}/rest/v1/rpc/upsert_anonymous_product_analytics",
        supabase_url()
    );

    let body = serde_json::json!({
        "p_anonymous_id": anonymous_id,
        "p_consented": consented,
        "p_daily_usage": daily_usage,
        "p_weekly_primary_activity": weekly_primary_activity,
    });

    let resp = client
        .post(&url)
        .header("apikey", supabase_anon_key())
        .header("Authorization", format!("Bearer {}", supabase_anon_key()))
        .header("Content-Type", "application/json")
        .json(&body)
        .send()
        .map_err(|e| format!("Analytics sync request failed: {e}"))?;

    if !resp.status().is_success() {
        let status = resp.status();
        let body = resp.text().unwrap_or_default();
        return Err(format!("Analytics sync failed ({status}): {body}"));
    }

    Ok(())
}

pub fn sync_anonymous_analytics_to_supabase(
    db_path: &Path,
    consent: &AnalyticsConsent,
) -> Result<(), String> {
    let anonymous_id = consent
        .anonymous_id
        .as_deref()
        .ok_or_else(|| "Missing anonymous analytics ID".to_string())?;

    let conn = Connection::open(db_path).map_err(|e| e.to_string())?;
    let daily_usage = compute_daily_usage(&conn, 7)?;
    let weekly_primary_activity = compute_weekly_primary_activity(&conn)?;

    upsert_anonymous_analytics_on_supabase(
        anonymous_id,
        consent.consented,
        &daily_usage,
        weekly_primary_activity.as_deref(),
    )
}

pub fn perform_analytics_sync(db_path: &Path) -> Result<bool, String> {
    let consent = load_analytics_consent(db_path)?;
    if !consent.consented {
        return Ok(false);
    }
    sync_anonymous_analytics_to_supabase(db_path, &consent)?;
    Ok(true)
}

pub fn start_analytics_sync_thread(db_path: PathBuf) {
    thread::spawn(move || {
        loop {
            match perform_analytics_sync(&db_path) {
                Ok(true) => log::debug!("[Analytics] Background sync completed"),
                Ok(false) => {}
                Err(e) => log::debug!("[Analytics] Background sync failed: {e}"),
            }
            thread::sleep(Duration::from_secs(ANALYTICS_SYNC_INTERVAL_HOURS * 3600));
        }
    });
}

#[tauri::command]
pub fn get_analytics_consent() -> Result<AnalyticsConsent, String> {
    let db_path = crate::paths::db_path()?;
    load_analytics_consent(&db_path)
}

#[tauri::command]
pub fn set_analytics_consent(consented: bool) -> Result<AnalyticsConsent, String> {
    let db_path = crate::paths::db_path()?;
    let mut consent = load_analytics_consent(&db_path)?;
    consent.decided = true;
    consent.consented = consented;
    consent.decided_at = Some(Local::now().format("%Y-%m-%d %H:%M").to_string());

    if consented && consent.anonymous_id.is_none() {
        consent.anonymous_id = Some(Uuid::new_v4().to_string());
    }

    let saved = save_analytics_consent(&db_path, consent)?;

    if saved.consented {
        sync_anonymous_analytics_to_supabase(&db_path, &saved)?;
    } else if saved.anonymous_id.is_some() {
        upsert_anonymous_analytics_on_supabase(
            saved.anonymous_id.as_deref().unwrap_or_default(),
            false,
            &[],
            None,
        )?;
    }

    Ok(saved)
}

#[tauri::command]
pub fn sync_anonymous_analytics() -> Result<bool, String> {
    let db_path = crate::paths::db_path()?;
    perform_analytics_sync(&db_path)
}

const FEEDBACK_MIN_CHARS: usize = 3;
const FEEDBACK_MAX_CHARS: usize = 2000;

fn submit_feedback_to_supabase(
    message: &str,
    anonymous_id: Option<&str>,
    app_version: &str,
) -> Result<(), String> {
    let client = Client::builder()
        .timeout(Duration::from_secs(20))
        .build()
        .map_err(|e| e.to_string())?;

    let url = format!(
        "{}/rest/v1/rpc/submit_product_feedback",
        supabase_url()
    );

    let body = serde_json::json!({
        "p_message": message,
        "p_anonymous_id": anonymous_id,
        "p_app_version": app_version,
    });

    let resp = client
        .post(&url)
        .header("apikey", supabase_anon_key())
        .header("Authorization", format!("Bearer {}", supabase_anon_key()))
        .header("Content-Type", "application/json")
        .json(&body)
        .send()
        .map_err(|e| format!("Feedback request failed: {e}"))?;

    if !resp.status().is_success() {
        let status = resp.status();
        let body = resp.text().unwrap_or_default();
        return Err(format!("Feedback submit failed ({status}): {body}"));
    }

    Ok(())
}

#[tauri::command]
pub fn submit_product_feedback(message: String) -> Result<(), String> {
    let trimmed = message.trim();
    if trimmed.len() < FEEDBACK_MIN_CHARS {
        return Err(format!(
            "Feedback must be at least {FEEDBACK_MIN_CHARS} characters."
        ));
    }
    if trimmed.len() > FEEDBACK_MAX_CHARS {
        return Err(format!(
            "Feedback must be at most {FEEDBACK_MAX_CHARS} characters."
        ));
    }

    let db_path = crate::paths::db_path()?;
    let consent = load_analytics_consent(&db_path).unwrap_or_default();
    submit_feedback_to_supabase(
        trimmed,
        consent.anonymous_id.as_deref(),
        env!("CARGO_PKG_VERSION"),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use rusqlite::Connection;
    use std::fs;

    fn temp_db() -> (tempfile::TempDir, PathBuf) {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("test.db");
        let conn = Connection::open(&path).expect("open db");
        conn.execute_batch(
            "CREATE TABLE reports (
                id INTEGER PRIMARY KEY,
                description TEXT,
                activity_type TEXT,
                created_at TEXT,
                duration_seconds INTEGER DEFAULT 30
             );",
        )
        .expect("schema");
        (dir, path)
    }

    fn insert_report(conn: &Connection, created_at: &str, category: &str, seconds: i32) {
        conn.execute(
            "INSERT INTO reports (description, activity_type, created_at, duration_seconds)
             VALUES ('test', ?1, ?2, ?3)",
            params![category, created_at, seconds],
        )
        .expect("insert");
    }

    #[test]
    fn daily_usage_aggregates_minutes_per_day() {
        let (_dir, path) = temp_db();
        let conn = Connection::open(&path).expect("open");
        let today = Local::now().date_naive();
        let yesterday = today - chrono::Duration::days(1);
        insert_report(
            &conn,
            &format!("{} 12:00:00", today.format("%Y-%m-%d")),
            "Coding",
            3600,
        );
        insert_report(
            &conn,
            &format!("{} 09:00:00", yesterday.format("%Y-%m-%d")),
            "Admin",
            1800,
        );

        let usage = compute_daily_usage(&conn, 2).expect("usage");
        assert_eq!(usage.len(), 2);
        assert_eq!(usage[0].minutes, 30);
        assert_eq!(usage[1].minutes, 60);
    }

    #[test]
    fn weekly_primary_activity_picks_top_category() {
        let (_dir, path) = temp_db();
        let conn = Connection::open(&path).expect("open");
        let today = Local::now().date_naive();
        let weekday = today.weekday().num_days_from_monday();
        let monday = today - chrono::Duration::days(weekday as i64);
        let stamp = |day: NaiveDate| format!("{} 10:00:00", day.format("%Y-%m-%d"));

        insert_report(&conn, &stamp(monday), "Coding", 7200);
        insert_report(
            &conn,
            &stamp(monday + chrono::Duration::days(1)),
            "Admin",
            900,
        );

        let primary = compute_weekly_primary_activity(&conn).expect("primary");
        assert_eq!(primary.as_deref(), Some("Coding"));
    }

    #[test]
    fn consent_roundtrip_in_config_table() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("consent.db");
        let conn = Connection::open(&path).expect("open");
        conn.execute(
            "CREATE TABLE config (key TEXT PRIMARY KEY, value TEXT)",
            [],
        )
        .expect("schema");
        drop(conn);

        let consent = AnalyticsConsent {
            decided: true,
            consented: true,
            anonymous_id: Some("00000000-0000-4000-8000-000000000001".to_string()),
            decided_at: Some("2026-06-24 10:00".to_string()),
        };
        save_analytics_consent(&path, consent.clone()).expect("save");
        let loaded = load_analytics_consent(&path).expect("load");
        assert_eq!(loaded, consent);
        fs::remove_dir_all(dir.path()).ok();
    }
}
