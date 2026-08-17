//! Non-Windows / non-macOS stub — telemetry foreground tracking disabled.

use super::{SharedAppInfo, SharedFlag, SharedRing, SharedTarget};

pub fn spawn(
    _ring: SharedRing,
    _running: SharedFlag,
    _uia_target: SharedTarget,
    _current_app: SharedAppInfo,
) {
    log::warn!("[Telemetry][Foreground] unsupported platform; tracking disabled");
}
