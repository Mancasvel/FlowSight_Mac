//! Local, opt-in reminders. Qwen may *propose* a notification through an internal
//! tool call; only this module can deliver one after checking observed evidence.

use chrono::{Local, NaiveDateTime};
use reqwest::blocking::Client;
use rusqlite::Connection;
use serde_json::{json, Value};
use std::collections::{HashSet, VecDeque};
use std::path::Path;
use std::sync::{Mutex, OnceLock};
use std::thread;
use std::time::{Duration, Instant};

#[cfg(desktop)]
use tauri_plugin_notification::NotificationExt;

use crate::vision_model::LLAMA_CHAT_MODEL_ID;

const SWITCH_WINDOW: Duration = Duration::from_secs(10 * 60);
const MIN_SWITCH_SPAN: Duration = Duration::from_secs(3 * 60);
const ALERT_COOLDOWN: Duration = Duration::from_secs(30 * 60);
const SHARED_COOLDOWN: Duration = Duration::from_secs(10 * 60);
const EVALUATION_COOLDOWN: Duration = Duration::from_secs(5 * 60);
const MAX_DECISION_AGE: Duration = Duration::from_secs(2 * 60);
const SWITCH_THRESHOLD: usize = 12;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum AlertKind {
    ContextSwitching,
    NonWorkBrowsing,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Advice {
    ChooseOneTask,
    FinishCurrentStep,
    PauseAndPrioritize,
    ReturnToTask,
    TimeboxBrowsing,
    IntentionalBreak,
}

impl Advice {
    fn from_str(value: &str) -> Option<Self> {
        match value {
            "choose_one_task" => Some(Self::ChooseOneTask),
            "finish_current_step" => Some(Self::FinishCurrentStep),
            "pause_and_prioritize" => Some(Self::PauseAndPrioritize),
            "return_to_task" => Some(Self::ReturnToTask),
            "timebox_browsing" => Some(Self::TimeboxBrowsing),
            "intentional_break" => Some(Self::IntentionalBreak),
            _ => None,
        }
    }

    fn allowed_for(self, kind: AlertKind) -> bool {
        match kind {
            AlertKind::ContextSwitching => matches!(
                self,
                Self::ChooseOneTask | Self::FinishCurrentStep | Self::PauseAndPrioritize
            ),
            AlertKind::NonWorkBrowsing => matches!(
                self,
                Self::ReturnToTask | Self::TimeboxBrowsing | Self::IntentionalBreak
            ),
        }
    }

    fn copy(self) -> (&'static str, &'static str) {
        match self {
            Self::ChooseOneTask => ("A moment to refocus", "You've switched among several apps. Could you choose one task for the next few minutes?"),
            Self::FinishCurrentStep => ("A moment to refocus", "There have been many app switches. Consider finishing one small step before changing tasks again."),
            Self::PauseAndPrioritize => ("A moment to refocus", "You've switched among several apps. A short pause to choose the next priority may help."),
            Self::ReturnToTask => ("A gentle focus reminder", "Non-work browsing has lasted at least two minutes. Ready to return to your task?"),
            Self::TimeboxBrowsing => ("A gentle focus reminder", "Non-work browsing has lasted at least two minutes. Would a short time limit help?"),
            Self::IntentionalBreak => ("A gentle focus reminder", "Non-work browsing has lasted at least two minutes. If you need a break, make it intentional."),
        }
    }
}

#[derive(Clone, Copy, Debug)]
enum Evidence {
    ContextSwitching {
        switches: usize,
        unique_apps: usize,
        span_seconds: u64,
    },
    NonWorkBrowsing {
        episodes_today: usize,
    },
}

impl Evidence {
    fn kind(self) -> AlertKind {
        match self {
            Self::ContextSwitching { .. } => AlertKind::ContextSwitching,
            Self::NonWorkBrowsing { .. } => AlertKind::NonWorkBrowsing,
        }
    }

    fn verified(self) -> bool {
        match self {
            Self::ContextSwitching {
                switches,
                unique_apps,
                span_seconds,
            } => {
                switches >= SWITCH_THRESHOLD
                    && unique_apps >= 3
                    && span_seconds >= MIN_SWITCH_SPAN.as_secs()
                    && span_seconds <= SWITCH_WINDOW.as_secs()
            }
            // The canonical detector counts only browsing episodes >= 2 min.
            Self::NonWorkBrowsing { episodes_today } => episodes_today > 0,
        }
    }

    fn model_input(self) -> Value {
        // No app/window names, page contents, task labels or URLs leave this module.
        match self {
            Self::ContextSwitching {
                switches,
                unique_apps,
                span_seconds,
            } => json!({
                "signal": "context_switching", "switches": switches,
                "unique_apps": unique_apps, "span_seconds": span_seconds,
            }),
            Self::NonWorkBrowsing { episodes_today } => json!({
                "signal": "non_work_browsing",
                "minimum_episode_seconds": crate::focus_semantics::BROWSING_DISTRACTION_MIN_SECS,
                "episodes_today": episodes_today,
            }),
        }
    }
}

struct Proposal {
    evidence: Evidence,
    generation: u64,
    created_at: Instant,
    origin_app: Option<String>,
    destination: Option<String>,
}

#[derive(Default)]
struct AlertState {
    enabled: bool,
    monitoring: bool,
    last_app: Option<String>,
    switches: VecDeque<(Instant, String)>,
    distraction_day: String,
    distraction_events: usize,
    last_switch_alert: Option<Instant>,
    last_distraction_alert: Option<Instant>,
    last_alert: Option<Instant>,
    last_switch_evaluation: Option<Instant>,
    last_distraction_evaluation: Option<Instant>,
    pending_switch: bool,
    pending_distraction: bool,
    generation: u64,
}

impl AlertState {
    fn pending(&self, kind: AlertKind) -> bool {
        match kind {
            AlertKind::ContextSwitching => self.pending_switch,
            AlertKind::NonWorkBrowsing => self.pending_distraction,
        }
    }

    fn set_pending(&mut self, kind: AlertKind, pending: bool) {
        match kind {
            AlertKind::ContextSwitching => self.pending_switch = pending,
            AlertKind::NonWorkBrowsing => self.pending_distraction = pending,
        }
    }
}

static STATE: OnceLock<Mutex<AlertState>> = OnceLock::new();

fn state() -> &'static Mutex<AlertState> {
    STATE.get_or_init(|| Mutex::new(AlertState::default()))
}

