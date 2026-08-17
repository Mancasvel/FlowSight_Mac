use serde::{Deserialize, Serialize};
use reqwest::blocking::Client;
use crate::sync_env::{supabase_anon_key, supabase_url};
use oauth2::{
    basic::BasicClient, AuthUrl, ClientId, RedirectUrl, TokenUrl,
    PkceCodeChallenge, CsrfToken, Scope, AuthorizationCode, TokenResponse
};
use oauth2::reqwest::http_client;
use url::Url;
use tiny_http::{Server, Response};
use rusqlite::Connection;
use std::sync::{Mutex, OnceLock};
use base64::Engine;
use std::io::Write;

static OAUTH_STATE: OnceLock<Mutex<OAuthState>> = OnceLock::new();

fn auth_log(message: impl AsRef<str>) {
    let message = message.as_ref();
    let line = format!(
        "[{}] {}\n",
        chrono::Local::now().format("%Y-%m-%d %H:%M:%S"),
        message
    );

    log::info!("{}", message);

    if let Ok(path) = crate::paths::auth_log_path() {
        if let Ok(mut file) = std::fs::OpenOptions::new().create(true).append(true).open(path) {
            let _ = file.write_all(line.as_bytes());
        }
    }
}

fn panic_payload_to_string(payload: Box<dyn std::any::Any + Send>) -> String {
    if let Some(s) = payload.downcast_ref::<&str>() {
        s.to_string()
    } else if let Some(s) = payload.downcast_ref::<String>() {
        s.clone()
    } else {
        "<non-string panic>".to_string()
    }
}

fn html_escape(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&#39;")
}

fn supabase_login_error_html(error: &str, description: &str) -> String {
    let error = html_escape(error);
    let description = html_escape(description);
    let auth_log_hint = crate::paths::auth_log_path()
        .map(|p| html_escape(&p.to_string_lossy()))
        .unwrap_or_else(|_| html_escape("auth.log under FlowSight's local app data folder"));
    format!(
        r#"<!DOCTYPE html>
<html lang="en">
<head>
  <meta charset="UTF-8">
  <meta name="viewport" content="width=device-width, initial-scale=1.0">
  <title>FlowSight - Login Failed</title>
  <style>
    body {{
      font-family: -apple-system, BlinkMacSystemFont, 'Segoe UI', Roboto, Arial, sans-serif;
      background: #0a0a0a;
      color: #fafafa;
      min-height: 100vh;
      display: flex;
      align-items: center;
      justify-content: center;
      margin: 0;
    }}
    .container {{
      max-width: 520px;
      padding: 32px;
      text-align: center;
    }}
    .error {{
      color: #ef4444;
      font-weight: 700;
      margin-bottom: 12px;
    }}
    .details {{
      background: rgba(255,255,255,0.06);
      border: 1px solid rgba(255,255,255,0.12);
      border-radius: 12px;
      padding: 16px;
      text-align: left;
      color: #d4d4d8;
      word-break: break-word;
      font-size: 13px;
      line-height: 1.5;
    }}
    .hint {{
      color: #a1a1aa;
      font-size: 13px;
      margin-top: 18px;
    }}
  </style>
</head>
<body>
  <div class="container">
    <h1 class="error">Login failed</h1>
    <div class="details">
      <strong>Supabase error:</strong> {error}<br>
      <strong>Description:</strong> {description}
    </div>
    <p class="hint">Copy this error and check <code>{auth_log_hint}</code> for the full OAuth trace.</p>
  </div>
</body>
</html>"#
    )
}

#[derive(Default)]
struct OAuthState {
    verifier: Option<String>,
    provider: Option<String>,
}

fn set_oauth_state(verifier: String, provider: String) {
    let mutex = OAUTH_STATE.get_or_init(|| Mutex::new(OAuthState::default()));
    // Usamos lock().ok() + default para sobrevivir a un poison: si un hilo
    // panicó mientras sostenía este mutex, no queremos que el próximo login
    // aborte el proceso.
    match mutex.lock() {
        Ok(mut lock) => {
            lock.verifier = Some(verifier);
            lock.provider = Some(provider);
        }
        Err(poisoned) => {
            let mut lock = poisoned.into_inner();
            lock.verifier = Some(verifier);
            lock.provider = Some(provider);
            println!("[Auth] OAuth state mutex was poisoned; recovered.");
        }
    }
}

fn get_oauth_state() -> (Option<String>, Option<String>) {
    let mutex = OAUTH_STATE.get_or_init(|| Mutex::new(OAuthState::default()));
    match mutex.lock() {
        Ok(lock) => (lock.verifier.clone(), lock.provider.clone()),
        Err(poisoned) => {
            let lock = poisoned.into_inner();
            println!("[Auth] OAuth state mutex was poisoned; recovered.");
            (lock.verifier.clone(), lock.provider.clone())
        }
    }
}

