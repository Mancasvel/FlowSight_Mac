use crate::entitlements::load_entitlements;
use crate::sync::get_user_session_from_conn;
use crate::sync_env::{supabase_anon_key, supabase_url};
use reqwest::blocking::Client;
use rusqlite::Connection;
use serde_json::{json, Value};

const PRO_PLAN_IDS: &[&str] = &["pro", "individual", "individual_pro"];

fn require_notion_pro(conn: &Connection) -> Result<(), String> {
    let entitlements = load_entitlements(conn);
    let pro_plan = entitlements
        .plan
        .as_deref()
        .is_some_and(|plan| PRO_PLAN_IDS.contains(&plan));
    if entitlements.status == "active" && entitlements.can_integrations && pro_plan {
        Ok(())
    } else {
        Err("Notion publishing requires an active FlowSight Pro plan.".to_string())
    }
}

fn call_edge_function(function: &str, body: &Value, requires_pro: bool) -> Result<Value, String> {
    let db_path = crate::paths::db_path()?;
    let conn = Connection::open(&db_path).map_err(|error| error.to_string())?;
    if requires_pro {
        require_notion_pro(&conn)?;
    }
    let session = get_user_session_from_conn(&conn).ok_or("Sign in before using Notion.")?;

    let response = Client::new()
        .post(format!("{}/functions/v1/{}", supabase_url(), function))
        .header("apikey", supabase_anon_key())
        .bearer_auth(&session.access_token)
        .json(body)
        .send()
        .map_err(|_| "Could not reach the Notion integration service.".to_string())?;
    let status = response.status();
    let payload: Value = response
        .json()
        .map_err(|_| "The Notion integration returned an invalid response.".to_string())?;
    if !status.is_success() {
        let message = payload["error"]
            .as_str()
            .unwrap_or("The Notion operation failed.");
        let code = payload["code"].as_str().unwrap_or("notion_error");
        return Err(format!("{message} [{code}]"));
    }
    Ok(payload)
}

#[tauri::command]
pub fn get_notion_status() -> Result<Value, String> {
    call_edge_function("notion-oauth", &json!({ "action": "status" }), false)
}

#[tauri::command]
pub fn start_notion_oauth() -> Result<Value, String> {
    let result = call_edge_function("notion-oauth", &json!({ "action": "start" }), true)?;
    let authorization_url = result["authorization_url"]
        .as_str()
        .ok_or("Notion authorization URL was missing.")?;
    let parsed =
        url::Url::parse(authorization_url).map_err(|_| "Notion authorization URL was invalid.")?;
    if parsed.scheme() != "https" || parsed.host_str() != Some("api.notion.com") {
        return Err("Notion authorization URL was not trusted.".to_string());
    }
    open::that(parsed.as_str())
        .map_err(|_| "Could not open Notion in your browser.".to_string())?;
    Ok(result)
}

#[tauri::command]
pub fn disconnect_notion() -> Result<Value, String> {
    call_edge_function("notion-oauth", &json!({ "action": "disconnect" }), false)
}

#[tauri::command]
pub fn search_notion_destinations(query: Option<String>) -> Result<Value, String> {
    call_edge_function(
        "notion-destinations",
        &json!({ "action": "search", "query": query.unwrap_or_default() }),
        true,
    )
}

#[tauri::command]
pub fn save_notion_destination(
    notion_object_id: String,
    destination_type: String,
    report_mode: String,
) -> Result<Value, String> {
    call_edge_function(
        "notion-destinations",
        &json!({
            "action": "save",
            "notion_object_id": notion_object_id,
            "destination_type": destination_type,
            "report_mode": report_mode,
        }),
        true,
    )
}

#[tauri::command]
pub fn create_notion_report_destination(
    parent_page_id: String,
    report_mode: String,
) -> Result<Value, String> {
    call_edge_function(
        "notion-destinations",
        &json!({
            "action": "create_report_page",
            "parent_page_id": parent_page_id,
            "report_mode": report_mode,
        }),
        true,
    )
}

fn publish_payload(local_report: Value, destination_id: Option<String>) -> Value {
    json!({
        "local_report": local_report,
        "destination_id": destination_id,
    })
}

#[tauri::command]
pub fn publish_notion_report(
    period_days: Option<i32>,
    destination_id: Option<String>,
) -> Result<Value, String> {
    let db_path = crate::paths::db_path()?;
    let conn = Connection::open(&db_path).map_err(|error| error.to_string())?;
    require_notion_pro(&conn)?;
    drop(conn);

    let days = period_days.unwrap_or(7).clamp(1, 30);
    let local_report = crate::insights_local::build_local_insights_report(&db_path, days)?;
    if local_report["focus_semantics"].is_null() {
        return Err("Canonical focus semantics are unavailable for this report.".to_string());
    }
    call_edge_function(
        "publish-notion-report",
        &publish_payload(local_report, destination_id),
        true,
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::entitlements::Entitlements;

    fn connection_with(entitlements: &Entitlements) -> Connection {
        let connection = Connection::open_in_memory().unwrap();
        connection
            .execute(
                "CREATE TABLE config (key TEXT PRIMARY KEY, value TEXT NOT NULL)",
                [],
            )
            .unwrap();
        crate::entitlements::save_entitlements(&connection, entitlements).unwrap();
        connection
    }

    #[test]
    fn local_gate_accepts_only_active_pro_with_integrations() {
        let pro = Entitlements {
            plan: Some("individual".into()),
            status: "active".into(),
            can_integrations: true,
            ..Entitlements::default()
        };
        assert!(require_notion_pro(&connection_with(&pro)).is_ok());

        let team = Entitlements {
            plan: Some("team".into()),
            status: "active".into(),
            can_integrations: true,
            ..Entitlements::default()
        };
        assert_eq!(
            require_notion_pro(&connection_with(&team)).unwrap_err(),
            "Notion publishing requires an active FlowSight Pro plan."
        );

        let expired = Entitlements {
            plan: Some("individual".into()),
            status: "past_due".into(),
            can_integrations: true,
            ..Entitlements::default()
        };
        assert!(require_notion_pro(&connection_with(&expired)).is_err());
    }

    #[test]
    fn publication_payload_passes_canonical_report_without_recalculation() {
        let report = json!({
            "period_start": "2026-08-16",
            "period_end": "2026-08-22",
            "focus_semantics": {
                "policy_version": "deep-focus-v2",
                "deep_focus_seconds": 3100,
                "context_category_mix": [
                    {"category": "Planning", "seconds": 600},
                    {"category": "Sales", "seconds": 300}
                ]
            }
        });
        let payload = publish_payload(report.clone(), Some("destination-1".into()));
        assert_eq!(payload["local_report"], report);
        assert_eq!(payload["destination_id"], "destination-1");
        assert_eq!(
            payload["local_report"]["focus_semantics"]["deep_focus_seconds"],
            3100
        );
    }
}
