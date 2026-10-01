use chrono::Local;
use reqwest::blocking::Client;
use rusqlite::{params, Connection, OpenFlags};
use serde::Serialize;
use std::collections::{BTreeMap, HashMap};
use std::time::Duration;
use tauri::Emitter;

use crate::vision_model::LLAMA_CHAT_MODEL_ID;

/// Per-section LLM context from SQLite aggregates. Keep the single local model's
/// memory footprint fixed while preserving evidence from the whole period.
const LLM_SECTION_STATS_MAX_CHARS: usize = 2800;
const LLM_PROFILE_MAX_CHARS: usize = 420;

const REPORT_SYSTEM_PROMPT: &str = "You are a privacy-first local report editor. \
Return valid JSON only. Select zero-based indices from the provided verified candidates. \
Never write report prose, invent metrics, or add fields. \
The application copies selected candidate text verbatim from local data.";

#[derive(Serialize)]
struct CategoryRow {
    category: String,
    total_seconds: i32,
    count: i32,
}

#[derive(Serialize)]
struct TicketRow {
    ticket: String,
    total_seconds: i32,
    count: i32,
}

#[derive(Serialize)]
struct DailyRow {
    date: String,
    total_seconds: i32,
    activity_count: i32,
}

#[derive(Serialize, Clone)]
struct ActivityCandidateRow {
    date: String,
    category: String,
    description: String,
    duration_seconds: i32,
    ticket: Option<String>,
}

#[derive(Serialize)]
struct WorkThemeRow {
    label: String,
    total_seconds: i32,
    activity_count: i32,
}

#[derive(Serialize)]
struct DayCategoryRow {
    date: String,
    top_category: String,
    top_hours: f64,
    total_hours: f64,
}

#[derive(Serialize)]
struct ActivitySample {
    date: String,
    category: String,
    description: String,
    duration_seconds: i32,
    ticket: Option<String>,
}

#[derive(Clone)]
struct ClippedReportRow {
    date: String,
    category: String,
    description: String,
    ticket: Option<String>,
    duration_seconds: i32,
    synced: i32,
    theme: Option<String>,
    /// One physical SQLite report may yield two calendar-day slices. Aggregate
    /// report counts must still count that observation only once.
    observation_count_increment: i32,
}

type RawReportRow = (
    String,
    String,
    String,
    Option<String>,
    i64,
    i32,
    Option<String>,
    Option<String>,
    Option<String>,
);

/// Aggregated local SQLite activity for cloud AI reports (Individual plan).
pub fn build_local_insights_report(
    db_path: &std::path::Path,
    period_days: i32,
) -> Result<serde_json::Value, String> {
    build_local_insights_report_inner(db_path, period_days, None).map(|(report, _)| report)
}

fn build_local_insights_report_inner(
    db_path: &std::path::Path,
    period_days: i32,
    excluded_applications: Option<&[String]>,
) -> Result<
    (
        serde_json::Value,
        Option<Result<crate::focus_semantics::DistractionAppAnalysis, String>>,
    ),
    String,
> {
    let days = period_days.clamp(1, 30);
    let conn = Connection::open_with_flags(db_path, OpenFlags::SQLITE_OPEN_READ_ONLY)
        .map_err(|e| e.to_string())?;
    // The aggregate and the optional app-level evidence must observe the
    // same SQLite snapshot, even while tracking writes new rows.
    conn.execute_batch("BEGIN DEFERRED TRANSACTION")
        .map_err(|e| e.to_string())?;

    let period_end = Local::now().date_naive();
    let period_start = period_end - chrono::Duration::days((days - 1) as i64);
    let start_str = period_start.format("%Y-%m-%d").to_string();
    let end_str = period_end.format("%Y-%m-%d").to_string();
    let window = crate::focus_semantics::LocalDateWindow::parse(&start_str, &end_str)?;

    let mut stmt = conn
        .prepare(
            "SELECT datetime(created_at, 'localtime') as ts,
                    activity_type,
                    description,
                    jira_ticket_id,
                    duration_seconds,
                    COALESCE(synced, 0) as synced,
                    active_app,
                    window_title,
                    theme_hint
             FROM reports
             WHERE date(created_at, 'localtime') >= ?1
               AND date(created_at, 'localtime') <= date(?2, '+1 day')
             ORDER BY datetime(created_at, 'localtime') ASC",
        )
        .map_err(|e| e.to_string())?;

    let raw_rows: Vec<RawReportRow> = stmt
        .query_map(params![start_str, end_str], |row| {
            Ok((
                row.get(0)?,
                row.get(1)?,
                row.get(2)?,
                row.get(3)?,
                row.get::<_, i64>(4).unwrap_or(0).max(0),
                row.get(5)?,
                row.get(6)?,
                row.get(7)?,
                row.get(8)?,
            ))
        })
        .map_err(|e| e.to_string())?
        .filter_map(|r| r.ok())
        .collect();
    let mut rows = Vec::<ClippedReportRow>::new();
    let mut activity_count = 0i32;
    for (timestamp, raw_category, description, ticket, duration, synced, _app, _title, theme) in
        raw_rows
    {
        let category = crate::agent_pure::resolve_persisted_category(&raw_category);
        let ticket = crate::focus_semantics::canonical_ticket_value(ticket.as_deref());
        let slices = window.slices_for_observation(&timestamp, duration);
        if slices.is_empty() {
            if duration == 0 && window.contains_local_timestamp(&timestamp) {
                activity_count += 1;
                rows.push(ClippedReportRow {
                    date: timestamp[..10].to_string(),
                    category,
                    description,
                    ticket,
                    duration_seconds: 0,
                    synced,
                    theme,
                    observation_count_increment: 1,
                });
            }
            continue;
        }
        activity_count += 1;
        for (index, slice) in slices.into_iter().enumerate() {
            rows.push(ClippedReportRow {
                date: slice.start.format("%Y-%m-%d").to_string(),
                category: category.clone(),
                description: description.clone(),
                ticket: ticket.clone(),
                duration_seconds: i32::try_from(slice.duration_seconds).unwrap_or(i32::MAX),
                synced,
                theme: theme.clone(),
                observation_count_increment: i32::from(index == 0),
            });
        }
    }

    let mut cat_map: HashMap<String, (i32, i32)> = HashMap::new();
    let mut ticket_map: HashMap<String, (i32, i32)> = HashMap::new();
    let mut daily_map: HashMap<String, (i32, i32)> = HashMap::new();
    let mut daily_category: HashMap<String, HashMap<String, i32>> = HashMap::new();
    let mut theme_map: HashMap<String, (String, i32, i32)> = HashMap::new();
    let mut total_seconds = 0i32;
    let mut ticketed_seconds = 0i32;
    let mut task_labeled_seconds = 0i32;
    let mut unsynced_count = 0i32;
    let mut all_samples: Vec<ActivitySample> = Vec::with_capacity(rows.len());

    let canonical_focus = crate::focus_semantics::summarize_from_db(&conn, &start_str, &end_str)?;

    for row in &rows {
        total_seconds = total_seconds.saturating_add(row.duration_seconds);

        let cat = cat_map.entry(row.category.clone()).or_insert((0, 0));
        cat.0 = cat.0.saturating_add(row.duration_seconds);
        cat.1 += row.observation_count_increment;

        let day = daily_map.entry(row.date.clone()).or_insert((0, 0));
        day.0 = day.0.saturating_add(row.duration_seconds);
        // Daily counts describe observations touching that calendar day. They
        // are not additive across days; the top-level/category counts remain
        // physical-report counts through `observation_count_increment`.
        day.1 += 1;

        daily_category
            .entry(row.date.clone())
            .or_default()
            .entry(row.category.clone())
            .and_modify(|seconds| *seconds = seconds.saturating_add(row.duration_seconds))
            .or_insert(row.duration_seconds);

        if let Some(ticket) = crate::focus_semantics::canonical_ticket_value(row.ticket.as_deref())
        {
            ticketed_seconds = ticketed_seconds.saturating_add(row.duration_seconds);
            let tk = ticket_map.entry(ticket).or_insert((0, 0));
            tk.0 = tk.0.saturating_add(row.duration_seconds);
            tk.1 += row.observation_count_increment;
        }

        let explicit_theme = crate::focus_semantics::canonical_theme_label(
            row.ticket.as_deref(),
            row.theme.as_deref(),
        );
        let theme_label = if let Some(label) = explicit_theme {
            task_labeled_seconds = task_labeled_seconds.saturating_add(row.duration_seconds);
            label
        } else {
            format!("{} — {}", row.category, clamp_line(&row.description, 48))
        };
        let theme_key = theme_label.to_lowercase();
        let th = theme_map.entry(theme_key).or_insert((theme_label, 0, 0));
        th.1 = th.1.saturating_add(row.duration_seconds);
        th.2 += row.observation_count_increment;

        if row.synced == 0 {
            unsynced_count += row.observation_count_increment;
        }

        all_samples.push(ActivitySample {
            date: row.date.clone(),
            category: row.category.clone(),
            description: clamp_line(&row.description, 220),
            duration_seconds: row.duration_seconds,
            ticket: row.ticket.clone(),
        });
    }

    let unticketed_seconds = total_seconds - ticketed_seconds;
    let active_days = daily_map
        .values()
        .filter(|(seconds, _)| *seconds > 0)
        .count() as i32;
    let avg_session_minutes = if canonical_focus.sessions.is_empty() {
        0.0
    } else {
        (canonical_focus
            .sessions
            .iter()
            .map(|s| s.focus_seconds)
            .sum::<i64>() as f64
            / canonical_focus.sessions.len() as f64
            / 60.0
            * 10.0)
            .round()
            / 10.0
    };

    let mut category_breakdown: Vec<CategoryRow> = cat_map
        .into_iter()
        .map(|(category, (total_seconds, count))| CategoryRow {
            category,
            total_seconds,
            count,
        })
        .collect();
    category_breakdown.sort_by_key(|row| std::cmp::Reverse(row.total_seconds));

    let mut ticket_breakdown: Vec<TicketRow> = ticket_map
        .into_iter()
        .map(|(ticket, (total_seconds, count))| TicketRow {
            ticket,
            total_seconds,
            count,
        })
        .collect();
    ticket_breakdown.sort_by_key(|row| std::cmp::Reverse(row.total_seconds));
    ticket_breakdown.truncate(20);

    let mut daily_totals: Vec<DailyRow> = daily_map
        .into_iter()
        .map(|(date, (total_seconds, activity_count))| DailyRow {
            date,
            total_seconds,
            activity_count,
        })
        .collect();
    daily_totals.sort_by(|a, b| a.date.cmp(&b.date));

    let mut day_category_breakdown: Vec<DayCategoryRow> = daily_category
        .into_iter()
        .map(|(date, cats)| {
            let total = cats.values().sum::<i32>();
            let (top_category, top_secs) = cats
                .into_iter()
                .max_by_key(|(_, secs)| *secs)
                .unwrap_or_else(|| ("General".to_string(), 0));
            DayCategoryRow {
                date,
                top_category,
                top_hours: round_hours(top_secs),
                total_hours: round_hours(total),
            }
        })
        .collect();
    day_category_breakdown.sort_by(|a, b| a.date.cmp(&b.date));

    let mut work_themes: Vec<WorkThemeRow> = theme_map
        .into_iter()
        .map(|(_, (label, total_seconds, activity_count))| WorkThemeRow {
            label,
            total_seconds,
            activity_count,
        })
        .collect();
    work_themes.sort_by_key(|row| std::cmp::Reverse(row.total_seconds));
    work_themes.truncate(12);

    let mut longest_activity_rows: Vec<ActivityCandidateRow> = all_samples
        .iter()
        .map(|s| ActivityCandidateRow {
            date: s.date.clone(),
            category: s.category.clone(),
            description: s.description.clone(),
            duration_seconds: s.duration_seconds,
            ticket: s.ticket.clone(),
        })
        .collect();
    longest_activity_rows.sort_by_key(|row| std::cmp::Reverse(row.duration_seconds));
    longest_activity_rows.truncate(12);

    let peak_day = daily_totals
        .iter()
        .max_by_key(|d| d.total_seconds)
        .map(|d| {
            serde_json::json!({
                "date": d.date,
                "hours": round_hours(d.total_seconds),
                "activities": d.activity_count,
            })
        });

    let quiet_day = daily_totals
        .iter()
        .filter(|d| d.total_seconds > 0)
        .min_by_key(|d| d.total_seconds)
        .map(|d| {
            serde_json::json!({
                "date": d.date,
                "hours": round_hours(d.total_seconds),
                "activities": d.activity_count,
            })
        });

    let peak_focus_hour = canonical_focus
        .hourly_deep_focus
        .iter()
        .max_by_key(|bucket| bucket.seconds)
        .filter(|bucket| bucket.seconds > 0)
        .map(|bucket| {
            serde_json::json!({
                "hour": bucket.hour,
                "deep_focus_minutes": bucket.seconds / 60,
            })
        });

    let prior_period = query_prior_period_metrics(&conn, period_start, days, total_seconds)?;

    let sample_activities = build_diverse_activity_samples(&all_samples, &longest_activity_rows);
    let ticket_coverage_pct = if total_seconds > 0 {
        ((ticketed_seconds as f64 / total_seconds as f64) * 1000.0).round() / 10.0
    } else {
        0.0
    };
    let task_label_coverage_pct = if total_seconds > 0 {
        ((task_labeled_seconds as f64 / total_seconds as f64) * 1000.0).round() / 10.0
    } else {
        0.0
    };
    let focus_eligible_seconds = canonical_focus.focus_eligible_seconds as i32;
    let deep_focus_seconds = canonical_focus.deep_focus_seconds as i32;
    let deep_focus_sessions = canonical_focus.deep_focus_sessions as i32;
    let distraction_count = canonical_focus.distraction_events as i32;
    let distraction_seconds = canonical_focus.distraction_seconds as i32;
    let tracking_consistency_pct = if days > 0 {
        ((active_days as f64 / days as f64) * 1000.0).round() / 10.0
    } else {
        0.0
    };
    let distraction_app_analysis = excluded_applications.map(|excluded| {
        crate::focus_semantics::distraction_apps_from_db(&conn, &start_str, &end_str, excluded)
    });
    conn.execute_batch("COMMIT").map_err(|e| e.to_string())?;

    Ok((
        serde_json::json!({
            "source": "local_sqlite",
            "period_start": start_str,
            "period_end": end_str,
            "period_days": days,
            "total_seconds": total_seconds,
            "total_hours": round_hours(total_seconds),
            "activity_count": activity_count,
            "deep_focus_seconds": deep_focus_seconds,
            "deep_focus_hours": round_hours(deep_focus_seconds),
            "focus_eligible_seconds": focus_eligible_seconds,
            "distraction_events": distraction_count,
            "distraction_seconds": distraction_seconds,
            "distraction_hours": round_hours(distraction_seconds),
            "ticketed_seconds": ticketed_seconds,
            "unticketed_seconds": unticketed_seconds,
            "ticketed_hours": round_hours(ticketed_seconds),
            "unticketed_hours": round_hours(unticketed_seconds),
            "ticket_coverage_pct": ticket_coverage_pct,
            "task_labeled_seconds": task_labeled_seconds,
            "task_labeled_hours": round_hours(task_labeled_seconds),
            "task_label_coverage_pct": task_label_coverage_pct,
            "active_days": active_days,
            "tracking_consistency_pct": tracking_consistency_pct,
            "avg_session_minutes": avg_session_minutes,
            "deep_focus_sessions": deep_focus_sessions,
            "focus_semantics": canonical_focus,
            "unsynced_reports": unsynced_count,
            "peak_day": peak_day,
            "quiet_day": quiet_day,
            "peak_focus_hour": peak_focus_hour,
            "prior_period": prior_period,
            "category_breakdown": category_breakdown,
            "ticket_breakdown": ticket_breakdown,
            "daily_totals": daily_totals,
            "day_category_breakdown": day_category_breakdown,
            "work_themes": work_themes,
            "sample_activities": sample_activities,
        }),
        distraction_app_analysis,
    ))
}

