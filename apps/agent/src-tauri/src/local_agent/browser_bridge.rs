//! Loopback bridge for the optional Chromium extension (including Arc). Only a bearer token
//! shown inside FlowSight can enqueue or complete browser commands.

use std::collections::{HashMap, VecDeque};
use std::io::Read;
use std::sync::{mpsc, Arc, Mutex, OnceLock};
use std::time::{Duration, Instant};

use serde_json::{json, Value};
use tauri::Manager;
use tiny_http::{Header, Method, Response, Server, StatusCode};
use url::Url;

const MAX_RESULT_BYTES: usize = 128 * 1024;
const PAIRING_TIMEOUT: Duration = Duration::from_secs(75);
const BRIDGE_PORT: u16 = 38547;
const TOKEN_KEY: &str = "local_agent_browser_pairing_v1";
static BRIDGE: OnceLock<Arc<Bridge>> = OnceLock::new();

struct BrowserCommand {
    id: String,
    name: String,
    arguments: Value,
}

#[derive(Default)]
struct Queue {
    commands: VecDeque<BrowserCommand>,
    waiters: HashMap<String, mpsc::Sender<Result<Value, String>>>,
    last_seen: Option<Instant>,
    focus_status: Value,
    focus_seen: Option<Instant>,
}

struct Bridge {
    port: u16,
    token: String,
    queue: Mutex<Queue>,
}

fn json_response(value: Value, code: u16) -> Response<std::io::Cursor<Vec<u8>>> {
    let body = serde_json::to_vec(&value).unwrap_or_default();
    let mut response = Response::from_data(body).with_status_code(StatusCode(code));
    response.add_header(Header::from_bytes("Content-Type", "application/json").unwrap());
    response.add_header(Header::from_bytes("Access-Control-Allow-Origin", "*").unwrap());
    response.add_header(
        Header::from_bytes(
            "Access-Control-Allow-Headers",
            "X-FlowSight-Token, Content-Type",
        )
        .unwrap(),
    );
    response.add_header(
        Header::from_bytes("Access-Control-Allow-Methods", "GET, POST, OPTIONS").unwrap(),
    );
    response
}

pub fn start() -> Result<(), String> {
    if BRIDGE.get().is_some() {
        return Ok(());
    }
    let server = Server::http(format!("127.0.0.1:{BRIDGE_PORT}")).map_err(|error| {
        format!("Could not bind browser bridge to local port {BRIDGE_PORT}: {error}")
    })?;
    let port = server
        .server_addr()
        .to_ip()
        .ok_or("The browser bridge did not bind a TCP address.")?
        .port();
    let conn =
        rusqlite::Connection::open(crate::paths::db_path()?).map_err(|error| error.to_string())?;
    conn.execute(
        "CREATE TABLE IF NOT EXISTS config (key TEXT PRIMARY KEY, value TEXT)",
        [],
    )
    .map_err(|error| error.to_string())?;
    let token = match crate::secure_config::load_secret(&conn, TOKEN_KEY)? {
        Some(token) => token,
        None => {
            let token = format!("{}{}", uuid::Uuid::new_v4(), uuid::Uuid::new_v4());
            crate::secure_config::save_secret(&conn, TOKEN_KEY, &token)?;
            token
        }
    };
    let bridge = Arc::new(Bridge {
        port,
        token,
        queue: Mutex::new(Queue {
            commands: VecDeque::from([BrowserCommand {
                id: uuid::Uuid::new_v4().to_string(),
                name: "browser.unblock_all".into(),
                arguments: json!({}),
            }]),
            ..Queue::default()
        }),
    });
    BRIDGE
        .set(bridge.clone())
        .map_err(|_| "The browser bridge was already started.".to_string())?;
    std::thread::spawn(move || {
        for mut request in server.incoming_requests() {
            let method = request.method().clone();
            if method == Method::Options {
                let _ = request.respond(json_response(json!({}), 204));
                continue;
            }
            let valid_token = request
                .headers()
                .iter()
                .find(|header| header.field.equiv("X-FlowSight-Token"))
                .is_some_and(|header| header.value.as_str() == bridge.token);
            if !valid_token {
                let _ = request.respond(json_response(json!({"error":"Unauthorized"}), 401));
                continue;
            }
            let path = request.url().to_string();
            let result = match (method, path.as_str()) {
                (Method::Get, "/next") => {
                    let focus = super::total_focus::policy().ok().flatten();
                    let mut queue = bridge.queue.lock().unwrap();
                    queue.last_seen = Some(Instant::now());
                    let command = queue.commands.pop_front();
                    json_response(
                        command.map_or_else(
                            || json!({"command":null,"focus":focus}),
                            |item| json!({"command":{"id":item.id,"name":item.name,"arguments":item.arguments},"focus":focus}),
                        ),
                        200,
                    )
                }
                (Method::Post, "/focus_status") => {
                    let mut body = Vec::new();
                    let read = request.as_reader().take(4097).read_to_end(&mut body);
                    if read.is_err() || body.len() > 4096 {
                        json_response(json!({"error":"Status too large"}), 413)
                    } else if let Ok(value) = serde_json::from_slice::<Value>(&body) {
                        if let Some(id) = value["cancelledSessionId"].as_str() {
                            let _ = super::total_focus::cancel_from_extension(id);
                        }
                        let mut queue = bridge.queue.lock().unwrap();
                        queue.focus_status = json!({"sessionId":value["sessionId"],"applied":value["applied"],"extensionVersion":value["extensionVersion"]});
                        queue.focus_seen = Some(Instant::now());
                        json_response(json!({"received":true}), 200)
                    } else {
                        json_response(json!({"error":"Invalid JSON"}), 400)
                    }
                }
                (Method::Post, "/result") => {
                    let mut body = Vec::new();
                    let read = request
                        .as_reader()
                        .take(MAX_RESULT_BYTES as u64 + 1)
                        .read_to_end(&mut body);
                    if read.is_err() || body.len() > MAX_RESULT_BYTES {
                        json_response(json!({"error":"Result too large"}), 413)
                    } else if let Ok(result) = serde_json::from_slice::<Value>(&body) {
                        let id = result["id"].as_str().unwrap_or("");
                        let waiter = bridge.queue.lock().unwrap().waiters.remove(id);
                        if let Some(waiter) = waiter {
                            let value = if result["ok"] == true {
                                Ok(result["result"].clone())
                            } else {
                                Err(result["error"]
                                    .as_str()
                                    .unwrap_or("Browser action failed.")
                                    .to_string())
                            };
                            let _ = waiter.send(value);
                            json_response(json!({"received":true}), 200)
                        } else {
                            json_response(json!({"error":"Unknown command"}), 404)
                        }
                    } else {
                        json_response(json!({"error":"Invalid JSON"}), 400)
                    }
                }
                _ => json_response(json!({"error":"Unknown route"}), 404),
            };
            let _ = request.respond(result);
        }
    });
    Ok(())
}

