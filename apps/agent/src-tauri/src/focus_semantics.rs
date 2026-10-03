//! Canonical, privacy-first Deep Focus semantics.
//!
//! `Deep Focus` is an observable continuity proxy, not phenomenological flow.
//! This module is the only place allowed to classify focus roles or segment
//! sessions. Renderers and coaches consume [`FocusSummary`] and must not
//! reimplement the detector.
//!
//! Evidence-to-decision log (policy v1; full details live in tests/names):
//! - Csikszentmihalyi & LeFevre, 1989, JPSP, "Optimal Experience in Work and
//!   Leisure". Signal: ESM; N=78 workers/~4,800 reports. Finding: flow also
//!   requires subjective challenge/skill and concentration. Limitation: old,
//!   small US sample. Change: proxy disclaimer; never claim detected flow.
//!   Confidence: high.
//! - Fong, Zaleski & Leach, 2015, J Positive Psychology, challenge-skill
//!   meta-analysis. Signal: self-report; 28 studies. Finding: moderate,
//!   heterogeneous association. Limitation: mainly correlational. Change:
//!   duration/category cannot establish flow. Confidence: high.
//! - Mark, Gonzalez & Harris, 2005, CHI, "No Task Left Behind?" Signal:
//!   observation/interviews; N=24 information workers. Finding: ~3:05 per
//!   event and 10-12 min per working sphere. Limitation: 2004 offices. Change:
//!   expose fragmentation and switch rate. Confidence: high.
//! - Mark, Gudith & Klocke, 2008, CHI, "The Cost of Interrupted Work".
//!   Signal: experiment; N=48. Finding: interrupted work was faster but more
//!   stressful/frustrating. Limitation: artificial tasks. Change: no 23:15
//!   claim and no volume-only health score. Confidence: high.
//! - Czerwinski, Horvitz & Wilhite, 2004, CHI, diary+desktop logs; N=11.
//!   Finding: longer/more interposed tasks impair resumption. Limitation:
//!   small/old sample. Change: break reason and resume latency. Confidence:
//!   medium-high.
//! - Dabbish, Mark & Gonzalez, 2011, CHI, activity logs/observation/probes;
//!   N=14. Finding: many switches were self-initiated. Limitation: small,
//!   non-developer sample. Change: neutral switch copy; no causal blame.
//!   Confidence: medium-high.
//! - Leroy, 2009, OBHDP, four studies. Signal: manipulated task completion
//!   and performance. Finding: unfinished-task attention residue harms the
//!   next task. Limitation: theme was known, while we infer it. Change:
//!   different explicit tickets break; tool changes alone do not. Confidence:
//!   high.
//! - Meyer et al., 2014, FSE, survey; N=379 developers. Finding: completion,
//!   few interruptions/switches and clear goals matter more than raw hours.
//!   Limitation: perceived productivity. Change: deep minutes/sessions and
//!   fragmentation replace coding ratio as primary metrics. Confidence: high.
//! - Meyer et al., 2017, TSE, logs+ESM; N=20 developers/5,971 ratings.
//!   Finding: productive days had more progress and fewer switches. Limitation:
//!   one small organization. Change: temporal detector, not category sums.
//!   Confidence: high.
//! - Puranik, Koopman & Vough, 2020, Journal of Management, integrative
//!   review. Finding: interruption effects depend on timing/content/control.
//!   Limitation: heterogeneous occupations. Change: distinguish hard breaks
//!   from one interval of sensor uncertainty. Confidence: high.
//! - Parry et al., 2021, Nature Human Behaviour, systematic review/meta-
//!   analysis; 106 studies/N=52,007. Finding: logged vs reported media use
//!   association about r=.38. Limitation: media use, not focus. Change: call
//!   this an observed proxy and validate on labelled timelines. Confidence:
//!   high.
//! - Albulescu et al., 2022, PLOS ONE, micro-break meta-analysis. Signal:
//!   experimental performance/well-being; 22 samples/N=2,335. Finding:
//!   vigor d=.36 and fatigue d=.35, but overall performance d=.16, p=.116.
//!   Limitation: heterogeneous laboratory/student studies. Change: Idle/break
//!   time is neutral and never counted as distraction. Confidence: high.
//! - Razi et al., 2022, PACM HCI/CSCW, quantified workplace and burnout.
//!   Signal: interviews/design probes; N=11 residents plus 5 residents/5
//!   attendings. Finding: manager access raised autonomy/retaliation concerns;
//!   small-team aggregation was not perceived as anonymous. Limitation: one
//!   medical program and speculative dashboard. Change: local/personal data,
//!   inspectable proxy, no manager ranking. Confidence: high for design risk.
//! - Lee et al., 2023, ICML, Pix2Struct, and Baechler et al., 2024, ScreenAI.
//!   Signal: screenshot understanding benchmarks; 282M/1.3B and 675M/1.88B/
//!   4.62B models. Finding: screen-specific pretraining and scale help, while
//!   OCR/metadata can still add up to ~4.5 points; pixel-only DocVQA trailed
//!   OCR (76.6 vs 84.7). Limitation: not workplace-category classification.
//!   Change: app/title/task priors precede VL; model swap requires an in-domain
//!   critical-category eval under the local resource budget. Confidence: high.

use chrono::{Duration, NaiveDate, NaiveDateTime, Timelike};
use rusqlite::{params, Connection};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

pub const POLICY_VERSION: &str = "deep-focus-v1";
/// Transparent reporting tiers, not biological or phenomenological cut-offs.
/// Twenty-five minutes is a legible product reference; 10/50 distinguish
/// short and extended blocks without introducing the unsupported 90m myth.
pub const FOCUSED_TIER_SECS: i64 = 10 * 60;
pub const DEEP_TIER_SECS: i64 = 25 * 60;
pub const EXTENDED_TIER_SECS: i64 = 50 * 60;
/// At the 30–60s capture cadence, at most one uncertain General/Idle sample
/// may be bridged. Its seconds are never counted as focus.
pub const SENSOR_GRACE_SECS: i64 = 90;
pub const BROWSING_DISTRACTION_MIN_SECS: i64 = 2 * 60;
pub const CONSTRUCT_LABEL: &str = "Sustained focus-eligible work without an observed theme change";
pub const PROXY_DISCLAIMER: &str = "Deep Focus is inferred from sustained local activity and explicit theme changes when available; unlabelled theme changes may be missed, and subjective flow is not detected.";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FocusRole {
    Eligible,
    /// Valuable collaborative or planning work. It breaks a sustained-focus
    /// session, but remains evidence for workload and transition analysis.
    Coordination,
    /// Valuable administrative or commercial work. Excluded from Deep Focus,
    /// not labelled as distraction.
    Operational,
    Distraction,
    MeasurementNoise,
    Unknown,
}

/// Single category registry shared by capture parsing, persistence, session
/// segmentation, insights and prompt generation. Adding a category anywhere
/// else would reintroduce the taxonomy drift this module is meant to prevent.
const CATEGORY_POLICIES: &[(&str, &str, FocusRole)] = &[
    ("analysis", "Analysis", FocusRole::Eligible),
    ("writing", "Writing", FocusRole::Eligible),
    ("coding", "Coding", FocusRole::Eligible),
    ("debugging", "Debugging", FocusRole::Eligible),
    ("codereview", "CodeReview", FocusRole::Eligible),
    ("testing", "Testing", FocusRole::Eligible),
    ("documentation", "Documentation", FocusRole::Eligible),
    ("design", "Design", FocusRole::Eligible),
    ("planning", "Planning", FocusRole::Coordination),
    ("meeting", "Meeting", FocusRole::Coordination),
    ("communication", "Communication", FocusRole::Coordination),
    ("research", "Research", FocusRole::Eligible),
    ("learning", "Learning", FocusRole::Eligible),
    ("devops", "DevOps", FocusRole::Eligible),
    ("database", "Database", FocusRole::Eligible),
    ("sales", "Sales", FocusRole::Operational),
    ("admin", "Admin", FocusRole::Operational),
    ("browsing", "Browsing", FocusRole::Distraction),
    ("idle", "Idle", FocusRole::MeasurementNoise),
    ("general", "General", FocusRole::MeasurementNoise),
];

fn normalized_category_key(value: &str) -> String {
    value
        .chars()
        .filter(|character| character.is_alphanumeric())
        .flat_map(char::to_lowercase)
        .collect()
}

fn category_policy(value: &str) -> Option<&'static (&'static str, &'static str, FocusRole)> {
    // Captured and persisted samples already use canonical labels. Avoid
    // allocating a normalized string every time the detector reads their role.
    if let Some(policy) = CATEGORY_POLICIES
        .iter()
        .find(|(_, label, _)| *label == value)
    {
        return Some(policy);
    }
    let normalized = normalized_category_key(value);
    CATEGORY_POLICIES
        .iter()
        .find(|(key, _, _)| *key == normalized)
}

pub fn canonical_category_label(value: &str) -> Option<&'static str> {
    category_policy(value).map(|(_, label, _)| *label)
}

pub fn canonicalize_category(value: &str) -> String {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        "General".to_string()
    } else {
        canonical_category_label(trimmed)
            .unwrap_or(trimmed)
            .to_string()
    }
}

pub fn allowed_categories_prompt() -> String {
    CATEGORY_POLICIES
        .iter()
        .map(|(_, label, _)| *label)
        .collect::<Vec<_>>()
        .join(", ")
}

pub fn focus_role(category: &str) -> FocusRole {
    category_policy(category)
        .map(|(_, _, role)| *role)
        .unwrap_or(FocusRole::Unknown)
}

#[derive(Debug, Clone)]
pub struct ActivitySample {
    /// Start of the observed interval in local time.
    pub start: NaiveDateTime,
    pub duration_seconds: i64,
    pub category: String,
    #[allow(dead_code)]
    // retained as raw evidence; category correction happens before segmentation
    pub description: String,
    pub ticket: Option<String>,
    /// Explicit task selected by the user; this is not tied to Jira/Linear.
    pub theme_hint: Option<String>,
    #[allow(dead_code)] // retained for explainability/eval, not used as a focus rule
    pub app_name: Option<String>,
    #[allow(dead_code)] // retained for explainability/eval, not used as a focus rule
    pub window_title: Option<String>,
}

/// Inclusive local-date range represented as a half-open timestamp window.
/// `reports.created_at` is the observation end, so every consumer must clip
/// the observed interval rather than assigning all seconds to that end date.
#[derive(Debug, Clone, Copy)]
pub(crate) struct LocalDateWindow {
    start: NaiveDateTime,
    end_exclusive: NaiveDateTime,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct LocalIntervalSlice {
    pub start: NaiveDateTime,
    pub duration_seconds: i64,
}

impl LocalDateWindow {
    pub(crate) fn parse(period_start: &str, period_end: &str) -> Result<Self, String> {
        let start_date = NaiveDate::parse_from_str(period_start, "%Y-%m-%d")
            .map_err(|error| error.to_string())?;
        let end_date =
            NaiveDate::parse_from_str(period_end, "%Y-%m-%d").map_err(|error| error.to_string())?;
        if end_date < start_date {
            return Err("period end precedes period start".to_string());
        }
        let start = start_date
            .and_hms_opt(0, 0, 0)
            .ok_or("invalid period start")?;
        let end_exclusive = end_date
            .succ_opt()
            .and_then(|date| date.and_hms_opt(0, 0, 0))
            .ok_or("invalid period end")?;
        Ok(Self {
            start,
            end_exclusive,
        })
    }

    /// Clips one end-timestamped observation to this date window and returns
    /// one slice per local calendar day. Invalid/zero/out-of-window rows do not
    /// contribute time; callers may still retain their raw narrative evidence.
    pub(crate) fn slices_for_observation(
        &self,
        observed_end_local: &str,
        duration_seconds: i64,
    ) -> Vec<LocalIntervalSlice> {
        if duration_seconds <= 0 {
            return Vec::new();
        }
        let Ok(observed_end) =
            NaiveDateTime::parse_from_str(observed_end_local, "%Y-%m-%d %H:%M:%S")
        else {
            return Vec::new();
        };
        let mut cursor = (observed_end - Duration::seconds(duration_seconds)).max(self.start);
        let clipped_end = observed_end.min(self.end_exclusive);
        if clipped_end <= cursor {
            return Vec::new();
        }

        let mut slices = Vec::new();
        while cursor < clipped_end {
            let next_midnight = cursor
                .date()
                .succ_opt()
                .and_then(|date| date.and_hms_opt(0, 0, 0))
                .unwrap_or(clipped_end);
            let slice_end = clipped_end.min(next_midnight);
            slices.push(LocalIntervalSlice {
                start: cursor,
                duration_seconds: (slice_end - cursor).num_seconds(),
            });
            cursor = slice_end;
        }
        slices
    }

