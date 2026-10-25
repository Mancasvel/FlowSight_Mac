//! Verify packaged local-AI assets and repair a damaged Windows installation.
//! The expected SHA-256 values are embedded in the signed application build.

use serde::Serialize;
use sha2::{Digest, Sha256};
use std::fs::File;
use std::io::Read;
use std::path::Path;
use tauri::{AppHandle, Manager};

struct InstallAsset {
    relative: &'static str,
    size: u64,
    sha256: &'static str,
}

include!(concat!(env!("OUT_DIR"), "/install_manifest.rs"));

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct InstallationHealth {
    healthy: bool,
    checked_files: usize,
}

fn check_asset(root: &Path, asset: &InstallAsset) -> Result<(), String> {
    let path = root.join(asset.relative);
    let mut file = File::open(&path).map_err(|error| format!("{}: {error}", path.display()))?;
    let actual_size = file
        .metadata()
        .map_err(|error| format!("{} metadata: {error}", path.display()))?
        .len();
    if actual_size != asset.size {
        return Err(format!(
            "{}: expected {} bytes, found {}",
            path.display(),
            asset.size,
            actual_size
        ));
    }

    let mut hash = Sha256::new();
    let mut buffer = vec![0_u8; 256 * 1024];
    loop {
        let read = file
            .read(&mut buffer)
            .map_err(|error| format!("{} read: {error}", path.display()))?;
        if read == 0 {
            break;
        }
        hash.update(&buffer[..read]);
    }
    if format!("{:x}", hash.finalize()) != asset.sha256 {
        return Err(format!("{}: SHA-256 mismatch", path.display()));
    }
    Ok(())
}

pub(crate) fn check_installation(app: &AppHandle) -> Result<(), String> {
    if INSTALL_ASSETS.is_empty() {
        // Development builds resolve assets from the working tree. The release
        // build always embeds a complete manifest and checks every listed file.
        return Ok(());
    }
    let root = app
        .path()
        .resource_dir()
        .map_err(|error| format!("resource directory: {error}"))?;
    for asset in INSTALL_ASSETS {
        check_asset(&root, asset)?;
    }
    Ok(())
}

#[cfg(all(windows, not(debug_assertions)))]
pub(crate) fn verify_before_local_ai(app: &AppHandle) -> Result<(), String> {
    use tauri::Emitter;

    check_installation(app).map_err(|error| {
        log::error!("[Installation] Refusing to start local AI: {error}");
        let _ = app.emit("installation-repair-required", ());
        "FlowSight needs a repair before local AI can start.".to_string()
    })
}

#[tauri::command]
pub async fn check_installation_health(app: AppHandle) -> InstallationHealth {
    let result = tauri::async_runtime::spawn_blocking(move || check_installation(&app)).await;
    let healthy = match result {
        Ok(Ok(())) => true,
        Ok(Err(error)) => {
            log::error!("[Installation] Integrity check failed: {error}");
            false
        }
        Err(error) => {
            log::error!("[Installation] Integrity worker failed: {error}");
            false
        }
    };
    InstallationHealth {
        healthy,
        checked_files: INSTALL_ASSETS.len(),
    }
}

#[cfg(windows)]
static REPAIR_RUNNING: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

#[cfg(windows)]
struct RepairGuard;

#[cfg(windows)]
impl Drop for RepairGuard {
    fn drop(&mut self) {
        REPAIR_RUNNING.store(false, std::sync::atomic::Ordering::SeqCst);
    }
}

#[cfg(windows)]
#[tauri::command]
pub async fn repair_installation(app: AppHandle) -> Result<(), String> {
    use std::sync::atomic::{AtomicU64, AtomicU8, Ordering};
    use std::sync::Arc;
    use tauri::Emitter;
    use tauri_plugin_updater::UpdaterExt;

    if REPAIR_RUNNING.swap(true, Ordering::SeqCst) {
        return Err("FlowSight is already repairing the installation.".to_string());
    }
    let _guard = RepairGuard;

    let updater = app
        .updater_builder()
        .version_comparator(|current, remote| remote.version >= current)
        .build()
        .map_err(|error| {
            log::error!("[Installation] Could not initialize signed repair: {error}");
            "FlowSight could not prepare the repair. Please try again.".to_string()
        })?;
    let update = updater.check().await.map_err(|error| {
        log::error!("[Installation] Could not check signed release: {error}");
        "FlowSight could not reach the repair download. Check your connection and try again."
            .to_string()
    })?;
    let Some(update) = update else {
        log::error!("[Installation] No eligible signed release for this version");
        return Err("A repair package is not available yet. Please try again later.".to_string());
    };

    let downloaded = Arc::new(AtomicU64::new(0));
    let last_percent = Arc::new(AtomicU8::new(0));
    let event_app = app.clone();
    let bytes = update
        .download(
            move |chunk_size, content_length| {
                let current =
                    downloaded.fetch_add(chunk_size as u64, Ordering::Relaxed) + chunk_size as u64;
                if let Some(total) = content_length.filter(|total| *total > 0) {
                    let percent = ((current.saturating_mul(100) / total).min(100)) as u8;
                    if percent > last_percent.swap(percent, Ordering::Relaxed) {
                        let _ = event_app.emit("installation-repair-progress", percent);
                    }
                }
            },
            || {},
        )
        .await
        .map_err(|error| {
            log::error!("[Installation] Signed repair download failed: {error}");
            "The repair download did not finish. Check your connection and try again.".to_string()
        })?;

    // `download` verifies the updater's cryptographic signature before
    // returning. The native installer replaces program files and relaunches;
    // user data and the local license are stored outside the install folder.
    let _ = app.emit("installation-repair-installing", ());
    update.install(bytes).map_err(|error| {
        log::error!("[Installation] Signed repair installation failed: {error}");
        "FlowSight could not complete the repair. Please try again.".to_string()
    })?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{check_asset, InstallAsset};
    use std::fs;

    #[test]
    fn integrity_check_rejects_missing_truncated_and_modified_files() {
        let dir = tempfile::tempdir().unwrap();
        let asset = InstallAsset {
            relative: "engine.dll",
            size: 3,
            sha256: "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad",
        };
        assert!(check_asset(dir.path(), &asset).is_err());
        fs::write(dir.path().join("engine.dll"), b"ab").unwrap();
        assert!(check_asset(dir.path(), &asset).is_err());
        fs::write(dir.path().join("engine.dll"), b"abd").unwrap();
        assert!(check_asset(dir.path(), &asset).is_err());
        fs::write(dir.path().join("engine.dll"), b"abc").unwrap();
        assert!(check_asset(dir.path(), &asset).is_ok());
    }
}
