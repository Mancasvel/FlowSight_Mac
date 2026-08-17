//! Escritura opcional de capturas bajo `screenshots_tmp\`.
//! En Windows usa **DPAPI** (`CryptProtectData`, ámbito usuario) en `.png.dpapi` para reducir datos
//! sensibles en claro en disco. Si DPAPI falla, **no** se graba archivo (solo memoria/Base64 siguen siendo válidos para el modelo).

#![cfg_attr(not(windows), allow(dead_code))]

use std::path::{Path, PathBuf};

#[cfg(windows)]
use std::io::Write;
#[cfg(windows)]
use windows_sys::Win32::Foundation::{LocalFree, HLOCAL};

/// Intenta persistir PNG de depuración. [`None`] = no crear archivo en disco.
pub fn write_debug_capture_image(png_plain: &[u8], stem: &str, dir: &Path) -> Option<PathBuf> {
    #[cfg(windows)]
    {
        let buf = dpapi_protect(png_plain)?;
        let p = dir.join(format!("{stem}.png.dpapi"));
        std::fs::File::create(&p)
            .and_then(|mut f| f.write_all(&buf))
            .map_err(|e| {
                log::warn!(
                    "[FlowSight] could not write encrypted screenshot blob {:?}: {}",
                    p,
                    e
                );
                e
            })
            .ok()?;
        Some(p)
    }
    #[cfg(not(windows))]
    {
        let p = dir.join(format!("{stem}.png"));
        std::fs::write(&p, png_plain).ok()?;
        Some(p)
    }
}

#[cfg(windows)]
#[allow(clippy::cast_possible_truncation)]
fn dpapi_protect(plain: &[u8]) -> Option<Vec<u8>> {
    use std::ptr::addr_of;

    use windows_sys::Win32::Security::Cryptography::{
        CryptProtectData, CRYPT_INTEGER_BLOB, CRYPTPROTECT_UI_FORBIDDEN,
    };

    let in_blob = CRYPT_INTEGER_BLOB {
        cbData: plain.len() as u32,
        pbData: plain.as_ptr() as *mut u8,
    };
    let mut out_blob = CRYPT_INTEGER_BLOB {
        cbData: 0,
        pbData: std::ptr::null_mut(),
    };

    let ok = unsafe {
        CryptProtectData(
            addr_of!(in_blob),
            std::ptr::null(),
            std::ptr::null(),
            std::ptr::null_mut(),
            std::ptr::null_mut(),
            CRYPTPROTECT_UI_FORBIDDEN,
            &mut out_blob,
        )
    };
    if ok == 0 || out_blob.pbData.is_null() || out_blob.cbData == 0 {
        log::warn!(
            "[FlowSight] CryptProtectData failed for screenshot debug: {}",
            std::io::Error::last_os_error()
        );
        return None;
    }

    let slice =
        unsafe { std::slice::from_raw_parts(out_blob.pbData, out_blob.cbData as usize) }.to_vec();
    unsafe {
        LocalFree(out_blob.pbData as HLOCAL);
    }
    Some(slice)
}
