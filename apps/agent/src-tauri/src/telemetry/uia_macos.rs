//! macOS Accessibility-backed action signals (Level 0 equivalent of Win32 UIA).
//!
//! Uses the Accessibility framework via raw FFI to observe focused-element
//! changes on the frontmost app. Window open/close is approximated by
//! frontmost-app switches (paired with `action_capture`), which is the strongest
//! reliable signal without requiring fragile per-app AX trees.

use super::action_capture::ActionCaptureTrigger;
use super::{push_event, ActionEvent, SharedAppInfo, SharedFlag, SharedRing, SharedTarget};
use std::ffi::c_void;
use std::sync::atomic::Ordering;
use std::sync::Arc;
use std::time::{Duration, Instant};

const POLL_INTERVAL: Duration = Duration::from_millis(250);

#[link(name = "ApplicationServices", kind = "framework")]
extern "C" {
    fn AXIsProcessTrusted() -> u8;
    fn AXUIElementCreateSystemWide() -> *mut c_void;
    fn AXUIElementCopyAttributeValue(
        element: *mut c_void,
        attribute: *const c_void,
        value: *mut *mut c_void,
    ) -> i32;
    fn CFRelease(cf: *const c_void);
    fn CFStringCreateWithCString(
        alloc: *const c_void,
        c_str: *const i8,
        encoding: u32,
    ) -> *mut c_void;
    fn CFStringGetCString(
        the_string: *const c_void,
        buffer: *mut i8,
        buffer_size: isize,
        encoding: u32,
    ) -> u8;
}

const K_CF_STRING_ENCODING_UTF8: u32 = 0x0800_0100;
const K_AX_ERROR_SUCCESS: i32 = 0;

fn lock_or_recover<T>(m: &std::sync::Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    m.lock().unwrap_or_else(|e| e.into_inner())
}

fn cf_str(s: &str) -> *mut c_void {
    let c = std::ffi::CString::new(s).unwrap_or_default();
    unsafe { CFStringCreateWithCString(std::ptr::null(), c.as_ptr(), K_CF_STRING_ENCODING_UTF8) }
}

fn cf_string_to_rust(cf: *mut c_void) -> Option<String> {
    if cf.is_null() {
        return None;
    }
    let mut buf = [0i8; 1024];
    let ok = unsafe {
        CFStringGetCString(
            cf,
            buf.as_mut_ptr(),
            buf.len() as isize,
            K_CF_STRING_ENCODING_UTF8,
        )
    };
    if ok == 0 {
        return None;
    }
    let cstr = unsafe { std::ffi::CStr::from_ptr(buf.as_ptr()) };
    Some(cstr.to_string_lossy().into_owned())
}

fn copy_ax_attr(element: *mut c_void, attr: &str) -> Option<*mut c_void> {
    let attr_cf = cf_str(attr);
    if attr_cf.is_null() {
        return None;
    }
    let mut value: *mut c_void = std::ptr::null_mut();
    let status = unsafe { AXUIElementCopyAttributeValue(element, attr_cf, &mut value) };
    unsafe { CFRelease(attr_cf) };
    if status != K_AX_ERROR_SUCCESS || value.is_null() {
        return None;
    }
    Some(value)
}

fn focused_control_info(system_wide: *mut c_void) -> (Option<String>, Option<String>) {
    let focused = match copy_ax_attr(system_wide, "AXFocusedUIElement") {
        Some(el) => el,
        None => return (None, None),
    };
    let name = copy_ax_attr(focused, "AXTitle")
        .or_else(|| copy_ax_attr(focused, "AXDescription"))
        .and_then(|v| {
            let s = cf_string_to_rust(v);
            unsafe { CFRelease(v) };
            s
        });
    let role = copy_ax_attr(focused, "AXRole").and_then(|v| {
        let s = cf_string_to_rust(v);
        unsafe { CFRelease(v) };
        s
    });
    unsafe { CFRelease(focused) };
    (name, role)
}

pub fn spawn(
    ring: SharedRing,
    running: SharedFlag,
    _target: SharedTarget,
    current_app: SharedAppInfo,
    trigger: Arc<ActionCaptureTrigger>,
) {
    std::thread::spawn(move || {
        if unsafe { AXIsProcessTrusted() } == 0 {
            log::warn!(
                "[Telemetry][AX] Accessibility permission not granted; \
                 grant FlowSight in System Settings → Privacy & Security → Accessibility"
            );
        }

        let system_wide = unsafe { AXUIElementCreateSystemWide() };
        if system_wide.is_null() {
            log::warn!("[Telemetry][AX] AXUIElementCreateSystemWide failed");
            return;
        }

        let mut last_focus_key: Option<(Option<String>, Option<String>)> = None;
        let mut last_app: Option<String> = None;

        loop {
            if running.load(Ordering::Relaxed) {
                let app_now = lock_or_recover(&current_app).clone();
                if app_now != last_app {
                    if let Some(prev) = last_app.take() {
                        push_event(
                            &ring,
                            ActionEvent::UiaWindowClosed {
                                name: Some(prev),
                                at: Instant::now(),
                            },
                        );
                    }
                    if let Some(ref app) = app_now {
                        push_event(
                            &ring,
                            ActionEvent::UiaWindowOpened {
                                name: Some(app.clone()),
                                at: Instant::now(),
                            },
                        );
                        trigger.trigger(format!(
                            "The user just focused the application '{app}'."
                        ));
                    }
                    last_app = app_now;
                    last_focus_key = None;
                }

                let (name, role) = focused_control_info(system_wide);
                let key = (name.clone(), role.clone());
                if last_focus_key.as_ref() != Some(&key) && (name.is_some() || role.is_some()) {
                    last_focus_key = Some(key);
                    push_event(
                        &ring,
                        ActionEvent::UiaFocusChanged {
                            control_name: name,
                            control_type: role,
                            at: Instant::now(),
                        },
                    );
                }
            } else {
                last_focus_key = None;
                last_app = None;
            }
            std::thread::sleep(POLL_INTERVAL);
        }
    });
}
