//! Keep the existing Windows startup choice attached to the installed executable.

use std::path::Path;
use winreg::{enums::HKEY_CURRENT_USER, enums::KEY_READ, enums::KEY_SET_VALUE, RegKey};

const RUN_KEY: &str = r"Software\Microsoft\Windows\CurrentVersion\Run";
const APPROVED_KEY: &str =
    r"Software\Microsoft\Windows\CurrentVersion\Explorer\StartupApproved\Run";
const APP_NAME: &str = "FlowSight Agent";

pub fn is_development_executable(executable: &Path) -> bool {
    let components: Vec<_> = executable
        .components()
        .map(|part| part.as_os_str().to_string_lossy().to_ascii_lowercase())
        .collect();
    components
        .windows(2)
        .any(|parts| parts[0] == "target" && matches!(parts[1].as_str(), "debug" | "release"))
}

fn startup_command(executable: &Path) -> String {
    format!("\"{}\" --flowsight-autostart", executable.display())
}

pub fn repair_existing(executable: &Path) -> Result<bool, String> {
    if is_development_executable(executable) {
        return Ok(false);
    }
    let current_user = RegKey::predef(HKEY_CURRENT_USER);
    let key = current_user
        .open_subkey_with_flags(RUN_KEY, KEY_READ | KEY_SET_VALUE)
        .map_err(|error| error.to_string())?;
    repair_in_key(&key, executable)
}

fn repair_in_key(key: &RegKey, executable: &Path) -> Result<bool, String> {
    let existing = match key.get_value::<String, _>(APP_NAME) {
        Ok(value) => value,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(false),
        Err(error) => return Err(error.to_string()),
    };
    let expected = startup_command(executable);
    if existing == expected {
        return Ok(false);
    }
    // Do not touch StartupApproved: a choice made in Task Manager stays intact.
    key.set_value(APP_NAME, &expected)
        .map_err(|error| error.to_string())?;
    Ok(true)
}

pub fn enable(executable: &Path) -> Result<(), String> {
    if is_development_executable(executable) {
        return Err("Install the release build before enabling launch at login.".into());
    }
    let current_user = RegKey::predef(HKEY_CURRENT_USER);
    current_user
        .open_subkey_with_flags(RUN_KEY, KEY_SET_VALUE)
        .and_then(|key| key.set_value(APP_NAME, &startup_command(executable)))
        .map_err(|error| error.to_string())?;
    // An explicit opt-in can re-enable the user's Task Manager startup choice.
    if let Ok(key) = current_user.open_subkey_with_flags(APPROVED_KEY, KEY_SET_VALUE) {
        key.set_raw_value(
            APP_NAME,
            &winreg::RegValue {
                vtype: winreg::enums::RegType::REG_BINARY,
                bytes: vec![2, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0],
            },
        )
        .map_err(|error| error.to_string())?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn startup_command_quotes_installation_paths_with_spaces() {
        assert_eq!(
            startup_command(Path::new(
                r"C:\Users\Some User\AppData\Local\FlowSight Agent\app.exe"
            )),
            r#""C:\Users\Some User\AppData\Local\FlowSight Agent\app.exe" --flowsight-autostart"#
        );
    }

    #[test]
    fn cargo_release_builds_cannot_replace_the_installed_startup_entry() {
        assert!(is_development_executable(Path::new(
            r"C:\source\apps\agent\src-tauri\target\release\app.exe"
        )));
        assert!(is_development_executable(Path::new(
            r"C:\source\TARGET\DEBUG\app.exe"
        )));
        assert!(!is_development_executable(Path::new(
            r"C:\Users\Some User\AppData\Local\FlowSight Agent\app.exe"
        )));
    }

    #[test]
    fn repairing_a_stale_entry_is_idempotent_and_never_enables_missing_startup() {
        let current_user = RegKey::predef(HKEY_CURRENT_USER);
        let fixture = format!(r"Software\FlowSight\AutostartTest-{}", uuid::Uuid::new_v4());
        let (key, _) = current_user.create_subkey(&fixture).unwrap();
        let executable = Path::new(r"C:\Users\Some User\AppData\Local\FlowSight Agent\app.exe");
        assert!(!repair_in_key(&key, executable).unwrap());
        assert!(key.get_value::<String, _>(APP_NAME).is_err());
        key.set_value(
            APP_NAME,
            &r"C:\source\target\release\app.exe --flowsight-autostart",
        )
        .unwrap();
        assert!(repair_in_key(&key, executable).unwrap());
        assert_eq!(
            key.get_value::<String, _>(APP_NAME).unwrap(),
            startup_command(executable)
        );
        assert!(!repair_in_key(&key, executable).unwrap());
        drop(key);
        current_user.delete_subkey(&fixture).unwrap();
    }
}