fn cooldown_elapsed(last: Option<Instant>, now: Instant, duration: Duration) -> bool {
    last.map_or(true, |last| now.saturating_duration_since(last) >= duration)
}

fn context_churn(switches: &VecDeque<(Instant, String)>, now: Instant) -> bool {
    if switches.len() < SWITCH_THRESHOLD {
        return false;
    }
    let Some((first, _)) = switches.front() else {
        return false;
    };
    if now.saturating_duration_since(*first) < MIN_SWITCH_SPAN {
        return false;
    }
    switches
        .iter()
        .map(|(_, app)| app.as_str())
        .collect::<HashSet<_>>()
        .len()
        >= 3
}

pub fn set_enabled(enabled: bool) {
    let mut guard = state().lock().unwrap_or_else(|error| error.into_inner());
    guard.enabled = enabled;
    guard.generation = guard.generation.wrapping_add(1);
    guard.pending_switch = false;
    guard.pending_distraction = false;
    guard.last_app = None;
    guard.switches.clear();
}

pub fn start_monitoring(db_path: &Path) {
    let today = Local::now().format("%Y-%m-%d").to_string();
    let baseline = Connection::open(db_path)
        .ok()
        .and_then(|conn| crate::focus_semantics::summarize_from_db(&conn, &today, &today).ok())
        .map(|summary| summary.distraction_events)
        .unwrap_or(0);
    let mut guard = state().lock().unwrap_or_else(|error| error.into_inner());
    guard.monitoring = true;
    guard.generation = guard.generation.wrapping_add(1);
    guard.pending_switch = false;
    guard.pending_distraction = false;
    guard.last_app = None;
    guard.switches.clear();
    guard.distraction_day = today;
    guard.distraction_events = baseline;
}

pub fn stop_monitoring() {
    let mut guard = state().lock().unwrap_or_else(|error| error.into_inner());
    guard.monitoring = false;
    guard.generation = guard.generation.wrapping_add(1);
    guard.pending_switch = false;
    guard.pending_distraction = false;
    guard.last_app = None;
    guard.switches.clear();
}

