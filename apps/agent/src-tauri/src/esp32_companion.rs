//! USB companion: the renderer owns timer semantics and translations. The serial
//! worker only transports bounded snapshots and explicit, deduplicated controls.
use serde_json::{json, Value};
use std::collections::VecDeque;
use std::sync::{Arc, Mutex};
#[cfg(windows)]
use std::time::Duration;
use std::time::Instant;
use tauri::State;
#[cfg(windows)]
use tauri::{Emitter, Manager};

#[derive(Default)]
struct Shared {
    snapshot: Option<(Value, Instant)>,
    replies: VecDeque<Value>,
    connection: u64,
    port: Option<String>,
    device: Option<Value>,
}

#[derive(Clone, Default)]
pub struct Companion(Arc<Mutex<Shared>>);

#[cfg(any(windows, test))]
pub fn valid_action(action: &str) -> bool {
    matches!(
        action,
        "start" | "pause" | "resume" | "stop" | "end_break" | "mode_normal" | "mode_pomodoro"
    )
}

#[tauri::command]
pub fn update_esp32_companion(snapshot: Value, state: State<'_, Companion>) -> Result<(), String> {
    if !snapshot.is_object() || snapshot.to_string().len() > 3000 {
        return Err("Invalid companion snapshot".into());
    }
    let mut shared = state.inner().0.lock().map_err(|e| e.to_string())?;
    shared.snapshot = Some((snapshot, Instant::now()));
    Ok(())
}

#[tauri::command]
pub fn complete_esp32_companion_action(
    connection: u64,
    id: u64,
    ok: bool,
    state: State<'_, Companion>,
) -> Result<(), String> {
    let mut shared = state.inner().0.lock().map_err(|e| e.to_string())?;
    if shared.connection == connection && shared.port.is_some() && shared.replies.len() < 16 {
        shared
            .replies
            .push_back(json!({"cmd":"ack", "id":id, "ok":ok}));
    }
    Ok(())
}

#[tauri::command]
pub fn get_esp32_companion_status(state: State<'_, Companion>) -> Value {
    let shared = state.inner().0.lock().unwrap();
    json!({"supported":cfg!(windows), "connected":shared.port.is_some(), "port":shared.port, "protocol":2, "device":shared.device})
}

pub fn start(app: tauri::AppHandle) {
    #[cfg(windows)]
    if let Err(error) = std::thread::Builder::new()
        .name("flowsight-usb".into())
        .spawn(move || windows::worker(app))
    {
        log::warn!("USB companion unavailable: {error}");
    }
    #[cfg(not(windows))]
    let _ = app;
}

#[cfg(windows)]
mod windows {
    use super::*;
    use std::fs::{File, OpenOptions};
    use std::io::{Read, Write};
    use std::os::windows::fs::OpenOptionsExt;
    use std::os::windows::io::AsRawHandle;
    use winreg::{enums::HKEY_LOCAL_MACHINE, RegKey};

    #[repr(C)]
    #[derive(Default)]
    struct Dcb {
        length: u32,
        baud: u32,
        flags: u32,
        reserved: u16,
        xon_limit: u16,
        xoff_limit: u16,
        byte_size: u8,
        parity: u8,
        stop_bits: u8,
        xon_char: i8,
        xoff_char: i8,
        error_char: i8,
        eof_char: i8,
        event_char: i8,
        reserved1: u16,
    }
    #[repr(C)]
    struct Timeouts {
        interval: u32,
        read_multiplier: u32,
        read_constant: u32,
        write_multiplier: u32,
        write_constant: u32,
    }
    #[link(name = "kernel32")]
    extern "system" {
        fn GetCommState(handle: std::os::windows::io::RawHandle, dcb: *mut Dcb) -> i32;
        fn SetCommState(handle: std::os::windows::io::RawHandle, dcb: *const Dcb) -> i32;
        fn SetCommTimeouts(
            handle: std::os::windows::io::RawHandle,
            timeouts: *const Timeouts,
        ) -> i32;
        fn EscapeCommFunction(handle: std::os::windows::io::RawHandle, function: u32) -> i32;
    }

