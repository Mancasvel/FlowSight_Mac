//! Process-level "last resort" containment for hardware exceptions
//! (illegal instruction / access violation) raised by third-party code
//! injected into this process outside of our control.
//!
//! Background: Windows lets any installed software register a Winsock
//! Layered Service Provider (LSP), and the OS then loads that vendor's DLL
//! into *every* process that opens a socket — including ours, even for a
//! purely local call like checking Ollama on `localhost:11434` — regardless
//! of whether the vendor's own application is running. Old/unsigned/broken
//! LSP DLLs (observed in the wild: Astrill VPN's `ASProxy64.dll`, Cloudflare
//! WARP) can crash with `STATUS_ILLEGAL_INSTRUCTION` the moment they're
//! loaded or exercised, and by default that crash is fatal to the *entire*
//! host process, not just the network call that triggered it.
//!
//! We cannot fix a third party's broken LSP from inside our app (that would
//! require admin rights on the end user's machine and doesn't scale to every
//! client's security software). What we *can* do is stop a fault in memory
//! we don't own from taking our whole process down: a Vectored Exception
//! Handler (VEH), registered as early as possible, inspects every hardware
//! exception and, if it originated outside our own module, terminates only
//! the faulting thread instead of letting the default unhandled-exception
//! behavior kill the process.
//!
//! This is intentionally narrow and conservative:
//! - Only `EXCEPTION_ILLEGAL_INSTRUCTION` and `EXCEPTION_ACCESS_VIOLATION`
//!   are considered (the two hardware fault classes that crashed us here).
//! - Exceptions whose address falls inside our own module (`app.exe`) are
//!   never intercepted (`EXCEPTION_CONTINUE_SEARCH`), so a real bug in our
//!   own code still crashes/panics normally instead of being masked.
//! - This does not replace normal Rust error handling (`Result`/`?`), which
//!   remains the primary mechanism for everything else in the codebase.

#[cfg(windows)]
mod windows_impl {
    use std::sync::atomic::{AtomicU32, AtomicUsize, Ordering};

    use windows::core::PCWSTR;
    use windows::Win32::Foundation::{EXCEPTION_ACCESS_VIOLATION, EXCEPTION_ILLEGAL_INSTRUCTION, HMODULE};
    use windows::Win32::System::Diagnostics::Debug::{
        AddVectoredExceptionHandler, EXCEPTION_CONTINUE_SEARCH, EXCEPTION_POINTERS,
    };
    use windows::Win32::System::LibraryLoader::{
        GetModuleFileNameW, GetModuleHandleExW, GetModuleHandleW,
        GET_MODULE_HANDLE_EX_FLAG_FROM_ADDRESS, GET_MODULE_HANDLE_EX_FLAG_UNCHANGED_REFCOUNT,
    };
    use windows::Win32::System::ProcessStatus::{GetModuleInformation, MODULEINFO};
    use windows::Win32::System::Threading::{ExitThread, GetCurrentProcess, GetCurrentThreadId};

    /// Base/end address of our own module (`app.exe`), captured once at
    /// startup. `0` means "not captured" (module range lookup failed), in
    /// which case we conservatively treat every faulting address as
    /// "foreign" rather than risk masking one of our own bugs.
    static OWN_MODULE_BASE: AtomicUsize = AtomicUsize::new(0);
    static OWN_MODULE_END: AtomicUsize = AtomicUsize::new(0);

    /// The thread that called `install()` — expected to be the process's
    /// main/UI thread. Used only for the diagnostic log line; see the
    /// module-level note on why the main thread should never reach this
    /// handler in the first place (all network calls must run off it).
    static MAIN_THREAD_ID: AtomicU32 = AtomicU32::new(0);

    pub fn install() {
        MAIN_THREAD_ID.store(unsafe { GetCurrentThreadId() }, Ordering::Relaxed);
        capture_own_module_range();

        // `first = 1`: register as the FIRST handler in the process's VEH
        // chain, so we see the exception before anything else (e.g. a
        // debugger or another library's own handler) decides what to do
        // with it.
        let registered = unsafe { AddVectoredExceptionHandler(1, Some(vectored_handler)) };
        if registered.is_null() {
            log::error!(
                "[CrashGuard] AddVectoredExceptionHandler failed to register; third-party DLL \
                 crashes (e.g. a broken Winsock LSP) will NOT be contained this session and may \
                 still take down the whole process"
            );
        } else {
            log::info!(
                "[CrashGuard] Vectored exception handler installed (own module range: {:#x}-{:#x})",
                OWN_MODULE_BASE.load(Ordering::Relaxed),
                OWN_MODULE_END.load(Ordering::Relaxed)
            );
        }
    }

    fn capture_own_module_range() {
        unsafe {
            let Ok(hmodule) = GetModuleHandleW(PCWSTR::null()) else {
                log::warn!("[CrashGuard] GetModuleHandleW(NULL) failed; cannot determine our own module range");
                return;
            };

            let process = GetCurrentProcess();
            let mut info = MODULEINFO::default();
            let size = std::mem::size_of::<MODULEINFO>() as u32;
            match GetModuleInformation(process, hmodule, &mut info, size) {
                Ok(()) => {
                    let base = info.lpBaseOfDll as usize;
                    OWN_MODULE_BASE.store(base, Ordering::Relaxed);
                    OWN_MODULE_END.store(base + info.SizeOfImage as usize, Ordering::Relaxed);
                }
                Err(e) => {
                    log::warn!("[CrashGuard] GetModuleInformation failed ({e}); cannot determine our own module range");
                }
            }
        }
    }