pub fn excluded_app_entered() {
    let mut guard = state().lock().unwrap_or_else(|error| error.into_inner());
    guard.last_app = None;
    guard.switches.clear();
    guard.generation = guard.generation.wrapping_add(1);
    guard.pending_switch = false;
    guard.pending_distraction = false;
}

pub fn record_app_switch(app: &tauri::AppHandle, app_name: &str) {
    let now = Instant::now();
    let normalized = app_name.trim().to_ascii_lowercase();
    static OWN_EXE: OnceLock<String> = OnceLock::new();
    let own_exe = OWN_EXE.get_or_init(|| {
        std::env::current_exe()
            .ok()
            .and_then(|path| {
                path.file_name()
                    .map(|name| name.to_string_lossy().to_ascii_lowercase())
            })
            .unwrap_or_default()
    });
    if normalized.is_empty() || normalized == *own_exe {
        return;
    }
    let proposal = {
        let mut guard = state().lock().unwrap_or_else(|error| error.into_inner());
        if !guard.enabled || !guard.monitoring {
            return;
        }
        if guard.last_app.as_deref() == Some(normalized.as_str()) {
            return;
        }
        if guard.last_app.replace(normalized.clone()).is_none() {
            return;
        }
        guard.switches.push_back((now, normalized));
        while guard
            .switches
            .front()
            .is_some_and(|(time, _)| now.saturating_duration_since(*time) > SWITCH_WINDOW)
        {
            guard.switches.pop_front();
        }
        let ready = context_churn(&guard.switches, now)
            && !guard.pending(AlertKind::ContextSwitching)
            && cooldown_elapsed(guard.last_switch_evaluation, now, EVALUATION_COOLDOWN)
            && cooldown_elapsed(guard.last_switch_alert, now, ALERT_COOLDOWN)
            && cooldown_elapsed(guard.last_alert, now, SHARED_COOLDOWN);
        if ready {
            let first = guard.switches.front().map(|(at, _)| *at).unwrap_or(now);
            let evidence = Evidence::ContextSwitching {
                switches: guard.switches.len(),
                unique_apps: guard
                    .switches
                    .iter()
                    .map(|(_, app)| app)
                    .collect::<HashSet<_>>()
                    .len(),
                span_seconds: now.saturating_duration_since(first).as_secs(),
            };
            guard.set_pending(AlertKind::ContextSwitching, true);
            guard.last_switch_evaluation = Some(now);
            Some(Proposal {
                evidence,
                generation: guard.generation,
                created_at: now,
                origin_app: None,
                destination: None,
            })
        } else {
            None
        }
    };
    if let Some(proposal) = proposal {
        evaluate_in_background(app.clone(), proposal);
    }
}

pub fn review_browsing_report(
    app: &tauri::AppHandle,
    db_path: &Path,
    category: &str,
    duration_seconds: u64,
    description: &str,
    captured_app: Option<&str>,
) {
    if duration_seconds == 0 || !category.eq_ignore_ascii_case("Browsing") {
        return;
    }
    let today = Local::now().format("%Y-%m-%d").to_string();
    let count = Connection::open(db_path)
        .ok()
        .and_then(|conn| crate::focus_semantics::summarize_from_db(&conn, &today, &today).ok())
        .map(|summary| summary.distraction_events);
    let Some(count) = count else { return };
    let now = Instant::now();
    let proposal = {
        let mut guard = state().lock().unwrap_or_else(|error| error.into_inner());
        if !guard.monitoring || !guard.enabled {
            return;
        }
        if guard.distraction_day != today {
            guard.distraction_day = today;
            guard.distraction_events = 0;
        }
        let is_new_episode = count > guard.distraction_events;
        guard.distraction_events = count;
        let ready = is_new_episode
            && !guard.pending(AlertKind::NonWorkBrowsing)
            && cooldown_elapsed(guard.last_distraction_evaluation, now, EVALUATION_COOLDOWN)
            && cooldown_elapsed(guard.last_distraction_alert, now, ALERT_COOLDOWN)
            && cooldown_elapsed(guard.last_alert, now, SHARED_COOLDOWN);
        if ready {
            guard.set_pending(AlertKind::NonWorkBrowsing, true);
            guard.last_distraction_evaluation = Some(now);
            Some(Proposal {
                evidence: Evidence::NonWorkBrowsing {
                    episodes_today: count,
                },
                generation: guard.generation,
                created_at: now,
                origin_app: captured_app.map(str::to_string),
                destination: None,
            })
        } else {
            None
        }
    };
    if let Some(mut proposal) = proposal {
        let sample = crate::focus_semantics::ActivitySample {
            start: Local::now().naive_local(),
            duration_seconds: i64::try_from(duration_seconds).unwrap_or(i64::MAX),
            category: category.to_string(),
            description: description.to_string(),
            ticket: None,
            theme_hint: None,
            app_name: captured_app.map(str::to_string),
            window_title: None,
        };
        proposal.destination =
            crate::focus_semantics::notification_destination(&sample).filter(|label| {
                let app_or_site = label.strip_suffix(" Shorts").unwrap_or(label);
                !crate::privacy::application_is_excluded(db_path, Some(app_or_site))
            });
        evaluate_in_background(app.clone(), proposal);
    }
}