// Provider configs
#[derive(Clone)]
struct ProviderConfig {
    /// Provider id (reserved for logging / future use).
    #[allow(dead_code)]
    name: &'static str,
    auth_url: &'static str,
    token_url: &'static str,
    scopes: &'static [&'static str],
    userinfo_url: &'static str,
}

const GOOGLE: ProviderConfig = ProviderConfig {
    name: "google",
    auth_url: "https://accounts.google.com/o/oauth2/v2/auth",
    token_url: "https://oauth2.googleapis.com/token",
    scopes: &["openid", "email", "profile"],
    userinfo_url: "https://www.googleapis.com/oauth2/v3/userinfo",
};

const JIRA: ProviderConfig = ProviderConfig {
    name: "jira",
    auth_url: "https://auth.atlassian.com/authorize",
    token_url: "https://auth.atlassian.com/oauth/token",
    scopes: &["read:jira-work", "read:jira-user", "offline_access", "read:me"],
    userinfo_url: "https://api.atlassian.com/me",
};

const LINEAR: ProviderConfig = ProviderConfig {
    name: "linear",
    auth_url: "https://linear.app/oauth/authorize",
    token_url: "https://api.linear.app/oauth/token",
    scopes: &["read", "issues:create"],
    userinfo_url: "https://api.linear.app/graphql",
};

const REDIRECT_URL: &str = "http://localhost:12345/callback";

