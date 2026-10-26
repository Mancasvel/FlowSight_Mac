//! Observable screen progression, independent of input frequency or focus grades.
//! Frames and context digests stay in memory; only bounded evidence is persisted.

use image::DynamicImage;
use rusqlite::Connection;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::time::{Duration, Instant};

pub(crate) const STAGNATION_THRESHOLD_SECONDS: u64 = 20 * 60;
const GRID_WIDTH: u32 = 48;
const GRID_HEIGHT: u32 = 27;

#[derive(Debug, Clone)]
pub(crate) struct VisualFingerprint {
    dimensions: (u32, u32),
    cells: Vec<[u8; 3]>,
}

impl VisualFingerprint {
    pub(crate) fn from_image(frame: &DynamicImage) -> Self {
        // Averaging suppresses a blinking caret, cursor, clock or tiny spinner.
        let grid = frame
            .resize_exact(
                GRID_WIDTH,
                GRID_HEIGHT,
                image::imageops::FilterType::Triangle,
            )
            .to_rgb8();
        Self {
            dimensions: (frame.width(), frame.height()),
            cells: grid.pixels().map(|p| p.0).collect(),
        }
    }

    fn materially_differs(&self, other: &Self) -> bool {
        if self.dimensions != other.dimensions || self.cells.len() != other.cells.len() {
            return true;
        }
        let changed = self
            .cells
            .iter()
            .zip(&other.cells)
            .filter(|(a, b)| {
                a.iter()
                    .zip(b.iter())
                    .map(|(x, y)| x.abs_diff(*y) as u32)
                    .sum::<u32>()
                    >= 36
            })
            .count();
        // Compare with the last meaningful frame, so slow cumulative changes
        // eventually count. Tiny isolated UI animations do not reset the timer.
        changed * 100 >= self.cells.len() * 3
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Engagement {
    Work,
    Reading,
    LiveMeeting,
    PlayingMedia,
    Waiting,
    Unknown,
}

impl Engagement {
    pub(crate) fn from_analysis(raw: &str) -> Self {
        // A product/process name alone cannot establish an ongoing call or reading.
        let fields: Vec<_> = raw
            .lines()
            .filter_map(|line| {
                let (label, value) = line.split_once(':')?;
                label
                    .trim()
                    .eq_ignore_ascii_case("ENGAGEMENT")
                    .then(|| value.trim().to_ascii_lowercase())
            })
            .collect();
        if fields.len() != 1 {
            return Self::Unknown;
        }
        match fields[0].as_str() {
            "work" => Self::Work,
            "reading" => Self::Reading,
            "live meeting" => Self::LiveMeeting,
            "playing media" => Self::PlayingMedia,
            "waiting" => Self::Waiting,
            _ => Self::Unknown,
        }
    }

    fn passive(self) -> bool {
        matches!(self, Self::Reading | Self::LiveMeeting | Self::PlayingMedia)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum ProgressState {
    Baseline,
    VisibleChange,
    Stable,
    PassiveContext,
    PossibleStagnation,
    Unknown,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct ProgressObservation {
    pub state: ProgressState,
    pub observed_seconds: u64,
    pub unchanged_seconds: u64,
    /// Only the part of this sample after the strict >20 minute boundary.
    pub over_threshold_seconds: u64,
}

struct Baseline {
    frame: VisualFingerprint,
    context: [u8; 32],
    run_started: Option<Instant>,
    unchanged_since: Instant,
    last_seen: Instant,
}

#[derive(Default)]
pub(crate) struct ProgressTracker {
    baseline: Option<Baseline>,
}

impl ProgressTracker {
    pub(crate) fn reset(&mut self) {
        self.baseline = None;
    }

    #[allow(clippy::too_many_arguments)] // all values belong to this single captured observation
    pub(crate) fn observe(
        &mut self,
        now: Instant,
        run_started: Option<Instant>,
        max_gap: Duration,
        context_parts: &[Option<&str>],
        frame: VisualFingerprint,
        engagement: Engagement,
        observed_seconds: u64,
    ) -> ProgressObservation {
        let mut digest = Sha256::new();
        for part in context_parts {
            let value = part.unwrap_or_default().as_bytes();
            digest.update((value.len() as u64).to_le_bytes());
            digest.update(value);
        }
        let context: [u8; 32] = digest.finalize().into();
        let continuous = self.baseline.as_ref().is_some_and(|b| {
            b.context == context
                && b.run_started == run_started
                && now.saturating_duration_since(b.last_seen) <= max_gap
        });
        if !continuous {
            self.baseline = Some(Baseline {
                frame,
                context,
                run_started,
                unchanged_since: now,
                last_seen: now,
            });
            return ProgressObservation {
                state: ProgressState::Baseline,
                observed_seconds,
                unchanged_seconds: 0,
                over_threshold_seconds: 0,
            };
        }
        let baseline = self.baseline.as_mut().unwrap();
        baseline.last_seen = now;
        if baseline.frame.materially_differs(&frame) {
            baseline.frame = frame;
            baseline.unchanged_since = now;
            return ProgressObservation {
                state: ProgressState::VisibleChange,
                observed_seconds,
                unchanged_seconds: 0,
                over_threshold_seconds: 0,
            };
        }
        let unchanged_seconds = now
            .saturating_duration_since(baseline.unchanged_since)
            .as_secs();
        let over_threshold_seconds =
            observed_seconds.min(unchanged_seconds.saturating_sub(STAGNATION_THRESHOLD_SECONDS));
        let state = if engagement.passive() {
            // Passive context is evidence of a plausible activity, not proof of attention.
            ProgressState::PassiveContext
        } else if engagement == Engagement::Unknown {
            ProgressState::Unknown
        } else if unchanged_seconds > STAGNATION_THRESHOLD_SECONDS {
            ProgressState::PossibleStagnation
        } else {
            ProgressState::Stable
        };
        ProgressObservation {
            state,
            observed_seconds,
            unchanged_seconds,
            over_threshold_seconds,
        }
    }
}

pub(crate) fn ensure_schema(conn: &Connection) -> rusqlite::Result<()> {
    if !has_observations(conn)? {
        conn.execute(
            "ALTER TABLE reports ADD COLUMN progress_observation TEXT",
            [],
        )?;
    }
    Ok(())
}

fn has_observations(conn: &Connection) -> rusqlite::Result<bool> {
    let mut stmt = conn.prepare("PRAGMA table_info(reports)")?;
    let columns = stmt.query_map([], |row| row.get::<_, String>(1))?;
    for column in columns {
        if column? == "progress_observation" {
            return Ok(true);
        }
    }
    Ok(false)
}

#[derive(Default, Debug, Serialize)]
pub(crate) struct ProgressSummary {
    pub threshold_seconds: u64,
    pub observed_seconds: u64,
    pub visible_change_observations: u64,
    pub longest_unchanged_seconds: u64,
    pub possible_stagnation_seconds: u64,
    pub passive_unchanged_seconds: u64,
    pub uncertain_unchanged_seconds: u64,
}

pub(crate) fn summarize_from_db(
    conn: &Connection,
    start_date: &str,
    end_date: &str,
) -> Result<ProgressSummary, String> {
    let mut summary = ProgressSummary {
        threshold_seconds: STAGNATION_THRESHOLD_SECONDS,
        ..Default::default()
    };
    // Legacy records lack temporal evidence; never reconstruct it from repeated descriptions.
    if !has_observations(conn).map_err(|e| e.to_string())? {
        return Ok(summary);
    }
    let window = crate::focus_semantics::LocalDateWindow::parse(start_date, end_date)?;
    let mut stmt = conn
        .prepare(
            "SELECT datetime(created_at, 'localtime'), duration_seconds, progress_observation
         FROM reports WHERE progress_observation IS NOT NULL AND capture_source = 'periodic_vision'
         AND date(created_at, 'localtime') >= ?1
         AND date(created_at, 'localtime') <= date(?2, '+1 day')",
        )
        .map_err(|e| e.to_string())?;
    let rows = stmt
        .query_map([start_date, end_date], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, i64>(1)?,
                row.get::<_, String>(2)?,
            ))
        })
        .map_err(|e| e.to_string())?;
    for row in rows {
        let (timestamp, duration, raw) = row.map_err(|e| e.to_string())?;
        let Ok(evidence) = serde_json::from_str::<ProgressObservation>(&raw) else {
            continue;
        };
        let duration = duration.max(0) as u64;
        let counted = window
            .slices_for_observation(&timestamp, duration as i64)
            .iter()
            .map(|s| s.duration_seconds as u64)
            .sum::<u64>();
        if counted == 0 {
            continue;
        }
        summary.observed_seconds += counted.min(evidence.observed_seconds);
        let unchanged_in_window = window
            .slices_for_observation(
                &timestamp,
                evidence.unchanged_seconds.min(366 * 24 * 60 * 60) as i64,
            )
            .iter()
            .map(|s| s.duration_seconds as u64)
            .sum::<u64>();
        summary.longest_unchanged_seconds =
            summary.longest_unchanged_seconds.max(unchanged_in_window);
        if evidence.state == ProgressState::VisibleChange
            && window.contains_local_timestamp(&timestamp)
        {
            summary.visible_change_observations += 1;
        }
        // The excess is the suffix of this end-timestamped interval. Clip that
        // suffix separately so midnight never charges the pre-threshold minutes.
        let excess = evidence.over_threshold_seconds.min(duration).min(
            evidence
                .unchanged_seconds
                .saturating_sub(STAGNATION_THRESHOLD_SECONDS),
        );
        let clipped_excess = window
            .slices_for_observation(&timestamp, excess as i64)
            .iter()
            .map(|s| s.duration_seconds as u64)
            .sum::<u64>();
        match evidence.state {
            ProgressState::PossibleStagnation => {
                summary.possible_stagnation_seconds += clipped_excess
            }
            ProgressState::PassiveContext => summary.passive_unchanged_seconds += clipped_excess,
            ProgressState::Unknown => summary.uncertain_unchanged_seconds += clipped_excess,
            _ => {}
        }
    }
    Ok(summary)
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;

    fn frame(shade: u8) -> VisualFingerprint {
        VisualFingerprint::from_image(&DynamicImage::ImageRgb8(image::RgbImage::from_pixel(
            480,
            270,
            image::Rgb([shade; 3]),
        )))
    }

    fn sample(
        tracker: &mut ProgressTracker,
        start: Instant,
        seconds: u64,
        engagement: Engagement,
    ) -> ProgressObservation {
        tracker.observe(
            start + Duration::from_secs(seconds),
            Some(start),
            Duration::from_secs(120),
            &[Some("app"), Some("document"), Some("task")],
            frame(240),
            engagement,
            60,
        )
    }

    fn static_streak(engagement: Engagement) -> ProgressObservation {
        let start = Instant::now();
        let mut tracker = ProgressTracker::default();
        for minutes in 0..=20 {
            let observation = sample(&mut tracker, start, minutes * 60, engagement);
            assert_ne!(observation.state, ProgressState::PossibleStagnation);
            assert_eq!(observation.over_threshold_seconds, 0);
        }
        sample(&mut tracker, start, 21 * 60, engagement)
    }

    #[test]
    fn static_work_counts_only_time_strictly_after_twenty_minutes() {
        let observation = static_streak(Engagement::Work);
        assert_eq!(observation.state, ProgressState::PossibleStagnation);
        assert_eq!(observation.unchanged_seconds, 1260);
        assert_eq!(observation.over_threshold_seconds, 60);
    }

    #[test]
    fn threshold_crossing_counts_only_the_suffix_of_the_sample() {
        let start = Instant::now();
        let mut tracker = ProgressTracker::default();
        for minute in 0..20 {
            sample(&mut tracker, start, minute * 60, Engagement::Work);
        }
        let observation = sample(&mut tracker, start, 1205, Engagement::Work);
        assert_eq!(observation.state, ProgressState::PossibleStagnation);
        assert_eq!(observation.over_threshold_seconds, 5);
    }

    #[test]
    fn reading_meetings_and_playback_can_continue_without_inputs_or_visual_changes() {
        for engagement in [
            Engagement::Reading,
            Engagement::LiveMeeting,
            Engagement::PlayingMedia,
        ] {
            let observation = static_streak(engagement);
            assert_eq!(observation.state, ProgressState::PassiveContext);
            assert_eq!(observation.over_threshold_seconds, 60);
        }
    }

    #[test]
    fn failed_or_unknown_semantics_never_claim_absence_or_stagnation() {
        let observation = static_streak(Engagement::Unknown);
        assert_eq!(observation.state, ProgressState::Unknown);
        assert_eq!(observation.over_threshold_seconds, 60);
        assert_eq!(
            Engagement::from_analysis("APP: Zoom\nCATEGORY: Meeting"),
            Engagement::Unknown
        );
        assert_eq!(
            Engagement::from_analysis("ENGAGEMENT: reading\nENGAGEMENT: work"),
            Engagement::Unknown
        );
        assert_eq!(
            Engagement::from_analysis("ENGAGEMENT: maybe reading"),
            Engagement::Unknown
        );
        assert_eq!(
            Engagement::from_analysis("ENGAGEMENT: live meeting\nCATEGORY: Meeting"),
            Engagement::LiveMeeting
        );
    }

    #[test]
    fn cursor_blink_and_small_visual_noise_do_not_reset_static_time() {
        let white = image::Rgb([240; 3]);
        let mut img = image::RgbImage::from_pixel(480, 270, white);
        let reference = VisualFingerprint::from_image(&DynamicImage::ImageRgb8(img.clone()));
        for x in 100..103 {
            for y in 80..97 {
                img.put_pixel(x, y, image::Rgb([0; 3]));
            }
        }
        let cursor = VisualFingerprint::from_image(&DynamicImage::ImageRgb8(img));
        assert!(!reference.materially_differs(&cursor));
    }

    #[test]
    fn substantial_content_changes_reset_the_streak_without_input_events() {
        let start = Instant::now();
        let mut tracker = ProgressTracker::default();
        for minute in 0..=20 {
            sample(&mut tracker, start, minute * 60, Engagement::Work);
        }
        let changed = tracker.observe(
            start + Duration::from_secs(1260),
            Some(start),
            Duration::from_secs(120),
            &[Some("app"), Some("document"), Some("task")],
            frame(40),
            Engagement::Work,
            60,
        );
        assert_eq!(changed.state, ProgressState::VisibleChange);
        assert_eq!(changed.unchanged_seconds, 0);
        assert_eq!(changed.over_threshold_seconds, 0);
    }

    #[test]
    fn gradual_change_is_compared_to_the_last_meaningful_frame() {
        let start = Instant::now();
        let mut tracker = ProgressTracker::default();
        for step in 0..=3 {
            let observation = tracker.observe(
                start + Duration::from_secs(step * 60),
                Some(start),
                Duration::from_secs(120),
                &[Some("app")],
                frame(240 - step as u8 * 5),
                Engagement::Work,
                60,
            );
            let expected = match step {
                0 => ProgressState::Baseline,
                3 => ProgressState::VisibleChange,
                _ => ProgressState::Stable,
            };
            assert_eq!(observation.state, expected);
        }
    }

    #[test]
    fn changing_task_document_account_or_monitoring_run_resets_comparison() {
        let start = Instant::now();
        for context in [
            vec![Some("other app"), Some("document"), Some("task")],
            vec![Some("app"), Some("other document"), Some("task")],
            vec![Some("app"), Some("document"), Some("other task")],
            vec![
                Some("app"),
                Some("document"),
                Some("task"),
                Some("other account"),
            ],
        ] {
            let mut tracker = ProgressTracker::default();
            sample(&mut tracker, start, 0, Engagement::Work);
            let observation = tracker.observe(
                start + Duration::from_secs(60),
                Some(start),
                Duration::from_secs(120),
                &context,
                frame(240),
                Engagement::Work,
                60,
            );
            assert_eq!(observation.state, ProgressState::Baseline);
        }
        let mut tracker = ProgressTracker::default();
        sample(&mut tracker, start, 0, Engagement::Work);
        let observation = tracker.observe(
            start + Duration::from_secs(60),
            Some(start + Duration::from_secs(30)),
            Duration::from_secs(120),
            &[Some("app"), Some("document"), Some("task")],
            frame(240),
            Engagement::Work,
            30,
        );
        assert_eq!(observation.state, ProgressState::Baseline);
    }

    #[test]
    fn capture_failure_sleep_and_explicit_reset_break_continuity() {
        let start = Instant::now();
        let mut tracker = ProgressTracker::default();
        sample(&mut tracker, start, 0, Engagement::Work);
        assert_eq!(
            sample(&mut tracker, start, 1260, Engagement::Work).state,
            ProgressState::Baseline
        );
        tracker.reset();
        assert_eq!(
            sample(&mut tracker, start, 1320, Engagement::Work).state,
            ProgressState::Baseline
        );
    }

    fn db() -> Connection {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch(
            "CREATE TABLE reports (
            id INTEGER PRIMARY KEY, created_at TEXT, duration_seconds INTEGER, capture_source TEXT
        )",
        )
        .unwrap();
        conn
    }

    fn insert(conn: &Connection, local_time: &str, duration: u64, evidence: &ProgressObservation) {
        let local = chrono::NaiveDateTime::parse_from_str(local_time, "%Y-%m-%d %H:%M:%S").unwrap();
        let utc = chrono::Local
            .from_local_datetime(&local)
            .single()
            .unwrap()
            .with_timezone(&chrono::Utc)
            .format("%Y-%m-%d %H:%M:%S")
            .to_string();
        conn.execute("INSERT INTO reports (created_at, duration_seconds, capture_source, progress_observation)
            VALUES (?1, ?2, 'periodic_vision', ?3)",
            rusqlite::params![utc, duration, serde_json::to_string(evidence).unwrap()],
        ).unwrap();
    }

    #[test]
    fn legacy_history_remains_unmeasured_and_migration_is_idempotent() {
        let conn = db();
        assert_eq!(
            summarize_from_db(&conn, "2026-10-22", "2026-10-22")
                .unwrap()
                .observed_seconds,
            0
        );
        ensure_schema(&conn).unwrap();
        ensure_schema(&conn).unwrap();
        assert_eq!(
            summarize_from_db(&conn, "2026-10-22", "2026-10-22")
                .unwrap()
                .possible_stagnation_seconds,
            0
        );
    }

    #[test]
    fn reports_measure_passive_static_time_separately_without_double_counting() {
        let conn = db();
        ensure_schema(&conn).unwrap();
        for (minute, state) in [
            (21, ProgressState::PossibleStagnation),
            (22, ProgressState::PassiveContext),
            (23, ProgressState::Unknown),
            (24, ProgressState::VisibleChange),
        ] {
            insert(
                &conn,
                &format!("2026-10-22 12:{minute}:00"),
                60,
                &ProgressObservation {
                    state,
                    observed_seconds: 60,
                    unchanged_seconds: 1260,
                    over_threshold_seconds: 60,
                },
            );
        }
        let summary = summarize_from_db(&conn, "2026-10-22", "2026-10-22").unwrap();
        assert_eq!(summary.observed_seconds, 240);
        assert_eq!(summary.possible_stagnation_seconds, 60);
        assert_eq!(summary.passive_unchanged_seconds, 60);
        assert_eq!(summary.uncertain_unchanged_seconds, 60);
        assert_eq!(summary.visible_change_observations, 1);
        // Evidence lives in its report row and disappears with existing privacy deletion.
        conn.execute("DELETE FROM reports", []).unwrap();
        assert_eq!(
            summarize_from_db(&conn, "2026-10-22", "2026-10-22")
                .unwrap()
                .observed_seconds,
            0
        );
    }

    #[test]
    fn midnight_clips_only_the_over_threshold_suffix_into_the_correct_day() {
        let conn = db();
        ensure_schema(&conn).unwrap();
        insert(
            &conn,
            "2026-10-22 00:01:00",
            120,
            &ProgressObservation {
                state: ProgressState::PossibleStagnation,
                observed_seconds: 120,
                unchanged_seconds: 1245,
                over_threshold_seconds: 45,
            },
        );
        let before = summarize_from_db(&conn, "2026-10-21", "2026-10-21").unwrap();
        let after = summarize_from_db(&conn, "2026-10-22", "2026-10-22").unwrap();
        assert_eq!(before.observed_seconds, 60);
        assert_eq!(after.observed_seconds, 60);
        assert_eq!(before.possible_stagnation_seconds, 0);
        assert_eq!(after.possible_stagnation_seconds, 45);
        assert_eq!(before.longest_unchanged_seconds, 1185);
        assert_eq!(after.longest_unchanged_seconds, 60);
    }

    #[test]
    fn zero_duration_annotations_and_malformed_evidence_are_excluded() {
        let conn = db();
        ensure_schema(&conn).unwrap();
        insert(
            &conn,
            "2026-10-22 12:00:00",
            0,
            &ProgressObservation {
                state: ProgressState::VisibleChange,
                observed_seconds: 60,
                unchanged_seconds: 0,
                over_threshold_seconds: 0,
            },
        );
        conn.execute("UPDATE reports SET capture_source = 'action_review'", [])
            .unwrap();
        conn.execute("INSERT INTO reports VALUES (2, '2026-10-22 12:00:00', 60, 'periodic_vision', '{broken')", []).unwrap();
        assert_eq!(
            summarize_from_db(&conn, "2026-10-22", "2026-10-22")
                .unwrap()
                .observed_seconds,
            0
        );
    }
}
