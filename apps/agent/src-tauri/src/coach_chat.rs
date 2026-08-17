use reqwest::blocking::Client;
use rusqlite::{params, Connection};
use serde::{Deserialize, Serialize};
use std::time::Duration;

use crate::sync::get_user_session_from_conn;
use crate::sync_env::{supabase_anon_key, supabase_url};

const COACH_MESSAGES_KEY: &str = "coach_chat_messages";
const MAX_MESSAGE_LEN: usize = 500;
const MAX_STORED_MESSAGES: usize = 40;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CoachChatMessage {
    pub id: String,
    pub role: String,
    pub content: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reasoning: Option<String>,
}

fn load_messages(conn: &Connection) -> Result<Vec<CoachChatMessage>, String> {
    let json: Option<String> = conn
        .query_row(
            "SELECT value FROM config WHERE key = ?1",
            params![COACH_MESSAGES_KEY],
            |row| row.get(0),
        )
        .ok();

    match json {
        Some(raw) => serde_json::from_str(&raw).map_err(|e| e.to_string()),
        None => Ok(vec![]),
    }
}

fn save_messages(conn: &Connection, messages: &[CoachChatMessage]) -> Result<(), String> {
    let trimmed: Vec<_> = messages
        .iter()
        .rev()
        .take(MAX_STORED_MESSAGES)
        .cloned()
        .collect::<Vec<_>>()
        .into_iter()
        .rev()
        .collect();

    let json = serde_json::to_string(&trimmed).map_err(|e| e.to_string())?;
    conn.execute(
        "INSERT OR REPLACE INTO config (key, value) VALUES (?1, ?2)",
        params![COACH_MESSAGES_KEY, json],
    )
    .map_err(|e| e.to_string())?;
    Ok(())
}

fn coach_http_client() -> Result<Client, String> {
    Client::builder()
        .timeout(Duration::from_secs(120))
        .build()
        .map_err(|e| e.to_string())
}

fn extract_coach_api_error(payload: &serde_json::Value, status: reqwest::StatusCode) -> String {
    if let Some(s) = payload.get("error").and_then(|v| v.as_str()) {
        return s.to_string();
    }
    if let Some(s) = payload.get("message").and_then(|v| v.as_str()) {
        return s.to_string();
    }
    if let Some(s) = payload.get("msg").and_then(|v| v.as_str()) {
        return s.to_string();
    }
    if payload.is_object() && payload.as_object().is_some_and(|o| !o.is_empty()) {
        return format!("Coach API error ({status}): {payload}");
    }
    format!("AI coach request failed ({status})")
}

#[tauri::command]
pub fn get_coach_chat_messages() -> Result<Vec<CoachChatMessage>, String> {
    let db_path = crate::paths::db_path()?;
    let conn = Connection::open(&db_path).map_err(|e| e.to_string())?;
    load_messages(&conn)
}

#[tauri::command]
pub fn clear_coach_chat() -> Result<(), String> {
    let db_path = crate::paths::db_path()?;
    let conn = Connection::open(&db_path).map_err(|e| e.to_string())?;
    conn.execute("DELETE FROM config WHERE key = ?1", params![COACH_MESSAGES_KEY])
        .map_err(|e| e.to_string())?;
    Ok(())
}

#[tauri::command]
pub fn get_coach_chat_usage() -> Result<serde_json::Value, String> {
    let db_path = crate::paths::db_path()?;
    crate::entitlements::require_feature(&db_path, "cloud_ai")?;

    let conn = Connection::open(&db_path).map_err(|e| e.to_string())?;
    let session = get_user_session_from_conn(&conn).ok_or("Not logged in")?;
    let entitlements = crate::entitlements::load_entitlements(&conn);

    let team_id = session
        .team_id
        .clone()
        .or_else(|| entitlements.active_team_id.clone())
        .unwrap_or_default();

    let client = coach_http_client()?;
    let url = format!(
        "{}/functions/v1/coach-chat?teamId={}",
        supabase_url(),
        urlencoding::encode(&team_id)
    );

    let resp = client
        .get(&url)
        .header("apikey", supabase_anon_key())
        .header("Authorization", format!("Bearer {}", session.access_token))
        .send()
        .map_err(|e| e.to_string())?;

    let status = resp.status();
    let body: serde_json::Value = resp.json().map_err(|e| e.to_string())?;

    if !status.is_success() {
        return Ok(serde_json::json!({
            "usage": {
                "used": 0,
                "limit": 0,
                "remaining": 0,
                "planId": entitlements.plan.unwrap_or_else(|| "free".to_string()),
                "allowed": false
            },
            "error": body.get("error").and_then(|v| v.as_str()).unwrap_or("Could not load coach usage")
        }));
    }

    Ok(body)
}

#[tauri::command]
pub fn send_coach_chat_message(message: String) -> Result<serde_json::Value, String> {
    let trimmed = message.trim();
    if trimmed.is_empty() {
        return Err("Message cannot be empty".to_string());
    }
    if trimmed.len() > MAX_MESSAGE_LEN {
        return Err(format!("Message must be {} characters or fewer", MAX_MESSAGE_LEN));
    }

    let db_path = crate::paths::db_path()?;
    crate::entitlements::require_feature(&db_path, "cloud_ai")?;

    let conn = Connection::open(&db_path).map_err(|e| e.to_string())?;
    let session = get_user_session_from_conn(&conn).ok_or("Not logged in")?;
    let entitlements = crate::entitlements::load_entitlements(&conn);

    let team_id = session
        .team_id
        .clone()
        .or_else(|| entitlements.active_team_id.clone());

    let mut messages = load_messages(&conn)?;
    let user_msg = CoachChatMessage {
        id: format!("u-{}", chrono::Utc::now().timestamp_millis()),
        role: "user".to_string(),
        content: trimmed.to_string(),
        reasoning: None,
    };
    messages.push(user_msg);

    let history: Vec<serde_json::Value> = messages
        .iter()
        .map(|m| {
            serde_json::json!({
                "role": m.role,
                "content": m.content,
            })
        })
        .collect();

    let local_context =
        crate::insights_local::build_local_insights_report(&db_path, 7).unwrap_or_else(|err| {
            serde_json::json!({ "error": err })
        });

    let body = serde_json::json!({
        "message": trimmed,
        "team_id": team_id,
        "history": history,
        "local_context": local_context,
    });

    let client = coach_http_client()?;
    let url = format!("{}/functions/v1/coach-chat", supabase_url());
    let resp = client
        .post(&url)
        .header("apikey", supabase_anon_key())
        .header("Authorization", format!("Bearer {}", session.access_token))
        .header("Content-Type", "application/json")
        .json(&body)
        .send()
        .map_err(|e| e.to_string())?;

    let status = resp.status();
    let payload: serde_json::Value = resp.json().map_err(|e| e.to_string())?;

    if !status.is_success() {
        return Err(extract_coach_api_error(&payload, status));
    }

    let reply = payload
        .get("reply")
        .and_then(|v| v.as_str())
        .ok_or("AI coach returned an empty response")?;

    let assistant_msg = CoachChatMessage {
        id: format!("a-{}", chrono::Utc::now().timestamp_millis()),
        role: "assistant".to_string(),
        content: reply.to_string(),
        reasoning: payload
            .get("reasoning")
            .and_then(|v| v.as_str())
            .map(String::from),
    };
    messages.push(assistant_msg.clone());
    save_messages(&conn, &messages)?;

    Ok(serde_json::json!({
        "reply": reply,
        "usage": payload.get("usage"),
        "messages": messages,
    }))
}
