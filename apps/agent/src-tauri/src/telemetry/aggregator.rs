//! Level 1 of the privacy-first pipeline: every 60 seconds, while monitoring
//! is ON, produce up to two independent `reports` rows:
//!
//!   (a) If any UIA/foreground events accumulated in the ring buffer, a
//!       privacy-reviewed text summary via the local llama-server model.
//!       Duration is 0 so this is a narrative signal, not a second minute of
//!       tracked time (action-triggered captures already use the same trick).
//!   (b) Always: a screenshot + local vision pass
//!       (`agent::capture_and_analyze_screen`), persisted with a 60s duration
//!       so time accounting matches the old periodic-snapshot architecture.
//!
//! Screenshots are handled here *and* by the separate, action-triggered
//! mechanism in `action_capture` (WindowOpened/WindowClosed, ~15s cooldown).
//! This cycle never infers idle / "away from keyboard": empty minutes still
//! get a vision snapshot of whatever is on screen, not a synthetic Idle row.
//!
//! Mirrors the always-on background thread pattern already used by
//! `sync::start_sync_thread` for the 10-minute rollup.

use super::{ActionEvent, SharedFlag, SharedRing, TaskContext};
use std::path::PathBuf;
use std::sync::atomic::Ordering;
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;
use tauri::Emitter;

const CYCLE: Duration = Duration::from_secs(60);
const VISION_REPORT_DURATION_SECS: u64 = 60;
/// Action-log reviews are complementary signals; time is owned by the vision snapshot.
const ACTION_REVIEW_DURATION_SECS: u64 = 0;
const MAX_EVENTS_IN_SUMMARY: usize = 40;

pub fn spawn(
    app_handle: tauri::AppHandle,
    db_path: PathBuf,
    ring: SharedRing,
    running: SharedFlag,
    task_ctx: Arc<Mutex<TaskContext>>,
) {
    thread::spawn(move || loop {
        thread::sleep(CYCLE);

        if !running.load(Ordering::Relaxed) {
            // Not monitoring: drop anything that slipped in and wait for the
            // next cycle rather than letting the buffer grow unbounded.
            ring.lock().unwrap().clear();
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
        let task_label = jira_ticket.clone().or_else(|| user_task.clone()).unwrap_or_else(|| "General".to_string());

        // (a) Text review of accumulated UIA/foreground actions, if any.
        // Skip empty minutes rather than inventing an idle/no-activity signal.
        if !events.is_empty() {
            let (description, category) = review_cycle(&events, &task_label);
            persist_and_emit(
                &app_handle,
                &db_path,
                &description,
                &category,
                jira_ticket.clone(),
                ACTION_REVIEW_DURATION_SECS,
            );
        }

        if !running.load(Ordering::Relaxed) {
            continue;
        }

        // (b) Always take a screenshot + vision pass for this minute.
        match crate::agent::capture_and_analyze_screen(&task_label) {
            Ok((description, category)) => persist_and_emit(
                &app_handle,
                &db_path,
                &description,
                &category,
                jira_ticket,
                VISION_REPORT_DURATION_SECS,
            ),
            Err(e) => log::warn!("[Telemetry] Periodic vision snapshot failed: {e}"),
        }
    });
}

fn persist_and_emit(
    app_handle: &tauri::AppHandle,
    db_path: &PathBuf,
    description: &str,
    category: &str,
    jira_ticket: Option<String>,
    duration_seconds: u64,
) {
    let category = crate::agent_pure::resolve_persisted_category(category);
    match crate::agent::insert_report(db_path, description, &category, jira_ticket.clone(), duration_seconds) {
        Some(id) => {
            let _ = app_handle.emit(
                "activity-report",
                serde_json::json!({
                    "id": id,
                    "description": description,
                    "category": category,
                    "jiraTicket": jira_ticket,
                }),
            );
        }
        None => log::warn!("[Telemetry] Failed to persist aggregated report to local DB"),
    }
}

fn review_cycle(events: &[ActionEvent], task_label: &str) -> (String, String) {
    let summary = summarize_events(events);
    match crate::agent::review_actions_with_local_model(&summary, task_label) {
        Ok(raw) => crate::agent_pure::parse_analysis(&raw),
        Err(e) => {
            log::warn!("[Telemetry] Action-log review failed: {e}");
            (format!("Automatic action-log review failed: {e}"), "General".to_string())
        }
    }
}

fn summarize_events(events: &[ActionEvent]) -> String {
    events
        .iter()
        .take(MAX_EVENTS_IN_SUMMARY)
        .map(|e| match e {
            ActionEvent::ForegroundChanged { app_name, window_title, .. } => {
                format!("- switched focus to app '{}' (window: '{}')", app_name, truncate(window_title, 80))
            }
            ActionEvent::UiaFocusChanged { control_name, control_type, .. } => format!(
                "- focused a {} control{}",
                control_type.as_deref().unwrap_or("UI"),
                control_name.as_deref().map(|n| format!(" named '{}'", truncate(n, 60))).unwrap_or_default()
            ),
            ActionEvent::UiaWindowOpened { name, .. } => {
                format!("- opened a window{}", name.as_deref().map(|n| format!(" '{}'", truncate(n, 60))).unwrap_or_default())
            }
            ActionEvent::UiaWindowClosed { name, .. } => {
                format!("- closed a window{}", name.as_deref().map(|n| format!(" '{}'", truncate(n, 60))).unwrap_or_default())
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