    pub(crate) fn contains_local_timestamp(&self, timestamp: &str) -> bool {
        NaiveDateTime::parse_from_str(timestamp, "%Y-%m-%d %H:%M:%S")
            .is_ok_and(|value| value >= self.start && value < self.end_exclusive)
    }
}

fn clean_theme_value(value: &str) -> Option<String> {
    let cleaned = value.split_whitespace().collect::<Vec<_>>().join(" ");
    let normalized_sentinel = cleaned
        .chars()
        .filter(|character| character.is_alphanumeric())
        .flat_map(char::to_lowercase)
        .collect::<String>();
    (!cleaned.is_empty()
        && !matches!(
            normalized_sentinel.as_str(),
            "general" | "noticket" | "generalnoticket"
        ))
    .then_some(cleaned)
}

pub(crate) fn canonical_ticket_value(ticket: Option<&str>) -> Option<String> {
    ticket.and_then(clean_theme_value)
}

pub(crate) fn canonical_theme_label(
    ticket: Option<&str>,
    theme_hint: Option<&str>,
) -> Option<String> {
    if let Some(ticket) = canonical_ticket_value(ticket) {
        return Some(format!("Ticket {ticket}"));
    }
    theme_hint
        .and_then(clean_theme_value)
        .map(|theme| format!("Task {theme}"))
}

impl ActivitySample {
    pub fn end(&self) -> NaiveDateTime {
        self.start + Duration::seconds(self.duration_seconds.max(0))
    }

    fn explicit_theme_key(&self) -> Option<String> {
        if let Some(ticket) = canonical_ticket_value(self.ticket.as_deref()) {
            return Some(format!("ticket:{}", ticket.to_lowercase()));
        }
        if let Some(theme) = self.theme_hint.as_deref().and_then(clean_theme_value) {
            return Some(format!("task:{}", theme.to_lowercase()));
        }
        None
    }

    fn explicit_theme_label(&self) -> Option<String> {
        canonical_theme_label(self.ticket.as_deref(), self.theme_hint.as_deref())
    }