const SUPABASE_SUCCESS_HTML_TEMPLATE: &str = r#"<!DOCTYPE html>
<html lang="en">
<head>
  <meta charset="UTF-8">
  <meta name="viewport" content="width=device-width, initial-scale=1.0">
  <title>FlowSight - Login Successful</title>
  <style>
    * { margin: 0; padding: 0; box-sizing: border-box; }
    body {
      font-family: -apple-system, BlinkMacSystemFont, 'Segoe UI', Roboto, 'Helvetica Neue', Arial, sans-serif;
      background: linear-gradient(135deg, #0a0a0a 0%, #1a1a2e 50%, #16213e 100%);
      min-height: 100vh;
      display: flex;
      align-items: center;
      justify-content: center;
      color: #fafafa;
    }
    .container {
      text-align: center;
      padding: 48px;
      max-width: 420px;
    }
    .logo-img {
      display: block;
      margin: 0 auto 20px;
      max-width: min(240px, 80vw);
      height: auto;
    }
    .check-icon {
      width: 80px;
      height: 80px;
      margin: 24px auto;
      background: linear-gradient(135deg, #22c55e, #16a34a);
      border-radius: 50%;
      display: flex;
      align-items: center;
      justify-content: center;
      box-shadow: 0 0 40px rgba(34, 197, 94, 0.3);
      animation: pulse 2s ease-in-out infinite;
    }
    .check-icon svg {
      width: 40px;
      height: 40px;
      stroke: white;
      stroke-width: 3;
      fill: none;
    }
    @keyframes pulse {
      0%, 100% { transform: scale(1); box-shadow: 0 0 40px rgba(34, 197, 94, 0.3); }
      50% { transform: scale(1.05); box-shadow: 0 0 60px rgba(34, 197, 94, 0.5); }
    }
    h1 {
      font-size: 24px;
      font-weight: 600;
      margin-bottom: 12px;
    }
    p {
      color: #a1a1aa;
      font-size: 14px;
      line-height: 1.6;
    }
    .hint {
      margin-top: 32px;
      padding: 16px 24px;
      background: rgba(255, 255, 255, 0.05);
      border: 1px solid rgba(255, 255, 255, 0.1);
      border-radius: 12px;
      font-size: 13px;
      color: #71717a;
    }
    .close-btn {
      margin-top: 24px;
      padding: 12px 32px;
      background: linear-gradient(135deg, #3b82f6, #8b5cf6);
      border: none;
      border-radius: 8px;
      color: white;
      font-size: 14px;
      font-weight: 500;
      cursor: pointer;
      transition: all 0.2s;
    }
    .close-btn:hover {
      transform: translateY(-2px);
      box-shadow: 0 8px 24px rgba(59, 130, 246, 0.4);
    }
  </style>
</head>
<body>
  <div class="container">
    <img class="logo-img" src="__FLOW_LOGO_DATA_URI__" alt="FlowSight" />
    <div class="check-icon">
      <svg viewBox="0 0 24 24"><polyline points="20 6 9 17 4 12"></polyline></svg>
    </div>
    <h1>Login Successful!</h1>
    <p>Your account has been connected successfully.</p>
    <div class="hint">You can now close this tab and return to the FlowSight app.</div>
    <button class="close-btn" onclick="window.close()">Close Tab</button>
  </div>
</body>
</html>"#;

fn supabase_login_success_html() -> String {
    static HTML: OnceLock<String> = OnceLock::new();
    HTML.get_or_init(|| {
        let bytes = include_bytes!(concat!(env!("CARGO_MANIFEST_DIR"), "/flowsight_sinfondo.png"));
        let b64 = base64::engine::general_purpose::STANDARD.encode(bytes);
        let data_uri = format!("data:image/png;base64,{}", b64);
        SUPABASE_SUCCESS_HTML_TEMPLATE.replace("__FLOW_LOGO_DATA_URI__", &data_uri)
    })
    .clone()
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct AuthUser {
    pub id: String,
    pub email: String,
    pub display_name: String,
    pub avatar_url: Option<String>,
    pub provider: String,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct AuthSession {
    pub user: AuthUser,
    pub access_token: String,
    pub refresh_token: Option<String>,
    pub provider: String,
}

fn get_env_var(key: &str) -> Option<String> {
    std::env::var(key).ok()
}

fn get_provider_client_id(provider: &str) -> String {
    match provider {
        "google" => get_env_var("VITE_GOOGLE_CLIENT_ID").unwrap_or_default(),
        "jira" => crate::oauth_env::jira_client_id(),
        "linear" => crate::oauth_env::linear_client_id(),
        _ => String::new(),
    }
}

fn get_provider_client_secret(provider: &str) -> Option<String> {
    match provider {
        "google" => get_env_var("VITE_GOOGLE_CLIENT_SECRET"),
        "jira" => crate::oauth_env::jira_client_secret(),
        "linear" => crate::oauth_env::linear_client_secret(),
        _ => None,
    }
}

fn get_provider_config(provider: &str) -> Option<ProviderConfig> {
    match provider {
        "google" => Some(GOOGLE),
        "jira" => Some(JIRA),
        "linear" => Some(LINEAR),
        _ => None,
    }
}

fn create_oauth_client(provider: &str) -> Result<BasicClient, String> {
    use oauth2::ClientSecret;
    
    let config = get_provider_config(provider).ok_or("Unknown provider")?;
    let client_id = get_provider_client_id(provider);
    let client_secret = get_provider_client_secret(provider);
    
    if client_id.is_empty() {
        return Err(format!("Missing client ID for {}", provider));
    }
    
    let mut client = BasicClient::new(
        ClientId::new(client_id),
        client_secret.map(ClientSecret::new),
        AuthUrl::new(config.auth_url.to_string()).map_err(|e| e.to_string())?,
        Some(TokenUrl::new(config.token_url.to_string()).map_err(|e| e.to_string())?)
    );
    
    client = client.set_redirect_uri(
        RedirectUrl::new(REDIRECT_URL.to_string()).map_err(|e| e.to_string())?
    );
    
    Ok(client)
}

#[tauri::command]
pub fn start_auth(provider: String) -> Result<String, String> {
    auth_log(format!("[Auth] start_auth requested for provider: {}", provider));

    let db_path = crate::paths::db_path()?;

    // Google uses Supabase OAuth (configured in Supabase Dashboard)
    if provider == "google" {
        return start_supabase_oauth(&provider);
    }

    // Jira and Linear are paid-plan integrations only.
    crate::entitlements::require_feature(&db_path, "integrations")?;
    
    // Jira and Linear use direct OAuth with .env keys
    let config = get_provider_config(&provider).ok_or("Unknown provider")?;
    let client = create_oauth_client(&provider)?;
    
    let (pkce_challenge, pkce_verifier) = PkceCodeChallenge::new_random_sha256();
    set_oauth_state(pkce_verifier.secret().to_string(), provider.clone());
    
    let mut auth_request = client
        .authorize_url(CsrfToken::new_random)
        .set_pkce_challenge(pkce_challenge);
    
    for scope in config.scopes {
        auth_request = auth_request.add_scope(Scope::new(scope.to_string()));
    }
    
    // Jira requires audience parameter
    let (mut auth_url, _csrf) = auth_request.url();
    if provider == "jira" {
        auth_url.query_pairs_mut().append_pair("audience", "api.atlassian.com");
    }
    
    auth_log(format!("[Auth] Opening direct OAuth URL for provider: {}", provider));

    // Open browser
    open::that(auth_url.as_str()).map_err(|e| e.to_string())?;
    
    // Start callback listener — aislado en catch_unwind por si un panic
    // suelto en el hilo llega a abortar el proceso (depende de profile.panic).
    std::thread::spawn(move || {
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            listen_for_callback();
        }));
        if let Err(payload) = result {
            auth_log(format!(
                "[Auth] Direct OAuth callback listener panicked: {}",
                panic_payload_to_string(payload)
            ));
        }
    });

    Ok(format!("Browser opened for {} login", provider))
}

// Supabase OAuth for providers configured in Supabase Dashboard (Google, etc.)
fn start_supabase_oauth(provider: &str) -> Result<String, String> {
    set_oauth_state("supabase".to_string(), provider.to_string());
    auth_log(format!("[Auth] Starting Supabase OAuth for provider: {}", provider));
    
    let redirect_to = "http://localhost:12345/callback";
    let auth_url = format!(
        "{}/auth/v1/authorize?provider={}&redirect_to={}",
        supabase_url(),
        provider,
        urlencoding::encode(redirect_to)
    );
    
    auth_log(format!("[Auth] Opening Supabase OAuth URL: {}", auth_url));
    
    open::that(&auth_url).map_err(|e| {
        auth_log(format!("[Auth] Failed to open browser: {}", e));
        format!("Failed to open browser: {}", e)
    })?;
    
    // Start callback listener for Supabase tokens — envuelto en catch_unwind
    // para no poder tumbar el proceso ante un panic inesperado.
    std::thread::spawn(move || {
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            listen_for_supabase_callback();
        }));
        if let Err(payload) = result {
            auth_log(format!(
                "[Auth] Supabase callback listener panicked: {}",
                panic_payload_to_string(payload)
            ));
        }
    });

    Ok(format!("Browser opened for {} login via Supabase", provider))
}

