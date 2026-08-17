//! Event-driven foreground window tracking via `SetWinEventHook`.
//!
//! Replaces the old `active-win-pos-rs` polling loop: instead of asking
//! "what's the active window?" every tick, Windows tells us the instant it
//! changes. Runs on its own dedicated thread with its own Win32 message loop
//! (a hard requirement for `SetWinEventHook` callbacks to fire at all).

use super::{push_event, ActionEvent, SharedAppInfo, SharedFlag, SharedRing, SharedTarget};
use std::cell::RefCell;
use std::sync::atomic::Ordering;
use std::time::Instant;
use windows::Win32::Foundation::HWND;
use windows::Win32::System::Threading::{
    OpenProcess, QueryFullProcessImageNameW, PROCESS_NAME_WIN32, PROCESS_QUERY_LIMITED_INFORMATION,
};
use windows::Win32::UI::Accessibility::{SetWinEventHook, UnhookWinEvent, HWINEVENTHOOK};
use windows::Win32::UI::WindowsAndMessaging::{
    DispatchMessageW, GetMessageW, GetWindowTextW, GetWindowThreadProcessId,
    TranslateMessage, EVENT_SYSTEM_FOREGROUND, MSG, WINEVENT_OUTOFCONTEXT,
};

struct ForegroundContext {
    ring: SharedRing,
    running: SharedFlag,
    uia_target: SharedTarget,
    current_app: SharedAppInfo,
}

thread_local! {
    static CONTEXT: RefCell<Option<ForegroundContext>> = const { RefCell::new(None) };
}

pub fn spawn(ring: SharedRing, running: SharedFlag, uia_target: SharedTarget, current_app: SharedAppInfo) {
    std::thread::spawn(move || {
        CONTEXT.with(|c| {
            *c.borrow_mut() = Some(ForegroundContext { ring, running, uia_target, current_app });
        });

        unsafe {
            let hook = SetWinEventHook(
                EVENT_SYSTEM_FOREGROUND,
                EVENT_SYSTEM_FOREGROUND,
                None,
                Some(win_event_proc),
                0,
                0,
                WINEVENT_OUTOFCONTEXT,
            );

            if hook.is_invalid() {
                log::warn!("[Telemetry][Foreground] SetWinEventHook failed; foreground tracking disabled");
                return;
            }

            let mut msg = MSG::default();
            // Required so the OS can dispatch the WinEvent callbacks it queues
            // for this thread (see module docs).
            while GetMessageW(&mut msg, None, 0, 0).as_bool() {
                let _ = TranslateMessage(&msg);
                DispatchMessageW(&msg);
            }

            let _ = UnhookWinEvent(hook);
        }
    });
}

unsafe extern "system" fn win_event_proc(
    _hook: HWINEVENTHOOK,
    event: u32,
    hwnd: HWND,
    id_object: i32,
    id_child: i32,
    _thread_id: u32,
    _timestamp: u32,
) {
    // OBJID_WINDOW == 0, CHILDID_SELF == 0: filters out sub-object noise so we
    // only react to genuine top-level foreground window switches.
    if event != EVENT_SYSTEM_FOREGROUND || hwnd.0.is_null() || id_object != 0 || id_child != 0 {
        return;
    }

    // Windows invokes this callback directly (no Rust frame above it on this
    // call stack), so a panic here would try to unwind straight across a
    // non-Rust `extern "system"` boundary — undefined behavior on Windows.
    // `catch_unwind` keeps any panic (e.g. a poisoned lock) inside Rust-land;
    // we just drop the event and keep the hook alive for the next one.
    let result = std::panic::catch_unwind(|| {
        CONTEXT.with(|c| {
            let borrowed = c.borrow();
            let Some(ctx) = borrowed.as_ref() else { return };

            if !ctx.running.load(Ordering::Relaxed) {
                return;
            }

            let window_title = get_window_title(hwnd);
            let app_name = get_process_name(hwnd).unwrap_or_else(|| "Unknown".to_string());

            *lock_or_recover(&ctx.uia_target) = Some(hwnd.0 as isize);
            *lock_or_recover(&ctx.current_app) = Some(app_name.clone());
            push_event(
                &ctx.ring,
                ActionEvent::ForegroundChanged { app_name, window_title, at: Instant::now() },
            );
        });
    });
    if result.is_err() {
        log::warn!("[Telemetry][Foreground] win_event_proc panicked; event dropped");
    }
}

/// Recovers from a poisoned lock instead of panicking again: telemetry state
/// is best-effort in-memory bookkeeping, not a correctness-critical
/// invariant, so surfacing whatever the previous (caught) panic left behind
/// is preferable to cascading failures on every subsequent callback.
fn lock_or_recover<T>(m: &std::sync::Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    m.lock().unwrap_or_else(|e| e.into_inner())
}

fn get_window_title(hwnd: HWND) -> String {
    let mut buf = [0u16; 512];
    let len = unsafe { GetWindowTextW(hwnd, &mut buf) };
    if len <= 0 {
        return String::new();
    }
    String::from_utf16_lossy(&buf[..len as usize])
}

fn get_process_name(hwnd: HWND) -> Option<String> {
    let mut pid: u32 = 0;
    unsafe { GetWindowThreadProcessId(hwnd, Some(&mut pid)) };
    if pid == 0 {
        return None;
    }

    unsafe {
        let handle = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid).ok()?;
        let mut buf = [0u16; 512];
        let mut size = buf.len() as u32;
        let ok = QueryFullProcessImageNameW(
            handle,
            PROCESS_NAME_WIN32,
            windows::core::PWSTR(buf.as_mut_ptr()),
            &mut size,
        );
        let _ = windows::Win32::Foundation::CloseHandle(handle);

        if ok.is_err() || size == 0 {
            return None;
        }

        let full_path = String::from_utf16_lossy(&buf[..size as usize]);
        full_path
            .rsplit(['\\', '/'])
            .next()
            .map(|s| s.to_string())
            .or(Some(full_path))
    }
}
