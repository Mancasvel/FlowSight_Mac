//! The model sees function names with underscores because OpenAI-compatible
//! function schemas restrict names. The public API keeps the dotted names.

use serde_json::{json, Value};

#[derive(Clone)]
pub struct ToolSpec {
    pub name: &'static str,
    pub model_name: &'static str,
    pub description: &'static str,
    pub parameters: Value,
    pub confirmation: bool,
}

fn spec(
    name: &'static str,
    model_name: &'static str,
    description: &'static str,
    properties: Value,
    required: &[&str],
    confirmation: bool,
) -> ToolSpec {
    ToolSpec {
        name,
        model_name,
        description,
        parameters: json!({
            "type": "object",
            "properties": properties,
            "required": required,
            "additionalProperties": false
        }),
        confirmation,
    }
}

pub fn specs() -> Vec<ToolSpec> {
    vec![
        spec("focus.total_start", "focus_total_start", "Activate total focus in the paired browser, without starting tracking. Uses saved settings unless sites or exceptions are supplied. Messaging auto replies are not available.", json!({
            "intention":{"type":"string","maxLength":160},
            "duration_minutes":{"type":"integer","minimum":5,"maximum":180},
            "patterns":{"type":"array","items":{"type":"string","maxLength":240},"minItems":1,"maxItems":20},
            "exceptions":{"type":"array","items":{"type":"string","maxLength":240},"maxItems":20}
        }), &["intention"], true),
        spec("focus.total_end", "focus_total_end", "End total focus and release its browser protection.", json!({}), &[], true),
        spec("focus.total_status", "focus_total_status", "Read total focus settings and the actual extension acknowledgement.", json!({}), &[], false),
        spec("focus.start", "focus_start", "Start a timed focus block with a specific intention. Ask the user to confirm first.", json!({
            "intention": {"type":"string","maxLength":160},
            "duration_minutes": {"type":"integer","minimum":5,"maximum":180},
            "protection": {"type":"string","enum":["gentle","standard","strict"]},
            "block_patterns": {"type":"array","items":{"type":"string","maxLength":240},"minItems":1,"maxItems":20}
        }), &["intention","duration_minutes","protection"], true),
        spec("focus.resume", "focus_resume", "Resume the paused focus block with its remaining time and protections.", json!({}), &[], true),
        spec("focus.pause", "focus_pause", "Pause the current focus block and tracking.", json!({}), &[], true),
        spec("focus.end", "focus_end", "End the current focus block and tracking.", json!({}), &[], true),
        spec("system.set_dnd", "system_set_dnd", "Silence Windows app notification banners or restore their previous setting. This does not configure Focus Assist allowlists.", json!({
            "enabled": {"type":"boolean"},
            "duration_minutes": {"type":"integer","minimum":1,"maximum":240}
        }), &["enabled"], true),
        spec("browser.block", "browser_block", "Temporarily block specified browser domains or URL paths in the paired extension.", json!({
            "patterns": {"type":"array","items":{"type":"string","maxLength":240},"minItems":1,"maxItems":20},
            "duration_minutes": {"type":"integer","minimum":1,"maximum":240}
        }), &["patterns","duration_minutes"], true),
        spec("browser.unblock", "browser_unblock", "Remove temporary blocks for specified domains or URL paths.", json!({
            "patterns": {"type":"array","items":{"type":"string","maxLength":240},"minItems":1,"maxItems":20}
        }), &["patterns"], true),
        spec("browser.close_tab", "browser_close_tab", "Close a specific browser tab after confirmation and keep a restore record.", json!({
            "tab_id": {"type":"integer","minimum":1}
        }), &["tab_id"], true),
        spec("browser.list_tabs", "browser_list_tabs", "List open browser tabs and FlowSight restore records so the user can identify a tab before closing it.", json!({}), &[], false),
        spec("browser.restore_tab", "browser_restore_tab", "Restore a tab previously closed by FlowSight.", json!({
            "restore_id": {"type":"string","maxLength":80}
        }), &["restore_id"], true),
        spec("tasks.create", "tasks_create", "Create a local task from a concrete intention.", json!({
            "title": {"type":"string","maxLength":240},
            "priority": {"type":"integer","minimum":1,"maximum":5},
            "due_at": {"type":"string","format":"date-time"}
        }), &["title"], true),
        spec("tasks.list", "tasks_list", "List recent local tasks and their IDs so one can be updated or completed.", json!({}), &[], false),
        spec("tasks.update", "tasks_update", "Update a local task title, due time, or status.", json!({
            "task_id": {"type":"string","maxLength":80},
            "title": {"type":"string","maxLength":240},
            "due_at": {"type":"string","format":"date-time"},
            "status": {"type":"string","enum":["open","in_progress","completed"]}
        }), &["task_id"], true),
        spec("tasks.complete", "tasks_complete", "Mark a local task completed.", json!({
            "task_id": {"type":"string","maxLength":80}
        }), &["task_id"], true),
        spec("tasks.reprioritize", "tasks_reprioritize", "Set a local task priority from 1 (highest) to 5.", json!({
            "task_id": {"type":"string","maxLength":80},
            "priority": {"type":"integer","minimum":1,"maximum":5}
        }), &["task_id","priority"], true),
        spec("calendar.get_availability", "calendar_get_availability", "Find free time in the connected calendar for a given interval.", json!({
            "start_at": {"type":"string","format":"date-time"},
            "end_at": {"type":"string","format":"date-time"}
        }), &["start_at","end_at"], false),
        spec("calendar.list_events", "calendar_list_events", "List FlowSight-owned local and connected calendar events with their IDs.", json!({}), &[], false),
        spec("calendar.get_current_event", "calendar_get_current_event", "Read the current connected calendar event, its task description, time, and organizer status. No mutation.", json!({}), &[], false),
        spec("calendar.create_event", "calendar_create_event", "Create a focus event in FlowSight's local calendar. Connected calendar writes are unavailable in this release.", json!({
            "title": {"type":"string","maxLength":200},
            "start_at": {"type":"string","format":"date-time"},
            "end_at": {"type":"string","format":"date-time"}
        }), &["title","start_at","end_at"], true),
        spec("calendar.move_event", "calendar_move_event", "Move a FlowSight-owned local event. Connected calendar writes are unavailable in this release.", json!({
            "event_id": {"type":"string","maxLength":160},
            "start_at": {"type":"string","format":"date-time"},
            "end_at": {"type":"string","format":"date-time"}
        }), &["event_id","start_at","end_at"], true),
        spec("notifications.digest", "notifications_digest", "Show the notifications FlowSight held during a focus block.", json!({}), &[], false),
        spec("messages.draft", "messages_draft", "Prepare a response without sending it. Email recipients are addresses; Slack recipients are channel or user IDs; Teams recipients are chat IDs.", json!({
            "channel": {"type":"string","enum":["email","slack","teams"]},
            "recipient": {"type":"string","maxLength":240},
            "body": {"type":"string","maxLength":4000},
            "subject": {"type":"string","maxLength":200}
        }), &["channel","recipient","body"], true),
        spec("messages.list_drafts", "messages_list_drafts", "List recent saved message drafts and their delivery state.", json!({}), &[], false),
        spec("messages.send", "messages_send", "Send one saved draft through its connected provider. The full recipient and body are shown before confirmation.", json!({
            "draft_id": {"type":"string","maxLength":80},
            "retry_if_uncertain": {"type":"boolean"}
        }), &["draft_id"], true),
        spec("project.get_current_work", "project_get_current_work", "Read the current local project and linked work item context.", json!({}), &[], false),
        spec("project.update_status", "project_update_status", "Update the current work item's status or handoff note in its provider.", json!({
            "provider": {"type":"string","enum":["github","linear","jira","notion"]},
            "item_id": {"type":"string","maxLength":160},
            "status": {"type":"string","maxLength":80},
            "note": {"type":"string","maxLength":2000}
        }), &["provider","item_id","status"], true),
        spec("project.create_subtask", "project_create_subtask", "Create a child issue or page under a specific GitHub, Linear, Jira, or Notion item after confirmation.", json!({
            "provider": {"type":"string","enum":["github","linear","jira","notion"]},
            "parent_id": {"type":"string","maxLength":160},
            "title": {"type":"string","maxLength":240},
            "description": {"type":"string","maxLength":2000}
        }), &["provider","parent_id","title"], true),
        spec("project.prepare_pr_description", "project_prepare_pr_description", "Format a PR description from supplied, verified changes and test results. This only returns a draft for review; it does not publish a PR.", json!({
            "title": {"type":"string","maxLength":160},
            "summary": {"type":"array","items":{"type":"string","maxLength":500},"minItems":1,"maxItems":8},
            "tests": {"type":"array","items":{"type":"string","maxLength":500},"minItems":1,"maxItems":8},
            "risks": {"type":"array","items":{"type":"string","maxLength":500},"minItems":1,"maxItems":5}
        }), &["title","summary"], false),
        spec("desktop.open_resource", "desktop_open_resource", "Open a user-specified local file, project, or HTTPS resource.", json!({
            "resource": {"type":"string","maxLength":2048}
        }), &["resource"], true),
        spec("automation.run_playbook", "automation_run_playbook", "Run an auditable deep work, end-of-day, or recover-focus routine.", json!({
            "playbook": {"type":"string","enum":["deep_work","end_of_day","recover_focus"]},
            "intention": {"type":"string","maxLength":160},
            "duration_minutes": {"type":"integer","minimum":5,"maximum":180},
            "block_patterns": {"type":"array","items":{"type":"string","maxLength":240},"minItems":1,"maxItems":20},
            "resource": {"type":"string","maxLength":2048},
            "tab_id": {"type":"integer","minimum":1}
        }), &["playbook"], true),
        spec("memory.save_preference", "memory_save_preference", "Remember a user-approved behavior rule on this device.", json!({
            "key": {"type":"string","maxLength":100},
            "value": {"type":"string","maxLength":500}
        }), &["key","value"], true),
        spec("memory.forget_preference", "memory_forget_preference", "Forget a previously saved behavior rule.", json!({
            "key": {"type":"string","maxLength":100}
        }), &["key"], true),
        spec("memory.list_preferences", "memory_list_preferences", "Show all behavior rules FlowSight currently remembers.", json!({}), &[], false),
    ]
}

