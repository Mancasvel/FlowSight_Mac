//! UI Automation action events, scoped to the current foreground window.
//!
//! KNOWN LIMITATION (documented per architecture spec): this is a best-effort
//! implementation. It robustly covers:
//!   - `AutomationFocusChanged` (UI focus moving between controls), and
//!   - `Window_WindowOpened` / `Window_WindowClosed`, scoped to the subtree of
//!     whatever window is currently in the foreground.
//!
//! It deliberately does NOT implement `Invoke_Invoked`, `SelectionItem_*`, or
//! `Text_TextChanged` handlers. Reasoning: UIA event handlers in Rust require
//! implementing out-of-process COM callback interfaces (`#[implement]` from
//! windows-rs) that fire on an STA thread pumping a Win32 message loop, while
//! simultaneously needing to re-subscribe every time the foreground window
//! changes (itself driven from a *different* thread). Getting Invoke/
//! SelectionItem callbacks to be reliable across arbitrary third-party apps'
//! (often inconsistent) UIA trees, without ever risking a COM re-entrancy
//! panic or a hung STA thread, needs significantly more hardening (retry
//! policy around `RemoveAutomationEventHandler`/`AddAutomationEventHandler`
//! races, per-app quirks, etc.) than is safe to ship in a first pass. Given
//! FocusChanged + WindowOpened/Closed already provide meaningful signal for
//! the Level 1 heuristic, we ship that now and leave Invoke/SelectionItem/
//! Text as documented follow-up work (see final report).
//!
//! Every COM call is wrapped and its `Result` is logged-and-dropped on
//! failure — this thread must never panic or hang the process even if UIA is
//! unavailable/misbehaving for a given foreground app.

use super::action_capture::ActionCaptureTrigger;
use super::{push_event, ActionEvent, SharedAppInfo, SharedFlag, SharedRing, SharedTarget};
use std::sync::atomic::Ordering;
use std::sync::Arc;
use std::time::{Duration, Instant};
use windows::core::{Ref, Result as WinResult};
use windows::Win32::Foundation::{E_FAIL, HWND};
use windows::Win32::System::Com::{
    CoCreateInstance, CoInitializeEx, CoUninitialize, CLSCTX_INPROC_SERVER, COINIT_APARTMENTTHREADED,
};
use windows::Win32::UI::Accessibility::{
    CUIAutomation, IUIAutomation, IUIAutomationElement, IUIAutomationEventHandler,
    IUIAutomationEventHandler_Impl, IUIAutomationFocusChangedEventHandler,
    IUIAutomationFocusChangedEventHandler_Impl, TreeScope_Subtree, UIA_EVENT_ID,
    UIA_Window_WindowClosedEventId, UIA_Window_WindowOpenedEventId,
};
use windows::Win32::UI::WindowsAndMessaging::{DispatchMessageW, PeekMessageW, TranslateMessage, MSG, PM_REMOVE};

const POLL_INTERVAL: Duration = Duration::from_millis(150);

/// Recovers from a poisoned lock instead of panicking again: telemetry state
/// is best-effort in-memory bookkeeping, not a correctness-critical
/// invariant, so surfacing whatever a previous (now-caught, see
/// `HandleFocusChangedEvent`/`HandleAutomationEvent`) panic left behind is
/// preferable to cascading failures on every subsequent callback.
fn lock_or_recover<T>(m: &std::sync::Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    m.lock().unwrap_or_else(|e| e.into_inner())
}

#[windows::core::implement(IUIAutomationFocusChangedEventHandler)]
struct FocusHandler {
    ring: SharedRing,
    running: SharedFlag,
}

impl IUIAutomationFocusChangedEventHandler_Impl for FocusHandler_Impl {
    fn HandleFocusChangedEvent(&self, sender: Ref<'_, IUIAutomationElement>) -> WinResult<()> {
        if !self.running.load(Ordering::Relaxed) {
            return Ok(());
        }

        // windows-rs's generated vtable thunk calls this method directly from
        // COM (no Rust frame above it), so a panic here would unwind across
        // an `extern "system"` boundary — UB on Windows. Catch it and report
        // a normal HRESULT failure instead of letting it escape.
        let ring = &self.ring;
        let outcome = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let element: Option<&IUIAutomationElement> = sender.as_ref();
            let control_name = element.and_then(|e| unsafe { e.CurrentName().ok() }).map(|b| b.to_string());
            let control_type = element
                .and_then(|e| unsafe { e.CurrentLocalizedControlType().ok() })
                .map(|b| b.to_string());

            push_event(
                ring,
                ActionEvent::UiaFocusChanged { control_name, control_type, at: Instant::now() },
            );
        }));

        match outcome {
            Ok(()) => Ok(()),
            Err(_) => {
                log::warn!("[Telemetry][UIA] HandleFocusChangedEvent panicked; event dropped");
                Err(windows::core::Error::from(E_FAIL))
            }
        }
    }
}

#[windows::core::implement(IUIAutomationEventHandler)]
struct WindowLifecycleHandler {
    ring: SharedRing,
    running: SharedFlag,
    current_app: SharedAppInfo,
    trigger: Arc<ActionCaptureTrigger>,
}

