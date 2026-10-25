use crate::agent_pure::{parse_analysis, resolve_persisted_category};
use crate::focus_semantics::{allowed_categories_prompt, canonical_ticket_value, LocalDateWindow};
use crate::vision_model::{CONFIG_VISION_MODEL_ID, LLAMA_CHAT_MODEL_ID, VISION_STATUS_LABEL};
use base64::{engine::general_purpose::STANDARD as BASE64, Engine as _};
use chrono::{Datelike, Local};
use rusqlite::{params, Connection};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::Duration;
use tauri::{Emitter, State};

pub type AgentState = Mutex<Option<FlowSightAgent>>;

#[derive(Serialize, Deserialize, Clone, Debug, Default)]
pub struct AgentConfig {
    #[serde(rename = "devName")]
    pub dev_name: Option<String>,
    #[serde(rename = "captureInterval")]
    pub capture_interval: Option<u64>,
    #[serde(rename = "visionModel")]
    pub vision_model: Option<String>,
    /// `None` or `-1` => automatic GPU layer ladder on local llama-server.
    /// `Some(n)` for `n >= 0` => fixed `--n-gpu-layers` (manual / power user).
    #[serde(rename = "gpuLayers")]
    pub gpu_layers: Option<i32>,
    #[serde(rename = "dailyGoalHours")]
    pub daily_goal_hours: Option<f64>,
}

pub struct FlowSightAgent {
    pub config: AgentConfig,
    pub is_running: bool,
    pub tracking_clock: Option<crate::tracking_clock::TrackingClock>,
    pub reports_sent: u32,
    pub db_path: PathBuf,
}

impl FlowSightAgent {
    pub fn new(app_handle: tauri::AppHandle) -> Self {
        let db_path = crate::paths::db_path().unwrap_or_else(|e| {
            log::error!(
                "[Agent] paths::db_path unavailable ({}); using cwd fallback.",
                e
            );
            dirs::data_local_dir()
                .unwrap_or_else(|| PathBuf::from("."))
                .join("FlowSight")
                .join("dev-agent.db")
        });

        if let Some(parent) = db_path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }

        let mut agent = Self {
            config: AgentConfig {
                dev_name: None,
                capture_interval: Some(60000),
                vision_model: Some(CONFIG_VISION_MODEL_ID.to_string()),
                // -1 = automatic tier probing (maximum compatibility + strongest profile that survives).
                gpu_layers: Some(-1),
                daily_goal_hours: Some(6.0),
            },
            is_running: false,
            tracking_clock: None,
            reports_sent: 0,
            db_path,
        };

        agent.init_db();
        if let Ok(conn) = Connection::open(&agent.db_path) {
            agent.tracking_clock = load_tracking_clock(&conn).ok();
        }
        agent.load_config();
        if crate::paths::isolated_pilot_dir().is_some() {
            return agent;
        }
        crate::focus_alerts::set_enabled(crate::desktop_presence::focus_alerts_enabled());
        if let Err(error) = crate::privacy::enforce_local_retention(&agent.db_path) {
            log::warn!("[Privacy] Local retention enforcement failed: {error}");
        }
        crate::privacy::start_local_retention_thread(agent.db_path.clone());
        let capture_interval = agent
            .config
            .capture_interval
            .unwrap_or(60_000)
            .clamp(5_000, 300_000);
        agent.config.capture_interval = Some(capture_interval);

        // Start Background Sync (10m interval)
        crate::sync::start_sync_thread(agent.db_path.clone());
        crate::online_leagues::start_sync_thread(agent.db_path.clone(), app_handle.clone());
        // Proactive Supabase JWT refresh (~every 2m when near expiry)
        crate::sync::start_token_refresh_thread(agent.db_path.clone());
        // Opt-in pseudonymous analytics sync (~every 6h when consented)
        crate::anonymous_analytics::start_analytics_sync_thread(agent.db_path.clone());
        // Level 0/1 privacy-first telemetry pipeline (foreground/UIA event
        // capture + configured-interval action-log review + periodic vision snapshot +
        // action-triggered capture). Data collection itself stays OFF until
        // `start_monitoring` toggles it on.
        crate::telemetry::start(app_handle, agent.db_path.clone(), capture_interval);

        agent
    }

    fn init_db(&self) {
        match Connection::open(&self.db_path) {
            Ok(conn) => {
                if let Err(e) = conn.execute_batch(
                    "CREATE TABLE IF NOT EXISTS config (key TEXT PRIMARY KEY, value TEXT);
                     CREATE TABLE IF NOT EXISTS reports (
                        id INTEGER PRIMARY KEY,
                        description TEXT,
                        activity_type TEXT,
                        synced INTEGER DEFAULT 0,
                        created_at TEXT DEFAULT CURRENT_TIMESTAMP
                     );",
                ) {
                    log::error!(
                        "[Agent] SQLite schema/bootstrap failed {:?}: {}",
                        self.db_path,
                        e
                    );
                }
                let _ = conn.execute("ALTER TABLE reports ADD COLUMN jira_ticket_id TEXT", []);
                let _ = conn.execute(
                    "ALTER TABLE reports ADD COLUMN duration_seconds INTEGER DEFAULT 30",
                    [],
                );
                // Additive, local-only provenance for the canonical focus detector.
                // Existing databases remain valid; old rows simply have NULL context.
                let _ = conn.execute("ALTER TABLE reports ADD COLUMN active_app TEXT", []);
                let _ = conn.execute("ALTER TABLE reports ADD COLUMN window_title TEXT", []);
                let _ = conn.execute("ALTER TABLE reports ADD COLUMN capture_source TEXT", []);
                let _ = conn.execute("ALTER TABLE reports ADD COLUMN theme_hint TEXT", []);
                if let Err(error) = crate::task_progress::ensure_schema(&conn) {
                    log::error!("[TaskProgress] SQLite progress schema failed: {error}");
                }
                if let Err(error) = conn.execute_batch(crate::sync_pure::PENDING_REPORT_INDEX_SQL) {
                    log::warn!("[Agent] Could not index pending reports: {error}");
                }
                if let Err(error) = crate::privacy::ensure_schema(&conn) {
                    log::error!("[Privacy] SQLite privacy schema failed: {error}");
                }
                if let Err(error) = crate::sync::ensure_capture_schema(&conn) {
                    log::error!("[Sync] Capture ownership schema failed: {error}");
                }
            }
            Err(e) => log::error!(
                "[Agent] SQLite open failed {:?} (init_db): {}",
                self.db_path,
                e
            ),
        }
    }

    fn load_config(&mut self) {
        let Ok(conn) = Connection::open(&self.db_path) else {
            log::warn!(
                "[Agent] load_config: cannot open {:?}; using defaults",
                self.db_path
            );
            return;
        };

        for (key, field) in [
            ("dev_name", &mut self.config.dev_name),
            ("vision_model", &mut self.config.vision_model),
        ] {
            if let Ok(val) = conn.query_row::<String, _, _>(
                "SELECT value FROM config WHERE key = ?",
                [key],
                |r| r.get(0),
            ) {
                *field = Some(val);
            }
        }

        if let Ok(val) = conn.query_row::<String, _, _>(
            "SELECT value FROM config WHERE key = 'gpu_layers'",
            [],
            |r| r.get(0),
        ) {
            if let Ok(parsed) = val.parse::<i32>() {
                self.config.gpu_layers = Some(parsed);
            }
        }

        if let Ok(val) = conn.query_row::<String, _, _>(
            "SELECT value FROM config WHERE key = 'daily_goal_hours'",
            [],
            |r| r.get(0),
        ) {
            if let Ok(parsed) = val.parse::<f64>() {
                self.config.daily_goal_hours = Some(parsed.clamp(0.0, 24.0));
            }
        }
    }

    fn save_config(&self) {
        let Ok(conn) = Connection::open(&self.db_path) else {
            log::warn!("[Agent] save_config: cannot open {:?}", self.db_path);
            return;
        };

        for (key, val) in [
            ("dev_name", &self.config.dev_name),
            ("vision_model", &self.config.vision_model),
        ] {
            if let Some(v) = val {
                let _ = conn.execute(
                    "INSERT OR REPLACE INTO config (key, value) VALUES (?, ?)",
                    params![key, v],
                );
            }
        }

        if let Some(layers) = self.config.gpu_layers {
            let _ = conn.execute(
                "INSERT OR REPLACE INTO config (key, value) VALUES (?, ?)",
                params!["gpu_layers", layers.to_string()],
            );
        }

        if let Some(hours) = self.config.daily_goal_hours {
            let _ = conn.execute(
                "INSERT OR REPLACE INTO config (key, value) VALUES (?, ?)",
                params!["daily_goal_hours", hours.to_string()],
            );
        }
    }
}

// Capture and analyze screen
// (Logic moved to Frontend for cross-platform support)

fn capture_screen(
    db_path: &Path,
) -> Result<
    (
        String,
        crate::context::SystemContext,
        crate::task_progress::VisualFingerprint,
    ),
    String,
