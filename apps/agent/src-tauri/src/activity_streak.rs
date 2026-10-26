//! Consecutive local days with recorded activity, independent of daily-win goals.
use chrono::NaiveDate;
use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};
use std::collections::HashSet;

pub const CHECKPOINT_KEY: &str = "streak_checkpoint";
const LOOKBACK_DAYS: i64 = 28;

// Preserve the legacy format so upgrades and retention keep an existing streak.
#[derive(Debug, Deserialize, Serialize)]
struct Checkpoint {
    last_active_day: String,
    streak_days: u64,
}

fn canonical_day(key: &str) -> Option<NaiveDate> {
    NaiveDate::parse_from_str(key, "%Y-%m-%d")
        .ok()
        .filter(|day| day.format("%Y-%m-%d").to_string() == key)
}

fn load_checkpoints(conn: &Connection, today: NaiveDate) -> Result<Vec<Checkpoint>, String> {
    let raw: Option<String> = conn
        .query_row(
            "SELECT value FROM config WHERE key = ?1",
            [CHECKPOINT_KEY],
            |row| row.get(0),
        )
        .optional()
        .map_err(|error| error.to_string())?;
    let checkpoint = raw
        .and_then(|raw| serde_json::from_str::<Checkpoint>(&raw).ok())
        .filter(|saved| {
            saved.streak_days > 0
                && canonical_day(&saved.last_active_day).is_some_and(|day| day <= today)
        });
    // Releases without the activity counter still saved proof of consecutive wins.
    // A win proves activity; this fallback never awards extra wins or tracking time.
    let raw: Option<String> = conn
        .query_row(
            "SELECT value FROM config WHERE key = ?1",
            [crate::daily_flow::PROGRESS_KEY],
            |row| row.get(0),
        )
        .optional()
        .map_err(|error| error.to_string())?;
    let win_checkpoint = raw
        .and_then(|raw| serde_json::from_str::<crate::daily_flow::SavedProgress>(&raw).ok())
        .filter(|saved| {
            saved.streak_at_last_win > 0 && saved.total_wins >= saved.streak_at_last_win
        })
        .and_then(|saved| {
            Some(Checkpoint {
                last_active_day: saved.last_win?,
                streak_days: saved.streak_at_last_win,
            })
        })
        .filter(|saved| canonical_day(&saved.last_active_day).is_some_and(|day| day <= today));
    Ok(match (checkpoint, win_checkpoint) {
        (Some(activity), Some(wins)) => {
            let (mut newer, older) = if activity.last_active_day >= wins.last_active_day {
                (activity, wins)
            } else {
                (wins, activity)
            };
            let distance = canonical_day(&newer.last_active_day)
                .unwrap()
                .signed_duration_since(canonical_day(&older.last_active_day).unwrap())
                .num_days() as u64;
            // Merge overlapping or adjacent proven streaks. A newer win counter
            // also survives if a stale activity checkpoint predates retained data.
            if distance <= newer.streak_days {
                newer.streak_days = newer
                    .streak_days
                    .max(older.streak_days.saturating_add(distance));
            }
            vec![newer, older]
        }
        (activity, wins) => activity.or(wins).into_iter().collect(),
    })
}

