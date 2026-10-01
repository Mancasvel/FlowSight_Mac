//! Application exclusions used by local focus reminders. No cloud disclosure.
use std::path::Path;

pub fn normalized_application(value: &str) -> String {
    let name = value
        .trim()
        .rsplit(['/', '\\'])
        .next()
        .unwrap_or(value)
        .to_ascii_lowercase();
    name.trim_end_matches(".exe")
        .trim_end_matches(".app")
        .to_string()
}

pub fn application_is_excluded(db_path: &Path, application: Option<&str>) -> bool {
    let Some(application) = application else {
        return true;
    };
    let name = normalized_application(application);
    if name.is_empty()
        || [
            "1password",
            "bitwarden",
            "keepass",
            "lastpass",
            "keychain access",
            "securityagent",
            "loginwindow",
        ]
        .iter()
        .any(|excluded| name.contains(excluded))
    {
        return true;
    }
    let configured = rusqlite::Connection::open(db_path)
        .ok()
        .and_then(|conn| {
            conn.query_row::<String, _, _>(
                "SELECT value FROM config WHERE key='excluded_applications'",
                [],
                |r| r.get(0),
            )
            .ok()
        })
        .and_then(|value| serde_json::from_str::<Vec<String>>(&value).ok())
        .unwrap_or_default();
    configured
        .iter()
        .any(|entry| normalized_application(entry) == name)
}
