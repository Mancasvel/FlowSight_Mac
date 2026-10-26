use crate::sync::get_user_session_from_conn;
use crate::sync_env::{supabase_anon_key, supabase_url};
use reqwest::blocking::Client;
use rusqlite::Connection;
use serde::{Deserialize, Serialize};
use tauri::Manager;

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct Entitlements {
    #[serde(default)]
    pub owner_user_id: Option<String>,
    pub plan: Option<String>,
    pub status: String,
    pub team_ids: Vec<String>,
    pub active_team_id: Option<String>,
    pub can_sync: bool,
    pub can_cloud_ai: bool,
    pub can_integrations: bool,
    #[serde(default)]
    pub usage_profile: Option<serde_json::Value>,
}

impl Entitlements {
    pub fn free() -> Self {
        Self {
            owner_user_id: None,
            plan: None,
            status: "free".to_string(),
            team_ids: vec![],
            active_team_id: None,
            can_sync: false,
            can_cloud_ai: false,
            can_integrations: false,
            usage_profile: None,
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
        owner_user_id: None,
        plan: value["plan"].as_str().map(String::from),
        status: value["status"].as_str().unwrap_or("free").to_string(),
        team_ids: team_ids.clone(),
        active_team_id: value["active_team_id"]
            .as_str()
            .map(String::from)
            .or_else(|| team_ids.first().cloned()),
        can_sync: features["sync"].as_bool().unwrap_or(false),
        can_cloud_ai: features["cloud_ai"].as_bool().unwrap_or(false),
        can_integrations: features["integrations"].as_bool().unwrap_or(false),
        usage_profile: value.get("usage_profile").cloned(),
    }
}

pub fn load_entitlements(conn: &Connection) -> Entitlements {
    let entitlements = conn
        .query_row(
            "SELECT value FROM config WHERE key = 'entitlements'",
            [],
            |row| row.get::<_, String>(0),
        )
        .ok()
        .and_then(|json| serde_json::from_str::<Entitlements>(&json).ok())
        .unwrap_or_else(Entitlements::free);
    if let Some(session) = get_user_session_from_conn(conn) {
        if entitlements.owner_user_id.as_deref() != Some(session.user_id.as_str()) {
            return Entitlements::free();
        }
    } else if entitlements.owner_user_id.is_some() {
        return Entitlements::free();
    }
    entitlements
}

/// Unconnected accounts default to Individual. An account-scoped team cache
/// retains worker restrictions during a temporary loss of network access.
pub(crate) fn profile_id(conn: &Connection) -> String {
    if get_user_session_from_conn(conn).is_none() {
        return "individual".into();
    }
    let entitlements = load_entitlements(conn);
    if let Some(id @ ("individual" | "team_employee" | "team_manager")) = entitlements
        .usage_profile
        .as_ref()
        .and_then(|p| p["id"].as_str())
    {
        return id.to_string();
    }
    if entitlements.plan.as_deref() == Some("team")
        || (entitlements.active_team_id.is_some()
            && entitlements.plan.as_deref() != Some("individual"))
    {
        "team_employee".into()
    } else {
        "individual".into()
    }
}

pub(crate) fn personal_features_allowed(path: &std::path::Path) -> bool {
    Connection::open(path)
        .map(|conn| profile_id(&conn) == "individual")
        .unwrap_or(false)
}

#[cfg(all(test, any(windows, target_os = "macos")))]
mod profile_tests {
    use super::*;

    fn account_cache(plan: Option<&str>, team: Option<&str>, profile: Option<&str>) -> Connection {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute("CREATE TABLE config (key TEXT PRIMARY KEY, value TEXT)", [])
            .unwrap();
        crate::secure_config::save_secret(&conn, "user_session", &serde_json::json!({
            "user_id":"worker", "team_id":team, "access_token":"synthetic", "refresh_token":null, "email":"worker@example.test"
        }).to_string()).unwrap();
        let mut value = Entitlements::free();
        value.owner_user_id = Some("worker".into());
        value.plan = plan.map(String::from);
        value.active_team_id = team.map(String::from);
        value.usage_profile = profile.map(|id| serde_json::json!({"id":id,"verified":true}));
        conn.execute(
            "INSERT INTO config (key,value) VALUES ('entitlements',?1)",
            [serde_json::to_string(&value).unwrap()],
        )
        .unwrap();
        conn
    }

