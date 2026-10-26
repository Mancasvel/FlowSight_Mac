//! Personal habit feedback derived from the durable tracking clock, never AI reports.
use crate::tracking_clock::TrackingClockSnapshot;
use chrono::{Datelike, NaiveDate};
use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};

pub const DAILY_WIN_SECONDS: u64 = 15 * 60;
pub const PROGRESS_KEY: &str = "daily_flow_progress_v1";

// Keep counters and at most seven current-week dates, never a lifetime date ledger.
#[derive(Debug, Default, Deserialize, Serialize)]
pub struct SavedProgress {
    pub total_wins: u64,
    pub last_win: Option<String>,
    pub streak_at_last_win: u64,
    pub week_start: String,
    pub week_dates: Vec<String>,
}

#[derive(Debug, Serialize)]
pub struct DailyFlowProgress {
    pub date: String,
    pub total_seconds: u64,
    pub completed_dates: Vec<String>,
    pub total_wins: u64,
    pub streak: u64,
}

pub fn reconcile_progress(conn: &Connection, today: NaiveDate) -> Result<SavedProgress, String> {
    let transaction = conn
        .unchecked_transaction()
        .map_err(|error| error.to_string())?;
    let raw: Option<String> = transaction
        .query_row(
            "SELECT value FROM config WHERE key = ?1",
            [PROGRESS_KEY],
            |row| row.get(0),
        )
        .optional()
        .map_err(|error| error.to_string())?;
    let mut saved: SavedProgress = raw
        .as_deref()
        .map(serde_json::from_str)
        .transpose()
        .map_err(|error| error.to_string())?
        .unwrap_or_default();
    let monday = today - chrono::Duration::days(today.weekday().num_days_from_monday() as i64);
    let week_start = monday.format("%Y-%m-%d").to_string();
    if saved.week_start != week_start {
        saved.week_start = week_start;
        saved.week_dates.clear();
    }
    let today_key = today.format("%Y-%m-%d").to_string();
    let mut statement = transaction
        .prepare(
            "SELECT date FROM tracking_daily_time
             WHERE elapsed_milliseconds >= ?1 AND date <= ?2 ORDER BY date",
        )
        .map_err(|error| error.to_string())?;
    let dates = statement
        .query_map(params![DAILY_WIN_SECONDS * 1000, &today_key], |row| {
            row.get::<_, String>(0)
        })
        .map_err(|error| error.to_string())?;
    let completed_dates = dates
        .collect::<Result<Vec<_>, _>>()
        .map_err(|error| error.to_string())?
        .into_iter()
        .filter(|date| {
            NaiveDate::parse_from_str(date, "%Y-%m-%d")
                .is_ok_and(|parsed| parsed.format("%Y-%m-%d").to_string() == *date)
        })
        .collect::<Vec<_>>();
    drop(statement);
    for key in completed_dates {
        let date =
            NaiveDate::parse_from_str(&key, "%Y-%m-%d").map_err(|error| error.to_string())?;
        if saved.last_win.as_ref().map_or(true, |last| key > *last) {
            let follows_last = saved
                .last_win
                .as_deref()
                .and_then(|last| NaiveDate::parse_from_str(last, "%Y-%m-%d").ok())
                .is_some_and(|last| date.signed_duration_since(last).num_days() == 1);
            saved.streak_at_last_win = if follows_last {
                saved.streak_at_last_win.saturating_add(1)
            } else {
                1
            };
            saved.last_win = Some(key.clone());
            saved.total_wins = saved.total_wins.saturating_add(1);
        }
        if date >= monday && !saved.week_dates.contains(&key) {
            saved.week_dates.push(key);
        }
    }
    saved.week_dates.sort();
    let encoded = serde_json::to_string(&saved).map_err(|error| error.to_string())?;
    if raw.as_deref() != Some(&encoded) {
        transaction
            .execute(
                "INSERT OR REPLACE INTO config (key, value) VALUES (?1, ?2)",
                params![PROGRESS_KEY, encoded],
            )
            .map_err(|error| error.to_string())?;
    }
    transaction.commit().map_err(|error| error.to_string())?;
    Ok(saved)
}

