//! Non-Windows / non-macOS stub — UI action tracking disabled.

use super::action_capture::ActionCaptureTrigger;
use super::{SharedAppInfo, SharedFlag, SharedRing, SharedTarget};
use std::sync::Arc;

pub fn spawn(
    _ring: SharedRing,
    _running: SharedFlag,
    _target: SharedTarget,
    _current_app: SharedAppInfo,
    _trigger: Arc<ActionCaptureTrigger>,
) {
    log::warn!("[Telemetry][UIA] unsupported platform; action tracking disabled");
}
