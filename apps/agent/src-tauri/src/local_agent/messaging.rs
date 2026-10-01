use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine as _};
use serde_json::{json, Value};

use super::connectors;
use super::state::MessageDraft;

fn validate_mail_header(value: &str) -> Result<(), String> {
    if value.contains('\r') || value.contains('\n') {
        return Err("Email headers cannot contain line breaks.".into());
    }
    Ok(())
}

pub fn preflight(draft: &MessageDraft, email_provider: Option<&str>) -> Result<(), String> {
    if draft.channel == "email" {
        validate_mail_header(&draft.recipient)?;
        validate_mail_header(draft.subject.as_deref().unwrap_or("FlowSight message"))?;
        if !draft.recipient.contains('@') || draft.recipient.contains(' ') {
            return Err("The email draft needs a valid recipient address.".into());
        }
        let provider =
            email_provider.ok_or("Choose an email provider in Local agent settings first.")?;
        if provider != "google" && provider != "microsoft" {
            return Err("Choose Google or Microsoft for email.".into());
        }
        connectors::credential(provider)?;
    } else if draft.channel == "slack" || draft.channel == "teams" {
        connectors::credential(&draft.channel)?;
    } else {
        return Err("Unsupported message channel.".into());
    }
    Ok(())
}

pub fn send(draft: &MessageDraft, email_provider: Option<&str>) -> Result<Value, String> {
    preflight(draft, email_provider)?;
    let client = connectors::client()?;
    match draft.channel.as_str() {
        "email" => {
            validate_mail_header(&draft.recipient)?;
            let subject = draft.subject.as_deref().unwrap_or("FlowSight message");
            validate_mail_header(subject)?;
            if !draft.recipient.contains('@') || draft.recipient.contains(' ') {
                return Err("The email draft needs a valid recipient address.".into());
            }
            let provider =
                email_provider.ok_or("Choose an email provider in Local agent settings first.")?;
            let token = connectors::credential(provider)?;
            if provider == "google" {
                let raw = format!("To: {}\r\nSubject: {}\r\nMIME-Version: 1.0\r\nContent-Type: text/plain; charset=UTF-8\r\n\r\n{}", draft.recipient, subject, draft.body);
                let response = client
                    .post("https://gmail.googleapis.com/gmail/v1/users/me/messages/send")
                    .bearer_auth(token)
                    .json(&json!({"raw":URL_SAFE_NO_PAD.encode(raw.as_bytes())}))
                    .send()
                    .map_err(|error| format!("Gmail request failed: {error}"))?;
                let body = connectors::checked_json(response, "Gmail")?;
                let id = body["id"]
                    .as_str()
                    .ok_or("Gmail did not return a message ID.")?;
                Ok(
                    json!({"sent":true,"provider":"google","messageId":id,"recipient":draft.recipient}),
                )
            } else if provider == "microsoft" {
                let response = client.post("https://graph.microsoft.com/v1.0/me/sendMail")
                    .bearer_auth(token)
                    .json(&json!({"message":{"subject":subject,"body":{"contentType":"Text","content":draft.body},"toRecipients":[{"emailAddress":{"address":draft.recipient}}]},"saveToSentItems":true}))
                    .send().map_err(|error| format!("Microsoft Mail request failed: {error}"))?;
                if response.status().as_u16() != 202 {
                    return Err(format!(
                        "Microsoft Mail returned HTTP {}.",
                        response.status()
                    ));
                }
                Ok(json!({"sent":true,"provider":"microsoft","recipient":draft.recipient}))
            } else {
                Err("Unsupported email provider.".into())
            }
        }
        "slack" => {
            let token = connectors::credential("slack")?;
            let mut channel = draft.recipient.clone();
            if channel.starts_with('U') {
                let response = client
                    .post("https://slack.com/api/conversations.open")
                    .bearer_auth(&token)
                    .json(&json!({"users":channel}))
                    .send()
                    .map_err(|error| format!("Slack request failed: {error}"))?;
                let body = connectors::checked_json(response, "Slack")?;
                if body["ok"] != true {
                    return Err(format!(
                        "Slack could not open a conversation: {}",
                        body["error"].as_str().unwrap_or("unknown error")
                    ));
                }
                channel = body["channel"]["id"]
                    .as_str()
                    .ok_or("Slack did not return a conversation ID.")?
                    .to_string();
            }
            if !channel.starts_with('C') && !channel.starts_with('G') && !channel.starts_with('D') {
                return Err("Use a Slack channel ID (C/G/D) or user ID (U).".into());
            }
            let response = client
                .post("https://slack.com/api/chat.postMessage")
                .bearer_auth(token)
                .json(&json!({"channel":channel,"text":draft.body,"client_msg_id":draft.id}))
                .send()
                .map_err(|error| format!("Slack send request failed: {error}"))?;
            let body = connectors::checked_json(response, "Slack")?;
            if body["ok"] != true {
                return Err(format!(
                    "Slack did not send the message: {}",
                    body["error"].as_str().unwrap_or("unknown error")
                ));
            }
            Ok(
                json!({"sent":true,"provider":"slack","recipient":draft.recipient,"messageId":body["ts"]}),
            )
        }
        "teams" => {
            let token = connectors::credential("teams")?;
            let chat = urlencoding::encode(&draft.recipient);
            let response = client
                .post(format!(
                    "https://graph.microsoft.com/v1.0/chats/{chat}/messages"
                ))
                .bearer_auth(token)
                .json(&json!({"body":{"contentType":"text","content":draft.body}}))
                .send()
                .map_err(|error| format!("Teams request failed: {error}"))?;
            let body = connectors::checked_json(response, "Teams")?;
            let id = body["id"]
                .as_str()
                .ok_or("Teams did not return a message ID.")?;
            Ok(json!({"sent":true,"provider":"teams","recipient":draft.recipient,"messageId":id}))
        }
        _ => Err("Unsupported message channel.".into()),
    }
}