fn evaluate_in_background(app: tauri::AppHandle, proposal: Proposal) {
    // The foreground hook must never wait for an LLM HTTP request.
    thread::spawn(move || {
        if proposal.evidence.kind() == AlertKind::ContextSwitching && recently_active_deep_focus() {
            finish_proposal(&app, proposal, None);
            return;
        }
        let decision = ask_local_qwen(proposal.evidence).unwrap_or_else(|error| {
            // Invalid or unavailable model output never turns into a notification.
            log::warn!("[FocusAlerts] Local tool decision unavailable: {error}");
            None
        });
        finish_proposal(&app, proposal, decision);
    });
}

fn recently_active_deep_focus() -> bool {
    let Ok(db_path) = crate::paths::db_path() else {
        return false;
    };
    let Ok(conn) = Connection::open(db_path) else {
        return false;
    };
    let now = Local::now().naive_local();
    let today = now.format("%Y-%m-%d").to_string();
    let Ok(summary) = crate::focus_semantics::summarize_from_db(&conn, &today, &today) else {
        return false;
    };
    summary
        .sessions
        .last()
        .is_some_and(|session| session_is_recent_deep_focus(session, now))
}

fn session_is_recent_deep_focus(
    session: &crate::focus_semantics::FocusSession,
    now: NaiveDateTime,
) -> bool {
    matches!(session.tier.as_str(), "deep" | "extended")
        && session.break_reason == crate::focus_semantics::BreakReason::EndOfWindow
        && NaiveDateTime::parse_from_str(&session.end, "%Y-%m-%d %H:%M:%S").is_ok_and(|ended| {
            let age = now.signed_duration_since(ended).num_seconds();
            (0..=120).contains(&age)
        })
}

fn ask_local_qwen(evidence: Evidence) -> Result<Option<Advice>, String> {
    if !evidence.verified() {
        return Ok(None);
    }
    let url = crate::llama_port::managed_chat_completions_url()
        .ok_or_else(|| "Local AI server offline".to_string())?;
    let client = Client::builder()
        .timeout(Duration::from_secs(45))
        .build()
        .map_err(|error| error.to_string())?;
    let body = json!({
        "model": LLAMA_CHAT_MODEL_ID,
        "messages": [
            { "role": "system", "content": "You are FlowSight's local focus reminder planner. You see only verified aggregate signals. You may call send_focus_notification at most once, or make no tool call. App switching can be productive; abstain if a reminder would be speculative or interruptive. Never invent a cause, app, task, emotion, or diagnosis. The app creates the notification from a validated local template; you only choose an allowed advice code." },
            { "role": "user", "content": evidence.model_input().to_string() }
        ],
        "tools": [{
            "type": "function",
            "function": {
                "name": "send_focus_notification",
                "description": "Propose one evidence-grounded focus reminder. The host application validates the signal, permission, cooldown and advice before showing anything.",
                "parameters": {
                    "type": "object",
                    "properties": { "advice": { "type": "string", "enum": [
                        "choose_one_task", "finish_current_step", "pause_and_prioritize",
                        "return_to_task", "timebox_browsing", "intentional_break"
                    ] } },
                    "required": ["advice"],
                    "additionalProperties": false
                }
            }
        }],
        "tool_choice": "auto",
        "temperature": 0,
        "max_tokens": 100,
        "stream": false
    });
    let response = client
        .post(url)
        .json(&body)
        .send()
        .map_err(|error| error.to_string())?;
    if !response.status().is_success() {
        return Err(format!("Local AI returned HTTP {}", response.status()));
    }
    let result: Value = response.json().map_err(|error| error.to_string())?;
    parse_tool_decision(&result, evidence.kind())
}