    /// Resolves a faulting address to the module it belongs to, returning
    /// `(file_name, full_path)`. Best-effort: falls back to `"<unknown>"` on
    /// any failure rather than propagating an error, since this only runs
    /// inside the exception handler itself.
    fn identify_module(address: usize) -> (String, String) {
        unsafe {
            let mut hmodule = HMODULE::default();
            let flags = GET_MODULE_HANDLE_EX_FLAG_FROM_ADDRESS | GET_MODULE_HANDLE_EX_FLAG_UNCHANGED_REFCOUNT;
            // GET_MODULE_HANDLE_EX_FLAG_FROM_ADDRESS repurposes the "module
            // name" parameter as the address to resolve — this is the
            // documented Win32 pattern, not a real string.
            let address_as_name = PCWSTR(address as *const u16);
            if GetModuleHandleExW(flags, address_as_name, &mut hmodule).is_err() {
                return ("<unknown module>".to_string(), "<unknown path>".to_string());
            }

            let mut buf = [0u16; 512];
            let len = GetModuleFileNameW(Some(hmodule), &mut buf);
            if len == 0 {
                return ("<unknown module>".to_string(), "<unknown path>".to_string());
            }

            let full_path = String::from_utf16_lossy(&buf[..len as usize]);
            let file_name = full_path
                .rsplit(['\\', '/'])
                .next()
                .unwrap_or(&full_path)
                .to_string();
            (file_name, full_path)
        }
    }

    unsafe extern "system" fn vectored_handler(exception_info: *mut EXCEPTION_POINTERS) -> i32 {
        // Defensive: a null pointer here would itself be undefined behavior
        // to dereference, so bail out to the next handler in the chain.
        let Some(info) = exception_info.as_ref() else {
            return EXCEPTION_CONTINUE_SEARCH;
        };
        let Some(record) = info.ExceptionRecord.as_ref() else {
            return EXCEPTION_CONTINUE_SEARCH;
        };

        let code = record.ExceptionCode;
        if code != EXCEPTION_ILLEGAL_INSTRUCTION && code != EXCEPTION_ACCESS_VIOLATION {
            // Anything else (including the SEH-based exceptions Rust's own
            // panic/unwind machinery uses) is none of our business.
            return EXCEPTION_CONTINUE_SEARCH;
        }

        let address = record.ExceptionAddress as usize;
        let own_base = OWN_MODULE_BASE.load(Ordering::Relaxed);
        let own_end = OWN_MODULE_END.load(Ordering::Relaxed);
        let in_own_module = own_base != 0 && address >= own_base && address < own_end;

        if in_own_module {
            // A real bug in our own code: never mask it, let it crash/panic
            // normally so it stays visible instead of being silently eaten.
            return EXCEPTION_CONTINUE_SEARCH;
        }

        let (module_name, module_path) = identify_module(address);
        let thread_id = GetCurrentThreadId();
        let is_main_thread = thread_id == MAIN_THREAD_ID.load(Ordering::Relaxed);
        let code_name = if code == EXCEPTION_ILLEGAL_INSTRUCTION {
            "EXCEPTION_ILLEGAL_INSTRUCTION"
        } else {
            "EXCEPTION_ACCESS_VIOLATION"
        };

        let message = format!(
            "[CrashGuard] Contained {code_name} (NTSTATUS {:#010x}) at address {address:#x} in \
             third-party module '{module_name}' ({module_path}). This is almost certainly a \
             broken Winsock LSP (VPN/antivirus network proxy DLL) injected into this process by \
             Windows, not a FlowSight bug. Terminating only the faulting thread (id {thread_id}, \
             {}) so the rest of the app keeps running; the network operation on that thread will \
             fail/retry instead of completing.",
            code.0 as u32,
            if is_main_thread { "MAIN/UI thread — see crash_guard module docs" } else { "background thread" }
        );

        // Best-effort on two channels: the app's structured logger (ends up
        // in the tauri-plugin-log file/target for real diagnostics), and
        // stderr, in case this fires before the logger plugin has finished
        // initializing this early in startup.
        log::error!("{message}");
        eprintln!("{message}");

        // SAFETY: ExitThread never returns; it tears down only the calling
        // (faulting) thread. Per Win32 semantics this also ends the process
        // if it happens to be the very last thread — which is the same
        // outcome an uncontained crash would have had anyway, just without
        // the diagnostic log lines above.
        ExitThread(1);
    }
}

#[cfg(windows)]
pub fn install() {
    windows_impl::install();
}

/// No-op on non-Windows targets: Winsock LSP injection is a Windows-specific
/// mechanism, so there is nothing to guard against on other platforms.
#[cfg(not(windows))]
pub fn install() {}