fn listen_for_callback() {
    // Give any previous listener thread time to release the port
    std::thread::sleep(std::time::Duration::from_millis(300));

    let mut server_opt = None;
    for attempt in 0..5 {
        match Server::http("127.0.0.1:12345") {
            Ok(s) => { server_opt = Some(s); break; }
            Err(_) => {
                println!("[Auth] Port 12345 busy (attempt {}), retrying...", attempt + 1);
                std::thread::sleep(std::time::Duration::from_secs(1));
            }
        }
    }
    let server = match server_opt {
        Some(s) => s,
        None => {
            println!("[Auth] Could not bind port 12345 after 5 attempts");
            return;
        }
    };

    // Auto-exit after 120s so este hilo nunca queda parkeado bloqueando el
    // puerto 12345 (tiny_http no tiene set_read_timeout; hay que usar
    // recv_timeout con deadline manual).
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(120);

    println!("[Auth] Listening for callback on 127.0.0.1:12345...");

    loop {
        let remaining = deadline.saturating_duration_since(std::time::Instant::now());
        if remaining.is_zero() {
            println!("[Auth] Listener timed out waiting for OAuth callback");
            break;
        }
        let request = match server.recv_timeout(remaining) {
            Ok(Some(r)) => r,
            Ok(None) => {
                println!("[Auth] Listener timed out waiting for OAuth callback");
                break;
            }
            Err(e) => {
                println!("[Auth] Listener error: {}", e);
                break;
            }
        };

        let url = format!("http://localhost:12345{}", request.url());
        let parsed = match Url::parse(&url) {
            Ok(p) => p,
            Err(e) => {
                println!("[Auth] Ignoring unparsable callback URL ({}): {}", e, url);
                let _ = request.respond(Response::from_string("Bad request"));
                continue;
            }
        };
        let pairs: std::collections::HashMap<_, _> = parsed.query_pairs().into_owned().collect();

        if let Some(code) = pairs.get("code") {
            let (verifier_opt, provider_opt) = get_oauth_state();

            let (verifier, provider) = match (verifier_opt, provider_opt) {
                (Some(v), Some(p)) => (v, p),
                _ => {
                    println!("[Auth] Callback received but OAuth state is missing");
                    let _ = request.respond(Response::from_string("Error: Invalid OAuth state"));
                    continue;
                }
            };
            
            match exchange_code(&provider, code, &verifier) {
                Ok(session) => {
                    save_auth_session(&session);
                    
                    // For Jira: also save provider-specific tokens so jira.rs can find them
                    if provider == "jira" {
                        save_jira_specific_tokens(&session);
                    }
                    
                    let _ = request.respond(Response::from_string(supabase_login_success_html())
                        .with_header(tiny_http::Header::from_bytes(&b"Content-Type"[..], &b"text/html"[..]).unwrap()));
                    break;
                }
                Err(e) => {
                    let _ = request.respond(Response::from_string(format!("Error: {}", e)));
                }
            }
        }
    }
}

