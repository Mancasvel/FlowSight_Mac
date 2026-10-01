use chrono::{DateTime, Utc};
use serde_json::{json, Value};

use super::connectors;
use super::state::LocalEvent;

fn graph_time(value: &Value) -> Result<DateTime<Utc>, String> {
    let date = value["dateTime"]
        .as_str()
        .ok_or("Microsoft returned an event without a time.")?;
    DateTime::parse_from_rfc3339(date)
        .or_else(|_| DateTime::parse_from_rfc3339(&format!("{date}Z")))
        .map(|value| value.with_timezone(&Utc))
        .map_err(|_| "Microsoft returned an invalid event time.".into())
}

pub fn busy_events(
    provider: &str,
    start: DateTime<Utc>,
    end: DateTime<Utc>,
) -> Result<Vec<LocalEvent>, String> {
    let token = crate::calendar_companion::access_token(provider)?;
    let client = connectors::client()?;
    let mut events = Vec::new();
    if provider == "google" {
        let response = client
            .post("https://www.googleapis.com/calendar/v3/freeBusy")
            .bearer_auth(token)
            .json(&json!({"timeMin":start.to_rfc3339(),"timeMax":end.to_rfc3339(),"items":[{"id":"primary"}]}))
            .send().map_err(|error| error.to_string())?;
        let body = connectors::checked_json(response, "Google Calendar")?;
        if let Some(error) = body["calendars"]["primary"]["errors"]
            .as_array()
            .and_then(|items| items.first())
        {
            return Err(format!("Google Calendar could not read free time: {error}"));
        }
        if !body["calendars"]["primary"]["busy"].is_array() {
            return Err("Google Calendar did not return availability.".into());
        }
        for (index, busy) in body["calendars"]["primary"]["busy"]
            .as_array()
            .into_iter()
            .flatten()
            .enumerate()
        {
            let Some(at) = busy["start"].as_str() else {
                continue;
            };
            let Some(to) = busy["end"].as_str() else {
                continue;
            };
            events.push(LocalEvent {
                id: format!("google-busy-{index}"),
                title: "Busy".into(),
                start_at: at.into(),
                end_at: to.into(),
                created_at: String::new(),
                updated_at: String::new(),
                provider: Some("google".into()),
                external_id: None,
            });
        }
    } else if provider == "microsoft" {
        let mut next = format!("https://graph.microsoft.com/v1.0/me/calendarView?startDateTime={}&endDateTime={}&$select=id,start,end,showAs,isCancelled&$top=1000", urlencoding::encode(&start.to_rfc3339()), urlencoding::encode(&end.to_rfc3339()));
        for _ in 0..10 {
            let response = client
                .get(&next)
                .bearer_auth(&token)
                .header("Prefer", "outlook.timezone=\"UTC\"")
                .send()
                .map_err(|error| error.to_string())?;
            let body = connectors::checked_json(response, "Microsoft Calendar")?;
            if !body["value"].is_array() {
                return Err("Microsoft Calendar did not return events.".into());
            }
            for (index, event) in body["value"].as_array().into_iter().flatten().enumerate() {
                if event["isCancelled"] == true || event["showAs"] == "free" {
                    continue;
                }
                events.push(LocalEvent {
                    id: event["id"].as_str().unwrap_or("").to_string(),
                    title: "Busy".into(),
                    start_at: graph_time(&event["start"])?.to_rfc3339(),
                    end_at: graph_time(&event["end"])?.to_rfc3339(),
                    created_at: String::new(),
                    updated_at: String::new(),
                    provider: Some("microsoft".into()),
                    external_id: Some(format!("{index}")),
                });
            }
            let Some(url) = body["@odata.nextLink"].as_str() else {
                return Ok(events);
            };
            let parsed = url::Url::parse(url)
                .map_err(|_| "Microsoft returned an invalid page link.".to_string())?;
            if parsed.scheme() != "https" || parsed.host_str() != Some("graph.microsoft.com") {
                return Err("Microsoft returned an untrusted page link.".into());
            }
            next = url.to_string();
        }
        return Err(
            "Microsoft Calendar returned too many pages to establish availability safely.".into(),
        );
    } else {
        return Err("Unsupported calendar provider.".into());
    }
    Ok(events)
}

