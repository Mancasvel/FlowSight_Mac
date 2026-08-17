//! Foreground window tracking on macOS via `active-win-pos-rs` polling.
//!
//! Windows uses `SetWinEventHook`; macOS has no equivalent without Accessibility
//! privileges. Polling the active window every ~400ms is reliable, low-cost, and
//! works once Screen Recording / Accessibility permissions are granted.

use super::{push_event, ActionEvent, SharedAppInfo, SharedFlag, SharedRing, SharedTarget};
use active_win_pos_rs::get_active_window;
use std::sync::atomic::Ordering;
use std::time::{Duration, Instant};

const POLL_INTERVAL: Duration = Duration::from_millis(400);

pub fn spawn(
    ring: SharedRing,
    running: SharedFlag,
    uia_target: SharedTarget,
    current_app: SharedAppInfo,
) {
    std::thread::spawn(move || {
        let mut last_key: Option<(String, String)> = None;
        loop {
            if running.load(Ordering::Relaxed) {
                match get_active_window() {
                    Ok(window) => {
                        let app_name = window.app_name.clone();
                        let window_title = window.title.clone();
                        let key = (app_name.clone(), window_title.clone());
                        if last_key.as_ref() != Some(&key) {
                            last_key = Some(key);
                            // Store a stable-ish process id for AX observers (best-effort).
                            *lock_or_recover(&uia_target) = Some(window.process_id as isize);
                            *lock_or_recover(&current_app) = Some(app_name.clone());
                            push_event(
                                &ring,
                                ActionEvent::ForegroundChanged {
                                    app_name,
                                    window_title,
                                    at: Instant::now(),
                                },
                            );
                        }
                    }
                    Err(_) => {
                        // Permissions missing or no focused window — stay quiet.
                    }
                }
            } else {
                last_key = None;
            }
            std::thread::sleep(POLL_INTERVAL);
        }
    });
}

fn lock_or_recover<T>(m: &std::sync::Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    m.lock().unwrap_or_else(|e| e.into_inner())
}
