use reqwest::blocking::Client;
use rusqlite::Connection;
use serde::{Deserialize, Serialize};
use std::error::Error;
use std::time::Duration;

// Constants for FlowSight (Registered Atlassian App)
// In a real production app, Client ID is public, Secret is NOT used for Public Clients (PKCE)
// However, Atlassian 3LO sometimes requires a "dummy" secret or strictly follows Code flow.
// For installed apps (Public Client), we usually don't send a secret, or send an empty one.
const TOKEN_URL: &str = "https://auth.atlassian.com/oauth/token";

fn get_client_id() -> String {
    crate::oauth_env::jira_client_id()
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct JiraIssue {
    pub key: String,
    pub summary: String,
    pub status: String,
}

// Persisted Config - (Items stored in generic config table)

fn get_client_secret() -> Option<String> {
    crate::oauth_env::jira_client_secret()
}

fn save_tokens(access: &str, refresh: Option<&str>) {
    let db_path = match crate::paths::db_path() {
        Ok(p) => p,
        Err(e) => {
            eprintln!("[jira] save_tokens: {}", e);
            return;
        }
    };
    if let Ok(conn) = Connection::open(db_path) {
        let _ = crate::secure_config::save_secret(&conn, "jira_access_token", access);
        if let Some(r) = refresh {
            let _ = crate::secure_config::save_secret(&conn, "jira_refresh_token", r);
        }

        // Also fetch Cloud ID (simplification: assume single cloud resource)
        if let Ok(cloud_id) = fetch_cloud_id(access) {
            let _ = conn.execute(
                "INSERT OR REPLACE INTO config (key, value) VALUES ('jira_cloud_id', ?)",
                [cloud_id],
            );
        }
    }
}

/// Refreshes the access token using the stored refresh token
/// Returns the new access token if successful
fn refresh_access_token() -> Result<String, String> {
    let db_path = crate::paths::db_path()?;
    let conn = Connection::open(&db_path).map_err(|e| e.to_string())?;

    let refresh_token = crate::secure_config::load_secret(&conn, "jira_refresh_token")?
        .ok_or_else(|| "No refresh token found. Please reconnect to Jira.".to_string())?;

    let client_id = get_client_id();
    let client_secret = get_client_secret();

    // Build the token refresh request
    let http_client = Client::builder()
        .timeout(Duration::from_secs(10))
        .build()
        .map_err(|e| e.to_string())?;
    let mut params = vec![
        ("grant_type", "refresh_token"),
        ("refresh_token", &refresh_token),
        ("client_id", &client_id),
    ];

    // Add client_secret if available (required for confidential clients)
    let secret_str;
    if let Some(ref secret) = client_secret {
        secret_str = secret.clone();
        params.push(("client_secret", &secret_str));
    }

    let resp = http_client
        .post(TOKEN_URL)
        .form(&params)
        .send()
        .map_err(|e| format!("Failed to refresh token: {}", e))?;

    if !resp.status().is_success() {
        let status = resp.status();
        println!("[Jira] Token refresh failed with HTTP {}", status);
        return Err(format!(
            "Token refresh failed ({}). Please reconnect to Jira.",
            status
        ));
    }

    let json: serde_json::Value = resp.json().map_err(|e| e.to_string())?;

    let new_access = json["access_token"]
        .as_str()
        .ok_or("No access_token in refresh response")?
        .to_string();

    let new_refresh = json["refresh_token"].as_str().map(String::from);

    // Save the new tokens
    save_tokens(&new_access, new_refresh.as_deref());
    println!("[Jira] Token refreshed successfully");

    Ok(new_access)
}

/// Gets a valid access token, refreshing if necessary
/// This is the main entry point for getting a token to use in API calls
pub(crate) fn get_valid_token() -> Result<String, String> {
    let db_path = crate::paths::db_path()?;
    let conn = Connection::open(&db_path).map_err(|e| e.to_string())?;

    let access_token = crate::secure_config::load_secret(&conn, "jira_access_token")?
        .ok_or_else(|| "Not connected to Jira".to_string())?;

    // Quick validation: try to access a lightweight endpoint
    let http_client = Client::builder()
        .timeout(Duration::from_secs(10))
        .build()
        .map_err(|e| e.to_string())?;
    let test_resp = http_client
        .get("https://api.atlassian.com/oauth/token/accessible-resources")
        .bearer_auth(&access_token)
        .send();

    match test_resp {
        Ok(resp) if resp.status().as_u16() == 401 => {
            // Token expired, try to refresh
            println!("[Jira] Access token expired, attempting refresh...");
            refresh_access_token()
        }
        Ok(resp) if resp.status().is_success() => {
            // Token is still valid
            Ok(access_token)
        }
        Ok(resp) => {
            // Other error
            Err(format!("Jira API error: {}", resp.status()))
        }
        Err(e) => {
            // Network error, return current token and let caller handle it
            println!("[Jira] Network check failed: {}, using cached token", e);
            Ok(access_token)
        }
    }
}

fn fetch_cloud_id(token: &str) -> Result<String, Box<dyn Error>> {
    let client = Client::builder()
        .timeout(Duration::from_secs(10))
        .build()
        .map_err(|e| e.to_string())?;
    let resp = client
        .get("https://api.atlassian.com/oauth/token/accessible-resources")
        .bearer_auth(token)
        .send()?;

    let json: serde_json::Value = resp.json()?;
    // Get first resource ID
    json[0]["id"]
        .as_str()
        .map(String::from)
        .ok_or("No accessible resources".into())
}

// Async so Tauri keeps the blocking `reqwest` calls below off the main
// thread: each one opens a real socket, which is what lets Windows inject a
// broken Winsock LSP into this process (see crash_guard.rs module docs).
#[tauri::command]
pub async fn fetch_jira_tasks() -> Result<Vec<JiraIssue>, String> {
    tauri::async_runtime::spawn_blocking(fetch_jira_tasks_blocking)
        .await
        .map_err(|e| format!("Task join error: {}", e))?
}

pub(crate) fn fetch_jira_tasks_blocking() -> Result<Vec<JiraIssue>, String> {
    let db_path = crate::paths::db_path()?;
    crate::entitlements::require_feature(&db_path, "integrations")?;
    // 1. Get valid token (auto-refreshes if expired)
    let access_token = get_valid_token()?;

    let db_path = crate::paths::db_path()?;
    let conn = Connection::open(db_path).map_err(|e| e.to_string())?;
    let cloud_id: String = conn
        .query_row(
            "SELECT value FROM config WHERE key = 'jira_cloud_id'",
            [],
            |r| r.get(0),
        )
        .map_err(|_| "Jira Cloud ID not found".to_string())?;

    // 2. Fetch Issues
    let client = Client::new();
    let url = format!(
        "https://api.atlassian.com/ex/jira/{}/rest/api/3/search/jql",
        cloud_id
    );
    let jql = "statusCategory != Done ORDER BY updated DESC";

    let resp = client
        .post(&url)
        .bearer_auth(&access_token)
        .header("Content-Type", "application/json")
        .json(&serde_json::json!({
            "jql": jql,
            "fields": ["summary", "status"],
            "maxResults": 50
        }))
        .send()
        .map_err(|e| e.to_string())?;

    println!("[Jira] Fetch Status: {}", resp.status());

    // Handle 401 with retry after refresh
    if resp.status().as_u16() == 401 {
        println!("[Jira] Got 401, attempting token refresh...");
        let new_token = refresh_access_token()?;

        // Retry with new token
        let retry_resp = client
            .post(&url)
            .bearer_auth(&new_token)
            .header("Content-Type", "application/json")
            .json(&serde_json::json!({
                "jql": jql,
                "fields": ["summary", "status"],
                "maxResults": 50
            }))
            .send()
            .map_err(|e| e.to_string())?;

        if !retry_resp.status().is_success() {
            return Err(format!(
                "Jira API failed after refresh: {}",
                retry_resp.status()
            ));
        }

        return parse_jira_issues(retry_resp.text().map_err(|e| e.to_string())?);
    }

    let text_resp = resp.text().map_err(|e| e.to_string())?;
    parse_jira_issues(text_resp)
}

fn parse_jira_issues(text_resp: String) -> Result<Vec<JiraIssue>, String> {
    let json: serde_json::Value = serde_json::from_str(&text_resp).map_err(|e| e.to_string())?;

    let mut issues = Vec::new();
    if let Some(opts) = json["issues"].as_array() {
        for i in opts {
            let key = i["key"].as_str().unwrap_or_default().to_string();
            let summary = i["fields"]["summary"]
                .as_str()
                .unwrap_or_default()
                .to_string();
            let status = i["fields"]["status"]["name"]
                .as_str()
                .unwrap_or_default()
                .to_string();
            issues.push(JiraIssue {
                key,
                summary,
                status,
            });
        }
    }
    Ok(issues)
}
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct JiraUser {
    pub display_name: String,
    pub avatar_url: String,
    pub email: String,
}

// Async so Tauri keeps the blocking `reqwest` calls below off the main
// thread (see crash_guard.rs module docs).
#[tauri::command]
pub async fn fetch_jira_profile() -> Result<JiraUser, String> {
    tauri::async_runtime::spawn_blocking(fetch_jira_profile_blocking)
        .await
        .map_err(|e| format!("Task join error: {}", e))?
}

fn fetch_jira_profile_blocking() -> Result<JiraUser, String> {
    let db_path = crate::paths::db_path()?;
    crate::entitlements::require_feature(&db_path, "integrations")?;
    // 1. Get valid token (auto-refreshes if expired)
    let access_token = get_valid_token()?;

    let db_path = crate::paths::db_path()?;
    let conn = Connection::open(&db_path).map_err(|e| e.to_string())?;
    let cloud_id: String = conn
        .query_row(
            "SELECT value FROM config WHERE key = 'jira_cloud_id'",
            [],
            |r| r.get(0),
        )
        .map_err(|_| "Jira Cloud ID not found".to_string())?;

    // 2. Call /myself
    let client = Client::new();
    let url = format!(
        "https://api.atlassian.com/ex/jira/{}/rest/api/3/myself",
        cloud_id
    );

    let resp = client
        .get(&url)
        .bearer_auth(&access_token)
        .send()
        .map_err(|e| e.to_string())?;

    // Handle 401 with retry after refresh
    if resp.status().as_u16() == 401 {
        println!("[Jira] Profile fetch got 401, refreshing token...");
        let new_token = refresh_access_token()?;

        let retry_resp = client
            .get(&url)
            .bearer_auth(&new_token)
            .send()
            .map_err(|e| e.to_string())?;

        if !retry_resp.status().is_success() {
            return Err(format!(
                "Failed to fetch profile after refresh: {}",
                retry_resp.status()
            ));
        }

        return parse_jira_profile(retry_resp.json().map_err(|e| e.to_string())?, &conn);
    }

    if !resp.status().is_success() {
        return Err(format!("Failed to fetch profile: {}", resp.status()));
    }

    parse_jira_profile(resp.json().map_err(|e| e.to_string())?, &conn)
}

fn parse_jira_profile(json: serde_json::Value, conn: &Connection) -> Result<JiraUser, String> {
    let user = JiraUser {
        display_name: json["displayName"]
            .as_str()
            .unwrap_or("Unknown")
            .to_string(),
        avatar_url: json["avatarUrls"]["48x48"]
            .as_str()
            .unwrap_or("")
            .to_string(),
        email: json["emailAddress"].as_str().unwrap_or("").to_string(),
    };

    // Update config "dev_name" automatically
    let _ = conn.execute(
        "INSERT OR REPLACE INTO config (key, value) VALUES ('dev_name', ?)",
        [&user.display_name],
    );

    Ok(user)
}