pub fn create_event(
    provider: &str,
    title: &str,
    start: DateTime<Utc>,
    end: DateTime<Utc>,
) -> Result<String, String> {
    let token = connectors::credential(provider)?;
    let client = connectors::client()?;
    let response = if provider == "google" {
        client.post("https://www.googleapis.com/calendar/v3/calendars/primary/events")
            .bearer_auth(token).json(&json!({"summary":title,"start":{"dateTime":start.to_rfc3339()},"end":{"dateTime":end.to_rfc3339()}}))
            .send().map_err(|error| error.to_string())?
    } else if provider == "microsoft" {
        client.post("https://graph.microsoft.com/v1.0/me/events")
            .bearer_auth(token).json(&json!({"subject":title,"start":{"dateTime":start.format("%Y-%m-%dT%H:%M:%S").to_string(),"timeZone":"UTC"},"end":{"dateTime":end.format("%Y-%m-%dT%H:%M:%S").to_string(),"timeZone":"UTC"}}))
            .send().map_err(|error| error.to_string())?
    } else {
        return Err("Unsupported calendar provider.".into());
    };
    let body = connectors::checked_json(response, provider)?;
    body["id"]
        .as_str()
        .map(str::to_string)
        .ok_or("Calendar did not return an event ID.".into())
}

pub fn delete_event(provider: &str, external_id: &str) -> Result<(), String> {
    let token = connectors::credential(provider)?;
    let encoded = urlencoding::encode(external_id);
    let url = match provider {
        "google" => {
            format!("https://www.googleapis.com/calendar/v3/calendars/primary/events/{encoded}")
        }
        "microsoft" => format!("https://graph.microsoft.com/v1.0/me/events/{encoded}"),
        _ => return Err("Unsupported calendar provider.".into()),
    };
    let response = connectors::client()?
        .delete(url)
        .bearer_auth(token)
        .send()
        .map_err(|error| error.to_string())?;
    if response.status().is_success() {
        Ok(())
    } else {
        Err(format!(
            "{provider} returned HTTP {} while removing the event.",
            response.status()
        ))
    }
}

pub fn move_event(
    provider: &str,
    external_id: &str,
    start: DateTime<Utc>,
    end: DateTime<Utc>,
) -> Result<(), String> {
    let token = connectors::credential(provider)?;
    let client = connectors::client()?;
    let encoded = urlencoding::encode(external_id);
    let response = if provider == "google" {
        client.patch(format!("https://www.googleapis.com/calendar/v3/calendars/primary/events/{encoded}"))
            .bearer_auth(token).json(&json!({"start":{"dateTime":start.to_rfc3339()},"end":{"dateTime":end.to_rfc3339()}}))
            .send().map_err(|error| error.to_string())?
    } else if provider == "microsoft" {
        client.patch(format!("https://graph.microsoft.com/v1.0/me/events/{encoded}"))
            .bearer_auth(token).json(&json!({"start":{"dateTime":start.format("%Y-%m-%dT%H:%M:%S").to_string(),"timeZone":"UTC"},"end":{"dateTime":end.format("%Y-%m-%dT%H:%M:%S").to_string(),"timeZone":"UTC"}}))
            .send().map_err(|error| error.to_string())?
    } else {
        return Err("Unsupported calendar provider.".into());
    };
    if response.status().is_success() {
        Ok(())
    } else {
        Err(format!(
            "{provider} returned HTTP {} while moving the event.",
            response.status()
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn graph_times_with_and_without_offsets_are_read_correctly() {
        assert_eq!(
            graph_time(&json!({"dateTime":"2026-10-01T09:00:00-04:00"}))
                .unwrap()
                .to_rfc3339(),
            "2026-10-01T13:00:00+00:00"
        );
        assert_eq!(
            graph_time(&json!({"dateTime":"2026-10-01T09:00:00"}))
                .unwrap()
                .to_rfc3339(),
            "2026-10-01T09:00:00+00:00"
        );
        assert!(graph_time(&json!({"dateTime":"tomorrow"})).is_err());
    }
}
