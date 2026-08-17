//! Puerto HTTP del llama-server gestionado por la app — **no** fijo (evita 8080 / proxies).
//!
//! ## TOCTOU (liberar listener y luego que `llama-server` haga `bind`)
//!
//! No podemos pasar un FD a `llama-server` sin soporte upstream, así que tras elegir un
//! puerto siempre hay una ventana donde otro proceso podría enlazarlo antes que el hijo.
//! Mitigaciones aquí:
//!
//! 1. Preferir el rango **`FLOWSIGHT_PORT_MIN`..=`FLOWSIGHT_PORT_MAX`**, fuera del rango éphemeral
//!    típico de conexiones salientes del kernel (p. ej. 32768–60999 en Linux), reduciendo
//!    colisiones con puertos “robados” por clientes efímeros.
//! 2. Tras el primer `bind` de prueba, un **segundo `bind` inmediato** detecta si el puerto
//!    fue reclamado entre el drop y el siguiente intento (rechaza y prueba otro).
//! 3. Fallback a `127.0.0.1:0` con **reintentos y backoff** si el rango está lleno.
//! 4. `spawn_llama_managed_child` **reintenta** con otro puerto ante `EADDRINUSE` / log de fallo
//!    de escucha (ver `agent.rs`).
//!
//! Todas las URLs usan **`127.0.0.1`**, nunca `localhost`, para alinear cliente y `--host` del
//! servidor (IPv4 estable frente a `::1`).

use std::io;
use std::net::{Ipv4Addr, TcpListener};
use std::sync::Mutex;
use std::thread;
use std::time::Duration;

static MANAGED_LLAMA_PORT: Mutex<Option<u16>> = Mutex::new(None);

/// Por debajo del rango éphemeral habitual de muchos Linux; alejado de 3000/5000/8000/8080.
const FLOWSIGHT_PORT_MIN: u16 = 40_000;
const FLOWSIGHT_PORT_MAX: u16 = 44_999;

const BIND0_MAX_RETRIES: u8 = 6;

fn lock_managed_port<'a>() -> std::sync::MutexGuard<'a, Option<u16>> {
    match MANAGED_LLAMA_PORT.lock() {
        Ok(g) => g,
        Err(poisoned) => {
            log::warn!(
                "[FlowSight] MANAGED_LLAMA_PORT mutex poisoned; recovering inner value: {}",
                poisoned
            );
            poisoned.into_inner()
        }
    }
}

/// `true` si el error suele indicar que el puerto TCP local ya está en uso (Windows + Unix).
pub(crate) fn tcp_bind_addr_in_use(err: &io::Error) -> bool {
    matches!(err.kind(), io::ErrorKind::AddrInUse)
        || err.raw_os_error() == Some(10048) // WSAEADDRINUSE
}

fn probe_double_bind_then_release(port: u16) -> Result<(), io::Error> {
    let first = TcpListener::bind((Ipv4Addr::LOCALHOST, port))?;
    drop(first);
    let second = TcpListener::bind((Ipv4Addr::LOCALHOST, port))?;
    drop(second);
    Ok(())
}

fn pick_port_in_preferred_range() -> Option<u16> {
    let span = u32::from(FLOWSIGHT_PORT_MAX - FLOWSIGHT_PORT_MIN + 1);
    let seed = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos() as u32)
        .unwrap_or(42);
    let start_offset = seed % span;

    for k in 0u32..span {
        let idx = (start_offset + k) % span;
        let port = FLOWSIGHT_PORT_MIN.checked_add(idx as u16)?;
        if probe_double_bind_then_release(port).is_ok() {
            return Some(port);
        }
    }
    None
}

fn tcp_bind_ephemeral_ipv4_port_with_backoff() -> Result<u16, String> {
    for attempt in 0..BIND0_MAX_RETRIES {
        if attempt > 0 {
            let ms = 5u64.saturating_mul(1u64 << attempt.min(8));
            thread::sleep(Duration::from_millis(ms));
        }

        let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, 0))
            .map_err(|e| format!("bind 127.0.0.1:0 failed: {e}"))?;
        let port = listener
            .local_addr()
            .map_err(|e| format!("TCP local_addr failed: {e}"))?
            .port();
        drop(listener);

        if probe_double_bind_then_release(port).is_ok() {
            return Ok(port);
        }
    }
    Err(format!(
        "could not allocate an ephemeral localhost port after {} attempts",
        BIND0_MAX_RETRIES
    ))
}

/// Elige un puerto en loopback IPv4 para `llama-server`: rango dedicado FlowSight primero,
/// luego `:0` con reintentos.
pub fn pick_localhost_listen_port() -> Result<u16, String> {
    if let Some(p) = pick_port_in_preferred_range() {
        return Ok(p);
    }
    tcp_bind_ephemeral_ipv4_port_with_backoff()
}

pub fn set_managed_llama_port(port: u16) {
    *lock_managed_port() = Some(port);
}

pub fn clear_managed_llama_port() {
    *lock_managed_port() = None;
}

pub fn current_managed_listen_port() -> Option<u16> {
    *lock_managed_port()
}

pub fn managed_llama_origin() -> Option<String> {
    current_managed_listen_port().map(|p| format!("http://127.0.0.1:{p}"))
}

pub fn managed_health_url() -> Option<String> {
    managed_llama_origin().map(|o| format!("{o}/health"))
}

pub fn managed_chat_completions_url() -> Option<String> {
    managed_llama_origin().map(|o| format!("{o}/v1/chat/completions"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn probe_double_bind_localhost_high_port() {
        let p = pick_port_in_preferred_range().expect("range");
        assert!((FLOWSIGHT_PORT_MIN..=FLOWSIGHT_PORT_MAX).contains(&p));
    }
}