fn total_focus_available(queue: &Queue) -> bool {
    queue
        .focus_seen
        .is_some_and(|seen| seen.elapsed() < PAIRING_TIMEOUT)
        && queue.focus_status["applied"].is_boolean()
}

pub fn focus_status() -> Value {
    BRIDGE.get().and_then(|bridge| bridge.queue.lock().ok().map(|queue| {
        json!({"connected":queue.last_seen.is_some_and(|seen| seen.elapsed() < PAIRING_TIMEOUT),
            "fresh":queue.focus_seen.is_some_and(|seen| seen.elapsed() < PAIRING_TIMEOUT),
            "totalFocusAvailable":total_focus_available(&queue),"extensionVersion":queue.focus_status["extensionVersion"],
            "sessionId":queue.focus_status["sessionId"],"applied":queue.focus_status["applied"] == true})
    })).unwrap_or_else(|| json!({"connected":false,"fresh":false,"applied":false}))
}

#[tauri::command]
pub fn get_browser_pairing() -> Result<Value, String> {
    let bridge = BRIDGE.get().ok_or("Browser bridge unavailable.")?;
    let queue = bridge.queue.lock().map_err(|error| error.to_string())?;
    Ok(json!({
        "port": bridge.port,
        "token": bridge.token,
        "connected": queue.last_seen.is_some_and(|seen| seen.elapsed() < PAIRING_TIMEOUT),
        "totalFocusAvailable": total_focus_available(&queue),
        "extensionVersion": queue.focus_status["extensionVersion"],
        "chromeStoreAvailable": browser_store_url("chrome").is_some(),
        "edgeStoreAvailable": browser_store_url("edge").is_some(),
    }))
}

fn browser_store_url(browser: &str) -> Option<&'static str> {
    let (raw, expected_host, path_prefix) = match browser {
        "chrome" => (
            option_env!("FLOWSIGHT_CHROME_EXTENSION_STORE_URL"),
            "chromewebstore.google.com",
            "/detail/",
        ),
        "edge" => (
            option_env!("FLOWSIGHT_EDGE_EXTENSION_STORE_URL"),
            "microsoftedge.microsoft.com",
            "/addons/detail/",
        ),
        _ => return None,
    };
    let raw = raw?;
    valid_store_url(raw, expected_host, path_prefix).then_some(raw)
}

