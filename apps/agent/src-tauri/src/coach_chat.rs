use reqwest::blocking::Client;
use rusqlite::{params, Connection};
use serde::{Deserialize, Serialize};
use std::time::Duration;

use crate::sync::get_user_session_from_conn;
use crate::sync_env::{supabase_anon_key, supabase_url};

const COACH_MESSAGES_KEY: &str = "coach_chat_messages";
const MAX_MESSAGE_LEN: usize = 500;
const MAX_STORED_MESSAGES: usize = 40;
const MESSAGE_RETENTION_DAYS: i64 = 30;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CoachChatMessage {
    pub id: String,
    pub role: String,
    pub content: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reasoning: Option<String>,
    #[serde(rename = "createdAt", default)]
    pub created_at: Option<String>,
}

pub(crate) fn load_messages(conn: &Connection) -> Result<Vec<CoachChatMessage>, String> {
    let json: Option<String> = conn
        .query_row(
            "SELECT value FROM config WHERE key = ?1",
            params![COACH_MESSAGES_KEY],
            |row| row.get(0),
        )
        .ok();

    match json {
        Some(raw) => {
            let cutoff = chrono::Utc::now() - chrono::Duration::days(MESSAGE_RETENTION_DAYS);
            let messages: Vec<CoachChatMessage> =
                serde_json::from_str(&raw).map_err(|e| e.to_string())?;
            Ok(messages
                .into_iter()
                .filter(|message| {
                    message
                        .created_at
                        .as_deref()
                        .and_then(|value| chrono::DateTime::parse_from_rfc3339(value).ok())
                        .is_some_and(|created| created >= cutoff)
                })
                .collect())
        }
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
    let messages = load_messages(&conn)?;
    save_messages(&conn, &messages)?;
    Ok(messages)
}

// The usage endpoint can wait on the network; never hold the Tauri UI thread.
#[tauri::command]
pub async fn get_coach_chat_usage() -> Result<serde_json::Value, String> {
    tauri::async_runtime::spawn_blocking(get_coach_chat_usage_blocking)
        .await
        .map_err(|error| format!("Coach usage worker failed: {error}"))?
}

fn get_coach_chat_usage_blocking() -> Result<serde_json::Value, String> {
    let db_path = crate::paths::db_path()?;
    crate::entitlements::require_feature(&db_path, "cloud_ai")?;
    crate::privacy::require_cloud_ai(&db_path)?;

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

// Local report assembly and the cloud reply are blocking work. Keep navigation
// responsive while the Coach is thinking, including on slow or failed requests.
#[tauri::command]
pub async fn send_coach_chat_message(message: String) -> Result<serde_json::Value, String> {
    tauri::async_runtime::spawn_blocking(move || send_coach_chat_message_blocking(message))
        .await
        .map_err(|error| format!("Coach message worker failed: {error}"))?
}

fn send_coach_chat_message_blocking(message: String) -> Result<serde_json::Value, String> {
    let trimmed = message.trim();
    if trimmed.is_empty() {
        return Err("Message cannot be empty".to_string());
    }
    if trimmed.len() > MAX_MESSAGE_LEN {
        return Err(format!(
            "Message must be {} characters or fewer",
            MAX_MESSAGE_LEN
        ));
    }

    let db_path = crate::paths::db_path()?;
    crate::entitlements::require_feature(&db_path, "cloud_ai")?;
    crate::privacy::require_cloud_ai(&db_path)?;

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
        created_at: Some(chrono::Utc::now().to_rfc3339()),
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

    let local_context = crate::insights_local::build_local_insights_report(&db_path, 7)
        .unwrap_or_else(|err| serde_json::json!({ "error": err }));

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
        created_at: Some(chrono::Utc::now().to_rfc3339()),
    };
    messages.push(assistant_msg.clone());
    save_messages(&conn, &messages)?;

    Ok(serde_json::json!({
        "reply": reply,
        "usage": payload.get("usage"),
        "messages": messages,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_connection() -> Connection {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute("CREATE TABLE config (key TEXT PRIMARY KEY, value TEXT)", [])
            .unwrap();
        conn
    }

    fn message(id: usize, created_at: Option<String>) -> CoachChatMessage {
        CoachChatMessage {
            id: id.to_string(),
            role: "user".to_string(),
            content: format!("message-{id}"),
            reasoning: None,
            created_at,
        }
    }

    #[test]
    fn loading_messages_drops_expired_and_legacy_undated_content() {
        let conn = test_connection();
        let recent = chrono::Utc::now().to_rfc3339();
        let expired = (chrono::Utc::now() - chrono::Duration::days(31)).to_rfc3339();
        let stored = vec![
            message(1, Some(recent)),
            message(2, Some(expired)),
            message(3, None),
        ];
        conn.execute(
            "INSERT INTO config (key, value) VALUES (?1, ?2)",
            params![COACH_MESSAGES_KEY, serde_json::to_string(&stored).unwrap()],
        )
        .unwrap();

        let loaded = load_messages(&conn).unwrap();
        assert_eq!(loaded.len(), 1);
        assert_eq!(loaded[0].id, "1");
    }

    #[test]
    fn saving_messages_keeps_only_the_newest_bounded_history() {
        let conn = test_connection();
        let now = chrono::Utc::now().to_rfc3339();
        let messages = (0..45)
            .map(|id| message(id, Some(now.clone())))
            .collect::<Vec<_>>();
        save_messages(&conn, &messages).unwrap();

        let loaded = load_messages(&conn).unwrap();
        assert_eq!(loaded.len(), MAX_STORED_MESSAGES);
        assert_eq!(loaded.first().map(|item| item.id.as_str()), Some("5"));
        assert_eq!(loaded.last().map(|item| item.id.as_str()), Some("44"));
    }
}
