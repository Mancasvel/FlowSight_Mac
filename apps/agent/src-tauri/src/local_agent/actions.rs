use chrono::{DateTime, Duration, Utc};
use serde_json::{json, Value};
use std::sync::Mutex;
use tauri::{AppHandle, Emitter, State};

use super::state::{self, FocusBlock, LocalEvent, LocalTask, MessageDraft, SavedPreference};
use crate::agent::AgentState;

static FOCUS_ACTION_LOCK: Mutex<()> = Mutex::new(());

fn text_arg<'a>(args: &'a Value, name: &str) -> Result<&'a str, String> {
    args[name]
        .as_str()
        .ok_or_else(|| format!("Missing {name}."))
}

fn integer_arg(args: &Value, name: &str) -> Result<i64, String> {
    args[name]
        .as_i64()
        .ok_or_else(|| format!("Missing {name}."))
}

fn parse_time(value: &str) -> Result<DateTime<Utc>, String> {
    DateTime::parse_from_rfc3339(value)
        .map(|date| date.with_timezone(&Utc))
        .map_err(|_| "Use a date and time with a time zone.".to_string())
}

fn interval(args: &Value) -> Result<(DateTime<Utc>, DateTime<Utc>), String> {
    let start = parse_time(text_arg(args, "start_at")?)?;
    let end = parse_time(text_arg(args, "end_at")?)?;
    if end <= start || end - start > Duration::days(30) {
        return Err("Choose an interval longer than zero and no longer than 30 days.".into());
    }
    Ok((start, end))
}

fn free_slots(events: &[LocalEvent], start: DateTime<Utc>, end: DateTime<Utc>) -> Vec<Value> {
    let mut busy: Vec<_> = events
        .iter()
        .filter_map(|event| {
            let event_start = parse_time(&event.start_at).ok()?;
            let event_end = parse_time(&event.end_at).ok()?;
            (event_start < end && event_end > start)
                .then_some((event_start.max(start), event_end.min(end)))
        })
        .collect();
    busy.sort_by_key(|item| item.0);
    let mut cursor = start;
    let mut free = Vec::new();
    for (busy_start, busy_end) in busy {
        if busy_start > cursor {
            free.push(json!({"startAt":cursor.to_rfc3339(),"endAt":busy_start.to_rfc3339()}));
        }
        cursor = cursor.max(busy_end);
    }
    if cursor < end {
        free.push(json!({"startAt":cursor.to_rfc3339(),"endAt":end.to_rfc3339()}));
    }
    free
}

fn focus_patterns(args: &Value) -> Vec<String> {
    args["block_patterns"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|item| item.as_str().map(str::to_string))
        .collect()
}

fn total_preferences(args: &Value) -> Result<super::total_focus::Preferences, String> {
    let mut preferences = state::read()?.total_focus_preferences;
    if let Some(minutes) = args["duration_minutes"].as_u64() {
        preferences.duration_minutes = minutes as u16;
    }
    if let Some(patterns) = args["patterns"].as_array() {
        preferences.patterns = patterns
            .iter()
            .filter_map(|value| value.as_str().map(str::to_string))
            .collect();
    }
    if let Some(exceptions) = args["exceptions"].as_array() {
        preferences.exceptions = exceptions
            .iter()
            .filter_map(|value| value.as_str().map(str::to_string))
            .collect();
    }
    super::total_focus::validate(preferences)
}

fn apply_focus_protection(
    protection: &str,
    patterns: &[String],
    minutes: i64,
) -> Result<bool, String> {
    if protection == "gentle" {
        return Ok(false);
    }
    if protection == "strict" {
        super::browser_bridge::execute(
            "browser.block",
            &json!({"patterns":patterns,"duration_minutes":minutes.max(1)}),
        )?;
    }
    let owned = state::read()?.quiet.is_none();
    if owned {
        if let Err(error) = super::system_quiet::enable(Some(minutes.max(1))) {
            if protection == "strict" {
                let _ = super::browser_bridge::execute(
                    "browser.unblock",
                    &json!({"patterns":patterns}),
                );
            }
            return Err(error);
        }
    }
    Ok(owned)
}

fn release_focus_protection(focus: &FocusBlock) -> Vec<String> {
    let mut warnings = Vec::new();
    if focus.quiet_owned {
        if let Err(error) = super::system_quiet::disable() {
            warnings.push(format!(
                "Windows notification setting could not be restored: {error}"
            ));
        }
    }
    if !focus.blocked_patterns.is_empty() {
        if let Err(error) = super::browser_bridge::execute(
            "browser.unblock",
            &json!({"patterns":focus.blocked_patterns}),
        ) {
            warnings.push(format!("Browser blocks will expire on their own: {error}"));
        }
    }
    warnings
}

fn elapsed_focus_seconds(focus: &FocusBlock) -> Result<i64, String> {
    let started = parse_time(&focus.started_at)?;
    let end = if focus.status == "paused" {
        focus
            .paused_at
            .as_deref()
            .map(parse_time)
            .transpose()?
            .unwrap_or_else(Utc::now)
    } else {
        Utc::now()
    };
    Ok((end - started)
        .num_seconds()
        .saturating_sub(focus.paused_seconds)
        .max(0))
}

pub fn browser_tab_url(tab_id: i64) -> Result<String, String> {
    let list = super::browser_bridge::execute("browser.list_tabs", &json!({}))?;
    list["tabs"]
        .as_array()
        .into_iter()
        .flatten()
        .find(|tab| tab["id"].as_i64() == Some(tab_id))
        .and_then(|tab| tab["url"].as_str())
        .map(str::to_string)
        .ok_or("That browser tab is not open anymore.".into())
}

