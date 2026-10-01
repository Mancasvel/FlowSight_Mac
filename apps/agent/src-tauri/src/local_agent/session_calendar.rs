//! Calendar I/O for a session that the user has explicitly reviewed and confirmed.
//! OAuth credentials are supplied by the owner-scoped companion, never read here.
//! Stable provider IDs and private markers let a persisted save journal recover an
//! uncertain HTTP result without creating a second copy of a work block.

use std::io::Read;
use std::time::Duration;

use chrono::{DateTime, NaiveDate, NaiveDateTime, TimeZone, Utc};
use chrono_tz::Tz;
use reqwest::blocking::{Client, Response};
use reqwest::StatusCode;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};

use super::state::LocalEvent;

const PAGE_LIMIT: usize = 10;
const EVENT_LIMIT: usize = 25_000;
const BODY_LIMIT: u64 = 8 * 1024 * 1024;
const SESSION_PROPERTY: &str =
    "String {235522ce-e5ac-4f6f-aec7-3f7e8ad0e11b} Name FlowSightSession";
const BLOCK_PROPERTY: &str = "String {235522ce-e5ac-4f6f-aec7-3f7e8ad0e11b} Name FlowSightBlock";

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CalendarTarget {
    pub owner_user_id: String,
    pub provider: String,
    pub calendar_id: String,
}

pub struct CalendarClient {
    target: CalendarTarget,
    token: String,
    http: Client,
    base_url: String,
}

fn digest(parts: &[&str]) -> String {
    let mut hash = Sha256::new();
    for part in parts {
        // Length prefixes avoid delimiter collisions in arbitrary calendar IDs.
        hash.update((part.len() as u64).to_be_bytes());
        hash.update(part.as_bytes());
    }
    format!("{:x}", hash.finalize())
}

fn time(value: &str) -> Result<DateTime<Utc>, String> {
    DateTime::parse_from_rfc3339(value)
        .map(|date| date.with_timezone(&Utc))
        .map_err(|_| "The calendar returned an invalid event time.".into())
}

fn zoned_time(value: &str, zone: &str) -> Result<DateTime<Utc>, String> {
    if let Ok(value) = time(value) {
        return Ok(value);
    }
    let zone: Tz = zone
        .parse()
        .map_err(|_| "The calendar returned an unsupported time zone.".to_string())?;
    let local = NaiveDateTime::parse_from_str(value, "%Y-%m-%dT%H:%M:%S%.f")
        .map_err(|_| "The calendar returned an invalid local event time.".to_string())?;
    zone.from_local_datetime(&local)
        .single()
        .map(|value| value.with_timezone(&Utc))
        .ok_or_else(|| "The calendar returned an ambiguous local event time.".into())
}

fn google_time(value: &Value, calendar_zone: &str) -> Result<DateTime<Utc>, String> {
    let zone = value["timeZone"].as_str().unwrap_or(calendar_zone);
    if let Some(date) = value["dateTime"].as_str() {
        return zoned_time(date, zone);
    }
    let date = value["date"]
        .as_str()
        .ok_or("The calendar returned an event without a time.")?;
    let date = NaiveDate::parse_from_str(date, "%Y-%m-%d")
        .map_err(|_| "The calendar returned an invalid all-day event date.".to_string())?;
    let zone: Tz = zone
        .parse()
        .map_err(|_| "The calendar returned an unsupported time zone.".to_string())?;
    zone.from_local_datetime(&date.and_hms_opt(0, 0, 0).unwrap())
        .single()
        .map(|value| value.with_timezone(&Utc))
        .ok_or_else(|| "The calendar returned an ambiguous all-day event date.".into())
}

fn graph_time(value: &Value) -> Result<DateTime<Utc>, String> {
    let date = value["dateTime"]
        .as_str()
        .ok_or("The calendar returned an event without a time.")?;
    // Prefer: outlook.timezone="UTC" is sent on every Graph read. Do not guess
    // that a local timestamp in another (possibly Windows) zone means UTC.
    zoned_time(date, value["timeZone"].as_str().unwrap_or(""))
}

fn read_json(mut response: Response) -> Result<Value, String> {
    let mut bytes = Vec::new();
    response
        .by_ref()
        .take(BODY_LIMIT + 1)
        .read_to_end(&mut bytes)
        .map_err(|_| "The calendar response could not be read. Please retry.".to_string())?;
    if bytes.len() as u64 > BODY_LIMIT {
        return Err("The calendar response exceeded the safe size limit.".into());
    }
    serde_json::from_slice(&bytes).map_err(|_| "The calendar returned an invalid response.".into())
}

fn http_error(status: StatusCode) -> String {
    match status.as_u16() {
        401 => "Your calendar connection expired. Reconnect it and retry.".into(),
        403 => {
            "Your calendar connection does not permit this action. Reconnect it and retry.".into()
        }
        429 => "Your calendar is temporarily limiting requests. Please retry shortly.".into(),
        _ => format!(
            "The calendar request failed (HTTP {}). Please retry.",
            status.as_u16()
        ),
    }
}

impl CalendarClient {
    pub fn new(target: CalendarTarget, token: String) -> Result<Self, String> {
        let base = match target.provider.as_str() {
            "google" => "https://www.googleapis.com/calendar/v3",
            "microsoft" => "https://graph.microsoft.com/v1.0",
            _ => return Err("Unsupported linked calendar provider.".into()),
        };
        Self::build(target, token, base)
    }

