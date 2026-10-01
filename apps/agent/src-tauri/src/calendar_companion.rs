//! Optional, local-first calendar context and short event recaps. Google's
//! OAuth code and tokens pass through the paid token broker; event context and
//! recorded activity stay on-device and are never sent to that broker.

use chrono::{DateTime, Duration as ChronoDuration, Local, NaiveDateTime, Utc};
use reqwest::blocking::Client;
use rusqlite::{params, Connection};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
use std::sync::{Mutex, OnceLock};
use std::time::Duration;
use tiny_http::{Response, Server};
use url::Url;

const STATE_KEY_PREFIX: &str = "calendar_companion_state_v2";
// Capture/analysis can finish shortly after a calendar event ends. Give those
// final observations time to reach SQLite before deciding whether to publish.
const REPORT_GRACE_SECONDS: i64 = 90;
const REPORT_PENDING_TTL_SECONDS: i64 = 300;
const REPORT_MIN_OBSERVED_SECONDS: i64 = 60;
const GOOGLE_SCOPE: &str = "https://www.googleapis.com/auth/calendar.events.owned https://www.googleapis.com/auth/calendar.calendarlist.readonly";
const MICROSOFT_SCOPE: &str = "offline_access Calendars.ReadWrite";
static STATE_LOCK: Mutex<()> = Mutex::new(());
static TOKEN_LOCK: Mutex<()> = Mutex::new(());
static STATUS: OnceLock<Mutex<RuntimeStatus>> = OnceLock::new();
type CalendarIdCache = BTreeMap<String, (DateTime<Utc>, CalendarList)>;
static CALENDAR_CACHE: OnceLock<Mutex<CalendarIdCache>> = OnceLock::new();

