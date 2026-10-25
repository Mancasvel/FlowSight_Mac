//! Level 1 of the privacy-first pipeline: at the configured capture interval, while monitoring
//! is ON, produce up to two independent `reports` rows:
//!
//!   (a) If any UIA/foreground events accumulated in the ring buffer, a
//!       privacy-reviewed text summary via the local llama-server model.
//!       Duration is 0 so this is a narrative signal, not a second minute of
//!       tracked time (action-triggered captures already use the same trick).
//!   (b) Always: a screenshot + local vision pass
//!       (`agent::capture_and_analyze_screen`), persisted with duration derived
//!       from monotonic elapsed monitoring time. Paused time is reset
//!       and never charged to the first observation after resume.
//!
//! Screenshots are handled here *and* by the separate, action-triggered
//! mechanism in `action_capture` (WindowOpened/WindowClosed, ~15s cooldown).
//! This cycle never infers idle / "away from keyboard": empty intervals still
//! get a vision snapshot of whatever is on screen, not a synthetic Idle row.
//!
//! Mirrors the always-on background thread pattern already used by
//! `sync::start_sync_thread` for the 10-minute rollup.

use super::{
    ActionEvent, SharedCaptureInterval, SharedFlag, SharedMonitoringStart, SharedRing, TaskContext,
};
use std::path::{Path, PathBuf};
use std::sync::atomic::Ordering;
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;
use tauri::Emitter;

/// Action-log reviews are complementary signals; time is owned by the vision snapshot.
const ACTION_REVIEW_DURATION_SECS: u64 = 0;
const MAX_EVENTS_IN_SUMMARY: usize = 40;

