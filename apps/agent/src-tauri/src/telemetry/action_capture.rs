//! Action-triggered screenshot capture — a native replacement for the old
//! Windows Steps Recorder (`psr.exe`) idea: pair a screenshot with the
//! specific UI action that just happened, instead of sampling the screen on
//! a fixed timer.
//!
//! `telemetry::uia`'s window-lifecycle handler calls [`ActionCaptureTrigger::trigger`]
//! whenever it sees a significant event (a window opening or closing). This
//! module rate-limits those calls (see `COOLDOWN`) and, if allowed, hands
//! off to a fresh thread that captures the screen, sends it to the local
//! vision model together with the textual action context, and persists the
//! result as its own `reports` row — independent of, and in addition to, the
//! 60s aggregator cycle (action-log review + periodic vision snapshot).

use super::{SharedFlag, TaskContext};
use std::path::PathBuf;
use std::sync::atomic::Ordering;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use tauri::Emitter;

/// Minimum time between two action-triggered captures. A full screenshot +
/// local vision-model round trip is comparatively expensive (seconds, not
/// milliseconds), so this is the main guardrail keeping background resource
/// usage low. It's also long enough to collapse a burst of window
/// transitions (e.g. an app popping several dialogs in a row) into a single
/// capture instead of one per transition.
const COOLDOWN: Duration = Duration::from_secs(15);

/// Action-triggered reports mark a point-in-time event rather than an
/// observed interval, unlike the 60s aggregator's rows.
const ACTION_REPORT_DURATION_SECS: u64 = 0;

pub struct ActionCaptureTrigger {
    app_handle: tauri::AppHandle,
    db_path: PathBuf,
    running: SharedFlag,
    task_ctx: Arc<Mutex<TaskContext>>,
    last_fired: Mutex<Option<Instant>>,
}

impl ActionCaptureTrigger {
    pub fn new(
        app_handle: tauri::AppHandle,
        db_path: PathBuf,
        running: SharedFlag,
        task_ctx: Arc<Mutex<TaskContext>>,
    ) -> Self {
        Self { app_handle, db_path, running, task_ctx, last_fired: Mutex::new(None) }
    }

    /// Attempts to fire an action-triggered capture for `action_context`
    /// (a short human-readable sentence such as "The user just opened a
    /// window named 'Settings' in notepad.exe."). Silently no-ops if
    /// monitoring is off or the cooldown hasn't elapsed yet. Must stay
    /// non-blocking: callers invoke this from a UI Automation COM event
    /// handler that must never hang or make re-entrant COM calls.
    pub fn trigger(self: &Arc<Self>, action_context: String) {
        if !self.running.load(Ordering::Relaxed) {
            return;
        }

        {
            let mut last = self.last_fired.lock().unwrap();
            let now = Instant::now();
            if last.map(|t| now.duration_since(t) < COOLDOWN).unwrap_or(false) {
                return;
            }
            *last = Some(now);
        }

        let this = Arc::clone(self);
        std::thread::spawn(move || this.run_capture(action_context));
    }

    fn run_capture(&self, action_context: String) {
        let (user_task, jira_ticket) = {
            let ctx = self.task_ctx.lock().unwrap();
            (ctx.user_task.clone(), ctx.jira_ticket.clone())
        };
        let task_label = jira_ticket.clone().or_else(|| user_task.clone()).unwrap_or_else(|| "General".to_string());

        let (description, category) = match crate::agent::capture_and_analyze_action(&task_label, &action_context) {
            Ok(result) => result,
            Err(e) => {
                log::warn!("[Telemetry][ActionCapture] capture/analysis failed: {e}");
                return;
            }
        };
        let category = crate::agent_pure::resolve_persisted_category(&category);

        match crate::agent::insert_report(
            &self.db_path,
            &description,
            &category,
            jira_ticket.clone(),
            ACTION_REPORT_DURATION_SECS,
        ) {
            Some(id) => {
                let _ = self.app_handle.emit(
                    "activity-report",
                    serde_json::json!({
                        "id": id,
                        "description": description,
                        "category": category,
                        "jiraTicket": jira_ticket,
                    }),
                );
            }
            None => log::warn!("[Telemetry][ActionCapture] Failed to persist action-triggered report to local DB"),
        }
    }
}
