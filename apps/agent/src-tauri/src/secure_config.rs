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
#[cfg(target_os = "macos")]
use std::sync::Mutex;

const PREFIX: &str = "keychain:aes256gcm:v1:";
#[cfg(target_os = "macos")]
static KEY_LOCK: Mutex<()> = Mutex::new(());

#[cfg(target_os = "macos")]
fn keychain_key(service: &str, account: &str, create: bool) -> Result<[u8; 32], String> {
    use security_framework::passwords::{get_generic_password, set_generic_password};
    let _guard = KEY_LOCK
        .lock()
        .map_err(|_| "The Keychain key lock is unavailable.")?;
    match get_generic_password(service, account) {
        Ok(bytes) => bytes
            .try_into()
            .map_err(|_| "The FlowSight Keychain key is invalid.".into()),
        Err(error) if error.code() == -25300 && create => {
            let mut key = [0u8; 32];
            rand::rngs::OsRng.fill_bytes(&mut key);
            set_generic_password(service, account, &key).map_err(|_| {
                "Could not save the FlowSight encryption key in Keychain.".to_string()
            })?;
            Ok(key)
        }
        Err(error) if error.code() == -25300 => {
            Err("The encryption key for the saved local plan is missing from Keychain.".into())
        }
        Err(_) => Err("Unlock your Keychain to save the local session plan.".into()),
    }
}

#[cfg(target_os = "macos")]
fn encryption_key(create: bool) -> Result<[u8; 32], String> {
    keychain_key(
        "ai.flowsight.agent.local-state",
        "planner-encryption-key-v1",
        create,
    )
}

#[cfg(not(target_os = "macos"))]
fn encryption_key(_create: bool) -> Result<[u8; 32], String> {
    Err("This build requires macOS Keychain for protected planner storage.".into())
}

pub fn save_secret(conn: &Connection, key: &str, value: &str) -> Result<(), String> {
    save_secret_with_key(conn, key, value, &encryption_key(true)?)
}

fn save_secret_with_key(
    conn: &Connection,
    key: &str,
    value: &str,
    encryption_key: &[u8; 32],
) -> Result<(), String> {
    let cipher = Aes256Gcm::new_from_slice(encryption_key).map_err(|e| e.to_string())?;
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
    decrypt_secret(&value, &encryption_key(false)?).map(Some)
}

fn decrypt_secret(value: &str, encryption_key: &[u8; 32]) -> Result<String, String> {
    let encoded = value
        .strip_prefix(PREFIX)
        .ok_or("Planner storage is not in the protected format.")?;
    let (nonce, ciphertext) = encoded
        .split_once(':')
        .ok_or("Protected planner storage is invalid.")?;
    let nonce: [u8; 12] = BASE64
        .decode(nonce)
        .map_err(|_| "Protected planner nonce is invalid.")?
        .try_into()
        .map_err(|_| "Protected planner nonce is invalid.")?;
    let ciphertext = BASE64
        .decode(ciphertext)
        .map_err(|_| "Protected planner storage is invalid.")?;
    let cipher = Aes256Gcm::new_from_slice(encryption_key).map_err(|e| e.to_string())?;
    let plaintext = cipher
        .decrypt(&nonce.into(), ciphertext.as_ref())
        .map_err(|_| "Could not decrypt the local planner state.")?;
    String::from_utf8(plaintext).map_err(|_| "Protected planner state is invalid UTF-8.".into())
}

#[cfg(all(test, target_os = "macos"))]
mod tests {
    use super::*;
    use security_framework::passwords::{delete_generic_password, get_generic_password};

    #[test]
    fn native_keychain_roundtrip_authenticates_ciphertext_and_does_not_recreate_missing_keys() {
        let service = format!(
            "ai.flowsight.agent.test.{}.{}",
            std::process::id(),
            chrono::Utc::now().timestamp_nanos_opt().unwrap()
        );
        let account = "isolated-test-key";
        struct Cleanup<'a>(&'a str, &'a str);
        impl Drop for Cleanup<'_> {
            fn drop(&mut self) {
                let _ = delete_generic_password(self.0, self.1);
            }
        }
        let _cleanup = Cleanup(&service, account);
        assert!(keychain_key(&service, account, false).is_err());
        assert!(get_generic_password(&service, account).is_err());
        let key = keychain_key(&service, account, true).unwrap();
        assert_eq!(keychain_key(&service, account, false).unwrap(), key);
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch("CREATE TABLE config (key TEXT PRIMARY KEY, value TEXT)")
            .unwrap();
        let plaintext = "Private ADDA session and three breaks";
        save_secret_with_key(&conn, "test", plaintext, &key).unwrap();
        let read = || {
            conn.query_row::<String, _, _>("SELECT value FROM config WHERE key='test'", [], |row| {
                row.get(0)
            })
            .unwrap()
        };
        let first = read();
        assert!(!first.contains(plaintext));
        assert_eq!(decrypt_secret(&first, &key).unwrap(), plaintext);
        save_secret_with_key(&conn, "test", plaintext, &key).unwrap();
        assert_ne!(read(), first, "Each save must use a fresh nonce");
        let encoded = first.strip_prefix(PREFIX).unwrap();
        let (nonce, ciphertext) = encoded.split_once(':').unwrap();
        let mut bytes = BASE64.decode(ciphertext).unwrap();
        bytes[0] ^= 1;
        assert!(
            decrypt_secret(&format!("{PREFIX}{nonce}:{}", BASE64.encode(bytes)), &key).is_err()
        );
        assert!(decrypt_secret("plaintext", &key).is_err());
        delete_generic_password(&service, account).unwrap();
        assert!(keychain_key(&service, account, false).is_err());
        assert!(get_generic_password(&service, account).is_err());
    }
}