pub fn spawn(
    app_handle: tauri::AppHandle,
    db_path: PathBuf,
    ring: SharedRing,
    running: SharedFlag,
    monitoring_started: SharedMonitoringStart,
    capture_interval_ms: SharedCaptureInterval,
    task_ctx: Arc<Mutex<TaskContext>>,
) {
    thread::spawn(move || {
        let mut last_snapshot_started: Option<std::time::Instant> = None;
        let mut progress_tracker = crate::task_progress::ProgressTracker::default();
        loop {
            let interval_ms = capture_interval_ms
                .load(Ordering::Relaxed)
                .clamp(5_000, 300_000);
            thread::sleep(Duration::from_millis(interval_ms));

            if !running.load(Ordering::Relaxed) {
                // Not monitoring: drop anything that slipped in and wait for the
                // next cycle rather than letting the buffer grow unbounded.
                ring.lock().unwrap().clear();
                last_snapshot_started = None;
                progress_tracker.reset();
                continue;
            }

            let foreground = crate::context::get_system_context();
            if crate::privacy::application_is_excluded(&db_path, foreground.app_name.as_deref()) {
                ring.lock().unwrap().clear();
                last_snapshot_started = Some(std::time::Instant::now());
                progress_tracker.reset();
                continue;
            }

            let events: Vec<ActionEvent> = {
                let mut buf = ring.lock().unwrap();
                buf.drain(..).collect()
            };

            let (user_task, jira_ticket) = {
                let ctx = task_ctx.lock().unwrap();
                (ctx.user_task.clone(), ctx.jira_ticket.clone())
            };
            let task_label = jira_ticket
                .clone()
                .or_else(|| user_task.clone())
                .unwrap_or_else(|| "General".to_string());
            let theme_hint = jira_ticket
                .clone()
                .or_else(|| user_task.clone())
                .filter(|theme| !theme.trim().is_empty() && !theme.eq_ignore_ascii_case("general"));

            // A document/app switch observed between sampled frames breaks a
            // continuous static interval, even when the next frame looks alike.
            // Mere UI control focus events are not evidence of task progression.
            if events.iter().any(|event| {
                matches!(
                    event,
                    ActionEvent::ForegroundChanged { .. }
                        | ActionEvent::UiaWindowOpened { .. }
                        | ActionEvent::UiaWindowClosed { .. }
                )
            }) {
                progress_tracker.reset();
            }

            // (a) Text review of accumulated UIA/foreground actions, if any.
            // Skip empty intervals rather than inventing an idle/no-activity signal.
            if !events.is_empty() {
                let include_titles = crate::privacy::store_window_titles(&db_path);
                let (description, category) = review_cycle(&events, &task_label, include_titles);
                persist_and_emit(
                    &app_handle,
                    &db_path,
                    &description,
                    &category,
                    jira_ticket.clone(),
                    ACTION_REVIEW_DURATION_SECS,
                    "action_review",
                    theme_hint.clone(),
                    None,
                    None,
                    None,
                );
            }

            if !running.load(Ordering::Relaxed) {
                continue;
            }

            // (b) Always take a screenshot + vision pass for this interval.
            let capture_started = std::time::Instant::now();
            let run_started = *monitoring_started.lock().unwrap();
            let observed_seconds = observation_duration_seconds(
                last_snapshot_started,
                run_started,
                capture_started,
                (interval_ms / 1_000).saturating_mul(2).max(1),
            );
            last_snapshot_started = Some(capture_started);
            match crate::agent::capture_and_analyze_screen(&db_path, &task_label) {
                Ok(capture) => {
                    // A stop/resume or privacy exclusion during inference must
                    // not extend a static streak or save a late observation.
                    if !running.load(Ordering::Relaxed)
                        || *monitoring_started.lock().unwrap() != run_started
                        || crate::privacy::application_is_excluded(
                            &db_path,
                            capture.window.app_name.as_deref(),
                        )
                    {
                        progress_tracker.reset();
                        continue;
                    }
                    let progress = progress_tracker.observe(
                        capture_started,
                        run_started,
                        // A normal local inference can take longer than a 5s
                        // configured capture interval. Allow that cadence without
                        // treating every frame as a monitoring interruption.
                        Duration::from_secs((interval_ms / 1_000).saturating_mul(2).max(120)),
                        &[
                            capture.window.app_name.as_deref(),
                            capture.window.window_title.as_deref(),
                            user_task.as_deref(),
                            jira_ticket.as_deref(),
                            capture.progress_owner_id.as_deref(),
                        ],
                        capture.fingerprint,
                        capture.engagement,
                        observed_seconds,
                    );
                    persist_and_emit(
                        &app_handle,
                        &db_path,
                        &capture.description,
                        &capture.category,
                        jira_ticket,
                        observed_seconds,
                        "periodic_vision",
                        theme_hint,
                        Some(capture.window),
                        Some(capture.observed_at_utc),
                        Some(&progress),
                    );
                }
                Err(e) => {
                    progress_tracker.reset();
                    log::warn!("[Telemetry] Periodic vision snapshot failed: {e}");
                }
            }
        }
    });
}

fn observation_duration_seconds(
    previous_capture: Option<std::time::Instant>,
    monitoring_started: Option<std::time::Instant>,
    now: std::time::Instant,
    max_seconds: u64,
) -> u64 {
    let start = match (previous_capture, monitoring_started) {
        (Some(previous), Some(run_start)) => previous.max(run_start),
        (Some(previous), None) => previous,
        (None, Some(run_start)) => run_start,
        (None, None) => return 1,
    };
    now.saturating_duration_since(start)
        .as_secs()
        .clamp(1, max_seconds)
}

#[allow(clippy::too_many_arguments)] // explicit bridge from capture output to persistence/event payload
fn persist_and_emit(
    app_handle: &tauri::AppHandle,
    db_path: &Path,
    description: &str,
    category: &str,
    jira_ticket: Option<String>,
    duration_seconds: u64,
    capture_source: &str,
    theme_hint: Option<String>,
    captured_context: Option<crate::agent::CapturedWindowContext>,
    observed_at_utc: Option<String>,
    progress: Option<&crate::task_progress::ProgressObservation>,
) {
    let category = crate::agent_pure::resolve_persisted_category(category);
    let alert_context = captured_context.clone();
    match crate::agent::insert_report(
        db_path,
        description,
        &category,
        jira_ticket.clone(),
        duration_seconds,
        capture_source,
        theme_hint,
        captured_context,
        observed_at_utc,
        progress,
    ) {
        Some(id) => {
            crate::focus_alerts::review_browsing_report(
                app_handle,
                db_path,
                &category,
                duration_seconds,
                description,
                alert_context
                    .as_ref()
                    .and_then(|context| context.app_name.as_deref()),
            );
            let _ = app_handle.emit(
                "activity-report",
                serde_json::json!({
                    "id": id,
                    "description": description,
                    "category": category,
                    "jiraTicket": jira_ticket,
                    "progress": progress,
                }),
            );
        }
        None => log::warn!("[Telemetry] Failed to persist aggregated report to local DB"),
    }
}