fn parse_tool_decision(result: &Value, kind: AlertKind) -> Result<Option<Advice>, String> {
    let message = &result["choices"][0]["message"];
    if !message.is_object() {
        return Err("Local AI returned no message".into());
    }
    let Some(calls) = message["tool_calls"].as_array() else {
        return Ok(None);
    };
    if calls.is_empty() {
        return Ok(None);
    }
    if calls.len() != 1 || calls[0]["function"]["name"] != "send_focus_notification" {
        return Err("Local AI requested an unsupported tool call".into());
    }
    let raw_args = &calls[0]["function"]["arguments"];
    let args: Value = match raw_args {
        Value::String(text) => serde_json::from_str(text).map_err(|error| error.to_string())?,
        Value::Object(_) => raw_args.clone(),
        _ => return Err("Local AI returned malformed tool arguments".into()),
    };
    if args.as_object().map_or(true, |obj| obj.len() != 1) {
        return Err("Local AI returned unexpected tool arguments".into());
    }
    let advice = args["advice"]
        .as_str()
        .and_then(Advice::from_str)
        .ok_or_else(|| "Local AI returned an unknown advice code".to_string())?;
    if !advice.allowed_for(kind) {
        return Err("Local AI advice does not match observed evidence".into());
    }
    Ok(Some(advice))
}

fn can_deliver(guard: &AlertState, proposal: &Proposal, advice: Advice, now: Instant) -> bool {
    guard.enabled
        && guard.monitoring
        && guard.pending(proposal.evidence.kind())
        && guard.generation == proposal.generation
        && now.saturating_duration_since(proposal.created_at) <= MAX_DECISION_AGE
        && proposal.evidence.verified()
        && advice.allowed_for(proposal.evidence.kind())
        && cooldown_elapsed(guard.last_alert, now, SHARED_COOLDOWN)
        && match proposal.evidence.kind() {
            AlertKind::ContextSwitching => {
                cooldown_elapsed(guard.last_switch_alert, now, ALERT_COOLDOWN)
            }
            AlertKind::NonWorkBrowsing => {
                cooldown_elapsed(guard.last_distraction_alert, now, ALERT_COOLDOWN)
            }
        }
}

fn safe_foreground_name(application: &str) -> bool {
    let name = crate::privacy::normalized_application(application);
    !name.is_empty()
        && !matches!(
            name.as_str(),
            "lockapp" | "logonui" | "winlogon" | "screensaver"
        )
        && !name.contains("flowsight")
}

fn notification_surface_safe() -> Option<crate::context::SystemContext> {
    let foreground = crate::context::get_system_context();
    let Some(app_name) = foreground.app_name.as_deref() else {
        // A locked desktop usually has no accessible foreground window.
        return None;
    };
    if !safe_foreground_name(app_name) {
        return None;
    }
    let Ok(db_path) = crate::paths::db_path() else {
        return None;
    };
    (!crate::privacy::application_is_excluded(&db_path, Some(app_name))).then_some(foreground)
}

fn clean_task_label(value: Option<String>) -> Option<String> {
    let cleaned = value?.split_whitespace().collect::<Vec<_>>().join(" ");
    if cleaned.is_empty() || cleaned.eq_ignore_ascii_case("general") {
        return None;
    }
    let shortened = cleaned.chars().take(56).collect::<String>();
    Some(if shortened.chars().count() < cleaned.chars().count() {
        format!("{shortened}…")
    } else {
        shortened
    })
}

