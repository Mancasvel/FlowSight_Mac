//! Local planning host. The model can draft a schedule; only confirmation saves it.
pub mod session_plan;
pub mod state;

use reqwest::blocking::Client;
use serde_json::Value;

fn parse_arguments(value: &Value) -> Result<Value, String> {
    match value {
        Value::String(raw) => serde_json::from_str(raw)
            .map_err(|_| "The local model returned malformed tool arguments.".to_string()),
        Value::Object(_) => Ok(value.clone()),
        _ => Err("The local model returned malformed tool arguments.".into()),
    }
}

fn send_model_request(client: &Client, url: &str, body: &Value) -> Result<Value, String> {
    let response = client
        .post(url)
        .json(body)
        .send()
        .map_err(|error| format!("Could not reach local Qwen: {error}"))?;
    if !response.status().is_success() {
        return Err(format!("Local Qwen returned HTTP {}.", response.status()));
    }
    response
        .json()
        .map_err(|_| "Local Qwen returned invalid JSON.".into())
}

#[tauri::command]
pub fn get_local_agent_data() -> Result<state::AgentData, String> {
    state::read()
}