#[derive(Clone, Debug)]
struct CalendarList {
    ids: Vec<String>,
    account_id: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct OAuthTokens {
    access_token: String,
    refresh_token: Option<String>,
    expires_at: DateTime<Utc>,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
struct Persisted {
    auto_publish: bool,
    active: Option<ObservedEvent>,
    pending: Vec<ObservedEvent>,
    completed: Vec<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ObservedEvent {
    provider: String,
    #[serde(default)]
    calendar_id: String,
    id: String,
    start_at: DateTime<Utc>,
    end_at: DateTime<Utc>,
    first_seen_at: DateTime<Utc>,
    last_seen_at: DateTime<Utc>,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CalendarEvent {
    provider: String,
    calendar_id: String,
    account_id: Option<String>,
    id: String,
    title: String,
    description: String,
    start_at: DateTime<Utc>,
    end_at: DateTime<Utc>,
    organizer: bool,
    attendee_count: usize,
    web_link: Option<String>,
}

#[derive(Clone, Debug, Default, Serialize)]
#[serde(rename_all = "camelCase")]
struct RuntimeStatus {
    owner_user_id: Option<String>,
    current: Option<CalendarEvent>,
    overlap_count: usize,
    last_error: Option<String>,
    last_report: Option<String>,
    checked_at: Option<DateTime<Utc>>,
}

fn conn() -> Result<Connection, String> {
    let conn = Connection::open(crate::paths::db_path()?).map_err(|e| e.to_string())?;
    conn.execute(
        "CREATE TABLE IF NOT EXISTS config (key TEXT PRIMARY KEY, value TEXT)",
        [],
    )
    .map_err(|e| e.to_string())?;
    Ok(conn)
}

fn current_owner_id() -> Result<String, String> {
    crate::sync::get_user_session_from_conn(&conn()?)
        .map(|session| session.user_id)
        .filter(|id| !id.trim().is_empty())
        .ok_or("Sign in to FlowSight before connecting a calendar.".into())
}

pub(crate) fn session_owner() -> Option<String> {
    current_owner_id().ok()
}

fn has_paid_calendar_access(
    entitlements: &crate::entitlements::Entitlements,
    owner_user_id: &str,
) -> bool {
    // These are Supabase Cloud entitlements, not one-time local FSI- licenses.
    entitlements.owner_user_id.as_deref() == Some(owner_user_id)
        && entitlements.status == "active"
        && entitlements.can_integrations
        && matches!(
            entitlements.plan.as_deref(),
            Some("pro" | "individual" | "individual_pro")
        )
}

fn require_paid_for(owner_user_id: &str) -> Result<(), String> {
    if current_owner_id()? != owner_user_id {
        return Err(
            "FlowSight account changed. Reconnect the calendar from the current account.".into(),
        );
    }
    let entitlements = crate::entitlements::load_entitlements(&conn()?);
    if has_paid_calendar_access(&entitlements, owner_user_id) {
        Ok(())
    } else {
        Err("Calendar companion requires an eligible active FlowSight Cloud plan with integrations. A one-time local license does not unlock it. Activate a Cloud plan in Settings.".into())
    }
}

fn owner_suffix(owner_user_id: &str) -> String {
    let digest = Sha256::digest(owner_user_id.as_bytes());
    digest[..16]
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

fn state_key(owner_user_id: &str) -> String {
    format!("{STATE_KEY_PREFIX}_{}", owner_suffix(owner_user_id))
}

fn token_key(provider: &str, owner_user_id: &str) -> Result<String, String> {
    match provider {
        "google" | "microsoft" => Ok(format!(
            "calendar_oauth_{provider}_v2_{}",
            owner_suffix(owner_user_id)
        )),
        _ => Err("Choose Google Calendar or Microsoft Calendar.".into()),
    }
}

fn client_id(provider: &str) -> Result<String, String> {
    let (runtime, compiled) = match provider {
        "google" => (
            "TAURI_GOOGLE_CALENDAR_CLIENT_ID",
            option_env!("TAURI_GOOGLE_CALENDAR_CLIENT_ID"),
        ),
        "microsoft" => (
            "TAURI_MICROSOFT_CALENDAR_CLIENT_ID",
            option_env!("TAURI_MICROSOFT_CALENDAR_CLIENT_ID"),
        ),
        _ => return Err("Unsupported calendar provider.".into()),
    };
    std::env::var(runtime)
        .ok()
        .filter(|s| !s.trim().is_empty())
        .or_else(|| {
            compiled
                .map(str::to_string)
                .filter(|s| !s.trim().is_empty())
        })
        .ok_or_else(|| format!("Calendar OAuth is not configured in this build ({runtime})."))
}

fn http() -> Result<Client, String> {
    Client::builder()
        .timeout(Duration::from_secs(30))
        .build()
        .map_err(|e| e.to_string())
}

// Google currently rejects this desktop client's PKCE token exchange without
// client_secret. Keep that credential only in the paid Supabase Edge Function;
// the local OAuth callback and encrypted token storage remain on this device.
fn google_token_service_for(owner_user_id: &str, body: Value) -> Result<Value, String> {
    require_paid_for(owner_user_id)?;
    let db_path = crate::paths::db_path()?;
    crate::sync::refresh_session_if_expiring(&db_path);
    let session = crate::sync::get_user_session_from_conn(&conn()?)
        .ok_or("Sign in to FlowSight before connecting Google Calendar.")?;
    if session.user_id != owner_user_id {
        return Err("FlowSight account changed. Start this calendar connection again.".into());
    }
    require_paid_for(owner_user_id)?;
    let response = http()?
        .post(format!(
            "{}/functions/v1/calendar-token",
            crate::sync_env::supabase_url()
        ))
        .header("apikey", crate::sync_env::supabase_anon_key())
        .bearer_auth(&session.access_token)
        .json(&body)
        .send()
        .map_err(|_| "Could not reach the Google Calendar connection service.")?;
    let status = response.status();
    let payload: Value = response
        .json()
        .map_err(|_| "The Google Calendar connection service returned an invalid response.")?;
    if !status.is_success() {
        let code = payload["code"].as_str().unwrap_or("calendar_service_error");
        return Err(format!(
            "Google Calendar connection service failed (HTTP {status}, {code})."
        ));
    }
    Ok(payload)
}

fn load_tokens_for(owner_user_id: &str, provider: &str) -> Result<Option<OAuthTokens>, String> {
    crate::secure_config::load_secret(&conn()?, &token_key(provider, owner_user_id)?)?
        .map(|raw| {
            serde_json::from_str(&raw)
                .map_err(|_| "Calendar credentials are invalid. Reconnect the calendar.".into())
        })
        .transpose()
}

fn save_tokens_for(
    owner_user_id: &str,
    provider: &str,
    tokens: &OAuthTokens,
) -> Result<(), String> {
    crate::secure_config::save_secret(
        &conn()?,
        &token_key(provider, owner_user_id)?,
        &serde_json::to_string(tokens).map_err(|e| e.to_string())?,
    )
}

fn token_endpoint(provider: &str) -> &'static str {
    if provider == "google" {
        "https://oauth2.googleapis.com/token"
    } else {
        "https://login.microsoftonline.com/common/oauth2/v2.0/token"
    }
}

fn parse_token_response(
    body: Value,
    previous_refresh: Option<String>,
) -> Result<OAuthTokens, String> {
    let access_token = body["access_token"]
        .as_str()
        .filter(|s| !s.is_empty())
        .ok_or("Calendar authorization did not return an access token.")?
        .to_string();
    let refresh_token = body["refresh_token"]
        .as_str()
        .map(str::to_string)
        .or(previous_refresh);
    let expires_in = body["expires_in"]
        .as_i64()
        .unwrap_or(3600)
        .clamp(60, 86_400);
    Ok(OAuthTokens {
        access_token,
        refresh_token,
        expires_at: Utc::now() + ChronoDuration::seconds(expires_in),
    })
}

fn oauth_rejection_hint(body: &Value) -> &'static str {
    let description = body["error_description"]
        .as_str()
        .unwrap_or_default()
        .to_ascii_lowercase();
    if description.contains("client_secret") {
        "client_secret_required"
    } else if description.contains("redirect_uri") {
        "redirect_uri_rejected"
    } else if description.contains("code_verifier") || description.contains("pkce") {
        "pkce_rejected"
    } else if description.contains("authorization code") || description.contains("invalid code") {
        "authorization_code_rejected"
    } else {
        "no_detail"
    }
}

fn access_token_for(owner_user_id: &str, provider: &str) -> Result<String, String> {
    token_key(provider, owner_user_id)?;
    require_paid_for(owner_user_id)?;
    let _guard = TOKEN_LOCK.lock().map_err(|e| e.to_string())?;
    let mut tokens = load_tokens_for(owner_user_id, provider)?
        .ok_or_else(|| format!("Connect {provider} Calendar in Settings first."))?;
    if tokens.expires_at > Utc::now() + ChronoDuration::seconds(90) {
        require_paid_for(owner_user_id)?;
        return Ok(tokens.access_token);
    }
    let refresh = tokens
        .refresh_token
        .clone()
        .ok_or("Calendar access expired. Reconnect in Settings.")?;
    let body = if provider == "google" {
        google_token_service_for(
            owner_user_id,
            json!({
                "action": "refresh",
                "refresh_token": refresh,
            }),
        )?
    } else {
        let form = [
            ("client_id", client_id(provider)?),
            ("grant_type", "refresh_token".to_string()),
            ("refresh_token", refresh.clone()),
            ("scope", MICROSOFT_SCOPE.to_string()),
        ];
        let response = http()?
            .post(token_endpoint(provider))
            .form(&form)
            .send()
            .map_err(|e| e.to_string())?;
        if !response.status().is_success() {
            return Err(format!(
                "{provider} Calendar authorization expired or was revoked. Reconnect in Settings."
            ));
        }
        response
            .json()
            .map_err(|_| "Invalid calendar token response.")?
    };
    tokens = parse_token_response(body, Some(refresh))?;
    require_paid_for(owner_user_id)?;
    save_tokens_for(owner_user_id, provider, &tokens)?;
    Ok(tokens.access_token)
}

pub fn access_token(provider: &str) -> Result<String, String> {
    access_token_for(&current_owner_id()?, provider)
}

/// Select the connected provider's default calendar for a reviewed session.
/// Tokens and calendar identity stay bound to the signed-in FlowSight owner.
pub(crate) fn session_target(
    preferred: Option<&str>,
) -> Result<Option<crate::local_agent::session_calendar::CalendarTarget>, String> {
    let Some(owner) = current_owner_id().ok() else {
        return Ok(None);
    };
    let google = load_tokens_for(&owner, "google")?.is_some();
    let microsoft = load_tokens_for(&owner, "microsoft")?.is_some();
    let provider = match preferred {
        Some("google") if google => Some("google"),
        Some("microsoft") if microsoft => Some("microsoft"),
        Some("google" | "microsoft") => {
            return Err(
                "Reconnect your selected calendar in Settings before planning a session.".into(),
            )
        }
        _ if google => Some("google"),
        _ if microsoft => Some("microsoft"),
        _ => None,
    };
    let Some(provider) = provider else {
        return Ok(None);
    };
    let token = access_token_for(&owner, provider)?;
    let client = http()?;
    let calendar_id = if provider == "google" {
        calendar_ids(&owner, provider, &token, &client, Utc::now())?
            .account_id
            .ok_or("Google Calendar did not identify an owned primary calendar.")?
    } else {
        let response = client
            .get("https://graph.microsoft.com/v1.0/me/calendar?$select=id,canEdit")
            .bearer_auth(token)
            .send()
            .map_err(|_| "Microsoft Calendar could not identify your default calendar.")?;
        if !response.status().is_success() {
            return Err(format!(
                "Microsoft Calendar could not identify your default calendar (HTTP {}).",
                response.status()
            ));
        }
        let value: Value = response
            .json()
            .map_err(|_| "Invalid default calendar response.")?;
        if value["canEdit"] == false {
            return Err("Your default Microsoft calendar is not editable.".into());
        }
        value["id"]
            .as_str()
            .filter(|id| !id.is_empty())
            .ok_or("Microsoft Calendar did not identify your default calendar.")?
            .to_string()
    };
    Ok(Some(crate::local_agent::session_calendar::CalendarTarget {
        owner_user_id: owner,
        provider: provider.into(),
        calendar_id,
    }))
}

pub(crate) fn session_token(
    target: &crate::local_agent::session_calendar::CalendarTarget,
) -> Result<String, String> {
    access_token_for(&target.owner_user_id, &target.provider)
}

#[tauri::command]
pub fn start_calendar_oauth(provider: String) -> Result<String, String> {
    let owner_user_id = current_owner_id()?;
    require_paid_for(&owner_user_id)?;
    let id = client_id(&provider)?;
    status_update_for(Some(&owner_user_id), |status| status.last_error = None);
    let server = Server::http("127.0.0.1:0")
        .map_err(|e| format!("Could not start local OAuth callback: {e}"))?;
    let port: u16 = server
        .server_addr()
        .to_string()
        .rsplit(':')
        .next()
        .ok_or("Local OAuth callback has no port.")?
        .parse()
        .map_err(|_| "Invalid local OAuth callback port.")?;
    // Entra ignores dynamic ports only for `localhost`, not 127.0.0.1.
    let host = if provider == "microsoft" {
        "localhost"
    } else {
        "127.0.0.1"
    };
    let redirect = format!("http://{host}:{port}/callback");
    let (challenge, verifier) = oauth2::PkceCodeChallenge::new_random_sha256();
    let state = uuid::Uuid::new_v4().to_string();
    let mut url = Url::parse(if provider == "google" {
        "https://accounts.google.com/o/oauth2/v2/auth"
    } else {
        "https://login.microsoftonline.com/common/oauth2/v2.0/authorize"
    })
    .map_err(|e| e.to_string())?;
    {
        let mut q = url.query_pairs_mut();
        q.append_pair("client_id", &id)
            .append_pair("redirect_uri", &redirect)
            .append_pair("response_type", "code")
            .append_pair(
                "scope",
                if provider == "google" {
                    GOOGLE_SCOPE
                } else {
                    MICROSOFT_SCOPE
                },
            )
            .append_pair("state", &state)
            .append_pair("code_challenge", challenge.as_str())
            .append_pair("code_challenge_method", "S256");
        if provider == "google" {
            q.append_pair("access_type", "offline")
                .append_pair("prompt", "consent select_account");
        }
    }
    open::that(url.as_str()).map_err(|e| format!("Could not open the browser: {e}"))?;
    std::thread::spawn(move || {
        let result = receive_oauth_callback(
            server,
            &owner_user_id,
            &provider,
            &id,
            &redirect,
            &state,
            verifier.secret(),
        );
        if let Err(error) = result {
            log::warn!("Calendar OAuth failed for {provider}: {error}");
            status_update_for(Some(&owner_user_id), |status| {
                status.last_error = Some(error)
            });
        }
    });
    Ok("Browser opened. Complete calendar authorization there.".into())
}

fn receive_oauth_callback(
    server: Server,
    owner_user_id: &str,
    provider: &str,
    id: &str,
    redirect: &str,
    expected_state: &str,
    verifier: &str,
) -> Result<(), String> {
    let deadline = std::time::Instant::now() + Duration::from_secs(180);
    loop {
        let remaining = deadline.saturating_duration_since(std::time::Instant::now());
        if remaining.is_zero() {
            return Err("Calendar authorization timed out.".into());
        }
        let Some(request) = server.recv_timeout(remaining).map_err(|e| e.to_string())? else {
            continue;
        };
        let url = Url::parse(&format!("http://127.0.0.1{}", request.url()))
            .map_err(|_| "Invalid OAuth callback URL.")?;
        if url.path() != "/callback" {
            let _ = request.respond(Response::from_string("Not found").with_status_code(404));
            continue;
        }
        let values: BTreeMap<String, String> = url
            .query_pairs()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect();
        if values.get("state").map(String::as_str) != Some(expected_state) {
            let _ = request.respond(
                Response::from_string("FlowSight rejected this authorization response.")
                    .with_status_code(400),
            );
            return Err("Calendar OAuth state mismatch.".into());
        }
        if let Some(error) = values.get("error") {
            let _ = request.respond(Response::from_string(
                "Calendar authorization was cancelled.",
            ));
            return Err(format!("Calendar authorization was cancelled: {error}"));
        }
        let code = values
            .get("code")
            .ok_or("Calendar authorization did not return a code.")?;
        if let Err(error) = require_paid_for(owner_user_id) {
            let _ = request.respond(Response::from_string(
                "FlowSight account changed. Start this calendar connection again.",
            ));
            return Err(error);
        }
        let token_body = if provider == "google" {
            google_token_service_for(
                owner_user_id,
                json!({
                    "action": "exchange",
                    "code": code,
                    "code_verifier": verifier,
                    "redirect_uri": redirect,
                }),
            )
        } else {
            let form = [
                ("client_id", id),
                ("grant_type", "authorization_code"),
                ("code", code.as_str()),
                ("redirect_uri", redirect),
                ("code_verifier", verifier),
            ];
            let response = http()?
                .post(token_endpoint(provider))
                .form(&form)
                .send()
                .map_err(|e| e.to_string())?;
            if !response.status().is_success() {
                let status = response.status();
                let body = response.json::<Value>().unwrap_or(Value::Null);
                let reason = body["error"].as_str().unwrap_or("unknown_error");
                let hint = oauth_rejection_hint(&body);
                Err(format!(
                    "{provider} did not accept calendar authorization (HTTP {status}, {reason}, {hint})."
                ))
            } else {
                response
                    .json::<Value>()
                    .map_err(|_| "Invalid calendar token response.".into())
            }
        };
        let tokens = match token_body.and_then(|body| parse_token_response(body, None)) {
            Ok(tokens) => tokens,
            Err(error) => {
                let _ = request.respond(Response::from_string(
                    "Calendar connection failed. Return to FlowSight Settings.",
                ));
                return Err(error);
            }
        };
        let _state_guard = STATE_LOCK.lock().map_err(|e| e.to_string())?;
        let _token_guard = TOKEN_LOCK.lock().map_err(|e| e.to_string())?;
        if let Err(error) = require_paid_for(owner_user_id) {
            let _ = request.respond(Response::from_string(
                "FlowSight account changed. Start this calendar connection again.",
            ));
            return Err(error);
        }
        let mut state = load_state_for(owner_user_id)?;
        if state
            .active
            .as_ref()
            .is_some_and(|event| event.provider == provider)
        {
            state.active = None;
        }
        state.pending.retain(|event| event.provider != provider);
        save_state_for(owner_user_id, &state)?;
        save_tokens_for(owner_user_id, provider, &tokens)?;
        if let Some(cache) = CALENDAR_CACHE.get() {
            if let Ok(mut cache) = cache.lock() {
                cache.remove(&token_key(provider, owner_user_id)?);
            }
        }
        let _ = request.respond(Response::from_string(
            "Calendar connected. You can return to FlowSight.",
        ));
        status_update_for(Some(owner_user_id), |status| {
            status.last_error = None;
            status.current = None;
        });
        return Ok(());
    }
}

fn load_state_for(owner_user_id: &str) -> Result<Persisted, String> {
    crate::secure_config::load_secret(&conn()?, &state_key(owner_user_id))?
        .map(|raw| serde_json::from_str(&raw).map_err(|_| "Calendar settings are invalid.".into()))
        .transpose()
        .map(|state| state.unwrap_or_default())
}

fn save_state_for(owner_user_id: &str, state: &Persisted) -> Result<(), String> {
    crate::secure_config::save_secret(
        &conn()?,
        &state_key(owner_user_id),
        &serde_json::to_string(state).map_err(|e| e.to_string())?,
    )
}

fn status_update_for(owner_user_id: Option<&str>, change: impl FnOnce(&mut RuntimeStatus)) {
    let mutex = STATUS.get_or_init(|| Mutex::new(RuntimeStatus::default()));
    if let Ok(mut status) = mutex.lock() {
        if status.owner_user_id.as_deref() != owner_user_id {
            *status = RuntimeStatus {
                owner_user_id: owner_user_id.map(str::to_string),
                ..RuntimeStatus::default()
            };
        }
        change(&mut status);
    }
}

pub fn on_cloud_logout() {
    if let Ok(owner_user_id) = current_owner_id() {
        if let Ok(_guard) = STATE_LOCK.lock() {
            if let Ok(mut state) = load_state_for(&owner_user_id) {
                state.auto_publish = false;
                state.active = None;
                state.pending.clear();
                if let Err(error) = save_state_for(&owner_user_id, &state) {
                    log::warn!("Could not suspend calendar publishing on logout: {error}");
                }
            }
        }
        if let Some(cache) = CALENDAR_CACHE.get() {
            if let Ok(mut cache) = cache.lock() {
                for provider in ["google", "microsoft"] {
                    if let Ok(key) = token_key(provider, &owner_user_id) {
                        cache.remove(&key);
                    }
                }
            }
        }
    }
    status_update_for(None, |_| {});
}

#[tauri::command]
pub fn get_calendar_companion_status() -> Result<Value, String> {
    let owner_user_id = current_owner_id().ok();
    let state = owner_user_id
        .as_deref()
        .map(load_state_for)
        .transpose()?
        .unwrap_or_default();
    let licensed = owner_user_id
        .as_deref()
        .is_some_and(|owner| require_paid_for(owner).is_ok());
    let status = STATUS
        .get_or_init(|| Mutex::new(RuntimeStatus::default()))
        .lock()
        .map_err(|e| e.to_string())?;
    let same_owner = status.owner_user_id.as_deref() == owner_user_id.as_deref();
    Ok(json!({
        "googleConnected": owner_user_id.as_deref().map(|owner| load_tokens_for(owner, "google")).transpose()?.flatten().is_some(),
        "microsoftConnected": owner_user_id.as_deref().map(|owner| load_tokens_for(owner, "microsoft")).transpose()?.flatten().is_some(),
        "googleAvailable": client_id("google").is_ok(),
        "microsoftAvailable": client_id("microsoft").is_ok(),
        "autoPublish": licensed && state.auto_publish,
        "current": if licensed && same_owner { status.current.as_ref() } else { None },
        "overlapCount": if same_owner { status.overlap_count } else { 0 },
        "lastError": if same_owner { status.last_error.as_ref() } else { None },
        "lastReport": if same_owner { status.last_report.as_ref() } else { None },
        "checkedAt": if same_owner { status.checked_at.as_ref() } else { None },
    }))
}

#[tauri::command]
pub fn set_calendar_auto_publish(enabled: bool) -> Result<(), String> {
    let owner_user_id = current_owner_id()?;
    if enabled {
        require_paid_for(&owner_user_id)?;
    }
    let _guard = STATE_LOCK.lock().map_err(|e| e.to_string())?;
    let mut state = load_state_for(&owner_user_id)?;
    state.auto_publish = enabled;
    if !enabled {
        state.pending.clear();
    }
    save_state_for(&owner_user_id, &state)
}

#[tauri::command]
pub fn disconnect_calendar(provider: String) -> Result<(), String> {
    let owner_user_id = current_owner_id()?;
    let _guard = STATE_LOCK.lock().map_err(|e| e.to_string())?;
    let _token_guard = TOKEN_LOCK.lock().map_err(|e| e.to_string())?;
    crate::secure_config::delete_secret(&conn()?, &token_key(&provider, &owner_user_id)?)?;
    let mut state = load_state_for(&owner_user_id)?;
    if state
        .active
        .as_ref()
        .is_some_and(|event| event.provider == provider)
    {
        state.active = None;
    }
    state.pending.retain(|event| event.provider != provider);
    save_state_for(&owner_user_id, &state)?;
    if let Some(cache) = CALENDAR_CACHE.get() {
        if let Ok(mut cache) = cache.lock() {
            cache.remove(&token_key(&provider, &owner_user_id)?);
        }
    }
    status_update_for(Some(&owner_user_id), |status| {
        if status
            .current
            .as_ref()
            .is_some_and(|event| event.provider == provider)
        {
            status.current = None;
        }
    });
    Ok(())
}

#[tauri::command]
pub fn open_current_calendar_event() -> Result<(), String> {
    let owner_user_id = current_owner_id()?;
    require_paid_for(&owner_user_id)?;
    let status = STATUS
        .get_or_init(|| Mutex::new(RuntimeStatus::default()))
        .lock()
        .map_err(|e| e.to_string())?;
    if status.owner_user_id.as_deref() != Some(owner_user_id.as_str()) {
        return Err("No current calendar event for this FlowSight account.".into());
    }
    let event = status.current.clone().ok_or("No current calendar event.")?;
    drop(status);
    let link = event
        .web_link
        .as_deref()
        .ok_or("This event has no browser link.")?;
    let mut url = Url::parse(link).map_err(|_| "Calendar returned an invalid event link.")?;
    let host = url
        .host_str()
        .ok_or("Calendar returned an invalid event link.")?;
    let trusted = if event.provider == "google" {
        host == "google.com" || host.ends_with(".google.com")
    } else {
        ["outlook.com", "office.com", "office365.com"]
            .iter()
            .any(|domain| host == *domain || host.ends_with(&format!(".{domain}")))
    };
    if url.scheme() != "https" || !trusted {
        return Err("Calendar returned an untrusted event link.".into());
    }
    if event.provider == "google" {
        let account_id = event
            .account_id
            .as_deref()
            .ok_or("Could not identify the connected Google account for this event.")?;
        with_google_authuser(&mut url, account_id);
    }
    open::that(url.as_str()).map_err(|e| format!("Could not open the calendar event: {e}"))
}

fn with_google_authuser(url: &mut Url, account_id: &str) {
    let existing = url
        .query_pairs()
        .filter(|(key, _)| key != "authuser")
        .map(|(key, value)| (key.into_owned(), value.into_owned()))
        .collect::<Vec<_>>();
    url.set_query(None);
    let mut query = url.query_pairs_mut();
    for (key, value) in existing {
        query.append_pair(&key, &value);
    }
    query.append_pair("authuser", account_id);
}

fn graph_time(value: &Value) -> Result<DateTime<Utc>, String> {
    let raw = value["dateTime"]
        .as_str()
        .ok_or("Microsoft returned an event without a time.")?;
    DateTime::parse_from_rfc3339(raw)
        .or_else(|_| DateTime::parse_from_rfc3339(&format!("{raw}Z")))
        .map(|date| date.with_timezone(&Utc))
        .map_err(|_| "Microsoft returned an invalid event time.".into())
}

fn plain_html(value: &str) -> String {
    let mut out = String::new();
    let mut in_tag = false;
    for ch in value.chars() {
        match ch {
            '<' => in_tag = true,
            '>' => {
                in_tag = false;
                out.push(' ');
            }
            _ if !in_tag => out.push(ch),
            _ => {}
        }
    }
    out.replace("&nbsp;", " ")
        .replace("&amp;", "&")
        .split_whitespace()
        .take(80)
        .collect::<Vec<_>>()
        .join(" ")
}

fn parse_event(provider: &str, calendar_id: &str, value: &Value) -> Option<CalendarEvent> {
    let id = value["id"].as_str()?.to_string();
    let (title, description, start_at, end_at, organizer, attendee_count, web_link) = if provider
        == "google"
    {
        if value["status"] == "cancelled"
            || value["transparency"] == "transparent"
            || value["start"]["dateTime"].is_null()
        {
            return None;
        }
        (
            value["summary"]
                .as_str()
                .unwrap_or("Untitled event")
                .to_string(),
            value["description"]
                .as_str()
                .unwrap_or("")
                .chars()
                .take(800)
                .collect(),
            DateTime::parse_from_rfc3339(value["start"]["dateTime"].as_str()?)
                .ok()?
                .with_timezone(&Utc),
            DateTime::parse_from_rfc3339(value["end"]["dateTime"].as_str()?)
                .ok()?
                .with_timezone(&Utc),
            value["organizer"]["self"] == true,
            value["attendees"].as_array().map(Vec::len).unwrap_or(0),
            value["htmlLink"].as_str().map(str::to_string),
        )
    } else {
        if value["isCancelled"] == true || value["isAllDay"] == true || value["showAs"] == "free" {
            return None;
        }
        (
            value["subject"]
                .as_str()
                .unwrap_or("Untitled event")
                .to_string(),
            plain_html(value["body"]["content"].as_str().unwrap_or("")),
            graph_time(&value["start"]).ok()?,
            graph_time(&value["end"]).ok()?,
            value["isOrganizer"] == true,
            value["attendees"].as_array().map(Vec::len).unwrap_or(0),
            value["webLink"].as_str().map(str::to_string),
        )
    };
    (start_at < end_at).then_some(CalendarEvent {
        provider: provider.into(),
        calendar_id: calendar_id.into(),
        account_id: None,
        id,
        title,
        description,
        start_at,
        end_at,
        organizer,
        attendee_count,
        web_link,
    })
}

fn calendar_ids(
    owner_user_id: &str,
    provider: &str,
    token: &str,
    client: &Client,
    now: DateTime<Utc>,
) -> Result<CalendarList, String> {
    let cache = CALENDAR_CACHE.get_or_init(|| Mutex::new(BTreeMap::new()));
    let cache_key = token_key(provider, owner_user_id)?;
    if let Some((checked, list)) = cache.lock().map_err(|e| e.to_string())?.get(&cache_key) {
        if now - *checked < ChronoDuration::minutes(10) {
            return Ok(list.clone());
        }
    }
    let mut ids = Vec::new();
    let mut account_id = None;
    let mut next = if provider == "google" {
        "https://www.googleapis.com/calendar/v3/users/me/calendarList?minAccessRole=owner&maxResults=100".to_string()
    } else {
        "https://graph.microsoft.com/v1.0/me/calendars?$select=id,canEdit&$top=100".to_string()
    };
    for _ in 0..4 {
        let response = client
            .get(&next)
            .bearer_auth(token)
            .send()
            .map_err(|e| e.to_string())?;
        if !response.status().is_success() {
            return Err(format!(
                "{provider} could not list calendars (HTTP {}).",
                response.status()
            ));
        }
        let body: Value = response
            .json()
            .map_err(|_| "Invalid calendar list response.")?;
        let items = if provider == "google" {
            &body["items"]
        } else {
            &body["value"]
        };
        for item in items
            .as_array()
            .ok_or("Calendar provider did not return calendars.")?
        {
            if provider == "google" && item["accessRole"] != "owner" {
                continue;
            }
            if provider == "microsoft" && item["canEdit"] == false {
                continue;
            }
            if let Some(id) = item["id"].as_str() {
                if provider == "google" && item["primary"] == true {
                    account_id = Some(id.to_string());
                }
                ids.push(id.to_string());
            }
        }
        if ids.len() > 20 {
            return Err("More than 20 editable calendars were found. Choose fewer calendars before enabling automatic reports.".into());
        }
        let page = if provider == "google" {
            body["nextPageToken"].as_str().map(|token| {
                let mut url =
                    Url::parse("https://www.googleapis.com/calendar/v3/users/me/calendarList")
                        .expect("constant URL");
                url.query_pairs_mut()
                    .append_pair("minAccessRole", "owner")
                    .append_pair("maxResults", "100")
                    .append_pair("pageToken", token);
                url.to_string()
            })
        } else {
            body["@odata.nextLink"].as_str().map(str::to_string)
        };
        let Some(page) = page else {
            let list = CalendarList { ids, account_id };
            cache
                .lock()
                .map_err(|e| e.to_string())?
                .insert(cache_key, (now, list.clone()));
            return Ok(list);
        };
        let parsed = Url::parse(&page).map_err(|_| "Invalid calendar list page URL.")?;
        let trusted = if provider == "google" {
            "www.googleapis.com"
        } else {
            "graph.microsoft.com"
        };
        if parsed.scheme() != "https" || parsed.host_str() != Some(trusted) {
            return Err("Calendar returned an untrusted list page URL.".into());
        }
        next = page;
    }
    Err("Calendar list has too many pages to process safely.".into())
}

fn events_url(
    provider: &str,
    calendar_id: &str,
    start: &DateTime<Utc>,
    end: &DateTime<Utc>,
) -> Result<Url, String> {
    if provider == "google" {
        // `path_segments_mut().push()` preserves a trailing empty segment. A
        // base ending in `/` would produce `calendars//{id}/events` (404).
        let mut url = Url::parse("https://www.googleapis.com/calendar/v3/calendars")
            .map_err(|e| e.to_string())?;
        url.path_segments_mut()
            .map_err(|_| "Invalid calendar URL.")?
            .push(calendar_id)
            .push("events");
        url.query_pairs_mut()
            .append_pair("timeMin", &start.to_rfc3339())
            .append_pair("timeMax", &end.to_rfc3339())
            .append_pair("singleEvents", "true")
            .append_pair("orderBy", "startTime")
            .append_pair("maxResults", "250");
        Ok(url)
    } else {
        let mut url = Url::parse("https://graph.microsoft.com/v1.0/me/calendars")
            .map_err(|e| e.to_string())?;
        url.path_segments_mut()
            .map_err(|_| "Invalid calendar URL.")?
            .push(calendar_id)
            .push("calendarView");
        url.query_pairs_mut()
            .append_pair("startDateTime", &start.to_rfc3339())
            .append_pair("endDateTime", &end.to_rfc3339())
            .append_pair("$select", "id,subject,body,start,end,isOrganizer,attendees,isCancelled,isAllDay,showAs,webLink")
            .append_pair("$top", "250");
        Ok(url)
    }
}

fn events_now(
    owner_user_id: &str,
    provider: &str,
    now: DateTime<Utc>,
) -> Result<Vec<CalendarEvent>, String> {
    let token = access_token_for(owner_user_id, provider)?;
    let client = http()?;
    let start = now - ChronoDuration::hours(24);
    let end = now + ChronoDuration::hours(2);
    let mut result = Vec::new();
    let calendars = calendar_ids(owner_user_id, provider, &token, &client, now)?;
    for calendar_id in calendars.ids {
        let mut url = events_url(provider, &calendar_id, &start, &end)?;
        let first_page = url.clone();
        let mut finished = false;
        for _ in 0..8 {
            let response = client
                .get(url.clone())
                .bearer_auth(&token)
                .header("Prefer", "outlook.timezone=\"UTC\"")
                .send()
                .map_err(|e| e.to_string())?;
            if !response.status().is_success() {
                return Err(format!(
                    "{provider} Calendar returned HTTP {}.",
                    response.status()
                ));
            }
            let body: Value = response
                .json()
                .map_err(|_| "Invalid calendar event response.")?;
            let items = if provider == "google" {
                &body["items"]
            } else {
                &body["value"]
            };
            for item in items.as_array().ok_or("Calendar did not return events.")? {
                if let Some(mut event) = parse_event(provider, &calendar_id, item) {
                    if event.start_at <= now && now < event.end_at {
                        event.account_id = calendars.account_id.clone();
                        result.push(event);
                    }
                }
            }
            let next = if provider == "google" {
                body["nextPageToken"].as_str().map(|page| {
                    let mut next = first_page.clone();
                    next.query_pairs_mut().append_pair("pageToken", page);
                    next.to_string()
                })
            } else {
                body["@odata.nextLink"].as_str().map(str::to_string)
            };
            let Some(next) = next else {
                finished = true;
                break;
            };
            let candidate = Url::parse(&next).map_err(|_| "Invalid calendar page URL.")?;
            let trusted = if provider == "google" {
                "www.googleapis.com"
            } else {
                "graph.microsoft.com"
            };
            if candidate.scheme() != "https" || candidate.host_str() != Some(trusted) {
                return Err("Calendar returned an untrusted page URL.".into());
            }
            url = candidate;
        }
        if !finished {
            return Err("Calendar returned too many event pages to choose an event safely.".into());
        }
    }
    Ok(result)
}

fn key(event: &ObservedEvent) -> String {
    let input = format!(
        "{}:{}:{}:{}",
        event.provider,
        event.calendar_id,
        event.id,
        event.start_at.to_rfc3339()
    );
    let digest = Sha256::digest(input.as_bytes());
    digest[..8]
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

fn report_duration(seconds: i64) -> String {
    if seconds < 60 {
        "<1 min".into()
    } else if seconds < 600 {
        let tenths = (seconds * 10 + 30) / 60;
        if tenths % 10 == 0 {
            format!("{} min", tenths / 10)
        } else {
            format!("{}.{} min", tenths / 10, tenths % 10)
        }
    } else if seconds < 3600 {
        format!("{} min", (seconds + 30) / 60)
    } else {
        format!("{:.1} h", seconds as f64 / 3600.0)
    }
}

fn mini_report(
    db: &Connection,
    event: &ObservedEvent,
    excluded_applications: &[String],
) -> Result<Option<String>, String> {
    let start = event.start_at.max(event.first_seen_at);
    let end = event.end_at;
    let mut stmt = db.prepare("SELECT datetime(created_at), COALESCE(duration_seconds, 0), activity_type, active_app FROM reports WHERE datetime(created_at) > datetime(?1) AND datetime(created_at) <= datetime(?2, '+30 minutes') ORDER BY datetime(created_at)")
        .map_err(|e| e.to_string())?;
    let rows = stmt
        .query_map(params![start.to_rfc3339(), end.to_rfc3339()], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, i64>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, Option<String>>(3)?,
            ))
        })
        .map_err(|e| e.to_string())?;
    let mut intervals = Vec::new();
    for row in rows {
        let (stamp, duration, category, app) = row.map_err(|e| e.to_string())?;
        let Some(app) = app.as_deref() else {
            continue;
        };
        let app = crate::privacy::normalized_application(app);
        if excluded_applications
            .iter()
            .any(|excluded| crate::privacy::normalized_application(excluded) == app)
        {
            continue;
        }
        let at = NaiveDateTime::parse_from_str(&stamp, "%Y-%m-%d %H:%M:%S")
            .map_err(|_| "Invalid recorded activity time.")?
            .and_utc();
        let from = (at - ChronoDuration::seconds(duration.clamp(0, 1800))).max(start);
        let to = at.min(end);
        if from >= to {
            continue;
        }
        // Calendar descriptions can be shared with guests. Unlike private
        // local reports, never echo an arbitrary custom/model category here.
        let category = crate::focus_semantics::canonical_category_label(&category)
            .unwrap_or("Other")
            .to_string();
        intervals.push((from, to, category));
    }
    if intervals.is_empty() {
        return Ok(None);
    }
    intervals.sort_by_key(|(from, _, _)| *from);
    let observed_start = intervals[0].0;
    let observed_end = intervals.iter().map(|(_, to, _)| *to).max().unwrap_or(end);
    let mut total_seconds = 0i64;
    let mut categories = BTreeMap::<String, i64>::new();
    let mut last_category = String::new();
    let mut switches = 0usize;
    let mut covered_until = None;
    for (from, to, category) in intervals {
        // Attribute overlapping capture slices once, so category durations
        // add up to the observed total rather than over-counting it.
        let attributed_from = covered_until.map_or(from, |end| from.max(end));
        if attributed_from >= to {
            continue;
        }
        let seconds = (to - attributed_from).num_seconds();
        total_seconds += seconds;
        *categories.entry(category.clone()).or_default() += seconds;
        if !last_category.is_empty() && last_category != category {
            switches += 1;
        }
        last_category = category;
        covered_until = Some(to);
    }
    // A single short capture is not evidence of how a calendar block went.
    // In particular, never post a "0 min" recap to an event with guests.
    if total_seconds < REPORT_MIN_OBSERVED_SECONDS {
        return Ok(None);
    }
    let mut ranked = categories.into_iter().collect::<Vec<_>>();
    ranked.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));
    let main = ranked
        .first()
        .map(|(name, _)| name.as_str())
        .unwrap_or("Unclassified");
    let scheduled_seconds = (end - event.start_at).num_seconds().max(1);
    let coverage_percent =
        ((total_seconds * 100 + scheduled_seconds / 2) / scheduled_seconds).clamp(0, 100);
    let main_percent = ranked
        .first()
        .map(|(_, seconds)| (seconds * 100 + total_seconds / 2) / total_seconds)
        .unwrap_or(0);
    let summary = if switches == 0 {
        format!("{main} made up {main_percent}% of the observed time; no activity-category changes were recorded.")
    } else {
        format!("{main} made up {main_percent}% of the observed time; {switches} activity-category change{} were recorded.", if switches == 1 { "" } else { "s" })
    };
    let mut breakdown = ranked
        .iter()
        .take(3)
        .map(|(name, seconds)| {
            let percent = (seconds * 100 + total_seconds / 2) / total_seconds;
            format!("- {name}: {} ({percent}%)", report_duration(*seconds))
        })
        .collect::<Vec<_>>();
    let other_seconds: i64 = ranked.iter().skip(3).map(|(_, seconds)| seconds).sum();
    if other_seconds > 0 {
        let percent = (other_seconds * 100 + total_seconds / 2) / total_seconds;
        breakdown.push(format!(
            "- Other categories: {} ({percent}%)",
            report_duration(other_seconds)
        ));
    }
    let next_step = if coverage_percent < 60 && scheduled_seconds >= 600 {
        "If you want a fuller recap, keep tracking through the whole scheduled block next time."
            .to_string()
    } else if switches >= 3 && ranked.len() > 1 {
        format!(
            "If the shifts into {} were unplanned, give that work a separate block next time.",
            ranked[1].0
        )
    } else {
        "Add the concrete outcome or decision to this event if you want a completion record; activity alone cannot confirm it.".to_string()
    };
    Ok(Some(format!(
        "FlowSight · Session review\n\nAT A GLANCE\nCalendar block: {} – {}\nRecorded: {} of {} scheduled ({}%)\nObserved window: {} – {}\n{}\n\nTIME BY ACTIVITY\n{}\n\nNEXT STEP\n{}",
        event
            .start_at
            .with_timezone(&Local)
            .format("%Y-%m-%d %H:%M %:z"),
        end.with_timezone(&Local).format("%Y-%m-%d %H:%M %:z"),
        report_duration(total_seconds),
        report_duration(scheduled_seconds),
        coverage_percent,
        observed_start
            .with_timezone(&Local)
            .format("%Y-%m-%d %H:%M %:z"),
        observed_end
            .with_timezone(&Local)
            .format("%Y-%m-%d %H:%M %:z"),
        summary,
        breakdown.join("\n"),
        next_step,
    )))
}