fn visible_destination<'a>(
    proposal: &'a Proposal,
    foreground: &crate::context::SystemContext,
) -> Option<&'a str> {
    let destination = proposal.destination.as_deref()?;
    let origin_app = proposal.origin_app.as_deref()?;
    let current_app = foreground.app_name.as_deref()?;
    if crate::privacy::normalized_application(origin_app)
        != crate::privacy::normalized_application(current_app)
    {
        return None;
    }
    let site = destination.strip_suffix(" Shorts").unwrap_or(destination);
    let key = crate::privacy::normalized_application(site);
    let app_key = crate::privacy::normalized_application(current_app);
    let title = foreground
        .window_title
        .as_deref()
        .unwrap_or_default()
        .to_lowercase();
    (app_key == key || title.contains(&key)).then_some(destination)
}

fn browsing_still_relevant(
    proposal: &Proposal,
    foreground: &crate::context::SystemContext,
) -> bool {
    if proposal.evidence.kind() != AlertKind::NonWorkBrowsing {
        return true;
    }
    let Some(origin) = proposal.origin_app.as_deref() else {
        return true;
    };
    crate::privacy::normalized_application(origin)
        == crate::privacy::normalized_application(
            foreground.app_name.as_deref().unwrap_or_default(),
        )
}

fn contextual_copy(
    advice: Advice,
    destination: Option<&str>,
    task: Option<&str>,
) -> (String, String) {
    if destination.is_none() && task.is_none() {
        let (title, body) = advice.copy();
        return (title.into(), body.into());
    }
    let task = task.map_or_else(|| "your task".to_string(), |name| format!("“{name}”"));
    let body = match advice {
        Advice::ChooseOneTask => {
            format!("You've switched apps several times. Choose {task} for the next few minutes.")
        }
        Advice::FinishCurrentStep => format!(
            "You've switched apps several times. Finish one step of {task} before switching again."
        ),
        Advice::PauseAndPrioritize => {
            format!("Many app switches. Take a moment to return to {task}.")
        }
        Advice::ReturnToTask => format!(
            "{}Return to {task} and keep your momentum going.",
            destination.map_or(String::new(), |name| format!("{name} can wait. "))
        ),
        Advice::TimeboxBrowsing => format!(
            "{}Set a short limit, then return to {task}.",
            destination.map_or(String::new(), |name| format!("You're on {name}. "))
        ),
        Advice::IntentionalBreak => {
            format!(
                "{}If this is a break, make it intentional, then return to {task}.",
                destination.map_or(String::new(), |name| format!("You're on {name}. "))
            )
        }
    };
    ("Keep your focus".into(), body)
}