pub const TOOL_FAMILIES: &[&str] = &[
    "focus",
    "system",
    "browser",
    "tasks",
    "calendar",
    "notifications",
    "messages",
    "project",
    "desktop",
    "automation",
    "memory",
    "chat",
];

/// External writes are implemented behind this release gate until their
/// provider-specific behavior has been verified with scoped test accounts.
const DISABLED_EXTERNAL_WRITES: &[&str] = &[
    "messages.send",
    "project.update_status",
    "project.create_subtask",
];

pub fn is_enabled_name(name: &str) -> bool {
    !DISABLED_EXTERNAL_WRITES.contains(&name)
}

pub fn enabled_specs() -> Vec<ToolSpec> {
    specs()
        .into_iter()
        .filter(|spec| is_enabled_name(spec.name))
        .collect()
}

pub fn enabled_by_public_name(name: &str) -> Option<ToolSpec> {
    enabled_specs().into_iter().find(|spec| spec.name == name)
}

pub const TOOL_FAMILY_PROMPT: &str = "Classify the latest user request by the ACTION VERB, not by nouns inside its content. Always call select_tool_family exactly once; never act during routing. Priority for overlapping words: remember/forget a rule (recuerda/olvida) = memory, even if about YouTube; run a named playbook or routine (ejecuta rutina) = automation, even if it starts focus; draft/send an email, Slack or Teams message (redacta/envia) = messages, even if its text mentions focus; open a file, folder, app or URL (abre) = desktop, even if it is a project folder. Example: 'Run the end-of-day playbook to close my focus session and plan tomorrow' MUST be automation, not focus. 'What is an end-of-day playbook?' is chat. Other families: focus = start/pause/end a timed focus session; system = toggle Windows notification banners or DND; browser = block a site or list/close/restore a tab; tasks = create/update/complete/reprioritize a task; calendar = availability or events; notifications = held reminder digest; project = read/update GitHub/Jira/Linear/Notion work items; chat = informational questions, ambiguity, or no clear action. Select chat if more than one unrelated action is requested.";

