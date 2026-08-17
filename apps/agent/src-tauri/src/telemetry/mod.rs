//! Privacy-first activity telemetry pipeline.
//!
//! Design ("Level 0" in the architecture): raw interaction signals (which
//! window/app has focus, which UI control was touched) are captured
//! event-driven (Win32/UI Automation on Windows; Accessibility + active-window
//! polling on macOS) and kept in a short-lived ring buffer that only ever lives
//! in process memory. This module itself never writes raw events to SQLite or
//! to any file. Two independent, complementary reviews turn that raw signal
//! into persisted `reports` rows:
//!   - `aggregator`: every 60s, (a) a pure-text review of whatever UIA/foreground
//!     events accumulated in the ring buffer during that minute (if any), and
//!     (b) a screenshot + local vision pass (`agent::capture_and_analyze_screen`,
//!     save -> analyze -> delete). Each becomes its own `reports` row.
//!   - `action_capture`: fired immediately (debounced ~15s) whenever significant
//!     window lifecycle events are detected, pairing a transient screenshot with
//!     the textual context of that action.
//!
//! Explicitly NOT implemented, by design: any global keyboard/mouse hook,
//! any idle/last-input polling, or any inference of "away from keyboard".

mod action_capture;
mod aggregator;

#[cfg(windows)]
#[path = "foreground_win.rs"]
mod foreground;
#[cfg(target_os = "macos")]
#[path = "foreground_macos.rs"]
mod foreground;
#[cfg(not(any(windows, target_os = "macos")))]
#[path = "foreground_stub.rs"]
mod foreground;

#[cfg(windows)]
#[path = "uia_win.rs"]
mod uia;
#[cfg(target_os = "macos")]
#[path = "uia_macos.rs"]
mod uia;
#[cfg(not(any(windows, target_os = "macos")))]
#[path = "uia_stub.rs"]
mod uia;

use std::collections::VecDeque;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::{Duration, Instant};

/// How long raw events are allowed to live in the in-memory ring buffer
/// before being purged, even if the aggregator hasn't run yet.
const RING_BUFFER_TTL: Duration = Duration::from_secs(120);

#[derive(Debug, Clone)]
pub enum ActionEvent {
    ForegroundChanged {
        app_name: String,
        window_title: String,
        at: Instant,
    },
    UiaFocusChanged {
        control_name: Option<String>,
        control_type: Option<String>,
        at: Instant,
    },
    UiaWindowOpened {
        name: Option<String>,
        at: Instant,
    },
    UiaWindowClosed {
        name: Option<String>,
        at: Instant,
    },
}

impl ActionEvent {
    fn timestamp(&self) -> Instant {
        match self {
            ActionEvent::ForegroundChanged { at, .. } => *at,
            ActionEvent::UiaFocusChanged { at, .. } => *at,
            ActionEvent::UiaWindowOpened { at, .. } => *at,
            ActionEvent::UiaWindowClosed { at, .. } => *at,
        }
    }
}

pub type SharedRing = Arc<Mutex<VecDeque<ActionEvent>>>;
pub type SharedFlag = Arc<AtomicBool>;
/// Platform window handle / AX element id stored as `isize` for cross-thread use.
pub type SharedTarget = Arc<Mutex<Option<isize>>>;
/// Name of the process currently in the foreground.
pub type SharedAppInfo = Arc<Mutex<Option<String>>>;

#[derive(Debug, Clone, Default)]
pub struct TaskContext {
    pub user_task: Option<String>,
    pub jira_ticket: Option<String>,
}

struct TelemetryController {
    running: SharedFlag,
    ring: SharedRing,
    task_ctx: Arc<Mutex<TaskContext>>,
}

static CONTROLLER: OnceLock<TelemetryController> = OnceLock::new();

fn push_event(ring: &SharedRing, event: ActionEvent) {
    let mut buf = ring.lock().unwrap_or_else(|e| e.into_inner());
    buf.push_back(event);
    let cutoff = Instant::now() - RING_BUFFER_TTL;
    while buf.front().map(|e| e.timestamp() < cutoff).unwrap_or(false) {
        buf.pop_front();
    }
}

/// Starts the telemetry pipeline's background threads. Idempotent.
pub fn start(app_handle: tauri::AppHandle, db_path: PathBuf) {
    if CONTROLLER.get().is_some() {
        return;
    }

    let running: SharedFlag = Arc::new(AtomicBool::new(false));
    let ring: SharedRing = Arc::new(Mutex::new(VecDeque::new()));
    let task_ctx = Arc::new(Mutex::new(TaskContext::default()));
    let uia_target: SharedTarget = Arc::new(Mutex::new(None));
    let current_app: SharedAppInfo = Arc::new(Mutex::new(None));

    let action_trigger = Arc::new(action_capture::ActionCaptureTrigger::new(
        app_handle.clone(),
        db_path.clone(),
        running.clone(),
        task_ctx.clone(),
    ));

    foreground::spawn(
        ring.clone(),
        running.clone(),
        uia_target.clone(),
        current_app.clone(),
    );
    uia::spawn(
        ring.clone(),
        running.clone(),
        uia_target,
        current_app,
        action_trigger,
    );
    aggregator::spawn(
        app_handle,
        db_path,
        ring.clone(),
        running.clone(),
        task_ctx.clone(),
    );

    let _ = CONTROLLER.set(TelemetryController {
        running,
        ring,
        task_ctx,
    });
}

pub fn set_running(value: bool) {
    if let Some(c) = CONTROLLER.get() {
        c.running.store(value, Ordering::Relaxed);
        if !value {
            c.ring.lock().unwrap().clear();
        }
    }
}

pub fn set_task_context(user_task: Option<String>, jira_ticket: Option<String>) {
    if let Some(c) = CONTROLLER.get() {
        let mut ctx = c.task_ctx.lock().unwrap();
        ctx.user_task = user_task;
        ctx.jira_ticket = jira_ticket;
    }
}
