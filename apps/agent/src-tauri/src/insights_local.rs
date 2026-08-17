use chrono::Local;
use reqwest::blocking::Client;
use rusqlite::{params, Connection};
use serde::Serialize;
use std::collections::HashMap;
use std::time::Duration;
use tauri::Emitter;

use crate::vision_model::LLAMA_CHAT_MODEL_ID;

const FOCUS_CATEGORIES: &[&str] = &[
    "Coding", "Debugging", "CodeReview", "Testing", "Design", "DevOps", "Database",
];
const DISTRACTION_CATEGORIES: &[&str] = &["Browsing", "Idle"];

/// Per-section LLM context from SQLite aggregates (separate calls, richer than a single snapshot).
const LLM_SECTION_STATS_MAX_CHARS: usize = 2800;

const REPORT_SYSTEM_PROMPT: &str = "You are an expert productivity consultant writing detailed work status reports. \
CRITICAL: English only. Output valid JSON only — no markdown. \
Cite specific dates, ticket IDs, hours, categories, and task descriptions from the provided STATS. \
Be concrete and actionable; avoid generic filler. \
Array fields may contain up to 6 items. String fields may be up to 220 characters.";

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
struct LongSessionRow {
    date: String,
    category: String,
    description: String,
    duration_seconds: i32,
    ticket: Option<String>,
}