const LLM_PASS_TIMEOUT_SECS: u64 = 150;
const LLM_PASS_RETRIES: u32 = 2;

/// TBI-style status report: auto-starts local AI and runs section-by-section generation.
#[tauri::command]
pub fn generate_local_status_report(
    app: tauri::AppHandle,
    state: tauri::State<'_, crate::agent::AgentState>,
    period_days: Option<i32>,
) -> Result<serde_json::Value, String> {
    let db_path = crate::paths::db_path()?;
    let days = period_days.unwrap_or(7).clamp(1, 30);
    let initial_privacy = crate::privacy::load_privacy_settings(&db_path);
    let (mut local_data, distraction_app_analysis) = build_local_insights_report_inner(
        &db_path,
        days,
        initial_privacy
            .as_ref()
            .ok()
            .map(|privacy| privacy.excluded_applications.as_slice()),
    )?;

    let app_handle = app.clone();
    emit_report_progress(
        &app_handle,
        0,
        "warmup",
        crate::language::copy("Starting local AI engine", "Iniciando la IA local"),
        crate::language::copy("Preparing model…", "Preparando el modelo…"),
        "start",
    );
    crate::agent::ensure_local_llm_ready(app, state)?;
    emit_report_progress(
        &app_handle,
        0,
        "warmup",
        crate::language::copy("Starting local AI engine", "Iniciando la IA local"),
        crate::language::copy("Local AI ready", "IA local lista"),
        "done",
    );

    let user_prefs = crate::user_preferences::load_user_preferences(&db_path).unwrap_or_default();
    let prefs_block = crate::user_preferences::preferences_llm_block(&user_prefs);

    let (report, generation_passes) = match generate_report_by_sections(
        &app_handle,
        &local_data,
        &prefs_block,
    ) {
        Ok(result) => result,
        Err(err) => {
            log::warn!(
                "[LocalReport] Pipeline incomplete ({}), merging partial + structured fallback",
                err
            );
            let fallback = build_rule_based_report(&local_data);
            (
                fallback,
                vec![serde_json::json!({
                    "id": "fallback",
                    "label": crate::language::copy("Structured summary", "Resumen estructurado"),
                    "detail": crate::language::copy(
                        "Full AI pipeline could not finish; showing a report from verified local data.",
                        "La IA no pudo terminar; se muestra un informe basado en datos locales verificados."
                    )
                })],
            )
        }
    };

    let mut report = report;
    // The model selects indices only. Host templates and user text must remain
    // verbatim, including non-Latin names, labels, and description samples.
    repair_learning_fields(&mut report, &local_data);

    let ai_powered = generation_passes
        .iter()
        .any(|p| p["source"].as_str() == Some("local_ai_selection"));

    // App names are appended only to the on-device report payload after the
    // local narrative is generated. If exclusions changed during generation,
    // discard the app-level result rather than disclose a newly excluded app.
    // The shared aggregate used by the cloud coach, MCP, and Notion remains unchanged.
    let current_privacy = crate::privacy::load_privacy_settings(&db_path);
    local_data["distraction_app_analysis"] = match (
        initial_privacy,
        current_privacy,
        distraction_app_analysis,
    ) {
        (Ok(initial), Ok(current), Some(Ok(analysis)))
            if initial.excluded_applications == current.excluded_applications =>
        {
            serde_json::json!(analysis)
        }
        (_, _, Some(Err(error))) => {
            log::warn!("[LocalReport] App-level distraction analysis unavailable: {error}");
            serde_json::json!({ "unavailable": true })
        }
        _ => {
            log::warn!("[LocalReport] App-level distraction analysis unavailable: privacy settings changed or could not be loaded");
            serde_json::json!({ "unavailable": true })
        }
    };

    let localized_report = build_localized_report(&report, &local_data);
    let language_key = if crate::language::is_spanish() {
        "es"
    } else {
        "en"
    };
    let visible_report = localized_report[language_key].clone();

    Ok(serde_json::json!({
        "local_data": local_data,
        "report": visible_report,
        "localized_report": localized_report,
        "user_preferences": user_prefs,
        "generated_at": Local::now().format("%Y-%m-%d %H:%M").to_string(),
        "model": "FlowSight Local Vision",
        "ai_powered": ai_powered,
        "generation_passes": generation_passes,
    }))
}

fn call_local_llm_with_system(
    prompt: &str,
    max_tokens: u32,
    temperature: f32,
    system_prompt: &str,
    response_format: &serde_json::Value,
) -> Result<String, String> {
    let chat_url = crate::llama_port::managed_chat_completions_url()
        .ok_or_else(|| "Local AI server offline.".to_string())?;

    let client = Client::builder()
        .timeout(Duration::from_secs(LLM_PASS_TIMEOUT_SECS))
        .build()
        .map_err(|e| e.to_string())?;

    let body = serde_json::json!({
        "model": LLAMA_CHAT_MODEL_ID,
        "messages": [
            {
                "role": "system",
                "content": system_prompt
            },
            { "role": "user", "content": prompt }
        ],
        "temperature": temperature,
        "max_tokens": max_tokens,
        "stream": false,
        "response_format": response_format
    });

    let resp = client
        .post(&chat_url)
        .json(&body)
        .send()
        .map_err(|e| e.to_string())?;

    if !resp.status().is_success() {
        let status = resp.status();
        let err_body = resp.text().unwrap_or_default();
        return Err(format!(
            "Local AI request failed ({}): {}",
            status, err_body
        ));
    }

    let json: serde_json::Value = resp.json().map_err(|e| e.to_string())?;
    let raw = json["choices"][0]["message"]["content"]
        .as_str()
        .unwrap_or("")
        .trim()
        .to_string();

    if raw.is_empty() {
        return Err("Local AI returned empty content.".to_string());
    }

    Ok(raw)
}

fn call_local_llm_json(
    prompt: &str,
    max_tokens: u32,
    temperature: f32,
    fallback: &serde_json::Value,
) -> Result<serde_json::Value, String> {
    let response_format = grounded_selection_response_format(fallback)
        .ok_or_else(|| "No verified candidates to rank.".to_string())?;
    let mut last_err = String::from("unknown error");

    for attempt in 0..=LLM_PASS_RETRIES {
        let raw = match call_local_llm_with_system(
            prompt,
            max_tokens,
            temperature,
            REPORT_SYSTEM_PROMPT,
            &response_format,
        ) {
            Ok(r) => r,
            Err(e) => {
                last_err = e;
                continue;
            }
        };

        match parse_report_json(&raw) {
            Ok(v) if apply_grounded_selection(fallback, &v).is_some() => return Ok(v),
            Ok(_) => {
                last_err = "Local AI selected invalid candidate indices.".to_string();
            }
            Err(e) => {
                last_err = e;
            }
        }
        log::warn!(
            "[LocalReport] Grounded selection attempt {} failed: {}",
            attempt + 1,
            last_err
        );
    }

    Err(last_err)
}

fn emit_report_progress(
    app: &tauri::AppHandle,
    step: u32,
    pass_id: &str,
    label: &str,
    detail: &str,
    phase: &str,
) {
    let payload = serde_json::json!({
        "step": step,
        "pass_id": pass_id,
        "label": label,
        "detail": detail,
        "phase": phase,
    });
    if let Err(e) = app.emit("local-report-progress", payload) {
        log::warn!("[LocalReport] progress emit failed: {}", e);
    }
}

fn report_progress_label(pass_id: &str) -> Option<&'static str> {
    let (en, es) = match pass_id {
        "project_summary" => ("Section — project summary", "Sección: resumen del trabajo"),
        "overall_health" => (
            "Section — overall workflow health",
            "Sección: estado general del trabajo",
        ),
        "health_breakdown" => (
            "Section — health breakdown table",
            "Sección: desglose del estado del trabajo",
        ),
        "timeline_insights" => ("Section — timeline review", "Sección: revisión del período"),
        "known_issues" => ("Section — known issues", "Sección: problemas observados"),
        "potential_risks" => ("Section — potential risks", "Sección: riesgos potenciales"),
        "progress_tasks" => (
            "Section — progress & observed work",
            "Sección: progreso y trabajo observado",
        ),
        "lessons_recommendations" => (
            "Section — lessons & recommendations",
            "Sección: aprendizajes y recomendaciones",
        ),
        _ => return None,
    };
    Some(crate::language::copy(en, es))
}

fn section_detail(result: &serde_json::Value, pass_id: &str) -> String {
    let raw = match pass_id {
        "project_summary" => result["summary"].as_str(),
        "overall_health" => result["overall_health"].as_str(),
        "health_breakdown" => result["health_breakdown"]
            .as_array()
            .and_then(|a| a.first())
            .and_then(|r| r["element"].as_str()),
        "timeline_insights" => result["caption"].as_str(),
        "known_issues" => result["known_issues"]
            .as_array()
            .and_then(|a| a.first())
            .and_then(|v| v.as_str()),
        "potential_risks" => result["potential_risks"]
            .as_array()
            .and_then(|a| a.first())
            .and_then(|v| v.as_str()),
        "progress_tasks" => result["observed_work"]
            .as_array()
            .and_then(|a| a.first())
            .and_then(|v| v.as_str()),
        "lessons_recommendations" => result["lessons_learned"]
            .as_array()
            .and_then(|a| a.first())
            .and_then(|l| l["title"].as_str()),
        _ => None,
    };
    raw.unwrap_or("Section complete.").to_string()
}

#[allow(clippy::too_many_arguments)] // each argument is an explicit generation/evidence control
fn llm_section(
    app: &tauri::AppHandle,
    step: u32,
    pass_id: &str,
    label: &str,
    stats: &str,
    _prompt_body: &str,
    max_tokens: u32,
    temperature: f32,
    fallback: serde_json::Value,
    passes: &mut Vec<serde_json::Value>,
) -> serde_json::Value {
    let label = report_progress_label(pass_id).unwrap_or(label);
    emit_report_progress(
        app,
        step,
        pass_id,
        label,
        crate::language::copy(
            "Selecting from verified local data…",
            "Seleccionando datos locales verificados…",
        ),
        "start",
    );
    log::info!("[LocalReport] Section {} — {}", step, pass_id);
    // The local model may rank verified candidates, but must never author a
    // factual claim that goes straight into a report. The final strings and
    // numbers always come from the SQLite-derived fallback below.
    let (result, source) = match grounded_selection_prompt(stats, &fallback) {
        None => (fallback, "verified_data"),
        Some(prompt) => match call_local_llm_json(&prompt, max_tokens, temperature, &fallback)
            .ok()
            .and_then(|choice| apply_grounded_selection(&fallback, &choice))
        {
            Some(selected) => (selected, "local_ai_selection"),
            None => {
                log::warn!("[LocalReport] Section {} used verified fallback", pass_id);
                (fallback, "verified_data")
            }
        },
    };
    let detail = if crate::language::is_spanish() {
        "Sección completada con datos locales verificados.".to_string()
    } else {
        section_detail(&result, pass_id)
    };
    emit_report_progress(app, step, pass_id, label, &detail, "done");
    passes.push(serde_json::json!({
        "id": pass_id,
        "label": label,
        "detail": detail,
        "source": source,
    }));
    result
}

fn grounded_selection_prompt(stats: &str, fallback: &serde_json::Value) -> Option<String> {
    let candidates = fallback
        .as_object()?
        .iter()
        .filter_map(|(key, value)| {
            value
                .as_array()
                .filter(|items| items.len() > 1)
                .map(|items| (key.clone(), serde_json::Value::Array(items.clone())))
        })
        .collect::<serde_json::Map<String, serde_json::Value>>();
    if candidates.is_empty() {
        return None;
    }
    Some(format!(
        "Rank the most useful verified report items for this period. Return JSON only: \
{{\"selected_indices\":{{\"FIELD\":[0,1]}}}}. Use each CANDIDATES field name exactly. \
For each field, select 1 to 6 distinct zero-based indices that exist in that field, most useful first. \
Do not write or edit report text. The application copies candidate text verbatim; your output is only indices.\n\nSTATS:\n{}\n\nCANDIDATES:\n{}",
        stats,
        serde_json::Value::Object(candidates)
    ))
}

fn grounded_selection_response_format(fallback: &serde_json::Value) -> Option<serde_json::Value> {
    let mut properties = serde_json::Map::new();
    let mut required = Vec::new();
    for (key, value) in fallback.as_object()? {
        let Some(items) = value.as_array().filter(|items| items.len() > 1) else {
            continue;
        };
        required.push(key.clone());
        properties.insert(
            key.clone(),
            serde_json::json!({
                "type": "array",
                "minItems": 1,
                "maxItems": items.len().min(6),
                "items": {"type": "integer", "enum": (0..items.len()).collect::<Vec<_>>()}
            }),
        );
    }
    if required.is_empty() {
        return None;
    }
    Some(serde_json::json!({
        "type": "json_schema",
        "json_schema": {
            "name": "GroundedReportSelection",
            "strict": true,
            "schema": {
                "type": "object", "additionalProperties": false,
                "required": ["selected_indices"],
                "properties": {"selected_indices": {
                    "type": "object", "additionalProperties": false,
                    "required": required, "properties": properties
                }}
            }
        }
    }))
}

/// Fail closed: prose, fabricated values, missing fields, duplicated/out-of-range
/// indices, and extra keys can never be copied into the user-visible report.
fn apply_grounded_selection(
    fallback: &serde_json::Value,
    selection: &serde_json::Value,
) -> Option<serde_json::Value> {
    let selected = selection
        .as_object()?
        .get("selected_indices")?
        .as_object()?;
    if selection.as_object()?.len() != 1 {
        return None;
    }
    let mut report = fallback.clone();
    let choices = fallback.as_object()?;
    let selectable = choices
        .iter()
        .filter_map(|(key, value)| {
            value
                .as_array()
                .filter(|items| items.len() > 1)
                .map(|items| (key, items))
        })
        .collect::<Vec<_>>();
    if selected.len() != selectable.len() {
        return None;
    }
    for (key, candidates) in selectable {
        let indices = selected.get(key)?.as_array()?;
        if indices.is_empty() || indices.len() > candidates.len().min(6) {
            return None;
        }
        let mut seen = std::collections::HashSet::new();
        let mut picked = Vec::with_capacity(indices.len());
        for index in indices {
            let index = usize::try_from(index.as_u64()?).ok()?;
            if !seen.insert(index) {
                return None;
            }
            picked.push(candidates.get(index)?.clone());
        }
        report[key] = serde_json::Value::Array(picked);
    }
    Some(report)
}

fn build_report_meta(local_data: &serde_json::Value) -> serde_json::Value {
    build_report_meta_for(local_data, ReportLanguage::English)
}

fn build_report_meta_for(
    local_data: &serde_json::Value,
    language: ReportLanguage,
) -> serde_json::Value {
    let top_category = local_data["category_breakdown"]
        .as_array()
        .and_then(|a| a.first())
        .and_then(|c| c["category"].as_str())
        .unwrap_or(language.text("General work", "Trabajo general"));

    serde_json::json!({
        "period_label": format!(
            "{} — {}",
            local_data["period_start"].as_str().unwrap_or(""),
            local_data["period_end"].as_str().unwrap_or("")
        ),
        "period_name": match language {
            ReportLanguage::English => format!("Workflow · {}", top_category),
            ReportLanguage::Spanish => format!("Trabajo · {}", top_category),
        },
        "focus_target": top_category,
        "tracked_hours": local_data["total_hours"],
        "deep_focus_hours": local_data["deep_focus_hours"],
        "activity_count": local_data["activity_count"],
    })
}

