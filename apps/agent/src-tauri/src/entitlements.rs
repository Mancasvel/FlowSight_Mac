use crate::sync::get_user_session_from_conn;
use crate::sync_env::{supabase_anon_key, supabase_url};
use reqwest::blocking::Client;
use rusqlite::Connection;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct Entitlements {
    pub plan: Option<String>,
    pub status: String,
    pub team_ids: Vec<String>,
    pub active_team_id: Option<String>,
    pub can_sync: bool,
    pub can_cloud_ai: bool,
    pub can_integrations: bool,
}

impl Entitlements {
    pub fn free() -> Self {
        Self {
            plan: None,
            status: "free".to_string(),
            team_ids: vec![],
            active_team_id: None,
            can_sync: false,
            can_cloud_ai: false,
            can_integrations: false,
        }
    }

    #[allow(dead_code)]
    pub fn is_paid(&self) -> bool {
        self.can_sync || self.can_cloud_ai || self.can_integrations
    }
}

fn parse_entitlements_json(value: &serde_json::Value) -> Entitlements {
    let features = &value["features"];
    let team_ids: Vec<String> = value["team_ids"]
        .as_array()
        .map(|arr| {
            arr.iter()
                .filter_map(|v| v.as_str().map(String::from))
                .collect()
        })
        .unwrap_or_default();

    Entitlements {
        plan: value["plan"].as_str().map(String::from),
        status: value["status"]
            .as_str()
            .unwrap_or("free")
            .to_string(),
        team_ids: team_ids.clone(),
        active_team_id: team_ids.first().cloned(),
        can_sync: features["sync"].as_bool().unwrap_or(false),
        can_cloud_ai: features["cloud_ai"].as_bool().unwrap_or(false),
        can_integrations: features["integrations"].as_bool().unwrap_or(false),
    }
}

pub fn load_entitlements(conn: &Connection) -> Entitlements {
    conn.query_row(
        "SELECT value FROM config WHERE key = 'entitlements'",
        [],
        |row| row.get::<_, String>(0),
    )
    .ok()
    .and_then(|json| serde_json::from_str::<Entitlements>(&json).ok())
    .unwrap_or_else(Entitlements::free)
}

pub fn save_entitlements(conn: &Connection, entitlements: &Entitlements) -> Result<(), String> {
    let json = serde_json::to_string(entitlements).map_err(|e| e.to_string())?;
    conn.execute(
        "INSERT OR REPLACE INTO config (key, value) VALUES ('entitlements', ?1)",
        [&json],
    )
    .map_err(|e| e.to_string())?;
    Ok(())
}

pub fn clear_entitlements(conn: &Connection) -> Result<(), String> {
    conn.execute("DELETE FROM config WHERE key = 'entitlements'", [])
        .map_err(|e| e.to_string())?;
    Ok(())
}

pub fn refresh_entitlements_from_supabase(access_token: &str) -> Result<Entitlements, String> {
    let client = Client::new();
    let url = format!("{}/rest/v1/rpc/get_user_entitlements", supabase_url());

    let resp = client
        .post(&url)
        .header("apikey", supabase_anon_key())
        .header("Authorization", format!("Bearer {}", access_token))
        .header("Content-Type", "application/json")
        .json(&serde_json::json!({}))
        .send()
        .map_err(|e| e.to_string())?;

    if !resp.status().is_success() {
        let body = resp.text().unwrap_or_default();
        return Err(format!("Failed to fetch entitlements: {}", body));
    }

    let json: serde_json::Value = resp.json().map_err(|e| e.to_string())?;
    Ok(parse_entitlements_json(&json))
}

pub fn require_feature(db_path: &std::path::Path, feature: &str) -> Result<(), String> {
    let conn = Connection::open(db_path).map_err(|e| e.to_string())?;
    let entitlements = load_entitlements(&conn);
    let allowed = match feature {
        "sync" => entitlements.can_sync,
        "cloud_ai" => entitlements.can_cloud_ai,
        "integrations" => entitlements.can_integrations,
        _ => false,
    };

    if allowed {
        Ok(())
    } else {
        Err(
            "This feature requires an Individual or Team license. Activate cloud features in Profile."
                .to_string(),
        )
    }
}