    #[test]
    fn unconnected_and_legacy_individual_accounts_keep_personal_tools() {
        assert_eq!(profile_id(&account_cache(None, None, None)), "individual");
        assert_eq!(
            profile_id(&account_cache(Some("individual"), Some("personal"), None)),
            "individual"
        );
        let conn = account_cache(Some("team"), Some("company"), Some("team_employee"));
        conn.execute("DELETE FROM config WHERE key='user_session'", [])
            .unwrap();
        // Old caches could omit owner_user_id. Signing out still wins over
        // any profile or plan left by a legacy client.
        let mut legacy = load_entitlements(&conn);
        legacy.owner_user_id = None;
        legacy.plan = Some("team".into());
        legacy.usage_profile = Some(serde_json::json!({"id":"team_employee"}));
        conn.execute(
            "UPDATE config SET value=?1 WHERE key='entitlements'",
            [serde_json::to_string(&legacy).unwrap()],
        )
        .unwrap();
        assert_eq!(profile_id(&conn), "individual");
    }

    #[test]
    fn known_worker_cache_stays_restricted_without_network() {
        assert_eq!(
            profile_id(&account_cache(Some("team"), Some("company"), None)),
            "team_employee"
        );
        assert_eq!(
            profile_id(&account_cache(
                Some("team"),
                Some("company"),
                Some("team_employee")
            )),
            "team_employee"
        );
        // Old manager caches cannot grant personal tools against a company
        // upload destination before the canonical server refresh completes.
        assert_ne!(
            profile_id(&account_cache(
                Some("team"),
                Some("company"),
                Some("team_manager")
            )),
            "individual"
        );
        assert_eq!(
            profile_id(&account_cache(None, None, Some("individual"))),
            "individual"
        );
    }
}

pub fn save_entitlements(conn: &Connection, entitlements: &Entitlements) -> Result<(), String> {
    let _guard = crate::sync::SESSION_REFRESH_LOCK
        .lock()
        .map_err(|_| "Session unavailable")?;
    if let Some(owner) = entitlements.owner_user_id.as_deref() {
        let mut session = get_user_session_from_conn(conn)
            .ok_or("Account signed out while verifying the profile")?;
        if session.user_id != owner {
            return Err("Account changed while verifying the profile".into());
        }
        if entitlements.usage_profile.is_some() {
            let old = load_entitlements(conn);
            if session.team_id != entitlements.active_team_id
                || (old.usage_profile.is_some()
                    && old.usage_profile.as_ref().map(|p| &p["id"])
                        != entitlements.usage_profile.as_ref().map(|p| &p["id"]))
            {
                crate::local_agent::end_personal_context()?;
                crate::privacy::disable_cloud_for_context_change(conn)?;
            }
            session.team_id = entitlements.active_team_id.clone();
            crate::secure_config::save_secret(
                conn,
                "user_session",
                &serde_json::to_string(&session).map_err(|e| e.to_string())?,
            )?;
        }
    }
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

pub fn refresh_entitlements_from_supabase(
    access_token: &str,
    owner_user_id: &str,
) -> Result<Entitlements, String> {
    let client = Client::builder()
        .timeout(std::time::Duration::from_secs(20))
        .build()
        .map_err(|e| e.to_string())?;
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
        return Err(format!(
            "Failed to fetch entitlements (HTTP {}).",
            resp.status()
        ));
    }

    let json: serde_json::Value = resp.json().map_err(|e| e.to_string())?;
    let mut entitlements = parse_entitlements_json(&json);
    entitlements.owner_user_id = Some(owner_user_id.to_string());
    Ok(entitlements)
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
pub fn refresh_entitlements() -> Result<Entitlements, String> {
    let db_path = crate::paths::db_path()?;
    let conn = Connection::open(&db_path).map_err(|e| e.to_string())?;

    crate::sync::refresh_session_if_expiring(&db_path);
    let session =
        get_user_session_from_conn(&conn).ok_or("Not logged in — cannot refresh entitlements")?;

    let entitlements =
        match refresh_entitlements_from_supabase(&session.access_token, &session.user_id) {
            Err(error) if error.contains("401") => {
                let renewed = crate::sync::refresh_supabase_token(&session)?;
                refresh_entitlements_from_supabase(&renewed.access_token, &renewed.user_id)?
            }
            result => result?,
        };
    save_entitlements(&conn, &entitlements)?;
    Ok(entitlements)
}

fn account_rpc(
    session: &crate::sync::UserSession,
    name: &str,
    body: &serde_json::Value,
) -> Result<serde_json::Value, String> {
    let send=|active: &crate::sync::UserSession| -> Result<(reqwest::StatusCode,serde_json::Value),String> {
        let response=Client::builder().timeout(std::time::Duration::from_secs(20)).build().map_err(|e|e.to_string())?
            .post(format!("{}/rest/v1/rpc/{}",supabase_url(),name)).header("apikey",supabase_anon_key())
            .bearer_auth(&active.access_token).json(body).send().map_err(|_| "Could not connect to FlowSight. Check your connection and retry")?;
        let status=response.status();
        Ok((status,response.json().map_err(|_| "FlowSight account response unavailable")?))
    };
    let (mut status, mut value) = send(session)?;
    if status.as_u16() == 401 {
        (status, value) = send(&crate::sync::refresh_supabase_token(session)?)?;
    }
    if !status.is_success() {
        return Err(value["message"]
            .as_str()
            .unwrap_or("Account request could not be completed")
            .to_string());
    }
    Ok(value)
}

#[tauri::command]
pub async fn claim_license_code(
    code: String,
    app: tauri::AppHandle,
) -> Result<Entitlements, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let code = code.trim();
        if code.is_empty() || code.len() > 128 {
            return Err("Enter your license code (FS-XXXX-XXXX).".into());
        }
        let path = crate::paths::db_path()?;
        crate::sync::refresh_session_if_expiring(&path);
        let conn = Connection::open(&path).map_err(|e| e.to_string())?;
        let session =
            get_user_session_from_conn(&conn).ok_or("Sign in before activating a license")?;
        account_rpc(
            &session,
            "claim_license",
            &serde_json::json!({"p_code":code}),
        )?;
        if get_user_session_from_conn(&conn)
            .as_ref()
            .map(|s| s.user_id.as_str())
            != Some(session.user_id.as_str())
        {
            return Err("Account changed while activating the license".into());
        }
        crate::agent::stop_monitoring(app.state::<crate::agent::AgentState>())?;
        crate::local_agent::end_personal_context()?;
        let mut entitlements = refresh_entitlements()?;
        if entitlements.plan.as_deref() == Some("individual")
            && entitlements.active_team_id.is_none()
        {
            let active =
                get_user_session_from_conn(&conn).ok_or("Sign in before activating a license")?;
            account_rpc(&active, "ensure_personal_team", &serde_json::json!({}))?;
            entitlements = refresh_entitlements()?;
        }
        Ok(entitlements)
    })
    .await
    .map_err(|_| "License activation interrupted".to_string())?
}