fn review_cycle(
    events: &[ActionEvent],
    task_label: &str,
    include_titles: bool,
) -> (String, String) {
    let summary = summarize_events(events, include_titles);
    match crate::agent::review_actions_with_local_model(&summary, task_label) {
        Ok(raw) => crate::agent_pure::parse_analysis(&raw),
        Err(e) => {
            log::warn!("[Telemetry] Action-log review failed: {e}");
            (
                format!("Automatic action-log review failed: {e}"),
                "General".to_string(),
            )
        }
    }
}

fn summarize_events(events: &[ActionEvent], include_titles: bool) -> String {
    events
        .iter()
        .take(MAX_EVENTS_IN_SUMMARY)
        .map(|e| match e {
            ActionEvent::ForegroundChanged {
                app_name,
                window_title,
                ..
            } => {
                if include_titles {
                    format!(
                        "- switched focus to app '{}' (window: '{}')",
                        app_name,
                        truncate(window_title, 80)
                    )
                } else {
                    format!("- switched focus to app '{}'", app_name)
                }
            }
            ActionEvent::UiaFocusChanged {
                control_name,
                control_type,
                ..
            } => format!(
                "- focused a {} control{}",
                control_type.as_deref().unwrap_or("UI"),
                if include_titles {
                    control_name.as_deref()
                } else {
                    None
                }
                .map(|n| format!(" named '{}'", truncate(n, 60)))
                .unwrap_or_default()
            ),
            ActionEvent::UiaWindowOpened { name, .. } => {
                format!(
                    "- opened a window{}",
                    if include_titles {
                        name.as_deref()
                    } else {
                        None
                    }
                    .map(|n| format!(" '{}'", truncate(n, 60)))
                    .unwrap_or_default()
                )
            }
            ActionEvent::UiaWindowClosed { name, .. } => {
                format!(
                    "- closed a window{}",
                    if include_titles {
                        name.as_deref()
                    } else {
                        None
                    }
                    .map(|n| format!(" '{}'", truncate(n, 60)))
                    .unwrap_or_default()
                )
            }
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn truncate(s: &str, max_len: usize) -> String {
    if s.chars().count() <= max_len {
        s.to_string()
    } else {
        let truncated: String = s.chars().take(max_len).collect();
        format!("{truncated}…")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn first_tick_uses_only_elapsed_monitored_time() {
        let now = std::time::Instant::now();
        let started = now - Duration::from_secs(17);
        assert_eq!(
            observation_duration_seconds(None, Some(started), now, 120),
            17
        );
    }

    #[test]
    fn resume_discards_time_from_previous_monitoring_run() {
        let now = std::time::Instant::now();
        let previous_run = now - Duration::from_secs(600);
        let resumed = now - Duration::from_secs(12);
        assert_eq!(
            observation_duration_seconds(Some(previous_run), Some(resumed), now, 120),
            12
        );
    }

    #[test]
    fn delayed_tick_uses_real_elapsed_time_within_the_safety_ceiling() {
        let now = std::time::Instant::now();
        let previous = now - Duration::from_secs(83);
        assert_eq!(
            observation_duration_seconds(Some(previous), None, now, 120),
            83
        );
    }

    #[test]
    fn implausibly_long_gap_is_capped_instead_of_inflating_tracked_time() {
        let now = std::time::Instant::now();
        let previous = now - Duration::from_secs(3_600);
        assert_eq!(
            observation_duration_seconds(Some(previous), None, now, 120),
            120
        );
    }
}