#[tauri::command]
pub fn get_entitlements() -> Result<Entitlements, String> {
    let db_path = crate::paths::db_path()?;
    let conn = Connection::open(&db_path).map_err(|e| e.to_string())?;
    Ok(load_entitlements(&conn))
}

#[tauri::command]
pub fn save_entitlements_command(entitlements: Entitlements) -> Result<(), String> {
    let db_path = crate::paths::db_path()?;
    let conn = Connection::open(&db_path).map_err(|e| e.to_string())?;
    save_entitlements(&conn, &entitlements)
}

#[tauri::command]
pub fn refresh_entitlements() -> Result<Entitlements, String> {
    let db_path = crate::paths::db_path()?;
    let conn = Connection::open(&db_path).map_err(|e| e.to_string())?;

    let session = get_user_session_from_conn(&conn)
        .ok_or("Not logged in — cannot refresh entitlements")?;

    let entitlements = refresh_entitlements_from_supabase(&session.access_token)?;
    save_entitlements(&conn, &entitlements)?;
    Ok(entitlements)
}

#[tauri::command]
pub fn fetch_cloud_insights(limit: Option<u32>) -> Result<Vec<serde_json::Value>, String> {
    let db_path = crate::paths::db_path()?;
    require_feature(&db_path, "cloud_ai")?;

    let conn = Connection::open(&db_path).map_err(|e| e.to_string())?;
    let session = get_user_session_from_conn(&conn).ok_or("Not logged in")?;

    let team_filter = session
        .team_id
        .as_ref()
        .map(|team_id| format!("&team_id=eq.{}", team_id))
        .unwrap_or_default();

    let max_rows = limit.unwrap_or(10).min(50);
    let url = format!(
        "{}/rest/v1/cloud_insights?select=*&order=created_at.desc&limit={}{}",
        supabase_url(),
        max_rows,
        team_filter
    );

    let client = Client::new();
    let resp = client
        .get(&url)
        .header("apikey", supabase_anon_key())
        .header("Authorization", format!("Bearer {}", session.access_token))
        .send()
        .map_err(|e| e.to_string())?;

    if !resp.status().is_success() {
        let body = resp.text().unwrap_or_default();
        return Err(format!("Failed to fetch cloud insights: {}", body));
    }

    resp.json::<Vec<serde_json::Value>>()
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub fn request_cloud_insights(period_days: Option<i32>, team_id: Option<String>) -> Result<serde_json::Value, String> {
    let db_path = crate::paths::db_path()?;
    require_feature(&db_path, "cloud_ai")?;

    let conn = Connection::open(&db_path).map_err(|e| e.to_string())?;
    let session = get_user_session_from_conn(&conn).ok_or("Not logged in")?;
    let entitlements = load_entitlements(&conn);

    let days = period_days.unwrap_or(7).clamp(1, 30);
    let resolved_team_id = team_id.or(session.team_id.clone());

    let mut body = serde_json::json!({
        "period_days": days,
        "team_id": resolved_team_id,
        "plan": entitlements.plan,
    });

    if entitlements.plan.as_deref() == Some("individual") {
        let local_report = crate::insights_local::build_local_insights_report(&db_path, days)?;
        body["local_report"] = local_report;
    }

    let client = Client::new();
    let url = format!("{}/functions/v1/generate-insights", supabase_url());
    let resp = client
        .post(&url)
        .header("apikey", supabase_anon_key())
        .header("Authorization", format!("Bearer {}", session.access_token))
        .header("Content-Type", "application/json")
        .json(&body)
        .send()
        .map_err(|e| e.to_string())?;

    if !resp.status().is_success() {
        let body = resp.text().unwrap_or_default();
        return Err(format!("Failed to generate cloud insights: {}", body));
    }

    resp.json::<serde_json::Value>()
        .map_err(|e| e.to_string())
}