pub fn load_progress(
    conn: &Connection,
    snapshot: TrackingClockSnapshot,
) -> Result<DailyFlowProgress, String> {
    let today =
        NaiveDate::parse_from_str(&snapshot.date, "%Y-%m-%d").map_err(|error| error.to_string())?;
    // Recover proof from the old win checkpoint before new wins can reset it.
    let streak = crate::activity_streak::reconcile(conn, today)?;
    let saved = reconcile_progress(conn, today)?;
    Ok(DailyFlowProgress {
        date: snapshot.date,
        total_seconds: snapshot.total_seconds,
        completed_dates: saved.week_dates,
        total_wins: saved.total_wins,
        streak,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wins_use_saved_clock_threshold_not_reports_or_open_window_time() {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch(
            "CREATE TABLE config (key TEXT PRIMARY KEY, value TEXT);
             CREATE TABLE tracking_daily_time (date TEXT PRIMARY KEY, elapsed_milliseconds INTEGER);
             INSERT INTO tracking_daily_time VALUES
             ('2026-09-30', 899999), ('2026-10-01', 900000),
             ('2026-10-02', 3600000), ('2026-10-03', 0),
             ('2026-10-04', 900000), ('2026-09-99', 900000);",
        )
        .unwrap();
        let snapshot = || TrackingClockSnapshot {
            date: "2026-10-03".into(),
            total_seconds: 0,
            total_milliseconds: 0,
            is_running: false,
        };
        for _ in 0..2 {
            let progress = load_progress(&conn, snapshot()).unwrap();
            assert_eq!(progress.completed_dates, ["2026-10-01", "2026-10-02"]);
            assert_eq!(progress.total_seconds, 0);
            assert_eq!(progress.total_wins, 2);
        }
    }

    #[test]
    fn milestones_and_week_survive_real_one_day_retention_and_erase_with_local_data() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("retention.db");
        let conn = Connection::open(&path).unwrap();
        conn.execute_batch("CREATE TABLE config (key TEXT PRIMARY KEY, value TEXT);
            CREATE TABLE reports (created_at TEXT, window_title TEXT);
            CREATE TABLE tracking_daily_time (date TEXT PRIMARY KEY, elapsed_milliseconds INTEGER);").unwrap();
        let today = chrono::Local::now().date_naive();
        for offset in 0..70 {
            let day = today - chrono::Duration::days(offset);
            conn.execute(
                "INSERT INTO tracking_daily_time VALUES (?1, 900000)",
                [day.format("%Y-%m-%d").to_string()],
            )
            .unwrap();
        }
        let snapshot = || TrackingClockSnapshot {
            date: today.format("%Y-%m-%d").to_string(),
            total_seconds: 900,
            total_milliseconds: 900000,
            is_running: false,
        };
        // The retention hook must capture earned progress even before UI is opened.
        let mut settings = crate::privacy::load_privacy_settings(&path).unwrap();
        settings.retention_days = 1;
        conn.execute(
            "INSERT OR REPLACE INTO config VALUES ('privacy_settings', ?1)",
            [serde_json::to_string(&settings).unwrap()],
        )
        .unwrap();
        crate::privacy::enforce_local_retention(&path).unwrap();
        let view = load_progress(&conn, snapshot()).unwrap();
        assert_eq!(view.total_wins, 70);
        assert_eq!(view.streak, 70);
        assert_eq!(
            view.completed_dates.len(),
            today.weekday().num_days_from_monday() as usize + 1
        );
        assert_eq!(load_progress(&conn, snapshot()).unwrap().total_wins, 70);
        crate::privacy::erase_local_database(&conn).unwrap();
        assert!(conn
            .query_row::<String, _, _>(
                "SELECT value FROM config WHERE key=?1",
                [PROGRESS_KEY],
                |row| row.get(0)
            )
            .optional()
            .unwrap()
            .is_none());
    }
}