fn valid_store_url(raw: &str, expected_host: &str, path_prefix: &str) -> bool {
    let Ok(parsed) = Url::parse(raw) else {
        return false;
    };
    parsed.scheme() == "https"
        && parsed.host_str() == Some(expected_host)
        && parsed.username().is_empty()
        && parsed.password().is_none()
        && parsed.path().starts_with(path_prefix)
        && parsed.path().len() > path_prefix.len()
        && parsed.query().is_none()
        && parsed.fragment().is_none()
}

#[tauri::command]
pub fn open_browser_extension_store(browser: String) -> Result<(), String> {
    let url = browser_store_url(&browser)
        .ok_or("The official browser extension listing is not available in this build yet.")?;
    open::that(url).map_err(|error| format!("Could not open the browser extension store: {error}"))
}

#[cfg(test)]
mod store_url_tests {
    use super::{total_focus_available, valid_store_url, Queue, PAIRING_TIMEOUT};
    use serde_json::json;
    use std::time::Instant;

    #[test]
    fn connection_requires_a_recent_valid_focus_acknowledgement() {
        let mut queue = Queue {
            last_seen: Some(Instant::now()),
            ..Queue::default()
        };
        assert!(
            !total_focus_available(&queue),
            "The 1.0.0 heartbeat cannot enable total focus"
        );
        queue.focus_seen = Some(Instant::now());
        queue.focus_status = json!({"applied": false});
        assert!(
            total_focus_available(&queue),
            "1.1.0 remains compatible without a version field"
        );
        queue.focus_status = json!({"applied": "false"});
        assert!(!total_focus_available(&queue));
        queue.focus_status = json!({"applied": true, "extensionVersion": "1.1.1"});
        queue.focus_seen = Some(Instant::now() - PAIRING_TIMEOUT);
        assert!(
            !total_focus_available(&queue),
            "An expired acknowledgement is not readiness"
        );
    }

    #[test]
    fn accepts_only_canonical_official_listing_urls() {
        assert!(valid_store_url(
            "https://chromewebstore.google.com/detail/flowsight/abcdefghijklmnopabcdefghijklmnop",
            "chromewebstore.google.com",
            "/detail/"
        ));
        assert!(!valid_store_url(
            "https://chromewebstore.google.com.evil.example/detail/flowsight/id",
            "chromewebstore.google.com",
            "/detail/"
        ));
        assert!(!valid_store_url(
            "http://chromewebstore.google.com/detail/flowsight/id",
            "chromewebstore.google.com",
            "/detail/"
        ));
        assert!(!valid_store_url(
            "https://chromewebstore.google.com/",
            "chromewebstore.google.com",
            "/detail/"
        ));
    }
}

pub fn queue_unblock_all() {
    if let Some(bridge) = BRIDGE.get() {
        if let Ok(mut queue) = bridge.queue.lock() {
            queue.commands.clear();
            for (_, waiter) in queue.waiters.drain() {
                let _ = waiter.send(Err("Local FlowSight data was deleted.".into()));
            }
            queue.commands.push_back(BrowserCommand {
                id: uuid::Uuid::new_v4().to_string(),
                name: "browser.unblock_all".into(),
                arguments: json!({}),
            });
        }
    }
}

#[tauri::command]
pub fn open_browser_extension_folder(app: tauri::AppHandle) -> Result<(), String> {
    let bundled = app
        .path()
        .resolve("browser-extension", tauri::path::BaseDirectory::Resource)
        .map_err(|error| error.to_string())?;
    let source = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../browser-extension");
    let path = if bundled.exists() { bundled } else { source };
    if !path.join("manifest.json").exists() {
        return Err("Browser extension files were not found.".into());
    }
    open::that(path).map_err(|error| format!("Could not open browser extension folder: {error}"))
}

pub fn execute(name: &str, arguments: &Value) -> Result<Value, String> {
    let bridge = BRIDGE.get().ok_or("Browser bridge unavailable.")?;
    let (sender, receiver) = mpsc::channel();
    let id = uuid::Uuid::new_v4().to_string();
    {
        let mut queue = bridge.queue.lock().map_err(|error| error.to_string())?;
        if !queue
            .last_seen
            .is_some_and(|seen| seen.elapsed() < PAIRING_TIMEOUT)
        {
            return Err(
                "Pair the FlowSight browser extension and leave the browser running first.".into(),
            );
        }
        queue.waiters.insert(id.clone(), sender);
        queue.commands.push_back(BrowserCommand {
            id: id.clone(),
            name: name.to_string(),
            arguments: arguments.clone(),
        });
    }
    let result = receiver.recv_timeout(Duration::from_secs(90));
    let mut queue = bridge.queue.lock().map_err(|error| error.to_string())?;
    queue.waiters.remove(&id);
    queue.commands.retain(|command| command.id != id);
    result.map_err(|_| "The browser extension did not answer within 90 seconds.".to_string())?
}
