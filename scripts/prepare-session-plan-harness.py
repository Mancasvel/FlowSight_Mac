"""Compile the planner's actual pure code and request builder without launching Tauri.

The adapter never reads the personal database. Files generated under review/ are
development evidence, not an alternate planner implementation.
"""
import pathlib
import sys

root = pathlib.Path(__file__).resolve().parents[1]
output = root / '.impeccable/review/suggestions-harness'
source = pathlib.Path(sys.argv[1]) if len(sys.argv) > 1 else root / 'apps/agent/src-tauri/src/local_agent/session_plan.rs'
session = source.read_text(encoding='utf8')
state = (root / 'apps/agent/src-tauri/src/local_agent/state.rs').read_text(encoding='utf8')
output.joinpath('src').mkdir(parents=True, exist_ok=True)
pure = session[session.index('const LIFETIME:'):session.index('\nfn planning_context')]
model = session[session.index('fn model_request('):session.index('\n#[tauri::command]\npub async fn propose_session_plan')]
add = session[session.index('fn reviewed_events('):session.index('\nfn save_session(')]
tests = session[session.index('#[cfg(test)]\nmod tests'):]
abandon = session[session.index('#[derive(Serialize)]\n#[serde(rename_all = "camelCase")]\npub struct SessionAbandonment'):session.index('\n#[tauri::command]\npub async fn abandon_session_plan')]
structs = state[state.index('#[derive(Clone, Debug, Default'):state.index('\nfn connection()')]
structs = structs.replace('super::session_calendar::CalendarTarget', 'crate::local_agent::session_calendar::CalendarTarget')
structs = structs.replace('super::total_focus::', 'crate::total_focus::')
focus_source = (root / 'apps/agent/src-tauri/src/local_agent/total_focus.rs').read_text(encoding='utf8')
focus_types = focus_source[focus_source.index('#[derive(Clone, Debug, Serialize, Deserialize)]'):focus_source.index('\nfn normalize(')]
calendar = (root / 'apps/agent/src-tauri/src/local_agent/session_calendar.rs').read_text(encoding='utf8')
target = calendar[calendar.index('#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]'):calendar.index('\npub struct CalendarClient')]
output.joinpath('Cargo.toml').write_text('''[package]
name = "flowsight-suggestions-harness"
version = "0.1.0"
edition = "2021"
[dependencies]
chrono = "0.4"
serde = { version = "1.0", features = ["derive"] }
serde_json = "1.0"
uuid = { version = "1", features = ["v4"] }
reqwest = { version = "0.12", features = ["json", "blocking"] }
''', encoding='utf8')
output.joinpath('src/state.rs').write_text('use std::collections::BTreeMap;\nuse serde::{Deserialize, Serialize};\n' + structs, encoding='utf8')
output.joinpath('src/total_focus.rs').write_text('use serde::{Deserialize, Serialize};\n' + focus_types, encoding='utf8')
output.joinpath('src/planner.rs').write_text('''use std::sync::Mutex;
use std::time::{Duration, Instant};
use chrono::{DateTime, Duration as TimeDelta, FixedOffset, Utc};
use reqwest::blocking::Client;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use crate::state::{ActionAudit, AgentData, LocalEvent};
use crate::state::SessionSave;
use crate::local_agent::session_calendar::CalendarTarget;
''' + pure + '\n' + model + '\n' + add + '\n' + abandon + '''
pub fn evaluate(input: &Value) -> Value {
    let request: SessionRequest = serde_json::from_value(input["request"].clone()).unwrap();
    let mut data = AgentData::default();
    if let Some(events) = input.get("events") { data.events = serde_json::from_value(events.clone()).unwrap(); }
    let context = json!({"session": request, "availableMinutes":window(&request).unwrap().1,
      "localCalendar": data.events,"openTasks":[],"savedPreferences":[],"observedTaskTime":input.get("observedTaskTime").cloned().unwrap_or(json!([])),"profile":""});
    let previous = input.get("previous").filter(|value| !value.is_null()).map(|value| SessionProposal {
      id: value["id"].as_str().unwrap_or("previous").into(), summary: value["summary"].as_str().unwrap().into(),
      localized_summary: value["localizedSummary"].clone(), localized_unscheduled: value["localizedUnscheduled"].clone(),
      calendar_destination: None,
      blocks:serde_json::from_value(value["blocks"].clone()).unwrap(),
      unscheduled: serde_json::from_value(value["unscheduled"].clone()).unwrap(), expires_in_seconds:1800,
    });
    let feedback = input["feedback"].as_str().unwrap_or("");
    let value = if let Some(plan) = input.get("plan") { plan.clone() } else {
      let client = Client::builder().no_proxy().timeout(Duration::from_secs(180)).build().unwrap();
      let url = std::env::var("FLOWSIGHT_PLAN_SMOKE_URL").expect("local model URL");
      let response = crate::local_agent::send_model_request(&client, &url, &model_request(&context, previous.as_ref(), feedback)).unwrap();
      let calls = response["choices"][0]["message"]["tool_calls"].as_array().expect("planning function call");
      assert_eq!(calls.len(), 1);
      assert_eq!(calls[0]["function"]["name"], "propose_session_blocks");
      crate::local_agent::parse_arguments(&calls[0]["function"]["arguments"]).unwrap()
    };
    match decode_plan_with_context(value.clone(), &request, &data, feedback, Some(&context), previous.as_ref()) {
      Err(error) => json!({"accepted":false,"error":error,"model":value}),
      Ok(proposal) => json!({"accepted":true,"model":value,"proposal":proposal,"calendarUntouched":data.events.len()==input["events"].as_array().map_or(0,Vec::len)}),
    }
}
''' + '\n' + tests, encoding='utf8')
output.joinpath('src/main.rs').write_text('''#![allow(dead_code, unused_imports)]
mod state;
mod total_focus;
mod language {
 pub fn copy<'a>(english:&'a str,spanish:&'a str)->&'a str { if std::env::var("FLOWSIGHT_TEST_LANGUAGE").as_deref()==Ok("es") {spanish} else {english} }
}
mod vision_model { pub const LLAMA_CHAT_MODEL_ID: &str = "flowsight-qwen3vl-2b-instruct"; }
mod local_agent {
 pub mod session_calendar {
  use serde::{Serialize, Deserialize};
''' + target + '''
 }
 use serde_json::Value;
 use reqwest::blocking::Client;
 #[path = "../planner.rs"] pub mod session_plan;
 pub fn parse_arguments(value:&Value)->Result<Value,String> { if let Some(text)=value.as_str() {serde_json::from_str(text).map_err(|e|e.to_string())} else {Ok(value.clone())} }
 pub fn send_model_request(client:&Client,url:&str,body:&Value)->Result<Value,String> {
  let response=client.post(url).json(body).send().map_err(|e|e.to_string())?;
  if !response.status().is_success() {return Err(format!("HTTP {}: {}",response.status(),response.text().unwrap_or_default()));}
  response.json().map_err(|e|e.to_string())
 }
}
use std::io::{self,BufRead};
fn main() {
 for line in io::stdin().lock().lines() {
  let input:serde_json::Value=serde_json::from_str(&line.unwrap()).unwrap();
  println!("{}",local_agent::session_plan::evaluate(&input));
 }
}
''', encoding='utf8')
output.joinpath('src/local_agent').mkdir(exist_ok=True)
print(output)