// Supabase OAuth callback - handles tokens in URL fragment
// Supabase returns tokens in hash fragment (#access_token=...) which browsers
// never send to the server. We serve an HTML page that extracts the fragment
// and submits it back via a same-origin <form> GET — this is never blocked
// by browser cross-origin navigation policies unlike window.location.href.
fn listen_for_supabase_callback() {
    auth_log("[Auth] Supabase callback listener starting");

    // Give any previous listener thread time to release port 12345
    std::thread::sleep(std::time::Duration::from_millis(300));

    let mut server_opt = None;
    for attempt in 0..5 {
        match Server::http("127.0.0.1:12345") {
            Ok(s) => {
                auth_log(format!(
                    "[Auth] Supabase callback listener bound to 127.0.0.1:12345 on attempt {}",
                    attempt + 1
                ));
                server_opt = Some(s);
                break;
            }
            Err(_) => {
                auth_log(format!("[Auth] Supabase port 12345 busy (attempt {}), retrying...", attempt + 1));
                std::thread::sleep(std::time::Duration::from_secs(1));
            }
        }
    }
    let server = match server_opt {
        Some(s) => s,
        None => {
            auth_log("[Auth] Could not bind port 12345 for Supabase after 5 attempts");
            return;
        }
    };

    // Auto-exit after 120s para no dejar el puerto pegado si el usuario
    // abandona el login. tiny_http no soporta set_read_timeout: aplicamos
    // deadline manual + recv_timeout más abajo.
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(120);

    auth_log("[Auth] Listening for Supabase callback on 127.0.0.1:12345...");
    
    // FIX: Use a same-origin <form> submit instead of window.location.href.
    // Chrome 115+ and Firefox block cross-origin href redirects to localhost
    // when the initiating page comes from a Supabase/Google domain. A form
    // submit is always same-origin (the target is our own localhost server)
    // and is never intercepted by browser security policies.
    let capture_html = r#"<!DOCTYPE html>
<html>
<head>
  <meta charset="UTF-8">
  <title>FlowSight Login</title>
</head>
<body>
  <p>Completing login, please wait...</p>
  <script>
    (function () {
      var hash = window.location.hash.substring(1);
      if (!hash) {
        document.body.innerHTML = '<p style="color:red">Login failed &ndash; no token received. Please close this tab and try again.</p>';
        return;
      }
      // Build a <form> and submit it as a GET to /token.
      // This is a same-origin request and is NEVER blocked by browser
      // cross-origin navigation guards (unlike window.location.href).
      var form = document.createElement('form');
      form.method = 'GET';
      form.action = '/token';
      hash.split('&').forEach(function (pair) {
        var eqIdx = pair.indexOf('=');
        if (eqIdx === -1) return;
        var key = decodeURIComponent(pair.substring(0, eqIdx));
        var val = decodeURIComponent(pair.substring(eqIdx + 1));
        var inp = document.createElement('input');
        inp.type = 'hidden';
        inp.name = key;
        inp.value = val;
        form.appendChild(inp);
      });
      document.body.appendChild(form);
      form.submit();
    })();
  </script>
</body>
</html>"#;
    
    loop {
        let remaining = deadline.saturating_duration_since(std::time::Instant::now());
        if remaining.is_zero() {
            auth_log("[Auth] Supabase listener timed out waiting for callback");
            break;
        }
        let request = match server.recv_timeout(remaining) {
            Ok(Some(r)) => r,
            Ok(None) => {
                auth_log("[Auth] Supabase listener timed out waiting for callback");
                break;
            }
            Err(e) => {
                auth_log(format!("[Auth] Supabase listener error: {}", e));
                break;
            }
        };

        let url = format!("http://localhost:12345{}", request.url());
        auth_log(format!("[Auth] Supabase callback request received: {}", url));
        let parsed = match Url::parse(&url) {
            Ok(p) => p,
            Err(e) => {
                auth_log(format!("[Auth] Ignoring unparsable Supabase callback URL ({}): {}", e, url));
                let _ = request.respond(Response::from_string("Bad request"));
                continue;
            }
        };
        let path = parsed.path();
        let pairs: std::collections::HashMap<_, _> = parsed.query_pairs().into_owned().collect();

        if let Some(error) = pairs.get("error") {
            let desc = pairs
                .get("error_description")
                .or_else(|| pairs.get("error_code"))
                .map(|s| s.as_str())
                .unwrap_or("");
            auth_log(format!(
                "[Auth] Supabase OAuth error on {}: {} - {}",
                path, error, desc
            ));
            let _ = request.respond(Response::from_string(supabase_login_error_html(error, desc))
                .with_header(tiny_http::Header::from_bytes(&b"Content-Type"[..], &b"text/html; charset=utf-8"[..]).unwrap()));
            break;
        }

        // First request: serve HTML to capture hash fragment via form submit
        if path == "/callback" {
            auth_log("[Auth] Serving token capture page (form-submit method)...");
            let _ = request.respond(Response::from_string(capture_html)
                .with_header(tiny_http::Header::from_bytes(&b"Content-Type"[..], &b"text/html; charset=utf-8"[..]).unwrap()));
            continue;
        }
        
        // Second request: receive tokens via query params from the form submit
        if path == "/token" {
            auth_log("[Auth] Supabase token callback received");
            
            if let Some(access_token) = pairs.get("access_token") {
                let (_, provider_opt) = get_oauth_state();
                let provider = provider_opt.unwrap_or_else(|| "google".to_string());
                auth_log(format!(
                    "[Auth] Supabase access token received (provider: {}, refresh_token: {})",
                    provider,
                    pairs.contains_key("refresh_token")
                ));
                
                // Fetch user info from Supabase
                match fetch_supabase_user(access_token) {
                    Ok(user) => {
                        let session = AuthSession {
                            user,
                            access_token: access_token.clone(),
                            refresh_token: pairs.get("refresh_token").cloned(),
                            provider,
                        };
                        save_auth_session(&session);
                        auth_log(format!(
                            "[Auth] Supabase login successful for {} ({})",
                            session.user.email, session.user.id
                        ));
                        let _ = request.respond(Response::from_string(supabase_login_success_html())
                            .with_header(tiny_http::Header::from_bytes(&b"Content-Type"[..], &b"text/html"[..]).unwrap()));
                        break;
                    }
                    Err(e) => {
                        auth_log(format!("[Auth] Failed to fetch Supabase user: {}", e));
                        let _ = request.respond(Response::from_string(format!("Error: {}", e)));
                    }
                }
            } else if let Some(error) = pairs.get("error") {
                let desc = pairs.get("error_description").map(|s| s.as_str()).unwrap_or("");
                auth_log(format!("[Auth] OAuth error from Supabase: {} - {}", error, desc));
                let _ = request.respond(Response::from_string(format!("Auth Error: {} - {}", error, desc)));
                break;
            } else {
                auth_log("[Auth] /token callback received without access_token or error");
            }
        } else {
            auth_log(format!("[Auth] Ignoring unexpected Supabase callback path: {}", path));
            let _ = request.respond(Response::from_string("Not found").with_status_code(404));
        }
    }
}

