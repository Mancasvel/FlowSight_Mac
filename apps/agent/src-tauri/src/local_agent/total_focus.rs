//! Intentional browser protection, independent of telemetry and cloud plans.
use crate::agent::AgentState;
use chrono::{Duration, Utc};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::sync::Mutex;
use tauri::{AppHandle, Manager};

use super::{browser_bridge, state, system_quiet};

static CONTROL: Mutex<()> = Mutex::new(());
static LINKED_CONTROL: Mutex<()> = Mutex::new(());

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct Preferences {
    pub patterns: Vec<String>,
    pub exceptions: Vec<String>,
    pub duration_minutes: u16,
    pub quiet_notifications: bool,
    pub music_enabled: bool,
    pub music_url: String,
}

impl Default for Preferences {
    fn default() -> Self {
        Self {
            patterns: vec!["instagram.com".into(), "tiktok.com".into(), "x.com".into()],
            exceptions: Vec::new(),
            duration_minutes: 50,
            quiet_notifications: true,
            music_enabled: false,
            music_url: String::new(),
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
    #[serde(default)]
    pub quiet_notifications: bool,
    #[serde(default)]
    pub linked_clock: bool,
    #[serde(default)]
    pub paused_at: Option<String>,
    #[serde(default)]
    pub remaining_seconds: i64,
    #[serde(default)]
    pub music_url: Option<String>,
    #[serde(default)]
    pub music_warning: Option<String>,
}

impl Session {
    pub fn is_active(&self) -> bool {
        self.paused_at.is_none() && self.remaining() > 0
    }
    pub fn remaining(&self) -> i64 {
        if self.paused_at.is_some() {
            self.remaining_seconds.max(0)
        } else {
            chrono::DateTime::parse_from_rfc3339(&self.expires_at)
                .map(|until| {
                    (until.with_timezone(&Utc) - Utc::now())
                        .num_seconds()
                        .max(0)
                })
                .unwrap_or(0)
        }
    }
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
    preferences.music_url = preferences.music_url.trim().to_string();
    if !preferences.music_url.is_empty() {
        preferences.music_url = validate_music_url(&preferences.music_url)?;
    } else if preferences.music_enabled {
        return Err("Add an HTTP(S) music link before enabling background music.".into());
    }
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

fn validate_music_url(raw: &str) -> Result<String, String> {
    let error = "Use a complete public HTTP(S) music link without spaces or sign-in credentials.";
    if raw.len() > 2048 || raw.chars().any(|c| c.is_whitespace() || c.is_control()) {
        return Err(error.into());
    }
    let url = url::Url::parse(raw).map_err(|_| error)?;
    let host = url.host_str().ok_or(error)?;
    if !matches!(url.scheme(), "http" | "https")
        || !url.username().is_empty()
        || url.password().is_some()
        || url.port().is_some()
        || !host.contains('.')
        || host.ends_with('.')
        || host.parse::<std::net::IpAddr>().is_ok()
    {
        return Err(error.into());
    }
    Ok(url.to_string())
}

pub fn policy() -> Result<Option<Session>, String> {
    Ok(state::read()?.total_focus.filter(Session::is_active))
}

#[tauri::command]
pub fn get_total_focus() -> Result<Value, String> {
    let data = state::read()?;
    Ok(
        json!({"preferences":data.total_focus_preferences,"session":data.total_focus.as_ref().filter(|session| session.is_active() || session.paused_at.is_some()),
        "browser":browser_bridge::focus_status(),"messagingAvailable":false,
        "systemNotificationsAvailable":cfg!(windows),"systemNotificationsQuiet":system_quiet::total_focus_confirmed(),
        "digest":if state::holding_notifications(&data) { Vec::new() } else { data.notification_digest }}),
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
    super::require_profile_tool("focus.total_start", &json!({}))?;
    activate_with(intention, preferences, browser_bridge::focus_status, || {
        let session = policy()?.ok_or("Total focus ended before the browser could apply it.")?;
        browser_bridge::execute(
            "browser.focus_status",
            &json!({"openMusic":session.music_url.is_some(),"sessionId":session.id}),
        )
    })
}

pub fn normalize_intention(intention: &str) -> Result<String, String> {
    let intention = intention.trim();
    if intention.chars().count() > 160 {
        return Err("Keep your focus task within 160 characters.".into());
    }
    Ok(if intention.is_empty() {
        "General"
    } else {
        intention
    }
    .into())
}

fn activate_with(
    intention: String,
    preferences: Preferences,
    status: impl FnOnce() -> Value,
    apply: impl FnOnce() -> Result<Value, String>,
) -> Result<Value, String> {
    let _guard = CONTROL.lock().map_err(|error| error.to_string())?;
    let preferences = validate(preferences)?;
    let intention = normalize_intention(&intention)?;
    if state::read()?
        .total_focus
        .is_some_and(|session| session.is_active() || session.paused_at.is_some())
    {
        return Err(
            "Total focus is already active. End it before starting another session.".into(),
        );
    }
    let browser = status();
    if browser["totalFocusAvailable"] != true {
        return Err(
            "Update Browser Controls in your browser and reconnect it before starting total focus."
                .into(),
        );
    }
    if preferences.music_enabled && browser["musicAvailable"] != true {
        return Err("Update Browser Controls to open music in the background.".into());
    }
    let session = Session {
        id: uuid::Uuid::new_v4().to_string(),
        intention,
        expires_at: (Utc::now() + Duration::minutes(preferences.duration_minutes.into()))
            .to_rfc3339(),
        patterns: preferences.patterns.clone(),
        exceptions: preferences.exceptions.clone(),
        quiet_notifications: preferences.quiet_notifications,
        linked_clock: false,
        paused_at: None,
        remaining_seconds: i64::from(preferences.duration_minutes) * 60,
        music_url: preferences
            .music_enabled
            .then(|| preferences.music_url.clone()),
        music_warning: None,
    };
    state::update(|data| {
        data.total_focus = Some(session.clone());
        Ok(())
    })?;
    if system_quiet::system_notifications_supported() && session.quiet_notifications {
        if let Err(error) = system_quiet::enable_total_focus(preferences.duration_minutes.into()) {
            clear_session(Some(&session.id))?;
            release_notifications()?;
            return Err(error);
        }
    }
    // The extension reconciles the authoritative policy before returning this ACK.
    let applied = apply();
    match applied {
        Ok(value)
            if value["sessionId"] == session.id
                && value["applied"] == true
                && policy()?.is_some_and(|current| current.id == session.id) =>
        {
            let music_warning = value["musicError"].as_str().or_else(|| {
                (session.music_url.is_some() && value["musicOpened"] != true)
                    .then_some("The browser did not confirm opening the music link.")
            });
            if let Some(warning) = music_warning {
                state::update(|data| {
                    if let Some(current) = data
                        .total_focus
                        .as_mut()
                        .filter(|current| current.id == session.id)
                    {
                        current.music_warning = Some(warning.into());
                    }
                    Ok(())
                })?;
            }
            get_total_focus()
        }
        result => {
            clear_session(Some(&session.id))?;
            release_notifications()?;
            Err(result.err().unwrap_or_else(|| {
                "The extension did not apply total focus. Check its connection and try again."
                    .into()
            }))
        }
    }
}

pub fn end() -> Result<Value, String> {
    let _guard = CONTROL.lock().map_err(|error| error.to_string())?;
    let previous = state::read()?.total_focus;
    clear_session(None)?;
    let notification_warning = release_notifications().err();
    let result = browser_bridge::execute(
        "browser.focus_status",
        &previous.map_or_else(|| json!({}), |session| {
            json!({"musicPlayback":"release","musicSessionId":session.id,"sessionId":session.id})
        }),
    );
    Ok(
        json!({"ended":true,"browserReleased":result.as_ref().is_ok_and(|value| value["applied"] == false),
        "warning":result.err(),"notificationWarning":notification_warning,"state":get_total_focus()?}),
    )
}

#[tauri::command]
pub async fn start_total_focus(
    app: AppHandle,
    intention: String,
    preferences: Preferences,
) -> Result<Value, String> {
    tauri::async_runtime::spawn_blocking(move || activate_linked(&app, intention, preferences))
        .await
        .map_err(|error| format!("Total focus worker failed: {error}"))?
}

#[tauri::command]
pub async fn end_total_focus(app: AppHandle) -> Result<Value, String> {
    tauri::async_runtime::spawn_blocking(move || end_linked(&app))
        .await
        .map_err(|error| format!("Total focus worker failed: {error}"))?
}

pub fn activate_linked(
    app: &AppHandle,
    intention: String,
    preferences: Preferences,
) -> Result<Value, String> {
    let _guard = LINKED_CONTROL.lock().map_err(|error| error.to_string())?;
    let intention = normalize_intention(&intention)?;
    crate::privacy::require_monitoring_acknowledgement(&crate::paths::db_path()?)?;
    if state::read()?
        .focus
        .is_some_and(|focus| focus.status != "ended")
    {
        return Err("End the existing focus block before starting total focus.".into());
    }
    activate(intention.clone(), preferences)?;
    if let Err(error) = crate::agent::start_monitoring(app.state::<AgentState>()) {
        let _ = end();
        return Err(error);
    }
    if let Err(error) = state::update(|data| {
        let session = data
            .total_focus
            .as_mut()
            .ok_or("Total focus ended before the clock could start.")?;
        session.linked_clock = true;
        data.total_focus_clock_stop_pending = false;
        Ok(())
    }) {
        let _ = crate::agent::stop_monitoring(app.state::<AgentState>());
        let _ = end();
        return Err(error);
    }
    crate::agent::set_task_context(Some(intention), None)?;
    get_total_focus()
}

pub fn end_linked(app: &AppHandle) -> Result<Value, String> {
    let _guard = LINKED_CONTROL.lock().map_err(|error| error.to_string())?;
    if state::read()?
        .total_focus
        .is_some_and(|session| session.linked_clock)
    {
        crate::agent::stop_monitoring(app.state::<AgentState>())?;
    }
    let result = end()?;
    state::update(|data| {
        data.total_focus_clock_stop_pending = false;
        Ok(())
    })?;
    Ok(result)
}

pub(super) fn end_for_context_change() -> Result<(), String> {
    let _guard = LINKED_CONTROL.lock().map_err(|error| error.to_string())?;
    if state::read()?.total_focus.is_some() {
        let _ = end()?;
    }
    state::update(|data| {
        data.total_focus_clock_stop_pending = false;
        Ok(())
    })?;
    release_notifications()
}

fn pause_with(apply: impl FnOnce(&Session) -> Result<Value, String>) -> Result<Value, String> {
    let _guard = CONTROL.lock().map_err(|error| error.to_string())?;
    let paused = state::update(|data| {
        let session = data
            .total_focus
            .as_mut()
            .ok_or("No total focus session is active.")?;
        if !session.is_active() {
            return Err("Total focus is already paused or ended.".into());
        }
        session.remaining_seconds = session.remaining();
        session.paused_at = Some(Utc::now().to_rfc3339());
        Ok(session.clone())
    })?;
    let notification_warning = release_notifications().err();
    let result = apply(&paused);
    if let Ok(value) = &result {
        update_music_warning(&paused.id, value)?;
    }
    Ok(
        json!({"browserReleased":result.as_ref().is_ok_and(|value| value["applied"] == false),
        "warning":result.err(),"notificationWarning":notification_warning,"state":get_total_focus()?}),
    )
}

fn resume_with(
    status: impl FnOnce() -> Value,
    apply: impl FnOnce(&Session) -> Result<Value, String>,
) -> Result<Value, String> {
    let _guard = CONTROL.lock().map_err(|error| error.to_string())?;
    let previous = state::read()?
        .total_focus
        .ok_or("No total focus session is paused.")?;
    if previous.paused_at.is_none() || previous.remaining_seconds <= 0 {
        return Err("No total focus session is paused.".into());
    }
    if status()["totalFocusAvailable"] != true {
        return Err("Reconnect Browser Controls before resuming total focus.".into());
    }
    let mut resumed = previous.clone();
    // A new policy identity prevents an old blocked page from cancelling a resumed session.
    resumed.id = uuid::Uuid::new_v4().to_string();
    resumed.paused_at = None;
    resumed.expires_at = (Utc::now() + Duration::seconds(previous.remaining_seconds)).to_rfc3339();
    state::update(|data| {
        data.total_focus = Some(resumed.clone());
        Ok(())
    })?;
    let result = (|| {
        if system_quiet::system_notifications_supported() && resumed.quiet_notifications {
            system_quiet::enable_total_focus((previous.remaining_seconds + 59) / 60)?;
        }
        let value = apply(&previous)?;
        if value["sessionId"] != resumed.id
            || value["applied"] != true
            || !policy()?.is_some_and(|session| session.id == resumed.id)
        {
            return Err(
                "The extension did not apply total focus. Check its connection and try again."
                    .into(),
            );
        }
        update_music_warning(&resumed.id, &value)?;
        get_total_focus()
    })();
    if result.is_err() {
        state::update(|data| {
            if data
                .total_focus
                .as_ref()
                .is_some_and(|session| session.id == resumed.id)
            {
                data.total_focus = Some(previous);
            }
            Ok(())
        })?;
        release_notifications()?;
    }
    result
}

fn update_music_warning(session_id: &str, value: &Value) -> Result<(), String> {
    state::update(|data| {
        if let Some(session) = data
            .total_focus
            .as_mut()
            .filter(|session| session.id == session_id)
        {
            if session.music_url.is_some() {
                session.music_warning = value["musicError"].as_str().map(str::to_string);
            }
        }
        Ok(())
    })
}

fn sync_music_playback(previous: &Session, playback: &str) -> Result<Value, String> {
    let session = state::read()?.total_focus.ok_or("Total focus has ended.")?;
    browser_bridge::execute(
        "browser.focus_status",
        &json!({"musicPlayback":playback,"musicSessionId":previous.id,"sessionId":session.id}),
    )
}

#[tauri::command]
pub async fn pause_total_focus(app: AppHandle) -> Result<Value, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let _guard = LINKED_CONTROL.lock().map_err(|error| error.to_string())?;
        crate::agent::stop_monitoring(app.state::<AgentState>())?;
        let result = pause_with(|session| sync_music_playback(session, "pause"));
        if result.is_err() && policy()?.is_some() {
            let _ = crate::agent::start_monitoring(app.state::<AgentState>());
        }
        result
    })
    .await
    .map_err(|error| format!("Total focus worker failed: {error}"))?
}

#[tauri::command]
pub async fn resume_total_focus(app: AppHandle) -> Result<Value, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let _guard = LINKED_CONTROL.lock().map_err(|error| error.to_string())?;
        super::require_profile_tool("focus.total_start", &json!({}))?;
        crate::privacy::require_monitoring_acknowledgement(&crate::paths::db_path()?)?;
        resume_with(browser_bridge::focus_status, |previous| {
            sync_music_playback(previous, "play")
        })?;
        if let Err(error) = crate::agent::start_monitoring(app.state::<AgentState>()) {
            let _ = pause_with(|session| sync_music_playback(session, "pause"));
            return Err(error);
        }
        if let Some(session) = state::read()?.total_focus {
            crate::agent::set_task_context(Some(session.intention), None)?;
        }
        get_total_focus()
    })
    .await
    .map_err(|error| format!("Total focus worker failed: {error}"))?
}