impl IUIAutomationEventHandler_Impl for WindowLifecycleHandler_Impl {
    fn HandleAutomationEvent(
        &self,
        sender: Ref<'_, IUIAutomationElement>,
        eventid: UIA_EVENT_ID,
    ) -> WinResult<()> {
        if !self.running.load(Ordering::Relaxed) {
            return Ok(());
        }

        // Same reasoning as `HandleFocusChangedEvent`: this is invoked
        // directly by the windows-rs vtable thunk from COM, so a panic here
        // must never be allowed to unwind across that `extern "system"`
        // boundary (UB on Windows) — catch it and fail the call cleanly.
        let outcome = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let name = sender.as_ref().and_then(|e| unsafe { e.CurrentName().ok() }).map(|b| b.to_string());
            let at = Instant::now();
            let is_opened = eventid == UIA_Window_WindowOpenedEventId;
            let event = if is_opened {
                ActionEvent::UiaWindowOpened { name: name.clone(), at }
            } else {
                ActionEvent::UiaWindowClosed { name: name.clone(), at }
            };
            push_event(&self.ring, event);

            // Window open/close is the "significant action" that drives a
            // Steps-Recorder-style screenshot (see `action_capture`).
            // `AutomationFocusChanged` is deliberately excluded as a trigger —
            // it fires far too often to screenshot on every occurrence — and
            // stays purely textual context for the 60s aggregator instead.
            let app_name = lock_or_recover(&self.current_app).clone().unwrap_or_else(|| "an application".to_string());
            let verb = if is_opened { "opened" } else { "closed" };
            let window_desc = name.as_deref().map(|n| format!(" named '{}'", n)).unwrap_or_default();
            self.trigger.trigger(format!(
                "The user just {verb} a window{window_desc} in the application '{app_name}'."
            ));
        }));

        match outcome {
            Ok(()) => Ok(()),
            Err(_) => {
                log::warn!("[Telemetry][UIA] HandleAutomationEvent panicked; event dropped");
                Err(windows::core::Error::from(E_FAIL))
            }
        }
    }
}

pub fn spawn(ring: SharedRing, running: SharedFlag, target: SharedTarget, current_app: SharedAppInfo, trigger: Arc<ActionCaptureTrigger>) {
    std::thread::spawn(move || run(ring, running, target, current_app, trigger));
}

fn run(ring: SharedRing, running: SharedFlag, target: SharedTarget, current_app: SharedAppInfo, trigger: Arc<ActionCaptureTrigger>) {
    unsafe {
        if let Err(e) = CoInitializeEx(None, COINIT_APARTMENTTHREADED).ok() {
            log::warn!("[Telemetry][UIA] CoInitializeEx failed ({e}); UIA action tracking disabled");
            return;
        }
    }

    let automation: IUIAutomation = match unsafe { CoCreateInstance(&CUIAutomation, None, CLSCTX_INPROC_SERVER) } {
        Ok(a) => a,
        Err(e) => {
            log::warn!("[Telemetry][UIA] Failed to create IUIAutomation instance ({e}); UIA action tracking disabled");
            unsafe { CoUninitialize() };
            return;
        }
    };

    let focus_handler: IUIAutomationFocusChangedEventHandler =
        FocusHandler { ring: ring.clone(), running: running.clone() }.into();
    if let Err(e) = unsafe { automation.AddFocusChangedEventHandler(None, &focus_handler) } {
        log::warn!("[Telemetry][UIA] Failed to register focus-changed handler ({e}); focus events disabled");
    }

    let window_handler: IUIAutomationEventHandler =
        WindowLifecycleHandler { ring, running, current_app, trigger }.into();

    let mut current_target: Option<isize> = None;
    let mut current_element: Option<IUIAutomationElement> = None;

    loop {
        pump_messages();

        let desired = *target.lock().unwrap();
        if desired != current_target {
            if let Some(el) = current_element.take() {
                unbind_window_events(&automation, &el, &window_handler);
            }
            current_target = desired;
            current_element = desired.and_then(|raw| bind_window_events(&automation, raw, &window_handler));
        }

        std::thread::sleep(POLL_INTERVAL);
    }
}

fn bind_window_events(
    automation: &IUIAutomation,
    raw_hwnd: isize,
    handler: &IUIAutomationEventHandler,
) -> Option<IUIAutomationElement> {
    let hwnd = HWND(raw_hwnd as *mut _);
    let element = match unsafe { automation.ElementFromHandle(hwnd) } {
        Ok(el) => el,
        Err(e) => {
            log::debug!("[Telemetry][UIA] ElementFromHandle failed, degrading gracefully: {e}");
            return None;
        }
    };

    if let Err(e) = unsafe {
        automation.AddAutomationEventHandler(UIA_Window_WindowOpenedEventId, &element, TreeScope_Subtree, None, handler)
    } {
        log::debug!("[Telemetry][UIA] AddAutomationEventHandler(WindowOpened) failed: {e}");
    }
    if let Err(e) = unsafe {
        automation.AddAutomationEventHandler(UIA_Window_WindowClosedEventId, &element, TreeScope_Subtree, None, handler)
    } {
        log::debug!("[Telemetry][UIA] AddAutomationEventHandler(WindowClosed) failed: {e}");
    }

    Some(element)
}

fn unbind_window_events(automation: &IUIAutomation, element: &IUIAutomationElement, handler: &IUIAutomationEventHandler) {
    if let Err(e) = unsafe { automation.RemoveAutomationEventHandler(UIA_Window_WindowOpenedEventId, element, handler) } {
        log::debug!("[Telemetry][UIA] RemoveAutomationEventHandler(WindowOpened) failed: {e}");
    }
    if let Err(e) = unsafe { automation.RemoveAutomationEventHandler(UIA_Window_WindowClosedEventId, element, handler) } {
        log::debug!("[Telemetry][UIA] RemoveAutomationEventHandler(WindowClosed) failed: {e}");
    }
}

fn pump_messages() {
    unsafe {
        let mut msg = MSG::default();
        while PeekMessageW(&mut msg, None, 0, 0, PM_REMOVE).as_bool() {
            let _ = TranslateMessage(&msg);
            DispatchMessageW(&msg);
        }
    }
}