fn active_days(
    conn: &Connection,
    start: NaiveDate,
    end: NaiveDate,
) -> Result<HashSet<String>, String> {
    let start_key = start.format("%Y-%m-%d").to_string();
    let end_key = end.format("%Y-%m-%d").to_string();
    let has_reports: bool = conn.query_row(
        "SELECT EXISTS(SELECT 1 FROM pragma_table_info('reports') WHERE name='duration_seconds')",
        [], |row| row.get(0),
    ).map_err(|error| error.to_string())?;
    let mut days = if has_reports {
        // Older installations predate the durable clock. Only their saved durations
        // prove activity; zero-second annotations and report text do not count.
        crate::agent::load_daily_totals(conn, &start_key, &end_key)?
            .into_iter()
            .filter(|(_, seconds)| *seconds > 0)
            .map(|(day, _)| day)
            .collect()
    } else {
        HashSet::new()
    };
    let has_clock: bool = conn.query_row(
        "SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE type='table' AND name='tracking_daily_time')",
        [], |row| row.get(0),
    ).map_err(|error| error.to_string())?;
    if has_clock {
        let mut statement = conn.prepare(
            "SELECT date FROM tracking_daily_time WHERE date >= ?1 AND date <= ?2 AND elapsed_milliseconds > 0",
        ).map_err(|error| error.to_string())?;
        let rows = statement
            .query_map(params![start_key, end_key], |row| row.get::<_, String>(0))
            .map_err(|error| error.to_string())?;
        for key in rows {
            let key = key.map_err(|error| error.to_string())?;
            if canonical_day(&key).is_some() {
                days.insert(key);
            }
        }
    }
    Ok(days)
}