fn clear_session(id: Option<&str>) -> Result<(), String> {
    state::update(|data| {
        if id.is_none()
            || data
                .total_focus
                .as_ref()
                .is_some_and(|session| Some(session.id.as_str()) == id)
        {
            data.total_focus_clock_stop_pending |= data
                .total_focus
                .as_ref()
                .is_some_and(|session| session.linked_clock);
            data.total_focus = None;
        }
        Ok(())
    })
}

pub fn cancel_from_extension(id: &str) -> Result<(), String> {
    // Never wait for CONTROL here: the bridge must still acknowledge activation.
    clear_session(Some(id))?;
    if let Ok(_guard) = CONTROL.try_lock() {
        release_notifications()?;
    }
    Ok(())
}

pub fn release_notifications() -> Result<(), String> {
    if policy()?.is_none() && state::read()?.total_focus_quiet.is_some() {
        system_quiet::disable_total_focus()?;
    }
    Ok(())
}

pub fn maintain() -> Result<(), String> {
    let _guard = CONTROL.lock().map_err(|error| error.to_string())?;
    if state::read()?
        .total_focus
        .as_ref()
        .is_some_and(|session| session.paused_at.is_none() && !session.is_active())
    {
        state::update(|data| {
            if data
                .total_focus
                .as_ref()
                .is_some_and(|session| session.paused_at.is_none() && !session.is_active())
            {
                data.total_focus_clock_stop_pending |= data
                    .total_focus
                    .as_ref()
                    .is_some_and(|session| session.linked_clock);
                data.total_focus = None;
            }
            Ok(())
        })?;
    }
    release_notifications()
}