pub fn family_router_definition() -> Value {
    json!({"type":"function","function":{
        "name":"select_tool_family",
        "description":"Select the one family relevant to this request; chat means no action.",
        "parameters":{"type":"object","properties":{"family":{"type":"string","enum":TOOL_FAMILIES}},"required":["family"],"additionalProperties":false}
    }})
}

/// Expose only the tools relevant to one request. All tools remain available
/// across turns, but their schemas no longer crowd the local model context.
pub fn specs_for_family(family: &str) -> Vec<ToolSpec> {
    if !TOOL_FAMILIES.contains(&family) || family == "chat" {
        return Vec::new();
    }
    enabled_specs()
        .into_iter()
        .filter(|spec| spec.name.split('.').next() == Some(family))
        .collect()
}

#[cfg(test)]
pub fn by_public_name(name: &str) -> Option<ToolSpec> {
    specs().into_iter().find(|spec| spec.name == name)
}

pub fn model_definition(spec: &ToolSpec) -> Value {
    json!({
        "type": "function",
        "function": {
            "name": spec.model_name,
            "description": spec.description,
            "parameters": spec.parameters
        }
    })
}

pub fn validate(spec: &ToolSpec, args: &Value) -> Result<(), String> {
    let object = args
        .as_object()
        .ok_or("Tool arguments must be an object.")?;
    let properties = spec.parameters["properties"]
        .as_object()
        .ok_or("Tool schema is invalid.")?;
    for required in spec.parameters["required"].as_array().into_iter().flatten() {
        let name = required.as_str().ok_or("Tool schema is invalid.")?;
        if !object.contains_key(name) {
            return Err(format!("Missing argument: {name}"));
        }
    }
    for (name, value) in object {
        let rule = properties
            .get(name)
            .ok_or_else(|| format!("Unexpected argument: {name}"))?;
        match rule["type"].as_str().unwrap_or("") {
            "string" => {
                let text = value
                    .as_str()
                    .ok_or_else(|| format!("{name} must be text."))?;
                if text.trim().is_empty() {
                    return Err(format!("{name} cannot be empty."));
                }
                if text.chars().count() > rule["maxLength"].as_u64().unwrap_or(u64::MAX) as usize {
                    return Err(format!("{name} is too long."));
                }
                if let Some(options) = rule["enum"].as_array() {
                    if !options.iter().any(|option| option == value) {
                        return Err(format!("Unsupported {name}."));
                    }
                }
                if rule["format"] == "date-time"
                    && chrono::DateTime::parse_from_rfc3339(text).is_err()
                {
                    return Err(format!("{name} must include a date, time, and time zone."));
                }
            }
            "integer" => {
                let number = value
                    .as_i64()
                    .ok_or_else(|| format!("{name} must be an integer."))?;
                if number < rule["minimum"].as_i64().unwrap_or(i64::MIN)
                    || number > rule["maximum"].as_i64().unwrap_or(i64::MAX)
                {
                    return Err(format!("{name} is outside the allowed range."));
                }
            }
            "boolean" if value.is_boolean() => {}
            "array" => {
                let items = value
                    .as_array()
                    .ok_or_else(|| format!("{name} must be a list."))?;
                if items.len() < rule["minItems"].as_u64().unwrap_or(0) as usize
                    || items.len() > rule["maxItems"].as_u64().unwrap_or(u64::MAX) as usize
                {
                    return Err(format!("{name} has the wrong number of items."));
                }
                for item in items {
                    let text = item
                        .as_str()
                        .ok_or_else(|| format!("{name} must contain text."))?;
                    if text.trim().is_empty()
                        || text.chars().count()
                            > rule["items"]["maxLength"].as_u64().unwrap_or(u64::MAX) as usize
                    {
                        return Err(format!("{name} contains an invalid item."));
                    }
                }
            }
            _ => return Err(format!("{name} has an invalid type.")),
        }
    }
    if spec.name == "focus.start"
        && args["protection"] == "strict"
        && match args["block_patterns"].as_array() {
            Some(patterns) => patterns.is_empty(),
            None => true,
        }
    {
        return Err("Strict protection needs one or more browser patterns to block.".into());
    }
    if spec.name == "automation.run_playbook"
        && args["playbook"] == "deep_work"
        && (args["intention"].as_str().is_none() || args["duration_minutes"].as_i64().is_none())
    {
        return Err("Deep work needs an intention and duration.".into());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn all_public_and_model_names_are_unique() {
        let specs = specs();
        let mut public = std::collections::HashSet::new();
        let mut model = std::collections::HashSet::new();
        for spec in specs {
            assert!(public.insert(spec.name));
            assert!(model.insert(spec.model_name));
            assert!(!spec.model_name.contains('.'));
        }
        assert_eq!(public.len(), 36);
    }

    #[test]
    fn every_tool_has_a_bounded_family_and_chat_has_no_tools() {
        let all = enabled_specs();
        assert!(specs_for_family("chat").is_empty());
        assert!(specs_for_family("unknown").is_empty());
        for spec in &all {
            let family = spec.name.split('.').next().unwrap();
            assert!(TOOL_FAMILIES.contains(&family));
            assert!(specs_for_family(family)
                .iter()
                .any(|candidate| candidate.name == spec.name));
        }
        assert!(TOOL_FAMILIES
            .iter()
            .filter(|family| **family != "chat")
            .all(|family| specs_for_family(family).len() < all.len()));
    }

    #[test]
    fn unverified_external_writes_are_not_offered() {
        assert_eq!(enabled_specs().len(), 33);
        for name in DISABLED_EXTERNAL_WRITES {
            assert!(by_public_name(name).is_some());
            assert!(enabled_by_public_name(name).is_none());
            assert!(!specs_for_family(name.split('.').next().unwrap())
                .iter()
                .any(|spec| spec.name == *name));
        }
    }

    #[test]
    fn host_rejects_unexpected_arguments_and_bad_times() {
        let spec = by_public_name("calendar.create_event").unwrap();
        assert!(validate(&spec, &json!({"title":"Focus","start_at":"2026-10-01T09:00:00+02:00","end_at":"2026-10-01T10:00:00+02:00"})).is_ok());
        assert!(validate(
            &spec,
            &json!({"title":"Focus","start_at":"tomorrow","end_at":"2026-10-01T10:00:00+02:00"})
        )
        .is_err());
        assert!(validate(&spec, &json!({"title":"Focus","start_at":"2026-10-01T09:00:00+02:00","end_at":"2026-10-01T10:00:00+02:00","send":true})).is_err());
    }

    #[test]
    fn strict_focus_requires_explicit_block_targets() {
        let spec = by_public_name("focus.start").unwrap();
        let base = json!({"intention":"Finish the PR","duration_minutes":45,"protection":"strict"});
        assert!(validate(&spec, &base).is_err());
        let standard =
            json!({"intention":"Finish the PR","duration_minutes":45,"protection":"standard"});
        assert!(validate(&spec, &standard).is_ok());
        let strict = json!({"intention":"Finish the PR","duration_minutes":45,"protection":"strict","block_patterns":["youtube.com/shorts"]});
        assert!(validate(&spec, &strict).is_ok());
    }

    #[test]
    fn send_draft_requires_an_explicit_retry_flag_when_requested() {
        let spec = by_public_name("messages.send").unwrap();
        assert!(validate(
            &spec,
            &json!({"draft_id":"known","retry_if_uncertain":true})
        )
        .is_ok());
        assert!(validate(
            &spec,
            &json!({"draft_id":"known","retry_if_uncertain":"true"})
        )
        .is_err());
        assert!(validate(&spec, &json!({"draft_id":"known","body":"injected"})).is_err());
    }
}