fn generate_report_by_sections(
    app: &tauri::AppHandle,
    local_data: &serde_json::Value,
    prefs_block: &str,
) -> Result<(serde_json::Value, Vec<serde_json::Value>), String> {
    let mut passes = Vec::new();
    let rule_fallback = build_rule_based_report(local_data);
    let stats =
        |section_id: &str| build_section_stats_snapshot(section_id, local_data, prefs_block);

    let project = llm_section(
        app,
        1,
        "project_summary",
        "Section — project summary",
        &stats("project_summary"),
        "English only. Use ONLY STATS (SQLite activity reports: descriptions, optional task labels, categories, durations).\n\
Use USER_PROFILE to personalize tone and priorities. Reference top categories, sustained-block hours/count, fragmentation, peak day, and explicit task coverage with numbers.\n\
Return JSON: {\"project_name\":\"short focus area label\",\"focus_target\":\"specific next priority aligned with USER_PROFILE\",\"summary\":\"4-6 sentences detailed executive summary citing concrete work items\"}",
        640,
        0.25,
        serde_json::json!({
            "project_name": rule_fallback["work_summary"].as_str().unwrap_or("Work period"),
            "focus_target": build_report_meta(local_data)["focus_target"],
            "summary": rule_fallback["executive_overview"],
        }),
        &mut passes,
    );

    let health = llm_section(
        app,
        2,
        "overall_health",
        "Section — overall workflow health",
        &stats("overall_health"),
        "English only. Use ONLY STATS and USER_PROFILE improvement goals.\n\
Explain observed workflow using sustained-block minutes/count, fragmentation, distraction episodes, tracking coverage, and explicit task coverage. Do not assign a productivity score.\n\
Return JSON: {\"overall_health\":\"Sustained blocks observed|Fragmented eligible work|Insufficient signal\",\"health_notes\":\"detailed paragraph (3-5 sentences) with specific metrics, dates, and personalized recommendations\"}",
        560,
        0.2,
        serde_json::json!({
            "overall_health": rule_fallback["overall_health"],
            "health_notes": rule_fallback["health_notes"],
        }),
        &mut passes,
    );

    let breakdown = llm_section(
        app,
        3,
        "health_breakdown",
        "Section — health breakdown table",
        &stats("health_breakdown"),
        "English only. Use ONLY STATS. Up to 6 rows covering categories and optional task labels where relevant.\n\
Each notes field must cite hours, activity count, or a concrete description sample.\n\
Return JSON: {\"health_breakdown\":[{\"element\":\"work area, category, or task label\",\"status\":\"Sustained-work eligible|Context work|Review|Observed|Uncertain\",\"owner_team\":\"Self\",\"notes\":\"specific 1-2 sentence insight\"}]}",
        720,
        0.25,
        serde_json::json!({ "health_breakdown": rule_fallback["health_breakdown"] }),
        &mut passes,
    );

    let timeline = llm_section(
        app,
        4,
        "timeline_insights",
        "Section — timeline review",
        &stats("timeline_insights"),
        "English only. Use ONLY STATS.\n\
Describe daily rhythm, peak/quiet days, hourly focus peaks, context switching, and period-over-period change.\n\
Return JSON: {\"caption\":\"3-5 sentences detailed timeline narrative with dates and hours\"}",
        480,
        0.25,
        serde_json::json!({
            "caption": rule_fallback["work_progress"].as_array()
                .and_then(|a| a.first())
                .and_then(|v| v.as_str())
                .unwrap_or("Activity tracked across the period.")
        }),
        &mut passes,
    );

    let issues = llm_section(
        app,
        5,
        "known_issues",
        "Section — known issues",
        &stats("known_issues"),
        "English only. Use ONLY STATS. Up to 6 bullets.\n\
Each bullet must name a category, ticket, date, or description pattern from the data.\n\
Return JSON: {\"known_issues\":[\"specific issue with evidence from STATS\"]}",
        520,
        0.25,
        serde_json::json!({ "known_issues": rule_fallback["known_issues"] }),
        &mut passes,
    );

    let risks = llm_section(
        app,
        6,
        "potential_risks",
        "Section — potential risks",
        &stats("potential_risks"),
        "English only. Use ONLY STATS. Up to 6 bullets.\n\
Include risks only when supported by tracking gaps, uncertain task continuity, period change, or fragmented focus. Treat tickets as optional.\n\
Return JSON: {\"potential_risks\":[\"specific risk with evidence\"]}",
        520,
        0.25,
        serde_json::json!({ "potential_risks": rule_fallback["potential_risks"] }),
        &mut passes,
    );

    let progress = llm_section(
        app,
        7,
        "progress_tasks",
        "Section — progress & observed work",
        &stats("progress_tasks"),
        "English only. Use ONLY STATS. Up to 6 items per array.\n\
observed_work: cite task labels and descriptions, but never infer completion. work_progress: cite daily totals and themes.\n\
Return JSON: {\"work_progress\":[\"daily or thematic highlight with date/hours\"],\"observed_work\":[\"specific observed work from task labels or descriptions\"]}",
        680,
        0.25,
        serde_json::json!({
            "work_progress": rule_fallback["work_progress"],
            "observed_work": rule_fallback["observed_work"],
        }),
        &mut passes,
    );

    let lessons = llm_section(
        app,
        8,
        "lessons_recommendations",
        "Section — lessons & recommendations",
        &stats("lessons_recommendations"),
        "English only. Use ONLY STATS and USER_PROFILE. Up to 4 lessons, up to 5 recommendations.\n\
Each lesson body must reference a concrete pattern from the data and the user's stated improvement goals.\n\
Return JSON: {\"lessons_learned\":[{\"title\":\"\",\"body\":\"2-3 sentences\"}],\"recommendations\":[\"actionable step tied to STATS and USER_PROFILE\"]}",
        720,
        0.2,
        serde_json::json!({
            "lessons_learned": rule_fallback["lessons_learned"],
            "recommendations": rule_fallback["recommendations"],
        }),
        &mut passes,
    );

    let meta = build_report_meta(local_data);
    let focus_target = project["focus_target"]
        .as_str()
        .filter(|s| !s.is_empty())
        .map(str::to_string)
        .unwrap_or_else(|| meta["focus_target"].as_str().unwrap_or("Focus").to_string());

    let report = serde_json::json!({
        "report_meta": meta,
        "executive_overview": project["summary"],
        "work_summary": project["summary"],
        "project_name": project["project_name"],
        "focus_target": focus_target,
        "overall_health": health["overall_health"],
        "health_notes": health["health_notes"],
        "health_breakdown": breakdown["health_breakdown"],
        "timeline_caption": timeline["caption"],
        "known_issues": issues["known_issues"],
        "potential_risks": risks["potential_risks"],
        "work_progress": progress["work_progress"],
        "observed_work": progress["observed_work"],
        "lessons_learned": lessons["lessons_learned"],
        "recommendations": lessons["recommendations"],
    });

    Ok((report, passes))
}

fn build_section_stats_snapshot(
    section_id: &str,
    local_data: &serde_json::Value,
    prefs_block: &str,
) -> String {
    let mut lines = build_stats_header_lines(local_data);
    if !prefs_block.is_empty() {
        lines.push(clamp_line(
            &prefs_block.split_whitespace().collect::<Vec<_>>().join(" "),
            LLM_PROFILE_MAX_CHARS,
        ));
        lines.push(
            "Personalize insights for USER_PROFILE roles, activities, and improvement goals."
                .to_string(),
        );
    }

    match section_id {
        "project_summary" => {
            append_top_categories(&mut lines, local_data, 8);
            append_top_tickets(&mut lines, local_data, 8);
            append_work_themes(&mut lines, local_data, 6);
            append_peak_quiet_day(&mut lines, local_data);
            append_activity_samples(&mut lines, local_data, 10, 120);
        }
        "overall_health" => {
            append_health_metrics(&mut lines, local_data);
            append_top_categories(&mut lines, local_data, 6);
            append_hourly_focus(&mut lines, local_data, 5);
            append_longest_sessions(&mut lines, local_data, 5);
        }
        "health_breakdown" => {
            append_category_detail(&mut lines, local_data);
            append_top_tickets(&mut lines, local_data, 10);
            append_work_themes(&mut lines, local_data, 8);
        }
        "timeline_insights" => {
            append_daily_detail(&mut lines, local_data);
            append_day_categories(&mut lines, local_data);
            append_hourly_focus(&mut lines, local_data, 8);
            append_prior_period(&mut lines, local_data);
            append_longest_sessions(&mut lines, local_data, 6);
        }
        "known_issues" => {
            append_health_metrics(&mut lines, local_data);
            append_distraction_detail(&mut lines, local_data);
            append_activity_samples(&mut lines, local_data, 12, 100);
        }
        "potential_risks" => {
            append_health_metrics(&mut lines, local_data);
            append_daily_detail(&mut lines, local_data);
            append_prior_period(&mut lines, local_data);
            append_top_tickets(&mut lines, local_data, 5);
        }
        "progress_tasks" => {
            append_daily_detail(&mut lines, local_data);
            append_activity_samples(&mut lines, local_data, 10, 80);
            append_top_tickets(&mut lines, local_data, 8);
            append_longest_sessions(&mut lines, local_data, 5);
            append_work_themes(&mut lines, local_data, 5);
        }
        _ => {
            append_health_metrics(&mut lines, local_data);
            append_top_categories(&mut lines, local_data, 6);
            append_top_tickets(&mut lines, local_data, 6);
            append_hourly_focus(&mut lines, local_data, 4);
            append_prior_period(&mut lines, local_data);
            append_work_themes(&mut lines, local_data, 6);
            append_longest_sessions(&mut lines, local_data, 4);
        }
    }

    pack_stats_lines(lines, LLM_SECTION_STATS_MAX_CHARS)
}

fn deep_threshold_minutes(local_data: &serde_json::Value) -> i64 {
    local_data["focus_semantics"]["deep_threshold_seconds"]
        .as_i64()
        .unwrap_or(crate::focus_semantics::DEEP_TIER_SECS)
        / 60
}

fn build_stats_header_lines(local_data: &serde_json::Value) -> Vec<String> {
    let period_start = local_data["period_start"].as_str().unwrap_or("?");
    let period_end = local_data["period_end"].as_str().unwrap_or("?");
    let days = local_data["period_days"].as_i64().unwrap_or(7);
    let total_h = local_data["total_hours"].as_f64().unwrap_or(0.0);
    let focus_h = local_data["deep_focus_hours"].as_f64().unwrap_or(0.0);
    let activities = local_data["activity_count"].as_i64().unwrap_or(0);
    let distractions = local_data["distraction_events"].as_i64().unwrap_or(0);
    let distraction_h = local_data["distraction_hours"].as_f64().unwrap_or(0.0);
    let deep_minutes = deep_threshold_minutes(local_data);

    vec![
        format!("PERIOD: {} to {} ({} days)", period_start, period_end, days),
        format!(
            "TOTAL: {:.1}h | SUSTAINED {}m+ BLOCKS: {:.1}h | ACTIVITIES: {} | SUSTAINED NON-WORK BROWSING: {} events ({:.1}h)",
            total_h, deep_minutes, focus_h, activities, distractions, distraction_h
        ),
    ]
}

fn append_health_metrics(lines: &mut Vec<String>, local_data: &serde_json::Value) {
    let task_label_cov = local_data["task_label_coverage_pct"]
        .as_f64()
        .unwrap_or(0.0);
    let task_labeled_h = local_data["task_labeled_hours"].as_f64().unwrap_or(0.0);
    let active_days = local_data["active_days"].as_i64().unwrap_or(0);
    let consistency = local_data["tracking_consistency_pct"]
        .as_f64()
        .unwrap_or(0.0);
    let avg_session = local_data["avg_session_minutes"].as_f64().unwrap_or(0.0);
    let deep_focus = local_data["deep_focus_sessions"].as_i64().unwrap_or(0);
    let switches = local_data["focus_semantics"]["explicit_theme_switches_per_labelled_focus_hour"]
        .as_f64()
        .unwrap_or(0.0);
    let focus_theme_coverage = local_data["focus_semantics"]["explicit_theme_coverage_pct"]
        .as_f64()
        .unwrap_or(0.0);
    let unsynced = local_data["unsynced_reports"].as_i64().unwrap_or(0);
    let deep_minutes = deep_threshold_minutes(local_data);

    lines.push(format!(
        "PATTERN: {:.0}% days tracked ({}/{}d) | avg observed focus block {:.0}m | sustained sessions {}m+: {} | explicit theme switches/labelled focus hour: {:.1}",
        consistency,
        active_days,
        local_data["period_days"].as_i64().unwrap_or(7),
        avg_session,
        deep_minutes,
        deep_focus,
        switches
    ));
    lines.push(format!(
        "TASK CONTEXT: {:.1}h explicitly labelled ({:.0}% of all tracked time; manual task or optional ticket) | {:.0}% of focus-eligible time has an explicit theme | {} unsynced local reports",
        task_labeled_h, task_label_cov, focus_theme_coverage, unsynced
    ));
}

fn append_top_categories(lines: &mut Vec<String>, local_data: &serde_json::Value, limit: usize) {
    if let Some(cats) = local_data["category_breakdown"].as_array() {
        let top: Vec<String> = cats
            .iter()
            .take(limit)
            .map(|c| {
                let name = c["category"].as_str().unwrap_or("?");
                let h = c["total_seconds"].as_i64().unwrap_or(0) as f64 / 3600.0;
                let n = c["count"].as_i64().unwrap_or(0);
                format!("{} {:.1}h ({} activities)", name, h, n)
            })
            .collect();
        if !top.is_empty() {
            lines.push(format!("CATEGORIES: {}", top.join(" | ")));
        }
    }
}

fn append_category_detail(lines: &mut Vec<String>, local_data: &serde_json::Value) {
    let total = local_data["total_seconds"].as_i64().unwrap_or(1).max(1) as f64;
    if let Some(cats) = local_data["category_breakdown"].as_array() {
        for c in cats.iter().take(10) {
            let name = c["category"].as_str().unwrap_or("?");
            let secs = c["total_seconds"].as_i64().unwrap_or(0) as f64;
            let n = c["count"].as_i64().unwrap_or(0);
            let pct = (secs / total * 1000.0).round() / 10.0;
            lines.push(format!(
                "- CAT {}: {:.1}h, {} activities, {:.1}% of period",
                name,
                secs / 3600.0,
                n,
                pct
            ));
        }
    }
}

fn append_top_tickets(lines: &mut Vec<String>, local_data: &serde_json::Value, limit: usize) {
    if let Some(tickets) = local_data["ticket_breakdown"].as_array() {
        let top: Vec<String> = tickets
            .iter()
            .take(limit)
            .map(|t| {
                let id = t["ticket"].as_str().unwrap_or("?");
                let h = t["total_seconds"].as_i64().unwrap_or(0) as f64 / 3600.0;
                let n = t["count"].as_i64().unwrap_or(0);
                format!("{} {:.1}h ({} captures)", id, h, n)
            })
            .collect();
        if !top.is_empty() {
            lines.push(format!("TICKETS: {}", top.join(" | ")));
        }
    }
}

fn append_daily_detail(lines: &mut Vec<String>, local_data: &serde_json::Value) {
    if let Some(days_arr) = local_data["daily_totals"].as_array() {
        if !days_arr.is_empty() {
            let daily = days_arr
                .iter()
                .map(|d| {
                    format!(
                        "{} {:.1}h/{}obs",
                        d["date"].as_str().unwrap_or("?"),
                        d["total_seconds"].as_i64().unwrap_or(0) as f64 / 3600.0,
                        d["activity_count"].as_i64().unwrap_or(0)
                    )
                })
                .collect::<Vec<_>>();
            lines.push(format!(
                "DAILY_TOTALS (observed days): {}",
                daily.join(" | ")
            ));
        }
    }
}