pub fn reconcile(conn: &Connection, today: NaiveDate) -> Result<u64, String> {
    let transaction = conn
        .unchecked_transaction()
        .map_err(|error| error.to_string())?;
    let checkpoints = load_checkpoints(&transaction, today)?;
    let mut day = today;
    let mut streak = 0u64;
    let mut preserved_streak = 0u64;
    let mut last_active_day = None;
    let mut earliest_loaded = today.succ_opt().unwrap_or(today);
    let mut days = HashSet::new();
    loop {
        for saved in checkpoints
            .iter()
            .filter(|saved| canonical_day(&saved.last_active_day) == Some(day))
        {
            preserved_streak = preserved_streak.max(streak.saturating_add(saved.streak_days));
            last_active_day.get_or_insert(day);
        }
        if day < earliest_loaded {
            let start = day
                .checked_sub_signed(chrono::Duration::days(LOOKBACK_DAYS - 1))
                .unwrap_or(NaiveDate::MIN);
            days = active_days(&transaction, start, day)?;
            earliest_loaded = start;
        }
        if days.contains(&day.format("%Y-%m-%d").to_string()) {
            streak = streak.saturating_add(1);
            last_active_day.get_or_insert(day);
        } else if day != today {
            break;
        }
        let Some(previous) = day.pred_opt() else {
            break;
        };
        day = previous;
    }
    // Retention can shorten the available history; upgrades can reveal history
    // longer than the old win counter. Keep whichever proves the longer streak.
    streak = streak.max(preserved_streak);
    if let Some(last) = last_active_day {
        let encoded = serde_json::to_string(&Checkpoint {
            last_active_day: last.format("%Y-%m-%d").to_string(),
            streak_days: streak,
        })
        .map_err(|error| error.to_string())?;
        transaction.execute(
            "INSERT INTO config (key, value) VALUES (?1, ?2) ON CONFLICT(key) DO UPDATE SET value=excluded.value WHERE value != excluded.value",
            params![CHECKPOINT_KEY, encoded],
        ).map_err(|error| error.to_string())?;
    } else {
        transaction
            .execute("DELETE FROM config WHERE key = ?1", [CHECKPOINT_KEY])
            .map_err(|error| error.to_string())?;
    }
    transaction.commit().map_err(|error| error.to_string())?;
    Ok(streak)
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::{Local, TimeZone, Utc};

    fn schema(conn: &Connection) {
        conn.execute_batch("CREATE TABLE config (key TEXT PRIMARY KEY, value TEXT);
            CREATE TABLE reports (created_at TEXT, duration_seconds INTEGER, window_title TEXT);
            CREATE TABLE tracking_daily_time (date TEXT PRIMARY KEY, elapsed_milliseconds INTEGER);").unwrap();
    }

    fn report(conn: &Connection, day: NaiveDate, seconds: i64) {
        let timestamp = Local
            .from_local_datetime(&day.and_hms_opt(12, 0, 0).unwrap())
            .earliest()
            .unwrap()
            .with_timezone(&Utc)
            .format("%Y-%m-%d %H:%M:%S")
            .to_string();
        conn.execute(
            "INSERT INTO reports (created_at, duration_seconds) VALUES (?1, ?2)",
            params![timestamp, seconds],
        )
        .unwrap();
    }

    fn clock(conn: &Connection, day: NaiveDate, milliseconds: i64) {
        conn.execute(
            "INSERT INTO tracking_daily_time VALUES (?1, ?2)",
            params![day.format("%Y-%m-%d").to_string(), milliseconds],
        )
        .unwrap();
    }

    fn checkpoint(conn: &Connection, day: NaiveDate, count: u64) {
        conn.execute(
            "INSERT OR REPLACE INTO config VALUES (?1, ?2)",
            params![
                CHECKPOINT_KEY,
                serde_json::to_string(&Checkpoint {
                    last_active_day: day.format("%Y-%m-%d").to_string(),
                    streak_days: count
                })
                .unwrap()
            ],
        )
        .unwrap();
    }

    #[test]
    fn upgrade_recovers_thirteen_activity_days_without_inflating_five_daily_wins() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("upgrade.db");
        let today = NaiveDate::from_ymd_opt(2026, 10, 7).unwrap();
        let conn = Connection::open(&path).unwrap();
        schema(&conn);
        for offset in 0..13 {
            report(
                &conn,
                today - chrono::Duration::days(offset),
                if offset == 5 { 41 } else { 900 },
            );
        }
        for offset in 0..6 {
            clock(
                &conn,
                today - chrono::Duration::days(offset),
                if offset == 5 { 77000 } else { 900000 },
            );
        }
        let snapshot = || crate::tracking_clock::TrackingClockSnapshot {
            date: today.format("%Y-%m-%d").to_string(),
            total_seconds: 900,
            total_milliseconds: 900000,
            is_running: false,
        };
        assert_eq!(
            reconcile(&conn, today).unwrap(),
            13,
            "History alone recovers the older days"
        );
        checkpoint(&conn, today, 13);
        let view = crate::daily_flow::load_progress(&conn, snapshot()).unwrap();
        assert_eq!(view.streak, 13);
        assert_eq!(view.total_wins, 5);
        assert_eq!(view.completed_dates.len(), 3);
        assert_eq!(view.total_seconds, 900);
        drop(conn);
        let reopened = Connection::open(&path).unwrap();
        for _ in 0..2 {
            let view = crate::daily_flow::load_progress(&reopened, snapshot()).unwrap();
            assert_eq!((view.streak, view.total_wins), (13, 5));
        }
        assert_eq!(
            reopened
                .query_row::<i64, _, _>("SELECT COUNT(*) FROM reports", [], |row| row.get(0))
                .unwrap(),
            13
        );
    }

    #[test]
    fn stale_checkpoint_advances_only_across_real_activity_and_resets_after_gap() {
        let conn = Connection::open_in_memory().unwrap();
        schema(&conn);
        let today = NaiveDate::from_ymd_opt(2026, 10, 7).unwrap();
        checkpoint(&conn, today - chrono::Duration::days(2), 40);
        clock(&conn, today.pred_opt().unwrap(), 1);
        assert_eq!(
            reconcile(&conn, today).unwrap(),
            41,
            "Pending today preserves yesterday"
        );
        clock(&conn, today, 1);
        assert_eq!(reconcile(&conn, today).unwrap(), 42);
        assert_eq!(reconcile(&conn, today.succ_opt().unwrap()).unwrap(), 42);
        let after_gap = today + chrono::Duration::days(2);
        assert_eq!(reconcile(&conn, after_gap).unwrap(), 0);
        report(&conn, after_gap, 1);
        assert_eq!(reconcile(&conn, after_gap).unwrap(), 1);
    }

    #[test]
    fn older_history_can_extend_a_win_checkpoint_without_double_counting_sources() {
        let conn = Connection::open_in_memory().unwrap();
        schema(&conn);
        let today = NaiveDate::from_ymd_opt(2026, 10, 7).unwrap();
        for offset in 0..36 {
            report(&conn, today - chrono::Duration::days(offset), 60);
            clock(&conn, today - chrono::Duration::days(offset), 60000);
        }
        let saved = crate::daily_flow::SavedProgress {
            total_wins: 5,
            last_win: Some(today.format("%Y-%m-%d").to_string()),
            streak_at_last_win: 5,
            ..Default::default()
        };
        conn.execute(
            "INSERT INTO config VALUES (?1, ?2)",
            params![
                crate::daily_flow::PROGRESS_KEY,
                serde_json::to_string(&saved).unwrap()
            ],
        )
        .unwrap();
        assert_eq!(reconcile(&conn, today).unwrap(), 36);
        conn.execute("DELETE FROM reports", []).unwrap();
        conn.execute("DELETE FROM tracking_daily_time", []).unwrap();
        assert_eq!(reconcile(&conn, today).unwrap(), 36);
    }

    #[test]
    fn win_checkpoint_preserves_proven_activity_when_history_has_expired() {
        let conn = Connection::open_in_memory().unwrap();
        schema(&conn);
        let today = NaiveDate::from_ymd_opt(2026, 10, 7).unwrap();
        let saved = crate::daily_flow::SavedProgress {
            total_wins: 70,
            last_win: Some(today.pred_opt().unwrap().format("%Y-%m-%d").to_string()),
            streak_at_last_win: 70,
            ..Default::default()
        };
        conn.execute(
            "INSERT INTO config VALUES (?1, ?2)",
            params![
                crate::daily_flow::PROGRESS_KEY,
                serde_json::to_string(&saved).unwrap()
            ],
        )
        .unwrap();
        assert_eq!(reconcile(&conn, today).unwrap(), 70);
        clock(&conn, today, 1);
        assert_eq!(reconcile(&conn, today).unwrap(), 71);
    }

    #[test]
    fn newer_win_proof_merges_with_an_old_activity_checkpoint_after_retention() {
        let conn = Connection::open_in_memory().unwrap();
        schema(&conn);
        let today = NaiveDate::from_ymd_opt(2026, 10, 7).unwrap();
        checkpoint(&conn, today - chrono::Duration::days(5), 40);
        let saved = crate::daily_flow::SavedProgress {
            total_wins: 5,
            last_win: Some(today.format("%Y-%m-%d").to_string()),
            streak_at_last_win: 5,
            ..Default::default()
        };
        conn.execute(
            "INSERT INTO config VALUES (?1, ?2)",
            params![
                crate::daily_flow::PROGRESS_KEY,
                serde_json::to_string(&saved).unwrap()
            ],
        )
        .unwrap();
        assert_eq!(reconcile(&conn, today).unwrap(), 45);
        assert_eq!(reconcile(&conn, today).unwrap(), 45);
        checkpoint(&conn, today - chrono::Duration::days(6), 40);
        assert_eq!(
            reconcile(&conn, today).unwrap(),
            5,
            "A real gap between proofs cannot extend the streak"
        );
        clock(&conn, today - chrono::Duration::days(5), 1);
        for offset in 0..5 {
            clock(&conn, today - chrono::Duration::days(offset), 900000);
        }
        checkpoint(&conn, today - chrono::Duration::days(6), 40);
        assert_eq!(
            reconcile(&conn, today).unwrap(),
            46,
            "Retained short activity can bridge the older checkpoint and newer wins"
        );
    }

    #[test]
    fn new_win_after_a_short_activity_day_does_not_overwrite_migration_evidence() {
        let conn = Connection::open_in_memory().unwrap();
        schema(&conn);
        let today = NaiveDate::from_ymd_opt(2026, 10, 7).unwrap();
        let saved = crate::daily_flow::SavedProgress {
            total_wins: 70,
            last_win: Some(
                (today - chrono::Duration::days(2))
                    .format("%Y-%m-%d")
                    .to_string(),
            ),
            streak_at_last_win: 70,
            ..Default::default()
        };
        conn.execute(
            "INSERT INTO config VALUES (?1, ?2)",
            params![
                crate::daily_flow::PROGRESS_KEY,
                serde_json::to_string(&saved).unwrap()
            ],
        )
        .unwrap();
        clock(&conn, today.pred_opt().unwrap(), 1);
        clock(&conn, today, 900000);
        let view = crate::daily_flow::load_progress(
            &conn,
            crate::tracking_clock::TrackingClockSnapshot {
                date: today.format("%Y-%m-%d").to_string(),
                total_seconds: 900,
                total_milliseconds: 900000,
                is_running: false,
            },
        )
        .unwrap();
        assert_eq!(view.streak, 72);
        assert_eq!(view.total_wins, 71);
        assert_eq!(reconcile(&conn, today).unwrap(), 72);
    }

    #[test]
    fn zero_duration_annotations_and_invalid_future_data_cannot_start_a_streak() {
        let conn = Connection::open_in_memory().unwrap();
        schema(&conn);
        let today = NaiveDate::from_ymd_opt(2026, 10, 7).unwrap();
        report(&conn, today, 0);
        report(&conn, today.pred_opt().unwrap(), -20);
        clock(&conn, today, 0);
        clock(&conn, today.succ_opt().unwrap(), 900000);
        conn.execute(
            "INSERT INTO tracking_daily_time VALUES ('2026-10-99', 900000)",
            [],
        )
        .unwrap();
        for raw in [
            "broken",
            r#"{"last_active_day":"2026-10-99","streak_days":900}"#,
            r#"{"last_active_day":"2026-10-08","streak_days":900}"#,
        ] {
            conn.execute(
                "INSERT OR REPLACE INTO config VALUES (?1, ?2)",
                params![CHECKPOINT_KEY, raw],
            )
            .unwrap();
            assert_eq!(reconcile(&conn, today).unwrap(), 0);
        }
    }

    #[test]
    fn local_midnight_report_proves_two_days_even_before_clock_was_introduced() {
        let conn = Connection::open_in_memory().unwrap();
        schema(&conn);
        conn.execute("DROP TABLE tracking_daily_time", []).unwrap();
        let monday = NaiveDate::from_ymd_opt(2026, 10, 26).unwrap(); // Madrid DST weekend
        let timestamp = Local
            .from_local_datetime(&monday.and_hms_opt(0, 1, 0).unwrap())
            .earliest()
            .unwrap()
            .with_timezone(&Utc)
            .format("%Y-%m-%d %H:%M:%S")
            .to_string();
        conn.execute(
            "INSERT INTO reports (created_at, duration_seconds) VALUES (?1, 120)",
            [timestamp],
        )
        .unwrap();
        assert_eq!(reconcile(&conn, monday).unwrap(), 2);
    }

    #[test]
    fn real_retention_saves_legacy_history_before_deletion_and_erasure_removes_checkpoint() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("retention.db");
        let conn = Connection::open(&path).unwrap();
        schema(&conn);
        let today = Local::now().date_naive();
        for offset in 0..36 {
            report(&conn, today - chrono::Duration::days(offset), 1);
        }
        let mut settings = crate::privacy::load_privacy_settings(&path).unwrap();
        settings.retention_days = 1;
        conn.execute(
            "INSERT OR REPLACE INTO config VALUES ('privacy_settings', ?1)",
            [serde_json::to_string(&settings).unwrap()],
        )
        .unwrap();
        assert!(crate::privacy::enforce_local_retention(&path).unwrap() > 0);
        assert_eq!(reconcile(&conn, today).unwrap(), 36);
        let raw: String = conn
            .query_row(
                "SELECT value FROM config WHERE key=?1",
                [CHECKPOINT_KEY],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(
            serde_json::from_str::<serde_json::Value>(&raw)
                .unwrap()
                .as_object()
                .unwrap()
                .len(),
            2
        );
        crate::privacy::erase_local_database(&conn).unwrap();
        assert_eq!(reconcile(&conn, today).unwrap(), 0);
    }
}