fn finish_proposal(app: &tauri::AppHandle, proposal: Proposal, decision: Option<Advice>) {
    let mut guard = state().lock().unwrap_or_else(|error| error.into_inner());
    if guard.generation != proposal.generation {
        return;
    }
    let Some(advice) = decision else {
        guard.set_pending(proposal.evidence.kind(), false);
        return;
    };
    let now = Instant::now();
    if !can_deliver(&guard, &proposal, advice, now) {
        guard.set_pending(proposal.evidence.kind(), false);
        return;
    }
    let Some(foreground) = notification_surface_safe() else {
        guard.set_pending(proposal.evidence.kind(), false);
        return;
    };
    if !browsing_still_relevant(&proposal, &foreground) {
        // The user already left the distracting app while Qwen was deciding.
        guard.set_pending(proposal.evidence.kind(), false);
        return;
    }
    let (title, body) = if crate::desktop_presence::contextual_focus_alerts_enabled() {
        let destination = visible_destination(&proposal, &foreground);
        let task = clean_task_label(crate::telemetry::selected_task_for_reminder());
        contextual_copy(advice, destination, task.as_deref())
    } else {
        let (title, body) = advice.copy();
        (title.into(), body.into())
    };
    if crate::local_agent::state::hold_notification(&title, &body).unwrap_or(false) {
        guard.set_pending(proposal.evidence.kind(), false);
        guard.last_alert = Some(now);
        return;
    }
    // Hold the state lock through the OS call: stop/opt-out cannot complete and
    // then have an old, in-flight model result show a notification afterwards.
    #[cfg(desktop)]
    let sent = match app
        .notification()
        .builder()
        .title(&title)
        .body(&body)
        .show()
    {
        Ok(()) => true,
        Err(error) => {
            log::warn!("[FocusAlerts] Could not show notification: {error}");
            false
        }
    };
    #[cfg(not(desktop))]
    let sent = {
        let _ = (app, title, body);
        false
    };
    guard.set_pending(proposal.evidence.kind(), false);
    if sent {
        guard.last_alert = Some(now);
        match proposal.evidence.kind() {
            AlertKind::ContextSwitching => guard.last_switch_alert = Some(now),
            AlertKind::NonWorkBrowsing => guard.last_distraction_alert = Some(now),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn repeated_windows_in_one_app_do_not_count_as_context_churn() {
        let now = Instant::now();
        let switches = (0..12)
            .map(|index| {
                (
                    now - Duration::from_secs((12 - index as u64) * 20),
                    "browser.exe".into(),
                )
            })
            .collect();
        assert!(!context_churn(&switches, now));
    }

    #[test]
    fn context_churn_needs_multiple_apps_and_sustained_switching() {
        let now = Instant::now();
        let sustained = (0..12)
            .map(|index| {
                (
                    now - Duration::from_secs((12 - index as u64) * 20),
                    ["editor.exe", "browser.exe", "chat.exe"][index % 3].into(),
                )
            })
            .collect();
        assert!(context_churn(&sustained, now));
        let burst = (0..12)
            .map(|index| {
                (
                    now - Duration::from_secs((12 - index as u64) * 5),
                    ["editor.exe", "browser.exe", "chat.exe"][index % 3].into(),
                )
            })
            .collect();
        assert!(!context_churn(&burst, now));
    }

    #[test]
    fn cooldown_prevents_repeat_reminders() {
        let now = Instant::now();
        assert!(!cooldown_elapsed(
            Some(now - Duration::from_secs(60)),
            now,
            ALERT_COOLDOWN
        ));
        assert!(cooldown_elapsed(
            Some(now - ALERT_COOLDOWN),
            now,
            ALERT_COOLDOWN
        ));
    }

    #[test]
    fn qwen_tool_call_must_match_verified_signal() {
        let call = json!({ "choices": [{ "message": { "tool_calls": [{
            "type": "function", "function": {
                "name": "send_focus_notification",
                "arguments": "{\"advice\":\"choose_one_task\"}"
            }
        }] } }] });
        assert_eq!(
            parse_tool_decision(&call, AlertKind::ContextSwitching).unwrap(),
            Some(Advice::ChooseOneTask)
        );
        assert!(parse_tool_decision(&call, AlertKind::NonWorkBrowsing).is_err());

        let invented = json!({ "choices": [{ "message": { "tool_calls": [{
            "function": { "name": "send_focus_notification",
                "arguments": "{\"advice\":\"choose_one_task\",\"body\":\"You visited a private page\"}"
            }
        }] } }] });
        assert!(parse_tool_decision(&invented, AlertKind::ContextSwitching).is_err());
    }

    #[test]
    fn no_tool_call_means_no_notification() {
        let abstain = json!({ "choices": [{ "message": { "content": "No reminder needed." } }] });
        assert_eq!(
            parse_tool_decision(&abstain, AlertKind::ContextSwitching).unwrap(),
            None
        );
        let unknown = json!({ "choices": [{ "message": { "tool_calls": [{
            "function": { "name": "send_email", "arguments": "{}" }
        }] } }] });
        assert!(parse_tool_decision(&unknown, AlertKind::ContextSwitching).is_err());
    }

    #[test]
    fn stopped_tracking_revoked_consent_and_cooldowns_block_delayed_delivery() {
        let now = Instant::now();
        let proposal = Proposal {
            evidence: Evidence::ContextSwitching {
                switches: 12,
                unique_apps: 3,
                span_seconds: 200,
            },
            generation: 7,
            created_at: now,
            origin_app: None,
            destination: None,
        };
        let mut guard = AlertState {
            enabled: true,
            monitoring: true,
            pending_switch: true,
            generation: 7,
            ..AlertState::default()
        };
        assert!(can_deliver(&guard, &proposal, Advice::ChooseOneTask, now));
        guard.monitoring = false;
        assert!(!can_deliver(&guard, &proposal, Advice::ChooseOneTask, now));
        guard.monitoring = true;
        guard.enabled = false;
        assert!(!can_deliver(&guard, &proposal, Advice::ChooseOneTask, now));
        guard.enabled = true;
        guard.generation += 1;
        assert!(!can_deliver(&guard, &proposal, Advice::ChooseOneTask, now));
        guard.generation = 7;
        guard.last_alert = Some(now);
        assert!(!can_deliver(&guard, &proposal, Advice::ChooseOneTask, now));
        guard.last_alert = None;
        assert!(!can_deliver(
            &guard,
            &proposal,
            Advice::ChooseOneTask,
            now + MAX_DECISION_AGE + Duration::from_secs(1)
        ));
    }

    #[test]
    fn contextual_reminder_names_only_the_current_destination_and_selected_task() {
        let proposal = Proposal {
            evidence: Evidence::NonWorkBrowsing { episodes_today: 1 },
            generation: 1,
            created_at: Instant::now(),
            origin_app: Some("Chrome.exe".into()),
            destination: Some("YouTube Shorts".into()),
        };
        let mut foreground = crate::context::SystemContext {
            app_name: Some("Chrome.exe".into()),
            window_title: Some("YouTube Shorts - Google Chrome".into()),
            ..Default::default()
        };
        assert_eq!(
            visible_destination(&proposal, &foreground),
            Some("YouTube Shorts")
        );
        assert!(browsing_still_relevant(&proposal, &foreground));
        let task = clean_task_label(Some("  Finish   release notes  ".into()));
        assert_eq!(
            contextual_copy(Advice::ReturnToTask, visible_destination(&proposal, &foreground), task.as_deref()).1,
            "YouTube Shorts can wait. Return to “Finish release notes” and keep your momentum going."
        );

        foreground.window_title = Some("Project wiki - Google Chrome".into());
        assert_eq!(visible_destination(&proposal, &foreground), None);
        foreground.app_name = Some("Editor.exe".into());
        assert_eq!(visible_destination(&proposal, &foreground), None);
        assert!(!browsing_still_relevant(&proposal, &foreground));
        assert_eq!(clean_task_label(Some("General".into())), None);
        let fallback = contextual_copy(Advice::ReturnToTask, None, None);
        let generic = Advice::ReturnToTask.copy();
        assert_eq!(fallback, (generic.0.into(), generic.1.into()));
    }

    #[test]
    fn model_input_contains_only_aggregate_evidence() {
        let evidence = Evidence::ContextSwitching {
            switches: 12,
            unique_apps: 3,
            span_seconds: 200,
        };
        assert!(evidence.verified());
        assert!(!Evidence::ContextSwitching {
            switches: 12,
            unique_apps: 2,
            span_seconds: 200
        }
        .verified());
        assert_eq!(
            evidence.model_input(),
            json!({
                "signal": "context_switching", "switches": 12,
                "unique_apps": 3, "span_seconds": 200,
            })
        );
    }

    #[test]
    fn an_active_deep_block_suppresses_switch_reminders() {
        use crate::focus_semantics::{BreakReason, FocusSession};
        let now = chrono::NaiveDate::from_ymd_opt(2026, 9, 29)
            .unwrap()
            .and_hms_opt(14, 30, 0)
            .unwrap();
        let mut session = FocusSession {
            start: "2026-09-29 14:00:00".into(),
            end: "2026-09-29 14:29:30".into(),
            focus_seconds: 1770,
            elapsed_seconds: 1770,
            tier: "deep".into(),
            category_mix: vec![],
            theme: None,
            interrupted: false,
            bridged_noise_seconds: 0,
            break_reason: BreakReason::EndOfWindow,
            break_category: None,
        };
        assert!(session_is_recent_deep_focus(&session, now));
        session.break_reason = BreakReason::NonFocus;
        assert!(!session_is_recent_deep_focus(&session, now));
        session.break_reason = BreakReason::EndOfWindow;
        session.end = "2026-09-29 14:20:00".into();
        assert!(!session_is_recent_deep_focus(&session, now));
    }

    #[test]
    fn lock_screen_and_flowsight_are_not_notification_surfaces() {
        for app in [
            "",
            "LockApp.exe",
            "LogonUI.exe",
            "winlogon.exe",
            "FlowSight Agent.exe",
        ] {
            assert!(!safe_foreground_name(app), "{app}");
        }
        assert!(safe_foreground_name("chrome.exe"));
    }
}