#[derive(Serialize)]
struct HourlyFocusRow {
    hour: u8,
    focus_minutes: i32,
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

/// Aggregated local SQLite activity for cloud AI reports (Individual plan).
pub fn build_local_insights_report(db_path: &std::path::Path, period_days: i32) -> Result<serde_json::Value, String> {
    let days = period_days.clamp(1, 30);
    let conn = Connection::open(db_path).map_err(|e| e.to_string())?;

    let period_end = Local::now().date_naive();
    let period_start = period_end - chrono::Duration::days((days - 1) as i64);
    let start_str = period_start.format("%Y-%m-%d").to_string();
    let end_str = period_end.format("%Y-%m-%d").to_string();

    let mut stmt = conn
        .prepare(
            "SELECT datetime(created_at, 'localtime') as ts,
                    date(created_at, 'localtime') as d,
                    CAST(strftime('%H', created_at, 'localtime') AS INTEGER) as hour,
                    activity_type,
                    description,
                    jira_ticket_id,
                    duration_seconds,
                    COALESCE(synced, 0) as synced
             FROM reports
             WHERE date(created_at, 'localtime') >= ?1
               AND date(created_at, 'localtime') <= ?2
             ORDER BY datetime(created_at, 'localtime') ASC",
        )
        .map_err(|e| e.to_string())?;

    let rows: Vec<(String, String, u8, String, String, Option<String>, i32, i32)> = stmt
        .query_map(params![start_str, end_str], |row| {
            Ok((
                row.get(0)?,
                row.get(1)?,
                row.get::<_, i32>(2)? as u8,
                row.get(3)?,
                row.get(4)?,
                row.get(5)?,
                row.get(6)?,
                row.get(7)?,
            ))
        })
        .map_err(|e| e.to_string())?
        .filter_map(|r| r.ok())
        .collect();

    let mut cat_map: HashMap<String, (i32, i32)> = HashMap::new();
    let mut ticket_map: HashMap<String, (i32, i32)> = HashMap::new();
    let mut daily_map: HashMap<String, (i32, i32)> = HashMap::new();
    let mut daily_category: HashMap<String, HashMap<String, i32>> = HashMap::new();
    let mut hourly_focus: HashMap<u8, i32> = HashMap::new();
    let mut theme_map: HashMap<String, (i32, i32)> = HashMap::new();
    let mut focus_seconds = 0i32;
    let mut distraction_count = 0i32;
    let mut distraction_seconds = 0i32;
    let mut total_seconds = 0i32;
    let mut ticketed_seconds = 0i32;
    let mut unsynced_count = 0i32;
    let mut deep_focus_sessions = 0i32;
    let mut session_lengths: Vec<i32> = Vec::new();
    let mut context_switches = 0i32;
    let mut all_samples: Vec<ActivitySample> = Vec::with_capacity(rows.len());

    let mut prev_day: Option<&str> = None;
    let mut prev_category: Option<&str> = None;

    for (_ts, date, hour, category, description, ticket, dur, synced) in &rows {
        total_seconds += dur;
        session_lengths.push(*dur);

        let cat = cat_map.entry(category.clone()).or_insert((0, 0));
        cat.0 += dur;
        cat.1 += 1;

        let day = daily_map.entry(date.clone()).or_insert((0, 0));
        day.0 += dur;
        day.1 += 1;

        daily_category
            .entry(date.clone())
            .or_default()
            .entry(category.clone())
            .and_modify(|s| *s += dur)
            .or_insert(*dur);

        if FOCUS_CATEGORIES.contains(&category.as_str()) {
            focus_seconds += dur;
            *hourly_focus.entry(*hour).or_insert(0) += dur;
            if *dur >= 1800 {
                deep_focus_sessions += 1;
            }
        }
        if DISTRACTION_CATEGORIES.contains(&category.as_str()) {
            distraction_count += 1;
            distraction_seconds += dur;
        }

        if let Some(t) = ticket {
            if !t.is_empty() {
                ticketed_seconds += dur;
                let tk = ticket_map.entry(t.clone()).or_insert((0, 0));
                tk.0 += dur;
                tk.1 += 1;
                let theme_key = format!("Ticket {}", t);
                let th = theme_map.entry(theme_key).or_insert((0, 0));
                th.0 += dur;
                th.1 += 1;
            }
        }

        if ticket.as_ref().map(|t| t.is_empty()).unwrap_or(true) {
            let theme_key = format!("{} — {}", category, clamp_line(description, 48));
            let th = theme_map.entry(theme_key).or_insert((0, 0));
            th.0 += dur;
            th.1 += 1;
        }

        if *synced == 0 {
            unsynced_count += 1;
        }

        if prev_day == Some(date.as_str()) {
            if prev_category != Some(category.as_str()) {
                context_switches += 1;
            }
        }
        prev_day = Some(date.as_str());
        prev_category = Some(category.as_str());

        all_samples.push(ActivitySample {
            date: date.clone(),
            category: category.clone(),
            description: clamp_line(description, 220),
            duration_seconds: *dur,
            ticket: ticket.clone(),
        });
    }

    let unticketed_seconds = total_seconds - ticketed_seconds;
    let active_days = daily_map.len() as i32;
    let avg_session_minutes = if rows.is_empty() {
        0.0
    } else {
        (total_seconds as f64 / rows.len() as f64 / 60.0 * 10.0).round() / 10.0
    };

    let mut category_breakdown: Vec<CategoryRow> = cat_map
        .into_iter()
        .map(|(category, (total_seconds, count))| CategoryRow {
            category,
            total_seconds,
            count,
        })
        .collect();
    category_breakdown.sort_by(|a, b| b.total_seconds.cmp(&a.total_seconds));

    let mut ticket_breakdown: Vec<TicketRow> = ticket_map
        .into_iter()
        .map(|(ticket, (total_seconds, count))| TicketRow {
            ticket,
            total_seconds,
            count,
        })
        .collect();
    ticket_breakdown.sort_by(|a, b| b.total_seconds.cmp(&a.total_seconds));
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

    let mut hourly_focus_rows: Vec<HourlyFocusRow> = hourly_focus
        .into_iter()
        .map(|(hour, secs)| HourlyFocusRow {
            hour,
            focus_minutes: secs / 60,
        })
        .collect();
    hourly_focus_rows.sort_by(|a, b| b.focus_minutes.cmp(&a.focus_minutes));

    let mut work_themes: Vec<WorkThemeRow> = theme_map
        .into_iter()
        .map(|(label, (total_seconds, activity_count))| WorkThemeRow {
            label,
            total_seconds,
            activity_count,
        })
        .collect();
    work_themes.sort_by(|a, b| b.total_seconds.cmp(&a.total_seconds));
    work_themes.truncate(12);

    let mut longest_sessions: Vec<LongSessionRow> = all_samples
        .iter()
        .map(|s| LongSessionRow {
            date: s.date.clone(),
            category: s.category.clone(),
            description: s.description.clone(),
            duration_seconds: s.duration_seconds,
            ticket: s.ticket.clone(),
        })
        .collect();
    longest_sessions.sort_by(|a, b| b.duration_seconds.cmp(&a.duration_seconds));
    longest_sessions.truncate(12);

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

    let peak_focus_hour = hourly_focus_rows.first().map(|h| {
        serde_json::json!({
            "hour": h.hour,
            "focus_minutes": h.focus_minutes,
        })
    });

    let prior_period = query_prior_period_metrics(&conn, period_start, days, total_seconds)?;

    let sample_activities = build_diverse_activity_samples(&all_samples, &longest_sessions);

    let ticket_coverage_pct = if total_seconds > 0 {
        ((ticketed_seconds as f64 / total_seconds as f64) * 1000.0).round() / 10.0
    } else {
        0.0
    };
    let focus_ratio_pct = if total_seconds > 0 {
        ((focus_seconds as f64 / total_seconds as f64) * 1000.0).round() / 10.0
    } else {
        0.0
    };
    let tracking_consistency_pct = if days > 0 {
        ((active_days as f64 / days as f64) * 1000.0).round() / 10.0
    } else {
        0.0
    };
    let context_switches_per_day = if active_days > 0 {
        ((context_switches as f64 / active_days as f64) * 10.0).round() / 10.0
    } else {
        0.0
    };

    Ok(serde_json::json!({
        "source": "local_sqlite",
        "period_start": start_str,
        "period_end": end_str,
        "period_days": days,
        "total_seconds": total_seconds,
        "total_hours": round_hours(total_seconds),
        "activity_count": rows.len(),
        "focus_seconds": focus_seconds,
        "focus_hours": round_hours(focus_seconds),
        "focus_ratio_pct": focus_ratio_pct,
        "distraction_events": distraction_count,
        "distraction_seconds": distraction_seconds,
        "distraction_hours": round_hours(distraction_seconds),
        "ticketed_seconds": ticketed_seconds,
        "unticketed_seconds": unticketed_seconds,
        "ticketed_hours": round_hours(ticketed_seconds),
        "unticketed_hours": round_hours(unticketed_seconds),
        "ticket_coverage_pct": ticket_coverage_pct,
        "active_days": active_days,
        "tracking_consistency_pct": tracking_consistency_pct,
        "avg_session_minutes": avg_session_minutes,
        "deep_focus_sessions_30m_plus": deep_focus_sessions,
        "context_switches": context_switches,
        "context_switches_per_day": context_switches_per_day,
        "unsynced_reports": unsynced_count,
        "peak_day": peak_day,
        "quiet_day": quiet_day,
        "peak_focus_hour": peak_focus_hour,
        "prior_period": prior_period,
        "category_breakdown": category_breakdown,
        "ticket_breakdown": ticket_breakdown,
        "daily_totals": daily_totals,
        "day_category_breakdown": day_category_breakdown,
        "hourly_focus": hourly_focus_rows,
        "work_themes": work_themes,
        "longest_sessions": longest_sessions,
        "sample_activities": sample_activities,
    }))
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
    let local_data = build_local_insights_report(&db_path, days)?;

    let app_handle = app.clone();
    emit_report_progress(&app_handle, 0, "warmup", "Starting local AI engine", "Preparing model…", "start");
    crate::agent::ensure_local_llm_ready(app, state)?;
    emit_report_progress(&app_handle, 0, "warmup", "Starting local AI engine", "Local AI ready", "done");

    let user_prefs = crate::user_preferences::load_user_preferences(&db_path).unwrap_or_default();
    let prefs_block = crate::user_preferences::preferences_llm_block(&user_prefs);

    let (report, generation_passes) = match generate_report_by_sections(&app_handle, &local_data, &prefs_block) {
        Ok(result) => result,
        Err(err) => {
            log::warn!("[LocalReport] Pipeline incomplete ({}), merging partial + structured fallback", err);
            let mut fallback = build_rule_based_report(&local_data);
            sanitize_report_english(&mut fallback);
            (
                fallback,
                vec![serde_json::json!({
                    "id": "fallback",
                    "label": "Structured summary",
                    "detail": "Full AI pipeline could not finish; showing data-driven report in English."
                })],
            )
        }
    };

    let mut report = report;
    sanitize_report_english(&mut report);

    let ai_powered = generation_passes
        .iter()
        .any(|p| p["id"].as_str() != Some("fallback"));

    Ok(serde_json::json!({
        "local_data": local_data,
        "report": report,
        "user_preferences": user_prefs,
        "generated_at": Local::now().format("%Y-%m-%d %H:%M").to_string(),
        "model": "FlowSight Local Vision",
        "ai_powered": ai_powered,
        "generation_passes": generation_passes,
    }))
}

fn call_local_llm(prompt: &str, max_tokens: u32, temperature: f32) -> Result<String, String> {
    call_local_llm_with_system(prompt, max_tokens, temperature, REPORT_SYSTEM_PROMPT)
}

fn call_local_llm_with_system(
    prompt: &str,
    max_tokens: u32,
    temperature: f32,
    system_prompt: &str,
) -> Result<String, String> {
    let chat_url = crate::llama_port::managed_chat_completions_url().ok_or_else(|| {
        "Local AI server offline.".to_string()
    })?;

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
        "stream": false
    });

    let resp = client
        .post(&chat_url)
        .json(&body)
        .send()
        .map_err(|e| e.to_string())?;

    if !resp.status().is_success() {
        let status = resp.status();
        let err_body = resp.text().unwrap_or_default();
        return Err(format!("Local AI request failed ({}): {}", status, err_body));
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

fn call_local_llm_json(prompt: &str, max_tokens: u32, temperature: f32) -> Result<serde_json::Value, String> {
    let json_hint = "\n\nReturn valid JSON only. Up to 6 array items; cite specific STATS facts in each string.";
    let mut last_err = String::from("unknown error");

    for attempt in 0..=LLM_PASS_RETRIES {
        let user_prompt = if attempt == 0 {
            format!("{}{}", prompt, json_hint)
        } else {
            format!(
                "{}{}\n\n(RETRY {}/{}) Return valid JSON. English only. Keep schema, be specific.",
                prompt, json_hint, attempt + 1, LLM_PASS_RETRIES + 1
            )
        };

        let raw = match call_local_llm(&user_prompt, max_tokens, temperature) {
            Ok(r) => r,
            Err(e) => {
                last_err = e;
                continue;
            }
        };

        match parse_report_json(&raw) {
            Ok(v) => return Ok(v),
            Err(e) => {
                last_err = e.clone();
                log::warn!("[LocalReport] JSON parse attempt {} failed: {}", attempt + 1, e);
                if attempt < LLM_PASS_RETRIES {
                    if let Ok(fixed) = call_local_llm(
                        &format!(
                            "The following is broken JSON. Return ONLY repaired valid JSON. English text only. Same schema, compact.\n\n{}",
                            raw.chars().take(1200).collect::<String>()
                        ),
                        max_tokens,
                        0.1,
                    ) {
                        if let Ok(v) = parse_report_json(&fixed) {
                            return Ok(v);
                        }
                    }
                }
            }
        }
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
        "progress_tasks" => result["tasks_completed"]
            .as_array()
            .and_then(|a| a.first())
            .and_then(|v| v.as_str()),
        "lessons_recommendations" => result["lessons_learned"]
            .as_array()
            .and_then(|a| a.first())
            .and_then(|l| l["title"].as_str()),
        _ => None,
    };
    extract_english_text(raw.unwrap_or("Section complete."))
}

fn llm_section(
    app: &tauri::AppHandle,
    step: u32,
    pass_id: &str,
    label: &str,
    stats: &str,
    prompt_body: &str,
    max_tokens: u32,
    temperature: f32,
    fallback: serde_json::Value,
    passes: &mut Vec<serde_json::Value>,
) -> serde_json::Value {
    emit_report_progress(app, step, pass_id, label, "Generating with local AI…", "start");
    log::info!("[LocalReport] Section {} — {}", step, pass_id);
    let prompt = format!("{}\n\nSTATS:\n{}", prompt_body, stats);
    let result = call_local_llm_json(&prompt, max_tokens, temperature).unwrap_or_else(|err| {
        log::warn!("[LocalReport] Section {} fallback: {}", pass_id, err);
        fallback
    });
    let detail = section_detail(&result, pass_id);
    emit_report_progress(app, step, pass_id, label, &detail, "done");
    passes.push(serde_json::json!({
        "id": pass_id,
        "label": label,
        "detail": detail,
    }));
    result
}

fn build_report_meta(local_data: &serde_json::Value) -> serde_json::Value {
    let top_category = local_data["category_breakdown"]
        .as_array()
        .and_then(|a| a.first())
        .and_then(|c| c["category"].as_str())
        .unwrap_or("General work");

    serde_json::json!({
        "period_label": format!(
            "{} — {}",
            local_data["period_start"].as_str().unwrap_or(""),
            local_data["period_end"].as_str().unwrap_or("")
        ),
        "period_name": format!("Workflow · {}", top_category),
        "focus_target": top_category,
        "tracked_hours": local_data["total_hours"],
        "focus_hours": local_data["focus_hours"],
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
    let stats = |section_id: &str| build_section_stats_snapshot(section_id, local_data, prefs_block);

    let project = llm_section(
        app,
        1,
        "project_summary",
        "Section — project summary",
        &stats("project_summary"),
        "English only. Use ONLY STATS (SQLite activity reports: descriptions, tickets, durations).\n\
Use USER_PROFILE to personalize tone and priorities. Reference top categories, tickets, hours, peak day, and focus ratio with numbers.\n\
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
Explain health using focus ratio, distraction time, deep-focus session count, tracking consistency, and ticket coverage.\n\
Return JSON: {\"overall_health\":\"On track|Attention|At risk\",\"health_notes\":\"detailed paragraph (3-5 sentences) with specific metrics, dates, and personalized recommendations\"}",
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
        "English only. Use ONLY STATS. Up to 6 rows covering categories AND top tickets where relevant.\n\
Each notes field must cite hours, activity count, or a concrete description sample.\n\
Return JSON: {\"health_breakdown\":[{\"element\":\"work area, category, or ticket\",\"status\":\"On track|Attention|At risk\",\"owner_team\":\"Self\",\"notes\":\"specific 1-2 sentence insight\"}]}",
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
Include risks from low tracking consistency, unticketed work, prior-period decline, or fragmented focus.\n\
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
        "Section — progress & tasks completed",
        &stats("progress_tasks"),
        "English only. Use ONLY STATS. Up to 6 items per array.\n\
tasks_completed: cite tickets and/or longest session descriptions. work_progress: cite daily totals and themes.\n\
Return JSON: {\"work_progress\":[\"daily or thematic highlight with date/hours\"],\"tasks_completed\":[\"specific completed work from tickets or descriptions\"]}",
        680,
        0.25,
        serde_json::json!({
            "work_progress": rule_fallback["work_progress"],
            "tasks_completed": rule_fallback["tasks_completed"],
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
        "tasks_completed": progress["tasks_completed"],
        "lessons_learned": lessons["lessons_learned"],
        "recommendations": lessons["recommendations"],
    });

    Ok((report, passes))
}

fn build_analysis_from_stats(local_data: &serde_json::Value) -> serde_json::Value {
    let mut focus_patterns = Vec::new();
    let mut distraction_patterns = Vec::new();
    let mut themes = Vec::new();

    if let Some(cats) = local_data["category_breakdown"].as_array() {
        for c in cats.iter().take(5) {
            let name = c["category"].as_str().unwrap_or("");
            let h = c["total_seconds"].as_i64().unwrap_or(0) as f64 / 3600.0;
            if FOCUS_CATEGORIES.contains(&name) {
                focus_patterns.push(format!("{:.1}h in {}", h, name));
            } else if DISTRACTION_CATEGORIES.contains(&name) {
                distraction_patterns.push(format!("{:.1}h in {}", h, name));
            }
            themes.push(format!("{} ({:.1}h)", name, h));
        }
    }

    let daily_rhythm = if let Some(days) = local_data["daily_totals"].as_array() {
        let active: Vec<_> = days
            .iter()
            .filter(|d| d["total_seconds"].as_i64().unwrap_or(0) > 0)
            .map(|d| {
                format!(
                    "{} {:.1}h",
                    d["date"].as_str().unwrap_or(""),
                    d["total_seconds"].as_i64().unwrap_or(0) as f64 / 3600.0
                )
            })
            .collect();
        if active.is_empty() {
            "No daily rhythm — insufficient tracked days.".to_string()
        } else {
            format!("Activity spread across: {}.", active.join(", "))
        }
    } else {
        "Insufficient daily data.".to_string()
    };

    serde_json::json!({
        "focus_patterns": focus_patterns,
        "distraction_patterns": distraction_patterns,
        "top_work_themes": themes,
        "ticket_progress": build_tasks_completed(local_data),
        "daily_rhythm": daily_rhythm,
        "friction_points": build_known_issues(local_data, local_data["distraction_events"].as_i64().unwrap_or(0) as i32),
        "notable_wins": build_work_progress(local_data),
        "data_gaps": ["Track consistently throughout the day for richer patterns."]
    })
}

fn build_diagnosis_from_stats(
    local_data: &serde_json::Value,
    _analysis: &serde_json::Value,
) -> serde_json::Value {
    let focus_seconds = local_data["focus_seconds"].as_i64().unwrap_or(0) as i32;
    let total_seconds = local_data["total_seconds"].as_i64().unwrap_or(0) as i32;
    let distractions = local_data["distraction_events"].as_i64().unwrap_or(0) as i32;
    let health = compute_overall_health(focus_seconds, total_seconds, distractions);

    serde_json::json!({
        "overall_health": health,
        "health_notes": format!(
            "Focus ratio {:.0}% with {} distraction events in period.",
            if total_seconds > 0 { focus_seconds as f64 / total_seconds as f64 * 100.0 } else { 0.0 },
            distractions
        ),
        "core_issues": build_known_issues(local_data, distractions),
        "risk_flags": build_potential_risks(local_data, focus_seconds, total_seconds),
        "health_breakdown": build_category_health_rows(local_data),
    })
}

fn build_section_stats_snapshot(
    section_id: &str,
    local_data: &serde_json::Value,
    prefs_block: &str,
) -> String {
    let mut lines = build_stats_header_lines(local_data);
    if !prefs_block.is_empty() {
        lines.push(prefs_block.to_string());
        lines.push(
            "Personalize insights for USER_PROFILE roles, activities, and improvement goals.".to_string(),
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
            append_prior_period(&mut lines, local_data);
            append_daily_detail(&mut lines, local_data);
            append_top_tickets(&mut lines, local_data, 5);
        }
        "progress_tasks" => {
            append_top_tickets(&mut lines, local_data, 12);
            append_longest_sessions(&mut lines, local_data, 10);
            append_work_themes(&mut lines, local_data, 8);
            append_daily_detail(&mut lines, local_data);
            append_activity_samples(&mut lines, local_data, 14, 140);
        }
        "lessons_recommendations" | _ => {
            append_health_metrics(&mut lines, local_data);
            append_top_categories(&mut lines, local_data, 6);
            append_top_tickets(&mut lines, local_data, 6);
            append_hourly_focus(&mut lines, local_data, 4);
            append_prior_period(&mut lines, local_data);
            append_work_themes(&mut lines, local_data, 6);
            append_longest_sessions(&mut lines, local_data, 4);
        }
    }

    truncate_stats_text(lines.join("\n"), LLM_SECTION_STATS_MAX_CHARS)
}

fn build_stats_header_lines(local_data: &serde_json::Value) -> Vec<String> {
    let period_start = local_data["period_start"].as_str().unwrap_or("?");
    let period_end = local_data["period_end"].as_str().unwrap_or("?");
    let days = local_data["period_days"].as_i64().unwrap_or(7);
    let total_h = local_data["total_hours"].as_f64().unwrap_or(0.0);
    let focus_h = local_data["focus_hours"].as_f64().unwrap_or(0.0);
    let focus_pct = local_data["focus_ratio_pct"].as_f64().unwrap_or(0.0);
    let activities = local_data["activity_count"].as_i64().unwrap_or(0);
    let distractions = local_data["distraction_events"].as_i64().unwrap_or(0);
    let distraction_h = local_data["distraction_hours"].as_f64().unwrap_or(0.0);

    vec![
        format!("PERIOD: {} to {} ({} days)", period_start, period_end, days),
        format!(
            "TOTAL: {:.1}h | FOCUS: {:.1}h ({:.0}%) | ACTIVITIES: {} | DISTRACTIONS: {} events ({:.1}h)",
            total_h, focus_h, focus_pct, activities, distractions, distraction_h
        ),
    ]
}

fn append_health_metrics(lines: &mut Vec<String>, local_data: &serde_json::Value) {
    let ticket_cov = local_data["ticket_coverage_pct"].as_f64().unwrap_or(0.0);
    let ticketed_h = local_data["ticketed_hours"].as_f64().unwrap_or(0.0);
    let unticketed_h = local_data["unticketed_hours"].as_f64().unwrap_or(0.0);
    let active_days = local_data["active_days"].as_i64().unwrap_or(0);
    let consistency = local_data["tracking_consistency_pct"].as_f64().unwrap_or(0.0);
    let avg_session = local_data["avg_session_minutes"].as_f64().unwrap_or(0.0);
    let deep_focus = local_data["deep_focus_sessions_30m_plus"].as_i64().unwrap_or(0);
    let switches = local_data["context_switches_per_day"].as_f64().unwrap_or(0.0);
    let unsynced = local_data["unsynced_reports"].as_i64().unwrap_or(0);

    lines.push(format!(
        "HEALTH: {:.0}% days tracked ({}/{}d) | avg session {:.0}m | deep-focus sessions 30m+: {} | context switches/day: {:.1}",
        consistency,
        active_days,
        local_data["period_days"].as_i64().unwrap_or(7),
        avg_session,
        deep_focus,
        switches
    ));
    lines.push(format!(
        "TICKETS: {:.1}h ticketed ({:.0}%) | {:.1}h unticketed | {} unsynced local reports",
        ticketed_h, ticket_cov, unticketed_h, unsynced
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
                format!("{} {:.1}h ({} sessions)", id, h, n)
            })
            .collect();
        if !top.is_empty() {
            lines.push(format!("TICKETS: {}", top.join(" | ")));
        }
    }
}

fn append_daily_detail(lines: &mut Vec<String>, local_data: &serde_json::Value) {
    if let Some(days_arr) = local_data["daily_totals"].as_array() {
        for d in days_arr {
            let date = d["date"].as_str().unwrap_or("?");
            let h = d["total_seconds"].as_i64().unwrap_or(0) as f64 / 3600.0;
            let n = d["activity_count"].as_i64().unwrap_or(0);
            lines.push(format!("- DAY {}: {:.1}h, {} activities", date, h, n));
        }
    }
}

fn append_day_categories(lines: &mut Vec<String>, local_data: &serde_json::Value) {
    if let Some(days) = local_data["day_category_breakdown"].as_array() {
        for d in days {
            lines.push(format!(
                "- DAY {} dominant: {} ({:.1}h of {:.1}h)",
                d["date"].as_str().unwrap_or("?"),
                d["top_category"].as_str().unwrap_or("?"),
                d["top_hours"].as_f64().unwrap_or(0.0),
                d["total_hours"].as_f64().unwrap_or(0.0),
            ));
        }
    }
}

fn append_hourly_focus(lines: &mut Vec<String>, local_data: &serde_json::Value, limit: usize) {
    if let Some(hours) = local_data["hourly_focus"].as_array() {
        let top: Vec<String> = hours
            .iter()
            .take(limit)
            .map(|h| {
                format!(
                    "{}:00 {}m focus",
                    h["hour"].as_u64().unwrap_or(0),
                    h["focus_minutes"].as_i64().unwrap_or(0)
                )
            })
            .collect();
        if !top.is_empty() {
            lines.push(format!("FOCUS_HOURS: {}", top.join(", ")));
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
    if let Some(sessions) = local_data["longest_sessions"].as_array() {
        for s in sessions.iter().take(limit) {
            let ticket = s["ticket"].as_str().unwrap_or("");
            let ticket_part = if ticket.is_empty() {
                String::new()
            } else {
                format!(" [{}]", ticket)
            };
            lines.push(format!(
                "- SESSION {} {}{} {}m: {}",
                s["date"].as_str().unwrap_or(""),
                s["category"].as_str().unwrap_or(""),
                ticket_part,
                s["duration_seconds"].as_i64().unwrap_or(0) / 60,
                clamp_line(s["description"].as_str().unwrap_or(""), 100)
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
            "PEAK_FOCUS_HOUR: {}:00 ({} focus minutes)",
            hour["hour"].as_u64().unwrap_or(0),
            hour["focus_minutes"].as_i64().unwrap_or(0)
        ));
    }
}

fn append_prior_period(lines: &mut Vec<String>, local_data: &serde_json::Value) {
    if let Some(prior) = local_data.get("prior_period") {
        let prev_h = prior["total_hours"].as_f64().unwrap_or(0.0);
        let change = prior["change_pct"].as_f64().unwrap_or(0.0);
        lines.push(format!(
            "PRIOR_PERIOD ({} to {}): {:.1}h total, {:.1}h focus, {} activities | change vs prior: {:+.0}%",
            prior["period_start"].as_str().unwrap_or("?"),
            prior["period_end"].as_str().unwrap_or("?"),
            prev_h,
            prior["focus_hours"].as_f64().unwrap_or(0.0),
            prior["activity_count"].as_i64().unwrap_or(0),
            change
        ));
    }
}

fn append_distraction_detail(lines: &mut Vec<String>, local_data: &serde_json::Value) {
    if let Some(cats) = local_data["category_breakdown"].as_array() {
        for cat in cats {
            let name = cat["category"].as_str().unwrap_or("");
            if DISTRACTION_CATEGORIES.contains(&name) {
                let mins = cat["total_seconds"].as_i64().unwrap_or(0) / 60;
                let n = cat["count"].as_i64().unwrap_or(0);
                lines.push(format!("- DISTRACTION {}: {}m across {} events", name, mins, n));
            }
        }
    }
}

fn truncate_stats_text(text: String, max_chars: usize) -> String {
    if text.chars().count() <= max_chars {
        return text;
    }
    text.chars().take(max_chars).collect::<String>() + "…"
}

fn truncate_json_compact(value: &serde_json::Value, max_chars: usize) -> String {
    let s = serde_json::to_string(value).unwrap_or_else(|_| "{}".to_string());
    if s.chars().count() <= max_chars {
        return s;
    }
    s.chars().take(max_chars).collect::<String>() + "…"
}

fn merge_diagnosis_into_draft(draft: &mut serde_json::Value, diagnosis: &serde_json::Value) {
    if draft.get("overall_health").and_then(|v| v.as_str()).unwrap_or("").is_empty() {
        if let Some(h) = diagnosis.get("overall_health") {
            draft["overall_health"] = h.clone();
        }
    }
    if draft.get("health_notes").and_then(|v| v.as_str()).unwrap_or("").is_empty() {
        if let Some(n) = diagnosis.get("health_notes") {
            draft["health_notes"] = n.clone();
        }
    }
    let draft_bd = draft.get("health_breakdown").and_then(|v| v.as_array());
    if draft_bd.map(|a| a.is_empty()).unwrap_or(true) {
        if let Some(bd) = diagnosis.get("health_breakdown") {
            draft["health_breakdown"] = bd.clone();
        }
    }
    let draft_issues = draft.get("known_issues").and_then(|v| v.as_array());
    if draft_issues.map(|a| a.is_empty()).unwrap_or(true) {
        if let Some(issues) = diagnosis.get("core_issues") {
            draft["known_issues"] = issues.clone();
        }
    }
    let draft_risks = draft.get("potential_risks").and_then(|v| v.as_array());
    if draft_risks.map(|a| a.is_empty()).unwrap_or(true) {
        if let Some(risks) = diagnosis.get("risk_flags") {
            draft["potential_risks"] = risks.clone();
        }
    }
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

fn is_cjk_char(c: char) -> bool {
    matches!(
        c,
        '\u{4E00}'..='\u{9FFF}'
            | '\u{3400}'..='\u{4DBF}'
            | '\u{3040}'..='\u{30FF}'
            | '\u{AC00}'..='\u{D7AF}'
    )
}

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
fn extract_english_text(s: &str) -> String {
    let cleaned: String = s
        .chars()
        .map(|c| if is_cjk_char(c) { ' ' } else { c })
        .collect();

    let segments: Vec<String> = cleaned
        .split(|c| c == '.' || c == '!' || c == '?' || c == '\n')
        .map(str::trim)
        .filter(|seg| !seg.is_empty() && latin_ratio(seg) >= 0.55)
        .map(|seg| seg.split_whitespace().collect::<Vec<_>>().join(" "))
        .collect();

    if segments.is_empty() {
        return cleaned.split_whitespace().collect::<Vec<_>>().join(" ");
    }

    let mut out = segments.join(". ");
    if !out.ends_with('.') && s.contains('.') {
        out.push('.');
    }
    out
}

fn sanitize_report_english(value: &mut serde_json::Value) {
    match value {
        serde_json::Value::String(s) => {
            *s = extract_english_text(s);
        }
        serde_json::Value::Array(arr) => {
            for item in arr.iter_mut() {
                sanitize_report_english(item);
            }
        }
        serde_json::Value::Object(map) => {
            for (_, v) in map.iter_mut() {
                sanitize_report_english(v);
            }
        }
        _ => {}
    }
}

fn build_rule_based_report(local_data: &serde_json::Value) -> serde_json::Value {
    let total_hours = local_data["total_hours"].as_f64().unwrap_or(0.0);
    let focus_hours = local_data["focus_hours"].as_f64().unwrap_or(0.0);
    let activity_count = local_data["activity_count"].as_u64().unwrap_or(0);
    let distraction_events = local_data["distraction_events"].as_u64().unwrap_or(0);
    let period_start = local_data["period_start"].as_str().unwrap_or("");
    let period_end = local_data["period_end"].as_str().unwrap_or("");

    let focus_seconds = local_data["focus_seconds"].as_i64().unwrap_or(0) as i32;
    let total_seconds = local_data["total_seconds"].as_i64().unwrap_or(0) as i32;
    let overall_health = compute_overall_health(focus_seconds, total_seconds, distraction_events as i32);

    let health_breakdown = build_category_health_rows(local_data);
    let tasks_completed = build_tasks_completed(local_data);
    let known_issues = build_known_issues(local_data, distraction_events as i32);
    let potential_risks = build_potential_risks(local_data, focus_seconds, total_seconds);
    let work_progress = build_work_progress(local_data);
    let lessons_learned = build_lessons_learned(local_data, focus_hours, total_hours);

    let executive_overview = if total_seconds == 0 {
        format!(
            "Between {} and {} no activity was recorded in local SQLite reports. Enable monitoring to populate this report.",
            period_start, period_end
        )
    } else {
        let focus_pct = local_data["focus_ratio_pct"].as_f64().unwrap_or(0.0);
        let ticket_cov = local_data["ticket_coverage_pct"].as_f64().unwrap_or(0.0);
        let active_days = local_data["active_days"].as_i64().unwrap_or(0);
        let top_cat = local_data["category_breakdown"]
            .as_array()
            .and_then(|a| a.first())
            .and_then(|c| c["category"].as_str())
            .unwrap_or("General work");
        let top_ticket = local_data["ticket_breakdown"]
            .as_array()
            .and_then(|a| a.first())
            .and_then(|t| t["ticket"].as_str());
        let ticket_line = top_ticket
            .map(|t| format!(" Top ticket: {}.", t))
            .unwrap_or_default();
        format!(
            "Between {} and {} you tracked {:.1}h across {} SQLite activity reports on {} active days. \
Deep focus was {:.1}h ({:.0}% of tracked time). Primary category: {}.{ticket_line} \
Ticket-linked work covered {:.0}% of hours.",
            period_start,
            period_end,
            total_hours,
            activity_count,
            active_days,
            focus_hours,
            focus_pct,
            top_cat,
            ticket_cov
        )
    };

    let health_notes = if total_seconds == 0 {
        "No tracked activity in this period. Start monitoring to build a baseline.".to_string()
    } else {
        let focus_pct = local_data["focus_ratio_pct"].as_f64().unwrap_or(0.0);
        let deep = local_data["deep_focus_sessions_30m_plus"].as_i64().unwrap_or(0);
        let consistency = local_data["tracking_consistency_pct"].as_f64().unwrap_or(0.0);
        let switches = local_data["context_switches_per_day"].as_f64().unwrap_or(0.0);
        let distraction_h = local_data["distraction_hours"].as_f64().unwrap_or(0.0);
        format!(
            "Focus work represents {:.0}% of tracked time with {} deep-focus sessions of 30+ minutes. \
Tracking consistency was {:.0}% of days in the period. Distraction categories consumed {:.1}h across {} events. \
Context switching averaged {:.1} category changes per active day.",
            focus_pct,
            deep,
            consistency,
            distraction_h,
            distraction_events,
            switches
        )
    };

    serde_json::json!({
        "executive_overview": executive_overview,
        "work_summary": format!(
            "Primary effort concentrated on top categories and tickets shown in the breakdown. {:.1} total hours were captured locally without cloud sync.",
            total_hours
        ),
        "overall_health": overall_health,
        "health_notes": health_notes,
        "health_breakdown": health_breakdown,
        "known_issues": known_issues,
        "potential_risks": potential_risks,
        "tasks_completed": tasks_completed,
        "work_progress": work_progress,
        "lessons_learned": lessons_learned,
        "recommendations": default_recommendations(local_data),
    })
}

fn compute_overall_health(focus_seconds: i32, total_seconds: i32, distraction_events: i32) -> &'static str {
    if total_seconds == 0 {
        return "Attention";
    }
    let focus_ratio = focus_seconds as f64 / total_seconds as f64;
    if focus_ratio >= 0.5 && distraction_events < 8 {
        "On track"
    } else if focus_ratio >= 0.25 {
        "Attention"
    } else {
        "At risk"
    }
}

fn build_category_health_rows(local_data: &serde_json::Value) -> Vec<serde_json::Value> {
    let total_seconds = local_data["total_seconds"].as_i64().unwrap_or(1).max(1) as f64;
    let mut rows = Vec::new();

    if let Some(cats) = local_data["category_breakdown"].as_array() {
        for cat in cats.iter().take(6) {
            let name = cat["category"].as_str().unwrap_or("Other");
            let secs = cat["total_seconds"].as_i64().unwrap_or(0) as f64;
            let share = secs / total_seconds;
            let status = if DISTRACTION_CATEGORIES.contains(&name) && share > 0.15 {
                "Attention"
            } else if FOCUS_CATEGORIES.contains(&name) && share >= 0.08 {
                "On track"
            } else if share < 0.03 {
                "Attention"
            } else {
                "On track"
            };
            rows.push(serde_json::json!({
                "element": name,
                "status": status,
                "owner_team": "Self",
                "notes": format!(
                    "{:.1}h across {} SQLite reports ({:.0}% of period).",
                    secs / 3600.0,
                    cat["count"].as_i64().unwrap_or(0),
                    share * 100.0
                ),
            }));
        }
    }

    if rows.is_empty() {
        rows.push(serde_json::json!({
            "element": "Tracking",
            "status": "Attention",
            "owner_team": "Self",
            "notes": "No category data yet — enable monitoring during work sessions.",
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
                    "status": "On track",
                    "owner_team": "Self",
                    "notes": format!("{:.1}h logged across {} tracked sessions in SQLite.", secs / 3600.0, n),
                }));
            }
        }
    }

    rows
}

fn build_tasks_completed(local_data: &serde_json::Value) -> Vec<String> {
    let mut items = Vec::new();

    if let Some(sessions) = local_data["longest_sessions"].as_array() {
        for s in sessions.iter().take(6) {
            let desc = s["description"].as_str().unwrap_or("");
            let cat = s["category"].as_str().unwrap_or("Work");
            let mins = s["duration_seconds"].as_i64().unwrap_or(0) / 60;
            let date = s["date"].as_str().unwrap_or("");
            let ticket = s["ticket"].as_str().unwrap_or("");
            if !desc.is_empty() {
                let prefix = if ticket.is_empty() {
                    format!("{} · {}", date, cat)
                } else {
                    format!("{} · {} ({})", date, ticket, cat)
                };
                items.push(format!("{} — {} ({}m)", prefix, clamp_line(desc, 90), mins));
            }
        }
    }

    if let Some(tickets) = local_data["ticket_breakdown"].as_array() {
        for t in tickets.iter().take(6) {
            let ticket = t["ticket"].as_str().unwrap_or("");
            let hours = t["total_seconds"].as_i64().unwrap_or(0) as f64 / 3600.0;
            let n = t["count"].as_i64().unwrap_or(0);
            if !ticket.is_empty() && !items.iter().any(|i| i.contains(ticket)) {
                items.push(format!(
                    "Ticket {} — {:.1}h logged across {} SQLite activity reports",
                    ticket, hours, n
                ));
            }
        }
    }

    if items.is_empty() {
        if let Some(samples) = local_data["sample_activities"].as_array() {
            for s in samples.iter().take(6) {
                let desc = s["description"].as_str().unwrap_or("");
                let cat = s["category"].as_str().unwrap_or("Work");
                if !desc.is_empty() {
                    items.push(format!("{} — {}", cat, clamp_line(desc, 90)));
                }
            }
        }
    }

    if items.is_empty() {
        items.push("No completed tasks identified — capture more activity to populate this section.".to_string());
    }

    items
}

fn build_known_issues(local_data: &serde_json::Value, distraction_events: i32) -> Vec<String> {
    let mut issues = Vec::new();
    if distraction_events > 0 {
        issues.push(format!(
            "{} browsing/idle events detected — context switching may be reducing focus blocks.",
            distraction_events
        ));
    }

    if let Some(cats) = local_data["category_breakdown"].as_array() {
        for cat in cats {
            let name = cat["category"].as_str().unwrap_or("");
            if DISTRACTION_CATEGORIES.contains(&name) {
                let mins = cat["total_seconds"].as_i64().unwrap_or(0) / 60;
                let n = cat["count"].as_i64().unwrap_or(0);
                if mins >= 15 {
                    issues.push(format!(
                        "{} consumed {} minutes across {} SQLite reports this period.",
                        name, mins, n
                    ));
                }
            }
        }
    }

    let unticketed_h = local_data["unticketed_hours"].as_f64().unwrap_or(0.0);
    if unticketed_h >= 2.0 {
        issues.push(format!(
            "{:.1}h of work has no ticket label in SQLite — task attribution is incomplete.",
            unticketed_h
        ));
    }

    let consistency = local_data["tracking_consistency_pct"].as_f64().unwrap_or(100.0);
    if consistency < 60.0 {
        issues.push(format!(
            "Tracking gaps — only {:.0}% of days in the period have SQLite activity reports.",
            consistency
        ));
    }

    if issues.is_empty() {
        issues.push("No major friction patterns detected in tracked data.".to_string());
    }

    issues
}

fn build_potential_risks(local_data: &serde_json::Value, focus_seconds: i32, total_seconds: i32) -> Vec<String> {
    let mut risks = Vec::new();
    if total_seconds > 0 {
        let focus_ratio = focus_seconds as f64 / total_seconds as f64;
        if focus_ratio < 0.35 {
            risks.push(format!(
                "Low focus-to-total ratio ({:.0}%) — deep work blocks may be fragmented across {} activities.",
                focus_ratio * 100.0,
                local_data["activity_count"].as_i64().unwrap_or(0)
            ));
        }
    }

    let ticket_cov = local_data["ticket_coverage_pct"].as_f64().unwrap_or(0.0);
    if ticket_cov < 40.0 && total_seconds > 0 {
        risks.push(format!(
            "Only {:.0}% of tracked hours are linked to tickets — progress may be hard to audit.",
            ticket_cov
        ));
    }

    if let Some(prior) = local_data.get("prior_period") {
        let change = prior["change_pct"].as_f64().unwrap_or(0.0);
        if change < -15.0 {
            risks.push(format!(
                "Tracked hours fell {:.0}% vs prior period ({} to {}).",
                change.abs(),
                prior["period_start"].as_str().unwrap_or("?"),
                prior["period_end"].as_str().unwrap_or("?")
            ));
        }
    }

    if let Some(days) = local_data["daily_totals"].as_array() {
        let active_days = days.iter().filter(|d| d["total_seconds"].as_i64().unwrap_or(0) > 0).count();
        let period_days = local_data["period_days"].as_i64().unwrap_or(7) as usize;
        if active_days <= 2 && period_days >= 5 {
            risks.push(format!(
                "Sparse tracking — only {} of {} days have SQLite reports; workload may be under-represented.",
                active_days, period_days
            ));
        }
    }

    let switches = local_data["context_switches_per_day"].as_f64().unwrap_or(0.0);
    if switches > 15.0 {
        risks.push(format!(
            "High context switching ({:.1} category changes per active day) may reduce sustained focus.",
            switches
        ));
    }

    if risks.is_empty() {
        risks.push("Maintain consistent daily tracking to catch trends early.".to_string());
    }

    risks
}

fn build_work_progress(local_data: &serde_json::Value) -> Vec<String> {
    let mut progress = Vec::new();

    if let Some(days) = local_data["day_category_breakdown"].as_array() {
        for d in days.iter().rev().take(7) {
            progress.push(format!(
                "{} — {:.1}h total, mostly {} ({:.1}h)",
                d["date"].as_str().unwrap_or(""),
                d["total_hours"].as_f64().unwrap_or(0.0),
                d["top_category"].as_str().unwrap_or("Work"),
                d["top_hours"].as_f64().unwrap_or(0.0)
            ));
        }
    } else if let Some(days) = local_data["daily_totals"].as_array() {
        for d in days.iter().rev().take(5) {
            let date = d["date"].as_str().unwrap_or("");
            let hours = d["total_seconds"].as_i64().unwrap_or(0) as f64 / 3600.0;
            let count = d["activity_count"].as_i64().unwrap_or(0);
            if hours > 0.0 {
                progress.push(format!("{} — {:.1}h across {} SQLite captures", date, hours, count));
            }
        }
    }

    if let Some(themes) = local_data["work_themes"].as_array() {
        for t in themes.iter().take(3) {
            let label = t["label"].as_str().unwrap_or("");
            let h = t["total_seconds"].as_i64().unwrap_or(0) as f64 / 3600.0;
            if !label.is_empty() && h > 0.0 {
                progress.push(format!("Theme: {} — {:.1}h in period", label, h));
            }
        }
    }

    if progress.is_empty() {
        progress.push("No daily progress recorded for this period.".to_string());
    }

    progress
}

fn build_lessons_learned(
    local_data: &serde_json::Value,
    focus_hours: f64,
    total_hours: f64,
) -> Vec<serde_json::Value> {
    let mut lessons = Vec::new();

    if total_hours > 0.0 {
        let focus_share = focus_hours / total_hours;
        lessons.push(serde_json::json!({
            "title": "Protect focus blocks",
            "body": format!(
                "Only {:.0}% of tracked time was deep-focus work. Schedule 90-minute blocks for your top category and reduce reactive browsing between tasks.",
                focus_share * 100.0
            ),
        }));
    }

    if let Some(top) = local_data["category_breakdown"].as_array().and_then(|a| a.first()) {
        let cat = top["category"].as_str().unwrap_or("Work");
        lessons.push(serde_json::json!({
            "title": format!("Double down on {}", cat),
            "body": format!(
                "{} dominated your tracked hours. Align ticket selection and daily goals with this area to maximize visible progress.",
                cat
            ),
        }));
    }

    lessons.push(serde_json::json!({
        "title": "Track consistently",
        "body": "Status reports improve when monitoring runs throughout the day. Even short sessions build a clearer picture of workflow bottlenecks.",
    }));

    lessons
}

fn default_recommendations(local_data: &serde_json::Value) -> Vec<String> {
    let mut recs = vec![
        "Review top distraction categories and batch admin/browsing into fixed windows.".to_string(),
        "Link tasks to Jira/Linear tickets when on a paid plan for clearer progress reporting.".to_string(),
    ];

    if local_data["ticket_breakdown"]
        .as_array()
        .map(|a| a.is_empty())
        .unwrap_or(true)
    {
        recs.push("Assign tickets or manual task labels to improve task_completed sections.".to_string());
    }

    recs
}

fn round_hours(seconds: i32) -> f64 {
    ((seconds as f64 / 3600.0) * 10.0).round() / 10.0
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

    let mut stmt = conn
        .prepare(
            "SELECT activity_type, duration_seconds
             FROM reports
             WHERE date(created_at, 'localtime') >= ?1
               AND date(created_at, 'localtime') <= ?2",
        )
        .map_err(|e| e.to_string())?;

    let rows: Vec<(String, i32)> = stmt
        .query_map(params![start_str, end_str], |row| Ok((row.get(0)?, row.get(1)?)))
        .map_err(|e| e.to_string())?
        .filter_map(|r| r.ok())
        .collect();

    let mut total_seconds = 0i32;
    let mut focus_seconds = 0i32;
    for (category, dur) in &rows {
        total_seconds += dur;
        if FOCUS_CATEGORIES.contains(&category.as_str()) {
            focus_seconds += dur;
        }
    }

    let change_pct = if current_total_seconds > 0 && total_seconds > 0 {
        ((current_total_seconds - total_seconds) as f64 / total_seconds as f64 * 1000.0).round() / 10.0
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
        "focus_seconds": focus_seconds,
        "focus_hours": round_hours(focus_seconds),
        "activity_count": rows.len(),
        "change_pct": change_pct,
    }))
}

fn build_diverse_activity_samples(
    all: &[ActivitySample],
    longest: &[LongSessionRow],
) -> Vec<ActivitySample> {
    let mut picked: Vec<ActivitySample> = Vec::new();
    let mut seen_dates: HashMap<String, bool> = HashMap::new();
    let mut seen_keys: HashMap<String, bool> = HashMap::new();

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