    fn build(target: CalendarTarget, token: String, base: &str) -> Result<Self, String> {
        if target.owner_user_id.trim().is_empty()
            || target.calendar_id.trim().is_empty()
            || token.trim().is_empty()
        {
            return Err("The linked calendar connection is incomplete.".into());
        }
        let http = Client::builder()
            .timeout(Duration::from_secs(20))
            .connect_timeout(Duration::from_secs(10))
            .redirect(reqwest::redirect::Policy::none())
            .build()
            .map_err(|_| "The calendar connection could not be prepared.".to_string())?;
        Ok(Self {
            target,
            token,
            http,
            base_url: base.trim_end_matches('/').to_string(),
        })
    }

    #[cfg(test)]
    fn test_client(target: CalendarTarget, base: &str) -> Self {
        let url = url::Url::parse(base).unwrap();
        assert_eq!(url.scheme(), "http");
        assert_eq!(url.host_str(), Some("127.0.0.1"));
        Self::build(target, "synthetic-token".into(), base).unwrap()
    }

    fn events_url(&self) -> String {
        let calendar = urlencoding::encode(&self.target.calendar_id);
        if self.target.provider == "google" {
            format!("{}/calendars/{calendar}/events", self.base_url)
        } else {
            format!("{}/me/calendars/{calendar}/events", self.base_url)
        }
    }

    fn get(&self, url: &str) -> Result<Response, String> {
        self.http
            .get(url)
            .bearer_auth(&self.token)
            .header("Prefer", "outlook.timezone=\"UTC\"")
            .send()
            .map_err(|_| "The linked calendar could not be reached. Please retry.".into())
    }

    fn checked_get(&self, url: &str) -> Result<Value, String> {
        let response = self.get(url)?;
        if !response.status().is_success() {
            return Err(http_error(response.status()));
        }
        read_json(response)
    }

    fn session_marker(&self, session: &str) -> String {
        digest(&[
            "flowsight-session-v1",
            &self.target.owner_user_id,
            &self.target.provider,
            &self.target.calendar_id,
            session,
        ])
    }

    fn block_marker(&self, session: &str, index: usize, event: &LocalEvent) -> String {
        digest(&[
            "flowsight-block-v1",
            &self.session_marker(session),
            &index.to_string(),
            &event.id,
        ])
    }

    fn google_id(&self, block_marker: &str) -> String {
        // Google accepts base32hex [0-9a-v]; lowercase hexadecimal is a subset.
        format!("fs{block_marker}")
    }

    fn transaction_id(&self, block_marker: &str) -> String {
        format!(
            "{}-{}-{}-{}-{}",
            &block_marker[..8],
            &block_marker[8..12],
            &block_marker[12..16],
            &block_marker[16..20],
            &block_marker[20..32]
        )
    }