> {
    use screenshots::Screen;

    let active = active_win_pos_rs::get_active_window().map_err(|_| {
        "The active window could not be identified; capture was skipped.".to_string()
    })?;
    if crate::privacy::application_is_excluded(db_path, Some(&active.app_name)) {
        return Err("Capture skipped for an excluded application".to_string());
    }

    let x = active.position.x.round() as i32;
    let y = active.position.y.round() as i32;
    let width = active.position.width.round().max(1.0) as u32;
    let height = active.position.height.round().max(1.0) as u32;
    let center_x = x.saturating_add((width / 2) as i32);
    let center_y = y.saturating_add((height / 2) as i32);
    let screen = Screen::from_point(center_x, center_y).map_err(|e| e.to_string())?;
    let display_right = screen
        .display_info
        .x
        .saturating_add(screen.display_info.width as i32);
    let display_bottom = screen
        .display_info
        .y
        .saturating_add(screen.display_info.height as i32);
    let left = x.max(screen.display_info.x);
    let top = y.max(screen.display_info.y);
    let right = x.saturating_add(width as i32).min(display_right);
    let bottom = y.saturating_add(height as i32).min(display_bottom);
    if right <= left || bottom <= top {
        return Err("The active window is outside the captured display.".to_string());
    }
    let relative_x = left - screen.display_info.x;
    let relative_y = top - screen.display_info.y;
    let captured = screen
        .capture_area(
            relative_x,
            relative_y,
            (right - left) as u32,
            (bottom - top) as u32,
        )
        .map_err(|e| e.to_string())?;

    // Convert to DynamicImage
    let (width, height) = captured.dimensions();
    let img = image::DynamicImage::ImageRgba8(
        image::RgbaImage::from_raw(width, height, captured.into_raw())
            .ok_or("Failed to create image")?,
    );

    let img = img.resize(960, 540, image::imageops::FilterType::Lanczos3);
    let fingerprint = crate::task_progress::VisualFingerprint::from_image(&img);

    let mut png = Vec::new();
    img.write_to(&mut std::io::Cursor::new(&mut png), image::ImageFormat::Png)
        .map_err(|e| e.to_string())?;

    // println!("[Agent] Captured screenshot size: {} bytes", png.len());

    // The screenshot is deliberately kept in memory only. Persisting even an
    // encrypted debug copy would collect more data than the product needs.
    let file_name = crate::context::file_hint_from_window_title(&active.title);
    let context = crate::context::SystemContext {
        app_name: Some(active.app_name),
        window_title: Some(active.title),
        file_name,
        file_path: None,
    };
    Ok((BASE64.encode(&png), context, fingerprint))
}

// ============== TAURI COMMANDS ==============

/// Comprueba que SQLite puede **escribir** en `dev-agent.db` (CFA / solo lectura / disco lleno).
fn probe_sqlite_database_rw() -> Result<(), String> {
    let db_path = crate::paths::db_path()?;
    let conn =
        Connection::open(&db_path).map_err(|e| format!("SQLite cannot open {:?}: {e}", db_path))?;
    conn.execute_batch(
        "BEGIN IMMEDIATE;
         CREATE TEMP TABLE IF NOT EXISTS _flowsight_io_probe (x INTEGER);
         INSERT INTO _flowsight_io_probe VALUES (1);
         COMMIT;",
    )
    .map_err(|e| {
        format!(
            "SQLite cannot write to {:?}. On Windows 11, verify Controlled Folder Access / Defender is not blocking this app from modifying its data folder ({e})",
            db_path
        )
    })?;
    Ok(())
}

#[tauri::command]
pub fn initialize_agent(
    app_handle: tauri::AppHandle,
    state: State<'_, AgentState>,
) -> Result<bool, String> {
    let mut g = state.lock().unwrap();
    if g.is_some() {
        return Ok(true);
    }

    crate::paths::verify_app_dir_filesystem_writable()?;
    probe_sqlite_database_rw()?;

    // Remove legacy transient screenshots from older releases. New captures
    // are memory-only, so no new files are created in this directory.
    match crate::paths::prune_screenshots_tmp_older_than(Duration::ZERO) {
        Ok(n) if n > 0 => {
            log::info!("[Privacy] removed {n} legacy temporary screenshot(s)");
        }
        Err(e) => log::warn!("[FlowSight] screenshots_tmp retention prune: {e}"),
        _ => {}
    }

    *g = Some(FlowSightAgent::new(app_handle.clone()));
    // Persist independently of renderer timers, including a hidden/tray window.
    std::thread::spawn(move || loop {
        std::thread::sleep(Duration::from_secs(15));
        use tauri::Manager;
        let _ = get_tracking_clock(app_handle.state::<AgentState>());
    });
    Ok(true)
}

#[tauri::command]
pub fn get_config(state: State<'_, AgentState>) -> Result<AgentConfig, String> {
    Ok(state
        .lock()
        .unwrap()
        .as_ref()
        .map(|a| a.config.clone())
        .unwrap_or_default())
}

#[tauri::command]
pub fn update_config(state: State<'_, AgentState>, patch: AgentConfig) -> Result<bool, String> {
    if let Some(agent) = state.lock().unwrap().as_mut() {
        let c = &mut agent.config;
        if patch.dev_name.is_some() {
            c.dev_name = patch.dev_name;
        }
        if let Some(interval) = patch.capture_interval {
            let interval = interval.clamp(5_000, 300_000);
            c.capture_interval = Some(interval);
            crate::telemetry::set_capture_interval(interval);
        }
        if patch.vision_model.is_some() {
            c.vision_model = patch.vision_model;
        }
        // callers (renderer) omit `gpuLayers`; full replace here used to wipe auto/manual choice
        if patch.gpu_layers.is_some() {
            c.gpu_layers = patch.gpu_layers;
        }
        if patch.daily_goal_hours.is_some() {
            c.daily_goal_hours = patch.daily_goal_hours.map(|h| h.clamp(0.0, 24.0));
        }
        agent.save_config();
    }
    Ok(true)
}

fn load_tracking_clock(conn: &Connection) -> Result<crate::tracking_clock::TrackingClock, String> {
    let today = Local::now().format("%Y-%m-%d").to_string();
    let observed = load_daily_totals(conn, &today, &today)?
        .get(&today)
        .copied()
        .unwrap_or(0)
        .max(0) as u64;
    crate::tracking_clock::TrackingClock::load(conn, observed)
}

#[tauri::command]
pub fn get_tracking_clock(
    state: State<'_, AgentState>,
) -> Result<crate::tracking_clock::TrackingClockSnapshot, String> {
    let mut guard = state.lock().map_err(|e| e.to_string())?;
    let agent = guard.as_mut().ok_or("Agent not initialized")?;
    snapshot_tracking_clock(agent)
}

fn snapshot_tracking_clock(
    agent: &mut FlowSightAgent,
) -> Result<crate::tracking_clock::TrackingClockSnapshot, String> {
    let conn = Connection::open(&agent.db_path).map_err(|e| e.to_string())?;
    if agent.tracking_clock.is_none() {
        agent.tracking_clock = Some(load_tracking_clock(&conn)?);
    }
    agent
        .tracking_clock
        .as_mut()
        .unwrap()
        .snapshot(&conn, agent.is_running)
}

#[tauri::command]
pub fn get_daily_flow_progress(
    state: State<'_, AgentState>,
) -> Result<crate::daily_flow::DailyFlowProgress, String> {
    let mut guard = state.lock().map_err(|e| e.to_string())?;
    let agent = guard.as_mut().ok_or("Agent not initialized")?;
    let snapshot = snapshot_tracking_clock(agent)?;
    let conn = Connection::open(&agent.db_path).map_err(|e| e.to_string())?;
    crate::daily_flow::load_progress(&conn, snapshot)
}

#[tauri::command]
pub fn get_status(state: State<'_, AgentState>) -> Result<serde_json::Value, String> {
    let agent = state.lock().unwrap();
    Ok(if let Some(a) = agent.as_ref() {
        serde_json::json!({
            "isRunning": a.is_running,
            "reportsSent": a.reports_sent
        })
    } else {
        serde_json::json!({"isRunning": false, "reportsSent": 0})
    })
}

#[tauri::command]
pub fn start_monitoring(state: State<'_, AgentState>) -> Result<bool, String> {
    let db_path = crate::paths::db_path()?;
    crate::privacy::require_monitoring_acknowledgement(&db_path)?;
    crate::focus_alerts::start_monitoring(&db_path);
    if let Some(a) = state.lock().unwrap().as_mut() {
        let conn = Connection::open(&a.db_path).map_err(|e| e.to_string())?;
        if a.tracking_clock.is_none() {
            a.tracking_clock = Some(load_tracking_clock(&conn)?);
        }
        a.tracking_clock.as_mut().unwrap().snapshot(&conn, true)?;
        a.is_running = true;
    }
    crate::telemetry::set_running(true);
    Ok(true)
}

#[tauri::command]
pub fn stop_monitoring(state: State<'_, AgentState>) -> Result<bool, String> {
    crate::focus_alerts::stop_monitoring();
    if let Some(a) = state.lock().unwrap().as_mut() {
        if let Some(clock) = a.tracking_clock.as_mut() {
            let conn = Connection::open(&a.db_path).map_err(|e| e.to_string())?;
            clock.snapshot(&conn, false)?;
        }
        a.is_running = false;
    }
    crate::telemetry::set_running(false);
    Ok(true)
}

/// Lets the frontend tell the backend-driven telemetry cycle (`telemetry::aggregator`
/// / `telemetry::action_capture`) what the user is currently working on (selected Jira
/// ticket / manual task label).
#[tauri::command]
pub fn set_task_context(
    user_task: Option<String>,
    jira_ticket: Option<String>,
) -> Result<bool, String> {
    crate::telemetry::set_task_context(user_task, jira_ticket);
    Ok(true)
}