pub fn maintain_linked_clock(app: &AppHandle) -> Result<(), String> {
    let Ok(_guard) = LINKED_CONTROL.try_lock() else {
        return Ok(());
    };
    // A personal session must never control the employee's Parar/Continuar clock.
    if !crate::entitlements::personal_features_allowed(&crate::paths::db_path()?) {
        return Ok(());
    }
    // Native initialization is performed by the renderer after app setup.
    let agent = app.state::<AgentState>();
    let Some(running) = agent
        .lock()
        .map_err(|error| error.to_string())?
        .as_ref()
        .map(|agent| agent.is_running)
    else {
        return Ok(());
    };
    let data = state::read()?;
    if data.total_focus_clock_stop_pending {
        crate::agent::stop_monitoring(agent)?;
        state::update(|data| {
            data.total_focus_clock_stop_pending = false;
            Ok(())
        })?;
    } else if let Some(session) = data.total_focus.filter(|session| session.linked_clock) {
        if session.is_active() && !running {
            crate::agent::set_task_context(Some(session.intention), None)?;
            crate::agent::start_monitoring(agent)?;
        } else if session.paused_at.is_some() && running {
            crate::agent::stop_monitoring(agent)?;
        }
    }
    Ok(())
}

#[tauri::command]
pub fn dismiss_total_focus_digest(ids: Vec<String>) -> Result<(), String> {
    state::update(|data| {
        if !state::holding_notifications(data) {
            data.notification_digest
                .retain(|item| !ids.contains(&item.id));
        }
        Ok(())
    })
}

