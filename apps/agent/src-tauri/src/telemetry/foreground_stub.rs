//! Non-Windows / non-macOS stub — telemetry foreground tracking disabled.

use super::{SharedAppInfo, SharedFlag, SharedRing, SharedTarget};

pub fn spawn(
    _app_handle: tauri::AppHandle,
    _ring: SharedRing,
    _running: SharedFlag,
    _privacy_blocked: SharedFlag,
    _uia_target: SharedTarget,
    _current_app: SharedAppInfo,
    _db_path: std::path::PathBuf,
) {
    log::warn!("[Telemetry][Foreground] unsupported platform; tracking disabled");
}
