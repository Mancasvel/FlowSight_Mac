//! Local Qwen tool planner. A model response can suggest one named operation;
//! the host validates it and holds every mutation until the user confirms it.

mod actions;
pub mod browser_bridge;
pub mod connectors;
mod external_calendar;
mod messaging;
mod projects;
mod registry;
pub mod session_calendar;
pub mod session_plan;
pub mod state;
mod system_quiet;
pub mod total_focus;

use std::sync::Mutex;
use std::time::{Duration, Instant};

use reqwest::blocking::Client;
use serde::Serialize;
use serde_json::{json, Value};
use tauri::{AppHandle, Manager, State};

use crate::agent::AgentState;
use crate::vision_model::LLAMA_CHAT_MODEL_ID;

const MAX_USER_MESSAGE_CHARS: usize = 1200;
const PROPOSAL_LIFETIME: Duration = Duration::from_secs(5 * 60);
static PENDING: Mutex<Vec<PendingAction>> = Mutex::new(Vec::new());

struct PendingAction {
    id: String,
    tool: String,
    arguments: Value,
    summary: String,
    browser_tab_url: Option<String>,
    expires_at: Instant,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ActionProposal {
    id: String,
    tool: String,
    summary: String,
    localized_summary: Value,
    arguments: Value,
    expires_in_seconds: u64,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentTurn {
    message: String,
    proposal: Option<ActionProposal>,
    result: Option<Value>,
}

fn parse_arguments(value: &Value) -> Result<Value, String> {
    match value {
        Value::String(raw) => serde_json::from_str(raw)
            .map_err(|_| "The local model returned malformed tool arguments.".to_string()),
        Value::Object(_) => Ok(value.clone()),
        _ => Err("The local model returned malformed tool arguments.".into()),
    }
}

fn proposal_for(spec: &registry::ToolSpec, arguments: Value) -> Result<ActionProposal, String> {
    let mut arguments = arguments;
    if spec.name == "focus.total_start" {
        let preferences = state::read()?.total_focus_preferences;
        let fields = arguments
            .as_object_mut()
            .ok_or("Tool arguments must be an object.")?;
        fields
            .entry("duration_minutes")
            .or_insert(json!(preferences.duration_minutes));
        fields
            .entry("patterns")
            .or_insert(json!(preferences.patterns));
        fields
            .entry("exceptions")
            .or_insert(json!(preferences.exceptions));
    }
    registry::validate(spec, &arguments)?;
    let id = uuid::Uuid::new_v4().to_string();
    let (mut summary, mut summary_es) = actions::preview(spec.name, &arguments)?;
    let tab_id = if spec.name == "browser.close_tab"
        || (spec.name == "automation.run_playbook" && arguments["playbook"] == "recover_focus")
    {
        arguments["tab_id"].as_i64()
    } else {
        None
    };
    let browser_tab_url = tab_id.map(actions::browser_tab_url).transpose()?;
    if let Some(ref url) = browser_tab_url {
        summary.push_str(&format!(". Target URL: {url}"));
        summary_es.push_str(&format!(". URL de destino: {url}"));
    }
    let mut pending = PENDING.lock().map_err(|error| error.to_string())?;
    pending.retain(|item| item.expires_at > Instant::now());
    if pending.len() >= 20 {
        pending.remove(0);
    }
    pending.push(PendingAction {
        id: id.clone(),
        tool: spec.name.to_string(),
        arguments: arguments.clone(),
        summary: summary.clone(),
        browser_tab_url,
        expires_at: Instant::now() + PROPOSAL_LIFETIME,
    });
    Ok(ActionProposal {
        id,
        tool: spec.name.to_string(),
        localized_summary: json!({"en": summary, "es": summary_es}),
        summary,
        arguments,
        expires_in_seconds: PROPOSAL_LIFETIME.as_secs(),
    })
}

fn decide_from_model(
    response: &Value,
    offered: &[registry::ToolSpec],
) -> Result<(String, Option<(registry::ToolSpec, Value)>), String> {
    let message = &response["choices"][0]["message"];
    if !message.is_object() {
        return Err("The local model did not return a message.".into());
    }
    let content = message["content"].as_str().unwrap_or("").trim().to_string();
    let Some(calls) = message["tool_calls"].as_array() else {
        return Ok((content, None));
    };
    if calls.is_empty() {
        return Ok((content, None));
    }
    if calls.len() != 1 {
        return Err("The local model requested multiple actions at once. Please ask for one action at a time.".into());
    }
    let call = &calls[0]["function"];
    let name = call["name"]
        .as_str()
        .ok_or("The local model omitted the tool name.")?;
    let spec = offered
        .iter()
        .find(|spec| spec.model_name == name)
        .cloned()
        .ok_or("The local model requested a tool that was not offered for this request.")?;
    let arguments = parse_arguments(&call["arguments"])?;
    registry::validate(&spec, &arguments)?;
    Ok((content, Some((spec, arguments))))
}

fn routed_family(response: &Value) -> Result<&'static str, String> {
    let calls = response["choices"][0]["message"]["tool_calls"]
        .as_array()
        .ok_or("The local model could not identify the request type. Please rephrase it.")?;
    if calls.len() != 1 || calls[0]["function"]["name"] != "select_tool_family" {
        return Err("The local model could not identify one request type. Please ask for one action at a time.".into());
    }
    let args = parse_arguments(&calls[0]["function"]["arguments"])?;
    let family = args["family"]
        .as_str()
        .ok_or("The local model did not choose a request type.")?;
    registry::TOOL_FAMILIES
        .iter()
        .copied()
        .find(|candidate| *candidate == family)
        .ok_or("The local model chose an unsupported request type.".into())
}

fn recent_messages(data: &state::AgentData, max_chars: usize) -> Vec<Value> {
    data.conversation
        .iter()
        .rev()
        .take(4)
        .rev()
        .filter(|item| item.role == "user" || item.role == "assistant")
        .map(|item| {
            json!({"role": item.role, "content": item.content.chars().take(max_chars).collect::<String>()})
        })
        .collect()
}

fn context_for_family(family: &str, data: &state::AgentData) -> Value {
    let mut context = json!({
        "now": chrono::Local::now().to_rfc3339(),
        "currentIntention": crate::telemetry::selected_task_for_reminder()
            .map(|value| value.chars().take(160).collect::<String>()),
    });
    let fields = context.as_object_mut().expect("context is an object");
    if matches!(family, "focus" | "system" | "notifications" | "automation") {
        fields.insert(
            "focus".into(),
            json!(data.focus.as_ref().map(|focus| json!({
                "intention":focus.intention,
                "durationMinutes":focus.duration_minutes,
                "protection":focus.protection,
                "startedAt":focus.started_at,
                "status":focus.status,
                "pausedAt":focus.paused_at,
                "blockedPatternCount":focus.blocked_patterns.len(),
            }))),
        );
    }
    if matches!(family, "tasks" | "focus" | "automation") {
        let count = if family == "tasks" { 10 } else { 5 };
        fields.insert(
            "tasks".into(),
            json!(data
                .tasks
                .iter()
                .rev()
                .take(count)
                .map(|task| json!({
                    "id":task.id,"title":task.title.chars().take(120).collect::<String>(),
                    "status":task.status,"priority":task.priority,"dueAt":task.due_at,
                }))
                .collect::<Vec<_>>()),
        );
    }
    if family == "calendar" {
        if let Ok(status) = crate::calendar_companion::get_calendar_companion_status() {
            let event = &status["current"];
            fields.insert("currentCalendarEvent".into(), if event.is_object() {
                json!({
                    "title": event["title"].as_str().unwrap_or("").chars().take(120).collect::<String>(),
                    "provider": event["provider"],
                    "startAt": event["startAt"],
                    "endAt": event["endAt"],
                    "organizer": event["organizer"],
                })
            } else { Value::Null });
            fields.insert(
                "overlappingCalendarEvents".into(),
                status["overlapCount"].clone(),
            );
        }
        fields.insert(
            "events".into(),
            json!(data
                .events
                .iter()
                .rev()
                .take(8)
                .map(|event| json!({
                    "id":event.id,"title":event.title.chars().take(120).collect::<String>(),
                    "startAt":event.start_at,"endAt":event.end_at,"provider":event.provider,
                }))
                .collect::<Vec<_>>()),
        );
        fields.insert("calendarProvider".into(), json!(data.calendar_provider));
    }
    if family == "messages" {
        fields.insert(
            "drafts".into(),
            json!(data
                .drafts
                .iter()
                .rev()
                .take(6)
                .map(|draft| json!({
                    "id":draft.id,"channel":draft.channel,"recipient":draft.recipient,
                    "subject":draft.subject,"sentAt":draft.sent_at,
                }))
                .collect::<Vec<_>>()),
        );
        fields.insert("emailProvider".into(), json!(data.email_provider));
    }
    if matches!(family, "memory" | "focus" | "automation") {
        let mut recent_preferences: Vec<_> = data.preferences.iter().collect();
        recent_preferences.sort_by(|a, b| b.1.updated_at.cmp(&a.1.updated_at));
        let count = if family == "memory" { 10 } else { 5 };
        fields.insert(
            "preferences".into(),
            json!(recent_preferences
                .into_iter()
                .take(count)
                .map(|(key, value)| json!({
                    "key":key,"value":value.value.chars().take(120).collect::<String>()
                }))
                .collect::<Vec<_>>()),
        );
    }
    context
}

fn send_model_request(client: &Client, url: &str, body: &Value) -> Result<Value, String> {
    let response = client
        .post(url)
        .json(body)
        .send()
        .map_err(|error| format!("Could not reach local Qwen: {error}"))?;
    if !response.status().is_success() {
        return Err(format!("Local Qwen returned HTTP {}.", response.status()));
    }
    response
        .json()
        .map_err(|_| "Local Qwen returned invalid JSON.".into())
}

fn request_model(
    message: &str,
    app: AppHandle,
    state: State<'_, AgentState>,
) -> Result<(Value, Vec<registry::ToolSpec>), String> {
    crate::agent::ensure_local_llm_ready(app, state)?;
    let url = crate::llama_port::managed_chat_completions_url()
        .ok_or("The local AI server is unavailable.")?;
    let data = state::read()?;
    let client = Client::builder()
        .no_proxy()
        .timeout(Duration::from_secs(90))
        .build()
        .map_err(|error| error.to_string())?;
    let mut route_messages = vec![json!({"role":"system","content":registry::TOOL_FAMILY_PROMPT})];
    route_messages.extend(recent_messages(&data, 250));
    route_messages.push(json!({"role":"user","content":message}));
    let route_body = json!({
        "model": LLAMA_CHAT_MODEL_ID,
        "messages": route_messages,
        "tools": [registry::family_router_definition()],
        "tool_choice": "required",
        "temperature": 0,
        "max_tokens": 96,
        "stream": false
    });
    let family = routed_family(&send_model_request(&client, &url, &route_body)?)?;
    let offered = registry::specs_for_family(family);
    let context = context_for_family(family, &data);
    let mut messages = vec![
        json!({"role":"system","content":"You are FlowSight's on-device action assistant. Suggest at most one function call per turn. Use only the user's explicit request and the available tools. Never claim an action happened before FlowSight confirms its result. Ask a short clarification if arguments are missing. In this release, messages cannot be sent and connected calendar events cannot be changed by agent tools; FlowSight's separately consented mini-report automation may append a recap after an event. Use exact IDs from local context. Browser tab IDs require browser.list_tabs first. All times need an explicit timezone offset. Keep replies brief."}),
        json!({"role":"system","content":format!("Current FlowSight context: {context}")}),
        json!({"role":"system","content":crate::language::copy(
            "Write replies in English. Preserve user text, task titles, message bodies, IDs, paths and tool argument enum values exactly as supplied.",
            "Write replies in Spanish. Preserve user text, task titles, message bodies, IDs, paths and tool argument enum values exactly as supplied.",
        )}),
    ];
    messages.extend(recent_messages(&data, 350));
    messages.push(json!({"role":"user","content":message}));
    let mut body = json!({
        "model": LLAMA_CHAT_MODEL_ID,
        "messages": messages,
        "temperature": 0,
        "max_tokens": 700,
        "stream": false
    });
    if !offered.is_empty() {
        body["tools"] = json!(offered
            .iter()
            .map(registry::model_definition)
            .collect::<Vec<_>>());
        body["tool_choice"] = json!("auto");
    }
    Ok((send_model_request(&client, &url, &body)?, offered))
}

#[tauri::command]
pub fn get_local_agent_tools() -> Vec<Value> {
    registry::enabled_specs()
        .iter()
        .map(|spec| {
            json!({
                "name": spec.name,
                "description": spec.description,
                "confirmationRequired": spec.confirmation,
            })
        })
        .collect()
}

#[tauri::command]
pub async fn propose_local_agent_tool(
    name: String,
    arguments: Value,
) -> Result<ActionProposal, String> {
    tauri::async_runtime::spawn_blocking(move || propose_local_agent_tool_blocking(name, arguments))
        .await
        .map_err(|error| format!("Local agent worker failed: {error}"))?
}

fn propose_local_agent_tool_blocking(
    name: String,
    arguments: Value,
) -> Result<ActionProposal, String> {
    let spec = registry::enabled_by_public_name(&name)
        .ok_or("This local agent tool is unavailable in this release.")?;
    if !spec.confirmation {
        return Err("This tool does not need a confirmation proposal.".into());
    }
    proposal_for(&spec, arguments)
}

#[tauri::command]
pub async fn ask_local_agent(app: AppHandle, message: String) -> Result<AgentTurn, String> {
    tauri::async_runtime::spawn_blocking(move || ask_local_agent_blocking(app, message))
        .await
        .map_err(|error| format!("Local agent worker failed: {error}"))?
}

fn ask_local_agent_blocking(app: AppHandle, message: String) -> Result<AgentTurn, String> {
    let message = message.trim();
    if message.is_empty() || message.chars().count() > MAX_USER_MESSAGE_CHARS {
        return Err("Write a request of up to 1,200 characters.".into());
    }
    let state = app.state::<AgentState>();
    let (response, offered) = request_model(message, app.clone(), state.clone())?;
    let (text, choice) = decide_from_model(&response, &offered)?;
    state::append_conversation("user", message)?;
    let Some((spec, arguments)) = choice else {
        let reply = if text.is_empty() {
            "I need a little more detail to act.".to_string()
        } else {
            text
        };
        state::append_conversation("assistant", &reply)?;
        return Ok(AgentTurn {
            message: reply,
            proposal: None,
            result: None,
        });
    };
    if spec.confirmation {
        let proposal = proposal_for(&spec, arguments)?;
        state::append_conversation(
            "assistant",
            &format!("Proposed {}: {}", proposal.tool, proposal.summary),
        )?;
        return Ok(AgentTurn {
            message: "Please review this action before it runs.".into(),
            proposal: Some(proposal),
            result: None,
        });
    }
    let result = actions::execute(spec.name, &arguments, app.clone(), state)?;
    state::append_conversation("assistant", &format!("{} result: {result}", spec.name))?;
    Ok(AgentTurn {
        message: "Here is what I found.".into(),
        proposal: None,
        result: Some(result),
    })
}

#[tauri::command]
pub async fn confirm_local_agent_action(app: AppHandle, id: String) -> Result<Value, String> {
    tauri::async_runtime::spawn_blocking(move || confirm_local_agent_action_blocking(app, id))
        .await
        .map_err(|error| format!("Local agent worker failed: {error}"))?
}

fn confirm_local_agent_action_blocking(app: AppHandle, id: String) -> Result<Value, String> {
    let state = app.state::<AgentState>();
    let pending = {
        let mut queue = PENDING.lock().map_err(|error| error.to_string())?;
        let index = queue
            .iter()
            .position(|item| item.id == id)
            .ok_or("That action is no longer pending.")?;
        queue.remove(index)
    };
    if pending.expires_at <= Instant::now() {
        return Err("That action expired. Ask the local agent again.".into());
    }
    let spec = registry::enabled_by_public_name(&pending.tool)
        .ok_or("That action is unavailable in this release.")?;
    if !spec.confirmation {
        return Err("That action does not require confirmation.".into());
    }
    registry::validate(&spec, &pending.arguments)?;
    if let (Some(expected), Some(tab_id)) = (
        &pending.browser_tab_url,
        pending.arguments["tab_id"].as_i64(),
    ) {
        if actions::browser_tab_url(tab_id)? != *expected {
            return Err("The browser tab changed since approval. Review a new action.".into());
        }
    }
    let result = actions::execute(spec.name, &pending.arguments, app.clone(), state);
    if let Ok(ref value) = result {
        let _ = state::append_conversation(
            "assistant",
            &format!("Confirmed {} result: {value}", spec.name),
        );
    }
    let status = if result.is_ok() {
        "completed"
    } else {
        "failed"
    };
    let summary = pending.summary;
    let _ = state::update(|data| {
        data.audit.push(state::ActionAudit {
            id: uuid::Uuid::new_v4().to_string(),
            tool: spec.name.to_string(),
            summary,
            status: status.to_string(),
            created_at: chrono::Utc::now().to_rfc3339(),
        });
        Ok(())
    });
    result
}

#[tauri::command]
pub fn cancel_local_agent_action(id: String) -> Result<(), String> {
    let mut queue = PENDING.lock().map_err(|error| error.to_string())?;
    queue.retain(|item| item.id != id);
    Ok(())
}

#[tauri::command]
pub fn get_local_agent_data() -> Result<state::AgentData, String> {
    let owner = crate::calendar_companion::session_owner();
    Ok(session_plan::owner_view(state::read()?, owner.as_deref()))
}

#[tauri::command]
pub async fn control_local_focus_block(app: AppHandle, action: String) -> Result<Value, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let tool = match action.as_str() {
            "pause" => "focus.pause",
            "resume" => "focus.resume",
            "end" => "focus.end",
            _ => return Err("Unknown focus control.".into()),
        };
        let state = app.state::<AgentState>();
        if action == "resume" {
            crate::agent::ensure_local_llm_ready(app.clone(), state.clone())?;
        }
        let result = actions::execute(tool, &json!({}), app.clone(), state)?;
        let _ = state::update(|data| {
            data.audit.push(state::ActionAudit {
                id: uuid::Uuid::new_v4().to_string(),
                tool: tool.into(),
                summary: format!("Timer control: {action}"),
                status: "completed".into(),
                created_at: chrono::Utc::now().to_rfc3339(),
            });
            Ok(())
        });
        Ok(result)
    })
    .await
    .map_err(|error| format!("Focus control worker failed: {error}"))?
}

