use chrono::Local;
use rusqlite::{params, Connection};
use serde::{Deserialize, Serialize};

const PREFS_KEY: &str = "user_preferences";

#[derive(Serialize, Deserialize, Clone, Debug, Default)]
pub struct UserPreferences {
    #[serde(rename = "onboardingCompleted", default)]
    pub onboarding_completed: bool,
    #[serde(rename = "displayName", default)]
    pub display_name: Option<String>,
    /// e.g. software_developer, designer, product_manager
    #[serde(rename = "workRoles", default)]
    pub work_roles: Vec<String>,
    /// Free-text job when `other` is selected in work_roles
    #[serde(rename = "customJob", default)]
    pub custom_job: Option<String>,
    /// e.g. coding, debugging, meetings
    #[serde(rename = "workActivities", default)]
    pub work_activities: Vec<String>,
    /// e.g. focus, distractions, time_awareness
    #[serde(rename = "improvementGoals", default)]
    pub improvement_goals: Vec<String>,
    #[serde(rename = "dailyGoalHours", default)]
    pub daily_goal_hours: Option<f64>,
    #[serde(rename = "updatedAt", default)]
    pub updated_at: Option<String>,
}

pub fn load_user_preferences(db_path: &std::path::Path) -> Result<UserPreferences, String> {
    let conn = Connection::open(db_path).map_err(|e| e.to_string())?;
    let raw: Option<String> = conn
        .query_row(
            "SELECT value FROM config WHERE key = ?1",
            params![PREFS_KEY],
            |row| row.get(0),
        )
        .ok();

    match raw {
        Some(json) => serde_json::from_str(&json).map_err(|e| format!("Invalid preferences JSON: {}", e)),
        None => Ok(UserPreferences::default()),
    }
}

pub fn save_user_preferences(
    db_path: &std::path::Path,
    mut prefs: UserPreferences,
) -> Result<UserPreferences, String> {
    prefs.updated_at = Some(Local::now().format("%Y-%m-%d %H:%M").to_string());
    let json = serde_json::to_string(&prefs).map_err(|e| e.to_string())?;
    let conn = Connection::open(db_path).map_err(|e| e.to_string())?;
    conn.execute(
        "INSERT OR REPLACE INTO config (key, value) VALUES (?1, ?2)",
        params![PREFS_KEY, json],
    )
    .map_err(|e| e.to_string())?;
    Ok(prefs)
}

/// Compact block for local LLM report personalization.
pub fn preferences_llm_block(prefs: &UserPreferences) -> String {
    if prefs.onboarding_completed
        || !prefs.work_roles.is_empty()
        || !prefs.improvement_goals.is_empty()
    {
        // ok
    } else {
        return String::new();
    }

    let name = prefs.display_name.as_deref().unwrap_or("User");
    let roles = format_work_roles_for_llm(prefs);
    let activities = if prefs.work_activities.is_empty() {
        "unspecified".to_string()
    } else {
        prefs.work_activities.join(", ")
    };
    let goals = if prefs.improvement_goals.is_empty() {
        "unspecified".to_string()
    } else {
        prefs.improvement_goals.join(", ")
    };
    let goal_h = prefs
        .daily_goal_hours
        .map(|h| format!("{:.0}h/day", h))
        .unwrap_or_else(|| "not set".to_string());

    format!(
        "USER_PROFILE: name={} | roles={} | activities={} | improve={} | daily_goal={}",
        name, roles, activities, goals, goal_h
    )
}

fn format_work_roles_for_llm(prefs: &UserPreferences) -> String {
    if prefs.work_roles.is_empty() {
        return "unspecified".to_string();
    }
    let custom = prefs
        .custom_job
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty());
    let parts: Vec<String> = prefs
        .work_roles
        .iter()
        .map(|id| {
            if id == "other" {
                custom
                    .map(|job| format!("other: {}", job))
                    .unwrap_or_else(|| "other".to_string())
            } else {
                id.clone()
            }
        })
        .collect();
    parts.join(", ")
}

#[tauri::command]
pub fn get_user_preferences() -> Result<UserPreferences, String> {
    let db_path = crate::paths::db_path()?;
    load_user_preferences(&db_path)
}

#[tauri::command]
pub fn save_user_preferences_command(prefs: UserPreferences) -> Result<UserPreferences, String> {
    let db_path = crate::paths::db_path()?;
    save_user_preferences(&db_path, prefs)
}
