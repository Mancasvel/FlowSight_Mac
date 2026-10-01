//! Language is a presentation preference, separate from tracking and consent.
use rusqlite::{params, Connection, OptionalExtension};
use serde::Serialize;
use std::sync::atomic::{AtomicBool, Ordering};

const KEY: &str = "app_language_preference";
static SPANISH: AtomicBool = AtomicBool::new(false);
pub fn is_spanish() -> bool {
    SPANISH.load(Ordering::Relaxed)
}
pub fn copy<'a>(english: &'a str, spanish: &'a str) -> &'a str {
    copy_for(if is_spanish() { "es" } else { "en" }, english, spanish)
}
pub fn copy_for<'a>(language: &str, english: &'a str, spanish: &'a str) -> &'a str {
    if language == "es" {
        spanish
    } else {
        english
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LanguagePreference {
    pub preference: String,
    pub language: String,
    pub system_language: String,
    pub persisted: bool,
}

fn system_language() -> &'static str {
    #[cfg(windows)]
    {
        #[link(name = "kernel32")]
        extern "system" {
            fn GetUserDefaultUILanguage() -> u16;
        }
        // All Spanish Windows language variants share primary LANGID 0x0a.
        if unsafe { GetUserDefaultUILanguage() } & 0x03ff == 0x000a {
            return "es";
        }
    }
    #[cfg(not(windows))]
    if sys_locale::get_locale().is_some_and(|locale| {
        locale
            .split(['-', '_'])
            .next()
            .is_some_and(|primary| primary.eq_ignore_ascii_case("es"))
    }) {
        return "es";
    }
    "en"
}
fn resolve(preference: &str, system: &str) -> &'static str {
    if preference == "es" || (preference == "system" && system == "es") {
        "es"
    } else {
        "en"
    }
}
fn read() -> Result<LanguagePreference, String> {
    let conn = Connection::open(crate::paths::db_path()?).map_err(|e| e.to_string())?;
    let result = read_from(&conn, system_language())?;
    SPANISH.store(result.language == "es", Ordering::Relaxed);
    Ok(result)
}
fn read_from(conn: &Connection, system: &str) -> Result<LanguagePreference, String> {
    let saved: Option<String> = conn
        .query_row(
            "SELECT value FROM config WHERE key=?1",
            params![KEY],
            |row| row.get(0),
        )
        .optional()
        .map_err(|e| e.to_string())?;
    let persisted = saved.is_some();
    let preference = saved
        .filter(|v| matches!(v.as_str(), "es" | "en" | "system"))
        .unwrap_or_else(|| "system".into());
    let language = resolve(&preference, system).to_string();
    Ok(LanguagePreference {
        preference,
        language,
        system_language: system.into(),
        persisted,
    })
}
fn write_to(conn: &Connection, preference: &str) -> Result<(), String> {
    conn.execute(
        "INSERT OR REPLACE INTO config(key,value) VALUES(?1,?2)",
        params![KEY, preference],
    )
    .map_err(|e| e.to_string())?;
    Ok(())
}
pub fn initialize() {
    SPANISH.store(system_language() == "es", Ordering::Relaxed);
    let _ = read();
}

#[tauri::command]
pub fn get_language_preference() -> Result<LanguagePreference, String> {
    read()
}

#[tauri::command]
pub fn set_language_preference(
    app: tauri::AppHandle,
    preference: String,
) -> Result<LanguagePreference, String> {
    if !matches!(preference.as_str(), "es" | "en" | "system") {
        return Err("Unsupported language preference.".into());
    }
    let language = resolve(&preference, system_language()).to_string();
    let conn = Connection::open(crate::paths::db_path()?).map_err(|e| e.to_string())?;
    write_to(&conn, &preference)?;
    SPANISH.store(language == "es", Ordering::Relaxed);
    #[cfg(desktop)]
    if let Some(tray) = app.tray_by_id("flowsight-tray") {
        use tauri::menu::{Menu, MenuItem};
        let open = MenuItem::with_id(
            &app,
            "open",
            copy("Open FlowSight", "Abrir FlowSight"),
            true,
            None::<&str>,
        )
        .map_err(|e| e.to_string())?;
        let quit = MenuItem::with_id(
            &app,
            "quit",
            copy("Quit FlowSight", "Salir de FlowSight"),
            true,
            None::<&str>,
        )
        .map_err(|e| e.to_string())?;
        tray.set_menu(Some(
            Menu::with_items(&app, &[&open, &quit]).map_err(|e| e.to_string())?,
        ))
        .map_err(|e| e.to_string())?;
    }
    Ok(LanguagePreference {
        preference,
        language,
        system_language: system_language().into(),
        persisted: true,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn explicit_choice_overrides_system_language() {
        assert_eq!(resolve("system", "es"), "es");
        assert_eq!(resolve("system", "fr"), "en");
        assert_eq!(resolve("en", "es"), "en");
        assert_eq!(resolve("es", "en"), "es");
    }
    #[test]
    fn persistence_preserves_other_preferences_and_system_language() {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch("CREATE TABLE config(key TEXT PRIMARY KEY,value TEXT); INSERT INTO config VALUES('devName','Review');").unwrap();
        let initial = read_from(&conn, "es").unwrap();
        assert_eq!(initial.preference, "system");
        assert_eq!(initial.language, "es");
        assert!(!initial.persisted);
        write_to(&conn, "en").unwrap();
        let saved = read_from(&conn, "es").unwrap();
        assert_eq!(saved.language, "en");
        assert_eq!(saved.system_language, "es");
        assert!(saved.persisted);
        write_to(&conn, "system").unwrap();
        assert_eq!(read_from(&conn, "es").unwrap().language, "es");
        let name: String = conn
            .query_row("SELECT value FROM config WHERE key='devName'", [], |row| {
                row.get(0)
            })
            .unwrap();
        assert_eq!(name, "Review");
    }
}