pub fn restore_after_restart() -> Result<(), String> {
    maintain()?;
    if let Some(session) = policy()? {
        if system_quiet::system_notifications_supported() && session.quiet_notifications {
            let until = chrono::DateTime::parse_from_rfc3339(&session.expires_at)
                .map_err(|error| error.to_string())?;
            let remaining = ((until.with_timezone(&Utc) - Utc::now()).num_seconds() + 59) / 60;
            system_quiet::enable_total_focus(remaining.max(1))?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn context_change_releases_old_clock_and_quiet_without_changing_preferences() {
        let _store = state::TestStore::new();
        system_quiet::mock_banners(Some(1));
        start_confirmed();
        let preferences = state::read().unwrap().total_focus_preferences;
        state::update(|data| {
            data.total_focus.as_mut().unwrap().linked_clock = true;
            data.total_focus_clock_stop_pending = true;
            Ok(())
        })
        .unwrap();
        end_for_context_change().unwrap();
        let data = state::read().unwrap();
        assert!(data.total_focus.is_none());
        assert!(!data.total_focus_clock_stop_pending);
        assert!(data.total_focus_quiet.is_none());
        assert_eq!(system_quiet::mocked_banners(), Some(1));
        assert_eq!(
            data.total_focus_preferences.duration_minutes,
            preferences.duration_minutes
        );
        assert_eq!(data.total_focus_preferences.patterns, preferences.patterns);
    }
    #[test]
    fn music_pause_resume_keeps_tab_identity_and_surfaces_playback_errors() {
        let _store = state::TestStore::new();
        system_quiet::mock_banners(None);
        start_confirmed();
        state::update(|data| {
            data.total_focus.as_mut().unwrap().music_url =
                Some("https://www.youtube.com/watch?v=music".into());
            Ok(())
        })
        .unwrap();
        let original = policy().unwrap().unwrap();
        pause_with(|session| {
            assert_eq!(session.id, original.id);
            assert!(session.paused_at.is_some());
            assert!(policy()?.is_none());
            Ok(json!({"applied":false,"musicError":"Music tab closed"}))
        })
        .unwrap();
        assert_eq!(
            state::read()
                .unwrap()
                .total_focus
                .unwrap()
                .music_warning
                .as_deref(),
            Some("Music tab closed")
        );
        resume_with(
            || json!({"totalFocusAvailable":true}),
            |previous| {
                assert_eq!(previous.id, original.id);
                let current = policy()?.unwrap();
                assert_ne!(current.id, previous.id);
                assert_eq!(current.music_url, previous.music_url);
                Ok(json!({"sessionId":current.id,"applied":true,"musicPlayback":"play"}))
            },
        )
        .unwrap();
        assert!(policy().unwrap().unwrap().music_warning.is_none());
    }

    #[test]
    fn pause_keeps_remaining_time_and_resumes_with_fresh_policy_identity() {
        let _store = state::TestStore::new();
        system_quiet::mock_banners(None);
        start_confirmed();
        state::update(|data| {
            data.total_focus.as_mut().unwrap().linked_clock = true;
            Ok(())
        })
        .unwrap();
        let original = policy().unwrap().unwrap();
        pause_with(|_| Ok(json!({"applied":false}))).unwrap();
        let paused = state::read().unwrap().total_focus.unwrap();
        assert!(paused.paused_at.is_some());
        assert!((2990..=3000).contains(&paused.remaining_seconds));
        assert!(policy().unwrap().is_none());
        assert_eq!(system_quiet::mocked_banners(), None);
        state::update(|data| {
            data.total_focus.as_mut().unwrap().expires_at =
                (Utc::now() - Duration::days(2)).to_rfc3339();
            Ok(())
        })
        .unwrap();
        maintain().unwrap();
        restore_after_restart().unwrap();
        assert!(state::read()
            .unwrap()
            .total_focus
            .unwrap()
            .paused_at
            .is_some());
        assert_eq!(system_quiet::mocked_banners(), None);
        resume_with(
            || json!({"totalFocusAvailable":true}),
            |_| {
                let id = policy()?.unwrap().id;
                Ok(json!({"sessionId":id,"applied":true}))
            },
        )
        .unwrap();
        let resumed = policy().unwrap().unwrap();
        assert_ne!(resumed.id, original.id);
        assert!((resumed.remaining() - paused.remaining_seconds).abs() <= 1);
        assert!(resumed.linked_clock);
        assert_eq!(system_quiet::mocked_banners(), Some(1));
        cancel_from_extension(&original.id).unwrap();
        assert!(policy().unwrap().is_some());
        cancel_from_extension(&resumed.id).unwrap();
        assert!(state::read().unwrap().total_focus_clock_stop_pending);
    }
    #[test]
    fn failed_resume_stays_paused_without_counting_or_notification_silence() {
        let _store = state::TestStore::new();
        system_quiet::mock_banners(Some(0));
        start_confirmed();
        pause_with(|_| Ok(json!({"applied":false}))).unwrap();
        let paused = state::read().unwrap().total_focus.unwrap();
        assert!(resume_with(
            || json!({"totalFocusAvailable":false}),
            |_| panic!("unavailable browser must not activate")
        )
        .is_err());
        assert_eq!(
            resume_with(
                || json!({"totalFocusAvailable":true}),
                |_| Err("DNR unavailable".into())
            )
            .unwrap_err(),
            "DNR unavailable"
        );
        let after = state::read().unwrap().total_focus.unwrap();
        assert_eq!(after.id, paused.id);
        assert_eq!(after.remaining_seconds, paused.remaining_seconds);
        assert!(after.paused_at.is_some());
        assert!(policy().unwrap().is_none());
        assert_eq!(system_quiet::mocked_banners(), Some(0));
        end().unwrap();
        assert!(state::read().unwrap().total_focus.is_none());
    }
    #[test]
    fn linked_expiry_journals_clock_stop_and_old_sessions_default_to_unlinked() {
        let _store = state::TestStore::new();
        system_quiet::mock_banners(None);
        start_confirmed();
        state::update(|data| {
            let session = data.total_focus.as_mut().unwrap();
            session.linked_clock = true;
            session.expires_at = (Utc::now() - Duration::seconds(1)).to_rfc3339();
            Ok(())
        })
        .unwrap();
        maintain().unwrap();
        assert!(state::read().unwrap().total_focus_clock_stop_pending);
        assert!(state::read().unwrap().total_focus.is_none());
        assert_eq!(system_quiet::mocked_banners(), None);
        let old:Session=serde_json::from_value(json!({"id":"old","intention":"Study","expiresAt":(Utc::now()+Duration::minutes(5)).to_rfc3339(),"patterns":["x.com"],"exceptions":[]})).unwrap();
        assert!(!old.linked_clock);
        assert!(old.is_active());
    }
    fn start_confirmed() {
        activate_with(
            "Study ADDA".into(),
            Preferences::default(),
            || json!({"totalFocusAvailable":true}),
            || {
                let id = policy()?.unwrap().id;
                Ok(json!({"sessionId":id,"applied":true}))
            },
        )
        .unwrap();
    }

    #[test]
    fn empty_task_starts_general_focus_and_named_tasks_remain_optional() {
        let _store = state::TestStore::new();
        assert_eq!(normalize_intention("  Study ADDA  ").unwrap(), "Study ADDA");
        assert!(normalize_intention(&"a".repeat(161)).is_err());
        for intention in ["", " \t\n "] {
            activate_with(
                intention.into(),
                Preferences {
                    quiet_notifications: false,
                    ..Preferences::default()
                },
                || json!({"totalFocusAvailable":true}),
                || {
                    let session = policy()?.unwrap();
                    assert_eq!(session.intention, "General");
                    Ok(json!({"sessionId":session.id,"applied":true}))
                },
            )
            .unwrap();
            clear_session(None).unwrap();
        }
    }

    #[test]
    fn holds_reminders_and_restores_banners_on_emergency_end() {
        let _store = state::TestStore::new();
        system_quiet::mock_banners(Some(0));
        start_confirmed();
        assert_eq!(system_quiet::mocked_banners(), Some(1));
        assert!(state::hold_notification("Return to ADDA", "Saved reminder").unwrap());
        assert!(get_total_focus().unwrap()["digest"]
            .as_array()
            .unwrap()
            .is_empty());
        let id = policy().unwrap().unwrap().id;
        cancel_from_extension("stale-session").unwrap();
        assert!(policy().unwrap().is_some());
        cancel_from_extension(&id).unwrap();
        maintain().unwrap(); // busy activation may defer cleanup to maintenance
        assert_eq!(system_quiet::mocked_banners(), Some(0));
        let digest = get_total_focus().unwrap()["digest"].clone();
        assert_eq!(digest[0]["body"], "Saved reminder");
        // Summary survives reload; only acknowledged IDs are removed.
        dismiss_total_focus_digest(vec!["unknown".into()]).unwrap();
        assert_eq!(get_total_focus().unwrap()["digest"], digest);
        dismiss_total_focus_digest(vec![digest[0]["id"].as_str().unwrap().into()]).unwrap();
        assert!(get_total_focus().unwrap()["digest"]
            .as_array()
            .unwrap()
            .is_empty());
        assert!(!state::hold_notification("After focus", "Deliver normally").unwrap());
    }

    #[test]
    fn activation_failure_restores_original_and_allows_retry() {
        let _store = state::TestStore::new();
        system_quiet::mock_banners(None);
        let result = activate_with(
            "Study".into(),
            Preferences::default(),
            || json!({"totalFocusAvailable":true}),
            || Err("DNR install failed".into()),
        );
        assert_eq!(result.unwrap_err(), "DNR install failed");
        assert!(policy().unwrap().is_none());
        assert_eq!(system_quiet::mocked_banners(), None);
        assert!(state::read().unwrap().total_focus_quiet.is_none());
        start_confirmed();
        end().unwrap();
        assert_eq!(system_quiet::mocked_banners(), None);
    }

    #[test]
    fn expires_resumes_on_restart_and_preserves_external_setting_change() {
        let _store = state::TestStore::new();
        system_quiet::mock_banners(Some(0));
        start_confirmed();
        system_quiet::disable_total_focus().unwrap(); // graceful quit
        assert_eq!(system_quiet::mocked_banners(), Some(0));
        restore_after_restart().unwrap();
        assert_eq!(system_quiet::mocked_banners(), Some(1));
        state::hold_notification("Reminder", "Do this later").unwrap();
        state::update(|data| {
            data.total_focus.as_mut().unwrap().expires_at =
                (Utc::now() - Duration::seconds(1)).to_rfc3339();
            Ok(())
        })
        .unwrap();
        maintain().unwrap();
        assert!(state::read().unwrap().total_focus.is_none());
        assert_eq!(system_quiet::mocked_banners(), Some(0));
        assert_eq!(
            get_total_focus().unwrap()["digest"]
                .as_array()
                .unwrap()
                .len(),
            1
        );
        start_confirmed();
        system_quiet::mock_banners(Some(0)); // user changes Windows setting during focus
        end().unwrap();
        assert_eq!(system_quiet::mocked_banners(), Some(0));
    }

    #[test]
    fn overlapping_modes_keep_silence_and_digest_until_last_owner_ends() {
        let _store = state::TestStore::new();
        system_quiet::mock_banners(Some(0));
        system_quiet::enable(Some(30)).unwrap();
        start_confirmed();
        state::hold_notification("Reminder", "Held").unwrap();
        end().unwrap();
        assert_eq!(system_quiet::mocked_banners(), Some(1));
        assert!(get_total_focus().unwrap()["digest"]
            .as_array()
            .unwrap()
            .is_empty());
        system_quiet::disable().unwrap();
        assert_eq!(system_quiet::mocked_banners(), Some(0));
        start_confirmed();
        system_quiet::enable(Some(30)).unwrap();
        system_quiet::disable().unwrap();
        assert_eq!(system_quiet::mocked_banners(), Some(1));
        assert!(state::update(|data| Ok(state::take_released_digest(data)))
            .unwrap()
            .is_empty());
        end().unwrap();
        assert_eq!(system_quiet::mocked_banners(), Some(0));
        assert_eq!(
            get_total_focus().unwrap()["digest"]
                .as_array()
                .unwrap()
                .len(),
            1
        );
    }

    #[test]
    fn optional_silence_and_old_saved_preferences_remain_usable() {
        let old: Preferences =
            serde_json::from_value(json!({"patterns":["x.com"],"durationMinutes":25})).unwrap();
        assert!(old.quiet_notifications);
        assert!(!old.music_enabled);
        assert!(old.music_url.is_empty());
        let _store = state::TestStore::new();
        system_quiet::mock_banners(Some(0));
        activate_with(
            "Study".into(),
            Preferences {
                quiet_notifications: false,
                ..old
            },
            || json!({"totalFocusAvailable":true}),
            || {
                let id = policy()?.unwrap().id;
                Ok(json!({"sessionId":id,"applied":true}))
            },
        )
        .unwrap();
        assert_eq!(system_quiet::mocked_banners(), Some(0));
        assert!(state::hold_notification("Reminder", "Still held").unwrap());
        end().unwrap();
    }
    #[test]
    fn validates_public_sites_and_preserves_path_exceptions() {
        let preferences = validate(Preferences {
            patterns: vec!["HTTPS://YouTube.com".into(), "www.youtube.com".into()],
            exceptions: vec!["youtube.com/watch".into()],
            duration_minutes: 50,
            quiet_notifications: true,
            ..Preferences::default()
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

    #[test]
    fn music_links_preserve_playlist_parameters_and_require_web_urls() {
        let url = "https://www.youtube.com/watch?v=abc&list=PL123#t=20";
        let preferences = validate(Preferences {
            music_enabled: true,
            music_url: format!("  {url}  "),
            ..Preferences::default()
        })
        .unwrap();
        assert_eq!(preferences.music_url, url);
        for bad in [
            "",
            "youtube.com/watch?v=abc",
            "javascript:alert(1)",
            "file:///C:/music.mp3",
            "https://user:password@music.example.com",
            "http://127.0.0.1",
            "https://music.example.com:8443",
            "https://music.example.com/a b",
        ] {
            assert!(
                validate(Preferences {
                    music_enabled: true,
                    music_url: bad.into(),
                    ..Preferences::default()
                })
                .is_err(),
                "{bad}"
            );
        }
    }

    #[test]
    fn music_requires_capability_and_freezes_the_link_into_the_session() {
        let _store = state::TestStore::new();
        let preferences = Preferences {
            music_enabled: true,
            music_url: "https://music.example.com/playlist?list=123".into(),
            quiet_notifications: false,
            ..Preferences::default()
        };
        let error = activate_with(
            "Study".into(),
            preferences.clone(),
            || json!({"totalFocusAvailable":true}),
            || panic!("Old extension must not receive music activation"),
        )
        .unwrap_err();
        assert!(error.contains("Update Browser Controls"));
        assert!(policy().unwrap().is_none());
        activate_with("Study".into(), preferences.clone(), || json!({"totalFocusAvailable":true,"musicAvailable":true}), || {
            let session = policy()?.unwrap();
            assert_eq!(session.music_url.as_deref(), Some(preferences.music_url.as_str()));
            Ok(json!({"sessionId":session.id,"applied":true,"musicError":"Keep a browser window open to open music in the background."}))
        }).unwrap();
        assert!(policy().unwrap().unwrap().music_warning.is_some());
        assert_eq!(state::read().unwrap().total_focus_preferences.music_url, "");
        clear_session(None).unwrap();
        activate_with(
            "".into(),
            preferences.clone(),
            || json!({"totalFocusAvailable":true,"musicAvailable":true}),
            || {
                let session = policy()?.unwrap();
                Ok(json!({"sessionId":session.id,"applied":true}))
            },
        )
        .unwrap();
        assert_eq!(
            policy().unwrap().unwrap().music_warning.as_deref(),
            Some("The browser did not confirm opening the music link.")
        );
        clear_session(None).unwrap();
        activate_with(
            "".into(),
            preferences,
            || json!({"totalFocusAvailable":true,"musicAvailable":true}),
            || {
                let session = policy()?.unwrap();
                Ok(json!({"sessionId":session.id,"applied":true,"musicOpened":true}))
            },
        )
        .unwrap();
        assert!(policy().unwrap().unwrap().music_warning.is_none());
    }
}