fn browser_restore_label(restore_id: &str) -> Result<String, String> {
    let list = super::browser_bridge::execute("browser.list_tabs", &json!({}))?;
    let tab = list["restorable"]
        .as_array()
        .into_iter()
        .flatten()
        .find(|tab| tab["restoreId"].as_str() == Some(restore_id))
        .ok_or("That browser restore record was not found.")?;
    Ok(format!(
        "{} ({})",
        tab["title"].as_str().unwrap_or("Untitled"),
        tab["url"].as_str().unwrap_or("unknown URL")
    ))
}

pub fn expire_focus_if_due(
    app: AppHandle,
    agent_state: State<'_, AgentState>,
) -> Result<bool, String> {
    let Some(focus) = state::read()?.focus else {
        return Ok(false);
    };
    if focus.status != "running"
        || elapsed_focus_seconds(&focus)? < focus.duration_minutes as i64 * 60
    {
        return Ok(false);
    }
    let result = execute("focus.end", &json!({"only_if_due":true}), app, agent_state)?;
    Ok(result["skipped"] != true)
}

fn paired(english: String, spanish: String) -> Result<(String, String), String> {
    Ok((english, spanish))
}

// Both copies use the same arguments and, where needed, one saved-state read.
// User titles, bodies, URLs and identifiers are always inserted verbatim.
pub fn preview(name: &str, args: &Value) -> Result<(String, String), String> {
    if name == "focus.total_start" {
        let prefs = total_preferences(args)?;
        return paired(format!("Total focus: {} · {} minutes. Block: {}. Exceptions: {}. No automatic messages.", text_arg(args,"intention")?,prefs.duration_minutes,prefs.patterns.join(", "),prefs.exceptions.join(", ")),
            format!("Concentración total: {} · {} minutos. Bloquear: {}. Excepciones: {}. Sin mensajes automáticos.", text_arg(args,"intention")?,prefs.duration_minutes,prefs.patterns.join(", "),prefs.exceptions.join(", ")));
    }
    if name == "focus.total_end" {
        return paired(
            "End total focus and release browser protection.".into(),
            "Finalizar concentración total y liberar la protección del navegador.".into(),
        );
    }
    match name {
        "focus.start" => {
            let protection = text_arg(args, "protection")?;
            let (protection_summary, protection_es) = match protection {
                "gentle" => ("tracking only".to_string(), "solo seguimiento".to_string()),
                "standard" => (
                    "Windows app banners silenced".to_string(),
                    "avisos de aplicaciones de Windows silenciados".to_string(),
                ),
                "strict" => (
                    format!(
                        "Windows app banners silenced; browser blocks: {}",
                        args["block_patterns"]
                    ),
                    format!(
                        "avisos de aplicaciones de Windows silenciados; bloqueos del navegador: {}",
                        args["block_patterns"]
                    ),
                ),
                _ => return Err("Unknown focus protection level.".into()),
            };
            paired(
                format!(
                    "Start a {}-minute focus block on '{}' ({protection_summary})",
                    integer_arg(args, "duration_minutes")?,
                    text_arg(args, "intention")?
                ),
                format!(
                    "Iniciar un bloque de concentración de {} minutos en '{}' ({protection_es})",
                    integer_arg(args, "duration_minutes")?,
                    text_arg(args, "intention")?
                ),
            )
        }
        "focus.pause" => paired(
            "Pause the active focus block and tracking".into(),
            "Pausar el bloque de concentración activo y el seguimiento".into(),
        ),
        "focus.resume" => paired(
            "Resume the paused focus block and its protections".into(),
            "Reanudar el bloque de concentración y sus protecciones".into(),
        ),
        "focus.end" => paired(
            "End the active focus block and tracking".into(),
            "Finalizar el bloque de concentración activo y el seguimiento".into(),
        ),
        "system.set_dnd" => paired(
            format!(
                "{} Windows app notification banners{}",
                if args["enabled"] == true {
                    "Silence"
                } else {
                    "Restore"
                },
                args["duration_minutes"]
                    .as_i64()
                    .map(|minutes| format!(" for {minutes} minutes"))
                    .unwrap_or_default(),
            ),
            format!(
                "{} los avisos de aplicaciones de Windows{}",
                if args["enabled"] == true {
                    "Silenciar"
                } else {
                    "Restaurar"
                },
                args["duration_minutes"]
                    .as_i64()
                    .map(|minutes| format!(" durante {minutes} minutos"))
                    .unwrap_or_default()
            ),
        ),
        "browser.block" => paired(
            format!("Block these browser patterns: {}", args["patterns"]),
            format!(
                "Bloquear estos patrones del navegador: {}",
                args["patterns"]
            ),
        ),
        "browser.unblock" => paired(
            format!("Unblock these browser patterns: {}", args["patterns"]),
            format!(
                "Desbloquear estos patrones del navegador: {}",
                args["patterns"]
            ),
        ),
        "browser.close_tab" => paired(
            format!("Close browser tab {}", integer_arg(args, "tab_id")?),
            format!(
                "Cerrar la pestaña {} del navegador",
                integer_arg(args, "tab_id")?
            ),
        ),
        "browser.restore_tab" => {
            let label = browser_restore_label(text_arg(args, "restore_id")?)?;
            paired(
                format!("Restore tab {label}"),
                format!("Restaurar la pestaña {label}"),
            )
        }
        "tasks.create" => paired(
            format!("Create task: {}", text_arg(args, "title")?),
            format!("Crear tarea: {}", text_arg(args, "title")?),
        ),
        "tasks.update" => paired(
            format!("Update task {}: {}", text_arg(args, "task_id")?, args),
            format!(
                "Actualizar la tarea {}: {}",
                text_arg(args, "task_id")?,
                args
            ),
        ),
        "tasks.complete" => paired(
            format!("Complete task {}", text_arg(args, "task_id")?),
            format!("Completar la tarea {}", text_arg(args, "task_id")?),
        ),
        "tasks.reprioritize" => paired(
            format!(
                "Set task {} to priority {}",
                text_arg(args, "task_id")?,
                integer_arg(args, "priority")?
            ),
            format!(
                "Asignar la prioridad {} a la tarea {}",
                integer_arg(args, "priority")?,
                text_arg(args, "task_id")?
            ),
        ),
        "calendar.create_event" => paired(
            format!(
                "Create '{}' in the FlowSight local calendar from {} to {}",
                text_arg(args, "title")?,
                text_arg(args, "start_at")?,
                text_arg(args, "end_at")?
            ),
            format!(
                "Crear '{}' en el calendario local de FlowSight de {} a {}",
                text_arg(args, "title")?,
                text_arg(args, "start_at")?,
                text_arg(args, "end_at")?
            ),
        ),
        "calendar.move_event" => paired(
            format!(
                "Move FlowSight local event {} to {}–{}",
                text_arg(args, "event_id")?,
                text_arg(args, "start_at")?,
                text_arg(args, "end_at")?
            ),
            format!(
                "Mover el evento local {} de FlowSight a {}–{}",
                text_arg(args, "event_id")?,
                text_arg(args, "start_at")?,
                text_arg(args, "end_at")?
            ),
        ),
        "messages.draft" => paired(
            format!(
                "Save a {} draft to {}: {}",
                text_arg(args, "channel")?,
                text_arg(args, "recipient")?,
                text_arg(args, "body")?
            ),
            format!(
                "Guardar un borrador de {} para {}: {}",
                text_arg(args, "channel")?,
                text_arg(args, "recipient")?,
                text_arg(args, "body")?
            ),
        ),
        "messages.send" => {
            let id = text_arg(args, "draft_id")?;
            let data = state::read()?;
            let draft = data
                .drafts
                .iter()
                .find(|draft| draft.id == id)
                .ok_or("Draft not found.")?;
            if draft.sent_at.is_some() {
                return Err("That draft has already been sent.".into());
            }
            if draft.send_claim.is_some() && args["retry_if_uncertain"] != true {
                return Err("Delivery may have happened already. Check the provider, then request retry_if_uncertain if it did not arrive.".into());
            }
            paired(
                format!(
                    "{}Send {} message to {}{}: {}",
                    if draft.send_claim.is_some() {
                        "POSSIBLE DUPLICATE — confirm delivery did not happen. "
                    } else {
                        ""
                    },
                    draft.channel,
                    draft.recipient,
                    draft
                        .subject
                        .as_deref()
                        .map(|subject| format!(" (subject: {subject})"))
                        .unwrap_or_default(),
                    draft.body
                ),
                format!(
                    "{}Enviar un mensaje de {} a {}{}: {}",
                    if draft.send_claim.is_some() {
                        "POSIBLE DUPLICADO — confirma que no se ha entregado. "
                    } else {
                        ""
                    },
                    draft.channel,
                    draft.recipient,
                    draft
                        .subject
                        .as_deref()
                        .map(|subject| format!(" (asunto: {subject})"))
                        .unwrap_or_default(),
                    draft.body
                ),
            )
        }
        "project.update_status" => paired(
            format!(
                "Update {} item {} to {}{}",
                text_arg(args, "provider")?,
                text_arg(args, "item_id")?,
                text_arg(args, "status")?,
                args["note"]
                    .as_str()
                    .map(|note| format!(". Note: {note}"))
                    .unwrap_or_default()
            ),
            format!(
                "Actualizar el elemento {} de {} a {}{}",
                text_arg(args, "item_id")?,
                text_arg(args, "provider")?,
                text_arg(args, "status")?,
                args["note"]
                    .as_str()
                    .map(|note| format!(". Nota: {note}"))
                    .unwrap_or_default()
            ),
        ),
        "project.create_subtask" => paired(
            format!(
                "Create a {} child under {}: {}{}",
                text_arg(args, "provider")?,
                text_arg(args, "parent_id")?,
                text_arg(args, "title")?,
                args["description"]
                    .as_str()
                    .map(|body| format!(". Description: {body}"))
                    .unwrap_or_default(),
            ),
            format!(
                "Crear una subtarea de {} en {}: {}{}",
                text_arg(args, "provider")?,
                text_arg(args, "parent_id")?,
                text_arg(args, "title")?,
                args["description"]
                    .as_str()
                    .map(|body| format!(". Descripción: {body}"))
                    .unwrap_or_default()
            ),
        ),
        "desktop.open_resource" => paired(
            format!("Open {}", text_arg(args, "resource")?),
            format!("Abrir {}", text_arg(args, "resource")?),
        ),
        "automation.run_playbook" => {
            let playbook = text_arg(args, "playbook")?;
            let extra = match playbook {
                "deep_work" => format!(
                    " for {} minutes on '{}'{}{}",
                    integer_arg(args, "duration_minutes")?,
                    text_arg(args, "intention")?,
                    args["block_patterns"]
                        .as_array()
                        .map(|items| format!(", blocking {}", json!(items)))
                        .unwrap_or_default(),
                    args["resource"]
                        .as_str()
                        .map(|value| format!(", opening {value}"))
                        .unwrap_or_default()
                ),
                "recover_focus" => format!(
                    "{}{}",
                    args["tab_id"]
                        .as_i64()
                        .map(|id| format!(", close tab {id}"))
                        .unwrap_or_default(),
                    args["resource"]
                        .as_str()
                        .map(|value| format!(", open {value}"))
                        .unwrap_or_default()
                ),
                _ => String::new(),
            };
            let (label_es, extra_es) = match playbook {
                "deep_work" => (
                    "trabajo profundo",
                    format!(
                        " durante {} minutos en '{}'{}{}",
                        integer_arg(args, "duration_minutes")?,
                        text_arg(args, "intention")?,
                        args["block_patterns"]
                            .as_array()
                            .map(|items| format!(", bloqueando {}", json!(items)))
                            .unwrap_or_default(),
                        args["resource"]
                            .as_str()
                            .map(|value| format!(", abriendo {value}"))
                            .unwrap_or_default()
                    ),
                ),
                "recover_focus" => (
                    "recuperar la concentración",
                    format!(
                        "{}{}",
                        args["tab_id"]
                            .as_i64()
                            .map(|id| format!(", cerrar la pestaña {id}"))
                            .unwrap_or_default(),
                        args["resource"]
                            .as_str()
                            .map(|value| format!(", abrir {value}"))
                            .unwrap_or_default()
                    ),
                ),
                "end_of_day" => ("cierre del día", String::new()),
                _ => (playbook, String::new()),
            };
            paired(
                format!("Run {playbook}{extra}"),
                format!("Ejecutar {label_es}{extra_es}"),
            )
        }
        "memory.save_preference" => paired(
            format!(
                "Remember {}: {}",
                text_arg(args, "key")?,
                text_arg(args, "value")?
            ),
            format!(
                "Recordar {}: {}",
                text_arg(args, "key")?,
                text_arg(args, "value")?
            ),
        ),
        "memory.forget_preference" => paired(
            format!("Forget {}", text_arg(args, "key")?),
            format!("Olvidar {}", text_arg(args, "key")?),
        ),
        _ => paired(name.to_string(), name.to_string()),
    }
}

