//! Explicit, locally protected credentials for optional external tools.
//! Credential values are never returned to the renderer or included in export.

use std::time::Duration;

use reqwest::blocking::{Client, Response};
use rusqlite::Connection;
use serde_json::{json, Value};

use super::state;

const PROVIDERS: &[&str] = &["google", "microsoft", "slack", "teams", "github", "notion"];

fn connection() -> Result<Connection, String> {
    let conn = Connection::open(crate::paths::db_path()?).map_err(|error| error.to_string())?;
    conn.execute(
        "CREATE TABLE IF NOT EXISTS config (key TEXT PRIMARY KEY, value TEXT)",
        [],
    )
    .map_err(|error| error.to_string())?;
    Ok(conn)
}

fn key(provider: &str) -> Result<String, String> {
    if !PROVIDERS.contains(&provider) {
        return Err("Unknown external provider.".into());
    }
    Ok(format!("local_agent_credential_{provider}"))
}

pub fn credential(provider: &str) -> Result<String, String> {
    crate::secure_config::load_secret(&connection()?, &key(provider)?)?
        .filter(|value| !value.is_empty())
        .ok_or_else(|| {
            format!("Connect {provider} in Local agent → Tools and saved preferences first.")
        })
}

pub fn client() -> Result<Client, String> {
    Client::builder()
        .timeout(Duration::from_secs(30))
        .build()
        .map_err(|error| error.to_string())
}

pub fn checked_json(response: Response, provider: &str) -> Result<Value, String> {
    if !response.status().is_success() {
        return Err(format!(
            "{provider} returned HTTP {}. Check the token, its scopes, and the target.",
            response.status()
        ));
    }
    response
        .json()
        .map_err(|_| format!("{provider} returned invalid JSON."))
}

#[tauri::command]
pub fn get_local_agent_connections() -> Result<Value, String> {
    let conn = connection()?;
    let mut configured = serde_json::Map::new();
    for provider in PROVIDERS {
        configured.insert(
            (*provider).to_string(),
            json!(crate::secure_config::load_secret(&conn, &key(provider)?)?.is_some()),
        );
    }
    let data = state::read()?;
    Ok(json!({
        "configured": configured,
        "calendarProvider": data.calendar_provider,
        "emailProvider": data.email_provider,
    }))
}

#[tauri::command]
pub fn save_local_agent_connection(provider: String, token: String) -> Result<(), String> {
    let token = token.trim();
    if token.is_empty() || token.chars().count() > 4096 {
        return Err("Enter an access token of up to 4,096 characters.".into());
    }
    crate::secure_config::save_secret(&connection()?, &key(&provider)?, token)
}

#[tauri::command]
pub fn remove_local_agent_connection(provider: String) -> Result<(), String> {
    crate::secure_config::delete_secret(&connection()?, &key(&provider)?)?;
    state::update(|data| {
        if data.calendar_provider.as_deref() == Some(&provider) {
            data.calendar_provider = None;
        }
        if data.email_provider.as_deref() == Some(&provider) {
            data.email_provider = None;
        }
        Ok(())
    })
}

#[tauri::command]
pub fn set_local_agent_providers(
    calendar_provider: Option<String>,
    email_provider: Option<String>,
) -> Result<(), String> {
    for provider in [&calendar_provider, &email_provider].into_iter().flatten() {
        if provider != "google" && provider != "microsoft" {
            return Err("Choose Google or Microsoft.".into());
        }
    }
    if let Some(provider) = calendar_provider.as_deref() {
        crate::calendar_companion::access_token(provider)?;
    }
    if let Some(provider) = email_provider.as_deref() {
        credential(provider)?;
    }
    state::update(|data| {
        data.calendar_provider = calendar_provider;
        data.email_provider = email_provider;
        Ok(())
    })
}