fn append_day_categories(lines: &mut Vec<String>, local_data: &serde_json::Value) {
    if let Some(days) = local_data["day_category_breakdown"].as_array() {
        let sampled = temporal_coverage_indices(days.len(), 8)
            .into_iter()
            .map(|index| {
                let d = &days[index];
                format!(
                    "{} {} {:.1}/{:.1}h",
                    d["date"].as_str().unwrap_or("?"),
                    clamp_line(d["top_category"].as_str().unwrap_or("?"), 24),
                    d["top_hours"].as_f64().unwrap_or(0.0),
                    d["total_hours"].as_f64().unwrap_or(0.0),
                )
            })
            .collect::<Vec<_>>();
        if !sampled.is_empty() {
            lines.push(format!(
                "DAY_TOP_CATEGORIES (sample): {}",
                sampled.join(" | ")
            ));
        }
    }
}

fn append_hourly_focus(lines: &mut Vec<String>, local_data: &serde_json::Value, limit: usize) {
    if let Some(hours) = local_data["focus_semantics"]["hourly_deep_focus"].as_array() {
        let mut ranked = hours
            .iter()
            .filter(|hour| hour["seconds"].as_i64().unwrap_or(0) > 0)
            .collect::<Vec<_>>();
        ranked.sort_by_key(|hour| std::cmp::Reverse(hour["seconds"].as_i64().unwrap_or(0)));
        let top: Vec<String> = ranked
            .into_iter()
            .take(limit)
            .map(|h| {
                format!(
                    "{}:00 {}m deep focus",
                    h["hour"].as_u64().unwrap_or(0),
                    h["seconds"].as_i64().unwrap_or(0) / 60
                )
            })
            .collect();
        if !top.is_empty() {
            lines.push(format!("DEEP_FOCUS_HOURS: {}", top.join(", ")));
        }
    }
}

fn append_work_themes(lines: &mut Vec<String>, local_data: &serde_json::Value, limit: usize) {
    if let Some(themes) = local_data["work_themes"].as_array() {
        for t in themes.iter().take(limit) {
            let label = t["label"].as_str().unwrap_or("?");
            let h = t["total_seconds"].as_i64().unwrap_or(0) as f64 / 3600.0;
            let n = t["activity_count"].as_i64().unwrap_or(0);
            lines.push(format!("- THEME {}: {:.1}h, {} captures", label, h, n));
        }
    }
}

fn append_longest_sessions(lines: &mut Vec<String>, local_data: &serde_json::Value, limit: usize) {
    if let Some(sessions) = local_data["focus_semantics"]["sessions"].as_array() {
        let mut ranked = sessions.iter().collect::<Vec<_>>();
        ranked.sort_by_key(|session| {
            std::cmp::Reverse(session["focus_seconds"].as_i64().unwrap_or(0))
        });
        for s in ranked.into_iter().take(limit) {
            let theme = s["theme"].as_str().unwrap_or("");
            let theme_part = if theme.is_empty() {
                String::new()
            } else {
                format!(" [{}]", theme)
            };
            let categories = s["category_mix"]
                .as_array()
                .map(|mix| {
                    mix.iter()
                        .filter_map(|c| c["category"].as_str())
                        .collect::<Vec<_>>()
                        .join("+")
                })
                .unwrap_or_else(|| "Work".to_string());
            lines.push(format!(
                "- SESSION {} {}{} {}m ({})",
                s["start"].as_str().unwrap_or(""),
                categories,
                theme_part,
                s["focus_seconds"].as_i64().unwrap_or(0) / 60,
                s["tier"].as_str().unwrap_or("fragment")
            ));
        }
    }
}

fn append_activity_samples(
    lines: &mut Vec<String>,
    local_data: &serde_json::Value,
    limit: usize,
    desc_max: usize,
) {
    lines.push("ACTIVITY_LOG:".to_string());
    if let Some(samples) = local_data["sample_activities"].as_array() {
        for s in samples.iter().take(limit) {
            let ticket = s["ticket"].as_str().unwrap_or("");
            let ticket_part = if ticket.is_empty() {
                String::new()
            } else {
                format!(" [{}]", ticket)
            };
            lines.push(format!(
                "- {} {}{} {}m: {}",
                s["date"].as_str().unwrap_or(""),
                s["category"].as_str().unwrap_or(""),
                ticket_part,
                s["duration_seconds"].as_i64().unwrap_or(0) / 60,
                clamp_line(s["description"].as_str().unwrap_or(""), desc_max)
            ));
        }
    }
}

fn append_peak_quiet_day(lines: &mut Vec<String>, local_data: &serde_json::Value) {
    if let Some(peak) = local_data.get("peak_day") {
        lines.push(format!(
            "PEAK_DAY: {} {:.1}h ({} activities)",
            peak["date"].as_str().unwrap_or("?"),
            peak["hours"].as_f64().unwrap_or(0.0),
            peak["activities"].as_i64().unwrap_or(0)
        ));
    }
    if let Some(quiet) = local_data.get("quiet_day") {
        lines.push(format!(
            "QUIET_DAY: {} {:.1}h ({} activities)",
            quiet["date"].as_str().unwrap_or("?"),
            quiet["hours"].as_f64().unwrap_or(0.0),
            quiet["activities"].as_i64().unwrap_or(0)
        ));
    }
    if let Some(hour) = local_data.get("peak_focus_hour") {
        lines.push(format!(
            "PEAK_DEEP_FOCUS_HOUR: {}:00 ({} deep-focus minutes)",
            hour["hour"].as_u64().unwrap_or(0),
            hour["deep_focus_minutes"].as_i64().unwrap_or(0)
        ));
    }
}

fn append_prior_period(lines: &mut Vec<String>, local_data: &serde_json::Value) {
    if let Some(prior) = local_data.get("prior_period") {
        let prev_h = prior["total_hours"].as_f64().unwrap_or(0.0);
        let change = prior["change_pct"].as_f64().unwrap_or(0.0);
        lines.push(format!(
            "PRIOR_PERIOD ({} to {}): {:.1}h total, {:.1}h deep focus, {} activities | change vs prior: {:+.0}%",
            prior["period_start"].as_str().unwrap_or("?"),
            prior["period_end"].as_str().unwrap_or("?"),
            prev_h,
            prior["deep_focus_hours"].as_f64().unwrap_or(0.0),
            prior["activity_count"].as_i64().unwrap_or(0),
            change
        ));
    }
}

fn append_distraction_detail(lines: &mut Vec<String>, local_data: &serde_json::Value) {
    let events = local_data["focus_semantics"]["distraction_events"]
        .as_i64()
        .unwrap_or(0);
    let minutes = local_data["focus_semantics"]["distraction_seconds"]
        .as_i64()
        .unwrap_or(0)
        / 60;
    if events > 0 {
        lines.push(format!(
            "- SUSTAINED NON-WORK BROWSING: {}m across {} canonical events",
            minutes, events
        ));
    }
}

fn pack_stats_lines(lines: Vec<String>, max_chars: usize) -> String {
    let mut selected = Vec::new();
    let mut used = 0;
    for line in lines {
        let cost = line.chars().count() + usize::from(!selected.is_empty());
        if used + cost <= max_chars {
            used += cost;
            selected.push(line);
        }
    }
    selected.join("\n")
}

fn extract_json_block(raw: &str) -> String {
    let trimmed = raw.trim();
    if trimmed.starts_with('{') {
        return trimmed.to_string();
    }
    if let Some(start) = trimmed.find('{') {
        if let Some(end) = trimmed.rfind('}') {
            return trimmed[start..=end].to_string();
        }
        return trimmed[start..].to_string();
    }
    trimmed.to_string()
}

fn close_json_brackets(s: &str) -> String {
    let mut result = s.trim().trim_end_matches(',').to_string();
    if result.ends_with(':') {
        result.pop();
        result = result.trim_end_matches(',').to_string();
    }
    if result.ends_with("\"") {
        // dangling key with no value — trim back
    } else if result.ends_with("\":") {
        result.pop();
        result.pop();
        result = result.trim_end_matches(',').to_string();
    }

    let open_brackets = result.chars().filter(|&c| c == '[').count();
    let close_brackets = result.chars().filter(|&c| c == ']').count();
    let open_braces = result.chars().filter(|&c| c == '{').count();
    let close_braces = result.chars().filter(|&c| c == '}').count();

    for _ in 0..open_brackets.saturating_sub(close_brackets) {
        result.push(']');
    }
    for _ in 0..open_braces.saturating_sub(close_braces) {
        result.push('}');
    }
    result
}

fn parse_report_json(raw: &str) -> Result<serde_json::Value, String> {
    let json_str = extract_json_block(raw);

    if let Ok(v) = serde_json::from_str::<serde_json::Value>(&json_str) {
        return Ok(v);
    }

    let repaired = close_json_brackets(&json_str);
    if let Ok(v) = serde_json::from_str::<serde_json::Value>(&repaired) {
        return Ok(v);
    }

    let chars: Vec<char> = json_str.chars().collect();
    for end in (20..chars.len()).rev() {
        let chunk: String = chars[..end].iter().collect();
        let candidate = close_json_brackets(&chunk);
        if let Ok(v) = serde_json::from_str::<serde_json::Value>(&candidate) {
            return Ok(v);
        }
    }

    Err(format!(
        "Invalid JSON from local AI: could not parse or repair response ({} chars)",
        json_str.chars().count()
    ))
}

#[cfg(test)]
fn is_cjk_char(c: char) -> bool {
    matches!(
        c,
        '\u{4E00}'..='\u{9FFF}'
            | '\u{3400}'..='\u{4DBF}'
            | '\u{3040}'..='\u{30FF}'
            | '\u{AC00}'..='\u{D7AF}'
    )
}

#[cfg(test)]
fn latin_ratio(s: &str) -> f64 {
    let mut latin = 0u32;
    let mut letters = 0u32;
    for c in s.chars() {
        if c.is_alphabetic() {
            letters += 1;
            if c.is_ascii() {
                latin += 1;
            }
        }
    }
    if letters == 0 {
        return 1.0;
    }
    latin as f64 / letters as f64
}

/// Keep English/Latin segments; drop CJK and low-Latin sentences from model output.
#[cfg(test)]
fn extract_english_text(s: &str) -> String {
    let cleaned: String = s
        .chars()
        .map(|c| if is_cjk_char(c) { ' ' } else { c })
        .collect();

    let mut raw_segments = Vec::new();
    let mut start = 0;
    for (index, character) in cleaned.char_indices() {
        let next_is_space = match cleaned[index + character.len_utf8()..].chars().next() {
            None => true,
            Some(next) => next.is_whitespace(),
        };
        if character == '\n' || (matches!(character, '.' | '!' | '?') && next_is_space) {
            raw_segments.push(&cleaned[start..index]);
            start = index + character.len_utf8();
        }
    }
    raw_segments.push(&cleaned[start..]);

    let segments: Vec<String> = raw_segments
        .into_iter()
        .map(str::trim)
        .filter(|seg| !seg.is_empty() && latin_ratio(seg) >= 0.55)
        .map(|seg| seg.split_whitespace().collect::<Vec<_>>().join(" "))
        .collect();

    if segments.is_empty() {
        return cleaned.split_whitespace().collect::<Vec<_>>().join(" ");
    }

    let mut out = segments.join(". ");
    if !out.ends_with('.') && s.trim_end().ends_with('.') {
        out.push('.');
    }
    out
}

fn repair_learning_fields(report: &mut serde_json::Value, local_data: &serde_json::Value) {
    let verified_lessons = build_lessons_learned(local_data);
    let mut lessons: Vec<serde_json::Value> = report["lessons_learned"]
        .as_array()
        .into_iter()
        .flatten()
        .filter(|lesson| verified_lessons.contains(lesson))
        .cloned()
        .collect();

    // Only verbatim, locally derived lessons are allowed in the final report.
    if local_data["total_seconds"].as_i64().unwrap_or(0) <= 0 {
        lessons.clear();
    } else if lessons.is_empty() {
        lessons = verified_lessons;
    }
    report["lessons_learned"] = serde_json::json!(lessons);

    let verified_recommendations = default_recommendations(local_data);
    let mut recommendations: Vec<String> = report["recommendations"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|item| item.as_str().map(str::trim))
        .filter(|item| {
            verified_recommendations
                .iter()
                .any(|verified| verified == item)
        })
        .map(str::to_string)
        .collect();
    if recommendations.is_empty() {
        recommendations = verified_recommendations;
    }
    report["recommendations"] = serde_json::json!(recommendations);
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ReportLanguage {
    English,
    Spanish,
}

impl ReportLanguage {
    fn text(self, en: &'static str, es: &'static str) -> &'static str {
        match self {
            Self::English => en,
            Self::Spanish => es,
        }
    }
}

// Paired literals share the same arguments and rule branches. Language never
// changes which evidence is included, its precision, or candidate ordering.
macro_rules! report_format {
    ($language:expr, $en:literal, $es:literal $(, $argument:expr)* $(,)?) => {
        match $language {
            ReportLanguage::English => format!($en $(, $argument)*),
            ReportLanguage::Spanish => format!($es $(, $argument)*),
        }
    };
}

fn build_rule_based_report(local_data: &serde_json::Value) -> serde_json::Value {
    build_rule_based_report_for(local_data, ReportLanguage::English)
}

fn build_lessons_learned(local_data: &serde_json::Value) -> Vec<serde_json::Value> {
    build_lessons_learned_for(local_data, ReportLanguage::English)
}

fn default_recommendations(local_data: &serde_json::Value) -> Vec<String> {
    default_recommendations_for(local_data, ReportLanguage::English)
}

#[cfg(test)]
fn build_work_progress(local_data: &serde_json::Value) -> Vec<String> {
    build_work_progress_for(local_data, ReportLanguage::English)
}

#[cfg(test)]
fn build_known_issues(local_data: &serde_json::Value, distraction_events: i32) -> Vec<String> {
    build_known_issues_for(local_data, distraction_events, ReportLanguage::English)
}

#[cfg(test)]
fn build_potential_risks(local_data: &serde_json::Value) -> Vec<String> {
    build_potential_risks_for(local_data, ReportLanguage::English)
}

/// Match host-authored candidates within their own field. The model may reorder
/// or select a subset, so array position in the final report is not sufficient.
/// Unmatched values remain verbatim; raw user content is never translated.
fn matching_report_copy(
    selected: &serde_json::Value,
    english: &serde_json::Value,
    spanish: &serde_json::Value,
) -> serde_json::Value {
    if selected == english {
        return spanish.clone();
    }
    match (selected, english, spanish) {
        (
            serde_json::Value::Array(chosen),
            serde_json::Value::Array(en),
            serde_json::Value::Array(es),
        ) => serde_json::Value::Array(
            chosen
                .iter()
                .map(|item| {
                    en.iter()
                        .position(|candidate| candidate == item)
                        .and_then(|index| es.get(index))
                        .cloned()
                        .unwrap_or_else(|| item.clone())
                })
                .collect(),
        ),
        (
            serde_json::Value::Object(chosen),
            serde_json::Value::Object(en),
            serde_json::Value::Object(es),
        ) => serde_json::Value::Object(
            chosen
                .iter()
                .map(|(key, value)| {
                    let copy = match (en.get(key), es.get(key)) {
                        (Some(en), Some(es)) => matching_report_copy(value, en, es),
                        _ => value.clone(),
                    };
                    (key.clone(), copy)
                })
                .collect(),
        ),
        _ => selected.clone(),
    }
}

