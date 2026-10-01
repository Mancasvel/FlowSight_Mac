//! Encrypted local planner state. A random AES-256 key lives in macOS Keychain;
//! SQLite stores only authenticated ciphertext. There is no plaintext fallback.
use aes_gcm::{
    aead::{Aead, AeadCore, OsRng},
    Aes256Gcm, KeyInit,
};
use base64::{engine::general_purpose::STANDARD as BASE64, Engine as _};
#[cfg(target_os = "macos")]
use rand::RngCore;
use rusqlite::{params, Connection, OptionalExtension};

const PREFIX: &str = "keychain:aes256gcm:v1:";

#[cfg(target_os = "macos")]
fn encryption_key() -> Result<[u8; 32], String> {
    use security_framework::passwords::{get_generic_password, set_generic_password};
    const SERVICE: &str = "ai.flowsight.agent.local-state";
    const ACCOUNT: &str = "planner-encryption-key-v1";
    match get_generic_password(SERVICE, ACCOUNT) {
        Ok(bytes) => bytes
            .try_into()
            .map_err(|_| "The FlowSight Keychain key is invalid.".into()),
        Err(error) if error.code() == -25300 => {
            let mut key = [0u8; 32];
            rand::rngs::OsRng.fill_bytes(&mut key);
            set_generic_password(SERVICE, ACCOUNT, &key).map_err(|_| {
                "Could not save the FlowSight encryption key in Keychain.".to_string()
            })?;
            Ok(key)
        }
        Err(_) => Err("Unlock your Keychain to save the local session plan.".into()),
    }
}

#[cfg(not(target_os = "macos"))]
fn encryption_key() -> Result<[u8; 32], String> {
    Err("This build requires macOS Keychain for protected planner storage.".into())
}

pub fn save_secret(conn: &Connection, key: &str, value: &str) -> Result<(), String> {
    let cipher = Aes256Gcm::new_from_slice(&encryption_key()?).map_err(|e| e.to_string())?;
    let nonce = Aes256Gcm::generate_nonce(&mut OsRng);
    let ciphertext = cipher
        .encrypt(&nonce, value.as_bytes())
        .map_err(|_| "Could not encrypt planner state.")?;
    let encoded = format!(
        "{PREFIX}{}:{}",
        BASE64.encode(nonce),
        BASE64.encode(ciphertext)
    );
    conn.execute(
        "INSERT OR REPLACE INTO config (key, value) VALUES (?1, ?2)",
        params![key, encoded],
    )
    .map_err(|e| e.to_string())?;
    Ok(())
}

pub fn load_secret(conn: &Connection, key: &str) -> Result<Option<String>, String> {
    let value: Option<String> = conn
        .query_row("SELECT value FROM config WHERE key=?1", [key], |r| r.get(0))
        .optional()
        .map_err(|e| e.to_string())?;
    let Some(value) = value else {
        return Ok(None);
    };
    let encoded = value
        .strip_prefix(PREFIX)
        .ok_or("Planner storage is not in the protected format.")?;
    let (nonce, ciphertext) = encoded
        .split_once(':')
        .ok_or("Protected planner storage is invalid.")?;
    let nonce = BASE64
        .decode(nonce)
        .map_err(|_| "Protected planner nonce is invalid.")?;
    if nonce.len() != 12 {
        return Err("Protected planner nonce is invalid.".into());
    }
    let ciphertext = BASE64
        .decode(ciphertext)
        .map_err(|_| "Protected planner storage is invalid.")?;
    let cipher = Aes256Gcm::new_from_slice(&encryption_key()?).map_err(|e| e.to_string())?;
    let plaintext = cipher
        .decrypt(aes_gcm::Nonce::from_slice(&nonce), ciphertext.as_ref())
        .map_err(|_| "Could not decrypt the local planner state.")?;
    String::from_utf8(plaintext)
        .map(Some)
        .map_err(|_| "Protected planner state is invalid UTF-8.".into())
}