pub fn start_maintenance(app: AppHandle) {
    if let Ok(data) = state::read() {
        if data.quiet.is_some() {
            if let Err(error) = system_quiet::disable() {
                log::warn!("Could not restore notifications after restart: {error}");
            }
        }
        if data
            .focus
            .as_ref()
            .is_some_and(|focus| focus.status == "running")
        {
            let _ = state::update(|data| {
                if let Some(focus) = data.focus.as_mut() {
                    focus.status = "paused".into();
                    focus.paused_at = focus
                        .last_active_at
                        .clone()
                        .or_else(|| Some(chrono::Utc::now().to_rfc3339()));
                    focus.quiet_owned = false;
                }
                Ok(())
            });
        }
    }
    std::thread::spawn(move || loop {
        std::thread::sleep(Duration::from_secs(15));
        if let Err(error) = actions::expire_focus_if_due(app.clone(), app.state::<AgentState>()) {
            log::warn!("Could not expire focus block: {error}");
        }
        if let Err(error) = system_quiet::restore_if_expired() {
            log::warn!("Could not restore Windows notification setting: {error}");
        }
        if state::read()
            .ok()
            .and_then(|data| data.focus)
            .is_some_and(|focus| focus.status == "running")
        {
            let _ = state::update(|data| {
                if let Some(focus) = data.focus.as_mut() {
                    if focus.status == "running" {
                        focus.last_active_at = Some(chrono::Utc::now().to_rfc3339());
                    }
                }
                Ok(())
            });
        }
    });
}

