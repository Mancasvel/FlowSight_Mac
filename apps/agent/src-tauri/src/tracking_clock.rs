//! Monotonic tracking time, independent of when activity analysis finishes.
use chrono::{DateTime, Local, TimeZone};
use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};
use std::time::Instant;

#[derive(Debug, Serialize, Deserialize)]
pub struct TrackingClockSnapshot {
    pub date: String,
    pub total_seconds: u64,
    pub total_milliseconds: u64,
    pub is_running: bool,
}

pub struct TrackingClock {
    day: String,
    elapsed_ms: u64,
    last_tick: Option<Instant>,
}

impl TrackingClock {
    pub fn load(conn: &Connection, observed_seconds: u64) -> Result<Self, String> {
        Self::load_at(conn, observed_seconds, Local::now())
    }

    fn load_at(
        conn: &Connection,
        observed_seconds: u64,
        wall: DateTime<Local>,
    ) -> Result<Self, String> {
        conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS tracking_daily_time (
                date TEXT PRIMARY KEY, elapsed_milliseconds INTEGER NOT NULL
             );",
        )
        .map_err(|e| e.to_string())?;
        let day = wall.format("%Y-%m-%d").to_string();
        conn.execute(
            "INSERT OR IGNORE INTO tracking_daily_time VALUES (?1, ?2)",
            params![
                day,
                observed_seconds.saturating_mul(1000).min(i64::MAX as u64)
            ],
        )
        .map_err(|e| e.to_string())?;
        let elapsed_ms: i64 = conn
            .query_row(
                "SELECT elapsed_milliseconds FROM tracking_daily_time WHERE date = ?1",
                [&day],
                |row| row.get(0),
            )
            .map_err(|e| e.to_string())?;
        Ok(Self {
            day,
            elapsed_ms: elapsed_ms.max(0) as u64,
            last_tick: None,
        })
    }

    pub fn snapshot(
        &mut self,
        conn: &Connection,
        running: bool,
    ) -> Result<TrackingClockSnapshot, String> {
        self.snapshot_at(conn, running, Instant::now(), Local::now())
    }

    fn snapshot_at(
        &mut self,
        conn: &Connection,
        running: bool,
        tick: Instant,
        wall: DateTime<Local>,
    ) -> Result<TrackingClockSnapshot, String> {
        let elapsed = self.last_tick.map_or(0, |previous| {
            tick.saturating_duration_since(previous)
                .as_millis()
                .min(u64::MAX as u128) as u64
        });
        let day = wall.format("%Y-%m-%d").to_string();
        let next_elapsed;
        let tx = conn.unchecked_transaction().map_err(|e| e.to_string())?;
        if day != self.day {
            // Allocate only the portion after local midnight to the new day.
            // A stopped clock adds no time, even when the window was closed.
            let midnight = Local
                .from_local_datetime(&wall.date_naive().and_hms_opt(0, 0, 0).unwrap())
                .earliest()
                .unwrap_or(wall);
            let today_ms = wall
                .signed_duration_since(midnight)
                .num_milliseconds()
                .max(0) as u64;
            let after_midnight = elapsed.min(today_ms);
            Self::save(
                &tx,
                &self.day,
                self.elapsed_ms.saturating_add(elapsed - after_midnight),
            )?;
            let existing = tx
                .query_row::<i64, _, _>(
                    "SELECT elapsed_milliseconds FROM tracking_daily_time WHERE date = ?1",
                    [&day],
                    |row| row.get(0),
                )
                .optional()
                .map_err(|e| e.to_string())?
                .unwrap_or(0)
                .max(0) as u64;
            next_elapsed = existing.saturating_add(after_midnight);
        } else {
            next_elapsed = self.elapsed_ms.saturating_add(elapsed);
        }
        Self::save(&tx, &day, next_elapsed)?;
        tx.commit().map_err(|e| e.to_string())?;
        // Commit memory only after SQLite succeeds. A retry cannot charge a delta twice.
        self.day = day;
        self.elapsed_ms = next_elapsed;
        self.last_tick = running.then_some(tick);
        Ok(TrackingClockSnapshot {
            date: self.day.clone(),
            total_seconds: self.elapsed_ms / 1000,
            total_milliseconds: self.elapsed_ms,
            is_running: running,
        })
    }

    fn save(conn: &Connection, day: &str, elapsed_ms: u64) -> Result<(), String> {
        conn.execute(
            "INSERT INTO tracking_daily_time VALUES (?1, ?2)
             ON CONFLICT(date) DO UPDATE SET elapsed_milliseconds = excluded.elapsed_milliseconds",
            params![day, elapsed_ms.min(i64::MAX as u64)],
        )
        .map(|_| ())
        .map_err(|e| e.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    fn wall(hour: u32, minute: u32, second: u32) -> DateTime<Local> {
        Local
            .with_ymd_and_hms(2026, 10, 1, hour, minute, second)
            .unwrap()
    }

    #[test]
    fn ticks_pause_resume_and_analysis_never_double_count() {
        let conn = Connection::open_in_memory().unwrap();
        let tick = Instant::now();
        let mut clock = TrackingClock::load_at(&conn, 20700, wall(12, 0, 0)).unwrap();
        clock
            .snapshot_at(&conn, true, tick, wall(12, 0, 0))
            .unwrap();
        let sample = clock
            .snapshot_at(
                &conn,
                true,
                tick + Duration::from_millis(1500),
                wall(12, 0, 1),
            )
            .unwrap();
        assert_eq!(sample.total_milliseconds, 20701500);
        assert_eq!(
            clock
                .snapshot_at(&conn, false, tick + Duration::from_secs(3), wall(12, 0, 3))
                .unwrap()
                .total_seconds,
            20703
        );
        assert_eq!(
            clock
                .snapshot_at(&conn, true, tick + Duration::from_secs(63), wall(12, 1, 3))
                .unwrap()
                .total_seconds,
            20703
        );
        assert_eq!(
            clock
                .snapshot_at(&conn, false, tick + Duration::from_secs(64), wall(12, 1, 4))
                .unwrap()
                .total_seconds,
            20704
        );
        // A new analysis or old renderer checkpoint cannot inflate persisted time.
        let reloaded = TrackingClock::load_at(&conn, 21571, wall(12, 1, 4)).unwrap();
        assert_eq!(reloaded.elapsed_ms, 20704000);
    }

    #[test]
    fn splits_local_midnight_and_does_not_charge_paused_time() {
        let conn = Connection::open_in_memory().unwrap();
        let tick = Instant::now();
        let mut clock = TrackingClock::load_at(&conn, 10, wall(23, 59, 58)).unwrap();
        clock
            .snapshot_at(&conn, true, tick, wall(23, 59, 58))
            .unwrap();
        let next_day = Local.with_ymd_and_hms(2026, 10, 2, 0, 0, 3).unwrap();
        let sample = clock
            .snapshot_at(&conn, false, tick + Duration::from_secs(5), next_day)
            .unwrap();
        assert_eq!(sample.total_seconds, 3);
        assert_eq!(
            conn.query_row::<u64, _, _>(
                "SELECT elapsed_milliseconds FROM tracking_daily_time WHERE date='2026-10-01'",
                [],
                |row| row.get(0)
            )
            .unwrap(),
            12000
        );
        let later = Local.with_ymd_and_hms(2026, 10, 3, 0, 0, 3).unwrap();
        assert_eq!(
            clock
                .snapshot_at(&conn, false, tick + Duration::from_secs(86405), later)
                .unwrap()
                .total_seconds,
            0
        );
    }

    #[test]
    fn uses_monotonic_time_when_wall_clock_changes() {
        let conn = Connection::open_in_memory().unwrap();
        let tick = Instant::now();
        let mut clock = TrackingClock::load_at(&conn, 0, wall(12, 0, 0)).unwrap();
        clock
            .snapshot_at(&conn, true, tick, wall(12, 0, 0))
            .unwrap();
        assert_eq!(
            clock
                .snapshot_at(&conn, true, tick + Duration::from_secs(5), wall(13, 0, 0))
                .unwrap()
                .total_seconds,
            5
        );
        assert_eq!(
            clock
                .snapshot_at(&conn, false, tick + Duration::from_secs(8), wall(11, 0, 0))
                .unwrap()
                .total_seconds,
            8
        );
    }

    #[test]
    fn failed_checkpoint_can_retry_without_double_counting() {
        let conn = Connection::open_in_memory().unwrap();
        let tick = Instant::now();
        let mut clock = TrackingClock::load_at(&conn, 0, wall(12, 0, 0)).unwrap();
        clock
            .snapshot_at(&conn, true, tick, wall(12, 0, 0))
            .unwrap();
        conn.execute_batch("CREATE TRIGGER fail_clock BEFORE UPDATE ON tracking_daily_time BEGIN SELECT RAISE(ABORT, 'busy fixture'); END;").unwrap();
        assert!(clock
            .snapshot_at(&conn, true, tick + Duration::from_secs(5), wall(12, 0, 5))
            .is_err());
        conn.execute_batch("DROP TRIGGER fail_clock;").unwrap();
        assert_eq!(
            clock
                .snapshot_at(&conn, false, tick + Duration::from_secs(8), wall(12, 0, 8))
                .unwrap()
                .total_seconds,
            8
        );
    }
}