    fn is_focus_eligible(&self) -> bool {
        focus_role(&self.category) == FocusRole::Eligible
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum BreakReason {
    NonFocus,
    MeetingOrCommunication,
    ThemeChanged,
    Gap,
    Midnight,
    EndOfWindow,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CategorySeconds {
    pub category: String,
    pub seconds: i64,
}

#[derive(Debug, Clone, Serialize)]
pub struct DistractionAppDay {
    pub date: String,
    pub seconds: i64,
}

#[derive(Debug, Clone, Serialize)]
pub struct DistractionAppRow {
    pub app_name: String,
    pub seconds: i64,
    /// An app can appear in the same qualifying episode as another app, so
    /// these per-app counts are not additive across rows.
    pub episodes: usize,
    pub days: usize,
    pub transitions_from_focus: usize,
    pub daily_seconds: Vec<DistractionAppDay>,
}

#[derive(Debug, Clone, Serialize)]
pub struct DistractionAppAnalysis {
    pub qualifying_episodes: usize,
    pub qualifying_seconds: i64,
    pub attributed_seconds: i64,
    pub unattributed_seconds: i64,
    pub other_app_seconds: i64,
    pub apps: Vec<DistractionAppRow>,
    /// Named foreground destinations inferred locally from captured screen
    /// context. These are visits, not exact browser tab/open events.
    pub detours: Vec<ContextDetourRow>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ContextDetourDay {
    pub date: String,
    pub visits: usize,
    pub seconds: i64,
    pub work_interleaved_revisits: usize,
    pub shortest_revisit_minutes: Option<i64>,
    /// Local clock times of the first captured frame in each observed visit.
    pub observed_at: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ContextDetourRow {
    pub label: String,
    pub kind: String,
    pub seconds: i64,
    pub visits: usize,
    pub days: usize,
    pub work_interleaved_revisits: usize,
    pub daily_visits: Vec<ContextDetourDay>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FocusSession {
    pub start: String,
    pub end: String,
    pub focus_seconds: i64,
    pub elapsed_seconds: i64,
    pub tier: String,
    pub category_mix: Vec<CategorySeconds>,
    pub theme: Option<String>,
    /// True when the block contains a tolerated uncertain observation or ends
    /// at an observed non-focus/theme boundary. `break_reason` distinguishes
    /// that evidence from an unknown gap or an administrative day/window end.
    pub interrupted: bool,
    pub bridged_noise_seconds: i64,
    pub break_reason: BreakReason,
    pub break_category: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HourlyDeepFocus {
    pub hour: u8,
    pub seconds: i64,
}

#[derive(Debug, Clone, Serialize)]
pub struct FocusSummary {
    pub policy_version: &'static str,
    pub construct_label: &'static str,
    pub proxy_disclaimer: &'static str,
    pub focused_threshold_seconds: i64,
    pub deep_threshold_seconds: i64,
    pub extended_threshold_seconds: i64,
    pub sensor_grace_seconds: i64,
    pub browsing_distraction_min_seconds: i64,
    pub focus_eligible_seconds: i64,
    pub explicit_theme_seconds: i64,
    pub explicit_theme_coverage_pct: f64,
    pub deep_focus_seconds: i64,
    pub deep_focus_sessions: usize,
    pub longest_focus_seconds: i64,
    pub theme_switches: usize,
    pub explicit_theme_switches_per_labelled_focus_hour: f64,
    pub fragmentation_pct: f64,
    pub resume_events: usize,
    pub average_resume_seconds: Option<f64>,
    pub distraction_events: usize,
    pub distraction_seconds: i64,
    /// Seconds discarded from ambiguous overlapping observations rather than
    /// double-counted. Exposed so coverage limitations remain inspectable.
    pub overlap_clipped_seconds: i64,
    /// Work that informs workload/collaboration conclusions but is outside the
    /// sustained-focus construct. It is deliberately not called distraction.
    pub context_work_seconds: i64,
    pub context_category_mix: Vec<CategorySeconds>,
    pub hourly_deep_focus: Vec<HourlyDeepFocus>,
    pub sessions: Vec<FocusSession>,
}

#[derive(Debug)]
struct SessionBuilder {
    start: NaiveDateTime,
    last_focus_end: NaiveDateTime,
    focus_seconds: i64,
    category_mix: BTreeMap<String, i64>,
    theme_key: Option<String>,
    theme: Option<String>,
    bridged_noise_seconds: i64,
    segments: Vec<(NaiveDateTime, i64)>,
}

impl SessionBuilder {
    fn new(sample: &ActivitySample) -> Self {
        let duration = sample.duration_seconds.max(0);
        let mut category_mix = BTreeMap::new();
        category_mix.insert(sample.category.clone(), duration);
        Self {
            start: sample.start,
            last_focus_end: sample.end(),
            focus_seconds: duration,
            category_mix,
            theme_key: sample.explicit_theme_key(),
            theme: sample.explicit_theme_label(),
            bridged_noise_seconds: 0,
            segments: vec![(sample.start, duration)],
        }
    }

    fn push(&mut self, sample: &ActivitySample, bridged_seconds: i64) {
        let duration = sample.duration_seconds.max(0);
        self.focus_seconds += duration;
        *self
            .category_mix
            .entry(sample.category.clone())
            .or_default() += duration;
        self.last_focus_end = sample.end();
        self.bridged_noise_seconds += bridged_seconds.max(0);
        self.segments.push((sample.start, duration));
        if self.theme_key.is_none() {
            self.theme_key = sample.explicit_theme_key();
            self.theme = sample.explicit_theme_label();
        }
    }
}

fn explicit_theme_changed(a: &SessionBuilder, b: &ActivitySample) -> bool {
    match (a.theme_key.as_deref(), b.explicit_theme_key()) {
        (Some(left), Some(right))
            if (left.starts_with("ticket:") || left.starts_with("task:"))
                && (right.starts_with("ticket:") || right.starts_with("task:")) =>
        {
            left != right
        }
        _ => false,
    }
}

fn tier(seconds: i64) -> &'static str {
    if seconds >= EXTENDED_TIER_SECS {
        "extended"
    } else if seconds >= DEEP_TIER_SECS {
        "deep"
    } else if seconds >= FOCUSED_TIER_SECS {
        "focused"
    } else {
        "fragment"
    }
}

fn finish_session(
    builder: SessionBuilder,
    break_reason: BreakReason,
    break_category: Option<&str>,
    sessions: &mut Vec<FocusSession>,
    hourly: &mut [i64; 24],
) {
    if builder.focus_seconds <= 0 {
        return;
    }
    let interrupted = builder.bridged_noise_seconds > 0
        || matches!(
            &break_reason,
            BreakReason::NonFocus | BreakReason::MeetingOrCommunication | BreakReason::ThemeChanged
        );
    if builder.focus_seconds >= DEEP_TIER_SECS {
        for (start, duration) in &builder.segments {
            split_into_hours(*start, *duration, hourly);
        }
    }
    sessions.push(FocusSession {
        start: builder.start.format("%Y-%m-%d %H:%M:%S").to_string(),
        end: builder
            .last_focus_end
            .format("%Y-%m-%d %H:%M:%S")
            .to_string(),
        focus_seconds: builder.focus_seconds,
        elapsed_seconds: (builder.last_focus_end - builder.start)
            .num_seconds()
            .max(0),
        tier: tier(builder.focus_seconds).to_string(),
        category_mix: builder
            .category_mix
            .into_iter()
            .map(|(category, seconds)| CategorySeconds { category, seconds })
            .collect(),
        theme: builder.theme,
        interrupted,
        bridged_noise_seconds: builder.bridged_noise_seconds,
        break_reason,
        break_category: break_category.map(str::to_string),
    });
}

fn split_into_hours(mut start: NaiveDateTime, mut seconds: i64, hourly: &mut [i64; 24]) {
    while seconds > 0 {
        let next_hour = (start + Duration::hours(1))
            .date()
            .and_hms_opt((start.hour() + 1) % 24, 0, 0)
            .unwrap_or_else(|| {
                start + Duration::seconds(3600 - start.minute() as i64 * 60 - start.second() as i64)
            });
        let room = (next_hour - start).num_seconds().max(1);
        let take = seconds.min(room);
        hourly[start.hour() as usize] += take;
        start += Duration::seconds(take);
        seconds -= take;
    }
}

fn flush_browsing_episode(
    episodes: &mut Vec<std::ops::Range<usize>>,
    active: &mut std::ops::Range<usize>,
    seconds: &mut i64,
) {
    if *seconds >= BROWSING_DISTRACTION_MIN_SECS {
        episodes.push(active.clone());
    }
    active.start = active.end;
    *seconds = 0;
}

fn qualifying_browsing_episodes(samples: &[ActivitySample]) -> Vec<std::ops::Range<usize>> {
    let mut episodes = Vec::new();
    let mut active = 0..0;
    let mut active_seconds = 0i64;
    let mut last_end: Option<NaiveDateTime> = None;

    for (index, sample) in samples.iter().enumerate() {
        let is_distraction = sample.category == "Browsing";
        if !is_distraction {
            flush_browsing_episode(&mut episodes, &mut active, &mut active_seconds);
            last_end = None;
            continue;
        }
        let contiguous = !active.is_empty()
            && last_end.is_some_and(|end| {
                end.date() == sample.start.date()
                    && (sample.start - end).num_seconds().max(0) <= SENSOR_GRACE_SECS
            });
        if !contiguous {
            flush_browsing_episode(&mut episodes, &mut active, &mut active_seconds);
        }
        if active.is_empty() {
            active.start = index;
        }
        active.end = index + 1;
        active_seconds += sample.duration_seconds;
        last_end = Some(sample.end());
    }
    flush_browsing_episode(&mut episodes, &mut active, &mut active_seconds);
    episodes
}

fn summarize_distractions(samples: &[ActivitySample]) -> (usize, i64) {
    let episodes = qualifying_browsing_episodes(samples);
    let seconds = episodes
        .iter()
        .flat_map(|episode| episode.clone())
        .map(|index| samples[index].duration_seconds)
        .sum();
    (episodes.len(), seconds)
}

fn app_identity(app_name: Option<&str>, excluded: &BTreeSet<String>) -> Option<(String, String)> {
    let cleaned = app_name?
        .chars()
        .map(|character| {
            if character.is_control() {
                ' '
            } else {
                character
            }
        })
        .collect::<String>()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ");
    let key = crate::privacy::normalized_application(&cleaned);
    let label = cleaned.chars().take(72).collect::<String>();
    (!key.is_empty() && !excluded.contains(&key)).then_some((key, label))
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct ForegroundDestination {
    key: String,
    label: String,
    kind: &'static str,
    strong_identity: bool,
}

fn description_field<'a>(description: &'a str, name: &str) -> Option<&'a str> {
    description.lines().find_map(|line| {
        let (field, value) = line.trim().split_once(':')?;
        field
            .trim()
            .eq_ignore_ascii_case(name)
            .then_some(value.trim())
    })
}

fn clean_destination_label(raw: &str) -> Option<String> {
    let cleaned = raw
        .trim()
        .trim_matches(|character: char| matches!(character, '"' | '\'' | '`' | '.' | ':'))
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ");
    let cleaned = cleaned
        .strip_suffix("'s")
        .or_else(|| cleaned.strip_suffix("’s"))
        .unwrap_or(&cleaned)
        .to_string();
    let key = crate::privacy::normalized_application(&cleaned);
    if cleaned.is_empty()
        || cleaned.chars().count() > 72
        || cleaned.split_whitespace().count() > 4
        || cleaned.contains("://")
        || cleaned.contains(['/', '\\', '@', '#', '?'])
        || matches!(
            key.as_str(),
            "unknown"
                | "none"
                | "n/a"
                | "not visible"
                | "not identifiable"
                | "browser"
                | "web browser"
                | "website"
                | "app"
                | "application"
                | "the screen"
                | "chat interface"
                | "music streaming app"
                | "android"
                | "java"
                | "windows"
                | "ios"
                | "macos"
                | "linux"
        )
        || is_browser_application(&key)
    {
        return None;
    }
    Some(cleaned)
}

fn native_application_label(raw: &str) -> Option<String> {
    let process = raw.rsplit(['/', '\\']).next()?.trim();
    let lower = process.to_lowercase();
    let without_suffix = if lower.ends_with(".exe") || lower.ends_with(".root") {
        &process[..process.len() - if lower.ends_with(".exe") { 4 } else { 5 }]
    } else {
        process
    };
    clean_destination_label(without_suffix)
}

fn is_name_word(word: &str) -> bool {
    if word.is_empty()
        || matches!(
            word.to_ascii_lowercase().as_str(),
            "the"
                | "a"
                | "an"
                | "user"
                | "viewing"
                | "browsing"
                | "using"
                | "current"
                | "window"
                | "visible"
                | "screen"
                | "this"
                | "that"
                | "in"
                | "on"
        )
    {
        return false;
    }
    word.chars().any(char::is_uppercase)
}

fn legacy_destination_from_line(line: &str) -> Option<String> {
    const AFTER_NAME: &[&str] = &[
        "app",
        "application",
        "interface",
        "homepage",
        "website",
        "site",
        "platform",
        "workspace",
        "webapp",
        "webpage",
        "feed",
        "tab",
        "channel",
        "player",
        "playlist",
        "tracks",
        "videos",
        "video",
        "messages",
        "chat",
        "page",
    ];
    let words = line
        .split_whitespace()
        .map(|word| {
            word.trim_matches(|character: char| {
                !character.is_alphanumeric() && !matches!(character, '.' | '-' | '&')
            })
        })
        .collect::<Vec<_>>();
    for (index, word) in words.iter().enumerate() {
        if !AFTER_NAME.contains(&word.to_ascii_lowercase().as_str()) {
            continue;
        }
        let mut start = index;
        while start > 0 && index - start < 3 && is_name_word(words[start - 1]) {
            start -= 1;
        }
        if start < index {
            if let Some(label) = clean_destination_label(&words[start..index].join(" ")) {
                return Some(label);
            }
        }
    }
    None
}

fn foreground_action_evidence(sample: &ActivitySample) -> String {
    let context = description_field(&sample.description, "WINDOW CONTEXT")
        .or_else(|| description_field(&sample.description, "VISIBLE CONTENT"))
        .unwrap_or_default();
    let action = description_field(&sample.description, "CURRENT ACTION").unwrap_or_default();
    format!("{context} {action}").to_lowercase()
}

fn obvious_work_context(sample: &ActivitySample) -> bool {
    let evidence = foreground_action_evidence(sample);
    [
        "audio production",
        "music production",
        "music composition",
        "audio mixing",
        "arranging a track",
        "midi pattern",
        "editing track",
        "developer verification",
        "app publication",
        "store listing",
        "publishing an app",
        "publishing an application",
        "application package",
        "integrated development environment",
        "code editor",
        "source code",
        " ide ",
        "tutorial",
        "coursework",
    ]
    .iter()
    .any(|term| evidence.contains(term))
}

fn destination_kind(sample: &ActivitySample) -> &'static str {
    let evidence = foreground_action_evidence(sample);
    if sample.category == "Communication" {
        "communication"
    } else if obvious_work_context(sample) {
        "other"
    } else if ["music", "playlist", "song", "playback", "audio player"]
        .iter()
        .any(|term| evidence.contains(term))
    {
        "music"
    } else if ["video", "watching", "shorts", "streaming"]
        .iter()
        .any(|term| evidence.contains(term))
    {
        "video"
    } else if sample.category == "Browsing" {
        "browsing"
    } else {
        "other"
    }
}

fn kind_priority(kind: &str) -> u8 {
    match kind {
        "communication" | "music" | "video" => 2,
        "browsing" => 1,
        _ => 0,
    }
}

fn is_browser_application(app: &str) -> bool {
    matches!(
        app,
        "arc"
            | "chrome"
            | "google chrome"
            | "msedge"
            | "microsoft edge"
            | "firefox"
            | "vivaldi"
            | "safari"
            | "brave"
            | "opera"
            | "chromium"
            | "browser"
            | "web browser"
            | "arc browser"
            | "chrome browser"
    )
}

fn is_generic_application_container(app: &str) -> bool {
    matches!(
        app,
        "applicationframehost" | "electron" | "msedgewebview2" | "runtimebroker" | "unknown"
    )
}

fn foreground_destination_identity(
    sample: &ActivitySample,
    excluded: &BTreeSet<String>,
) -> Option<ForegroundDestination> {
    if sample.category == "Idle" {
        return None;
    }
    let app = sample
        .app_name
        .as_deref()
        .or_else(|| description_field(&sample.description, "APP"))
        .unwrap_or_default();
    let app_key =
        crate::privacy::normalized_application(app.rsplit(['/', '\\']).next().unwrap_or(app));
    if excluded.contains(&app_key) {
        return None;
    }
    let kind = destination_kind(sample);
    if !app_key.is_empty()
        && !is_browser_application(&app_key)
        && !is_generic_application_container(&app_key)
    {
        if let Some(label) = native_application_label(app) {
            let key = crate::privacy::normalized_application(&label);
            return (!excluded.contains(&key)).then_some(ForegroundDestination {
                key,
                label,
                kind,
                strong_identity: true,
            });
        }
    }

    // New local vision captures provide a product/site label without an
    // account, title, or URL. The browser process itself is never the label.
    if let Some(label) = description_field(&sample.description, "FOREGROUND DESTINATION")
        .and_then(clean_destination_label)
    {
        let key = crate::privacy::normalized_application(&label);
        return (!excluded.contains(&key)).then_some(ForegroundDestination {
            key,
            label,
            kind,
            strong_identity: true,
        });
    }

    // Legacy summaries predate that field. Extract only a proper product name
    // immediately attached to a foreground UI noun, never arbitrary words in
    // visible content or a discussion about another service.
    let context = description_field(&sample.description, "WINDOW CONTEXT")
        .or_else(|| description_field(&sample.description, "VISIBLE CONTENT"))
        .unwrap_or_default();
    let context_lower = context.to_lowercase();
    if [
        "discussing",
        "messages about",
        "conversation about",
        "no active window",
        "application icons",
        "desktop shows",
    ]
    .iter()
    .any(|term| context_lower.contains(term))
        || (context_lower.contains("desktop")
            && ["icon", "no open window", "start menu", "wallpaper"]
                .iter()
                .any(|term| context_lower.contains(term)))
    {
        return None;
    }
    let from_context = legacy_destination_from_line(context);
    if from_context.is_none()
        && [
            "file explorer",
            "finder",
            "chat interface",
            "conversation",
            "messages about",
        ]
        .iter()
        .any(|term| context_lower.contains(term))
    {
        return None;
    }
    let label = from_context.or_else(|| {
        description_field(&sample.description, "CURRENT ACTION")
            .and_then(legacy_destination_from_line)
    })?;
    let key = crate::privacy::normalized_application(&label);
    (!excluded.contains(&key)).then_some(ForegroundDestination {
        key,
        label,
        kind,
        strong_identity: false,
    })
}

/// A short public app/site label for an opt-in system notification. Never use
/// legacy guesses, raw window titles, URLs or the browser process as copy.
pub(crate) fn notification_destination(sample: &ActivitySample) -> Option<String> {
    if sample.category != "Browsing" {
        return None;
    }
    let destination = foreground_destination_identity(sample, &BTreeSet::new())?;
    if !destination.strong_identity || destination.label.chars().count() > 40 {
        return None;
    }
    let app_key = sample
        .app_name
        .as_deref()
        .map(|app| {
            crate::privacy::normalized_application(app.rsplit(['/', '\\']).next().unwrap_or(app))
        })
        .unwrap_or_default();
    let browser_or_unknown = app_key.is_empty()
        || is_browser_application(&app_key)
        || is_generic_application_container(&app_key);
    if browser_or_unknown
        && !matches!(
            destination.key.as_str(),
            "youtube"
                | "youtube shorts"
                | "tiktok"
                | "instagram"
                | "reddit"
                | "twitch"
                | "netflix"
                | "facebook"
                | "twitter"
                | "pinterest"
                | "discord"
        )
    {
        return None;
    }
    if destination.key == "youtube"
        && description_field(&sample.description, "CURRENT ACTION").is_some_and(|action| {
            let action = action.to_ascii_lowercase();
            action.contains("shorts") || action.contains("short videos")
        })
    {
        return Some("YouTube Shorts".into());
    }
    Some(destination.label)
}

/// Prefix snapshots of the two most recent focus observations with distinct
/// destinations. A revisit can check its intervening work in constant time,
/// without rescanning the same timeline for each destination.
struct WorkContextIndex<'a> {
    destinations: &'a [Option<ForegroundDestination>],
    // Store positions plus one; zero means absent. This halves the storage
    // compared with a pair of Option<usize> per observation.
    latest: Vec<(usize, usize)>,
}

impl<'a> WorkContextIndex<'a> {
    fn new(samples: &[ActivitySample], destinations: &'a [Option<ForegroundDestination>]) -> Self {
        let mut index = Self {
            destinations,
            latest: Vec::with_capacity(samples.len() + 1),
        };
        let (mut latest, mut different) = (None, None);
        index.latest.push((0, 0));
        for (position, sample) in samples.iter().enumerate() {
            if sample.is_focus_eligible() {
                if latest.is_some_and(|previous| {
                    index.destination_key(previous) != index.destination_key(position)
                }) {
                    different = latest;
                }
                latest = Some(position);
            }
            index.latest.push((
                latest.map_or(0, |position| position + 1),
                different.map_or(0, |position| position + 1),
            ));
        }
        index
    }

    fn destination_key(&self, position: usize) -> Option<&str> {
        self.destinations[position]
            .as_ref()
            .map(|destination| destination.key.as_str())
    }

    fn has_other_destination(&self, start: usize, end: usize, key: &str) -> bool {
        let (latest, different) = self.latest[end];
        let candidate = if latest > 0 && self.destination_key(latest - 1) != Some(key) {
            latest
        } else {
            different
        };
        candidate > start
    }
}

fn analyze_context_detours(
    samples: &[ActivitySample],
    excluded: &BTreeSet<String>,
) -> Vec<ContextDetourRow> {
    struct Visit {
        key: String,
        label: String,
        kind: &'static str,
        date: String,
        start: NaiveDateTime,
        end: NaiveDateTime,
        seconds: i64,
        browsing_seconds: i64,
        interruption_seconds: i64,
        strong_identity: bool,
        observed_at: NaiveDateTime,
        first_index: usize,
        last_index: usize,
    }

    let mut visits = Vec::<Visit>::new();
    let destinations = samples
        .iter()
        .map(|sample| foreground_destination_identity(sample, excluded))
        .collect::<Vec<_>>();
    let work_context = WorkContextIndex::new(samples, &destinations);
    for (index, sample) in samples.iter().enumerate() {
        let Some(destination) = destinations[index].as_ref() else {
            continue;
        };
        let clearly_work = obvious_work_context(sample);
        let browsing_seconds = if sample.category == "Browsing" && !clearly_work {
            sample.duration_seconds
        } else {
            0
        };
        let interruption_seconds = if sample.category == "Communication"
            || (!clearly_work
                && (sample.category == "Browsing"
                    || (sample.category == "General" && destination.strong_identity)))
        {
            sample.duration_seconds
        } else {
            0
        };
        if let Some(last) = visits.last_mut() {
            let unidentified_browsing_bridge = index > last.last_index + 1
                && index - last.last_index <= 3
                && (sample.start - last.end).num_seconds().max(0) <= 3 * 60
                && (last.last_index + 1..index).all(|between| {
                    destinations[between].is_none()
                        && samples[between].category == "Browsing"
                        && samples[between]
                            .app_name
                            .as_deref()
                            .map(crate::privacy::normalized_application)
                            .map_or(true, |app| is_browser_application(&app))
                });
            if last.key == destination.key
                && (last.last_index + 1 == index || unidentified_browsing_bridge)
                && last.end.date() == sample.start.date()
                && (sample.start - last.end).num_seconds().max(0)
                    <= if unidentified_browsing_bridge {
                        3 * 60
                    } else {
                        SENSOR_GRACE_SECS
                    }
            {
                last.end = sample.end();
                last.seconds += sample.duration_seconds;
                last.browsing_seconds += browsing_seconds;
                last.interruption_seconds += interruption_seconds;
                last.strong_identity |= destination.strong_identity;
                if kind_priority(destination.kind) > kind_priority(last.kind) {
                    last.kind = destination.kind;
                }
                last.last_index = index;
                continue;
            }
        }
        visits.push(Visit {
            key: destination.key.clone(),
            label: destination.label.clone(),
            kind: destination.kind,
            date: sample.start.date().format("%Y-%m-%d").to_string(),
            start: sample.start,
            end: sample.end(),
            seconds: sample.duration_seconds,
            browsing_seconds,
            interruption_seconds,
            strong_identity: destination.strong_identity,
            observed_at: sample.end(),
            first_index: index,
            last_index: index,
        });
    }

    let mut grouped = BTreeMap::<String, Vec<&Visit>>::new();
    for visit in &visits {
        grouped.entry(visit.key.clone()).or_default().push(visit);
    }
    let mut rows = Vec::new();
    for (key, group) in grouped {
        // The reporting UI itself is instrumentation, not a useful
        // third-party attention destination.
        if key == "flowsight" || key.starts_with("flowsight ") || key.starts_with("flowsight.") {
            continue;
        }
        let mut daily = BTreeMap::<String, ContextDetourDay>::new();
        for visit in &group {
            let day = daily
                .entry(visit.date.clone())
                .or_insert_with(|| ContextDetourDay {
                    date: visit.date.clone(),
                    visits: 0,
                    seconds: 0,
                    work_interleaved_revisits: 0,
                    shortest_revisit_minutes: None,
                    observed_at: Vec::new(),
                });
            day.visits += 1;
            day.seconds += visit.seconds;
            day.observed_at
                .push(visit.observed_at.format("%H:%M").to_string());
        }
        let mut work_interleaved_revisits = 0;
        for pair in group.windows(2) {
            let previous = pair[0];
            let next = pair[1];
            if previous.date != next.date
                || (next.start - previous.end).num_seconds() > 60 * 60
                || !work_context.has_other_destination(
                    previous.last_index + 1,
                    next.first_index,
                    &previous.key,
                )
            {
                continue;
            }
            work_interleaved_revisits += 1;
            if let Some(day) = daily.get_mut(&next.date) {
                day.work_interleaved_revisits += 1;
                let minutes = (next.start - previous.start).num_minutes().max(1);
                day.shortest_revisit_minutes = Some(
                    day.shortest_revisit_minutes
                        .map_or(minutes, |current| current.min(minutes)),
                );
            }
        }
        let browsing_seconds: i64 = group.iter().map(|visit| visit.browsing_seconds).sum();
        let interruption_seconds: i64 = group.iter().map(|visit| visit.interruption_seconds).sum();
        let strong_identity = group.iter().any(|visit| visit.strong_identity);
        // Repeated transitions are useful even for a work-classified app such
        // as Slack. A one-off app session is shown only if it was sustained
        // casual browsing, not merely because the app existed on screen.
        if interruption_seconds == 0
            || (!strong_identity && browsing_seconds < 2 * 60)
            || (work_interleaved_revisits == 0 && browsing_seconds < 5 * 60)
        {
            continue;
        }
        rows.push(ContextDetourRow {
            label: group[0].label.clone(),
            kind: group
                .iter()
                .max_by_key(|visit| kind_priority(visit.kind))
                .map(|visit| visit.kind)
                .unwrap_or("other")
                .to_string(),
            seconds: group.iter().map(|visit| visit.seconds).sum(),
            visits: group.len(),
            days: daily.len(),
            work_interleaved_revisits,
            daily_visits: daily.into_values().collect(),
        });
    }
    rows.sort_by(|left, right| {
        right
            .work_interleaved_revisits
            .cmp(&left.work_interleaved_revisits)
            .then_with(|| right.visits.cmp(&left.visits))
            .then_with(|| right.seconds.cmp(&left.seconds))
            .then_with(|| left.label.cmp(&right.label))
    });
    rows.truncate(5);
    rows
}

fn analyze_distraction_apps(
    samples: Vec<ActivitySample>,
    excluded_applications: &[String],
) -> DistractionAppAnalysis {
    struct AppAccumulator {
        app_name: String,
        seconds: i64,
        episodes: usize,
        transitions_from_focus: usize,
        daily_seconds: BTreeMap<String, i64>,
    }

    let excluded = excluded_applications
        .iter()
        .map(|app| crate::privacy::normalized_application(app))
        .collect::<BTreeSet<_>>();
    // Current privacy exclusions also remove historical observations from
    // this optional app-level analysis, not merely their display names.
    let samples = samples
        .into_iter()
        .filter(|sample| {
            sample
                .app_name
                .as_deref()
                .map(|app| !excluded.contains(&crate::privacy::normalized_application(app)))
                .unwrap_or(true)
        })
        .collect();
    let (samples, _) = normalize_timeline(samples);
    let detours = analyze_context_detours(&samples, &excluded);
    let mut display_labels = BTreeMap::<String, String>::new();
    for sample in &samples {
        if let Some((key, label)) = app_identity(sample.app_name.as_deref(), &excluded) {
            display_labels.entry(key).or_insert(label);
        }
    }
    let episodes = qualifying_browsing_episodes(&samples);
    let mut apps = BTreeMap::<String, AppAccumulator>::new();
    let mut qualifying_seconds = 0i64;
    let mut attributed_seconds = 0i64;

    for episode in &episodes {
        let first_index = episode.start;
        let first = &samples[first_index];
        let followed_focus = first_index > 0
            && samples[first_index - 1].is_focus_eligible()
            && (first.start - samples[first_index - 1].end())
                .num_seconds()
                .max(0)
                <= SENSOR_GRACE_SECS;
        let first_app_key = app_identity(first.app_name.as_deref(), &excluded).map(|(key, _)| key);
        let mut seen = BTreeSet::new();

        for index in episode.clone() {
            let sample = &samples[index];
            qualifying_seconds += sample.duration_seconds;
            let Some((key, label)) = app_identity(sample.app_name.as_deref(), &excluded) else {
                continue;
            };
            attributed_seconds += sample.duration_seconds;
            let app = apps.entry(key.clone()).or_insert_with(|| AppAccumulator {
                app_name: display_labels.get(&key).cloned().unwrap_or(label),
                seconds: 0,
                episodes: 0,
                transitions_from_focus: 0,
                daily_seconds: BTreeMap::new(),
            });
            app.seconds += sample.duration_seconds;
            *app.daily_seconds
                .entry(sample.start.date().format("%Y-%m-%d").to_string())
                .or_default() += sample.duration_seconds;
            if seen.insert(key.clone()) {
                app.episodes += 1;
                if followed_focus && first_app_key.as_deref() == Some(key.as_str()) {
                    app.transitions_from_focus += 1;
                }
            }
        }
    }

    let mut rows = apps
        .into_values()
        .map(|app| DistractionAppRow {
            app_name: app.app_name,
            seconds: app.seconds,
            episodes: app.episodes,
            days: app.daily_seconds.len(),
            transitions_from_focus: app.transitions_from_focus,
            daily_seconds: app
                .daily_seconds
                .into_iter()
                .map(|(date, seconds)| DistractionAppDay { date, seconds })
                .collect(),
        })
        .collect::<Vec<_>>();
    rows.sort_by(|left, right| {
        right
            .seconds
            .cmp(&left.seconds)
            .then_with(|| left.app_name.cmp(&right.app_name))
    });
    let other_app_seconds = rows.iter().skip(5).map(|row| row.seconds).sum();
    rows.truncate(5);

    DistractionAppAnalysis {
        qualifying_episodes: episodes.len(),
        qualifying_seconds,
        attributed_seconds,
        unattributed_seconds: qualifying_seconds - attributed_seconds,
        other_app_seconds,
        apps: rows,
        detours,
    }
}

fn normalize_timeline(mut samples: Vec<ActivitySample>) -> (Vec<ActivitySample>, i64) {
    samples.retain(|s| s.duration_seconds > 0);
    samples.sort_by_key(|s| s.start);
    let mut normalized = Vec::with_capacity(samples.len());
    let mut covered_until: Option<NaiveDateTime> = None;
    let mut overlap_clipped_seconds = 0;

    for mut sample in samples {
        if let Some(end) = covered_until {
            if sample.start < end {
                let clipped = (end - sample.start)
                    .num_seconds()
                    .min(sample.duration_seconds)
                    .max(0);
                sample.start += Duration::seconds(clipped);
                sample.duration_seconds -= clipped;
                overlap_clipped_seconds += clipped;
            }
        }
        if sample.duration_seconds <= 0 {
            continue;
        }

        while sample.end().date() > sample.start.date() {
            let midnight = sample
                .start
                .date()
                .succ_opt()
                .and_then(|date| date.and_hms_opt(0, 0, 0))
                .expect("valid next midnight");
            let first_seconds = (midnight - sample.start).num_seconds();
            let mut first = sample.clone();
            first.duration_seconds = first_seconds;
            normalized.push(first);
            sample.start = midnight;
            sample.duration_seconds -= first_seconds;
        }
        covered_until = Some(sample.end());
        if sample.duration_seconds > 0 {
            normalized.push(sample);
        }
    }
    (normalized, overlap_clipped_seconds)
}

pub fn summarize(samples: Vec<ActivitySample>) -> FocusSummary {
    let (samples, overlap_clipped_seconds) = normalize_timeline(samples);
    let (distraction_events, distraction_seconds) = summarize_distractions(&samples);

    let focus_eligible_seconds: i64 = samples
        .iter()
        .filter(|s| s.is_focus_eligible())
        .map(|s| s.duration_seconds.max(0))
        .sum();
    let explicit_theme_seconds: i64 = samples
        .iter()
        .filter(|s| s.is_focus_eligible() && s.explicit_theme_key().is_some())
        .map(|s| s.duration_seconds.max(0))
        .sum();
    let explicit_theme_coverage_pct = if focus_eligible_seconds > 0 {
        (explicit_theme_seconds as f64 / focus_eligible_seconds as f64 * 1000.0).round() / 10.0
    } else {
        0.0
    };
    let mut context_categories = BTreeMap::<String, i64>::new();
    for sample in &samples {
        if matches!(
            focus_role(&sample.category),
            FocusRole::Coordination | FocusRole::Operational
        ) {
            *context_categories
                .entry(sample.category.clone())
                .or_default() += sample.duration_seconds.max(0);
        }
    }
    let context_work_seconds = context_categories.values().sum();
    let mut sessions = Vec::new();
    let mut hourly = [0i64; 24];
    let mut current: Option<SessionBuilder> = None;
    let mut pending_noise_seconds = 0i64;
    let mut pending_noise_observations = 0usize;
    let mut theme_switches = 0usize;
    let mut interrupted_theme: Option<(String, NaiveDateTime)> = None;
    let mut resume_latencies = Vec::new();
    let mut last_observation_date = None;

    for sample in &samples {
        if last_observation_date.is_some_and(|date| date != sample.start.date()) {
            if let Some(done) = current.take() {
                finish_session(
                    done,
                    BreakReason::Midnight,
                    None,
                    &mut sessions,
                    &mut hourly,
                );
            }
            pending_noise_seconds = 0;
            pending_noise_observations = 0;
            interrupted_theme = None;
        }
        last_observation_date = Some(sample.start.date());

        if sample.is_focus_eligible() {
            if let Some(active) = current.as_mut() {
                let reason = if explicit_theme_changed(active, sample) {
                    theme_switches += 1;
                    Some(BreakReason::ThemeChanged)
                } else {
                    let gap = (sample.start - active.last_focus_end).num_seconds().max(0);
                    if gap > SENSOR_GRACE_SECS {
                        Some(BreakReason::Gap)
                    } else {
                        None
                    }
                };
                if let Some(reason) = reason {
                    let done = current.take().expect("active session");
                    if reason == BreakReason::Gap
                        && done.theme_key.is_some()
                        && done.theme_key.as_deref() == sample.explicit_theme_key().as_deref()
                    {
                        let latency = (sample.start - done.last_focus_end).num_seconds().max(0);
                        resume_latencies.push(latency);
                    }
                    finish_session(done, reason, None, &mut sessions, &mut hourly);
                    pending_noise_seconds = 0;
                    pending_noise_observations = 0;
                    current = Some(SessionBuilder::new(sample));
                } else {
                    let bridge = pending_noise_seconds.min(SENSOR_GRACE_SECS);
                    active.push(sample, bridge);
                    pending_noise_seconds = 0;
                    pending_noise_observations = 0;
                }
            } else {
                if let (Some((prior_theme, ended)), Some(next_theme)) =
                    (interrupted_theme.take(), sample.explicit_theme_key())
                {
                    if prior_theme == next_theme {
                        resume_latencies.push((sample.start - ended).num_seconds().max(0));
                    } else {
                        theme_switches += 1;
                    }
                }
                current = Some(SessionBuilder::new(sample));
                pending_noise_seconds = 0;
                pending_noise_observations = 0;
            }
            continue;
        }

        match focus_role(&sample.category) {
            FocusRole::MeasurementNoise if current.is_some() => {
                pending_noise_seconds += sample.duration_seconds;
                pending_noise_observations += 1;
                if pending_noise_seconds > SENSOR_GRACE_SECS || pending_noise_observations > 1 {
                    let done = current.take().expect("active session");
                    if let Some(theme_key) = done.theme_key.clone() {
                        interrupted_theme = Some((theme_key, done.last_focus_end));
                    }
                    finish_session(
                        done,
                        BreakReason::NonFocus,
                        Some(sample.category.as_str()),
                        &mut sessions,
                        &mut hourly,
                    );
                    pending_noise_seconds = 0;
                    pending_noise_observations = 0;
                }
            }
            _ => {
                if let Some(done) = current.take() {
                    let reason = if matches!(sample.category.as_str(), "Meeting" | "Communication")
                    {
                        BreakReason::MeetingOrCommunication
                    } else {
                        BreakReason::NonFocus
                    };
                    if let Some(theme_key) = done.theme_key.clone() {
                        interrupted_theme = Some((theme_key, done.last_focus_end));
                    }
                    finish_session(
                        done,
                        reason,
                        Some(sample.category.as_str()),
                        &mut sessions,
                        &mut hourly,
                    );
                }
                pending_noise_seconds = 0;
                pending_noise_observations = 0;
            }
        }
    }
    if let Some(done) = current.take() {
        finish_session(
            done,
            BreakReason::EndOfWindow,
            None,
            &mut sessions,
            &mut hourly,
        );
    }

    let deep_focus_seconds: i64 = sessions
        .iter()
        .filter(|s| s.focus_seconds >= DEEP_TIER_SECS)
        .map(|s| s.focus_seconds)
        .sum();
    let deep_focus_sessions = sessions
        .iter()
        .filter(|s| s.focus_seconds >= DEEP_TIER_SECS)
        .count();
    let longest_focus_seconds = sessions.iter().map(|s| s.focus_seconds).max().unwrap_or(0);
    let fragmented_seconds: i64 = sessions
        .iter()
        .filter(|s| s.focus_seconds < DEEP_TIER_SECS)
        .map(|s| s.focus_seconds)
        .sum();
    let fragmentation_pct = if focus_eligible_seconds > 0 {
        (fragmented_seconds as f64 / focus_eligible_seconds as f64 * 1000.0).round() / 10.0
    } else {
        0.0
    };
    let explicit_theme_switches_per_labelled_focus_hour = if explicit_theme_seconds > 0 {
        (theme_switches as f64 / (explicit_theme_seconds as f64 / 3600.0) * 10.0).round() / 10.0
    } else {
        0.0
    };
    let average_resume_seconds = if resume_latencies.is_empty() {
        None
    } else {
        Some(
            (resume_latencies.iter().sum::<i64>() as f64 / resume_latencies.len() as f64 * 10.0)
                .round()
                / 10.0,
        )
    };

    FocusSummary {
        policy_version: POLICY_VERSION,
        construct_label: CONSTRUCT_LABEL,
        proxy_disclaimer: PROXY_DISCLAIMER,
        focused_threshold_seconds: FOCUSED_TIER_SECS,
        deep_threshold_seconds: DEEP_TIER_SECS,
        extended_threshold_seconds: EXTENDED_TIER_SECS,
        sensor_grace_seconds: SENSOR_GRACE_SECS,
        browsing_distraction_min_seconds: BROWSING_DISTRACTION_MIN_SECS,
        focus_eligible_seconds,
        explicit_theme_seconds,
        explicit_theme_coverage_pct,
        deep_focus_seconds,
        deep_focus_sessions,
        longest_focus_seconds,
        theme_switches,
        explicit_theme_switches_per_labelled_focus_hour,
        fragmentation_pct,
        resume_events: resume_latencies.len(),
        average_resume_seconds,
        distraction_events,
        distraction_seconds,
        overlap_clipped_seconds,
        context_work_seconds,
        context_category_mix: context_categories
            .into_iter()
            .map(|(category, seconds)| CategorySeconds { category, seconds })
            .collect(),
        hourly_deep_focus: hourly
            .iter()
            .enumerate()
            .map(|(hour, seconds)| HourlyDeepFocus {
                hour: hour as u8,
                seconds: *seconds,
            })
            .collect(),
        sessions,
    }
}

pub fn summarize_from_db(
    conn: &Connection,
    period_start: &str,
    period_end: &str,
) -> Result<FocusSummary, String> {
    Ok(summarize(load_samples_from_db(
        conn,
        period_start,
        period_end,
    )?))
}

pub fn distraction_apps_from_db(
    conn: &Connection,
    period_start: &str,
    period_end: &str,
    excluded_applications: &[String],
) -> Result<DistractionAppAnalysis, String> {
    Ok(analyze_distraction_apps(
        load_samples_from_db(conn, period_start, period_end)?,
        excluded_applications,
    ))
}

fn load_samples_from_db(
    conn: &Connection,
    period_start: &str,
    period_end: &str,
) -> Result<Vec<ActivitySample>, String> {
    let window = LocalDateWindow::parse(period_start, period_end)?;
    let mut stmt = conn
        .prepare(
            "SELECT datetime(created_at, 'localtime'), activity_type, description,
                    jira_ticket_id, duration_seconds, active_app, window_title, theme_hint
             FROM reports
             WHERE date(created_at, 'localtime') >= ?1
               AND date(created_at, 'localtime') <= date(?2, '+1 day')
             ORDER BY datetime(created_at, 'localtime') ASC",
        )
        .map_err(|e| e.to_string())?;
    let samples = stmt
        .query_map(params![period_start, period_end], |row| {
            let observed_end: String = row.get(0)?;
            let duration_seconds: i64 = row.get::<_, i64>(4).unwrap_or(0).max(0);
            Ok((
                observed_end,
                duration_seconds,
                canonicalize_category(
                    &row.get::<_, String>(1).unwrap_or_else(|_| "General".into()),
                ),
                row.get::<_, String>(2).unwrap_or_default(),
                row.get::<_, Option<String>>(3).unwrap_or(None),
                row.get::<_, Option<String>>(5).unwrap_or(None),
                row.get::<_, Option<String>>(6).unwrap_or(None),
                row.get::<_, Option<String>>(7).unwrap_or(None),
            ))
        })
        .map_err(|e| e.to_string())?
        .filter_map(|row| row.ok())
        .flat_map(
            |(observed_end, duration_seconds, category, description, ticket, app, title, theme)| {
                window
                    .slices_for_observation(&observed_end, duration_seconds)
                    .into_iter()
                    .map(move |slice| ActivitySample {
                        start: slice.start,
                        duration_seconds: slice.duration_seconds,
                        category: category.clone(),
                        description: description.clone(),
                        ticket: ticket.clone(),
                        theme_hint: theme.clone(),
                        app_name: app.clone(),
                        window_title: title.clone(),
                    })
            },
        )
        .collect();
    Ok(samples)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn category_fast_path_preserves_aliases_and_unknown_values() {
        for (key, label, role) in CATEGORY_POLICIES {
            for value in [
                label.to_string(),
                key.to_string(),
                format!("  {}  ", label.to_uppercase()),
                label
                    .chars()
                    .map(|character| format!("{character}-_"))
                    .collect(),
            ] {
                assert_eq!(canonical_category_label(&value), Some(*label), "{value}");
                assert_eq!(focus_role(&value), *role, "{value}");
            }
        }
        for value in ["", " ", "Custom category", "研发", "códing"] {
            assert_eq!(canonical_category_label(value), None);
            assert_eq!(focus_role(value), FocusRole::Unknown);
        }
    }

    #[test]
    fn indexed_work_context_matches_scanning_every_interval() {
        // All four-observation timelines with focus/non-focus and known/unknown
        // destinations, including same-destination work that must not count.
        for mut pattern in 0..6usize.pow(4) {
            let mut samples = Vec::new();
            let mut destinations = Vec::new();
            for position in 0..4 {
                let state = pattern % 6;
                pattern /= 6;
                samples.push(sample(
                    22,
                    9,
                    position,
                    60,
                    if state < 3 { "Research" } else { "Browsing" },
                    None,
                ));
                destinations.push(match state % 3 {
                    0 => None,
                    key => Some(ForegroundDestination {
                        key: key.to_string(),
                        label: key.to_string(),
                        kind: "other",
                        strong_identity: true,
                    }),
                });
            }
            let index = WorkContextIndex::new(&samples, &destinations);
            for start in 0..=samples.len() {
                for end in start..=samples.len() {
                    for key in ["1", "2", "absent"] {
                        let scanned = (start..end).any(|position| {
                            samples[position].is_focus_eligible()
                                && destinations[position]
                                    .as_ref()
                                    .map(|destination| destination.key.as_str())
                                    != Some(key)
                        });
                        assert_eq!(index.has_other_destination(start, end, key), scanned);
                    }
                }
            }
        }
    }

    fn at(day: u32, hour: u32, minute: u32) -> NaiveDateTime {
        chrono::NaiveDate::from_ymd_opt(2026, 8, day)
            .unwrap()
            .and_hms_opt(hour, minute, 0)
            .unwrap()
    }

    fn sample(
        day: u32,
        hour: u32,
        minute: u32,
        seconds: i64,
        category: &str,
        ticket: Option<&str>,
    ) -> ActivitySample {
        ActivitySample {
            start: at(day, hour, minute),
            duration_seconds: seconds,
            category: category.into(),
            description: "technical work in VS Code".into(),
            ticket: ticket.map(str::to_string),
            theme_hint: None,
            app_name: Some("Visual Studio Code".into()),
            window_title: Some("main.rs - project".into()),
        }
    }

    #[test]
    fn same_ticket_coding_to_debugging_is_one_deep_session() {
        let out = summarize(vec![
            sample(22, 9, 0, 900, "Coding", Some("FS-1")),
            sample(22, 9, 15, 900, "Debugging", Some("FS-1")),
        ]);
        assert_eq!(out.deep_focus_sessions, 1);
        assert_eq!(out.deep_focus_seconds, 1800);
        assert_eq!(out.sessions[0].theme.as_deref(), Some("Ticket FS-1"));
    }

    #[test]
    fn ticket_change_breaks_attention_residue_boundary() {
        let out = summarize(vec![
            sample(22, 9, 0, 900, "Coding", Some("FS-1")),
            sample(22, 9, 15, 900, "Coding", Some("FS-2")),
        ]);
        assert_eq!(out.deep_focus_sessions, 0);
        assert_eq!(out.theme_switches, 1);
        assert_eq!(out.sessions.len(), 2);
        assert_eq!(out.sessions[0].break_reason, BreakReason::ThemeChanged);
        assert!(out.sessions[0].interrupted);
    }

    #[test]
    fn one_interval_general_noise_is_bridged_but_not_counted() {
        let out = summarize(vec![
            sample(22, 9, 0, 720, "Coding", Some("FS-1")),
            sample(22, 9, 12, 60, "General", Some("FS-1")),
            sample(22, 9, 13, 840, "Testing", Some("FS-1")),
        ]);
        assert_eq!(out.deep_focus_sessions, 1);
        assert_eq!(out.deep_focus_seconds, 1560);
        assert_eq!(out.sessions[0].bridged_noise_seconds, 60);
        assert!(out.sessions[0].interrupted);
    }

    #[test]
    fn real_browsing_and_meeting_cut_immediately() {
        for breaker in ["Browsing", "Meeting", "Communication"] {
            let out = summarize(vec![
                sample(22, 9, 0, 900, "Coding", Some("FS-1")),
                sample(22, 9, 15, 30, breaker, Some("FS-1")),
                sample(22, 9, 16, 900, "Coding", Some("FS-1")),
            ]);
            assert_eq!(out.deep_focus_sessions, 0, "breaker={breaker}");
            assert!(out.sessions[0].interrupted, "breaker={breaker}");
            let expected_reason = if matches!(breaker, "Meeting" | "Communication") {
                BreakReason::MeetingOrCommunication
            } else {
                BreakReason::NonFocus
            };
            assert_eq!(out.sessions[0].break_reason, expected_reason);
            assert_eq!(out.sessions[0].break_category.as_deref(), Some(breaker));
        }
    }

    #[test]
    fn zero_second_annotations_do_not_break_or_add_time() {
        let mut event = sample(22, 9, 10, 0, "Communication", Some("FS-1"));
        event.description = "UIA annotation".into();
        let out = summarize(vec![
            sample(22, 9, 0, 900, "Coding", Some("FS-1")),
            event,
            sample(22, 9, 15, 900, "Testing", Some("FS-1")),
        ]);
        assert_eq!(out.deep_focus_sessions, 1);
        assert_eq!(out.deep_focus_seconds, 1800);
    }

    #[test]
    fn midnight_and_large_gap_split_sessions() {
        let midnight = summarize(vec![
            sample(22, 23, 45, 900, "Coding", Some("FS-1")),
            sample(23, 0, 0, 900, "Coding", Some("FS-1")),
        ]);
        assert_eq!(midnight.deep_focus_sessions, 0);
        assert_eq!(midnight.sessions[0].break_reason, BreakReason::Midnight);

        let gap = summarize(vec![
            sample(22, 9, 0, 600, "Coding", Some("FS-1")),
            sample(22, 9, 20, 600, "Coding", Some("FS-1")),
        ]);
        assert_eq!(gap.sessions.len(), 2);
        assert_eq!(gap.sessions[0].break_reason, BreakReason::Gap);
        assert!(!gap.sessions[0].interrupted);
    }

    #[test]
    fn one_observation_crossing_midnight_is_split_between_days() {
        let out = summarize(vec![sample(22, 23, 50, 1800, "Writing", Some("REPORT"))]);
        assert_eq!(out.sessions.len(), 2);
        assert_eq!(out.sessions[0].focus_seconds, 600);
        assert_eq!(out.sessions[0].break_reason, BreakReason::Midnight);
        assert_eq!(out.sessions[1].focus_seconds, 1200);
        assert_eq!(out.deep_focus_sessions, 0);
    }

    #[test]
    fn observation_ending_exactly_at_midnight_has_no_phantom_session() {
        let out = summarize(vec![sample(22, 23, 50, 10 * 60, "Writing", Some("REPORT"))]);
        assert_eq!(out.sessions.len(), 1);
        assert_eq!(out.sessions[0].focus_seconds, 10 * 60);
        assert_eq!(out.sessions[0].end, "2026-08-23 00:00:00");
        assert_eq!(out.focus_eligible_seconds, 10 * 60);
    }

    #[test]
    fn overlapping_observations_are_not_double_counted() {
        let first = sample(22, 9, 0, 900, "Analysis", Some("FORECAST"));
        let second = sample(22, 9, 10, 900, "Analysis", Some("FORECAST"));
        let out = summarize(vec![first, second]);
        assert_eq!(out.focus_eligible_seconds, 1500);
        assert_eq!(out.deep_focus_seconds, 1500);
        assert_eq!(out.overlap_clipped_seconds, 300);
    }

    #[test]
    fn non_developer_knowledge_work_is_focus_eligible() {
        for category in [
            "Research",
            "Documentation",
            "Writing",
            "Analysis",
            "Design",
            "Learning",
        ] {
            let mut activity = sample(22, 9, 0, 1500, category, None);
            activity.description = "sustained work on a report".into();
            activity.app_name = Some("LibreOffice".into());
            activity.window_title = Some("Quarterly analysis".into());
            assert_eq!(
                summarize(vec![activity]).deep_focus_sessions,
                1,
                "{category}"
            );
        }
    }

    #[test]
    fn canonical_category_registry_is_unique_total_and_prompt_backed() {
        let prompt = allowed_categories_prompt();
        let labels: Vec<&str> = prompt.split(", ").collect();
        let unique: std::collections::BTreeSet<&str> = labels.iter().copied().collect();

        assert_eq!(labels.len(), CATEGORY_POLICIES.len());
        assert_eq!(unique.len(), labels.len());
        for label in labels {
            assert_eq!(canonical_category_label(label), Some(label));
            assert_ne!(focus_role(label), FocusRole::Unknown, "{label}");
        }
        assert_eq!(canonical_category_label("code review"), Some("CodeReview"));
        assert_eq!(canonical_category_label("DEV_OPS"), Some("DevOps"));
        assert_eq!(canonicalize_category("Custom workflow"), "Custom workflow");
        assert_eq!(
            canonical_ticket_value(Some("  ABC-1  ")).as_deref(),
            Some("ABC-1")
        );
        assert_eq!(canonical_ticket_value(Some("General")), None);
        assert_eq!(canonical_ticket_value(Some("General / No Ticket")), None);
        assert_eq!(canonical_ticket_value(Some(" no   ticket ")), None);
        assert_eq!(canonical_ticket_value(Some("  \t ")), None);
    }

    #[test]
    fn manual_theme_change_breaks_without_a_ticket() {
        let mut first = sample(22, 9, 0, 900, "Writing", None);
        first.theme_hint = Some("Customer proposal".into());
        let mut second = sample(22, 9, 15, 900, "Analysis", None);
        second.theme_hint = Some("Quarterly forecast".into());
        let unlabelled = sample(22, 9, 30, 3600, "Analysis", None);
        let out = summarize(vec![first, second, unlabelled]);
        assert_eq!(out.sessions.len(), 2);
        assert_eq!(out.theme_switches, 1);
        assert_eq!(out.explicit_theme_switches_per_labelled_focus_hour, 2.0);
        assert_eq!(
            out.sessions[0].theme.as_deref(),
            Some("Task Customer proposal")
        );
        assert_eq!(
            out.sessions[1].theme.as_deref(),
            Some("Task Quarterly forecast")
        );
    }

    #[test]
    fn theme_matching_normalizes_unicode_case_and_internal_whitespace() {
        let mut first = sample(22, 9, 0, 900, "Writing", None);
        first.theme_hint = Some("  Informe   Trimestral ".into());
        let mut second = sample(22, 9, 15, 900, "Analysis", None);
        second.theme_hint = Some("informe trimestral".into());

        let out = summarize(vec![first, second]);

        assert_eq!(out.sessions.len(), 1);
        assert_eq!(out.deep_focus_sessions, 1);
        assert_eq!(out.theme_switches, 0);
        assert_eq!(
            out.sessions[0].theme.as_deref(),
            Some("Task Informe Trimestral")
        );
    }

    #[test]
    fn unlabeled_work_after_a_gap_is_not_claimed_as_a_resume() {
        let mut labelled = sample(22, 9, 0, 600, "Writing", None);
        labelled.theme_hint = Some("Customer proposal".into());
        let unlabeled = sample(22, 9, 12, 600, "Writing", None);

        let out = summarize(vec![labelled, unlabeled]);

        assert_eq!(out.sessions.len(), 2);
        assert_eq!(out.resume_events, 0);
        assert_eq!(out.average_resume_seconds, None);
    }

    #[test]
    fn midnight_resets_pending_resume_state() {
        let before = sample(22, 23, 30, 1200, "Writing", Some("REPORT"));
        let interruption = sample(22, 23, 50, 600, "Meeting", None);
        let after = sample(23, 0, 0, 1500, "Writing", Some("REPORT"));

        let out = summarize(vec![before, interruption, after]);

        assert_eq!(out.resume_events, 0);
        assert_eq!(out.theme_switches, 0);
        assert_eq!(out.deep_focus_sessions, 1);
    }

    #[test]
    fn policy_boundaries_are_inclusive_only_at_the_declared_thresholds() {
        let below_deep = summarize(vec![sample(22, 9, 0, DEEP_TIER_SECS - 1, "Analysis", None)]);
        let at_deep = summarize(vec![sample(22, 9, 0, DEEP_TIER_SECS, "Analysis", None)]);
        let at_extended = summarize(vec![sample(22, 9, 0, EXTENDED_TIER_SECS, "Writing", None)]);

        assert_eq!(below_deep.deep_focus_sessions, 0);
        assert_eq!(below_deep.sessions[0].tier, "focused");
        assert_eq!(at_deep.deep_focus_sessions, 1);
        assert_eq!(at_deep.sessions[0].tier, "deep");
        assert_eq!(at_extended.sessions[0].tier, "extended");

        let first = sample(22, 13, 0, 600, "Research", Some("TOPIC"));
        let mut exact_grace = sample(22, 13, 0, 900, "Research", Some("TOPIC"));
        exact_grace.start = first.end() + Duration::seconds(SENSOR_GRACE_SECS);
        let connected = summarize(vec![first.clone(), exact_grace]);
        assert_eq!(connected.deep_focus_sessions, 1);

        let mut beyond_grace = sample(22, 13, 0, 900, "Research", Some("TOPIC"));
        beyond_grace.start = first.end() + Duration::seconds(SENSOR_GRACE_SECS + 1);
        let split = summarize(vec![first, beyond_grace]);
        assert_eq!(split.deep_focus_sessions, 0);
        assert_eq!(split.sessions.len(), 2);
    }

    #[test]
    fn summary_conserves_focus_seconds_and_hourly_deep_minutes() {
        let mut writing = sample(22, 9, 50, 900, "Writing", None);
        writing.theme_hint = Some("Brief".into());
        let mut research = sample(22, 10, 5, 900, "Research", None);
        research.theme_hint = Some("Brief".into());
        let meeting = sample(22, 10, 20, 300, "Meeting", None);
        let out = summarize(vec![writing, research, meeting]);

        assert_eq!(
            out.sessions
                .iter()
                .map(|session| session.focus_seconds)
                .sum::<i64>(),
            out.focus_eligible_seconds
        );
        assert_eq!(
            out.hourly_deep_focus
                .iter()
                .map(|bucket| bucket.seconds)
                .sum::<i64>(),
            out.deep_focus_seconds
        );
        assert_eq!(out.deep_focus_seconds, 1800);
        assert_eq!(out.context_work_seconds, 300);
    }

    #[test]
    fn canonical_payload_serializes_policy_metrics_and_human_theme_labels() {
        let mut writing = sample(22, 9, 0, DEEP_TIER_SECS, "Writing", None);
        writing.theme_hint = Some("Policy brief".into());
        let payload = serde_json::to_value(summarize(vec![writing])).unwrap();

        assert_eq!(payload["policy_version"], POLICY_VERSION);
        assert_eq!(payload["construct_label"], CONSTRUCT_LABEL);
        assert_eq!(payload["proxy_disclaimer"], PROXY_DISCLAIMER);
        assert_eq!(payload["focused_threshold_seconds"], FOCUSED_TIER_SECS);
        assert_eq!(payload["deep_threshold_seconds"], DEEP_TIER_SECS);
        assert_eq!(payload["extended_threshold_seconds"], EXTENDED_TIER_SECS);
        assert_eq!(payload["sensor_grace_seconds"], SENSOR_GRACE_SECS);
        assert_eq!(
            payload["browsing_distraction_min_seconds"],
            BROWSING_DISTRACTION_MIN_SECS
        );
        assert_eq!(payload["deep_focus_seconds"], DEEP_TIER_SECS);
        assert_eq!(payload["sessions"][0]["theme"], "Task Policy brief");
        assert!(payload.get("switches_per_focus_hour").is_none());
        assert!(!payload.to_string().contains("task:policy brief"));
    }

    #[test]
    fn renderer_and_cloud_prompts_depend_on_the_canonical_payload() {
        let renderer = include_str!("../../src/renderer/index.html");
        let charts = include_str!("../../src/renderer/insights-charts.mjs");
        let task_context = include_str!("../../src/renderer/today-task-context.mjs");
        assert!(renderer.contains("data.focus?.deep_focus_seconds"));
        assert!(renderer.contains("focus.hourly_deep_focus"));
        assert!(renderer.contains("focus.deep_threshold_seconds"));
        assert!(renderer.contains("focus.sensor_grace_seconds"));
        assert!(renderer.contains("focus.browsing_distraction_min_seconds"));
        assert!(renderer.contains("resolveTaskContext"));
        assert!(task_context.contains("selectedValue !== 'General'"));
        assert!(renderer.contains("Sustained non-work browsing"));
        assert!(renderer.contains("no deep focus"));
        assert!(renderer.contains("each bar is one hour"));
        assert!(renderer.contains("focusBarPercent(seconds)"));
        assert!(charts.contains("value / 3600 * 100"));
        assert!(!renderer.contains("Focus, flow, and planning"));
        for forbidden in [
            "FOCUS_CATEGORIES",
            "DISTRACTION_CATEGORIES",
            "computeFocusSeconds",
            "deep_focus_sessions_30m_plus",
        ] {
            assert!(
                !renderer.contains(forbidden),
                "renderer contains {forbidden}"
            );
        }

        let coach = include_str!("../../../../supabase/functions/coach-chat/index.ts");
        let insights = include_str!("../../../../supabase/functions/generate-insights/index.ts");
        assert!(coach.contains("local_context.focus_semantics"));
        assert!(coach.contains("Never reconstruct Deep Focus"));
        assert!(coach.contains("Use only its distraction_events/distraction_seconds"));
        assert!(insights.contains("DATA.localReport.focus_semantics"));
        assert!(insights.contains("do not estimate it from categories"));
        assert!(insights.contains("Use only focus_semantics.distraction_events"));

        // Notion is temporarily unavailable in this macOS release. Verify the
        // guard and unregistered native command rather than requiring backend
        // publishing sources that are not shipped by this platform.
        assert!(renderer.contains("const NOTION_INTEGRATION_ENABLED = false;"));
        let native_commands = include_str!("lib.rs");
        assert!(!native_commands.contains("mod notion;"));
        assert!(!native_commands.contains("notion::publish_notion_report"));
        assert!(!renderer.contains("NOTION_CLIENT_SECRET"));
        assert!(!renderer.contains("token_ciphertext"));
    }

    #[test]
    fn hourly_buckets_split_cross_hour_intervals() {
        let out = summarize(vec![sample(22, 9, 50, 1500, "Coding", Some("FS-1"))]);
        assert_eq!(out.hourly_deep_focus[9].seconds, 600);
        assert_eq!(out.hourly_deep_focus[10].seconds, 900);
    }

    #[test]
    fn distraction_thresholds_ignore_blips_and_lock_screen_noise() {
        let short_browse = summarize(vec![sample(22, 9, 0, 60, "Browsing", None)]);
        assert_eq!(short_browse.distraction_events, 0);

        let browsing = summarize(vec![
            sample(22, 9, 0, 60, "Browsing", None),
            sample(22, 9, 1, 60, "Browsing", None),
        ]);
        assert_eq!(browsing.distraction_events, 1);
        assert_eq!(browsing.distraction_seconds, 120);

        let lock_blip = summarize(vec![sample(22, 9, 0, 240, "Idle", None)]);
        assert_eq!(lock_blip.distraction_events, 0);

        let long_break = summarize(vec![sample(22, 9, 0, 1800, "Idle", None)]);
        assert_eq!(long_break.distraction_events, 0);
        assert_eq!(long_break.distraction_seconds, 0);
    }

    #[test]
    fn app_patterns_attribute_only_qualifying_browsing_and_respect_exclusions() {
        let mut a_first = sample(22, 9, 10, 60, "Browsing", None);
        a_first.app_name = Some("Browser A.exe".into());
        let mut b = sample(22, 9, 11, 60, "Browsing", None);
        b.app_name = Some("Browser B.exe".into());
        let mut a_long = sample(22, 9, 20, 180, "Browsing", None);
        a_long.app_name = Some("browser a.EXE".into());
        let mut a_blip = sample(22, 9, 30, 60, "Browsing", None);
        a_blip.app_name = Some("Browser A.exe".into());
        let mut a_next_day = sample(23, 9, 0, 120, "Browsing", None);
        a_next_day.app_name = Some("Browser A.exe".into());
        let samples = vec![
            sample(22, 9, 0, 600, "Coding", Some("FS-1")),
            a_first,
            b,
            a_long,
            a_blip,
            sample(22, 9, 31, 300, "Meeting", None),
            a_next_day,
        ];
        let canonical = summarize(samples.clone());
        let apps = analyze_distraction_apps(samples, &["browser b".into()]);

        assert_eq!(canonical.distraction_events, 3);
        assert_eq!(canonical.distraction_seconds, 420);
        assert_eq!(apps.qualifying_episodes, 2);
        assert_eq!(apps.qualifying_seconds, 300);
        assert_eq!(apps.attributed_seconds, 300);
        assert_eq!(apps.unattributed_seconds, 0);
        assert_eq!(apps.apps.len(), 1);
        assert_eq!(apps.apps[0].app_name, "Browser A.exe");
        assert_eq!(apps.apps[0].episodes, 2);
        assert_eq!(apps.apps[0].days, 2);
        assert_eq!(apps.apps[0].transitions_from_focus, 0);
        assert_eq!(apps.apps[0].daily_seconds[0].seconds, 180);
        assert_eq!(apps.apps[0].daily_seconds[1].seconds, 120);
    }

    #[test]
    fn browser_destinations_count_short_work_interleaved_revisits_not_the_browser_process() {
        let mut work_before = sample(22, 9, 0, 60, "Research", None);
        work_before.app_name = Some("Arc".into());
        let mut music_first = sample(22, 9, 1, 60, "Browsing", None);
        music_first.app_name = Some("Arc".into());
        music_first.description = "WINDOW CONTEXT: Apple Music playlist and playback controls\nCURRENT ACTION: selecting a song in Apple Music".into();
        let mut work_between = sample(22, 9, 2, 180, "Coding", None);
        work_between.app_name = Some("Arc".into());
        let mut music_again = sample(22, 9, 5, 60, "Browsing", None);
        music_again.app_name = Some("Arc".into());
        music_again.description = "WINDOW CONTEXT: music streaming app showing a playlist\nCURRENT ACTION: browsing tracks in the Apple Music app".into();
        let mut work_after = sample(22, 9, 6, 60, "Design", None);
        work_after.app_name = Some("Arc".into());
        let analysis = analyze_distraction_apps(
            vec![
                work_before,
                music_first,
                work_between,
                music_again,
                work_after,
            ],
            &[],
        );

        assert_eq!(analysis.detours.len(), 1);
        let music = &analysis.detours[0];
        assert_eq!(music.label, "Apple Music");
        assert_eq!(music.kind, "music");
        assert_eq!(music.seconds, 120);
        assert_eq!(music.visits, 2);
        assert_eq!(music.work_interleaved_revisits, 1);
        assert_eq!(music.daily_visits[0].shortest_revisit_minutes, Some(4));
        assert_eq!(music.daily_visits[0].observed_at, ["09:02", "09:06"]);
        assert!(
            analysis.apps.is_empty(),
            "two one-minute visits must not create a fake Arc app distraction"
        );
    }

    #[test]
    fn notification_uses_only_a_verified_public_destination() {
        let mut browsing = sample(22, 9, 1, 120, "Browsing", None);
        browsing.app_name = Some("Chrome.exe".into());
        browsing.description =
            "FOREGROUND DESTINATION: YouTube\nCURRENT ACTION: watching YouTube Shorts".into();
        assert_eq!(
            notification_destination(&browsing).as_deref(),
            Some("YouTube Shorts")
        );
        browsing.app_name = Some("C:\\Program Files\\Google\\Chrome\\chrome.exe".into());
        assert_eq!(
            notification_destination(&browsing).as_deref(),
            Some("YouTube Shorts")
        );
        browsing.app_name = Some("Chrome.exe".into());
        browsing.description =
            "FOREGROUND DESTINATION: YouTube\nCURRENT ACTION: scrolling short videos".into();
        assert_eq!(
            notification_destination(&browsing).as_deref(),
            Some("YouTube Shorts")
        );

        browsing.description = "WINDOW CONTEXT: YouTube feed\nCURRENT ACTION: browsing".into();
        assert_eq!(notification_destination(&browsing), None);

        browsing.description =
            "FOREGROUND DESTINATION: https://private.example/path\nCURRENT ACTION: browsing".into();
        assert_eq!(notification_destination(&browsing), None);

        browsing.description =
            "FOREGROUND DESTINATION: Confidential plan\nCURRENT ACTION: browsing".into();
        assert_eq!(notification_destination(&browsing), None);

        browsing.description =
            "FOREGROUND DESTINATION: YouTube\nCURRENT ACTION: watching videos".into();
        browsing.category = "Research".into();
        assert_eq!(notification_destination(&browsing), None);

        browsing.category = "Browsing".into();
        browsing.app_name = Some("Discord.exe".into());
        assert_eq!(
            notification_destination(&browsing).as_deref(),
            Some("Discord")
        );
    }

    #[test]
    fn same_destination_with_changing_work_category_is_one_visit() {
        let mut youtube_first = sample(22, 9, 1, 60, "Browsing", None);
        youtube_first.app_name = Some("Arc".into());
        youtube_first.description =
            "WINDOW CONTEXT: YouTube video page\nCURRENT ACTION: browsing YouTube videos".into();
        let mut youtube_misclassified = sample(22, 9, 2, 60, "Research", None);
        youtube_misclassified.app_name = Some("Arc".into());
        youtube_misclassified.description =
            "WINDOW CONTEXT: YouTube video page\nCURRENT ACTION: watching YouTube video".into();
        let mut youtube_again = sample(22, 9, 3, 60, "Browsing", None);
        youtube_again.app_name = Some("Arc".into());
        youtube_again.description =
            "WINDOW CONTEXT: YouTube video page\nCURRENT ACTION: browsing YouTube videos".into();
        let work = sample(22, 9, 4, 60, "Coding", None);
        let mut youtube_after_work = sample(22, 9, 5, 60, "Browsing", None);
        youtube_after_work.app_name = Some("Arc".into());
        youtube_after_work.description =
            "WINDOW CONTEXT: YouTube video page\nCURRENT ACTION: browsing YouTube videos".into();

        let analysis = analyze_distraction_apps(
            vec![
                youtube_first,
                youtube_misclassified,
                youtube_again,
                work,
                youtube_after_work,
            ],
            &[],
        );
        assert_eq!(analysis.detours.len(), 1);
        let youtube = &analysis.detours[0];
        assert_eq!(youtube.visits, 2);
        assert_eq!(youtube.seconds, 240);
        assert_eq!(youtube.work_interleaved_revisits, 1);
    }

    #[test]
    fn any_structured_browser_destination_can_be_reported_without_a_brand_allowlist() {
        let mut first = sample(22, 9, 1, 60, "Browsing", None);
        first.app_name = Some("Arc".into());
        first.description = "FOREGROUND DESTINATION: Kiteboard\nWINDOW CONTEXT: project workspace\nCURRENT ACTION: reviewing a board".into();
        let mut second = sample(22, 9, 5, 60, "Browsing", None);
        second.app_name = Some("Chrome".into());
        second.description = first.description.clone();
        let analysis = analyze_distraction_apps(
            vec![first, sample(22, 9, 2, 180, "Coding", None), second],
            &[],
        );
        assert_eq!(analysis.detours.len(), 1);
        assert_eq!(analysis.detours[0].label, "Kiteboard");
        assert_eq!(analysis.detours[0].visits, 2);
        assert_eq!(analysis.detours[0].work_interleaved_revisits, 1);
    }

    #[test]
    fn unknown_or_container_processes_do_not_hide_a_structured_destination() {
        for app in ["Unknown", "ApplicationFrameHost"] {
            let mut screen = sample(22, 9, 1, 60, "Browsing", None);
            screen.app_name = Some(app.into());
            screen.description =
                "FOREGROUND DESTINATION: Kiteboard\nWINDOW CONTEXT: project workspace".into();
            let destination = foreground_destination_identity(&screen, &BTreeSet::new()).unwrap();
            assert_eq!(destination.label, "Kiteboard");
        }
    }

    #[test]
    fn native_message_and_unknown_apps_need_a_repeated_work_interruption() {
        let mut slack_first = sample(22, 9, 1, 60, "Communication", None);
        slack_first.app_name = Some("Slack.exe".into());
        let mut slack_second = sample(22, 9, 5, 60, "Communication", None);
        slack_second.app_name = Some("Slack.exe".into());
        let mut pixel_first = sample(22, 9, 10, 60, "General", None);
        pixel_first.app_name = Some("PixelNest.exe".into());
        let mut pixel_second = sample(22, 9, 14, 60, "General", None);
        pixel_second.app_name = Some("PixelNest.exe".into());
        let mut one_off = sample(22, 9, 20, 600, "Communication", None);
        one_off.app_name = Some("Microsoft Outlook".into());
        let analysis = analyze_distraction_apps(
            vec![
                slack_first,
                sample(22, 9, 2, 180, "Coding", None),
                slack_second,
                pixel_first,
                sample(22, 9, 11, 180, "Design", None),
                pixel_second,
                one_off,
            ],
            &[],
        );
        assert_eq!(analysis.detours.len(), 2);
        assert_eq!(analysis.detours[0].label, "PixelNest");
        assert_eq!(analysis.detours[0].work_interleaved_revisits, 1);
        assert_eq!(analysis.detours[1].label, "Slack");
        assert_eq!(analysis.detours[1].kind, "communication");
        assert_eq!(analysis.detours[1].work_interleaved_revisits, 1);
        assert!(!analysis
            .detours
            .iter()
            .any(|row| row.label == "Microsoft Outlook"));
    }

    #[test]
    fn legacy_browser_context_finds_unknown_site_but_not_a_chat_mention() {
        let mut first = sample(22, 9, 1, 60, "Browsing", None);
        first.app_name = Some("Arc".into());
        first.description = "WINDOW CONTEXT: Kiteboard workspace showing a project board\nCURRENT ACTION: using Kiteboard app".into();
        let mut second = sample(22, 9, 5, 60, "Browsing", None);
        second.app_name = Some("Arc".into());
        second.description = first.description.clone();
        let mut discussion = sample(22, 9, 8, 60, "Communication", None);
        discussion.app_name = Some("Arc".into());
        discussion.description = "WINDOW CONTEXT: chat interface discussing Kiteboard app\nCURRENT ACTION: sending a message about Kiteboard app".into();
        let analysis = analyze_distraction_apps(
            vec![
                first,
                sample(22, 9, 2, 180, "Coding", None),
                second,
                discussion,
            ],
            &[],
        );
        assert_eq!(analysis.detours.len(), 1);
        assert_eq!(analysis.detours[0].label, "Kiteboard");
        assert_eq!(analysis.detours[0].visits, 2);
    }

    #[test]
    fn desktop_shortcuts_are_not_counted_as_open_destinations() {
        let mut icon = sample(22, 9, 1, 600, "Browsing", None);
        icon.app_name = None;
        icon.description = "VISIBLE CONTENT: The Windows desktop start menu shows a Kiteboard app icon; no app window is open\nCURRENT ACTION: possibly opening the Kiteboard app".into();
        let analysis = analyze_distraction_apps(vec![icon], &[]);
        assert!(analysis.detours.is_empty());
    }

    #[test]
    fn casual_browsing_label_does_not_override_obvious_creative_or_publishing_work() {
        let mut music_work = sample(22, 9, 1, 180, "Browsing", None);
        music_work.app_name = None;
        music_work.description = "WINDOW CONTEXT: audio production software with MIDI patterns\nCURRENT ACTION: arranging a track in SoundForge app".into();
        let mut music_work_again = sample(22, 9, 7, 180, "Browsing", None);
        music_work_again.app_name = None;
        music_work_again.description = music_work.description.clone();
        let mut publishing = sample(22, 10, 1, 180, "Browsing", None);
        publishing.app_name = Some("Arc".into());
        publishing.description = "FOREGROUND DESTINATION: Kiteboard Console\nWINDOW CONTEXT: developer verification dashboard\nCURRENT ACTION: publishing an application".into();
        let mut publishing_again = sample(22, 10, 7, 180, "Browsing", None);
        publishing_again.app_name = Some("Arc".into());
        publishing_again.description = publishing.description.clone();
        let analysis = analyze_distraction_apps(
            vec![
                music_work,
                sample(22, 9, 4, 180, "Coding", None),
                music_work_again,
                publishing,
                sample(22, 10, 4, 180, "Coding", None),
                publishing_again,
            ],
            &[],
        );
        assert!(analysis.detours.is_empty());
    }

    #[test]
    fn ambiguous_browser_sample_does_not_invent_an_exit_and_possessive_names_merge() {
        let mut first = sample(22, 9, 1, 60, "Browsing", None);
        first.app_name = Some("Arc".into());
        first.description =
            "WINDOW CONTEXT: YouTube video feed\nCURRENT ACTION: browsing videos".into();
        let mut unnamed = sample(22, 9, 2, 60, "Browsing", None);
        unnamed.app_name = Some("Arc".into());
        unnamed.description =
            "WINDOW CONTEXT: video player\nCURRENT ACTION: watching a clip".into();
        let mut same_site = sample(22, 9, 3, 60, "Browsing", None);
        same_site.app_name = Some("Arc".into());
        same_site.description =
            "WINDOW CONTEXT: YouTube's video feed\nCURRENT ACTION: browsing videos".into();
        let mut after_work = sample(22, 9, 5, 60, "Browsing", None);
        after_work.app_name = Some("Arc".into());
        after_work.description = first.description.clone();
        let analysis = analyze_distraction_apps(
            vec![
                first,
                unnamed,
                same_site,
                sample(22, 9, 4, 60, "Coding", None),
                after_work,
            ],
            &[],
        );
        assert_eq!(analysis.detours.len(), 1);
        assert_eq!(analysis.detours[0].label, "YouTube");
        assert_eq!(analysis.detours[0].visits, 2);
        assert_eq!(analysis.detours[0].work_interleaved_revisits, 1);
    }

    #[test]
    fn the_reporting_app_does_not_rank_as_its_own_distraction() {
        let mut first = sample(22, 9, 1, 60, "General", None);
        first.app_name = Some("FlowSight Agent".into());
        let mut second = sample(22, 9, 5, 60, "General", None);
        second.app_name = Some("FlowSight Agent".into());
        let analysis = analyze_distraction_apps(
            vec![first, sample(22, 9, 2, 180, "Coding", None), second],
            &[],
        );
        assert!(analysis.detours.is_empty());
    }

    #[test]
    fn context_mentions_and_excluded_apps_do_not_become_destinations() {
        let mut telegram = sample(22, 10, 0, 60, "Browsing", None);
        telegram.app_name = Some("Telegram Desktop".into());
        telegram.description = "WINDOW CONTEXT: chat interface discussing Apple Music\nCURRENT ACTION: sending a message about Apple Music".into();
        let mut folder = sample(22, 10, 1, 60, "Browsing", None);
        folder.app_name = None;
        folder.description = "VISIBLE CONTENT: A file explorer shows folders and an Apple Music sidebar\nCURRENT ACTION: selecting files in Finder".into();
        let mut music = sample(22, 10, 2, 300, "Browsing", None);
        music.app_name = Some("Arc".into());
        music.description = "VISIBLE CONTENT: Apple Music is open with a playlist\nCURRENT ACTION: browsing Apple Music tracks".into();
        let mut annotation = music.clone();
        annotation.start = at(22, 10, 4);
        annotation.duration_seconds = 0;

        let included = analyze_distraction_apps(
            vec![telegram.clone(), folder.clone(), music.clone(), annotation],
            &[],
        );
        assert_eq!(included.detours.len(), 1);
        assert_eq!(included.detours[0].seconds, 300);
        assert_eq!(included.detours[0].visits, 1);

        let excluded = analyze_distraction_apps(vec![telegram, folder, music], &["Arc".into()]);
        assert!(excluded.detours.is_empty());
    }

    #[test]
    fn app_patterns_do_not_treat_short_browsing_or_context_work_as_distraction() {
        let apps = analyze_distraction_apps(
            vec![
                sample(22, 9, 0, 90, "Browsing", None),
                sample(22, 9, 2, 180, "Communication", None),
                sample(22, 9, 5, 300, "Research", None),
            ],
            &[],
        );
        assert_eq!(apps.qualifying_episodes, 0);
        assert_eq!(apps.qualifying_seconds, 0);
        assert!(apps.apps.is_empty());
    }

    #[test]
    fn valuable_non_focus_work_is_retained_as_context_not_distraction() {
        let out = summarize(vec![
            sample(22, 9, 0, 600, "Planning", None),
            sample(22, 9, 10, 300, "Meeting", None),
            sample(22, 9, 15, 240, "Communication", None),
            sample(22, 9, 19, 360, "Sales", None),
            sample(22, 9, 25, 300, "Admin", None),
        ]);

        assert_eq!(out.context_work_seconds, 1800);
        assert_eq!(out.distraction_seconds, 0);
        assert_eq!(out.deep_focus_seconds, 0);
        assert_eq!(out.context_category_mix.len(), 5);
        assert_eq!(focus_role("Planning"), FocusRole::Coordination);
        assert_eq!(focus_role("Sales"), FocusRole::Operational);
        assert_eq!(focus_role("Browsing"), FocusRole::Distraction);
    }

    #[test]
    fn sqlite_loader_produces_the_same_canonical_session_payload() {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch(
            "CREATE TABLE reports (
                created_at TEXT NOT NULL,
                activity_type TEXT NOT NULL,
                description TEXT NOT NULL,
                jira_ticket_id TEXT,
                duration_seconds INTEGER NOT NULL,
                active_app TEXT,
                window_title TEXT,
                theme_hint TEXT
            );
            INSERT INTO reports VALUES
                ('2026-08-22 09:15:00', 'Writing', 'Drafting a brief', NULL, 900, 'Writer', 'Brief', 'Policy brief'),
                ('2026-08-22 09:30:00', 'Research', 'Checking sources', NULL, 900, 'Browser', 'Source', 'Policy brief');",
        )
        .unwrap();

        let out = summarize_from_db(&conn, "2026-08-21", "2026-08-23").unwrap();
        assert_eq!(out.deep_focus_sessions, 1);
        assert_eq!(out.deep_focus_seconds, 1800);
        assert_eq!(out.explicit_theme_coverage_pct, 100.0);
        assert_eq!(out.sessions[0].category_mix.len(), 2);
    }

    #[test]
    fn sqlite_loader_clips_a_capture_across_the_requested_day_boundary() {
        use chrono::TimeZone;

        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch(
            "CREATE TABLE reports (
                created_at TEXT NOT NULL,
                activity_type TEXT NOT NULL,
                description TEXT NOT NULL,
                jira_ticket_id TEXT,
                duration_seconds INTEGER NOT NULL,
                active_app TEXT,
                window_title TEXT,
                theme_hint TEXT
            );",
        )
        .unwrap();
        let local_end = chrono::Local
            .with_ymd_and_hms(2026, 8, 23, 0, 10, 0)
            .single()
            .unwrap();
        let stored_utc = local_end
            .with_timezone(&chrono::Utc)
            .format("%Y-%m-%d %H:%M:%S")
            .to_string();
        conn.execute(
            "INSERT INTO reports VALUES (?1, 'Writing', 'Cross-midnight draft', NULL, 1200, 'Writer', 'Draft', 'Report')",
            params![stored_utc],
        )
        .unwrap();

        let first_day = summarize_from_db(&conn, "2026-08-22", "2026-08-22").unwrap();
        let second_day = summarize_from_db(&conn, "2026-08-23", "2026-08-23").unwrap();

        assert_eq!(first_day.focus_eligible_seconds, 600);
        assert_eq!(second_day.focus_eligible_seconds, 600);
        assert_eq!(first_day.sessions[0].focus_seconds, 600);
        assert_eq!(second_day.sessions[0].focus_seconds, 600);
    }

    #[test]
    fn labelled_timeline_eval_has_zero_minute_and_session_error() {
        #[derive(serde::Deserialize)]
        struct FixtureSample {
            start: String,
            seconds: i64,
            category: String,
            theme: Option<String>,
        }
        #[derive(serde::Deserialize)]
        struct FixtureCase {
            id: String,
            samples: Vec<FixtureSample>,
            deep_seconds: i64,
            deep_sessions: usize,
            total_sessions: usize,
            interrupted_sessions: usize,
            theme_switches: usize,
            resume_events: usize,
            distraction_events: usize,
            context_seconds: i64,
        }

        let fixtures: Vec<FixtureCase> =
            serde_json::from_str(include_str!("../testdata/focus_timeline_cases.json"))
                .expect("valid timeline fixture");
        assert!(
            fixtures.len() >= 10,
            "timeline eval needs multiple scenarios"
        );

        let mut absolute_minute_error = 0.0;
        let mut session_count_error = 0usize;
        let mut total_session_count_error = 0usize;
        let mut interrupted_session_count_error = 0usize;
        let mut switch_count_error = 0usize;
        let mut resume_count_error = 0usize;
        let mut distraction_count_error = 0usize;
        let mut context_minute_error = 0.0;
        let mut failures = Vec::new();
        for fixture in &fixtures {
            let samples = fixture
                .samples
                .iter()
                .map(|row| ActivitySample {
                    start: NaiveDateTime::parse_from_str(&row.start, "%Y-%m-%d %H:%M:%S").unwrap(),
                    duration_seconds: row.seconds,
                    category: row.category.clone(),
                    description: format!("{} fixture", row.category),
                    ticket: None,
                    theme_hint: row.theme.clone(),
                    app_name: Some("Fixture app".into()),
                    window_title: Some(fixture.id.clone()),
                })
                .collect();
            let actual = summarize(samples);
            absolute_minute_error +=
                (actual.deep_focus_seconds - fixture.deep_seconds).abs() as f64 / 60.0;
            session_count_error += actual.deep_focus_sessions.abs_diff(fixture.deep_sessions);
            total_session_count_error += actual.sessions.len().abs_diff(fixture.total_sessions);
            let interrupted_sessions = actual
                .sessions
                .iter()
                .filter(|session| session.interrupted)
                .count();
            interrupted_session_count_error +=
                interrupted_sessions.abs_diff(fixture.interrupted_sessions);
            switch_count_error += actual.theme_switches.abs_diff(fixture.theme_switches);
            resume_count_error += actual.resume_events.abs_diff(fixture.resume_events);
            distraction_count_error += actual
                .distraction_events
                .abs_diff(fixture.distraction_events);
            context_minute_error +=
                (actual.context_work_seconds - fixture.context_seconds).abs() as f64 / 60.0;
            if actual.deep_focus_seconds != fixture.deep_seconds
                || actual.deep_focus_sessions != fixture.deep_sessions
                || actual.sessions.len() != fixture.total_sessions
                || interrupted_sessions != fixture.interrupted_sessions
                || actual.theme_switches != fixture.theme_switches
                || actual.resume_events != fixture.resume_events
                || actual.distraction_events != fixture.distraction_events
                || actual.context_work_seconds != fixture.context_seconds
            {
                failures.push(format!(
                    "{}: seconds {}/{}, deep sessions {}/{}, all sessions {}/{}, interrupted sessions {}/{}, switches {}/{}, resumes {}/{}, distractions {}/{}, context seconds {}/{}",
                    fixture.id,
                    actual.deep_focus_seconds,
                    fixture.deep_seconds,
                    actual.deep_focus_sessions,
                    fixture.deep_sessions,
                    actual.sessions.len(),
                    fixture.total_sessions,
                    interrupted_sessions,
                    fixture.interrupted_sessions,
                    actual.theme_switches,
                    fixture.theme_switches,
                    actual.resume_events,
                    fixture.resume_events,
                    actual.distraction_events,
                    fixture.distraction_events,
                    actual.context_work_seconds,
                    fixture.context_seconds
                ));
            }
        }

        let n = fixtures.len() as f64;
        let minute_mae = absolute_minute_error / n;
        let session_mae = session_count_error as f64 / n;
        let all_session_mae = total_session_count_error as f64 / n;
        let interrupted_session_mae = interrupted_session_count_error as f64 / n;
        let switch_mae = switch_count_error as f64 / n;
        let resume_mae = resume_count_error as f64 / n;
        let distraction_mae = distraction_count_error as f64 / n;
        let context_minute_mae = context_minute_error / n;
        eprintln!(
            "timeline_eval n={} deep_minute_mae={:.3} deep_session_mae={:.3} all_session_mae={:.3} interrupted_session_mae={:.3} switch_mae={:.3} resume_mae={:.3} distraction_mae={:.3} context_minute_mae={:.3}",
            fixtures.len(), minute_mae, session_mae, all_session_mae, interrupted_session_mae, switch_mae, resume_mae, distraction_mae, context_minute_mae
        );
        assert!(failures.is_empty(), "timeline failures: {failures:?}");
        assert_eq!(minute_mae, 0.0);
        assert_eq!(session_mae, 0.0);
        assert_eq!(all_session_mae, 0.0);
        assert_eq!(interrupted_session_mae, 0.0);
        assert_eq!(switch_mae, 0.0);
        assert_eq!(resume_mae, 0.0);
        assert_eq!(distraction_mae, 0.0);
        assert_eq!(context_minute_mae, 0.0);
    }

    #[test]
    fn fixture_eval_reports_zero_minute_error_and_resume_latency() {
        let mut before = sample(22, 9, 0, 1500, "Writing", None);
        before.theme_hint = Some("Policy brief".into());
        let interruption = sample(22, 9, 25, 120, "Communication", None);
        let mut after = sample(22, 9, 27, 1500, "Writing", None);
        after.theme_hint = Some("Policy brief".into());
        let out = summarize(vec![before, interruption, after]);

        let expected_deep_seconds = 3000;
        let minute_absolute_error = (out.deep_focus_seconds - expected_deep_seconds).abs() / 60;
        assert_eq!(minute_absolute_error, 0);
        assert_eq!(out.deep_focus_sessions, 2);
        assert_eq!(out.resume_events, 1);
        assert_eq!(out.average_resume_seconds, Some(120.0));
    }
}