    fn ports() -> Vec<String> {
        // Limit probing to USB adapters used by supported ESP32 boards.
        let Ok(usb) =
            RegKey::predef(HKEY_LOCAL_MACHINE).open_subkey("SYSTEM\\CurrentControlSet\\Enum\\USB")
        else {
            return vec![];
        };
        let mut ports = vec![];
        for name in usb.enum_keys().filter_map(Result::ok).filter(|name| {
            name.starts_with("VID_1A86&PID_7523")
                || name.starts_with("VID_10C4&PID_EA60")
                || name.starts_with("VID_303A&PID_1001")
        }) {
            let Ok(device) = usb.open_subkey(name) else {
                continue;
            };
            for instance in device.enum_keys().filter_map(Result::ok) {
                let Ok(parameters) = device.open_subkey(format!("{instance}\\Device Parameters"))
                else {
                    continue;
                };
                if let Ok(port) = parameters.get_value::<String, _>("PortName") {
                    if port.starts_with("COM")
                        && port.len() > 3
                        && port[3..].bytes().all(|b| b.is_ascii_digit())
                    {
                        ports.push(port)
                    }
                }
            }
        }
        ports.sort();
        ports.dedup();
        ports
    }
    fn open(port: &str) -> std::io::Result<File> {
        let file = OpenOptions::new()
            .read(true)
            .write(true)
            .share_mode(0)
            .open(format!("\\\\.\\{port}"))?;
        let handle = file.as_raw_handle();
        let mut dcb = Dcb {
            length: std::mem::size_of::<Dcb>() as u32,
            ..Default::default()
        };
        // DTR and RTS remain inactive: attaching the app must not reset the ESP32.
        unsafe {
            if GetCommState(handle, &mut dcb) == 0 {
                return Err(std::io::Error::last_os_error());
            }
            dcb.baud = 115200;
            dcb.flags = 1;
            dcb.byte_size = 8;
            dcb.parity = 0;
            dcb.stop_bits = 0;
            if SetCommState(handle, &dcb) == 0 {
                return Err(std::io::Error::last_os_error());
            }
            let timeouts = Timeouts {
                interval: u32::MAX,
                read_multiplier: 0,
                read_constant: 50,
                write_multiplier: 0,
                write_constant: 400,
            };
            if SetCommTimeouts(handle, &timeouts) == 0 {
                return Err(std::io::Error::last_os_error());
            }
            EscapeCommFunction(handle, 6);
            EscapeCommFunction(handle, 4);
        }
        Ok(file)
    }
    fn send(file: &mut File, value: &Value) -> std::io::Result<()> {
        let mut bytes = serde_json::to_vec(value)?;
        bytes.push(b'\n');
        file.write_all(&bytes)
    }
    fn receive(file: &mut File, line: &mut Vec<u8>) -> std::io::Result<Vec<Value>> {
        let mut bytes = [0u8; 512];
        let n = file.read(&mut bytes)?;
        let mut messages = vec![];
        for &byte in &bytes[..n] {
            if byte == b'\n' {
                if line.len() <= 4096 {
                    if let Ok(value) = serde_json::from_slice(line) {
                        messages.push(value)
                    }
                }
                line.clear();
            } else if line.len() <= 4096 {
                line.push(byte)
            }
        }
        Ok(messages)
    }
    fn identify(file: &mut File) -> bool {
        if send(file, &json!({"cmd":"hello"})).is_err() {
            return false;
        }
        let until = Instant::now() + Duration::from_millis(1200);
        let mut line = vec![];
        while Instant::now() < until {
            let Ok(messages) = receive(file, &mut line) else {
                return false;
            };
            if messages.iter().any(|m| {
                m["event"] == "hello" && m["product"] == "flowsight-esp32" && m["protocol"] == 2
            }) {
                return true;
            }
        }
        false
    }
    fn connected(
        app: &tauri::AppHandle,
        state: &Companion,
        port: &str,
        file: &mut File,
    ) -> std::io::Result<()> {
        let connection = {
            let mut s = state.0.lock().unwrap();
            s.connection += 1;
            s.port = Some(port.into());
            s.replies.clear();
            s.snapshot = None;
            s.device = None;
            s.connection
        };
        let mut line = vec![];
        let mut last_id = 0;
        let mut refresh = Instant::now() - Duration::from_secs(1);
        let mut received = Instant::now();
        loop {
            if refresh.elapsed() >= Duration::from_millis(500) {
                app.emit("esp32-companion-refresh", ()).ok();
                let snapshot = { state.0.lock().unwrap().snapshot.clone() };
                if let Some((snapshot, at)) =
                    snapshot.filter(|(_, at)| at.elapsed() < Duration::from_secs(4))
                {
                    let _ = at;
                    send(file, &json!({"cmd":"sync","state":snapshot}))?;
                } else {
                    send(file, &json!({"cmd":"hello"}))?;
                }
                refresh = Instant::now();
            }
            let replies = {
                state
                    .0
                    .lock()
                    .unwrap()
                    .replies
                    .drain(..)
                    .collect::<Vec<_>>()
            };
            for reply in replies {
                send(file, &reply)?
            }
            for message in receive(file, &mut line)? {
                if matches!(message["event"].as_str(), Some("synced" | "hello")) {
                    received = Instant::now();
                    state.0.lock().unwrap().device = Some(message.clone());
                }
                if message["event"] != "control" {
                    continue;
                }
                let id = message["id"].as_u64().unwrap_or(0);
                let action = message["action"].as_str().unwrap_or("");
                if id <= last_id || !valid_action(action) {
                    continue;
                }
                last_id = id;
                app.emit(
                    "esp32-companion-control",
                    json!({"connection":connection,"id":id,"action":action}),
                )
                .ok();
            }
            if received.elapsed() > Duration::from_secs(8) {
                return Err(std::io::Error::new(
                    std::io::ErrorKind::TimedOut,
                    "USB heartbeat lost",
                ));
            }
        }
    }
    pub fn worker(app: tauri::AppHandle) {
        let state = app.state::<Companion>().inner().clone();
        loop {
            for port in ports() {
                if let Ok(mut file) = open(&port) {
                    if identify(&mut file) {
                        let _ = connected(&app, &state, &port, &mut file);
                        let mut s = state.0.lock().unwrap();
                        s.port = None;
                        s.snapshot = None;
                        s.device = None;
                        s.replies.clear();
                    }
                }
            }
            std::thread::sleep(Duration::from_secs(3));
        }
    }
    #[test]
    fn windows_serial_struct_layout() {
        assert_eq!(std::mem::size_of::<Dcb>(), 28);
        assert_eq!(std::mem::size_of::<Timeouts>(), 20);
    }
}

#[test]
fn only_explicit_timer_actions_are_allowed() {
    for action in [
        "start",
        "pause",
        "resume",
        "stop",
        "end_break",
        "mode_normal",
        "mode_pomodoro",
    ] {
        assert!(valid_action(action));
    }
    for action in ["toggle", "reset", "erase", "send", "", "start\npause"] {
        assert!(!valid_action(action));
    }
}