#[tauri::command]
pub async fn set_usage_profile(
    mode: String,
    team_id: Option<String>,
    app: tauri::AppHandle,
) -> Result<Entitlements, String> {
    tauri::async_runtime::spawn_blocking(move || set_usage_profile_blocking(mode, team_id, &app))
        .await
        .map_err(|_| "Profile request interrupted".to_string())?
}

pub(crate) fn set_usage_profile_blocking(
    mode: String,
    team_id: Option<String>,
    app: &tauri::AppHandle,
) -> Result<Entitlements, String> {
    let path = crate::paths::db_path()?;
    crate::sync::refresh_session_if_expiring(&path);
    let conn = Connection::open(&path).map_err(|e| e.to_string())?;
    let session =
        get_user_session_from_conn(&conn).ok_or("Sign in to choose a licensed profile")?;
    let send = |active: &crate::sync::UserSession| -> Result<(reqwest::StatusCode,serde_json::Value),String> {
        let response = Client::builder().timeout(std::time::Duration::from_secs(20)).build().map_err(|e| e.to_string())?
            .post(format!("{}/rest/v1/rpc/set_flowsight_usage_profile",supabase_url()))
            .header("apikey",supabase_anon_key()).bearer_auth(&active.access_token)
            .json(&serde_json::json!({"p_mode":mode,"p_team_id":team_id})).send().map_err(|_| "Could not connect to FlowSight")?;
        let status=response.status();
        Ok((status,response.json().map_err(|_| "Profile response unavailable")?))
    };
    let (mut status, mut value) = send(&session)?;
    if status.as_u16() == 401 {
        (status, value) = send(&crate::sync::refresh_supabase_token(&session)?)?;
    }
    if !status.is_success() {
        return Err(value["message"]
            .as_str()
            .unwrap_or("Profile access could not be verified")
            .to_string());
    }
    let mut entitlements = parse_entitlements_json(&value);
    let old = load_entitlements(&conn);
    if old.active_team_id != entitlements.active_team_id
        || old.usage_profile.as_ref().map(|p| &p["id"])
            != entitlements.usage_profile.as_ref().map(|p| &p["id"])
    {
        crate::agent::stop_monitoring(app.state::<crate::agent::AgentState>())?;
        crate::local_agent::end_personal_context()?;
    }
    entitlements.owner_user_id = Some(session.user_id);
    save_entitlements(&conn, &entitlements)?;
    crate::online_leagues::request_sync();
    Ok(entitlements)
}