fn fetch_supabase_user(access_token: &str) -> Result<AuthUser, String> {
    let client = Client::new();
    let resp = client.get(format!("{}/auth/v1/user", supabase_url()))
        .header("apikey", supabase_anon_key())
        .header("Authorization", format!("Bearer {}", access_token))
        .send()
        .map_err(|e| e.to_string())?;
    
    if !resp.status().is_success() {
        return Err("Failed to fetch user from Supabase".to_string());
    }
    
    let json: serde_json::Value = resp.json().map_err(|e| e.to_string())?;
    
    Ok(AuthUser {
        id: json["id"].as_str().unwrap_or_default().to_string(),
        email: json["email"].as_str().unwrap_or_default().to_string(),
        display_name: json["user_metadata"]["full_name"].as_str()
            .or(json["user_metadata"]["name"].as_str())
            .unwrap_or("User")
            .to_string(),
        avatar_url: json["user_metadata"]["avatar_url"].as_str().map(String::from),
        provider: "google".to_string(),
    })
}

fn exchange_code(provider: &str, code: &str, verifier: &str) -> Result<AuthSession, String> {
    let client = create_oauth_client(provider)?;
    
    let token_result = client
        .exchange_code(AuthorizationCode::new(code.to_string()))
        .set_pkce_verifier(oauth2::PkceCodeVerifier::new(verifier.to_string()))
        .request(http_client)
        .map_err(|e| format!("Token exchange failed: {:?}", e))?;
    
    let access_token = token_result.access_token().secret().to_string();
    let refresh_token = token_result.refresh_token().map(|t| t.secret().to_string());
    
    // Fetch user info
    let user = fetch_user_info(provider, &access_token)?;
    
    Ok(AuthSession {
        user,
        access_token,
        refresh_token,
        provider: provider.to_string(),
    })
}

fn fetch_user_info(provider: &str, access_token: &str) -> Result<AuthUser, String> {
    let http_client = Client::new();
    
    match provider {
        "google" => {
            let resp = http_client.get(GOOGLE.userinfo_url)
                .bearer_auth(access_token)
                .send()
                .map_err(|e| e.to_string())?;
            
            let json: serde_json::Value = resp.json().map_err(|e| e.to_string())?;
            
            Ok(AuthUser {
                id: json["sub"].as_str().unwrap_or_default().to_string(),
                email: json["email"].as_str().unwrap_or_default().to_string(),
                display_name: json["name"].as_str().unwrap_or_default().to_string(),
                avatar_url: json["picture"].as_str().map(String::from),
                provider: "google".to_string(),
            })
        }
        "jira" => {
            let resp = http_client.get(JIRA.userinfo_url)
                .bearer_auth(access_token)
                .send()
                .map_err(|e| e.to_string())?;
            
            let json: serde_json::Value = resp.json().map_err(|e| e.to_string())?;
            
            Ok(AuthUser {
                id: json["account_id"].as_str().unwrap_or_default().to_string(),
                email: json["email"].as_str().unwrap_or_default().to_string(),
                display_name: json["name"].as_str().unwrap_or_default().to_string(),
                avatar_url: json["picture"].as_str().map(String::from),
                provider: "jira".to_string(),
            })
        }
        "linear" => {
            let query = r#"{ "query": "{ viewer { id email name avatarUrl } }" }"#;
            let resp = http_client.post(LINEAR.userinfo_url)
                .bearer_auth(access_token)
                .header("Content-Type", "application/json")
                .body(query)
                .send()
                .map_err(|e| e.to_string())?;
            
            let json: serde_json::Value = resp.json().map_err(|e| e.to_string())?;
            let viewer = &json["data"]["viewer"];
            
            Ok(AuthUser {
                id: viewer["id"].as_str().unwrap_or_default().to_string(),
                email: viewer["email"].as_str().unwrap_or_default().to_string(),
                display_name: viewer["name"].as_str().unwrap_or_default().to_string(),
                avatar_url: viewer["avatarUrl"].as_str().map(String::from),
                provider: "linear".to_string(),
            })
        }
        _ => Err("Unknown provider".to_string()),
    }
}