fn build_localized_report(
    selected: &serde_json::Value,
    local_data: &serde_json::Value,
) -> serde_json::Value {
    let mut english = build_rule_based_report(local_data);
    let mut spanish = build_rule_based_report_for(local_data, ReportLanguage::Spanish);
    for (reference, language) in [
        (&mut english, ReportLanguage::English),
        (&mut spanish, ReportLanguage::Spanish),
    ] {
        let meta = build_report_meta_for(local_data, language);
        reference["project_name"] = reference["work_summary"].clone();
        reference["focus_target"] = meta["focus_target"].clone();
        reference["report_meta"] = meta;
        reference["timeline_caption"] = reference["work_progress"]
            .as_array()
            .and_then(|items| items.first())
            .cloned()
            .unwrap_or_else(|| {
                serde_json::json!(language.text(
                    "Activity tracked across the period.",
                    "Actividad registrada a lo largo del período.",
                ))
            });
    }
    // The section pipeline aliases work_summary to executive_overview. The full
    // structured fallback uses a distinct work_summary; support both shapes.
    if selected["work_summary"] == english["executive_overview"] {
        english["work_summary"] = english["executive_overview"].clone();
        spanish["work_summary"] = spanish["executive_overview"].clone();
    }
    serde_json::json!({
        "en": selected,
        "es": matching_report_copy(selected, &english, &spanish),
    })
}

fn build_rule_based_report_for(
    local_data: &serde_json::Value,
    language: ReportLanguage,
) -> serde_json::Value {
    let total_hours = local_data["total_hours"].as_f64().unwrap_or(0.0);
    let deep_focus_hours = local_data["deep_focus_hours"].as_f64().unwrap_or(0.0);
    let activity_count = local_data["activity_count"].as_u64().unwrap_or(0);
    let distraction_events = local_data["distraction_events"].as_u64().unwrap_or(0);
    let period_start = local_data["period_start"].as_str().unwrap_or("");
    let period_end = local_data["period_end"].as_str().unwrap_or("");
    let deep_minutes = deep_threshold_minutes(local_data);

    let total_seconds = local_data["total_seconds"].as_i64().unwrap_or(0) as i32;
    let overall_health = compute_overall_health_for(local_data, language);

    let health_breakdown = build_category_health_rows_for(local_data, language);
    let observed_work = build_observed_work_for(local_data, language);
    let known_issues = build_known_issues_for(local_data, distraction_events as i32, language);
    let potential_risks = build_potential_risks_for(local_data, language);
    let work_progress = build_work_progress_for(local_data, language);
    let lessons_learned = build_lessons_learned_for(local_data, language);

    let executive_overview = if total_seconds == 0 {
        report_format!(language,
            "Between {} and {} no activity was recorded in local SQLite reports. Enable monitoring to populate this report.", "Entre {} y {} no se registró actividad en los informes locales de SQLite. Activa el seguimiento para completar este informe.",
            period_start, period_end
        )
    } else {
        let task_label_cov = local_data["task_label_coverage_pct"]
            .as_f64()
            .unwrap_or(0.0);
        let active_days = local_data["active_days"].as_i64().unwrap_or(0);
        let top_cat = local_data["category_breakdown"]
            .as_array()
            .and_then(|a| a.first())
            .and_then(|c| c["category"].as_str())
            .unwrap_or(language.text("General work", "Trabajo general"));
        report_format!(language,
            "Between {} and {} you tracked {:.1}h across {} SQLite activity reports on {} active days. \
Sustained {}+ minute blocks totalled {:.1}h. Primary category: {}. \
Explicit task labels covered {:.0}% of tracked time.", "Entre {} y {} registraste {:.1}h en {} informes de actividad de SQLite durante {} días activos. \
Los bloques sostenidos de {} minutos o más sumaron {:.1}h. Categoría principal: {}. \
Las etiquetas explícitas de tareas cubrieron el {:.0}% del tiempo registrado.",
            period_start,
            period_end,
            total_hours,
            activity_count,
            active_days,
            deep_minutes,
            deep_focus_hours,
            top_cat,
            task_label_cov
        )
    };

    let health_notes = if total_seconds == 0 {
        language.text("No tracked activity in this period. Start monitoring to build a baseline.", "No hay actividad registrada en este período. Inicia el seguimiento para obtener una referencia.").to_string()
    } else {
        let deep = local_data["deep_focus_sessions"].as_i64().unwrap_or(0);
        let fragmentation = local_data["focus_semantics"]["fragmentation_pct"]
            .as_f64()
            .unwrap_or(0.0);
        let consistency = local_data["tracking_consistency_pct"]
            .as_f64()
            .unwrap_or(0.0);
        let switches = local_data["focus_semantics"]
            ["explicit_theme_switches_per_labelled_focus_hour"]
            .as_f64()
            .unwrap_or(0.0);
        let distraction_h = local_data["distraction_hours"].as_f64().unwrap_or(0.0);
        report_format!(language,
            "Observed {} sessions of {}+ minutes; {:.0}% of focus-eligible time remained in shorter fragments. \
Tracking consistency was {:.0}% of days in the period. Sustained non-work browsing totalled {:.1}h across {} canonical events. \
Observed explicit theme changes averaged {:.1} per labelled focus hour.", "Se observaron {} sesiones de {} minutos o más; el {:.0}% del tiempo elegible para concentración quedó en fragmentos más cortos. \
Hubo seguimiento en el {:.0}% de los días del período. La navegación sostenida ajena al trabajo sumó {:.1}h en {} eventos canónicos. \
Los cambios explícitos de tema observados promediaron {:.1} por hora de concentración con etiquetas.",
            deep,
            deep_minutes,
            fragmentation,
            consistency,
            distraction_h,
            distraction_events,
            switches
        )
    };

    serde_json::json!({
        "executive_overview": executive_overview,
        "work_summary": report_format!(language,
            "The breakdown lists observed work areas and optional task labels. {:.1} total hours were captured in the local report.", "El desglose enumera las áreas de trabajo observadas y las etiquetas opcionales de tareas. El informe local registró {:.1} horas en total.",
            total_hours
        ),
        "overall_health": overall_health,
        "health_notes": health_notes,
        "health_breakdown": health_breakdown,
        "known_issues": known_issues,
        "potential_risks": potential_risks,
        "observed_work": observed_work,
        "work_progress": work_progress,
        "lessons_learned": lessons_learned,
        "recommendations": default_recommendations_for(local_data, language),
    })
}

fn compute_overall_health_for(
    local_data: &serde_json::Value,
    language: ReportLanguage,
) -> &'static str {
    let eligible = local_data["focus_eligible_seconds"].as_i64().unwrap_or(0);
    let deep_sessions = local_data["deep_focus_sessions"].as_i64().unwrap_or(0);
    if eligible == 0 {
        language.text("No sustained-work signal", "Sin señal de trabajo sostenido")
    } else if deep_sessions > 0 {
        language.text(
            "Sustained blocks observed",
            "Se observaron bloques sostenidos",
        )
    } else {
        language.text("Fragmented eligible work", "Trabajo elegible fragmentado")
    }
}

fn build_category_health_rows_for(
    local_data: &serde_json::Value,
    language: ReportLanguage,
) -> Vec<serde_json::Value> {
    let total_seconds = local_data["total_seconds"].as_i64().unwrap_or(1).max(1) as f64;
    let mut rows = Vec::new();

    if let Some(cats) = local_data["category_breakdown"].as_array() {
        for cat in cats.iter().take(6) {
            let name = cat["category"]
                .as_str()
                .unwrap_or(language.text("Other", "Otros"));
            let secs = cat["total_seconds"].as_i64().unwrap_or(0) as f64;
            let share = secs / total_seconds;
            let status = match crate::focus_semantics::focus_role(
                cat["category"].as_str().unwrap_or("Other"),
            ) {
                crate::focus_semantics::FocusRole::Eligible => {
                    language.text("Sustained-work eligible", "Elegible para trabajo sostenido")
                }
                crate::focus_semantics::FocusRole::Coordination
                | crate::focus_semantics::FocusRole::Operational => {
                    language.text("Context work", "Trabajo de contexto")
                }
                crate::focus_semantics::FocusRole::Distraction if share > 0.15 => {
                    language.text("Review", "Revisar")
                }
                crate::focus_semantics::FocusRole::Distraction => {
                    language.text("Observed", "Observado")
                }
                crate::focus_semantics::FocusRole::MeasurementNoise => {
                    language.text("Uncertain", "Incierto")
                }
                crate::focus_semantics::FocusRole::Unknown => {
                    language.text("Unclassified", "Sin clasificar")
                }
            };
            rows.push(serde_json::json!({
                "element": name,
                "status": status,
                "owner_team": language.text("Self", "Yo"),
                "notes": report_format!(language,
                    "{:.1}h across {} SQLite reports ({:.0}% of period).", "{:.1}h en {} informes de SQLite ({:.0}% del período).",
                    secs / 3600.0,
                    cat["count"].as_i64().unwrap_or(0),
                    share * 100.0
                ),
            }));
        }
    }

    if rows.is_empty() {
        rows.push(serde_json::json!({
            "element": language.text("Tracking", "Seguimiento"),
            "status": language.text("Attention", "Requiere atención"),
            "owner_team": language.text("Self", "Yo"),
            "notes": language.text("No category data yet — enable monitoring during work sessions.", "Aún no hay datos por categoría; activa el seguimiento durante tus sesiones de trabajo."),
        }));
    }

    if let Some(tickets) = local_data["ticket_breakdown"].as_array() {
        for t in tickets.iter().take(3) {
            let ticket = t["ticket"].as_str().unwrap_or("");
            let secs = t["total_seconds"].as_i64().unwrap_or(0) as f64;
            let n = t["count"].as_i64().unwrap_or(0);
            if !ticket.is_empty() {
                rows.push(serde_json::json!({
                    "element": ticket,
                    "status": language.text("Observed", "Observado"),
                    "owner_team": language.text("Self", "Yo"),
                    "notes": report_format!(language,"{:.1}h logged across {} SQLite activity observations.", "{:.1}h registradas en {} observaciones de actividad de SQLite.", secs / 3600.0, n),
                }));
            }
        }
    }

    rows
}

fn build_observed_work_for(
    local_data: &serde_json::Value,
    language: ReportLanguage,
) -> Vec<String> {
    let mut items: Vec<String> = Vec::new();
    let mut english_candidates: Vec<String> = Vec::new();

    if let Some(tickets) = local_data["ticket_breakdown"].as_array() {
        for t in tickets.iter().take(6) {
            let ticket = t["ticket"].as_str().unwrap_or("");
            let hours = t["total_seconds"].as_i64().unwrap_or(0) as f64 / 3600.0;
            let n = t["count"].as_i64().unwrap_or(0);
            // Preserve the existing candidate deduplication in both languages.
            // A ticket matching localized template prose must not change which
            // candidates exist when the language changes.
            if !ticket.is_empty() && !english_candidates.iter().any(|i| i.contains(ticket)) {
                english_candidates.push(format!(
                    "Ticket {} — {:.1}h logged across {} SQLite activity reports",
                    ticket, hours, n
                ));
                items.push(report_format!(
                    language,
                    "Ticket {} — {:.1}h logged across {} SQLite activity reports",
                    "Ticket {} — {:.1}h registradas en {} informes de actividad de SQLite",
                    ticket,
                    hours,
                    n
                ));
            }
        }
    }

    if items.is_empty() {
        if let Some(samples) = local_data["sample_activities"].as_array() {
            for s in samples.iter().take(6) {
                let desc = s["description"].as_str().unwrap_or("");
                let cat = s["category"]
                    .as_str()
                    .unwrap_or(language.text("Work", "Trabajo"));
                if !desc.is_empty() {
                    items.push(format!("{} — {}", cat, desc));
                }
            }
        }
    }

    if items.is_empty() {
        items.push(
            language
                .text(
                    "No specifically labelled work was observed in this period.",
                    "No se observó trabajo con etiquetas específicas en este período.",
                )
                .to_string(),
        );
    }

    items
}

fn build_known_issues_for(
    local_data: &serde_json::Value,
    distraction_events: i32,
    language: ReportLanguage,
) -> Vec<String> {
    let mut issues = Vec::new();
    if distraction_events > 0 {
        let distraction_minutes = local_data["focus_semantics"]["distraction_seconds"]
            .as_i64()
            .unwrap_or(0)
            / 60;
        issues.push(report_format!(language,
            "{} sustained non-work browsing events ({} minutes) were observed; inspect their timing before inferring an effect on focus blocks.", "Se observaron {} eventos de navegación sostenida ajena al trabajo ({} minutos); revisa cuándo ocurrieron antes de inferir un efecto en los bloques de concentración.",
            distraction_events, distraction_minutes
        ));
    }

    let consistency = local_data["tracking_consistency_pct"]
        .as_f64()
        .unwrap_or(100.0);
    if consistency < 60.0 {
        issues.push(report_format!(language,
            "Tracking gaps — only {:.0}% of days in the period have SQLite activity reports.", "Lagunas de seguimiento: solo el {:.0}% de los días del período tienen informes de actividad de SQLite.",
            consistency
        ));
    }

    if issues.is_empty() {
        issues.push(
            language.text("No configured friction pattern crossed its threshold in the available tracked data.", "Ningún patrón de fricción configurado superó su umbral en los datos registrados disponibles.")
                .to_string(),
        );
    }

    issues
}

fn build_potential_risks_for(
    local_data: &serde_json::Value,
    language: ReportLanguage,
) -> Vec<String> {
    let mut risks = Vec::new();
    let total_seconds = local_data["total_seconds"].as_i64().unwrap_or(0) as i32;
    let fragmentation = local_data["focus_semantics"]["fragmentation_pct"]
        .as_f64()
        .unwrap_or(0.0);
    let deep_minutes = deep_threshold_minutes(local_data);
    if fragmentation > 0.0 {
        risks.push(report_format!(language,
            "{:.0}% of focus-eligible time remained in blocks shorter than the transparent {}-minute reference.", "El {:.0}% del tiempo elegible para concentración quedó en bloques más cortos que la referencia explícita de {} minutos.",
            fragmentation, deep_minutes
        ));
    }

    let task_label_cov = local_data["focus_semantics"]["explicit_theme_coverage_pct"]
        .as_f64()
        .unwrap_or(0.0);
    let focus_eligible_seconds = local_data["focus_semantics"]["focus_eligible_seconds"]
        .as_i64()
        .unwrap_or(0);
    if task_label_cov < 100.0 && focus_eligible_seconds > 0 && total_seconds > 0 {
        risks.push(report_format!(language,
            "Explicit task labels cover {:.0}% of focus-eligible time; same-category task switches in the unlabelled portion cannot be observed.", "Las etiquetas explícitas de tareas cubren el {:.0}% del tiempo elegible para concentración; no se pueden observar cambios de tarea dentro de la misma categoría en la parte sin etiquetas.",
            task_label_cov
        ));
    }

    if let Some(prior) = local_data.get("prior_period") {
        let change = prior["change_pct"].as_f64().unwrap_or(0.0);
        if change < -15.0 {
            risks.push(report_format!(language,
                "Tracked hours fell {:.0}% vs prior period ({} to {}).", "Las horas registradas disminuyeron un {:.0}% respecto al período anterior ({} a {}).",
                change.abs(),
                prior["period_start"].as_str().unwrap_or("?"),
                prior["period_end"].as_str().unwrap_or("?")
            ));
        }
    }

    if let Some(days) = local_data["daily_totals"].as_array() {
        let active_days = days
            .iter()
            .filter(|d| d["total_seconds"].as_i64().unwrap_or(0) > 0)
            .count();
        let period_days = local_data["period_days"].as_i64().unwrap_or(7) as usize;
        if active_days <= 2 && period_days >= 5 {
            risks.push(report_format!(language,
                "Sparse tracking — only {} of {} days have SQLite reports; workload may be under-represented.", "Seguimiento escaso: solo {} de {} días tienen informes de SQLite; la carga de trabajo puede estar infrarrepresentada.",
                active_days, period_days
            ));
        }
    }

    let switches = local_data["focus_semantics"]["explicit_theme_switches_per_labelled_focus_hour"]
        .as_f64()
        .unwrap_or(0.0);
    if switches > 0.0 {
        risks.push(report_format!(language,
            "Observed explicit theme changes were {:.1} per labelled focus hour; inspect their break reasons before drawing a causal conclusion.", "Los cambios explícitos de tema observados fueron {:.1} por hora de concentración con etiquetas; revisa los motivos registrados de esas interrupciones antes de llegar a una conclusión causal.",
            switches
        ));
    }

    if risks.is_empty() {
        risks.push(
            language.text("No configured risk rule crossed its threshold; compare another similarly tracked period before changing workflow.", "Ninguna regla de riesgo configurada superó su umbral; compara otro período con un seguimiento similar antes de cambiar tu forma de trabajar.")
                .to_string(),
        );
    }

    risks
}