fn require_local_calendar_write(provider: Option<&str>) -> Result<(), String> {
    if provider.is_some() {
        Err("Connected-calendar writes are unavailable in this release. Select the FlowSight local calendar first.".into())
    } else {
        Ok(())
    }
}

pub fn execute(
    name: &str,
    args: &Value,
    app: AppHandle,
    agent_state: State<'_, AgentState>,
) -> Result<Value, String> {
    if !super::registry::is_enabled_name(name) {
        return Err("Connected-service writes are unavailable in this release.".into());
    }
    let _focus_guard = if name.starts_with("focus.") {
        Some(
            FOCUS_ACTION_LOCK
                .lock()
                .map_err(|error| error.to_string())?,
        )
    } else {
        None
    };
    match name {
        "focus.total_start" => super::total_focus::activate(
            text_arg(args, "intention")?.into(),
            total_preferences(args)?,
        ),
        "focus.total_end" => super::total_focus::end(),
        "focus.total_status" => super::total_focus::get_total_focus(),
        "focus.start" => {
            if state::read()?
                .focus
                .as_ref()
                .is_some_and(|focus| focus.status != "ended")
            {
                return Err(
                    "A focus block is already active. End it before starting another.".into(),
                );
            }
            let protection = text_arg(args, "protection")?;
            let intention = text_arg(args, "intention")?.trim().to_string();
            let minutes = integer_arg(args, "duration_minutes")? as u16;
            let blocked_patterns = if protection == "strict" {
                focus_patterns(args)
            } else {
                Vec::new()
            };
            let quiet_owned =
                apply_focus_protection(protection, &blocked_patterns, minutes as i64)?;
            let rollback = FocusBlock {
                intention: intention.clone(),
                duration_minutes: minutes,
                protection: protection.to_string(),
                started_at: Utc::now().to_rfc3339(),
                status: "ended".into(),
                paused_at: None,
                paused_seconds: 0,
                blocked_patterns: blocked_patterns.clone(),
                quiet_owned,
                last_active_at: None,
            };
            if let Err(error) = crate::agent::set_task_context(Some(intention.clone()), None) {
                let _ = release_focus_protection(&rollback);
                return Err(error);
            }
            if let Err(error) = crate::agent::start_monitoring(agent_state.clone()) {
                let _ = release_focus_protection(&rollback);
                return Err(error);
            }
            let focus = FocusBlock {
                intention,
                duration_minutes: minutes,
                protection: protection.to_string(),
                started_at: Utc::now().to_rfc3339(),
                status: "running".into(),
                paused_at: None,
                paused_seconds: 0,
                blocked_patterns,
                quiet_owned,
                last_active_at: Some(Utc::now().to_rfc3339()),
            };
            if let Err(error) = state::update(|data| {
                data.focus = Some(focus.clone());
                Ok(())
            }) {
                let _ = crate::agent::stop_monitoring(agent_state);
                let _ = release_focus_protection(&focus);
                return Err(error);
            }
            let _ = app.emit("local-agent-focus-changed", &focus);
            Ok(json!({"focus":focus}))
        }
        "focus.pause" => {
            let current = state::read()?.focus.ok_or("No focus block is active.")?;
            if current.status != "running" {
                return Err("The focus block is already paused.".into());
            }
            crate::agent::stop_monitoring(agent_state)?;
            let protection_warnings = release_focus_protection(&current);
            let focus = state::update(|data| {
                let focus = data.focus.as_mut().ok_or("No focus block is active.")?;
                focus.status = "paused".into();
                focus.paused_at = Some(Utc::now().to_rfc3339());
                focus.quiet_owned = false;
                Ok(focus.clone())
            })?;
            let _ = app.emit("local-agent-focus-changed", &focus);
            Ok(json!({"focus":focus,"protectionWarnings":protection_warnings}))
        }
        "focus.resume" => {
            let current = state::read()?.focus.ok_or("No focus block is paused.")?;
            if current.status != "paused" {
                return Err("No focus block is paused.".into());
            }
            let remaining =
                ((current.duration_minutes as i64 * 60 - elapsed_focus_seconds(&current)? + 59)
                    / 60)
                    .max(1);
            let quiet_owned =
                apply_focus_protection(&current.protection, &current.blocked_patterns, remaining)?;
            if let Err(error) = crate::agent::start_monitoring(agent_state.clone()) {
                let mut rollback = current.clone();
                rollback.quiet_owned = quiet_owned;
                let _ = release_focus_protection(&rollback);
                return Err(error);
            }
            let focus = state::update(|data| {
                let focus = data.focus.as_mut().ok_or("No focus block is paused.")?;
                let paused_at = focus
                    .paused_at
                    .as_deref()
                    .map(parse_time)
                    .transpose()?
                    .unwrap_or_else(Utc::now);
                focus.paused_seconds += (Utc::now() - paused_at).num_seconds().max(0);
                focus.paused_at = None;
                focus.status = "running".into();
                focus.quiet_owned = quiet_owned;
                focus.last_active_at = Some(Utc::now().to_rfc3339());
                Ok(focus.clone())
            });
            let focus = match focus {
                Ok(focus) => focus,
                Err(error) => {
                    let _ = crate::agent::stop_monitoring(agent_state);
                    let mut rollback = current;
                    rollback.quiet_owned = quiet_owned;
                    let _ = release_focus_protection(&rollback);
                    return Err(error);
                }
            };
            let _ = app.emit("local-agent-focus-changed", &focus);
            Ok(json!({"focus":focus}))
        }
        "focus.end" => {
            let current = state::read()?.focus.ok_or("No focus block is active.")?;
            if args["only_if_due"] == true
                && (current.status != "running"
                    || elapsed_focus_seconds(&current)? < current.duration_minutes as i64 * 60)
            {
                return Ok(json!({"skipped":true}));
            }
            if current.status == "running" {
                crate::agent::stop_monitoring(agent_state)?;
            }
            let protection_warnings = release_focus_protection(&current);
            let focus = state::update(|data| {
                let mut focus = data.focus.take().ok_or("No focus block is active.")?;
                focus.status = "ended".into();
                let digest = std::mem::take(&mut data.notification_digest);
                Ok((focus, digest))
            })?;
            let _ = app.emit("local-agent-focus-changed", &focus.0);
            if !focus.1.is_empty() {
                let _ = app.emit("local-agent-digest", &focus.1);
            }
            Ok(json!({"focus":focus.0,"digest":focus.1,"protectionWarnings":protection_warnings}))
        }
        "tasks.create" => {
            let now = Utc::now().to_rfc3339();
            let task = LocalTask {
                id: uuid::Uuid::new_v4().to_string(),
                title: text_arg(args, "title")?.trim().to_string(),
                status: "open".into(),
                priority: args["priority"].as_u64().unwrap_or(3) as u8,
                due_at: args["due_at"].as_str().map(str::to_string),
                created_at: now.clone(),
                updated_at: now,
            };
            state::update(|data| {
                data.tasks.push(task.clone());
                Ok(json!({"task":task}))
            })
        }
        "tasks.list" => {
            let data = state::read()?;
            Ok(
                json!({"source":"FlowSight local tasks","tasks":data.tasks.into_iter().rev().take(30).collect::<Vec<_>>()}),
            )
        }
        "tasks.update" | "tasks.complete" | "tasks.reprioritize" => {
            let id = text_arg(args, "task_id")?;
            state::update(|data| {
                let task = data
                    .tasks
                    .iter_mut()
                    .find(|task| task.id == id)
                    .ok_or("Task not found.")?;
                if name == "tasks.complete" {
                    task.status = "completed".into();
                } else if name == "tasks.reprioritize" {
                    task.priority = integer_arg(args, "priority")? as u8;
                } else {
                    let mut changed = false;
                    if let Some(title) = args["title"].as_str() {
                        task.title = title.trim().to_string();
                        changed = true;
                    }
                    if let Some(due) = args["due_at"].as_str() {
                        task.due_at = Some(due.to_string());
                        changed = true;
                    }
                    if let Some(status) = args["status"].as_str() {
                        task.status = status.to_string();
                        changed = true;
                    }
                    if !changed {
                        return Err("Provide a title, due time, or status to update.".into());
                    }
                }
                task.updated_at = Utc::now().to_rfc3339();
                Ok(json!({"task":task}))
            })
        }
        "calendar.get_availability" => {
            let (start, end) = interval(args)?;
            let data = state::read()?;
            let mut events = data.events.clone();
            let source = if let Some(ref provider) = data.calendar_provider {
                events.extend(super::external_calendar::busy_events(provider, start, end)?);
                format!("{provider} calendar and FlowSight local calendar")
            } else {
                "FlowSight local calendar".to_string()
            };
            let slots = free_slots(&events, start, end);
            Ok(json!({"source":source,"freeSlots":slots,"eventCount":events.len()}))
        }
        "calendar.list_events" => {
            let data = state::read()?;
            Ok(
                json!({"source":"FlowSight-owned calendar events","events":data.events.into_iter().rev().take(30).collect::<Vec<_>>()}),
            )
        }
        "calendar.get_current_event" => {
            let status = crate::calendar_companion::get_calendar_companion_status()?;
            Ok(
                json!({"current":status["current"],"overlapCount":status["overlapCount"],"checkedAt":status["checkedAt"]}),
            )
        }
        "calendar.create_event" => {
            let (start, end) = interval(args)?;
            let data = state::read()?;
            require_local_calendar_write(data.calendar_provider.as_deref())?;
            if data.events.iter().any(|existing| {
                parse_time(&existing.start_at).is_ok_and(|at| at < end)
                    && parse_time(&existing.end_at).is_ok_and(|at| at > start)
            }) {
                return Err("That time overlaps an existing FlowSight event.".into());
            }
            let provider = data.calendar_provider;
            if let Some(ref provider) = provider {
                if !super::external_calendar::busy_events(provider, start, end)?.is_empty() {
                    return Err("That time is busy in the connected calendar.".into());
                }
            }
            let external_id = if let Some(ref provider) = provider {
                Some(super::external_calendar::create_event(
                    provider,
                    text_arg(args, "title")?,
                    start,
                    end,
                )?)
            } else {
                None
            };
            let now = Utc::now().to_rfc3339();
            let event = LocalEvent {
                id: uuid::Uuid::new_v4().to_string(),
                title: text_arg(args, "title")?.trim().to_string(),
                start_at: start.to_rfc3339(),
                end_at: end.to_rfc3339(),
                created_at: now.clone(),
                updated_at: now,
                provider: provider.clone(),
                external_id,
            };
            let saved = state::update(|data| {
                if data.events.iter().any(|existing| {
                    parse_time(&existing.start_at).is_ok_and(|at| at < end)
                        && parse_time(&existing.end_at).is_ok_and(|at| at > start)
                }) {
                    return Err("That time overlaps an existing FlowSight event.".into());
                }
                data.events.push(event.clone());
                Ok(
                    json!({"event":event,"source":provider.as_deref().unwrap_or("FlowSight local calendar")}),
                )
            });
            if saved.is_err() {
                if let (Some(provider), Some(id)) = (&event.provider, &event.external_id) {
                    if super::external_calendar::delete_event(provider, id).is_err() {
                        return Err("The calendar event was created, but its local record could not be saved. Check the connected calendar.".into());
                    }
                }
            }
            saved
        }
        "calendar.move_event" => {
            let id = text_arg(args, "event_id")?;
            let (start, end) = interval(args)?;
            let data = state::read()?;
            let existing = data
                .events
                .iter()
                .find(|event| event.id == id)
                .cloned()
                .ok_or("Event not found or not owned by FlowSight.")?;
            require_local_calendar_write(existing.provider.as_deref())?;
            if data.events.iter().any(|event| {
                event.id != id
                    && parse_time(&event.start_at).is_ok_and(|at| at < end)
                    && parse_time(&event.end_at).is_ok_and(|at| at > start)
            }) {
                return Err("That time overlaps an existing FlowSight event.".into());
            }
            if let Some(ref provider) = existing.provider {
                let external_id = existing
                    .external_id
                    .as_deref()
                    .ok_or("The connected event ID is missing.")?;
                super::external_calendar::move_event(provider, external_id, start, end)?;
            }
            let updated = state::update(|data| {
                if data.events.iter().any(|existing| {
                    existing.id != id
                        && parse_time(&existing.start_at).is_ok_and(|at| at < end)
                        && parse_time(&existing.end_at).is_ok_and(|at| at > start)
                }) {
                    return Err("That time overlaps an existing FlowSight event.".into());
                }
                let event = data
                    .events
                    .iter_mut()
                    .find(|event| event.id == id)
                    .ok_or("Event not found or not owned by FlowSight.")?;
                event.start_at = start.to_rfc3339();
                event.end_at = end.to_rfc3339();
                event.updated_at = Utc::now().to_rfc3339();
                Ok(
                    json!({"event":event,"source":event.provider.as_deref().unwrap_or("FlowSight local calendar")}),
                )
            });
            if updated.is_err() {
                if let (Some(provider), Some(remote_id)) =
                    (&existing.provider, &existing.external_id)
                {
                    let old_start = parse_time(&existing.start_at)?;
                    let old_end = parse_time(&existing.end_at)?;
                    if super::external_calendar::move_event(provider, remote_id, old_start, old_end)
                        .is_err()
                    {
                        return Err("The connected calendar moved the event, but FlowSight could not save the new time. Check the calendar.".into());
                    }
                }
            }
            updated
        }
        "notifications.digest" => {
            let items = state::read()?.notification_digest;
            Ok(json!({"count":items.len(),"items":items}))
        }
        "messages.draft" => {
            let channel = text_arg(args, "channel")?;
            let recipient = text_arg(args, "recipient")?.trim();
            if channel == "email"
                && (!recipient.contains('@')
                    || recipient.contains(' ')
                    || recipient.contains('\r')
                    || recipient.contains('\n'))
            {
                return Err("Use a valid email address for the recipient.".into());
            }
            let subject = args["subject"].as_str().map(str::to_string);
            if subject
                .as_deref()
                .is_some_and(|value| value.contains('\r') || value.contains('\n'))
            {
                return Err("The subject cannot contain line breaks.".into());
            }
            let draft = MessageDraft {
                id: uuid::Uuid::new_v4().to_string(),
                channel: channel.to_string(),
                recipient: recipient.to_string(),
                body: text_arg(args, "body")?.trim().to_string(),
                created_at: Utc::now().to_rfc3339(),
                subject,
                sent_at: None,
                send_claim: None,
            };
            state::update(|data| {
                data.drafts.push(draft.clone());
                Ok(json!({"draft":draft,"sent":false}))
            })
        }
        "messages.list_drafts" => {
            let data = state::read()?;
            Ok(json!({"drafts":data.drafts.into_iter().rev().take(20).collect::<Vec<_>>()}))
        }
        "project.get_current_work" => {
            let data = state::read()?;
            let task = data
                .tasks
                .iter()
                .filter(|task| task.status != "completed")
                .min_by_key(|task| task.priority);
            let jira = crate::jira::fetch_jira_tasks_blocking();
            let linear = crate::linear::fetch_linear_tasks_blocking();
            Ok(json!({
                "source":"FlowSight local state and connected Jira/Linear work",
                "focus":data.focus,"topTask":task,
                "currentIntention":crate::telemetry::selected_task_for_reminder(),
                "jiraIssues":jira.as_ref().ok().map(|items| items.iter().take(10).collect::<Vec<_>>()),
                "jiraError":jira.as_ref().err(),
                "linearIssues":linear.as_ref().ok().map(|items| items.iter().take(10).collect::<Vec<_>>()),
                "linearError":linear.as_ref().err(),
            }))
        }
        "project.update_status" => super::projects::update_status(
            text_arg(args, "provider")?,
            text_arg(args, "item_id")?,
            text_arg(args, "status")?,
            args["note"].as_str(),
        ),
        "project.create_subtask" => super::projects::create_subtask(
            text_arg(args, "provider")?,
            text_arg(args, "parent_id")?,
            text_arg(args, "title")?,
            args["description"].as_str(),
        ),
        "project.prepare_pr_description" => {
            let lines = |name: &str| -> Vec<String> {
                args[name]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .filter_map(Value::as_str)
                    .map(|line| format!("- {}", line.trim()))
                    .collect()
            };
            let mut body = format!("## Summary\n{}", lines("summary").join("\n"));
            if args["tests"].is_array() {
                body.push_str(&format!("\n\n## Tests\n{}", lines("tests").join("\n")));
            }
            if args["risks"].is_array() {
                body.push_str(&format!("\n\n## Risks\n{}", lines("risks").join("\n")));
            }
            Ok(json!({"title":text_arg(args, "title")?.trim(),"body":body,"published":false}))
        }
        "desktop.open_resource" => {
            let resource = text_arg(args, "resource")?;
            let target = if let Ok(url) = url::Url::parse(resource) {
                if url.scheme() != "https" && url.scheme() != "http" {
                    return Err("Only HTTP and HTTPS links can be opened.".into());
                }
                resource.to_string()
            } else {
                let path = std::path::Path::new(resource);
                if !path.is_absolute() || !path.exists() {
                    return Err("Choose an existing absolute file or folder path.".into());
                }
                resource.to_string()
            };
            open::that(&target).map_err(|error| format!("Could not open resource: {error}"))?;
            Ok(json!({"opened":target}))
        }
        "memory.save_preference" => {
            let key = text_arg(args, "key")?.trim().to_lowercase();
            let value = text_arg(args, "value")?.trim().to_string();
            state::update(|data| {
                data.preferences.insert(
                    key.clone(),
                    SavedPreference {
                        value: value.clone(),
                        updated_at: Utc::now().to_rfc3339(),
                    },
                );
                Ok(json!({"key":key,"value":value}))
            })
        }
        "memory.forget_preference" => {
            let key = text_arg(args, "key")?.trim().to_lowercase();
            state::update(|data| {
                let forgotten = data.preferences.remove(&key).is_some();
                Ok(json!({"key":key,"forgotten":forgotten}))
            })
        }
        "memory.list_preferences" => {
            let data = state::read()?;
            Ok(json!({"preferences":data.preferences}))
        }
        "messages.send" => {
            let id = text_arg(args, "draft_id")?;
            let snapshot = state::read()?;
            let preview_draft = snapshot
                .drafts
                .iter()
                .find(|draft| draft.id == id)
                .ok_or("Draft not found.")?;
            super::messaging::preflight(preview_draft, snapshot.email_provider.as_deref())?;
            let claim = uuid::Uuid::new_v4().to_string();
            let (draft, email_provider) = state::update(|data| {
                let email_provider = data.email_provider.clone();
                let draft = data
                    .drafts
                    .iter_mut()
                    .find(|draft| draft.id == id)
                    .ok_or("Draft not found.")?;
                if draft.sent_at.is_some() {
                    return Err("That draft has already been sent.".into());
                }
                if draft.send_claim.is_some() && args["retry_if_uncertain"] != true {
                    return Err(
                        "Delivery may already have happened. Check the provider before retrying."
                            .into(),
                    );
                }
                draft.send_claim = Some(claim.clone());
                Ok((draft.clone(), email_provider))
            })?;
            let result = super::messaging::send(&draft, email_provider.as_deref())?;
            state::update(|data| {
                let draft = data
                    .drafts
                    .iter_mut()
                    .find(|item| item.id == id)
                    .ok_or("Draft not found after sending.")?;
                draft.sent_at = Some(Utc::now().to_rfc3339());
                Ok(())
            })?;
            Ok(result)
        }
        "browser.block"
        | "browser.unblock"
        | "browser.close_tab"
        | "browser.restore_tab"
        | "browser.list_tabs" => super::browser_bridge::execute(name, args),
        "system.set_dnd" => {
            if args["enabled"] == true {
                super::system_quiet::enable(args["duration_minutes"].as_i64())
            } else {
                super::system_quiet::disable()
            }
        }
        "automation.run_playbook" => {
            let playbook = text_arg(args, "playbook")?;
            let mut steps = Vec::new();
            match playbook {
                "deep_work" => {
                    let patterns = focus_patterns(args);
                    let protection = if patterns.is_empty() {
                        "standard"
                    } else {
                        "strict"
                    };
                    let focus = execute(
                        "focus.start",
                        &json!({
                            "intention": text_arg(args, "intention")?,
                            "duration_minutes": integer_arg(args, "duration_minutes")?,
                            "protection": protection,
                            "block_patterns": patterns,
                        }),
                        app.clone(),
                        agent_state.clone(),
                    )?;
                    steps.push(json!({"tool":"focus.start","result":focus}));
                    if let Some(resource) = args["resource"].as_str() {
                        match execute(
                            "desktop.open_resource",
                            &json!({"resource":resource}),
                            app,
                            agent_state,
                        ) {
                            Ok(result) => {
                                steps.push(json!({"tool":"desktop.open_resource","result":result}))
                            }
                            Err(error) => {
                                steps.push(json!({"tool":"desktop.open_resource","error":error}))
                            }
                        }
                    }
                }
                "end_of_day" => {
                    if state::read()?.focus.is_some() {
                        steps.push(json!({"tool":"focus.end","result":execute("focus.end", &json!({}), app.clone(), agent_state.clone())?}));
                    } else {
                        steps.push(json!({"tool":"notifications.digest","result":execute("notifications.digest", &json!({}), app.clone(), agent_state.clone())?}));
                    }
                    let tasks: Vec<_> = state::read()?
                        .tasks
                        .into_iter()
                        .filter(|task| task.status != "completed")
                        .collect();
                    steps.push(json!({"tomorrowPlan":tasks}));
                }
                "recover_focus" => {
                    if let Some(tab_id) = args["tab_id"].as_i64() {
                        steps.push(json!({"tool":"browser.close_tab","result":execute("browser.close_tab", &json!({"tab_id":tab_id}), app.clone(), agent_state.clone())?}));
                    }
                    if let Some(resource) = args["resource"].as_str() {
                        match execute(
                            "desktop.open_resource",
                            &json!({"resource":resource}),
                            app.clone(),
                            agent_state.clone(),
                        ) {
                            Ok(result) => {
                                steps.push(json!({"tool":"desktop.open_resource","result":result}))
                            }
                            Err(error) => {
                                steps.push(json!({"tool":"desktop.open_resource","error":error}))
                            }
                        }
                    }
                    let intention = args["intention"]
                        .as_str()
                        .map(str::to_string)
                        .or_else(|| {
                            state::read()
                                .ok()
                                .and_then(|data| data.focus.map(|focus| focus.intention))
                        })
                        .unwrap_or_else(|| "the current task".into());
                    steps.push(json!({"microplan":[
                        format!("Return to {intention}"),
                        "Choose the smallest next step that can be finished in three minutes.",
                        "Do that step before opening another app."
                    ]}));
                }
                _ => return Err("Unsupported playbook.".into()),
            }
            Ok(json!({"playbook":playbook,"steps":steps}))
        }
        _ => Err("Unsupported local agent tool.".into()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn paired_previews_preserve_user_text_and_arguments() {
        let title = "Review · Break · investigación 日本語";
        let args = json!({"title":title});
        let (en, es) = preview("tasks.create", &args).unwrap();
        assert_eq!(en, format!("Create task: {title}"));
        assert_eq!(es, format!("Crear tarea: {title}"));
        assert_eq!(args, json!({"title":title}));
        let body = "Review at 10:00. Break luego. No traducir 日本語";
        let args = json!({"channel":"email", "recipient":"test@example.invalid", "body":body});
        let (en, es) = preview("messages.draft", &args).unwrap();
        for summary in [en, es] {
            assert!(summary.ends_with(body));
            assert!(summary.contains("test@example.invalid"));
        }
    }

    #[test]
    fn paired_previews_describe_the_same_protections_and_interval() {
        let args = json!({"duration_minutes":75,"intention":"PLE · 日本語","protection":"strict","block_patterns":["example.invalid"]});
        let (en, es) = preview("focus.start", &args).unwrap();
        for summary in [en, es] {
            assert!(summary.contains("75"));
            assert!(summary.contains("PLE · 日本語"));
            assert!(summary.contains("Windows"));
            assert!(summary.contains("example.invalid"));
        }
        let args = json!({"title":"Review", "start_at":"2026-10-01T10:35:00+02:00", "end_at":"2026-10-01T11:50:00+02:00"});
        let (en, es) = preview("calendar.create_event", &args).unwrap();
        assert!(en.contains("FlowSight local calendar"));
        assert!(es.contains("calendario local de FlowSight"));
        for summary in [en, es] {
            assert!(summary.contains("Review"));
            assert!(summary.contains("2026-10-01T10:35:00+02:00"));
            assert!(summary.contains("2026-10-01T11:50:00+02:00"));
        }
        assert!(preview("focus.start", &json!({"protection":"unknown"})).is_err());
    }

    #[test]
    fn calendar_writes_are_local_only_until_connectors_are_verified() {
        assert!(require_local_calendar_write(None).is_ok());
        assert!(require_local_calendar_write(Some("google")).is_err());
        assert!(require_local_calendar_write(Some("microsoft")).is_err());
    }

    #[test]
    fn availability_merges_overlapping_events() {
        let event = |id: &str, start: &str, end: &str| LocalEvent {
            id: id.into(),
            title: id.into(),
            start_at: start.into(),
            end_at: end.into(),
            created_at: String::new(),
            updated_at: String::new(),
            provider: None,
            external_id: None,
        };
        let events = vec![
            event("a", "2026-10-01T09:00:00Z", "2026-10-01T10:00:00Z"),
            event("b", "2026-10-01T09:30:00Z", "2026-10-01T11:00:00Z"),
        ];
        let slots = free_slots(
            &events,
            parse_time("2026-10-01T08:00:00Z").unwrap(),
            parse_time("2026-10-01T12:00:00Z").unwrap(),
        );
        assert_eq!(slots.len(), 2);
        assert_eq!(slots[0]["endAt"], "2026-10-01T09:00:00+00:00");
        assert_eq!(slots[1]["startAt"], "2026-10-01T11:00:00+00:00");
    }
}