fn get_db_conn() -> Result<Connection, String> {
    // En instalación fresca, %LOCALAPPDATA%\FlowSight\ todavía no existe hasta
    // que corre FlowSightAgent::new(). El callback OAuth llega ANTES de que
    // `initialize_agent` haya corrido (el usuario no está logueado todavía),
    // así que hay que garantizar el directorio acá o sqlite devuelve
    // "unable to open database file" y perdemos la sesión en silencio.
    let base = dirs::data_local_dir().ok_or("No local data dir")?;
    let dir = base.join("FlowSight");
    if !dir.exists() {
        std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    }
    let db_path = dir.join("dev-agent.db");
    let conn = Connection::open(&db_path).map_err(|e| e.to_string())?;
    // Asegurar tabla `config` por si somos los primeros en abrir la DB (antes
    // de que agent::init_db corra). Sin esto, los INSERT posteriores también
    // fallan en silencio.
    conn.execute(
        "CREATE TABLE IF NOT EXISTS config (key TEXT PRIMARY KEY, value TEXT)",
        [],
    )
    .map_err(|e| e.to_string())?;
    Ok(conn)
}

// Save Jira-specific tokens so jira.rs functions can find them
// jira.rs reads from 'jira_access_token', 'jira_refresh_token', 'jira_cloud_id' config keys
fn save_jira_specific_tokens(session: &AuthSession) {
    if let Ok(conn) = get_db_conn() {
        let _ = conn.execute(
            "INSERT OR REPLACE INTO config (key, value) VALUES ('jira_access_token', ?1)",
            [&session.access_token]
        );
        if let Some(ref rt) = session.refresh_token {
            let _ = conn.execute(
                "INSERT OR REPLACE INTO config (key, value) VALUES ('jira_refresh_token', ?1)",
                [rt]
            );
        }
        
        // Fetch and save Cloud ID (needed for Jira API calls)
        let http_client = Client::new();
        match http_client.get("https://api.atlassian.com/oauth/token/accessible-resources")
            .bearer_auth(&session.access_token)
            .send()
        {
            Ok(resp) => {
                if let Ok(json) = resp.json::<serde_json::Value>() {
                    if let Some(cloud_id) = json[0]["id"].as_str() {
                        let _ = conn.execute(
                            "INSERT OR REPLACE INTO config (key, value) VALUES ('jira_cloud_id', ?1)",
                            [cloud_id]
                        );
                        println!("[Auth] Saved Jira cloud_id: {}", cloud_id);
                    }
                }
            }
            Err(e) => println!("[Auth] Could not fetch Jira cloud_id: {}", e),
        }
        
        println!("[Auth] Jira-specific tokens saved for jira.rs compatibility");
    }
}

fn save_auth_session(session: &AuthSession) {
    match get_db_conn() {
        Ok(conn) => {
            let json = serde_json::to_string(session).unwrap_or_default();
            match conn.execute(
                "INSERT OR REPLACE INTO config (key, value) VALUES ('auth_session', ?1)",
                [&json],
            ) {
                Ok(_) => println!("[Auth] Session saved for: {}", session.user.email),
                Err(e) => println!("[Auth] FAILED to persist session for {}: {}", session.user.email, e),
            }
        }
        Err(e) => println!("[Auth] FAILED to open DB while saving session: {}", e),
    }
}

#[tauri::command]
pub fn get_auth_session() -> Result<Option<AuthSession>, String> {
    let conn = get_db_conn()?;
    
    let json: Result<String, _> = conn.query_row(
        "SELECT value FROM config WHERE key = 'auth_session'",
        [],
        |row| row.get(0)
    );
    
    match json {
        Ok(j) => Ok(serde_json::from_str(&j).ok()),
        Err(_) => Ok(None),
    }
}