    fn property<'a>(event: &'a Value, name: &str) -> Option<&'a str> {
        event["singleValueExtendedProperties"]
            .as_array()?
            .iter()
            .find(|property| property["id"].as_str() == Some(name))?["value"]
            .as_str()
    }

    fn belongs_to_session(&self, event: &Value, session: &str) -> bool {
        let marker = self.session_marker(session);
        if self.target.provider == "google" {
            event["extendedProperties"]["private"]["fsSession"].as_str() == Some(&marker)
                && event["extendedProperties"]["private"]["fsOwner"].as_str()
                    == Some(&digest(&[&self.target.owner_user_id]))
        } else {
            Self::property(event, SESSION_PROPERTY) == Some(marker.as_str())
        }
    }

    fn trusted_next(&self, next: &str, expected_path: &str) -> Result<String, String> {
        let base = url::Url::parse(&self.base_url).unwrap();
        let url = url::Url::parse(next)
            .map_err(|_| "The calendar returned an invalid page link.".to_string())?;
        if url.origin() != base.origin()
            || url.username() != ""
            || url.password().is_some()
            || url.path() != expected_path
            || url.fragment().is_some()
        {
            return Err("The calendar returned an untrusted page link.".into());
        }
        Ok(url.to_string())
    }

    fn graph_session_event_ids(
        &self,
        session: &str,
    ) -> Result<std::collections::HashSet<String>, String> {
        // Graph calendarView does not reliably expose legacy extended properties.
        // Resolve IDs through the events endpoint, whose extension filter/expand
        // is supported, then use those IDs for availability recovery.
        let mut url = url::Url::parse(&self.events_url()).unwrap();
        let expected_path = url.path().to_string();
        url.query_pairs_mut()
            .append_pair(
                "$filter",
                &format!("singleValueExtendedProperties/Any(ep: ep/id eq '{SESSION_PROPERTY}' and ep/value eq '{}')", self.session_marker(session)),
            )
            .append_pair("$select", "id")
            .append_pair(
                "$expand",
                &format!("singleValueExtendedProperties($filter=id eq '{SESSION_PROPERTY}')"),
            )
            .append_pair("$top", "1000");
        let mut ids = std::collections::HashSet::new();
        let mut seen_pages = std::collections::HashSet::new();
        for _ in 0..PAGE_LIMIT {
            if !seen_pages.insert(url.to_string()) {
                return Err("The calendar repeated a page of saved blocks.".into());
            }
            let body = self.checked_get(url.as_str())?;
            let items = body["value"]
                .as_array()
                .ok_or("The calendar returned an invalid saved-session lookup.")?;
            for event in items {
                if !self.belongs_to_session(event, session) {
                    return Err("The calendar returned a mismatched private session marker.".into());
                }
                let id = event["id"]
                    .as_str()
                    .filter(|id| !id.is_empty())
                    .ok_or("The calendar returned a saved block without an ID.")?;
                ids.insert(id.to_string());
                if ids.len() > EVENT_LIMIT {
                    return Err("The calendar returned too many saved blocks.".into());
                }
            }
            let Some(next) = body["@odata.nextLink"].as_str() else {
                return Ok(ids);
            };
            url = url::Url::parse(&self.trusted_next(next, &expected_path)?).unwrap();
        }
        Err("The calendar returned too many pages of saved blocks.".into())
    }

    /// Read only opaque busy blocks. Summaries, descriptions, invitees and
    /// unrelated event payloads never leave this module.
    pub fn busy_events(
        &self,
        start: DateTime<Utc>,
        end: DateTime<Utc>,
        exclude_session: Option<&str>,
    ) -> Result<Vec<LocalEvent>, String> {
        if start >= end {
            return Err("The session has an invalid time window.".into());
        }
        let google = self.target.provider == "google";
        let graph_excluded = if !google {
            exclude_session
                .map(|session| self.graph_session_event_ids(session))
                .transpose()?
                .unwrap_or_default()
        } else {
            std::collections::HashSet::new()
        };
        let mut url = url::Url::parse(&if google {
            self.events_url()
        } else {
            format!(
                "{}/calendarView",
                self.events_url().trim_end_matches("/events")
            )
        })
        .unwrap();
        let expected_path = url.path().to_string();
        if google {
            url.query_pairs_mut()
                .append_pair("timeMin", &start.to_rfc3339())
                .append_pair("timeMax", &end.to_rfc3339())
                .append_pair("singleEvents", "true")
                .append_pair("showDeleted", "false")
                .append_pair("maxResults", "2500");
        } else {
            url.query_pairs_mut()
                .append_pair("startDateTime", &start.to_rfc3339())
                .append_pair("endDateTime", &end.to_rfc3339())
                .append_pair("$top", "1000")
                .append_pair("$select", "id,start,end,showAs,isCancelled");
        }
        let first_url = url.clone();
        let mut events = Vec::new();
        let mut seen_pages = std::collections::HashSet::new();
        for _ in 0..PAGE_LIMIT {
            if !seen_pages.insert(url.to_string()) {
                return Err("The calendar repeated a page of events.".into());
            }
            let body = self.checked_get(url.as_str())?;
            let items = body[if google { "items" } else { "value" }]
                .as_array()
                .ok_or("The calendar did not return availability.")?;
            for event in items {
                let free = if google {
                    event["status"] == "cancelled" || event["transparency"] == "transparent"
                } else {
                    event["isCancelled"] == true || event["showAs"] == "free"
                };
                let excluded = if google {
                    exclude_session.is_some_and(|session| self.belongs_to_session(event, session))
                } else {
                    event["id"]
                        .as_str()
                        .is_some_and(|id| graph_excluded.contains(id))
                };
                if free || excluded {
                    continue;
                }
                let at = if google {
                    google_time(&event["start"], body["timeZone"].as_str().unwrap_or(""))?
                } else {
                    graph_time(&event["start"])?
                };
                let to = if google {
                    google_time(&event["end"], body["timeZone"].as_str().unwrap_or(""))?
                } else {
                    graph_time(&event["end"])?
                };
                if at >= to {
                    return Err("The calendar returned an invalid busy interval.".into());
                }
                if at >= end || to <= start {
                    continue;
                }
                let id = event["id"]
                    .as_str()
                    .filter(|id| !id.is_empty())
                    .ok_or("The calendar returned an event without an ID.")?;
                events.push(LocalEvent {
                    id: format!("{}-busy-{}", self.target.provider, digest(&[id])),
                    title: "Busy".into(),
                    start_at: at.to_rfc3339(),
                    end_at: to.to_rfc3339(),
                    created_at: String::new(),
                    updated_at: String::new(),
                    provider: Some(self.target.provider.clone()),
                    external_id: Some(id.to_string()),
                });
                if events.len() > EVENT_LIMIT {
                    return Err("The calendar returned too many busy intervals.".into());
                }
            }
            if google {
                let Some(token) = body["nextPageToken"].as_str() else {
                    return Ok(events);
                };
                if token.is_empty() || token.len() > 4096 {
                    return Err("The calendar returned an invalid page token.".into());
                }
                url = first_url.clone();
                url.query_pairs_mut().append_pair("pageToken", token);
            } else {
                let Some(next) = body["@odata.nextLink"].as_str() else {
                    return Ok(events);
                };
                url = url::Url::parse(&self.trusted_next(next, &expected_path)?).unwrap();
            }
        }
        Err("The calendar returned too many pages to establish availability safely.".into())
    }

    fn verify_existing(
        &self,
        remote: &Value,
        session: &str,
        block_marker: &str,
        event: &LocalEvent,
    ) -> Result<String, String> {
        let google = self.target.provider == "google";
        let title = remote[if google { "summary" } else { "subject" }].as_str();
        let marker_matches = if google {
            remote["extendedProperties"]["private"]["fsBlock"].as_str() == Some(block_marker)
                && remote["id"].as_str() == Some(self.google_id(block_marker).as_str())
                && remote["status"] != "cancelled"
        } else {
            Self::property(remote, BLOCK_PROPERTY) == Some(block_marker)
                && remote["transactionId"].as_str()
                    == Some(self.transaction_id(block_marker).as_str())
                && remote["isCancelled"] != true
        };
        let at = if google {
            google_time(&remote["start"], "")?
        } else {
            graph_time(&remote["start"])?
        };
        let to = if google {
            google_time(&remote["end"], "")?
        } else {
            graph_time(&remote["end"])?
        };
        let session_matches = if google {
            self.belongs_to_session(remote, session)
        } else {
            // Graph supports expanding a specific legacy property. The block
            // hash already binds owner, calendar and session; validate the
            // session property too if the creation response includes it.
            Self::property(remote, SESSION_PROPERTY)
                .map_or(true, |value| value == self.session_marker(session))
        };
        if !session_matches
            || !marker_matches
            || title != Some(event.title.as_str())
            || at != time(&event.start_at)?
            || to != time(&event.end_at)?
        {
            return Err("A saved calendar block changed or its private marker does not match. Review the calendar before retrying.".into());
        }
        remote["id"]
            .as_str()
            .filter(|id| !id.is_empty())
            .map(str::to_string)
            .ok_or_else(|| "The calendar did not return an event ID.".into())
    }

    fn lookup(&self, block_marker: &str) -> Result<Option<Value>, String> {
        if self.target.provider == "google" {
            let response = self.get(&format!(
                "{}/{}",
                self.events_url(),
                self.google_id(block_marker)
            ))?;
            if response.status() == StatusCode::NOT_FOUND {
                return Ok(None);
            }
            if !response.status().is_success() {
                return Err(http_error(response.status()));
            }
            return read_json(response).map(Some);
        }
        let mut url = url::Url::parse(&self.events_url()).unwrap();
        url.query_pairs_mut()
            .append_pair(
                "$filter",
                &format!("singleValueExtendedProperties/Any(ep: ep/id eq '{BLOCK_PROPERTY}' and ep/value eq '{block_marker}')"),
            )
            .append_pair("$select", "id,subject,start,end,transactionId,isCancelled")
            .append_pair(
                "$expand",
                &format!("singleValueExtendedProperties($filter=id eq '{BLOCK_PROPERTY}')"),
            )
            .append_pair("$top", "2");
        let body = self.checked_get(url.as_str())?;
        let items = body["value"]
            .as_array()
            .ok_or("The calendar returned an invalid saved-block lookup.")?;
        if items.len() > 1 || body["@odata.nextLink"].is_string() {
            return Err(
                "The calendar contains duplicate private markers. Review it before retrying."
                    .into(),
            );
        }
        Ok(items.first().cloned())
    }

    /// Ensure one journaled block exists unchanged. At most one POST is made
    /// per call. All retries first read the stable ID/private marker; Graph's
    /// transactionId additionally protects a commit followed by a lost reply.
    pub fn ensure_event(
        &self,
        session: &str,
        index: usize,
        event: &LocalEvent,
    ) -> Result<String, String> {
        if session.trim().is_empty() || event.id.trim().is_empty() || event.title.trim().is_empty()
        {
            return Err("The reviewed calendar block is incomplete.".into());
        }
        let at = time(&event.start_at)?;
        let to = time(&event.end_at)?;
        if at >= to {
            return Err("The reviewed calendar block has invalid times.".into());
        }
        let block_marker = self.block_marker(session, index, event);
        if let Some(existing) = self.lookup(&block_marker)? {
            return self.verify_existing(&existing, session, &block_marker, event);
        }
        let payload = if self.target.provider == "google" {
            json!({
                "id":self.google_id(&block_marker), "summary":event.title,
                "start":{"dateTime":at.to_rfc3339()}, "end":{"dateTime":to.to_rfc3339()},
                "extendedProperties":{"private":{
                    "fsSession":self.session_marker(session), "fsBlock":block_marker,
                    "fsOwner":digest(&[&self.target.owner_user_id])
                }}
            })
        } else {
            json!({
                "subject":event.title, "transactionId":self.transaction_id(&block_marker),
                "start":{"dateTime":at.format("%Y-%m-%dT%H:%M:%S%.f").to_string(),"timeZone":"UTC"},
                "end":{"dateTime":to.format("%Y-%m-%dT%H:%M:%S%.f").to_string(),"timeZone":"UTC"},
                "singleValueExtendedProperties":[
                    {"id":SESSION_PROPERTY,"value":self.session_marker(session)},
                    {"id":BLOCK_PROPERTY,"value":block_marker}
                ]
            })
        };
        let response = self
            .http
            .post(self.events_url())
            .bearer_auth(&self.token)
            .header("Prefer", "outlook.timezone=\"UTC\"")
            .json(&payload)
            .send();
        match response {
            Ok(response) if response.status().is_success() => {
                // Creation replies can omit private properties. The same read
                // used for recovery is authoritative, never an unchecked ID.
                if let Ok(body) = read_json(response) {
                    if let Ok(id) = self.verify_existing(&body, session, &block_marker, event) {
                        return Ok(id);
                    }
                }
            }
            Ok(response)
                if response.status() != StatusCode::CONFLICT
                    && !response.status().is_server_error() =>
            {
                return Err(http_error(response.status()));
            }
            Ok(_) | Err(_) => {}
        }
        // A lost/failed response may still have committed. Recover by reading;
        // do not repeat POST in this call or turn a partial save into success.
        if let Some(existing) = self.lookup(&block_marker)? {
            return self.verify_existing(&existing, session, &block_marker, event);
        }
        Err("Saving this calendar block could not be confirmed. Retry the reviewed session to resume without duplicates.".into())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{Arc, Mutex};
    use std::thread;
    use tiny_http::{Request, Server};

    fn target(provider: &str) -> CalendarTarget {
        CalendarTarget {
            owner_user_id: "fictional-owner-a".into(),
            provider: provider.into(),
            calendar_id: "calendar+example@invalid.test".into(),
        }
    }

    fn event(index: usize) -> LocalEvent {
        LocalEvent {
            id: format!("fixture-{index}"),
            title: format!("Ejercicio {} de ADDA", index + 1),
            start_at: format!("2026-10-01T{:02}:00:00+02:00", 10 + index),
            end_at: format!("2026-10-01T{:02}:45:00+02:00", 10 + index),
            created_at: String::new(),
            updated_at: String::new(),
            provider: None,
            external_id: None,
        }
    }

    fn serve(
        count: usize,
        mut handle: impl FnMut(&mut Request, usize) -> (u16, Value) + Send + 'static,
    ) -> (String, thread::JoinHandle<()>) {
        let server = Server::http("127.0.0.1:0").unwrap();
        let base = format!("http://{}", server.server_addr());
        let worker = thread::spawn(move || {
            for index in 0..count {
                let mut request = server
                    .recv_timeout(Duration::from_secs(5))
                    .unwrap()
                    .expect("expected a synthetic calendar request");
                assert_eq!(
                    request
                        .headers()
                        .iter()
                        .find(|header| header.field.equiv("Authorization"))
                        .unwrap()
                        .value
                        .as_str(),
                    "Bearer synthetic-token"
                );
                let (status, body) = handle(&mut request, index);
                request
                    .respond(
                        tiny_http::Response::from_string(body.to_string())
                            .with_status_code(status)
                            .with_header(
                                tiny_http::Header::from_bytes("Content-Type", "application/json")
                                    .unwrap(),
                            ),
                    )
                    .unwrap();
            }
            assert!(server
                .recv_timeout(Duration::from_millis(100))
                .unwrap()
                .is_none());
        });
        (base, worker)
    }

    fn body(request: &mut Request) -> Value {
        let mut value = String::new();
        request.as_reader().read_to_string(&mut value).unwrap();
        serde_json::from_str(&value).unwrap()
    }

    fn remote(client: &CalendarClient, session: &str, index: usize, value: &LocalEvent) -> Value {
        let block = client.block_marker(session, index, value);
        if client.target.provider == "google" {
            json!({
                "id":client.google_id(&block), "status":"confirmed", "summary":value.title,
                "start":{"dateTime":value.start_at}, "end":{"dateTime":value.end_at},
                "extendedProperties":{"private":{
                    "fsSession":client.session_marker(session),"fsBlock":block,
                    "fsOwner":digest(&[&client.target.owner_user_id])
                }}
            })
        } else {
            json!({
                "id":format!("remote-{index}"),"subject":value.title,"isCancelled":false,
                "start":{"dateTime":time(&value.start_at).unwrap().format("%Y-%m-%dT%H:%M:%S").to_string(),"timeZone":"UTC"},
                "end":{"dateTime":time(&value.end_at).unwrap().format("%Y-%m-%dT%H:%M:%S").to_string(),"timeZone":"UTC"},
                "transactionId":client.transaction_id(&block),
                "singleValueExtendedProperties":[
                    {"id":SESSION_PROPERTY,"value":client.session_marker(session)},
                    {"id":BLOCK_PROPERTY,"value":block}
                ]
            })
        }
    }

    #[test]
    fn google_save_and_repeated_confirmation_create_one_event() {
        let stored = Arc::new(Mutex::new(None::<Value>));
        let server_state = stored.clone();
        let (base, worker) = serve(3, move |request, index| match index {
            0 => {
                assert_eq!(request.method().as_str(), "GET");
                assert!(request
                    .url()
                    .contains("calendar%2Bexample%40invalid.test/events/fs"));
                (404, json!({"error":"synthetic missing"}))
            }
            1 => {
                assert_eq!(request.method().as_str(), "POST");
                let value = body(request);
                assert_eq!(value["summary"], "Ejercicio 1 de ADDA");
                assert_eq!(value["start"]["dateTime"], "2026-10-01T08:00:00+00:00");
                assert!(value.get("attendees").is_none());
                *server_state.lock().unwrap() = Some(value.clone());
                (201, value)
            }
            _ => (200, server_state.lock().unwrap().clone().unwrap()),
        });
        let client = CalendarClient::test_client(target("google"), &base);
        let first = client
            .ensure_event("reviewed-session", 0, &event(0))
            .unwrap();
        let second = client
            .ensure_event("reviewed-session", 0, &event(0))
            .unwrap();
        assert_eq!(first, second);
        assert_eq!(first.len(), 66);
        worker.join().unwrap();
    }

    #[test]
    fn google_uncertain_commit_is_recovered_without_another_post() {
        let stored = Arc::new(Mutex::new(None::<Value>));
        let server_state = stored.clone();
        let (base, worker) = serve(3, move |request, index| match index {
            0 => (404, json!({})),
            1 => {
                *server_state.lock().unwrap() = Some(body(request));
                // Simulate provider committing and then returning a gateway error.
                (502, json!({"error":"synthetic lost response"}))
            }
            _ => (200, server_state.lock().unwrap().clone().unwrap()),
        });
        let client = CalendarClient::test_client(target("google"), &base);
        assert!(client
            .ensure_event("reviewed-session", 0, &event(0))
            .is_ok());
        worker.join().unwrap();
    }

    #[test]
    fn google_creation_conflict_recovers_only_an_unchanged_owned_marker() {
        for matching_owner in [true, false] {
            let fixture = CalendarClient::test_client(target("google"), "http://127.0.0.1:1");
            let mut existing = remote(&fixture, "reviewed-session", 0, &event(0));
            if !matching_owner {
                existing["extendedProperties"]["private"]["fsOwner"] = json!("different-owner");
            }
            let (base, worker) = serve(3, move |request, index| match index {
                0 => (404, json!({})),
                1 => {
                    assert_eq!(request.method().as_str(), "POST");
                    (409, json!({"error":"synthetic concurrent commit"}))
                }
                _ => (200, existing.clone()),
            });
            let client = CalendarClient::test_client(target("google"), &base);
            assert_eq!(
                client
                    .ensure_event("reviewed-session", 0, &event(0))
                    .is_ok(),
                matching_owner
            );
            worker.join().unwrap();
        }
    }

    #[test]
    fn google_partial_batch_resumes_completed_and_uncertain_blocks_without_duplicates() {
        let stored = Arc::new(Mutex::new(std::collections::HashMap::<String, Value>::new()));
        let server_state = stored.clone();
        let (base, worker) = serve(7, move |request, index| {
            let id = request.url().rsplit('/').next().unwrap().to_string();
            if request.method().as_str() == "POST" {
                let value = body(request);
                server_state
                    .lock()
                    .unwrap()
                    .insert(value["id"].as_str().unwrap().to_string(), value.clone());
                // The second block commits but both POST and recovery GET fail.
                return if index == 3 {
                    (503, json!({}))
                } else {
                    (201, value)
                };
            }
            if index == 4 {
                return (503, json!({}));
            }
            match server_state.lock().unwrap().get(&id).cloned() {
                Some(value) => (200, value),
                None => (404, json!({})),
            }
        });
        let client = CalendarClient::test_client(target("google"), &base);
        let first = client
            .ensure_event("reviewed-session", 0, &event(0))
            .unwrap();
        assert!(client
            .ensure_event("reviewed-session", 1, &event(1))
            .is_err());
        assert_eq!(
            client
                .ensure_event("reviewed-session", 0, &event(0))
                .unwrap(),
            first
        );
        assert!(client
            .ensure_event("reviewed-session", 1, &event(1))
            .is_ok());
        assert_eq!(stored.lock().unwrap().len(), 2);
        worker.join().unwrap();
    }

    #[test]
    fn google_collision_or_manually_edited_block_is_rejected_without_write() {
        for edit in ["owner", "title", "start", "session", "cancelled"] {
            let fixture = CalendarClient::test_client(target("google"), "http://127.0.0.1:1");
            let mut existing = remote(&fixture, "reviewed-session", 0, &event(0));
            match edit {
                "owner" => {
                    existing["extendedProperties"]["private"]["fsOwner"] = json!("someone-else")
                }
                "title" => existing["summary"] = json!("User edited title"),
                "start" => existing["start"]["dateTime"] = json!("2026-10-01T08:15:00Z"),
                "session" => {
                    existing["extendedProperties"]["private"]["fsSession"] =
                        json!("another-session")
                }
                _ => existing["status"] = json!("cancelled"),
            }
            let (base, worker) = serve(1, move |request, _| {
                assert_eq!(request.method().as_str(), "GET");
                (200, existing.clone())
            });
            let client = CalendarClient::test_client(target("google"), &base);
            assert!(client
                .ensure_event("reviewed-session", 0, &event(0))
                .is_err());
            worker.join().unwrap();
        }
    }

    #[test]
    fn forbidden_write_is_not_retried_and_does_not_reveal_token_or_payload() {
        let (base, worker) = serve(2, |request, index| {
            if index == 0 {
                (404, json!({}))
            } else {
                assert_eq!(request.method().as_str(), "POST");
                (
                    403,
                    json!({"secret":"synthetic-token","task":"private text"}),
                )
            }
        });
        let client = CalendarClient::test_client(target("google"), &base);
        let error = client
            .ensure_event("reviewed-session", 0, &event(0))
            .unwrap_err();
        assert!(error.contains("permit"));
        assert!(!error.contains("synthetic-token"));
        assert!(!error.contains("private text"));
        worker.join().unwrap();
    }

    #[test]
    fn google_busy_read_expands_recurrence_and_handles_all_day_dst_and_free_events() {
        let (base, worker) = serve(2, |request, index| {
            assert_eq!(request.method().as_str(), "GET");
            assert!(request.url().contains("singleEvents=true"));
            assert!(!request.url().contains("freeBusy"));
            if index == 0 {
                (
                    200,
                    json!({"timeZone":"Europe/Madrid","nextPageToken":"page two/+","items":[
                        {"id":"all-day","start":{"date":"2026-10-25"},"end":{"date":"2026-10-26"}},
                        {"id":"free","transparency":"transparent","start":{"date":"bad"}},
                        {"id":"deleted","status":"cancelled","start":{"date":"bad"}}
                    ]}),
                )
            } else {
                assert!(request.url().contains("pageToken=page+two%2F%2B"));
                (
                    200,
                    json!({"timeZone":"Europe/Madrid","items":[
                        {"id":"recurring-instance","start":{"dateTime":"2026-10-25T12:00:00+01:00"},"end":{"dateTime":"2026-10-25T13:00:00+01:00"}}
                    ]}),
                )
            }
        });
        let client = CalendarClient::test_client(target("google"), &base);
        let events = client
            .busy_events(
                time("2026-10-24T00:00:00Z").unwrap(),
                time("2026-10-27T00:00:00Z").unwrap(),
                None,
            )
            .unwrap();
        assert_eq!(events.len(), 2);
        assert_eq!(events[0].title, "Busy");
        assert_eq!(events[0].start_at, "2026-10-24T22:00:00+00:00");
        assert_eq!(events[0].end_at, "2026-10-25T23:00:00+00:00");
        assert_eq!(events[1].start_at, "2026-10-25T11:00:00+00:00");
        worker.join().unwrap();
    }

    #[test]
    fn busy_read_excludes_only_exact_owner_and_session_markers() {
        let fixture = CalendarClient::test_client(target("google"), "http://127.0.0.1:1");
        let own = remote(&fixture, "reviewed-session", 0, &event(0));
        let mut other_owner = own.clone();
        other_owner["id"] = json!("other-owner");
        other_owner["extendedProperties"]["private"]["fsOwner"] = json!("different-owner");
        let other_session = remote(&fixture, "different-session", 1, &event(1));
        let (base, worker) = serve(1, move |_, _| {
            (
                200,
                json!({"timeZone":"UTC","items":[own,other_owner,other_session]}),
            )
        });
        let client = CalendarClient::test_client(target("google"), &base);
        let events = client
            .busy_events(
                time("2026-10-01T00:00:00Z").unwrap(),
                time("2026-10-02T00:00:00Z").unwrap(),
                Some("reviewed-session"),
            )
            .unwrap();
        assert_eq!(events.len(), 2);
        worker.join().unwrap();
    }

    #[test]
    fn microsoft_uncertain_commit_is_found_by_marker_and_transaction() {
        let stored = Arc::new(Mutex::new(None::<Value>));
        let server_state = stored.clone();
        let (base, worker) = serve(4, move |request, index| match index {
            0 => {
                assert!(request.url().contains("%24filter="));
                (200, json!({"value":[]}))
            }
            1 => {
                let mut value = body(request);
                assert_eq!(value["start"]["timeZone"], "UTC");
                assert_eq!(value["transactionId"].as_str().unwrap().len(), 36);
                value["id"] = json!("graph-opaque-id");
                *server_state.lock().unwrap() = Some(value);
                (502, json!({}))
            }
            _ => (
                200,
                json!({"value":[server_state.lock().unwrap().clone().unwrap()]}),
            ),
        });
        let client = CalendarClient::test_client(target("microsoft"), &base);
        assert_eq!(
            client
                .ensure_event("reviewed-session", 0, &event(0))
                .unwrap(),
            "graph-opaque-id"
        );
        assert_eq!(
            client
                .ensure_event("reviewed-session", 0, &event(0))
                .unwrap(),
            "graph-opaque-id"
        );
        worker.join().unwrap();
    }

    #[test]
    fn microsoft_duplicate_marker_is_rejected_without_post() {
        let fixture = CalendarClient::test_client(target("microsoft"), "http://127.0.0.1:1");
        let value = remote(&fixture, "reviewed-session", 0, &event(0));
        let (base, worker) = serve(1, move |request, _| {
            assert_eq!(request.method().as_str(), "GET");
            (200, json!({"value":[value,value]}))
        });
        let client = CalendarClient::test_client(target("microsoft"), &base);
        assert!(client
            .ensure_event("reviewed-session", 0, &event(0))
            .unwrap_err()
            .contains("duplicate"));
        worker.join().unwrap();
    }

    #[test]
    fn microsoft_retry_busy_read_resolves_private_ids_using_events_endpoint() {
        let fixture = CalendarClient::test_client(target("microsoft"), "http://127.0.0.1:1");
        let own = remote(&fixture, "reviewed-session", 0, &event(0));
        let (base, worker) = serve(2, move |request, index| {
            if index == 0 {
                assert!(request.url().contains("/events?"));
                assert!(request.url().contains("%24filter="));
                assert!(request.url().contains("%24expand="));
                (
                    200,
                    json!({"value":[{"id":"own-block","singleValueExtendedProperties":own["singleValueExtendedProperties"]}]}),
                )
            } else {
                assert!(request.url().contains("/calendarView?"));
                assert!(!request.url().contains("%24expand="));
                (
                    200,
                    json!({"value":[
                        {"id":"own-block","start":{"dateTime":"2026-10-01T08:00:00","timeZone":"UTC"},"end":{"dateTime":"2026-10-01T08:45:00","timeZone":"UTC"}},
                        {"id":"unrelated","start":{"dateTime":"2026-10-01T09:00:00","timeZone":"UTC"},"end":{"dateTime":"2026-10-01T10:00:00","timeZone":"UTC"}},
                        {"id":"free","showAs":"free","start":{"dateTime":"invalid"}},
                        {"id":"cancelled","isCancelled":true,"start":{"dateTime":"invalid"}}
                    ]}),
                )
            }
        });
        let client = CalendarClient::test_client(target("microsoft"), &base);
        let events = client
            .busy_events(
                time("2026-10-01T00:00:00Z").unwrap(),
                time("2026-10-02T00:00:00Z").unwrap(),
                Some("reviewed-session"),
            )
            .unwrap();
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].external_id.as_deref(), Some("unrelated"));
        worker.join().unwrap();
    }

    #[test]
    fn microsoft_specific_block_expansion_is_sufficient_for_unchanged_recovery() {
        let fixture = CalendarClient::test_client(target("microsoft"), "http://127.0.0.1:1");
        let mut existing = remote(&fixture, "reviewed-session", 0, &event(0));
        existing["singleValueExtendedProperties"]
            .as_array_mut()
            .unwrap()
            .retain(|property| property["id"] == BLOCK_PROPERTY);
        let (base, worker) = serve(1, move |request, _| {
            assert!(request.url().contains("%24expand="));
            (200, json!({"value":[existing]}))
        });
        let client = CalendarClient::test_client(target("microsoft"), &base);
        assert_eq!(
            client
                .ensure_event("reviewed-session", 0, &event(0))
                .unwrap(),
            "remote-0"
        );
        worker.join().unwrap();
    }

    #[test]
    fn malformed_saved_event_lookup_never_causes_post() {
        for provider in ["google", "microsoft"] {
            let (base, worker) = serve(1, move |request, _| {
                assert_eq!(request.method().as_str(), "GET");
                (200, json!({"unexpected":"payload"}))
            });
            let client = CalendarClient::test_client(target(provider), &base);
            assert!(client
                .ensure_event("reviewed-session", 0, &event(0))
                .is_err());
            worker.join().unwrap();
        }
    }

    #[test]
    fn microsoft_busy_read_rejects_untrusted_pagination_before_sending_credentials() {
        for next in [
            "https://attacker.invalid/v1.0/me/calendarView",
            "https://graph.microsoft.com@attacker.invalid/v1.0/me/calendarView",
            "http://127.0.0.1:1/unrelated",
        ] {
            let (base, worker) = serve(1, move |_, _| {
                (200, json!({"value":[],"@odata.nextLink":next}))
            });
            let client = CalendarClient::test_client(target("microsoft"), &base);
            assert!(client
                .busy_events(
                    time("2026-10-01T00:00:00Z").unwrap(),
                    time("2026-10-02T00:00:00Z").unwrap(),
                    None
                )
                .unwrap_err()
                .contains("untrusted"));
            worker.join().unwrap();
        }
    }

    #[test]
    fn malformed_busy_times_stop_confirmation_instead_of_becoming_free_time() {
        for provider in ["google", "microsoft"] {
            let (base, worker) = serve(1, move |_, _| {
                (
                    200,
                    json!({"items":[{"id":"bad","start":{"dateTime":"tomorrow"},"end":{"dateTime":"later"}}],"value":[{"id":"bad","start":{"dateTime":"tomorrow","timeZone":"UTC"},"end":{"dateTime":"later","timeZone":"UTC"}}]}),
                )
            });
            let client = CalendarClient::test_client(target(provider), &base);
            assert!(client
                .busy_events(
                    time("2026-10-01T00:00:00Z").unwrap(),
                    time("2026-10-02T00:00:00Z").unwrap(),
                    None
                )
                .is_err());
            worker.join().unwrap();
        }
    }

    #[test]
    fn session_ids_are_bound_to_owner_provider_and_calendar_without_delimiter_collisions() {
        let original = CalendarClient::test_client(target("google"), "http://127.0.0.1:1");
        let marker = original.session_marker("a:b");
        for altered in [
            CalendarTarget {
                owner_user_id: "fictional-owner-b".into(),
                ..target("google")
            },
            CalendarTarget {
                calendar_id: "different-calendar".into(),
                ..target("google")
            },
            target("microsoft"),
        ] {
            let client = CalendarClient::test_client(altered, "http://127.0.0.1:1");
            assert_ne!(client.session_marker("a:b"), marker);
        }
        assert_ne!(digest(&["a:b", "c"]), digest(&["a", "b:c"]));
        assert_ne!(
            original.block_marker("a:b", 0, &event(0)),
            original.block_marker("a:b", 1, &event(0))
        );
        assert_ne!(
            original.block_marker("a:b", 0, &event(0)),
            original.block_marker("a:b", 0, &event(1))
        );
    }
}