#[derive(Serialize, Deserialize, Debug)]
pub struct DayHistoryEntry {
    pub time: String,
    pub description: String,
    pub category: String,
    pub ticket: Option<String>,
    pub duration_seconds: i32,
    pub app_name: Option<String>,
    pub window_title: Option<String>,
    pub capture_source: Option<String>,
    pub theme_hint: Option<String>,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct CategoryBreakdown {
    pub category: String,
    pub total_seconds: i32,
    pub count: i32,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct TicketBreakdown {
    pub ticket: String,
    pub total_seconds: i32,
    pub count: i32,
}

#[derive(Serialize, Debug)]
pub struct TodayHistory {
    pub entries: Vec<DayHistoryEntry>,
    pub total_seconds: i32,
    pub tracking: Option<crate::tracking_clock::TrackingClockSnapshot>,
    pub category_breakdown: Vec<CategoryBreakdown>,
    pub ticket_breakdown: Vec<TicketBreakdown>,
    pub date: String,
    pub focus: crate::focus_semantics::FocusSummary,
}

fn load_day_history_entries(conn: &Connection, day: &str) -> Result<Vec<DayHistoryEntry>, String> {
    let window = LocalDateWindow::parse(day, day)?;
    let mut stmt = conn
        .prepare(
            "SELECT datetime(created_at, 'localtime'), description, activity_type, jira_ticket_id, duration_seconds,
                    active_app, window_title, capture_source, theme_hint
             FROM reports
             WHERE date(created_at, 'localtime') >= ?1
               AND date(created_at, 'localtime') <= date(?1, '+1 day')
             ORDER BY datetime(created_at, 'localtime') ASC",
        )
        .map_err(|error| error.to_string())?;
    let rows = stmt
        .query_map(params![day], |row| {
            Ok((
                row.get::<_, String>(0).unwrap_or_default(),
                row.get::<_, String>(1).unwrap_or_default(),
                row.get::<_, String>(2).unwrap_or_default(),
                row.get::<_, Option<String>>(3).unwrap_or(None),
                row.get::<_, i64>(4).unwrap_or(0).max(0),
                row.get::<_, Option<String>>(5).unwrap_or(None),
                row.get::<_, Option<String>>(6).unwrap_or(None),
                row.get::<_, Option<String>>(7).unwrap_or(None),
                row.get::<_, Option<String>>(8).unwrap_or(None),
            ))
        })
        .map_err(|error| error.to_string())?
        .filter_map(Result::ok);

    let mut entries = Vec::new();
    for (time, description, raw_category, ticket, duration, app, title, source, theme) in rows {
        let category = resolve_persisted_category(&raw_category);
        let ticket = canonical_ticket_value(ticket.as_deref());
        let slices = window.slices_for_observation(&time, duration);
        if slices.is_empty() {
            // Zero-duration action reviews remain useful narrative evidence,
            // but never add tracked time or enter the focus detector.
            if duration == 0 && window.contains_local_timestamp(&time) {
                entries.push(DayHistoryEntry {
                    time,
                    description,
                    category,
                    ticket,
                    duration_seconds: 0,
                    app_name: app,
                    window_title: title,
                    capture_source: source,
                    theme_hint: theme,
                });
            }
            continue;
        }
        for slice in slices {
            let slice_end = slice.start + chrono::Duration::seconds(slice.duration_seconds);
            entries.push(DayHistoryEntry {
                time: slice_end.format("%Y-%m-%d %H:%M:%S").to_string(),
                description: description.clone(),
                category: category.clone(),
                ticket: ticket.clone(),
                duration_seconds: i32::try_from(slice.duration_seconds).unwrap_or(i32::MAX),
                app_name: app.clone(),
                window_title: title.clone(),
                capture_source: source.clone(),
                theme_hint: theme.clone(),
            });
        }
    }
    entries.sort_by(|left, right| right.time.cmp(&left.time));
    Ok(entries)
}

pub(crate) fn load_daily_totals(
    conn: &Connection,
    period_start: &str,
    period_end: &str,
) -> Result<std::collections::HashMap<String, i32>, String> {
    let window = LocalDateWindow::parse(period_start, period_end)?;
    let mut stmt = conn
        .prepare(
            "SELECT datetime(created_at, 'localtime'), duration_seconds
             FROM reports
             WHERE date(created_at, 'localtime') >= ?1
               AND date(created_at, 'localtime') <= date(?2, '+1 day')
             ORDER BY datetime(created_at, 'localtime') ASC",
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
    let mut totals = std::collections::HashMap::new();
    for (observed_end, duration) in rows.filter_map(Result::ok) {
        for slice in window.slices_for_observation(&observed_end, duration) {
            let date = slice.start.format("%Y-%m-%d").to_string();
            let seconds = i32::try_from(slice.duration_seconds).unwrap_or(i32::MAX);
            totals
                .entry(date)
                .and_modify(|total: &mut i32| *total = total.saturating_add(seconds))
                .or_insert(seconds);
        }
    }
    Ok(totals)
}

#[tauri::command]
pub fn get_today_history(state: State<'_, AgentState>) -> Result<TodayHistory, String> {
    let mut agent = state.lock().unwrap();
    let agent = agent.as_mut().ok_or("Agent not initialized")?;

    let conn = Connection::open(&agent.db_path).map_err(|e| e.to_string())?;
    let tracking = agent
        .tracking_clock
        .as_mut()
        .map(|clock| clock.snapshot(&conn, agent.is_running))
        .transpose()?;
    // Calendar 'today' in local TZ must use UTC→local conversion: `created_at`
    // defaults to CURRENT_TIMESTAMP (UTC). Comparing plain `date(created_at)`
    // to `date('now','localtime')` used mismatched halves and often returned zero rows.
    let today = Local::now().format("%Y-%m-%d").to_string();

    let entries = load_day_history_entries(&conn, &today)?;

    // Calculate total
    let total_seconds: i32 = entries.iter().map(|e| e.duration_seconds).sum();

    // Category breakdown
    let mut cat_map: std::collections::HashMap<String, (i32, i32)> =
        std::collections::HashMap::new();
    for e in &entries {
        let entry = cat_map.entry(e.category.clone()).or_insert((0, 0));
        entry.0 += e.duration_seconds;
        entry.1 += 1;
    }
    let mut category_breakdown: Vec<CategoryBreakdown> = cat_map
        .into_iter()
        .map(|(category, (total_seconds, count))| CategoryBreakdown {
            category,
            total_seconds,
            count,
        })
        .collect();
    category_breakdown.sort_by(|left, right| {
        right
            .total_seconds
            .cmp(&left.total_seconds)
            .then_with(|| left.category.cmp(&right.category))
    });

    // Ticket breakdown
    let mut ticket_map: std::collections::HashMap<String, (i32, i32)> =
        std::collections::HashMap::new();
    for e in &entries {
        if let Some(ref ticket) = e.ticket {
            let entry = ticket_map.entry(ticket.clone()).or_insert((0, 0));
            entry.0 += e.duration_seconds;
            entry.1 += 1;
        }
    }
    let mut ticket_breakdown: Vec<TicketBreakdown> = ticket_map
        .into_iter()
        .map(|(ticket, (total_seconds, count))| TicketBreakdown {
            ticket,
            total_seconds,
            count,
        })
        .collect();
    ticket_breakdown.sort_by(|left, right| {
        right
            .total_seconds
            .cmp(&left.total_seconds)
            .then_with(|| left.ticket.cmp(&right.ticket))
    });

    let focus = crate::focus_semantics::summarize_from_db(&conn, &today, &today)?;

    Ok(TodayHistory {
        entries,
        total_seconds,
        tracking,
        category_breakdown,
        ticket_breakdown,
        date: today,
        focus,
    })
}

#[derive(Serialize, Deserialize, Debug)]
pub struct DayActivity {
    pub date: String,
    pub weekday: String,
    pub total_seconds: i32,
    pub has_activity: bool,
    pub is_today: bool,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct WeekSummary {
    pub days: Vec<DayActivity>,
    pub yesterday_seconds: i32,
}

#[tauri::command]
pub fn get_week_summary(state: State<'_, AgentState>) -> Result<WeekSummary, String> {
    let agent = state.lock().unwrap();
    let agent = agent.as_ref().ok_or("Agent not initialized")?;

    let conn = Connection::open(&agent.db_path).map_err(|e| e.to_string())?;
    let today = Local::now().date_naive();
    let weekday = today.weekday().num_days_from_monday();
    let week_start = today - chrono::Duration::days(weekday as i64);
    let week_end = week_start + chrono::Duration::days(6);
    let yesterday = today - chrono::Duration::days(1);

    let start_str = week_start.format("%Y-%m-%d").to_string();
    let end_str = week_end.format("%Y-%m-%d").to_string();
    let day_totals = load_daily_totals(&conn, &start_str, &end_str)?;

    let weekday_labels = ["M", "T", "W", "T", "F", "S", "S"];
    let mut days = Vec::with_capacity(7);
    for offset in 0..7 {
        let day = week_start + chrono::Duration::days(offset);
        let date_str = day.format("%Y-%m-%d").to_string();
        let total = day_totals.get(&date_str).copied().unwrap_or(0);
        days.push(DayActivity {
            date: date_str,
            weekday: weekday_labels[offset as usize].to_string(),
            total_seconds: total,
            has_activity: total > 0,
            is_today: day == today,
        });
    }

    let yesterday_str = yesterday.format("%Y-%m-%d").to_string();
    let yesterday_seconds = day_totals.get(&yesterday_str).copied().unwrap_or(0);

    Ok(WeekSummary {
        days,
        yesterday_seconds,
    })
}

// Health check against nuestro llama-server local (NO es ollama; el nombre se
// mantuvo en el tauri command hist\u00f3ricamente pero el endpoint es de llama.cpp).
//
// Timeout generoso: en equipos lentos o con antivirus el primer /health puede tardar
// mientras el modelo termina de cargar; 1s provocaba falsos "offline" intermitentes.
const LOCAL_HEALTH_HTTP_TIMEOUT_SECS: u64 = 12;

/// Quick binary health check reused by diagnostics and automated tier probing.
fn local_server_health_ok() -> bool {
    let Some(health_url) = crate::llama_port::managed_health_url() else {
        return false;
    };
    let Ok(client) = reqwest::blocking::Client::builder()
        .timeout(std::time::Duration::from_secs(
            LOCAL_HEALTH_HTTP_TIMEOUT_SECS,
        ))
        .build()
    else {
        return false;
    };
    client
        .get(&health_url)
        .send()
        .ok()
        .map(|r| r.status().is_success())
        .unwrap_or(false)
}

// Async so Tauri dispatches it off the main/UI thread: the `reqwest::blocking`
// call below opens a real TCP socket even for localhost, which is exactly what
// makes Windows inject any registered Winsock LSP (VPN/AV network proxies —
// see crash_guard.rs) into this process. Keeping that off the main thread
// means a crash_guard-contained LSP crash here only takes down this
// background thread, never the window's message loop.
#[tauri::command]
pub async fn check_local_server() -> Result<serde_json::Value, String> {
    tauri::async_runtime::spawn_blocking(check_local_server_blocking)
        .await
        .map_err(|e| format!("Task join error: {}", e))?
}

fn check_local_server_blocking() -> Result<serde_json::Value, String> {
    let client = reqwest::blocking::Client::builder()
        .timeout(std::time::Duration::from_secs(
            LOCAL_HEALTH_HTTP_TIMEOUT_SECS,
        ))
        .build()
        .map_err(|e| e.to_string())?;

    let Some(health_url) = crate::llama_port::managed_health_url() else {
        return Ok(serde_json::json!({
            "online": false,
            "installed": true,
            "error": "No managed llama-server port (start Local AI first)."
        }));
    };

    match client.get(&health_url).send() {
        Ok(r) if r.status().is_success() => Ok(serde_json::json!({
            "online": true,
            "installed": true,
            "models": [VISION_STATUS_LABEL],
            "hasVisionModel": true,
            "localServerPort": crate::llama_port::current_managed_listen_port(),
        })),
        Ok(r) => Ok(serde_json::json!({
            "online": false,
            "installed": true,
            "error": format!("Local server status: {}", r.status())
        })),
        Err(_) => Ok(serde_json::json!({
            "online": false,
            "installed": true
        })),
    }
}

// LLAMA SERVER COMMANDS

static SERVER_PROCESS: Mutex<Option<std::process::Child>> = Mutex::new(None);

/// Puertos nuevos ante `EADDRINUSE`/fallo rápido de escucha tras TOCTOU o TIME_WAIT.
const LLAMA_LISTEN_PORT_SPAWN_ATTEMPTS: u8 = 8;

fn clamp_llama_gpu_layers(n: i32) -> i32 {
    n.clamp(0, 16_384)
}

/// Probe a bounded set of GPU options, then CPU. The old nested ladder tried 25
/// combinations and could leave the setup screen waiting for over 20 minutes.
const AUTO_START_CANDIDATES: &[(Option<&str>, i32)] = &[
    (None, 56),
    (Some("0"), 56),
    (Some("1"), 56),
    (None, 24),
    (None, 0),
];
/// Per-attempt budget while the weights load (slow disks / AV can dominate here).
const AUTO_TIER_HEALTH_WAIT_SECS: u64 = 56;

#[derive(Clone, Copy, Debug)]
enum GpuServeMode {
    Automatic,
    Manual(i32),
}

fn gpu_serve_mode(state: &State<AgentState>) -> GpuServeMode {
    let guard = state.lock().unwrap();
    let raw = match guard.as_ref() {
        None => return GpuServeMode::Automatic,
        Some(a) => a.config.gpu_layers,
    };
    match raw {
        None | Some(-1) => GpuServeMode::Automatic,
        Some(n) if n >= 0 => GpuServeMode::Manual(clamp_llama_gpu_layers(n)),
        Some(_) => GpuServeMode::Automatic,
    }
}

/// Returns true once `/health` succeeds. If the managed child exits, clears it and stops early.
fn wait_for_managed_health_secs(max_secs: u64) -> bool {
    for _ in 0..max_secs {
        if local_server_health_ok() {
            return true;
        }

        let still_running = {
            let mut g = SERVER_PROCESS.lock().unwrap();
            match g.as_mut() {
                Some(ch) => match ch.try_wait() {
                    Ok(Some(_)) => {
                        g.take();
                        false
                    }
                    Ok(None) => true,
                    Err(_) => {
                        let _ = g.take();
                        false
                    }
                },
                None => false,
            }
        };

        if !still_running {
            return false;
        }

        std::thread::sleep(std::time::Duration::from_secs(1));
    }
    false
}

/// Starts the managed llama-server if needed and waits until `/health` responds.
pub fn ensure_local_llm_ready(
    app: tauri::AppHandle,
    state: State<'_, AgentState>,
) -> Result<(), String> {
    if local_server_health_ok() {
        return Ok(());
    }

    log::info!("[LocalReport] Local AI offline — starting server for insight generation…");
    let result = start_server(app, state)?;
    let status = result["status"].as_str().unwrap_or("");
    if status != "started" && status != "already_running" {
        return Err(format!("Could not start local AI: {}", result));
    }

    const REPORT_READY_WAIT_SECS: u64 = 180;
    if wait_for_managed_health_secs(REPORT_READY_WAIT_SECS) {
        log::info!("[LocalReport] Local AI ready for report generation");
        return Ok(());
    }

    Err(format!(
        "Local AI did not respond within {}s. Start monitoring from Today and retry.",
        REPORT_READY_WAIT_SECS
    ))
}

fn read_server_log_tail_chars(max_chars: usize) -> String {
    let Ok(path) = crate::paths::server_log_path() else {
        return String::new();
    };
    let Ok(s) = std::fs::read_to_string(&path) else {
        return String::new();
    };
    if s.len() <= max_chars {
        s
    } else {
        s[s.len() - max_chars..].to_string()
    }
}

/// Estado del proceso hijo que FlowSight lanzó (no confundir con un llama-server huérfano).
#[tauri::command]
pub fn llama_managed_process_status() -> Result<serde_json::Value, String> {
    let mut guard = SERVER_PROCESS.lock().unwrap();
    match guard.as_mut() {
        None => Ok(serde_json::json!({
            "managed": false,
            "alive": null
        })),
        Some(child) => match child.try_wait() {
            Ok(Some(status)) => {
                let code = status.code();
                guard.take();
                Ok(serde_json::json!({
                    "managed": true,
                    "alive": false,
                    "exitCode": code
                }))
            }
            Ok(None) => Ok(serde_json::json!({
                "managed": true,
                "alive": true
            })),
            Err(e) => Err(e.to_string()),
        },
    }
}

#[tauri::command]
pub fn llama_server_log_tail(max_chars: Option<usize>) -> Result<String, String> {
    let n = max_chars.unwrap_or(1_200).max(200);
    Ok(read_server_log_tail_chars(n))
}

#[allow(clippy::too_many_arguments)] // mirrors llama-server's independent runtime knobs
fn configure_llama_command(
    bin_path: &Path,
    model_path: &Path,
    mmproj_path: &Path,
    log_path: &Path,
    listen_port: u16,
    gpu_layers: i32,
    vulkan_visible_device_index: Option<&str>,
    redirect_log_to_file: bool,
    #[cfg_attr(not(windows), allow(unused_variables))] creation_flags: Option<u32>,
) -> Result<std::process::Command, String> {
    #[cfg(windows)]
    use std::os::windows::process::CommandExt;
    use std::process::Command;

    let n_gpu_layers = clamp_llama_gpu_layers(gpu_layers);

    let mut cmd = Command::new(bin_path);
    // Evita heredar stdin inválido tras FreeConsole en el proceso padre (release Windows).
    cmd.stdin(std::process::Stdio::null());
    cmd.arg("-m")
        .arg(model_path)
        .arg("--mmproj")
        .arg(mmproj_path)
        .arg("--alias")
        .arg(LLAMA_CHAT_MODEL_ID)
        // Keep the local classifier in content-only mode so an unexpected
        // reasoning segment cannot consume the short response budget.
        .arg("--reasoning-budget")
        .arg("0")
        .arg("--chat-template-kwargs")
        .arg(r#"{"enable_thinking":false}"#)
        .arg("--host")
        .arg("127.0.0.1")
        .arg("--port")
        .arg(listen_port.to_string())
        .arg("--ctx-size")
        .arg("8192")
        .arg("--parallel")
        .arg("2")
        .arg("--threads")
        .arg("2")
        .arg("--n-gpu-layers")
        .arg(n_gpu_layers.to_string());

    if let Some(idx) = vulkan_visible_device_index {
        cmd.env("GGML_VK_VISIBLE_DEVICES", idx);
    }

    // Con pesos sólo en CPU, los builds con Vulkan pueden igual inicializar la API y fallar
    // (p. ej. `vkCreateFence: Invalid device`) antes de que `/health` responda.
    // GGML + llama.cpp respetan estas variables sin pasar flags extra por CLI.
    if n_gpu_layers == 0 {
        cmd.env("GGML_DISABLE_VULKAN", "1");
        cmd.env("LLAMA_ARG_DEVICE", "none");
    }

    // CWD y PATH apuntan a la carpeta del binario. Algunos backends de
    // llama.cpp cargan DLLs dinámicamente por nombre, y en instalaciones
    // Windows no siempre basta con que estén junto al exe.
    if let Some(parent) = bin_path.parent() {
        cmd.current_dir(parent);
        if let Some(existing_path) = std::env::var_os("PATH") {
            let mut paths = vec![parent.to_path_buf()];
            paths.extend(std::env::split_paths(&existing_path));
            if let Ok(joined_path) = std::env::join_paths(paths) {
                cmd.env("PATH", joined_path);
            }
        } else {
            cmd.env("PATH", parent);
        }
    }

    #[cfg(windows)]
    if let Some(flags) = creation_flags {
        cmd.creation_flags(flags);
    }

    if redirect_log_to_file {
        if let Ok(file) = std::fs::File::create(log_path) {
            if let Ok(file_err) = file.try_clone() {
                cmd.stdout(std::process::Stdio::from(file));
                cmd.stderr(std::process::Stdio::from(file_err));
            }
        }
    }

    Ok(cmd)
}

fn log_suggests_listen_bind_failure(tail: &str) -> bool {
    let t = tail.to_ascii_lowercase();
    t.contains("eaddrinuse")
        || t.contains("address already in use")
        || t.contains("10048")
        || t.contains("failed to bind")
        || t.contains("bind failed")
        || t.contains("could not bind")
        || (t.contains("bind") && t.contains("in use"))
        || (t.contains("error") && t.contains("listen") && t.contains("socket"))
}

fn try_spawn_llama_process(
    bin_path: &Path,
    model_path: &Path,
    mmproj_path: &Path,
    log_path: &Path,
    listen_port: u16,
    gpu_layers: i32,
    vulkan_visible_device_index: Option<&str>,
) -> Result<std::process::Child, std::io::Error> {
    #[cfg(windows)]
    {
        use std::io::Error as IoError;

        const CREATE_NO_WINDOW: u32 = 0x08000000;
        const BELOW_NORMAL_PRIORITY: u32 = 0x00004000;

        let attempts: [(Option<u32>, bool); 6] = [
            (Some(CREATE_NO_WINDOW | BELOW_NORMAL_PRIORITY), true),
            (Some(CREATE_NO_WINDOW), true),
            (None, true),
            (Some(CREATE_NO_WINDOW | BELOW_NORMAL_PRIORITY), false),
            (Some(CREATE_NO_WINDOW), false),
            (None, false),
        ];

        let mut last_err = IoError::other("llama-server spawn failed (no attempts)");
        let mut spawned: Option<std::process::Child> = None;
        for &(flags, redirect_log) in &attempts {
            let mut cmd = configure_llama_command(
                bin_path,
                model_path,
                mmproj_path,
                log_path,
                listen_port,
                gpu_layers,
                vulkan_visible_device_index,
                redirect_log,
                flags,
            )
            .map_err(IoError::other)?;

            match cmd.spawn() {
                Ok(child) => {
                    spawned = Some(child);
                    break;
                }
                Err(e) => {
                    let retry_os50 = e.raw_os_error() == Some(50);
                    last_err = e;
                    if !retry_os50 {
                        break;
                    }
                }
            }
        }
        spawned.ok_or(last_err)
    }

    #[cfg(not(windows))]
    {
        let mut cmd = configure_llama_command(
            bin_path,
            model_path,
            mmproj_path,
            log_path,
            listen_port,
            gpu_layers,
            vulkan_visible_device_index,
            true,
            None,
        )
        .map_err(std::io::Error::other)?;
        #[cfg(unix)]
        {
            use std::os::unix::process::CommandExt;
            cmd.process_group(0);
        }
        cmd.spawn()
    }
}

fn spawn_llama_managed_child(
    app: &tauri::AppHandle,
    gpu_layers: i32,
    vulkan_visible_device_index: Option<&str>,
) -> Result<std::process::Child, String> {
    // Runtime (binarios + pesos) empacados como Tauri bundle resources. En
    // dev cae al layout del repo autom\u00e1ticamente.
    let local_llm_dir = crate::paths::resource_local_llm_dir(app)?;
    let bin_path = local_llm_dir
        .join("bin")
        .join(crate::paths::llama_server_bin_name());
    let (model_path, mmproj_path) = crate::model_assets::resolved_vision_weights(app)
        .ok_or_else(|| "Local AI weights are missing. Download the model and retry.".to_string())?;

    if !bin_path.exists() {
        return Err(format!(
            "llama-server not found at {:?}. Reinstall FlowSight Agent.",
            bin_path
        ));
    }
    // Check immediately before spawning as well as at UI startup. Native
    // commands can start this process without using the renderer's start path,
    // and Windows shows its own DLL loader dialog before llama-server's main().
    #[cfg(all(windows, not(debug_assertions)))]
    crate::install_health::verify_before_local_ai(app)?;

    let log_path = crate::paths::server_log_path()?;

    let mut last_err: Option<String> = None;

    for attempt in 0..LLAMA_LISTEN_PORT_SPAWN_ATTEMPTS {
        let listen_port = crate::llama_port::pick_localhost_listen_port()?;

        let spawn_result = try_spawn_llama_process(
            &bin_path,
            &model_path,
            &mmproj_path,
            &log_path,
            listen_port,
            gpu_layers,
            vulkan_visible_device_index,
        );

        match spawn_result {
            Ok(mut child) => {
                crate::llama_port::set_managed_llama_port(listen_port);
                std::thread::sleep(std::time::Duration::from_secs(2));
                if let Ok(Some(status)) = child.try_wait() {
                    crate::llama_port::clear_managed_llama_port();
                    let log_tail = read_server_log_tail_chars(1_200);
                    let can_retry_port = (attempt + 1) < LLAMA_LISTEN_PORT_SPAWN_ATTEMPTS
                        && log_suggests_listen_bind_failure(&log_tail);
                    if can_retry_port {
                        log::warn!(
                            "[FlowSight llama-server] quick exit (code {:?}); retrying another listen port (attempt {}/{})",
                            status.code(),
                            attempt + 2,
                            LLAMA_LISTEN_PORT_SPAWN_ATTEMPTS
                        );
                        std::thread::sleep(std::time::Duration::from_millis(
                            40_u64.saturating_mul(u64::from(attempt) + 1),
                        ));
                        continue;
                    }
                    return Err(format!(
                        "llama-server exited during startup (code: {:?}). {}",
                        status.code(),
                        if log_tail.is_empty() {
                            format!("See {:?}", log_path)
                        } else {
                            format!("Log tail: {}", log_tail)
                        }
                    ));
                }
                #[cfg(windows)]
                if let Err(e) =
                    crate::llama_windows_job::assign_llama_child_to_kill_on_close_job(&child)
                {
                    log::warn!(
                        "[FlowSight llama-server] Windows job-object attach skipped: {}",
                        e
                    );
                }
                return Ok(child);
            }
            Err(e) => {
                let msg = format!("Failed to start server: {}", e);
                last_err = Some(msg.clone());
                if (attempt + 1) < LLAMA_LISTEN_PORT_SPAWN_ATTEMPTS
                    && crate::llama_port::tcp_bind_addr_in_use(&e)
                {
                    log::warn!(
                        "[FlowSight llama-server] spawn EADDRINUSE-style error; retrying another port ({}/{}) — {}",
                        attempt + 2,
                        LLAMA_LISTEN_PORT_SPAWN_ATTEMPTS,
                        e
                    );
                    std::thread::sleep(std::time::Duration::from_millis(
                        50_u64.saturating_mul(u64::from(attempt) + 1),
                    ));
                    continue;
                }
                return Err(msg);
            }
        }
    }

    Err(last_err
        .unwrap_or_else(|| "Failed to start server: exhausted listen-port retries.".to_string()))
}

/// Arranca llama-server: modo automático sube desde capas GPU altas hasta que `/health`
/// responda; modo manual fuerza `--n-gpu-layers` fijo.
#[tauri::command]
pub fn start_server(
    app: tauri::AppHandle,
    state: State<'_, AgentState>,
) -> Result<serde_json::Value, String> {
    // Background reports and Coach can start local AI without the renderer's
    // monitoring button. Verify before any model download on those paths.
    #[cfg(all(windows, not(debug_assertions)))]
    crate::install_health::verify_before_local_ai(&app)?;

    let mode = gpu_serve_mode(&state);
    {
        let guard = SERVER_PROCESS.lock().unwrap();
        if guard.is_some() {
            return Ok(serde_json::json!({
                "status": "already_running",
                "message": "Server is already running",
                "gpuAuto": false,
                "localServerPort": crate::llama_port::current_managed_listen_port(),
            }));
        }
    }

    // The UI downloads in a responsive command first. This also covers
    // callers such as report generation without launching empty weights.
    crate::model_assets::ensure_vision_weights(&app)?;

    match mode {
        GpuServeMode::Manual(gpu_layers) => {
            let mut guard = SERVER_PROCESS.lock().unwrap();
            let child = spawn_llama_managed_child(&app, gpu_layers, None)?;
            *guard = Some(child);
            Ok(serde_json::json!({
                "status": "started",
                "pid": "managed",
                "model": VISION_STATUS_LABEL,
                "gpuLayers": gpu_layers,
                "gpuAuto": false,
                "localServerPort": crate::llama_port::current_managed_listen_port(),
            }))
        }
        GpuServeMode::Automatic => {
            let mut last_err = String::from("unknown auto-start error");
            for (index, &(vk_vis, layers)) in AUTO_START_CANDIDATES.iter().enumerate() {
                let vk_label = vk_vis.unwrap_or("default");
                let backend = if layers == 0 {
                    "CPU".to_string()
                } else if let Some(device) = vk_vis {
                    format!("GPU device {device}")
                } else {
                    "GPU".to_string()
                };
                let _ = app.emit(
                    "local-ai-startup-progress",
                    serde_json::json!({
                        "attempt": index + 1,
                        "total": AUTO_START_CANDIDATES.len(),
                        "backend": backend,
                    }),
                );
                let _ = stop_server();
                std::thread::sleep(std::time::Duration::from_millis(450));

                let child = match spawn_llama_managed_child(&app, layers, vk_vis) {
                    Ok(c) => c,
                    Err(e) => {
                        log::warn!(
                            "[FlowSight llama-server] Auto tier GGML_VK_VISIBLE_DEVICES={} gpu_layers={} spawn failed: {}",
                            vk_label,
                            layers,
                            e
                        );
                        last_err = e;
                        continue;
                    }
                };

                {
                    let mut guard = SERVER_PROCESS.lock().unwrap();
                    *guard = Some(child);
                }

                log::info!(
                    "[FlowSight llama-server] Auto tier GGML_VK_VISIBLE_DEVICES={} gpu_layers={}, waiting health up to {}s",
                    vk_label,
                    layers,
                    AUTO_TIER_HEALTH_WAIT_SECS
                );

                if wait_for_managed_health_secs(AUTO_TIER_HEALTH_WAIT_SECS) {
                    return Ok(serde_json::json!({
                        "status": "started",
                        "pid": "managed",
                        "model": VISION_STATUS_LABEL,
                        "gpuLayers": layers,
                        "gpuAuto": true,
                        "vulkanVisibleDevice": vk_label,
                        "localServerPort": crate::llama_port::current_managed_listen_port(),
                    }));
                }

                last_err = format!(
                    "GGML_VK_VISIBLE_DEVICES={} gpu_layers={} did not reach /health within {}s{}",
                    vk_label,
                    layers,
                    AUTO_TIER_HEALTH_WAIT_SECS,
                    {
                        let t = read_server_log_tail_chars(800);
                        if t.is_empty() {
                            String::new()
                        } else {
                            format!(". Last log excerpt: {}", t)
                        }
                    }
                );
                log::warn!("[FlowSight llama-server] {}", last_err);
                let _ = stop_server();
                std::thread::sleep(std::time::Duration::from_millis(350));
            }

            Err(format!(
                "Local AI startup failed after GPU and CPU attempts. {}",
                last_err
            ))
        }
    }
}

/// Tras fallos interminables con GPU (drivers/hardware), reinicia sólo CPU — más lento pero mucho más compatible.
#[tauri::command]
pub fn restart_llama_server_cpu_only(app: tauri::AppHandle) -> Result<serde_json::Value, String> {
    let _ = stop_server();
    std::thread::sleep(std::time::Duration::from_millis(500));

    let mut guard = SERVER_PROCESS.lock().unwrap();
    if guard.is_some() {
        return Err("Could not clear managed server slot; try restarting FlowSight.".to_string());
    }

    let child = spawn_llama_managed_child(&app, 0, None)?;
    *guard = Some(child);
    Ok(serde_json::json!({
        "status": "started",
        "pid": "managed",
        "model": VISION_STATUS_LABEL,
        "gpuLayers": 0,
        "cpuFallback": true,
        "gpuAuto": false,
        "localServerPort": crate::llama_port::current_managed_listen_port(),
    }))
}

#[tauri::command]
pub fn stop_server() -> Result<bool, String> {
    let mut guard = SERVER_PROCESS.lock().unwrap();
    if let Some(mut child) = guard.take() {
        #[cfg(unix)]
        {
            let group = child.id() as i32;
            unsafe {
                libc::kill(-group, libc::SIGTERM);
            }
            std::thread::sleep(Duration::from_millis(200));
            unsafe {
                libc::kill(-group, libc::SIGKILL);
            }
        }
        let _ = child.kill();
        let _ = child.wait();
        crate::llama_port::clear_managed_llama_port();
        #[cfg(windows)]
        crate::llama_windows_job::reset_llama_job();
        return Ok(true);
    }

    crate::llama_port::clear_managed_llama_port();
    #[cfg(windows)]
    crate::llama_windows_job::reset_llama_job();

    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        use std::process::Command;
        let _ = Command::new("taskkill")
            .args(["/F", "/IM", "llama-server.exe"])
            .creation_flags(0x08000000)
            .output();
    }

    Ok(true)
}

fn truncate_repetition_words(text: &str) -> String {
    let words: Vec<&str> = text.split_whitespace().collect();
    if words.len() < 10 {
        return text.to_string();
    }

    let mut result: Vec<&str> = Vec::with_capacity(words.len());
    let mut repeat_count = 0u32;

    for (i, word) in words.iter().enumerate() {
        if i > 0 && *word == words[i - 1] {
            repeat_count += 1;
            if repeat_count >= 4 {
                continue;
            }
        } else {
            repeat_count = 0;
        }
        result.push(word);
    }

    if result.len() < words.len() {
        println!(
            "[Vision] Truncated {} repeated tokens from output",
            words.len() - result.len()
        );
    }
    result.join(" ")
}

/// Collapse runaway token repeats without flattening newlines, so a trailing
/// `CATEGORY: X` line stays parseable and can be stripped from the description.
fn truncate_repetition(text: &str) -> String {
    if !text.contains('\n') {
        return truncate_repetition_words(text);
    }
    text.lines()
        .map(truncate_repetition_words)
        .collect::<Vec<_>>()
        .join("\n")
}

// RESTORED AI ANALYSIS (Backend)
#[tauri::command]
fn analyze_image_with_vision(
    base64_img: &str,
    current_task: &str,
    _gpu_layers: Option<i32>,
) -> Result<String, String> {
    let chat_url = crate::llama_port::managed_chat_completions_url().ok_or_else(|| {
        "Local vision server URL unknown — start the embedded Local AI server first.".to_string()
    })?;
    let client = reqwest::blocking::Client::builder()
        .timeout(std::time::Duration::from_secs(300))
        .build()
        .map_err(|e| e.to_string())?;

    let system_msg = "You are a privacy-first screenshot analysis assistant running on the user's device. You ALWAYS respond with a filled-in template. You NEVER refuse. You NEVER say you cannot see the image. Be accurate and concise: capture the primary task, not a full inventory of the UI. A public app, website, or product name may be named as the foreground destination; never transcribe identifying people, account or workspace names, document titles, full URLs or paths, message contents, credentials, or personal/financial/medical values. Use a generic artifact description or omit sensitive details. The last line of every reply MUST be `CATEGORY: <one allowed label>` — never omit it.";

    let prompt = format!(
        r#"Study this screenshot and complete EVERY field below. Plain text only (no markdown). If the screen is very dense (spreadsheet, large table, dashboard, long doc), stay high-level — do NOT transcribe cell values, columns, or long lists.

TASK CONTEXT (may be empty): {}

Complete this template exactly:

APP: [application name, e.g. Microsoft Excel, Google Chrome, Visual Studio Code]
FOREGROUND DESTINATION: [public app, website or product visibly being used, e.g. Slack inside Chrome; write Unknown if it cannot be identified. Name the destination, not the browser process. Never include an account, workspace, document title, person, URL or path]
WINDOW CONTEXT: [generic screen or document type; never copy the literal title]
VISIBLE CONTENT: [1–2 short sentences: the main artifact on screen and what it is for — not every panel or control]
ARTIFACT TYPE: [generic type only, such as report, spreadsheet, design, source file, or research article; never copy a file name, path, URL, person, customer, or account name]
CURRENT ACTION: [what the user appears to be doing right now, one sentence]
PROGRESS: [errors, warnings, build/test status if any, or None visible]
ENGAGEMENT: [exactly one: work, reading, live meeting, playing media, waiting, unknown. Use visible content, not input activity or the application name. A call lobby or ended call is not a live meeting. A static document may be reading; this is a possible context, not proof of attention. Use waiting for a loading/stalled work surface. Never claim task completion or temporal progress from this single frame]
NEXT STEP: [one short sentence: likely next action]
CATEGORY: [pick exactly ONE from: {}]

The LAST line MUST be exactly `CATEGORY: <one label from the list>`. Never omit it. Never leave it blank.

CATEGORY rules (follow strictly):
- Analysis: sustained spreadsheet, data, financial, scientific, business-intelligence, or dashboard analysis.
- Writing: drafting or substantially editing prose, reports, articles, proposals, or presentations.
- Design: creating or substantially editing visual, product, architectural, or service designs.
- Planning: roadmaps, schedules, task organization, prioritization, and project planning.
- Meeting: a live call, presentation, workshop, or meeting in progress.
- Communication: email, chat, messages, or asynchronous collaboration.
- Documentation: producing or maintaining reference material, procedures, or structured internal knowledge.
- Learning: coursework, tutorials, deliberate study, or skills practice.
- Sales: CRM, prospecting, proposals, pipeline work, or customer-facing commercial activity.
- Admin: expenses, forms, routine record keeping, scheduling, finance operations, or access/settings maintenance.
- Coding: IDEs and AI code editors (Eclipse, Cursor, VS Code, JetBrains, Xcode, Neovim), editing source files, programming terminals (git/cargo/npm), local app development UIs.
- Debugging: inspecting failures with a debugger, stack trace, logs, or breakpoints.
- CodeReview: pull requests, diff review, GitHub/GitLab PR pages.
- Testing: running or inspecting test suites, test results, or quality checks.
- DevOps: CI/CD, GitHub Actions, release pipelines, Docker/K8s ops.
- Database: querying, designing, or administering databases and data stores.
- Research: scholarly or subject-matter sources, market/customer research, reference material, API docs, or documentation consulted to answer a work question (NOT social media).
- Browsing: ONLY casual/non-work web use (social, news, shopping, YouTube, LinkedIn feed). Safari/Chrome showing GitHub repos, Actions, Releases, or code is NOT Browsing — use Coding, CodeReview, or DevOps.
- Idle: a lock screen or clearly inactive/blank work surface; never infer this from a lack of keyboard or mouse data.
- General: use only when the primary work activity is genuinely ambiguous after considering the screenshot and task context.
- Spreadsheets may be Analysis or Admin depending on the visible task. Documents may be Writing, Documentation, Research, or Admin. Email/chat are Communication. Do not assume knowledge work means software development.
Never label Eclipse showing a source file/workspace, Cursor, VS Code, or a GitHub engineering page as Browsing or General. Eclipse Java editing is Coding; debugger/breakpoint work is Debugging; running JUnit/test suites is Testing."#,
        current_task,
        allowed_categories_prompt()
    );

    // Retry up to 2 times on empty/refusal responses
    let max_attempts = 2;
    for attempt in 1..=max_attempts {
        let body = serde_json::json!({
            "model": LLAMA_CHAT_MODEL_ID,
            "messages": [
                {
                    "role": "system",
                    "content": system_msg
                },
                {
                    "role": "user",
                    "content": [
                        { "type": "text", "text": prompt },
                        {
                            "type": "image_url",
                            "image_url": {
                                "url": format!("data:image/png;base64,{}", base64_img)
                            }
                        }
                    ]
                }
            ],
            "temperature": 0.1,
            "top_p": 0.9,
            "max_tokens": 800,
            "repeat_penalty": 1.3,
            "frequency_penalty": 0.5,
            "presence_penalty": 0.5,
            "stream": false
        });

        let resp = client
            .post(&chat_url)
            .json(&body)
            .send()
            .map_err(|e| format!("Request failed: {}", e))?;

        if !resp.status().is_success() {
            return Err(format!("Server Error: {}", resp.status()));
        }

        let json: serde_json::Value = resp.json().map_err(|e| e.to_string())?;
        let content = json["choices"][0]["message"]["content"]
            .as_str()
            .unwrap_or("")
            .trim();

        // Detect empty or refusal responses
        let is_empty = content.is_empty();
        let c = content.to_lowercase();
        let is_refusal = c.contains("i'm unable to")
            || c.contains("i cannot")
            || c.contains("i can't")
            || c.contains("i am unable")
            || c.contains("unable to view")
            || c.contains("unable to analyze")
            || c.contains("can't assist")
            || c.contains("cannot assist")
            || c.contains("as an ai language model")
            || c.contains("no puedo ver")
            || c.contains("no puedo analizar");

        if is_empty || is_refusal {
            println!(
                "[Vision] Attempt {}/{}: empty or refusal response, retrying...",
                attempt, max_attempts
            );
            if attempt < max_attempts {
                std::thread::sleep(std::time::Duration::from_secs(1));
                continue;
            }
            return Err(if is_empty {
                "Model returned empty response after retries".to_string()
            } else {
                "Model refused or could not analyze the screenshot after retries".to_string()
            });
        }

        let content = truncate_repetition(content);
        return Ok(content);
    }

    Err("Model analysis failed after retries".to_string())
}

// ============== TELEMETRY PIPELINE SUPPORT ==============
//
// The functions below back `telemetry::aggregator` (configured-interval action-log review of
// accumulated UI Automation events, plus a periodic screenshot + vision pass)
// and `telemetry::action_capture` (screenshot fired on a significant window
// open/close, debounced). All reuse the canonical `reports` table/schema;
// capture source metadata distinguishes periodic and action-triggered records.

/// Free-function insert into the shared `reports` table, reusing the exact
/// canonical schema used by the telemetry pipeline. Used by the
/// aggregator/action_capture threads, which have no
/// `AgentState` handle (they only hold a `db_path`).
#[allow(clippy::too_many_arguments)] // explicit persistence boundary; fields map one-to-one to SQLite
pub(crate) fn insert_report(
    db_path: &Path,
    description: &str,
    activity_type: &str,
    jira_ticket: Option<String>,
    duration_seconds: u64,
    capture_source: &str,
    theme_hint: Option<String>,
    captured_context: Option<CapturedWindowContext>,
    observed_at_utc: Option<String>,
    progress: Option<&crate::task_progress::ProgressObservation>,
) -> Option<i64> {
    let activity_type = resolve_persisted_category(activity_type);
    let jira_ticket = canonical_ticket_value(jira_ticket.as_deref());
    let conn = Connection::open(db_path).ok()?;
    let mut system = captured_context
        .unwrap_or_else(|| CapturedWindowContext::from(crate::context::get_system_context()));
    if !crate::privacy::store_window_titles(db_path) {
        system.window_title = None;
    }
    let progress_json = progress.and_then(|value| serde_json::to_string(value).ok());
    conn.execute(
        "INSERT INTO reports (description, activity_type, jira_ticket_id, duration_seconds, active_app, window_title, capture_source, theme_hint, created_at, progress_observation) VALUES (?, ?, ?, ?, ?, ?, ?, ?, COALESCE(?, CURRENT_TIMESTAMP), ?)",
        params![description, activity_type, jira_ticket, duration_seconds, system.app_name, system.window_title, capture_source, theme_hint, observed_at_utc, progress_json],
    )
    .ok()?;
    let report_id = conn.last_insert_rowid();
    crate::online_leagues::bind_observation(&conn, report_id, system.league_user_id.as_deref());
    crate::sync::bind_cloud_observation(&conn, report_id, system.cloud_owner.as_ref());
    if capture_source == "periodic_vision" && system.league_user_id.is_some() {
        crate::online_leagues::request_sync();
    }
    Some(report_id)
}

#[derive(Debug, Clone)]
pub(crate) struct CapturedWindowContext {
    pub app_name: Option<String>,
    pub window_title: Option<String>,
    pub league_user_id: Option<String>,
    pub cloud_owner: Option<crate::sync::CloudCaptureOwner>,
}

impl From<crate::context::SystemContext> for CapturedWindowContext {
    fn from(value: crate::context::SystemContext) -> Self {
        Self {
            app_name: value.app_name,
            window_title: value.window_title,
            league_user_id: None,
            cloud_owner: None,
        }
    }
}

#[derive(Debug, Clone)]
pub(crate) struct AnalyzedCapture {
    pub description: String,
    pub category: String,
    pub window: CapturedWindowContext,
    /// UTC wall-clock timestamp taken with the frame, before local inference.
    pub observed_at_utc: String,
    pub fingerprint: crate::task_progress::VisualFingerprint,
    pub engagement: crate::task_progress::Engagement,
    pub progress_owner_id: Option<String>,
}

/// Configured-interval screenshot + local vision analysis, driven by
/// `telemetry::aggregator`. Uses the full `analyze_image_with_vision` template
/// and keeps the frame in memory only.
pub(crate) fn capture_and_analyze_screen(
    db_path: &Path,
    task_context: &str,
) -> Result<AnalyzedCapture, String> {
    let observed_at_utc = chrono::Utc::now().format("%Y-%m-%d %H:%M:%S").to_string();
    let league_user_id = crate::online_leagues::capture_owner(db_path);
    let cloud_owner = crate::sync::cloud_capture_owner(db_path);
    // Scope comparison to the local signed-in account even when leagues are off.
    let progress_owner_id = Connection::open(db_path)
        .ok()
        .and_then(|conn| crate::sync::get_user_session_from_conn(&conn))
        .map(|session| session.user_id);
    let (capture, sys, fingerprint) = capture_screen(db_path)?;
    let raw_analysis =
        analyze_image_with_vision(&capture, task_context, None).unwrap_or_else(|e| {
            log::warn!("[Telemetry][VisionSnapshot] vision analysis failed: {e}");
            "Screen analysis failed.\nCATEGORY: General".to_string()
        });
    let (description, model_category) = parse_analysis(&raw_analysis);
    let category = crate::agent_pure::correct_category_with_window(
        &model_category,
        sys.app_name.as_deref(),
        sys.window_title.as_deref(),
    );
    Ok(AnalyzedCapture {
        fingerprint,
        engagement: crate::task_progress::Engagement::from_analysis(&raw_analysis),
        progress_owner_id,
        description,
        category,
        window: CapturedWindowContext {
            app_name: sys.app_name,
            window_title: sys.window_title,
            league_user_id,
            cloud_owner,
        },
        observed_at_utc,
    })
}

/// Screenshot capture triggered by a specific, significant UI Automation
/// action (a window opening/closing — see `telemetry::action_capture`),
/// combining the frame with the textual context of what triggered it in a
/// single local-model call so it can fuse visual + semantic signal. Uses
/// The frame remains in memory and is dropped immediately after analysis.
pub(crate) fn capture_and_analyze_action(
    db_path: &Path,
    task_context: &str,
    action_context: &str,
) -> Result<AnalyzedCapture, String> {
    let observed_at_utc = chrono::Utc::now().format("%Y-%m-%d %H:%M:%S").to_string();
    let league_user_id = crate::online_leagues::capture_owner(db_path);
    let cloud_owner = crate::sync::cloud_capture_owner(db_path);
    let (capture, sys, fingerprint) = capture_screen(db_path)?;
    let raw_analysis =
        analyze_action_screenshot_with_vision(&capture, task_context, action_context)
            .unwrap_or_else(|e| {
                log::warn!("[Telemetry][ActionCapture] vision analysis failed: {e}");
                "Screen analysis failed.\nCATEGORY: General".to_string()
            });
    let (description, model_category) = parse_analysis(&raw_analysis);
    let category = crate::agent_pure::correct_category_with_window(
        &model_category,
        sys.app_name.as_deref(),
        sys.window_title.as_deref(),
    );
    Ok(AnalyzedCapture {
        fingerprint,
        engagement: crate::task_progress::Engagement::Unknown,
        progress_owner_id: None,
        description,
        category,
        window: CapturedWindowContext {
            app_name: sys.app_name,
            window_title: sys.window_title,
            league_user_id,
            cloud_owner,
        },
        observed_at_utc,
    })
}

/// Vision analysis for an action-triggered capture: same privacy stance as
/// `review_actions_with_local_model` (never echo identifying details
/// verbatim, generalize or omit sensitive content), but also given the
/// screenshot so the model can combine what it sees with the UI action that
/// just fired. Deliberately a short prompt/response (unlike the full
/// `analyze_image_with_vision` template): this call fires on every
/// significant window event, so it must stay cheap.
fn analyze_action_screenshot_with_vision(
    base64_img: &str,
    current_task: &str,
    action_context: &str,
) -> Result<String, String> {
    let chat_url = crate::llama_port::managed_chat_completions_url().ok_or_else(|| {
        "Local vision server URL unknown — start the embedded Local AI server first.".to_string()
    })?;
    let client = reqwest::blocking::Client::builder()
        .timeout(std::time::Duration::from_secs(120))
        .build()
        .map_err(|e| e.to_string())?;

    let system_msg = format!(
        "You are an activity reviewer running 100% on the user's own device. You receive a \
screenshot plus a short note about the UI action that just triggered it. Strict privacy rules, no exceptions: \
never reproduce full window titles, full file names, URLs, or field contents verbatim if they \
could be identifying; always generalize instead (e.g. \"editing a spreadsheet\" rather than the exact file \
name); if something looks sensitive (financial, medical, personal, credentials), omit it entirely rather than \
anonymizing it. Combine what you see in the screenshot with the triggering action into 1-2 short generic \
sentences. The LAST line of your reply MUST be exactly `CATEGORY: X` where X is one of: {}. Never omit that line.",
        allowed_categories_prompt()
    );

    let user_prompt = format!(
        "Current task context: {}\n\nTriggering action: {}\n\nDescribe in 1-2 short, generic sentences what kind \
of work this looks like, combining the screenshot with the triggering action (never repeat identifying details \
verbatim).\n\nThe LAST line of your reply MUST be exactly:\nCATEGORY: X\nwhere X is one of: {}\nDo not skip this line. Do not leave it blank.",
        current_task, action_context, allowed_categories_prompt()
    );

    let body = serde_json::json!({
        "model": LLAMA_CHAT_MODEL_ID,
        "messages": [
            { "role": "system", "content": system_msg },
            {
                "role": "user",
                "content": [
                    { "type": "text", "text": user_prompt },
                    { "type": "image_url", "image_url": { "url": format!("data:image/png;base64,{}", base64_img) } }
                ]
            }
        ],
        "temperature": 0.3,
        "max_tokens": 300,
        "stream": false
    });

    let resp = client
        .post(&chat_url)
        .json(&body)
        .send()
        .map_err(|e| format!("Request failed: {}", e))?;
    if !resp.status().is_success() {
        return Err(format!("Server Error: {}", resp.status()));
    }
    let json: serde_json::Value = resp.json().map_err(|e| e.to_string())?;
    let content = json["choices"][0]["message"]["content"]
        .as_str()
        .unwrap_or("")
        .trim();
    if content.is_empty() {
        return Err("Model returned empty response".to_string());
    }
    Ok(truncate_repetition(content))
}

/// Privacy-first review of raw UI Automation action events (Level 1 of the
/// telemetry pipeline, see `telemetry::aggregator`). No screenshot involved —
/// only a short textual log of which controls/windows/apps were interacted
/// with. The system prompt instructs the local model to never echo back
/// identifiable details (file names, URLs, window titles) and to
/// generalize or omit anything sensitive instead of anonymizing it.
pub(crate) fn review_actions_with_local_model(
    action_summary: &str,
    current_task: &str,
) -> Result<String, String> {
    let chat_url = crate::llama_port::managed_chat_completions_url().ok_or_else(|| {
        "Local vision server URL unknown — start the embedded Local AI server first.".to_string()
    })?;
    let client = reqwest::blocking::Client::builder()
        .timeout(std::time::Duration::from_secs(60))
        .build()
        .map_err(|e| e.to_string())?;

    let system_msg = format!(
        "You are an activity reviewer running 100% on the user's own device. You receive a short \
log of UI interaction events (which controls were used, in which apps) — never raw screen content and never \
any keystroke or last-input signal. Strict privacy rules, no exceptions: never reproduce full window titles, \
full file names, URLs, or field contents verbatim if they could be identifying; always generalize instead \
(e.g. \"editing a spreadsheet\" rather than the exact file name); if something looks sensitive (financial, \
medical, personal, credentials), omit it entirely rather than anonymizing it. Do not infer that the user is \
away or idle from a short/empty log. Your only allowed output is 1-2 short generic sentences, then a last line \
that MUST be exactly `CATEGORY: X` where X is one of: {}. Never omit the CATEGORY line. Never leave it blank.",
        allowed_categories_prompt()
    );

    let user_prompt = format!(
        "Current task context: {}\n\nAction log for the last minute:\n{}\n\nSummarize in 1-2 short, generic \
sentences what kind of work this looks like (never repeat identifying details from the log verbatim).\n\nThe \
LAST line of your reply MUST be exactly:\nCATEGORY: X\nwhere X is one of: {}\nDo not skip this line. Do not leave it blank.",
        current_task, action_summary, allowed_categories_prompt()
    );

    let body = serde_json::json!({
        "model": LLAMA_CHAT_MODEL_ID,
        "messages": [
            { "role": "system", "content": system_msg },
            { "role": "user", "content": user_prompt }
        ],
        "temperature": 0.2,
        "max_tokens": 200,
        "stream": false
    });

    let resp = client
        .post(&chat_url)
        .json(&body)
        .send()
        .map_err(|e| format!("Request failed: {}", e))?;
    if !resp.status().is_success() {
        return Err(format!("Server Error: {}", resp.status()));
    }
    let json: serde_json::Value = resp.json().map_err(|e| e.to_string())?;
    let content = json["choices"][0]["message"]["content"]
        .as_str()
        .unwrap_or("")
        .trim();
    if content.is_empty() {
        return Err("Model returned empty response".to_string());
    }
    Ok(content.to_string())
}

#[cfg(test)]
mod agent_struct_tests {
    use super::*;
    use chrono::TimeZone;

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
    fn agent_config_json_roundtrip_negative_one_auto_marker() {
        let c = AgentConfig {
            dev_name: Some("Tester".into()),
            capture_interval: Some(42_000),
            vision_model: Some("model-id".into()),
            gpu_layers: Some(-1),
            daily_goal_hours: Some(6.0),
        };
        let json = serde_json::to_string(&c).unwrap();
        let back: AgentConfig = serde_json::from_str(&json).unwrap();
        assert_eq!(back.gpu_layers, Some(-1));
    }

    #[test]
    fn agent_config_json_roundtrip() {
        let c = AgentConfig {
            dev_name: Some("Tester".into()),
            capture_interval: Some(42_000),
            vision_model: Some("model-id".into()),
            gpu_layers: Some(4),
            daily_goal_hours: Some(8.0),
        };
        let json = serde_json::to_string(&c).unwrap();
        let back: AgentConfig = serde_json::from_str(&json).unwrap();
        assert_eq!(back.dev_name, c.dev_name);
        assert_eq!(back.gpu_layers, c.gpu_layers);
    }

    #[test]
    fn insert_report_persists_time_and_context_from_the_captured_frame() {
        let dir = tempfile::tempdir().unwrap();
        let db_path = dir.path().join("capture-context.sqlite");
        let conn = Connection::open(&db_path).unwrap();
        conn.execute_batch(
            "CREATE TABLE config (key TEXT PRIMARY KEY, value TEXT);
             INSERT INTO config (key, value) VALUES (
                'privacy_settings',
                '{\"noticeVersion\":\"2026-08-23\",\"storeWindowTitles\":true}'
             );
             CREATE TABLE reports (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                description TEXT,
                activity_type TEXT,
                jira_ticket_id TEXT,
                duration_seconds INTEGER,
                active_app TEXT,
                window_title TEXT,
                capture_source TEXT,
                theme_hint TEXT,
                created_at TEXT DEFAULT CURRENT_TIMESTAMP
            );",
        )
        .unwrap();
        crate::task_progress::ensure_schema(&conn).unwrap();
        drop(conn);

        insert_report(
            &db_path,
            "Drafting a proposal",
            "Writing",
            Some("  General  ".into()),
            42,
            "periodic_vision",
            Some("Proposal".into()),
            Some(CapturedWindowContext {
                app_name: Some("Captured App".into()),
                window_title: Some("Captured Window".into()),
                league_user_id: None,
                cloud_owner: None,
            }),
            Some("2026-08-22 07:30:00".into()),
            Some(&crate::task_progress::ProgressObservation {
                state: crate::task_progress::ProgressState::PassiveContext,
                observed_seconds: 42,
                unchanged_seconds: 1260,
                over_threshold_seconds: 42,
            }),
        )
        .unwrap();

        let conn = Connection::open(&db_path).unwrap();
        let evidence_json: String = conn
            .query_row("SELECT progress_observation FROM reports", [], |row| {
                row.get(0)
            })
            .unwrap();
        let evidence: crate::task_progress::ProgressObservation =
            serde_json::from_str(&evidence_json).unwrap();
        assert_eq!(
            evidence.state,
            crate::task_progress::ProgressState::PassiveContext
        );
        assert_eq!(evidence.observed_seconds, 42);
        let saved: (String, String, String, Option<String>) = conn
            .query_row(
                "SELECT active_app, window_title, created_at, jira_ticket_id FROM reports",
                [],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
            )
            .unwrap();
        assert_eq!(
            saved,
            (
                "Captured App".into(),
                "Captured Window".into(),
                "2026-08-22 07:30:00".into(),
                None,
            )
        );
    }

    #[test]
    fn daily_history_and_week_totals_split_end_timestamped_rows_at_midnight() {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch(
            "CREATE TABLE reports (
                id INTEGER PRIMARY KEY,
                created_at TEXT NOT NULL,
                description TEXT,
                activity_type TEXT,
                jira_ticket_id TEXT,
                duration_seconds INTEGER,
                active_app TEXT,
                window_title TEXT,
                capture_source TEXT,
                theme_hint TEXT
            );",
        )
        .unwrap();
        let today = Local::now().date_naive();
        let previous = today.pred_opt().unwrap();
        let crossing_end = today.and_hms_opt(0, 1, 0).unwrap();
        let annotation_time = today.and_hms_opt(12, 0, 0).unwrap();
        conn.execute(
            "INSERT INTO reports (
                created_at, description, activity_type, jira_ticket_id,
                duration_seconds, active_app, window_title, capture_source, theme_hint
             ) VALUES (?1, 'crossing work', 'Writing', NULL, 120, 'Writer', 'Draft', 'test', 'Report')",
            params![utc_storage_timestamp(crossing_end)],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO reports (
                created_at, description, activity_type, jira_ticket_id,
                duration_seconds, active_app, window_title, capture_source, theme_hint
             ) VALUES (?1, 'annotation', 'Planning', NULL, 0, 'Calendar', 'Plan', 'action_review', 'Report')",
            params![utc_storage_timestamp(annotation_time)],
        )
        .unwrap();

        let previous_str = previous.format("%Y-%m-%d").to_string();
        let today_str = today.format("%Y-%m-%d").to_string();
        let previous_entries = load_day_history_entries(&conn, &previous_str).unwrap();
        let today_entries = load_day_history_entries(&conn, &today_str).unwrap();
        assert_eq!(
            previous_entries
                .iter()
                .map(|entry| entry.duration_seconds)
                .sum::<i32>(),
            60
        );
        assert_eq!(
            today_entries
                .iter()
                .map(|entry| entry.duration_seconds)
                .sum::<i32>(),
            60
        );
        assert_eq!(today_entries.len(), 2, "zero-second annotation is retained");

        let totals = load_daily_totals(&conn, &previous_str, &today_str).unwrap();
        assert_eq!(totals.get(&previous_str), Some(&60));
        assert_eq!(totals.get(&today_str), Some(&60));
        assert_eq!(totals.values().sum::<i32>(), 120);
    }
}

#[cfg(test)]
mod repetition_tests {
    use super::truncate_repetition;

    #[test]
    fn truncate_short_text_noop() {
        let s = "a b c d e f g h i";
        assert_eq!(truncate_repetition(s), s);
    }

    #[test]
    fn truncate_collapses_many_repeated_words() {
        let spam = "spam ".repeat(25);
        let out = truncate_repetition(spam.trim());
        assert!(out.len() < spam.len());
    }

    #[test]
    fn truncate_preserves_category_newline() {
        let raw = "CURRENT ACTION: editing a file in the editor window now\nCATEGORY: Coding";
        let out = truncate_repetition(raw);
        assert!(out.contains('\n'));
        assert!(out
            .lines()
            .any(|l| l.to_uppercase().starts_with("CATEGORY:")));
    }
}