fn build_work_progress_for(
    local_data: &serde_json::Value,
    language: ReportLanguage,
) -> Vec<String> {
    let mut progress = Vec::new();

    if let Some(days) = local_data["day_category_breakdown"].as_array() {
        for index in temporal_coverage_indices(days.len(), 7) {
            let d = &days[index];
            progress.push(report_format!(
                language,
                "{} — {:.1}h total, largest category {} ({:.1}h)",
                "{} — {:.1}h en total, categoría principal {} ({:.1}h)",
                d["date"].as_str().unwrap_or(""),
                d["total_hours"].as_f64().unwrap_or(0.0),
                d["top_category"]
                    .as_str()
                    .unwrap_or(language.text("Work", "Trabajo")),
                d["top_hours"].as_f64().unwrap_or(0.0)
            ));
        }
    } else if let Some(days) = local_data["daily_totals"].as_array() {
        for index in temporal_coverage_indices(days.len(), 5) {
            let d = &days[index];
            let date = d["date"].as_str().unwrap_or("");
            let hours = d["total_seconds"].as_i64().unwrap_or(0) as f64 / 3600.0;
            let count = d["activity_count"].as_i64().unwrap_or(0);
            if hours > 0.0 {
                progress.push(report_format!(
                    language,
                    "{} — {:.1}h across {} SQLite captures",
                    "{} — {:.1}h en {} capturas de SQLite",
                    date,
                    hours,
                    count
                ));
            }
        }
    }

    if let Some(themes) = local_data["work_themes"].as_array() {
        for t in themes.iter().take(3) {
            let label = t["label"].as_str().unwrap_or("");
            let h = t["total_seconds"].as_i64().unwrap_or(0) as f64 / 3600.0;
            if !label.is_empty() && h > 0.0 {
                progress.push(report_format!(
                    language,
                    "Theme: {} — {:.1}h in period",
                    "Tema: {} — {:.1}h en el período",
                    label,
                    h
                ));
            }
        }
    }

    if progress.is_empty() {
        progress.push(
            language
                .text(
                    "No daily progress recorded for this period.",
                    "No hay progreso diario registrado para este período.",
                )
                .to_string(),
        );
    }

    progress
}

fn build_lessons_learned_for(
    local_data: &serde_json::Value,
    language: ReportLanguage,
) -> Vec<serde_json::Value> {
    let mut lessons = Vec::new();

    if local_data["total_seconds"].as_i64().unwrap_or(0) <= 0 {
        return lessons;
    }

    let eligible_seconds = local_data["focus_eligible_seconds"].as_i64().unwrap_or(0);
    if eligible_seconds > 0 {
        let fragmentation = local_data["focus_semantics"]["fragmentation_pct"]
            .as_f64()
            .unwrap_or(0.0);
        let deep_minutes = deep_threshold_minutes(local_data);
        if fragmentation > 0.0 {
            lessons.push(serde_json::json!({
                "title": language.text("Inspect fragmentation", "Revisa la fragmentación"),
                "body": report_format!(language,
                    "{:.0}% of focus-eligible time remained in shorter fragments. Review the recorded transition that ended the largest fragments before changing the schedule; {} minutes is a product reference, not a biological rule.", "El {:.0}% del tiempo elegible para concentración quedó en fragmentos más cortos. Revisa la transición registrada que terminó los fragmentos más largos antes de cambiar el horario; {} minutos es una referencia del producto, no una regla biológica.",
                    fragmentation, deep_minutes
                ),
            }));
        } else {
            lessons.push(serde_json::json!({
                "title": language.text("Sustained-work pattern", "Patrón de trabajo sostenido"),
                "body": report_format!(language,
                    "No focus-eligible time fell below the configured {}-minute reference in this tracked period. This describes observed continuity, not subjective flow or productivity.", "Ningún tiempo elegible para concentración quedó por debajo de la referencia configurada de {} minutos en este período registrado. Esto describe la continuidad observada, no el estado de flow subjetivo ni la productividad.",
                    deep_minutes
                ),
            }));
        }
    }

    if let Some(top) = local_data["category_breakdown"]
        .as_array()
        .and_then(|a| a.first())
    {
        let category_seconds = top["total_seconds"].as_i64().unwrap_or(0);
        if category_seconds > 0 {
            let cat = top["category"]
                .as_str()
                .unwrap_or(language.text("Work", "Trabajo"));
            let total_seconds = local_data["total_seconds"].as_i64().unwrap_or(0);
            lessons.push(serde_json::json!({
                "title": report_format!(language,"Review the role of {}", "Revisa el papel de {}", cat),
                "body": report_format!(language,
                    "{} accounted for {:.1}h ({:.0}% of tracked time). Check whether that mix matches your intended work; the category is descriptive, not a productivity score.", "{} representó {:.1}h ({:.0}% del tiempo registrado). Comprueba si esa distribución coincide con el trabajo que querías hacer; la categoría es descriptiva, no una puntuación de productividad.",
                    cat,
                    category_seconds as f64 / 3600.0,
                    category_seconds as f64 / total_seconds as f64 * 100.0
                ),
            }));
        }
    }

    if let Some(consistency) = local_data["tracking_consistency_pct"].as_f64() {
        if consistency < 100.0 {
            lessons.push(serde_json::json!({
                "title": language.text("Coverage limits the conclusion", "La cobertura limita la conclusión"),
                "body": report_format!(language,
                    "Activity was observed on {:.0}% of days in the selected period. Treat untracked days as missing data, not as days without work.", "Se observó actividad en el {:.0}% de los días del período seleccionado. Trata los días sin seguimiento como datos ausentes, no como días sin trabajo.",
                    consistency
                ),
            }));
        }
    }

    if lessons.is_empty() {
        let hours = local_data["total_seconds"].as_i64().unwrap_or(0) as f64 / 3600.0;
        lessons.push(serde_json::json!({
            "title": language.text("Recorded time is a starting point", "El tiempo registrado es un punto de partida"),
            "body": report_format!(language,
                "{:.1}h was recorded, but category and focus signals are too limited for a specific workflow conclusion. Add task context or compare another period before changing plans.", "Se registraron {:.1}h, pero las señales de categoría y concentración son demasiado limitadas para una conclusión concreta sobre tu forma de trabajar. Añade contexto de tareas o compara otro período antes de cambiar tus planes.",
                hours
            ),
        }));
    }

    lessons
}

fn default_recommendations_for(
    local_data: &serde_json::Value,
    language: ReportLanguage,
) -> Vec<String> {
    let mut recs = Vec::new();
    let focus = &local_data["focus_semantics"];
    let distraction_events = focus["distraction_events"].as_i64().unwrap_or(0);
    if distraction_events > 0 {
        let minutes = focus["distraction_seconds"].as_i64().unwrap_or(0) / 60;
        recs.push(report_format!(language,
            "Review the timing of the {} sustained non-work browsing event(s) ({} minutes) separately from valuable coordination and operational work.", "Revisa cuándo ocurrieron los {} eventos de navegación sostenida ajena al trabajo ({} minutos) por separado del trabajo valioso de coordinación y operaciones.",
            distraction_events, minutes
        ));
    }

    let label_coverage = focus["explicit_theme_coverage_pct"].as_f64().unwrap_or(0.0);
    if focus["focus_eligible_seconds"].as_i64().unwrap_or(0) > 0 && label_coverage < 100.0 {
        recs.push(report_format!(language,
            "Explicit task labels cover {:.0}% of focus-eligible time. Use a short manual label when theme continuity matters; no issue tracker is required.", "Las etiquetas explícitas de tareas cubren el {:.0}% del tiempo elegible para concentración. Usa una etiqueta manual breve cuando importe la continuidad del tema; no hace falta un gestor de incidencias.",
            label_coverage
        ));
    }

    let fragmentation = focus["fragmentation_pct"].as_f64().unwrap_or(0.0);
    if fragmentation > 0.0 {
        recs.push(report_format!(language,
            "Inspect the recorded break reasons behind the {:.0}% of focus-eligible time in short fragments before changing your schedule.", "Revisa los motivos de interrupción registrados que explican el {:.0}% del tiempo elegible para concentración en fragmentos cortos antes de cambiar tu horario.",
            fragmentation
        ));
    }

    if recs.is_empty() {
        recs.push(
            language.text("The current signal does not justify a specific workflow change; keep tracking to compare future periods.", "La señal actual no justifica un cambio concreto en tu forma de trabajar; continúa el seguimiento para comparar períodos futuros.")
                .to_string(),
        );
    }

    recs
}

fn round_hours(seconds: i32) -> f64 {
    ((seconds as f64 / 3600.0) * 10.0).round() / 10.0
}

fn query_clipped_total_and_count(
    conn: &Connection,
    period_start: &str,
    period_end: &str,
) -> Result<(i32, i32), String> {
    let window = crate::focus_semantics::LocalDateWindow::parse(period_start, period_end)?;
    let mut stmt = conn
        .prepare(
            "SELECT datetime(created_at, 'localtime'), duration_seconds
             FROM reports
             WHERE date(created_at, 'localtime') >= ?1
               AND date(created_at, 'localtime') <= date(?2, '+1 day')",
        )
        .map_err(|error| error.to_string())?;
    let rows = stmt
        .query_map(params![period_start, period_end], |row| {
            Ok((
                row.get::<_, String>(0).unwrap_or_default(),
                row.get::<_, i64>(1).unwrap_or(0).max(0),
            ))
        })
        .map_err(|error| error.to_string())?;
    let mut total_seconds = 0i32;
    let mut activity_count = 0i32;
    for (timestamp, duration) in rows.filter_map(Result::ok) {
        let slices = window.slices_for_observation(&timestamp, duration);
        if !slices.is_empty() || (duration == 0 && window.contains_local_timestamp(&timestamp)) {
            activity_count += 1;
        }
        for slice in slices {
            total_seconds = total_seconds
                .saturating_add(i32::try_from(slice.duration_seconds).unwrap_or(i32::MAX));
        }
    }
    Ok((total_seconds, activity_count))
}

fn query_prior_period_metrics(
    conn: &Connection,
    period_start: chrono::NaiveDate,
    days: i32,
    current_total_seconds: i32,
) -> Result<serde_json::Value, String> {
    let prior_end = period_start - chrono::Duration::days(1);
    let prior_start = prior_end - chrono::Duration::days((days - 1) as i64);
    let start_str = prior_start.format("%Y-%m-%d").to_string();
    let end_str = prior_end.format("%Y-%m-%d").to_string();

    let (total_seconds, activity_count) =
        query_clipped_total_and_count(conn, &start_str, &end_str)?;
    let prior_focus = crate::focus_semantics::summarize_from_db(conn, &start_str, &end_str)?;
    let deep_focus_seconds = prior_focus.deep_focus_seconds as i32;

    let change_pct = if current_total_seconds > 0 && total_seconds > 0 {
        ((current_total_seconds - total_seconds) as f64 / total_seconds as f64 * 1000.0).round()
            / 10.0
    } else if current_total_seconds > 0 && total_seconds == 0 {
        100.0
    } else {
        0.0
    };

    Ok(serde_json::json!({
        "period_start": start_str,
        "period_end": end_str,
        "total_seconds": total_seconds,
        "total_hours": round_hours(total_seconds),
        "deep_focus_seconds": deep_focus_seconds,
        "deep_focus_hours": round_hours(deep_focus_seconds),
        "activity_count": activity_count,
        "change_pct": change_pct,
    }))
}

fn build_diverse_activity_samples(
    all: &[ActivitySample],
    longest: &[ActivityCandidateRow],
) -> Vec<ActivitySample> {
    let mut picked: Vec<ActivitySample> = Vec::new();
    let mut seen_dates: HashMap<String, bool> = HashMap::new();
    let mut seen_keys: HashMap<String, bool> = HashMap::new();

    // The first ten samples feed the smallest section prompt. Reserve those
    // slots for representative dates across the entire period, not whichever
    // captures happened to be longest or newest.
    let mut by_date: BTreeMap<&str, &ActivitySample> = BTreeMap::new();
    for sample in all {
        let rank = |item: &ActivitySample| {
            (
                item.duration_seconds,
                item.ticket.is_some(),
                !item.description.trim().is_empty(),
            )
        };
        let entry = by_date.entry(&sample.date).or_insert(sample);
        if rank(sample) > rank(entry) {
            *entry = sample;
        }
    }
    let representative_days = by_date.into_values().collect::<Vec<_>>();
    for index in temporal_coverage_indices(representative_days.len(), 10) {
        try_push_activity_sample(
            representative_days[index],
            &mut picked,
            &mut seen_dates,
            &mut seen_keys,
        );
    }

    for s in longest.iter().take(8) {
        try_push_activity_sample(
            &ActivitySample {
                date: s.date.clone(),
                category: s.category.clone(),
                description: s.description.clone(),
                duration_seconds: s.duration_seconds,
                ticket: s.ticket.clone(),
            },
            &mut picked,
            &mut seen_dates,
            &mut seen_keys,
        );
    }

    for s in all.iter().rev().take(80) {
        if picked.len() >= 40 {
            break;
        }
        if !seen_dates.contains_key(&s.date) || s.ticket.is_some() {
            try_push_activity_sample(s, &mut picked, &mut seen_dates, &mut seen_keys);
        }
    }

    for s in all.iter().rev() {
        if picked.len() >= 50 {
            break;
        }
        try_push_activity_sample(s, &mut picked, &mut seen_dates, &mut seen_keys);
    }

    picked
}

/// Prioritize the newest, middle, and earliest observations; then repeatedly
/// fill the largest uncovered temporal gap. This keeps a small prompt useful
/// for 1–30-day reports without implying that unselected days had no activity.
fn temporal_coverage_indices(len: usize, limit: usize) -> Vec<usize> {
    let target = len.min(limit);
    if target == 0 {
        return Vec::new();
    }
    let mut selected = vec![len - 1];
    if target >= 3 {
        selected.push((len - 1) / 2);
    }
    if target >= 2 {
        selected.push(0);
    }
    while selected.len() < target {
        let next = (0..len)
            .filter(|index| !selected.contains(index))
            .max_by_key(|index| {
                let nearest = selected
                    .iter()
                    .map(|chosen| index.abs_diff(*chosen))
                    .min()
                    .unwrap_or(0);
                (nearest, std::cmp::Reverse(*index))
            });
        if let Some(index) = next {
            selected.push(index);
        } else {
            break;
        }
    }
    selected
}

