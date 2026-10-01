//! Intentional browser protection, independent of telemetry and cloud plans.
use chrono::{Duration, Utc};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::sync::Mutex;

use super::{browser_bridge, state};

static CONTROL: Mutex<()> = Mutex::new(());

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct Preferences {
    pub patterns: Vec<String>,
    pub exceptions: Vec<String>,
    pub duration_minutes: u16,
}

impl Default for Preferences {
    fn default() -> Self {
        Self {
            patterns: vec!["instagram.com".into(), "tiktok.com".into(), "x.com".into()],
            exceptions: Vec::new(),
            duration_minutes: 50,
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Session {
    pub id: String,
    pub intention: String,
    pub expires_at: String,
    pub patterns: Vec<String>,
    pub exceptions: Vec<String>,
}

fn normalize(pattern: &str) -> Result<String, String> {
    let pattern = pattern.trim();
    if pattern.is_empty() || pattern.len() > 240 || pattern.chars().any(char::is_whitespace) {
        return Err("Use a domain or HTTP(S) path without spaces.".into());
    }
    let input = if pattern.contains("://") {
        pattern.to_string()
    } else {
        format!("https://{pattern}")
    };
    let url = url::Url::parse(&input).map_err(|_| "Use a valid domain or HTTP(S) path.")?;
    let host = url
        .host_str()
        .ok_or("Use a valid domain or HTTP(S) path.")?;
    if !matches!(url.scheme(), "http" | "https")
        || !url.username().is_empty()
        || url.password().is_some()
        || url.query().is_some()
        || url.fragment().is_some()
        || url.port().is_some()
        || !host.contains('.')
        || host.ends_with('.')
        || host.parse::<std::net::IpAddr>().is_ok()
        || host == "localhost"
    {
        return Err(
            "Use a public domain or HTTP(S) path, without a port, query, or fragment.".into(),
        );
    }
    Ok(format!(
        "{}{}",
        host.strip_prefix("www.").unwrap_or(host),
        if url.path() == "/" { "" } else { url.path() }
    ))
}

pub fn validate(mut preferences: Preferences) -> Result<Preferences, String> {
    if !(5..=180).contains(&preferences.duration_minutes)
        || preferences.patterns.is_empty()
        || preferences.patterns.len() > 20
        || preferences.exceptions.len() > 20
    {
        return Err("Choose 5–180 minutes, 1–20 blocked sites, and up to 20 exceptions.".into());
    }
    for list in [&mut preferences.patterns, &mut preferences.exceptions] {
        *list = list
            .iter()
            .map(|item| normalize(item))
            .collect::<Result<Vec<_>, _>>()?;
        list.sort();
        list.dedup();
    }
    Ok(preferences)
}

pub fn policy() -> Result<Option<Session>, String> {
    Ok(state::read()?.total_focus.filter(|session| {
        chrono::DateTime::parse_from_rfc3339(&session.expires_at)
            .is_ok_and(|until| until > Utc::now())
    }))
}

#[tauri::command]
pub fn get_total_focus() -> Result<Value, String> {
    let data = state::read()?;
    Ok(
        json!({"preferences":data.total_focus_preferences,"session":policy()?,
        "browser":browser_bridge::focus_status(),"messagingAvailable":false}),
    )
}

#[tauri::command]
pub fn save_total_focus_preferences(preferences: Preferences) -> Result<Preferences, String> {
    let preferences = validate(preferences)?;
    state::update(|data| {
        data.total_focus_preferences = preferences.clone();
        Ok(())
    })?;
    Ok(preferences)
}

pub fn activate(intention: String, preferences: Preferences) -> Result<Value, String> {
    let _guard = CONTROL.lock().map_err(|error| error.to_string())?;
    let preferences = validate(preferences)?;
    if intention.trim().is_empty() || intention.chars().count() > 160 {
        return Err("Describe your focus task in 1–160 characters.".into());
    }
    if policy()?.is_some() {
        return Err(
            "Total focus is already active. End it before starting another session.".into(),
        );
    }
    let session = Session {
        id: uuid::Uuid::new_v4().to_string(),
        intention: intention.trim().into(),
        expires_at: (Utc::now() + Duration::minutes(preferences.duration_minutes.into()))
            .to_rfc3339(),
        patterns: preferences.patterns.clone(),
        exceptions: preferences.exceptions.clone(),
    };
    state::update(|data| {
        data.total_focus = Some(session.clone());
        Ok(())
    })?;
    // The extension reconciles the authoritative policy before returning this ACK.
    let applied = browser_bridge::execute("browser.focus_status", &json!({}));
    match applied {
        Ok(value) if value["sessionId"] == session.id && value["applied"] == true => {
            get_total_focus()
        }
        result => {
            state::update(|data| {
                data.total_focus = None;
                Ok(())
            })?;
            Err(result.err().unwrap_or_else(|| {
                "The extension did not apply total focus. Check its connection and try again."
                    .into()
            }))
        }
    }
}

pub fn end() -> Result<Value, String> {
    let _guard = CONTROL.lock().map_err(|error| error.to_string())?;
    state::update(|data| {
        data.total_focus = None;
        Ok(())
    })?;
    let result = browser_bridge::execute("browser.focus_status", &json!({}));
    Ok(
        json!({"ended":true,"browserReleased":result.as_ref().is_ok_and(|value| value["applied"] == false),
        "warning":result.err(),"state":get_total_focus()?}),
    )
}

#[tauri::command]
pub async fn start_total_focus(
    intention: String,
    preferences: Preferences,
) -> Result<Value, String> {
    tauri::async_runtime::spawn_blocking(move || activate(intention, preferences))
        .await
        .map_err(|error| format!("Total focus worker failed: {error}"))?
}

#[tauri::command]
pub async fn end_total_focus() -> Result<Value, String> {
    tauri::async_runtime::spawn_blocking(end)
        .await
        .map_err(|error| format!("Total focus worker failed: {error}"))?
}

pub fn cancel_from_extension(id: &str) -> Result<(), String> {
    state::update(|data| {
        if data
            .total_focus
            .as_ref()
            .is_some_and(|session| session.id == id)
        {
            data.total_focus = None;
        }
        Ok(())
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn validates_public_sites_and_preserves_path_exceptions() {
        let preferences = validate(Preferences {
            patterns: vec!["HTTPS://YouTube.com".into(), "www.youtube.com".into()],
            exceptions: vec!["youtube.com/watch".into()],
            duration_minutes: 50,
        })
        .unwrap();
        assert_eq!(preferences.patterns, vec!["youtube.com"]);
        assert_eq!(preferences.exceptions, vec!["youtube.com/watch"]);
        for bad in [
            "file:///etc/passwd",
            "127.0.0.1",
            "localhost",
            "https://a.com?x=1",
            "a.com:123",
            "a.com#x",
            "https://user@a.com",
            "a b.com",
        ] {
            assert!(normalize(bad).is_err(), "{bad}");
        }
    }
    #[test]
    fn preferences_bound_duration_and_lists() {
        assert!(validate(Preferences::default()).is_ok());
        assert!(validate(Preferences {
            duration_minutes: 0,
            ..Preferences::default()
        })
        .is_err());
        assert!(validate(Preferences {
            patterns: vec![],
            ..Preferences::default()
        })
        .is_err());
    }
}