pub fn restore_on_exit() {
    if state::read().ok().and_then(|data| data.quiet).is_some() {
        if let Err(error) = system_quiet::disable() {
            log::warn!("Could not restore notifications during quit: {error}");
        }
    }
}

pub fn clear_pending_after_data_deletion() {
    session_plan::clear_pending();
    if let Ok(mut pending) = PENDING.lock() {
        pending.clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unknown_or_multiple_model_calls_never_execute() {
        let unknown = json!({"choices":[{"message":{"tool_calls":[{"function":{"name":"shell_run","arguments":"{}"}}]}}]});
        assert!(decide_from_model(&unknown, &registry::specs_for_family("focus")).is_err());
        let two = json!({"choices":[{"message":{"tool_calls":[{"function":{"name":"focus_pause","arguments":"{}"}},{"function":{"name":"focus_end","arguments":"{}"}}]}}]});
        assert!(decide_from_model(&two, &registry::specs_for_family("focus")).is_err());
    }

    #[test]
    fn routing_rejects_missing_multiple_and_unknown_families() {
        let good = json!({"choices":[{"message":{"tool_calls":[{"function":{"name":"select_tool_family","arguments":"{\"family\":\"focus\"}"}}]}}]});
        assert_eq!(routed_family(&good).unwrap(), "focus");
        let missing = json!({"choices":[{"message":{"content":"focus"}}]});
        assert!(routed_family(&missing).is_err());
        let multiple = json!({"choices":[{"message":{"tool_calls":[{"function":{"name":"select_tool_family","arguments":"{\"family\":\"focus\"}"}},{"function":{"name":"select_tool_family","arguments":"{\"family\":\"tasks\"}"}}]}}]});
        assert!(routed_family(&multiple).is_err());
        let unknown = json!({"choices":[{"message":{"tool_calls":[{"function":{"name":"select_tool_family","arguments":"{\"family\":\"shell\"}"}}]}}]});
        assert!(routed_family(&unknown).is_err());
    }

    #[test]
    fn model_cannot_call_a_tool_outside_the_selected_family() {
        let response = json!({"choices":[{"message":{"tool_calls":[{"function":{"name":"messages_send","arguments":"{\"draft_id\":\"known\"}"}}]}}]});
        assert!(decide_from_model(&response, &registry::specs_for_family("tasks")).is_err());
        assert!(decide_from_model(&response, &registry::specs_for_family("chat")).is_err());
    }

    #[test]
    fn a_valid_mutation_is_held_for_confirmation() {
        let response = json!({"choices":[{"message":{"tool_calls":[{"function":{"name":"tasks_create","arguments":"{\"title\":\"Review PR\"}"}}]}}]});
        let (_, choice) =
            decide_from_model(&response, &registry::specs_for_family("tasks")).unwrap();
        let (spec, args) = choice.unwrap();
        assert!(spec.confirmation);
        let proposal = proposal_for(&spec, args).unwrap();
        assert_eq!(proposal.tool, "tasks.create");
    }
}