#[tauri::command]
pub fn logout() -> Result<(), String> {
    let conn = get_db_conn()?;
    conn.execute("DELETE FROM config WHERE key = 'auth_session'", [])
        .map_err(|e| e.to_string())?;
    conn.execute("DELETE FROM config WHERE key = 'user_session'", [])
        .map_err(|e| e.to_string())?;
    crate::entitlements::clear_entitlements(&conn)?;
    println!("[Auth] Logged out (cleared auth_session + user_session + entitlements)");
    Ok(())
}

/// Parses `access_token` / optional `refresh_token` from a hash or query fragment, or treats `code` as a raw JWT.
pub(crate) fn parse_tokens_from_oauth_code(code: &str) -> Result<(String, Option<String>), String> {
    if !code.contains("access_token=") {
        return Ok((code.to_string(), None));
    }
    let fragment = if let Some(hash_pos) = code.find('#') {
        &code[hash_pos + 1..]
    } else {
        code
    };

    let params: std::collections::HashMap<String, String> = fragment
        .split('&')
        .filter_map(|pair| {
            let mut parts = pair.splitn(2, '=');
            Some((parts.next()?.to_string(), parts.next()?.to_string()))
        })
        .collect();

    let at = params
        .get("access_token")
        .ok_or_else(|| "No access_token found in URL".to_string())?
        .clone();
    let rt = params.get("refresh_token").cloned();
    Ok((at, rt))
}

// Async so Tauri dispatches it off the main/UI thread: the blocking `reqwest`
// call below opens a real socket, which is what lets Windows inject a broken
// Winsock LSP into this process (see crash_guard.rs module docs).
#[tauri::command]
pub async fn login_with_code(code: String) -> Result<AuthSession, String> {
    tauri::async_runtime::spawn_blocking(move || login_with_code_blocking(code))
        .await
        .map_err(|e| format!("Task join error: {}", e))?
}

fn login_with_code_blocking(code: String) -> Result<AuthSession, String> {
    // Support both raw JWT tokens and full redirect URLs with hash fragments
    // e.g., "https://flowsight.site/#access_token=eyJ...&refresh_token=abc&..."
    let (access_token, refresh_token) = match parse_tokens_from_oauth_code(&code) {
        Ok((at, rt)) => {
            if code.contains("access_token=") {
                println!("[Auth] Extracted tokens from URL (refresh_token: {})", rt.is_some());
            }
            (at, rt)
        }
        Err(e) => return Err(e),
    };
    
    // Validate against Supabase Auth API
    let client = Client::new();
    let resp = client.get(format!("{}/auth/v1/user", supabase_url()))
        .header("apikey", supabase_anon_key())
        .header("Authorization", format!("Bearer {}", access_token))
        .send()
        .map_err(|e| e.to_string())?;
    
    if !resp.status().is_success() {
        return Err("Invalid token or expired code".to_string());
    }
    
    let json: serde_json::Value = resp.json().map_err(|e| e.to_string())?;
    
    // Construct session
    let user = AuthUser {
        id: json["id"].as_str().unwrap_or_default().to_string(),
        email: json["email"].as_str().unwrap_or_default().to_string(),
        display_name: json["user_metadata"]["full_name"].as_str()
            .or(json["user_metadata"]["name"].as_str())
            .unwrap_or("User")
            .to_string(),
        avatar_url: json["user_metadata"]["avatar_url"].as_str().map(String::from),
        provider: "google".to_string(),
    };
    
    let session = AuthSession {
        user,
        access_token,
        refresh_token,
        provider: "google".to_string(),
    };
    
    save_auth_session(&session);
    println!("[Auth] Login with code successful for: {}", session.user.email);
    Ok(session)
}

#[tauri::command]
pub fn is_logged_in() -> Result<bool, String> {
    Ok(get_auth_session()?.is_some())
}

#[cfg(test)]
mod oauth_code_parse_tests {
    use super::parse_tokens_from_oauth_code;

    #[test]
    fn raw_token_without_equals() {
        let (at, rt) = parse_tokens_from_oauth_code("eyJhbGciOiJIUzI1NiJ9.x.y").unwrap();
        assert_eq!(at, "eyJhbGciOiJIUzI1NiJ9.x.y");
        assert!(rt.is_none());
    }

    #[test]
    fn hash_fragment_tokens() {
        let url = "https://app.test/callback#access_token=AAA&refresh_token=BBB&token_type=bearer";
        let (at, rt) = parse_tokens_from_oauth_code(url).unwrap();
        assert_eq!(at, "AAA");
        assert_eq!(rt.as_deref(), Some("BBB"));
    }

    #[test]
    fn query_style_without_hash() {
        let s = "access_token=CCC&refresh_token=DDD";
        let (at, rt) = parse_tokens_from_oauth_code(s).unwrap();
        assert_eq!(at, "CCC");
        assert_eq!(rt.as_deref(), Some("DDD"));
    }

}