fn report_grace_elapsed(event: &ObservedEvent, now: DateTime<Utc>) -> bool {
    now >= event.end_at + ChronoDuration::seconds(REPORT_GRACE_SECONDS)
}

fn html_escape(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

fn appended_payload(
    provider: &str,
    old_body: &str,
    content_type: &str,
    report: &str,
    marker: &str,
) -> Value {
    if provider == "google" {
        json!({"description":format!("{old_body}\n\n---\n{report}\n{marker}")})
    } else {
        let preserved = if content_type.eq_ignore_ascii_case("text") {
            html_escape(old_body).replace('\n', "<br>")
        } else {
            old_body.to_string()
        };
        let escaped = html_escape(report).replace('\n', "<br>");
        json!({"body":{"contentType":"HTML","content":format!("{preserved}<hr><p>{escaped}<br>{marker}</p>")}})
    }
}

fn publish(owner_user_id: &str, event: &ObservedEvent, report: &str) -> Result<(), String> {
    require_paid_for(owner_user_id)?;
    if !load_state_for(owner_user_id)?.auto_publish {
        return Err("Automatic calendar reports were disabled before publishing.".into());
    }
    let token = access_token_for(owner_user_id, &event.provider)?;
    let client = http()?;
    let encoded = urlencoding::encode(&event.id);
    let url = if event.provider == "google" {
        let calendar_id = if event.calendar_id.is_empty() {
            "primary"
        } else {
            &event.calendar_id
        };
        format!(
            "https://www.googleapis.com/calendar/v3/calendars/{}/events/{encoded}",
            urlencoding::encode(calendar_id)
        )
    } else {
        format!("https://graph.microsoft.com/v1.0/me/events/{encoded}")
    };
    // Graph returns dateTime without an offset. Match the UTC representation
    // used by calendarView before comparing the occurrence for a safe PATCH.
    let response = client
        .get(&url)
        .bearer_auth(&token)
        .header("Prefer", "outlook.timezone=\"UTC\"")
        .send()
        .map_err(|e| e.to_string())?;
    if !response.status().is_success() {
        return Err(format!(
            "Could not re-read calendar event (HTTP {}).",
            response.status()
        ));
    }
    let etag = response
        .headers()
        .get("etag")
        .and_then(|v| v.to_str().ok())
        .map(str::to_string);
    let body: Value = response
        .json()
        .map_err(|_| "Invalid calendar event response.")?;
    let latest = parse_event(&event.provider, &event.calendar_id, &body)
        .ok_or("The event no longer has a valid timed occurrence.")?;
    if !latest.organizer || latest.start_at != event.start_at || latest.end_at != event.end_at {
        return Err(
            "The event changed or you are not its organizer. No report was published.".into(),
        );
    }
    let marker = format!("FlowSight report ID: {}", key(event));
    let old_body = if event.provider == "google" {
        body["description"].as_str().unwrap_or("")
    } else {
        body["body"]["content"].as_str().unwrap_or("")
    };
    if old_body.contains(&marker) {
        return Ok(());
    }
    let payload = appended_payload(
        &event.provider,
        old_body,
        body["body"]["contentType"].as_str().unwrap_or("HTML"),
        report,
        &marker,
    );
    let patch_url = if event.provider == "google" {
        format!("{url}?sendUpdates=none")
    } else {
        url
    };
    if !load_state_for(owner_user_id)?.auto_publish {
        return Err("Automatic calendar reports were disabled before publishing.".into());
    }
    require_paid_for(owner_user_id)?;
    let mut request = client.patch(patch_url).bearer_auth(token).json(&payload);
    if let Some(etag) = etag {
        request = request.header("If-Match", etag);
    }
    let response = request.send().map_err(|e| e.to_string())?;
    if !response.status().is_success() {
        return Err(format!(
            "Calendar rejected the mini report (HTTP {}).",
            response.status()
        ));
    }
    Ok(())
}

fn tick() -> Result<(), String> {
    let now = Utc::now();
    let owner_user_id = current_owner_id().ok();
    if owner_user_id
        .as_deref()
        .map_or(true, |owner| require_paid_for(owner).is_err())
    {
        status_update_for(owner_user_id.as_deref(), |status| {
            status.current = None;
            status.overlap_count = 0;
            status.checked_at = Some(now);
        });
        if let Some(owner) = owner_user_id.as_deref() {
            let _guard = STATE_LOCK.lock().map_err(|e| e.to_string())?;
            let mut state = load_state_for(owner)?;
            if state.auto_publish || state.active.is_some() || !state.pending.is_empty() {
                state.auto_publish = false;
                state.active = None;
                state.pending.clear();
                save_state_for(owner, &state)?;
            }
        }
        return Ok(());
    }
    let owner_user_id = owner_user_id.expect("paid calendar requires a FlowSight account");
    let connected = ["google", "microsoft"]
        .into_iter()
        .filter(|provider| {
            load_tokens_for(&owner_user_id, provider)
                .ok()
                .flatten()
                .is_some()
        })
        .collect::<Vec<_>>();
    if connected.is_empty() {
        status_update_for(Some(&owner_user_id), |status| {
            status.current = None;
            status.overlap_count = 0;
            status.checked_at = Some(now);
        });
        return Ok(());
    }
    let mut events = Vec::new();
    for provider in connected {
        events.extend(events_now(&owner_user_id, provider, now)?);
    }
    if current_owner_id()? != owner_user_id {
        return Ok(());
    }
    events.sort_by_key(|event| event.start_at);
    let current = (events.len() == 1).then(|| events[0].clone());
    status_update_for(Some(&owner_user_id), |status| {
        status.current = current.clone();
        status.overlap_count = events.len();
        status.checked_at = Some(now);
        status.last_error = None;
    });
    let pending = {
        let _guard = STATE_LOCK.lock().map_err(|e| e.to_string())?;
        let mut state = load_state_for(&owner_user_id)?;
        if let Some(active) = state.active.clone() {
            let same = current.as_ref().is_some_and(|event| {
                event.provider == active.provider
                    && event.calendar_id == active.calendar_id
                    && event.id == active.id
                    && event.start_at == active.start_at
            });
            if same {
                state.active.as_mut().unwrap().last_seen_at = now;
            } else {
                let report_key = key(&active);
                if active.end_at <= now
                    && now - active.last_seen_at <= ChronoDuration::minutes(2)
                    && state.auto_publish
                    && !state.completed.contains(&report_key)
                {
                    state.pending.push(active);
                }
                state.active = None;
            }
        }
        if state.active.is_none() {
            if let Some(event) = current {
                if event.organizer {
                    state.active = Some(ObservedEvent {
                        provider: event.provider,
                        calendar_id: event.calendar_id,
                        id: event.id,
                        start_at: event.start_at,
                        end_at: event.end_at,
                        first_seen_at: now,
                        last_seen_at: now,
                    });
                }
            }
        }
        state.pending.retain(|event| {
            now - event.end_at <= ChronoDuration::seconds(REPORT_PENDING_TTL_SECONDS)
        });
        let pending = state.pending.first().cloned();
        save_state_for(&owner_user_id, &state)?;
        pending
    };
    let Some(pending) = pending else {
        return Ok(());
    };
    if !report_grace_elapsed(&pending, now) {
        return Ok(());
    }
    let excluded =
        crate::privacy::load_privacy_settings(&crate::paths::db_path()?)?.excluded_applications;
    let report = mini_report(&conn()?, &pending, &excluded)?;
    if let Some(ref report) = report {
        publish(&owner_user_id, &pending, report)?;
    }
    let _guard = STATE_LOCK.lock().map_err(|e| e.to_string())?;
    let mut state = load_state_for(&owner_user_id)?;
    let completed_key = key(&pending);
    state.pending.retain(|event| key(event) != completed_key);
    state.completed.push(completed_key);
    state.completed = state
        .completed
        .into_iter()
        .rev()
        .take(100)
        .collect::<Vec<_>>()
        .into_iter()
        .rev()
        .collect();
    save_state_for(&owner_user_id, &state)?;
    status_update_for(Some(&owner_user_id), |status| {
        status.last_report = Some(if report.is_some() {
            format!("Mini report added to the {} event.", pending.provider)
        } else {
            "Not enough recorded activity for a useful mini report, so nothing was posted.".into()
        })
    });
    Ok(())
}

pub fn start_monitor() {
    std::thread::spawn(|| loop {
        if let Err(error) = tick() {
            log::warn!("Calendar companion: {error}");
            let owner_user_id = current_owner_id().ok();
            status_update_for(owner_user_id.as_deref(), |status| {
                status.last_error = Some(error);
                status.current = None;
                status.overlap_count = 0;
            });
        }
        std::thread::sleep(Duration::from_secs(30));
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn calendar_requires_an_active_paid_plan_with_integrations() {
        let mut entitlements = crate::entitlements::Entitlements::free();
        assert!(!has_paid_calendar_access(&entitlements, "owner-a"));
        entitlements.status = "active".into();
        entitlements.can_integrations = true;
        assert!(!has_paid_calendar_access(&entitlements, "owner-a"));
        entitlements.plan = Some("individual_pro".into());
        assert!(!has_paid_calendar_access(&entitlements, "owner-a"));
        entitlements.owner_user_id = Some("owner-a".into());
        assert!(has_paid_calendar_access(&entitlements, "owner-a"));
        assert!(!has_paid_calendar_access(&entitlements, "owner-b"));
        entitlements.status = "past_due".into();
        assert!(!has_paid_calendar_access(&entitlements, "owner-a"));
        entitlements.status = "active".into();
        entitlements.plan = Some("individual_local".into());
        assert!(!has_paid_calendar_access(&entitlements, "owner-a"));
        entitlements.plan = Some("team".into());
        assert!(!has_paid_calendar_access(&entitlements, "owner-a"));
    }

    #[test]
    fn calendar_tokens_settings_and_cache_keys_are_scoped_per_flowsight_user() {
        let owner_a = "00000000-0000-0000-0000-000000000001";
        let owner_b = "00000000-0000-0000-0000-000000000002";
        assert_ne!(state_key(owner_a), state_key(owner_b));
        assert_ne!(
            token_key("google", owner_a).unwrap(),
            token_key("google", owner_b).unwrap()
        );
        assert_ne!(
            token_key("google", owner_a).unwrap(),
            token_key("microsoft", owner_a).unwrap()
        );
        assert_ne!(
            token_key("google", owner_a).unwrap(),
            "calendar_oauth_google"
        );
    }

    #[test]
    fn runtime_calendar_status_does_not_follow_a_different_flowsight_user() {
        status_update_for(Some("owner-a"), |status| {
            status.overlap_count = 2;
            status.last_report = Some("owner-a report".into());
        });
        status_update_for(Some("owner-b"), |_| {});
        let status = STATUS.get().unwrap().lock().unwrap();
        assert_eq!(status.owner_user_id.as_deref(), Some("owner-b"));
        assert_eq!(status.overlap_count, 0);
        assert!(status.last_report.is_none());
    }

    #[test]
    fn oauth_rejection_hint_never_returns_provider_description() {
        assert_eq!(
            oauth_rejection_hint(&json!({
                "error": "invalid_request",
                "error_description": "client_secret is missing."
            })),
            "client_secret_required"
        );
        assert_eq!(
            oauth_rejection_hint(&json!({
                "error_description": "Some unexpected provider detail"
            })),
            "no_detail"
        );
    }

    #[test]
    fn parses_only_timed_events_and_respects_organizer() {
        let event = parse_event("google", "primary", &json!({"id":"e1","summary":"Design","start":{"dateTime":"2026-09-30T10:00:00Z"},"end":{"dateTime":"2026-09-30T11:00:00Z"},"organizer":{"self":true},"attendees":[{}]})).unwrap();
        assert!(event.organizer);
        assert_eq!(event.attendee_count, 1);
        assert!(parse_event(
            "google",
            "primary",
            &json!({"id":"e2","start":{"date":"2026-09-30"}})
        )
        .is_none());
    }

    #[test]
    fn microsoft_event_keeps_its_organizer_and_occurrence_time() {
        let event = parse_event(
            "microsoft",
            "calendar-one",
            &json!({
                "id":"occurrence-1",
                "subject":"Planning",
                "start":{"dateTime":"2026-09-30T10:00:00.0000000","timeZone":"UTC"},
                "end":{"dateTime":"2026-09-30T11:00:00.0000000","timeZone":"UTC"},
                "isOrganizer":true,
                "attendees":[{"emailAddress":{"address":"guest@example.com"}}],
                "body":{"contentType":"html","content":"<p>Agenda</p>"}
            }),
        )
        .unwrap();
        assert_eq!(event.provider, "microsoft");
        assert_eq!(event.calendar_id, "calendar-one");
        assert_eq!(event.start_at.to_rfc3339(), "2026-09-30T10:00:00+00:00");
        assert_eq!(event.description, "Agenda");
        assert!(event.organizer);
        assert_eq!(event.attendee_count, 1);
    }

    #[test]
    fn report_key_is_stable_per_occurrence() {
        let start = DateTime::parse_from_rfc3339("2026-09-30T10:00:00Z")
            .unwrap()
            .with_timezone(&Utc);
        let event = ObservedEvent {
            provider: "google".into(),
            calendar_id: "primary".into(),
            id: "one".into(),
            start_at: start,
            end_at: start + ChronoDuration::hours(1),
            first_seen_at: start,
            last_seen_at: start,
        };
        assert_eq!(key(&event), key(&event));
        let mut other = event.clone();
        other.start_at += ChronoDuration::days(7);
        assert_ne!(key(&event), key(&other));
    }

    #[test]
    fn google_event_url_has_no_empty_calendar_segment() {
        let start = DateTime::parse_from_rfc3339("2026-09-30T10:00:00Z")
            .unwrap()
            .with_timezone(&Utc);
        let end = start + ChronoDuration::hours(2);
        let url = events_url("google", "primary", &start, &end).unwrap();
        assert_eq!(url.path(), "/calendar/v3/calendars/primary/events");
        let graph = events_url("microsoft", "calendar-one", &start, &end).unwrap();
        assert_eq!(graph.path(), "/v1.0/me/calendars/calendar-one/calendarView");
    }

    #[test]
    fn google_event_link_uses_the_connected_account() {
        let mut url =
            Url::parse("https://www.google.com/calendar/event?eid=abc&authuser=wrong").unwrap();
        with_google_authuser(&mut url, "owner@example.com");
        let pairs = url.query_pairs().collect::<Vec<_>>();
        assert_eq!(pairs.iter().filter(|(key, _)| key == "authuser").count(), 1);
        assert!(pairs.contains(&("eid".into(), "abc".into())));
        assert!(pairs.contains(&("authuser".into(), "owner@example.com".into())));
    }

    #[test]
    fn mini_report_is_clipped_to_seen_event_time_and_excludes_private_apps() {
        let db = Connection::open_in_memory().unwrap();
        db.execute_batch("CREATE TABLE reports (created_at TEXT, duration_seconds INTEGER, activity_type TEXT, active_app TEXT);").unwrap();
        db.execute(
            "INSERT INTO reports VALUES ('2026-09-30 10:10:00', 600, 'Coding', 'Code.exe')",
            [],
        )
        .unwrap();
        db.execute(
            "INSERT INTO reports VALUES ('2026-09-30 10:20:00', 600, 'Coding', 'Code.exe')",
            [],
        )
        .unwrap();
        db.execute(
            "INSERT INTO reports VALUES ('2026-09-30 10:30:00', 600, 'Browsing', 'Bitwarden.exe')",
            [],
        )
        .unwrap();
        let start = DateTime::parse_from_rfc3339("2026-09-30T10:00:00Z")
            .unwrap()
            .with_timezone(&Utc);
        let event = ObservedEvent {
            provider: "google".into(),
            calendar_id: "primary".into(),
            id: "one".into(),
            start_at: start,
            end_at: start + ChronoDuration::hours(1),
            first_seen_at: start + ChronoDuration::minutes(5),
            last_seen_at: start + ChronoDuration::minutes(59),
        };
        let report = mini_report(&db, &event, &["Bitwarden".into()])
            .unwrap()
            .unwrap();
        assert!(report.contains("AT A GLANCE"));
        assert!(report.contains("Recorded: 15 min of 1.0 h scheduled (25%)"));
        assert!(report.contains("TIME BY ACTIVITY\n- Coding: 15 min (100%)"));
        assert!(report.contains("NEXT STEP\nIf you want a fuller recap"));
        assert!(!report.contains("Bitwarden"));
        assert!(!report.contains("10:30"));
    }

    #[test]
    fn mini_report_waits_for_late_capture_and_skips_too_little_evidence() {
        let db = Connection::open_in_memory().unwrap();
        db.execute_batch("CREATE TABLE reports (created_at TEXT, duration_seconds INTEGER, activity_type TEXT, active_app TEXT);").unwrap();
        let start = DateTime::parse_from_rfc3339("2026-09-30T10:00:00Z")
            .unwrap()
            .with_timezone(&Utc);
        let event = ObservedEvent {
            provider: "google".into(),
            calendar_id: "primary".into(),
            id: "one".into(),
            start_at: start,
            end_at: start + ChronoDuration::minutes(10),
            first_seen_at: start,
            last_seen_at: start + ChronoDuration::minutes(9),
        };
        assert!(!report_grace_elapsed(
            &event,
            event.end_at + ChronoDuration::seconds(89)
        ));
        assert!(report_grace_elapsed(
            &event,
            event.end_at + ChronoDuration::seconds(90)
        ));
        db.execute(
            "INSERT INTO reports VALUES ('2026-09-30 10:01:00', 19, 'Analysis', 'Code.exe')",
            [],
        )
        .unwrap();
        assert!(mini_report(&db, &event, &[]).unwrap().is_none());
        // This slice lands in SQLite after the event, but its observed time
        // still overlaps the event. It belongs in the final recap.
        db.execute(
            "INSERT INTO reports VALUES ('2026-09-30 10:10:30', 120, 'Coding', 'Code.exe')",
            [],
        )
        .unwrap();
        let report = mini_report(&db, &event, &[]).unwrap().unwrap();
        assert!(report.contains("Recorded: 1.8 min of 10 min scheduled (18%)"));
        assert!(report.contains("- Coding: 1.5 min (83%)"));
        assert!(report.contains("- Analysis: <1 min (17%)"));
    }

    #[test]
    fn mini_report_gives_only_evidence_based_next_steps_without_private_app_names() {
        let db = Connection::open_in_memory().unwrap();
        db.execute_batch("CREATE TABLE reports (created_at TEXT, duration_seconds INTEGER, activity_type TEXT, active_app TEXT);").unwrap();
        for (minute, category, app) in [
            (1, "Coding", "Code.exe"),
            (2, "Communication", "PrivateMessenger.exe"),
            (3, "Coding", "Code.exe"),
            (4, "Communication", "PrivateMessenger.exe"),
        ] {
            db.execute(
                "INSERT INTO reports VALUES (?1, 60, ?2, ?3)",
                params![format!("2026-09-30 10:{minute:02}:00"), category, app],
            )
            .unwrap();
        }
        let start = DateTime::parse_from_rfc3339("2026-09-30T10:00:00Z")
            .unwrap()
            .with_timezone(&Utc);
        let event = ObservedEvent {
            provider: "google".into(),
            calendar_id: "primary".into(),
            id: "with-guests".into(),
            start_at: start,
            end_at: start + ChronoDuration::minutes(4),
            first_seen_at: start,
            last_seen_at: start + ChronoDuration::minutes(4),
        };
        let report = mini_report(&db, &event, &[]).unwrap().unwrap();
        assert!(report.contains("Recorded: 4 min of 4 min scheduled (100%)"));
        assert!(report.contains("3 activity-category changes were recorded"));
        assert!(report.contains("NEXT STEP\nIf the shifts into Communication were unplanned"));
        assert!(!report.contains("PrivateMessenger"));
        assert!(!report.contains("Code.exe"));
    }

    #[test]
    fn shared_calendar_report_never_repeats_custom_category_text() {
        let db = Connection::open_in_memory().unwrap();
        db.execute_batch("CREATE TABLE reports (created_at TEXT, duration_seconds INTEGER, activity_type TEXT, active_app TEXT);").unwrap();
        db.execute(
            "INSERT INTO reports VALUES ('2026-09-30 10:01:00', 60, 'Private client project', 'SecretApp.exe')",
            [],
        )
        .unwrap();
        db.execute(
            "INSERT INTO reports VALUES ('2026-09-30 10:02:00', 60, 'Analysis', 'Code.exe')",
            [],
        )
        .unwrap();
        let start = DateTime::parse_from_rfc3339("2026-09-30T10:00:00Z")
            .unwrap()
            .with_timezone(&Utc);
        let event = ObservedEvent {
            provider: "google".into(),
            calendar_id: "primary".into(),
            id: "with-guests".into(),
            start_at: start,
            end_at: start + ChronoDuration::minutes(2),
            first_seen_at: start,
            last_seen_at: start + ChronoDuration::minutes(2),
        };
        let report = mini_report(&db, &event, &[]).unwrap().unwrap();
        assert!(report.contains("- Other: 1 min (50%)"));
        assert!(!report.contains("Private client project"));
        assert!(!report.contains("SecretApp"));
    }

    #[test]
    fn append_preserves_existing_event_content_and_escapes_microsoft_html() {
        let google = appended_payload(
            "google",
            "Original agenda",
            "text",
            "FlowSight mini work report",
            "FlowSight report ID: a1",
        );
        assert!(google["description"]
            .as_str()
            .unwrap()
            .starts_with("Original agenda\n\n---"));
        let graph = appended_payload(
            "microsoft",
            "<div>Teams join blob</div>",
            "HTML",
            "Focus <done>",
            "FlowSight report ID: a1",
        );
        let content = graph["body"]["content"].as_str().unwrap();
        assert!(content.starts_with("<div>Teams join blob</div>"));
        assert!(content.contains("Focus &lt;done&gt;"));
        assert_eq!(graph["body"]["contentType"], "HTML");
    }
}