fn try_push_activity_sample(
    sample: &ActivitySample,
    picked: &mut Vec<ActivitySample>,
    seen_dates: &mut HashMap<String, bool>,
    seen_keys: &mut HashMap<String, bool>,
) {
    let key = format!(
        "{}|{}|{}",
        sample.date,
        sample.category,
        clamp_line(&sample.description, 40)
    );
    if seen_keys.contains_key(&key) {
        return;
    }
    seen_keys.insert(key, true);
    seen_dates.insert(sample.date.clone(), true);
    picked.push(ActivitySample {
        date: sample.date.clone(),
        category: sample.category.clone(),
        description: sample.description.clone(),
        duration_seconds: sample.duration_seconds,
        ticket: sample.ticket.clone(),
    });
}

fn clamp_line(s: &str, max_chars: usize) -> String {
    let count = s.chars().count();
    if count <= max_chars {
        return s.to_string();
    }
    s.chars().take(max_chars).collect::<String>() + "…"
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;

    fn bilingual_report_fixture() -> serde_json::Value {
        let mut days = Vec::new();
        for day in 1..=30 {
            days.push(serde_json::json!({
                "date": format!("2026-09-{day:02}"),
                "total_hours": day as f64 / 10.0,
                "top_category": "Research María 数据",
                "top_hours": day as f64 / 20.0,
            }));
        }
        serde_json::json!({
            "period_start": "2026-09-01", "period_end": "2026-09-30", "period_days": 30,
            "total_seconds": 37800, "total_hours": 10.5, "activity_count": 81,
            "active_days": 2, "tracking_consistency_pct": 6.7, "task_label_coverage_pct": 31.2,
            "deep_focus_hours": 3.5, "deep_focus_sessions": 4, "focus_eligible_seconds": 25200,
            "distraction_events": 3, "distraction_hours": 0.2,
            "category_breakdown": [
                {"category": "Analysis", "total_seconds": 18000, "count": 21},
                {"category": "Research María 数据", "total_seconds": 7200, "count": 8},
                {"category": "Meeting", "total_seconds": 1800, "count": 5},
                {"category": "Sales", "total_seconds": 1800, "count": 7},
                {"category": "Browsing", "total_seconds": 7200, "count": 9},
                {"category": "Idle", "total_seconds": 1800, "count": 4}
            ],
            "ticket_breakdown": [
                {"ticket": "Sprint María 数据 7", "total_seconds": 3600, "count": 12},
                {"ticket": "registradas", "total_seconds": 7200, "count": 8},
                {"ticket": "reports", "total_seconds": 1800, "count": 4}
            ],
            "work_themes": [{"label": "Draft María 数据 7", "total_seconds": 9000}],
            "day_category_breakdown": days,
            "daily_totals": [{"date": "2026-09-01", "total_seconds": 37800, "activity_count": 81}],
            "prior_period": {"change_pct": -21.2, "period_start": "2026-08-02", "period_end": "2026-08-31"},
            "focus_semantics": {
                "deep_threshold_seconds": 1500, "fragmentation_pct": 42.6,
                "focus_eligible_seconds": 25200, "explicit_theme_coverage_pct": 46.2,
                "explicit_theme_switches_per_labelled_focus_hour": 1.6,
                "distraction_events": 3, "distraction_seconds": 720
            }
        })
    }

    fn numeric_copy_tokens(text: &str) -> Vec<String> {
        text.split(|character: char| !character.is_ascii_digit() && character != '.')
            .filter(|part| part.chars().any(|character| character.is_ascii_digit()))
            .map(|part| part.trim_matches('.').to_string())
            .collect()
    }

    fn assert_same_report_evidence(en: &serde_json::Value, es: &serde_json::Value) {
        match (en, es) {
            (serde_json::Value::String(en), serde_json::Value::String(es)) => {
                assert_eq!(
                    numeric_copy_tokens(en),
                    numeric_copy_tokens(es),
                    "{en} / {es}"
                );
            }
            (serde_json::Value::Array(en), serde_json::Value::Array(es)) => {
                assert_eq!(en.len(), es.len());
                for (en, es) in en.iter().zip(es) {
                    assert_same_report_evidence(en, es);
                }
            }
            (serde_json::Value::Object(en), serde_json::Value::Object(es)) => {
                assert_eq!(en.keys().collect::<Vec<_>>(), es.keys().collect::<Vec<_>>());
                for (key, en) in en {
                    assert_same_report_evidence(en, &es[key]);
                }
            }
            _ => assert_eq!(en, es),
        }
    }

    #[test]
    fn bilingual_report_preserves_numbers_dates_category_roles_and_timeline_coverage() {
        let data = bilingual_report_fixture();
        let en = build_rule_based_report(&data);
        let es = build_rule_based_report_for(&data, ReportLanguage::Spanish);
        assert_same_report_evidence(&en, &es);
        assert_ne!(en["executive_overview"], es["executive_overview"]);
        assert_eq!(es["overall_health"], "Se observaron bloques sostenidos");
        assert_eq!(
            es["health_breakdown"][0]["status"],
            "Elegible para trabajo sostenido"
        );
        assert_eq!(es["health_breakdown"][1]["status"], "Sin clasificar");
        assert_eq!(es["health_breakdown"][2]["status"], "Trabajo de contexto");
        assert_eq!(es["health_breakdown"][3]["status"], "Trabajo de contexto");
        assert_eq!(es["health_breakdown"][4]["status"], "Revisar");
        assert_eq!(es["health_breakdown"][5]["status"], "Incierto");
        for (en, es) in en["health_breakdown"]
            .as_array()
            .unwrap()
            .iter()
            .zip(es["health_breakdown"].as_array().unwrap())
        {
            assert_eq!(en["element"], es["element"]);
        }
        let progress = es["work_progress"].as_array().unwrap();
        assert!(progress
            .first()
            .unwrap()
            .as_str()
            .unwrap()
            .contains("2026-09-30"));
        assert!(progress
            .iter()
            .any(|item| item.as_str().unwrap().contains("2026-09-01")));
        assert!(progress
            .iter()
            .any(|item| item.as_str().unwrap().contains("Draft María 数据 7")));
        assert_eq!(es["observed_work"].as_array().unwrap().len(), 2);
        assert!(es["observed_work"][1]
            .as_str()
            .unwrap()
            .contains("registradas"));
    }

    #[test]
    fn bilingual_report_maps_selected_subsets_in_model_order_without_reranking() {
        let data = bilingual_report_fixture();
        let en = build_rule_based_report(&data);
        let es = build_rule_based_report_for(&data, ReportLanguage::Spanish);
        let mut indices = serde_json::Map::new();
        for (key, value) in en.as_object().unwrap() {
            if let Some(items) = value.as_array().filter(|items| items.len() > 1) {
                indices.insert(key.clone(), serde_json::json!([items.len() - 1, 0]));
            }
        }
        let selected =
            apply_grounded_selection(&en, &serde_json::json!({"selected_indices": indices}))
                .unwrap();
        let payload = build_localized_report(&selected, &data);
        assert_eq!(payload["en"], selected);
        for (key, choices) in &indices {
            let choices = choices.as_array().unwrap();
            for (position, index) in choices.iter().enumerate() {
                assert_eq!(
                    payload["es"][key][position],
                    es[key][index.as_u64().unwrap() as usize]
                );
            }
        }
        assert_same_report_evidence(&payload["en"], &payload["es"]);
    }

    #[test]
    fn bilingual_report_localizes_pipeline_aliases_and_preserves_metadata() {
        let data = bilingual_report_fixture();
        let mut selected = build_rule_based_report(&data);
        selected["project_name"] = selected["work_summary"].clone();
        selected["work_summary"] = selected["executive_overview"].clone();
        selected["timeline_caption"] = selected["work_progress"][0].clone();
        selected["report_meta"] = build_report_meta(&data);
        selected["focus_target"] = selected["report_meta"]["focus_target"].clone();
        let payload = build_localized_report(&selected, &data);
        let expected = build_rule_based_report_for(&data, ReportLanguage::Spanish);
        assert_eq!(
            payload["es"]["work_summary"],
            expected["executive_overview"]
        );
        assert_eq!(payload["es"]["project_name"], expected["work_summary"]);
        assert_eq!(
            payload["es"]["timeline_caption"],
            expected["work_progress"][0]
        );
        assert_eq!(
            payload["es"]["report_meta"]["period_name"],
            "Trabajo · Analysis"
        );
        for key in [
            "focus_target",
            "tracked_hours",
            "deep_focus_hours",
            "activity_count",
            "period_label",
        ] {
            assert_eq!(
                payload["en"]["report_meta"][key],
                payload["es"]["report_meta"][key]
            );
        }
        assert_same_report_evidence(&payload["en"], &payload["es"]);
    }

    #[test]
    fn bilingual_report_preserves_user_content_unicode_whitespace_and_template_words() {
        let description = format!("María  数据: Review General work.\n{}", "X".repeat(120));
        let mut data = bilingual_report_fixture();
        data["category_breakdown"][0]["category"] = serde_json::json!("General work");
        data["ticket_breakdown"] = serde_json::json!([]);
        data["sample_activities"] = serde_json::json!([
            {"category": "Self  数据", "description": description}
        ]);
        let mut report = build_rule_based_report(&data);
        repair_learning_fields(&mut report, &data);
        report["report_meta"] = build_report_meta(&data);
        report["focus_target"] = serde_json::json!("General work");
        let payload = build_localized_report(&report, &data);
        for language in ["en", "es"] {
            assert_eq!(payload[language]["focus_target"], "General work");
            assert_eq!(
                payload[language]["report_meta"]["focus_target"],
                "General work"
            );
            assert_eq!(
                payload[language]["health_breakdown"][0]["element"],
                "General work"
            );
            assert_eq!(
                payload[language]["observed_work"][0],
                format!("Self  数据 — {description}")
            );
            assert!(payload[language]["lessons_learned"][1]["body"]
                .as_str()
                .unwrap()
                .starts_with("General work"));
        }
        // A field containing arbitrary user text is not matched against a
        // different field's template, even if the text is identical.
        assert_eq!(
            matching_report_copy(
                &serde_json::json!({"user_note": "Self"}),
                &serde_json::json!({"status": "Self"}),
                &serde_json::json!({"status": "Yo"}),
            ),
            serde_json::json!({"user_note": "Self"})
        );
    }

    #[test]
    fn bilingual_report_learning_repair_maps_only_verified_lessons_and_recommendations() {
        let data = bilingual_report_fixture();
        let mut selected = serde_json::json!({
            "lessons_learned": [{"title": "Invented", "body": "Finished all work"}],
            "recommendations": ["Invented advice"],
        });
        repair_learning_fields(&mut selected, &data);
        let payload = build_localized_report(&selected, &data);
        assert_eq!(
            payload["en"]["lessons_learned"],
            serde_json::json!(build_lessons_learned(&data))
        );
        assert_eq!(
            payload["es"]["lessons_learned"],
            serde_json::json!(build_lessons_learned_for(&data, ReportLanguage::Spanish))
        );
        assert_eq!(
            payload["es"]["recommendations"],
            serde_json::json!(default_recommendations_for(&data, ReportLanguage::Spanish))
        );
        assert!(!payload.to_string().contains("Invented"));
    }

    #[test]
    fn bilingual_empty_report_keeps_missing_data_distinct_from_productivity() {
        let data = serde_json::json!({
            "period_start": "2026-09-01", "period_end": "2026-09-30",
            "total_seconds": 0, "total_hours": 0.0,
        });
        let mut selected = build_rule_based_report(&data);
        repair_learning_fields(&mut selected, &data);
        let payload = build_localized_report(&selected, &data);
        assert_eq!(
            payload["es"]["overall_health"],
            "Sin señal de trabajo sostenido"
        );
        assert!(payload["es"]["executive_overview"]
            .as_str()
            .unwrap()
            .contains("no se registró actividad"));
        assert!(payload["es"]["lessons_learned"]
            .as_array()
            .unwrap()
            .is_empty());
        assert_eq!(
            payload["es"]["health_breakdown"][0]["element"],
            "Seguimiento"
        );
        assert_eq!(
            payload["es"]["recommendations"].as_array().unwrap().len(),
            1
        );
        assert_same_report_evidence(&payload["en"], &payload["es"]);
    }

    #[test]
    fn month_report_context_keeps_full_timeline_and_spread_samples() {
        let start = chrono::NaiveDate::from_ymd_opt(2026, 8, 1).unwrap();
        let mut daily = Vec::new();
        let mut categories = Vec::new();
        let mut activities = Vec::new();
        for offset in 0..30 {
            let date = (start + chrono::Duration::days(offset))
                .format("%Y-%m-%d")
                .to_string();
            daily.push(serde_json::json!({
                "date": date, "total_seconds": 3600 + offset * 60, "activity_count": 3
            }));
            categories.push(serde_json::json!({
                "date": date, "top_category": "Analysis", "top_hours": 1.0, "total_hours": 1.0
            }));
            activities.push(ActivitySample {
                date,
                category: "Analysis".to_string(),
                description: format!("verified-day-{offset:02}"),
                duration_seconds: 60 + offset as i32,
                ticket: None,
            });
        }
        let longest = activities
            .iter()
            .rev()
            .take(8)
            .map(|sample| ActivityCandidateRow {
                date: sample.date.clone(),
                category: sample.category.clone(),
                description: sample.description.clone(),
                duration_seconds: sample.duration_seconds,
                ticket: sample.ticket.clone(),
            })
            .collect::<Vec<_>>();
        let samples = build_diverse_activity_samples(&activities, &longest);
        let sample_dates = samples
            .iter()
            .take(10)
            .map(|s| s.date.as_str())
            .collect::<Vec<_>>();
        assert!(sample_dates.contains(&"2026-08-01"));
        assert!(sample_dates.contains(&"2026-08-15"));
        assert!(sample_dates.contains(&"2026-08-30"));

        let local_data = serde_json::json!({
            "period_start": "2026-08-01", "period_end": "2026-08-30", "period_days": 30,
            "total_hours": 30.0, "deep_focus_hours": 0.0, "activity_count": 90,
            "distraction_events": 0, "distraction_hours": 0.0,
            "daily_totals": daily, "day_category_breakdown": categories,
            "sample_activities": samples,
            "focus_semantics": {"deep_threshold_seconds": 1500, "sessions": [], "hourly_deep_focus": []}
        });
        let profile = format!("USER_PROFILE: {}", "work goals ".repeat(100));
        let timeline = build_section_stats_snapshot("timeline_insights", &local_data, &profile);
        let progress = build_section_stats_snapshot("progress_tasks", &local_data, &profile);
        for snapshot in [&timeline, &progress] {
            assert!(snapshot.chars().count() <= LLM_SECTION_STATS_MAX_CHARS);
            assert!(snapshot.contains("2026-08-01"));
            assert!(snapshot.contains("2026-08-15"));
            assert!(snapshot.contains("2026-08-30"));
            assert!(snapshot.contains("DAILY_TOTALS (observed days):"));
        }
        for offset in 0..30 {
            let date = (start + chrono::Duration::days(offset))
                .format("%Y-%m-%d")
                .to_string();
            assert!(timeline.contains(&date), "timeline omitted {date}");
        }
        assert!(progress.contains("verified-day-00"));
        assert!(progress.contains("verified-day-14"));
        assert!(progress.contains("verified-day-29"));

        let visible_progress = build_work_progress(&local_data);
        assert!(visible_progress
            .iter()
            .any(|item| item.contains("2026-08-01")));
        assert!(visible_progress
            .iter()
            .any(|item| item.contains("2026-08-15")));
        assert!(visible_progress
            .iter()
            .any(|item| item.contains("2026-08-30")));
    }

    #[test]
    fn session_evidence_uses_actual_longest_blocks() {
        let data = serde_json::json!({"focus_semantics": {"sessions": [
            {"start":"short", "focus_seconds":300, "tier":"fragment"},
            {"start":"long", "focus_seconds":3600, "tier":"deep"},
            {"start":"middle", "focus_seconds":1800, "tier":"deep"}
        ]}});
        let mut lines = Vec::new();
        append_longest_sessions(&mut lines, &data, 2);
        assert_eq!(lines.len(), 2);
        assert!(lines[0].contains("long"));
        assert!(lines[1].contains("middle"));
    }

    #[test]
    fn stats_budget_never_cuts_a_verified_line() {
        let packed = pack_stats_lines(
            vec![
                "PERIOD: Aug".into(),
                "very long evidence line".into(),
                "TAIL".into(),
            ],
            16,
        );
        assert_eq!(packed, "PERIOD: Aug\nTAIL");
    }

    fn utc_storage_timestamp(local: chrono::NaiveDateTime) -> String {
        Local
            .from_local_datetime(&local)
            .earliest()
            .expect("test local timestamp exists")
            .with_timezone(&chrono::Utc)
            .format("%Y-%m-%d %H:%M:%S")
            .to_string()
    }

    #[test]
    fn report_payload_uses_only_canonical_focus_sessions() {
        let dir = tempfile::tempdir().unwrap();
        let db_path = dir.path().join("insights.sqlite");
        let conn = Connection::open(&db_path).unwrap();
        conn.execute_batch(
            "CREATE TABLE reports (
                id INTEGER PRIMARY KEY,
                created_at TEXT NOT NULL,
                activity_type TEXT NOT NULL,
                description TEXT NOT NULL,
                jira_ticket_id TEXT,
                duration_seconds INTEGER NOT NULL,
                synced INTEGER DEFAULT 0,
                active_app TEXT,
                window_title TEXT,
                capture_source TEXT,
                theme_hint TEXT
            );",
        )
        .unwrap();
        let today = Local::now().format("%Y-%m-%d").to_string();
        for (time, category, duration, theme) in [
            ("12:15:00", "Writing", 900, "  Policy   brief "),
            ("12:30:00", "Research", 900, "policy brief"),
            ("12:35:00", "Meeting", 300, "Policy brief"),
        ] {
            conn.execute(
                "INSERT INTO reports (
                    created_at, activity_type, description, duration_seconds,
                    synced, active_app, window_title, capture_source, theme_hint
                 ) VALUES (?1, ?2, ?3, ?4, 0, 'Fixture', 'Fixture window', 'test', ?5)",
                params![
                    format!("{today} {time}"),
                    category,
                    format!("{category} fixture"),
                    duration,
                    theme
                ],
            )
            .unwrap();
        }
        drop(conn);

        let report = build_local_insights_report(&db_path, 1).unwrap();
        assert!(report.get("deep_focus_sessions_30m_plus").is_none());
        assert!(report.get("focus_ratio_pct").is_none());
        assert!(report.get("focus_seconds").is_none());
        assert!(report.get("focus_hours").is_none());
        assert!(report.get("focus_sessions").is_none());
        assert!(report.get("hourly_focus").is_none());
        assert!(report.get("context_switches").is_none());
        assert!(report.get("deep_focus_share_pct").is_none());
        assert_eq!(report["deep_focus_seconds"], 1800);
        assert_eq!(report["deep_focus_hours"], 0.5);
        assert_eq!(report["focus_semantics"]["deep_focus_seconds"], 1800);
        assert_eq!(report["focus_semantics"]["deep_focus_sessions"], 1);
        assert_eq!(report["focus_semantics"]["context_work_seconds"], 300);
        assert_eq!(
            report["focus_semantics"]["sessions"]
                .as_array()
                .unwrap()
                .len(),
            1
        );
        assert_eq!(report["work_themes"].as_array().unwrap().len(), 1);
        assert_eq!(report["work_themes"][0]["label"], "Task Policy brief");
    }

    #[test]
    fn report_totals_and_focus_use_the_same_midnight_clipping() {
        let dir = tempfile::tempdir().unwrap();
        let db_path = dir.path().join("midnight-insights.sqlite");
        let conn = Connection::open(&db_path).unwrap();
        conn.execute_batch(
            "CREATE TABLE reports (
                id INTEGER PRIMARY KEY,
                created_at TEXT NOT NULL,
                activity_type TEXT NOT NULL,
                description TEXT NOT NULL,
                jira_ticket_id TEXT,
                duration_seconds INTEGER NOT NULL,
                synced INTEGER DEFAULT 0,
                active_app TEXT,
                window_title TEXT,
                capture_source TEXT,
                theme_hint TEXT
            );",
        )
        .unwrap();
        let today = Local::now().date_naive();
        let previous = today.pred_opt().unwrap();
        let observed_end = today.and_hms_opt(0, 1, 0).unwrap();
        conn.execute(
            "INSERT INTO reports (
                created_at, activity_type, description, duration_seconds,
                synced, active_app, window_title, capture_source, theme_hint
             ) VALUES (?1, 'Writing', 'cross-midnight draft', 120, 0,
                       'Writer', 'Draft', 'test', 'Report')",
            params![utc_storage_timestamp(observed_end)],
        )
        .unwrap();
        drop(conn);

        let report = build_local_insights_report(&db_path, 2).unwrap();
        assert_eq!(report["total_seconds"], 120);
        assert_eq!(report["activity_count"], 1);
        assert_eq!(report["active_days"], 2);
        assert_eq!(report["focus_semantics"]["focus_eligible_seconds"], 120);
        assert_eq!(report["category_breakdown"][0]["count"], 1);

        let daily = report["daily_totals"].as_array().unwrap();
        let seconds_for = |date: &str| {
            daily
                .iter()
                .find(|row| row["date"] == date)
                .and_then(|row| row["total_seconds"].as_i64())
        };
        assert_eq!(
            seconds_for(&previous.format("%Y-%m-%d").to_string()),
            Some(60)
        );
        assert_eq!(seconds_for(&today.format("%Y-%m-%d").to_string()), Some(60));
    }

    #[test]
    fn app_names_are_only_added_to_the_local_status_payload_and_exclusions_are_applied() {
        let dir = tempfile::tempdir().unwrap();
        let db_path = dir.path().join("app-distractions.sqlite");
        let conn = Connection::open(&db_path).unwrap();
        conn.execute_batch(
            "CREATE TABLE config (key TEXT PRIMARY KEY, value TEXT);
             CREATE TABLE reports (
                id INTEGER PRIMARY KEY,
                created_at TEXT NOT NULL,
                activity_type TEXT NOT NULL,
                description TEXT NOT NULL,
                jira_ticket_id TEXT,
                duration_seconds INTEGER NOT NULL,
                synced INTEGER DEFAULT 0,
                active_app TEXT,
                window_title TEXT,
                capture_source TEXT,
                theme_hint TEXT
             );",
        )
        .unwrap();
        let today = Local::now().date_naive();
        for (minute, app) in [(5, "Browser.exe"), (15, "Bitwarden.exe")] {
            let observed_end = today.and_hms_opt(12, minute, 0).unwrap();
            conn.execute(
                "INSERT INTO reports (
                    created_at, activity_type, description, duration_seconds,
                    synced, active_app, capture_source
                 ) VALUES (?1, 'Browsing', 'fixture', 180, 0, ?2, 'test')",
                params![utc_storage_timestamp(observed_end), app],
            )
            .unwrap();
        }
        drop(conn);

        let excluded = vec!["Bitwarden".to_string()];
        let (aggregate, details) =
            build_local_insights_report_inner(&db_path, 1, Some(&excluded)).unwrap();
        assert!(aggregate.get("distraction_app_analysis").is_none());
        let details = serde_json::to_value(details.unwrap().unwrap()).unwrap();
        assert_eq!(details["qualifying_episodes"], 1);
        assert_eq!(details["qualifying_seconds"], 180);
        assert_eq!(details["attributed_seconds"], 180);
        assert_eq!(details["unattributed_seconds"], 0);
        assert_eq!(details["apps"][0]["app_name"], "Browser.exe");
        assert!(!details.to_string().contains("Bitwarden"));
    }

    #[test]
    fn report_copy_uses_canonical_distraction_events_not_raw_browsing_rows() {
        let mut lines = Vec::new();
        let below_threshold = serde_json::json!({
            "category_breakdown": [{"category": "Browsing", "total_seconds": 90, "count": 3}],
            "focus_semantics": {"distraction_events": 0, "distraction_seconds": 0},
        });
        append_distraction_detail(&mut lines, &below_threshold);
        assert!(lines.is_empty());

        let canonical_event = serde_json::json!({
            "category_breakdown": [{"category": "Browsing", "total_seconds": 210, "count": 7}],
            "focus_semantics": {"distraction_events": 1, "distraction_seconds": 120},
        });
        append_distraction_detail(&mut lines, &canonical_event);
        assert_eq!(lines.len(), 1);
        assert!(lines[0].contains("2m across 1 canonical events"));
        assert!(!lines[0].contains("7"));
    }

    #[test]
    fn recommendations_are_role_neutral_and_do_not_require_tickets() {
        let local_data = serde_json::json!({
            "total_hours": 3.0,
            "total_seconds": 10800,
            "task_label_coverage_pct": 20.0,
            "tracking_consistency_pct": 100.0,
            "focus_semantics": {
                "distraction_events": 0,
                "distraction_seconds": 0,
                "focus_eligible_seconds": 7200,
                "explicit_theme_coverage_pct": 20.0,
                "fragmentation_pct": 25.0
            },
        });
        let issues = build_known_issues(&local_data, 0);
        assert!(!issues.iter().any(|item| item.contains("no ticket")));

        let risks = build_potential_risks(&local_data);
        assert!(risks
            .iter()
            .any(|item| item.contains("same-category task switches")));
        assert!(!risks.iter().any(|item| item.contains("no ticket")));

        let recommendations = default_recommendations(&local_data);
        assert!(recommendations
            .iter()
            .any(|item| item.contains("no issue tracker is required")));
    }

    #[test]
    fn rule_based_copy_does_not_turn_capture_counts_into_outcomes() {
        let local_data = serde_json::json!({
            "period_start": "2026-08-01",
            "period_end": "2026-08-07",
            "period_days": 7,
            "total_seconds": 7200,
            "total_hours": 2.0,
            "activity_count": 4,
            "active_days": 2,
            "tracking_consistency_pct": 28.6,
            "task_label_coverage_pct": 100.0,
            "deep_focus_hours": 2.0,
            "deep_focus_sessions": 2,
            "focus_eligible_seconds": 7200,
            "distraction_events": 0,
            "distraction_hours": 0.0,
            "category_breakdown": [{"category":"Writing","total_seconds":7200,"count":4}],
            "ticket_breakdown": [{"ticket":"Proposal","total_seconds":7200,"count":4}],
            "focus_semantics": {
                "fragmentation_pct": 0.0,
                "explicit_theme_switches_per_labelled_focus_hour": 0.0,
                "explicit_theme_coverage_pct": 100.0,
                "focus_eligible_seconds": 7200,
                "distraction_events": 0,
                "distraction_seconds": 0,
                "hourly_deep_focus": []
            }
        });

        let report = build_rule_based_report(&local_data);
        assert_eq!(report["health_breakdown"][1]["status"], "Observed");
        assert!(report["health_breakdown"][1]["notes"]
            .as_str()
            .unwrap()
            .contains("activity observations"));
        assert!(!report["health_notes"]
            .as_str()
            .unwrap()
            .contains("Distraction categories"));
        assert!(report["lessons_learned"]
            .as_array()
            .unwrap()
            .iter()
            .any(|lesson| lesson["title"] == "Sustained-work pattern"));
    }

    #[test]
    fn missing_ai_learning_fields_recover_from_recorded_activity() {
        let local_data = serde_json::json!({
            "total_seconds": 7200,
            "focus_eligible_seconds": 3600,
            "tracking_consistency_pct": 50.0,
            "category_breakdown": [{"category": "Analysis", "total_seconds": 7200}],
            "focus_semantics": {"fragmentation_pct": 25.0, "focus_eligible_seconds": 3600},
        });
        let mut report = serde_json::json!({
            "lessons_learned": [{"title": "", "body": "Missing title"}],
            "recommendations": [],
        });

        repair_learning_fields(&mut report, &local_data);

        assert!(report["lessons_learned"].as_array().unwrap().len() >= 2);
        assert!(report["lessons_learned"]
            .as_array()
            .unwrap()
            .iter()
            .any(|lesson| lesson["title"] == "Inspect fragmentation"));
        assert!(report["lessons_learned"]
            .as_array()
            .unwrap()
            .iter()
            .any(|lesson| lesson["body"]
                .as_str()
                .is_some_and(|body| body.contains("2.0h (100% of tracked time)"))));
        assert!(!report["recommendations"].as_array().unwrap().is_empty());

        let mut missing_section = serde_json::json!({});
        repair_learning_fields(&mut missing_section, &local_data);
        assert!(!missing_section["lessons_learned"]
            .as_array()
            .unwrap()
            .is_empty());
    }

    #[test]
    fn learning_repair_rejects_unverified_ai_copy_and_empty_period_claims() {
        let local_data = serde_json::json!({"total_seconds": 3600});
        let mut report = serde_json::json!({
            "lessons_learned": [{"title": "Observed pattern", "body": "One hour was recorded."}],
            "recommendations": ["Keep the next session labelled."],
        });
        repair_learning_fields(&mut report, &local_data);
        assert_ne!(report["lessons_learned"][0]["title"], "Observed pattern");
        assert_ne!(
            report["recommendations"][0],
            "Keep the next session labelled."
        );

        let mut empty_report = report;
        repair_learning_fields(&mut empty_report, &serde_json::json!({"total_seconds": 0}));
        assert!(empty_report["lessons_learned"]
            .as_array()
            .unwrap()
            .is_empty());
        assert!(build_lessons_learned(&serde_json::json!({"total_seconds": 0})).is_empty());
    }

    #[test]
    fn english_report_cleanup_preserves_decimal_metrics_and_domain_names() {
        let copy = "FlowSight.ai recorded 2.0h (100% of tracked time). Review the mix.";
        assert_eq!(extract_english_text(copy), copy);
        assert_eq!(extract_english_text("2.5h tracked"), "2.5h tracked");
    }

    #[test]
    fn grounded_selection_only_reorders_verified_candidates() {
        let fallback = serde_json::json!({
            "lessons_learned": [
                {"title":"Observed A", "body":"2.0h of Analysis recorded."},
                {"title":"Observed B", "body":"1.0h of Research recorded."}
            ],
            "recommendations": ["Review the Analysis block.", "Label the next task."],
            "health_notes": "2.0h of Analysis recorded."
        });
        let selection = serde_json::json!({
            "selected_indices": {"lessons_learned":[1,0], "recommendations":[1]}
        });
        let chosen = apply_grounded_selection(&fallback, &selection).unwrap();
        assert_eq!(chosen["lessons_learned"][0], fallback["lessons_learned"][1]);
        assert_eq!(chosen["recommendations"][0], fallback["recommendations"][1]);
        assert_eq!(chosen["health_notes"], fallback["health_notes"]);
    }

    #[test]
    fn grounded_selection_rejects_fabricated_prose_and_invalid_indices() {
        let fallback = serde_json::json!({
            "observed_work": ["Analysis was recorded.", "Research was recorded."],
            "summary": "Only observed work is shown."
        });
        for invalid in [
            serde_json::json!({"observed_work":["Completed product launch."]}),
            serde_json::json!({"selected_indices":{"observed_work":[2]}}),
            serde_json::json!({"selected_indices":{"observed_work":[0,0]}}),
            serde_json::json!({"selected_indices":{"observed_work":[0]}, "summary":"Completed product launch."}),
            serde_json::json!({"selected_indices":{"observed_work":["0"]}}),
        ] {
            assert!(apply_grounded_selection(&fallback, &invalid).is_none());
        }
    }
}
