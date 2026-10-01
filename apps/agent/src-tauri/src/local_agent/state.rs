//! On-device state for the local action agent. The whole document is protected
//! with the current Windows user's DPAPI key through `secure_config`.

use std::collections::BTreeMap;
use std::sync::Mutex;

use rusqlite::Connection;
use serde::{Deserialize, Serialize};

const STATE_KEY: &str = "local_agent_state_v1";
static STATE_LOCK: Mutex<()> = Mutex::new(());

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct AgentData {
    pub tasks: Vec<LocalTask>,
    pub events: Vec<LocalEvent>,
    pub drafts: Vec<MessageDraft>,
    pub preferences: BTreeMap<String, SavedPreference>,
    pub focus: Option<FocusBlock>,
    pub quiet: Option<SystemQuiet>,
    pub total_focus_preferences: super::total_focus::Preferences,
    pub total_focus: Option<super::total_focus::Session>,
    pub calendar_provider: Option<String>,
    pub email_provider: Option<String>,
    pub notification_digest: Vec<DigestItem>,
    pub audit: Vec<ActionAudit>,
    pub conversation: Vec<ConversationMessage>,
    pub session_saves: Vec<SessionSave>,
}

/// A reviewed session is journaled before any provider request. Its stable
/// identities allow recovery after network failures or an application restart.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionSave {
    pub id: String,
    pub target: Option<super::session_calendar::CalendarTarget>,
    pub start_at: String,
    pub end_at: String,
    pub intention: String,
    pub events: Vec<LocalEvent>,
    pub complete: bool,
    #[serde(default)]
    pub abandoned: bool,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LocalTask {
    pub id: String,
    pub title: String,
    pub status: String,
    pub priority: u8,
    pub due_at: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LocalEvent {
    pub id: String,
    pub title: String,
    pub start_at: String,
    pub end_at: String,
    pub created_at: String,
    pub updated_at: String,
    #[serde(default)]
    pub provider: Option<String>,
    #[serde(default)]
    pub external_id: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MessageDraft {
    pub id: String,
    pub channel: String,
    pub recipient: String,
    pub body: String,
    pub created_at: String,
    #[serde(default)]
    pub subject: Option<String>,
    #[serde(default)]
    pub sent_at: Option<String>,
    #[serde(default)]
    pub send_claim: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SavedPreference {
    pub value: String,
    pub updated_at: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FocusBlock {
    pub intention: String,
    pub duration_minutes: u16,
    pub protection: String,
    pub started_at: String,
    pub status: String,
    pub paused_at: Option<String>,
    pub paused_seconds: i64,
    #[serde(default)]
    pub blocked_patterns: Vec<String>,
    #[serde(default)]
    pub quiet_owned: bool,
    #[serde(default)]
    pub last_active_at: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SystemQuiet {
    pub original: Option<u32>,
    pub until_at: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DigestItem {
    pub id: String,
    pub title: String,
    pub body: String,
    pub created_at: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ActionAudit {
    pub id: String,
    pub tool: String,
    pub summary: String,
    pub status: String,
    pub created_at: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ConversationMessage {
    pub role: String,
    pub content: String,
    pub created_at: String,
}

fn connection() -> Result<Connection, String> {
    let conn = Connection::open(crate::paths::db_path()?).map_err(|error| error.to_string())?;
    conn.execute(
        "CREATE TABLE IF NOT EXISTS config (key TEXT PRIMARY KEY, value TEXT)",
        [],
    )
    .map_err(|error| error.to_string())?;
    Ok(conn)
}

fn load(conn: &Connection) -> Result<AgentData, String> {
    let Some(raw) = crate::secure_config::load_secret(conn, STATE_KEY)? else {
        return Ok(AgentData::default());
    };
    serde_json::from_str(&raw).map_err(|error| format!("Local agent data is invalid: {error}"))
}

pub fn read() -> Result<AgentData, String> {
    let _guard = STATE_LOCK.lock().map_err(|error| error.to_string())?;
    load(&connection()?)
}

pub fn update<T>(change: impl FnOnce(&mut AgentData) -> Result<T, String>) -> Result<T, String> {
    let _guard = STATE_LOCK.lock().map_err(|error| error.to_string())?;
    let conn = connection()?;
    let mut data = load(&conn)?;
    let result = change(&mut data)?;
    data.audit = data
        .audit
        .into_iter()
        .rev()
        .take(200)
        .collect::<Vec<_>>()
        .into_iter()
        .rev()
        .collect();
    data.drafts = data
        .drafts
        .into_iter()
        .rev()
        .take(100)
        .collect::<Vec<_>>()
        .into_iter()
        .rev()
        .collect();
    data.notification_digest = data
        .notification_digest
        .into_iter()
        .rev()
        .take(100)
        .collect::<Vec<_>>()
        .into_iter()
        .rev()
        .collect();
    data.conversation = data
        .conversation
        .into_iter()
        .rev()
        .take(30)
        .collect::<Vec<_>>()
        .into_iter()
        .rev()
        .collect();
    let encoded = serde_json::to_string(&data).map_err(|error| error.to_string())?;
    crate::secure_config::save_secret(&conn, STATE_KEY, &encoded)?;
    Ok(result)
}

pub fn append_conversation(role: &str, content: &str) -> Result<(), String> {
    update(|data| {
        data.conversation.push(ConversationMessage {
            role: role.to_string(),
            content: content.chars().take(5000).collect(),
            created_at: chrono::Utc::now().to_rfc3339(),
        });
        Ok(())
    })
}

pub fn hold_notification(title: &str, body: &str) -> Result<bool, String> {
    fn holding(data: &AgentData) -> bool {
        data.quiet.is_some()
            || data.total_focus.as_ref().is_some_and(|session| {
                chrono::DateTime::parse_from_rfc3339(&session.expires_at)
                    .is_ok_and(|until| until > chrono::Utc::now())
            })
    }
    if !holding(&read()?) {
        return Ok(false);
    }
    update(|data| {
        if !holding(data) {
            return Ok(false);
        }
        data.notification_digest.push(DigestItem {
            id: uuid::Uuid::new_v4().to_string(),
            title: title.to_string(),
            body: body.to_string(),
            created_at: chrono::Utc::now().to_rfc3339(),
        });
        Ok(true)
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_and_roundtrip_are_stable() {
        let mut data = AgentData::default();
        data.preferences.insert(
            "research.youtube".into(),
            SavedPreference {
                value: "allowed".into(),
                updated_at: "2026-09-29T10:00:00Z".into(),
            },
        );
        let restored: AgentData =
            serde_json::from_str(&serde_json::to_string(&data).unwrap()).unwrap();
        assert_eq!(restored.preferences["research.youtube"].value, "allowed");
        assert!(restored.tasks.is_empty());
    }
}
