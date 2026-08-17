//! Single source of truth for filesystem paths used by the agent.
//!
//! Motivation: hasta ahora cada módulo (`auth`, `sync`, `jira`, `linear`,
//! `agent`, `main`) construía por su cuenta `dirs::data_local_dir().unwrap().join("FlowSight")`
//! sin garantizar que el directorio existiese. En instalación fresca
//! (pre-login, pre-`initialize_agent`) el directorio no existe y cualquier
//! `Connection::open` o escritura de log fallaba silenciosamente.
//!
//! Todos los paths del runtime del usuario deben pasar por acá. Los paths
//! de recursos read-only bundlados con el instalador de Tauri se resuelven
//! vía `resource_local_llm_dir` y requieren `AppHandle`.

use std::path::PathBuf;

use serde_json::json;
use tauri::{AppHandle, Manager};

const APP_DIR_NAME: &str = "FlowSight";
const DB_FILE: &str = "dev-agent.db";
const SERVER_LOG_FILE: &str = "server.log";
const AGENT_ERROR_LOG_FILE: &str = "agent_error.log";
const CRASH_LOG_FILE: &str = "crash.log";
const SCREENSHOTS_TMP_DIR: &str = "screenshots_tmp";

/// Carpeta local de datos de FlowSight dentro del perfil del usuario (creada si no existe).
///
/// Se resuelve con `dirs::data_local_dir()` (Known Folders en Windows, equivalentes en otros
/// SO): **ruta real en disco**, sin depender del idioma de la interfaz ni de variables de entorno
/// escritas en la UI para el usuario.
///
/// Es el único lugar escribible que usamos. Tiene que funcionar igual en dev,
/// en release portable y en instalaciones a `Program Files` (donde el
/// directorio de instalación NO es escribible por el usuario estándar).
pub fn app_data_dir() -> Result<PathBuf, String> {
    let base = dirs::data_local_dir().ok_or_else(|| "No local data dir available".to_string())?;
    let dir = base.join(APP_DIR_NAME);
    if !dir.exists() {
        std::fs::create_dir_all(&dir).map_err(|e| format!("Failed to create {:?}: {}", dir, e))?;
    }
    Ok(dir)
}

/// Path a `dev-agent.db`. Crea el directorio padre si hace falta.
pub fn db_path() -> Result<PathBuf, String> {
    Ok(app_data_dir()?.join(DB_FILE))
}

/// Variante infalible para sitios donde no podemos propagar Result (panic hooks,
/// static init). En ese caso cae a `.` que es subóptimo pero no panica.
pub fn db_path_or_fallback() -> PathBuf {
    db_path().unwrap_or_else(|_| PathBuf::from(DB_FILE))
}

pub fn server_log_path() -> Result<PathBuf, String> {
    Ok(app_data_dir()?.join(SERVER_LOG_FILE))
}

pub fn auth_log_path() -> Result<PathBuf, String> {
    Ok(app_data_dir()?.join("auth.log"))
}

pub fn agent_error_log_path() -> Result<PathBuf, String> {
    Ok(app_data_dir()?.join(AGENT_ERROR_LOG_FILE))
}

pub fn crash_log_path_or_fallback() -> PathBuf {
    app_data_dir()
        .map(|d| d.join(CRASH_LOG_FILE))
        .unwrap_or_else(|_| PathBuf::from(CRASH_LOG_FILE))
}

/// PNG de captura persistentes solo para depuración; mismo árbol que la BD/logs
/// que [`app_data_dir`], subcarpeta `screenshots_tmp\`, no el Escritorio ni la carpeta de instalación.
pub fn screenshots_tmp_dir() -> Result<PathBuf, String> {
    let dir = app_data_dir()?.join(SCREENSHOTS_TMP_DIR);
    if !dir.exists() {
        std::fs::create_dir_all(&dir).map_err(|e| format!("Failed to create {:?}: {}", dir, e))?;
    }
    Ok(dir)
}

/// Detecta bloqueos de escritura (p. ej. **Controlled Folder Access**, ACL, AV) antes de confiar en SQLite.
pub fn verify_app_dir_filesystem_writable() -> Result<(), String> {
    let dir = app_data_dir()?;
    let probe = dir.join(".flowsight_fs_write_probe");
    std::fs::write(&probe, b"ok").map_err(|e| {
        format!(
            "Cannot write application data under {} ({e}). Check folder permissions{}.",
            dir.display(),
            if cfg!(windows) {
                " (on Windows 11 this may be Controlled Folder Access / Defender blocking an unsigned app)"
            } else if cfg!(target_os = "macos") {
                " (on macOS, Full Disk Access is not required for Application Support)"
            } else {
                ""
            }
        )
    })?;
    let _ = std::fs::remove_file(&probe);
    Ok(())
}

/// Elimina capturas de depuración `capture_*` más antiguas que `max_age` (retención / cumplimiento).
pub fn prune_screenshots_tmp_older_than(max_age: std::time::Duration) -> Result<usize, String> {
    use std::time::SystemTime;

    let dir = screenshots_tmp_dir()?;
    let now = SystemTime::now();
    let mut removed = 0usize;
    let entries =
        std::fs::read_dir(&dir).map_err(|e| format!("Failed to read screenshots_tmp: {e}"))?;
    for ent in entries.filter_map(Result::ok) {
        let name = ent.file_name();
        let s = name.to_string_lossy();
        if !s.starts_with("capture_") {
            continue;
        }
        let Ok(meta) = ent.metadata() else {
            continue;
        };
        let Ok(mtime) = meta.modified() else {
            continue;
        };
        let Ok(elapsed) = now.duration_since(mtime) else {
            continue;
        };
        if elapsed > max_age {
            let p = ent.path();
            if std::fs::remove_file(&p).is_ok() {
                removed += 1;
            }
        }
    }
    Ok(removed)
}

/// Nombre del binario `llama-server` según plataforma.
pub fn llama_server_bin_name() -> &'static str {
    if cfg!(windows) {
        "llama-server.exe"
    } else {
        "llama-server"
    }
}

fn llama_server_present(dir: &std::path::Path) -> bool {
    dir.join("bin").join(llama_server_bin_name()).exists()
}

/// Resuelve el directorio de recursos bundlados donde vive `local_llm/`.
///
/// En un instalador Tauri, los `bundle.resources` van a `<install>/resources/`.
/// En dev, `resource_dir` apunta al target de cargo; caemos al layout del repo
/// (`<repo-root>/local_llm`) si los bundleados no están.
pub fn resource_local_llm_dir(app: &AppHandle) -> Result<PathBuf, String> {
    let resource_dir = app
        .path()
        .resource_dir()
        .map_err(|e| format!("resource_dir unavailable: {}", e))?;

    let bundled = resource_dir.join("local_llm");
    if llama_server_present(&bundled) {
        return Ok(bundled);
    }

    // Fallback dev: subir desde target/.../<bin> hasta encontrar local_llm/bin.
    if let Ok(exe) = std::env::current_exe() {
        let mut dir = exe.parent().map(|p| p.to_path_buf()).unwrap_or_default();
        for _ in 0..8 {
            let candidate = dir.join("local_llm");
            if llama_server_present(&candidate) {
                return Ok(candidate);
            }
            if !dir.pop() {
                break;
            }
        }
    }

    Err(format!(
        "local_llm runtime not found (looked for {} under bundled {:?} and repo tree)",
        llama_server_bin_name(),
        bundled
    ))
}

fn sanitize_pdf_filename(name: &str) -> Result<String, String> {
    let trimmed = name.trim();
    if trimmed.is_empty() || !trimmed.to_ascii_lowercase().ends_with(".pdf") {
        return Err("Invalid PDF filename".to_string());
    }
    let safe: String = trimmed
        .chars()
        .filter(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '-'))
        .collect();
    if safe.len() < 5 {
        return Err("Invalid PDF filename".to_string());
    }
    Ok(safe)
}

fn unique_download_path(downloads: &std::path::Path, filename: &str) -> PathBuf {
    let mut path = downloads.join(filename);
    if !path.exists() {
        return path;
    }
    let stem = filename.strip_suffix(".pdf").unwrap_or(filename);
    for n in 2..=99 {
        let candidate = format!("{stem}_{n}.pdf");
        path = downloads.join(&candidate);
        if !path.exists() {
            return path;
        }
    }
    let stamp = chrono::Local::now().format("%H%M%S");
    downloads.join(format!("{stem}_{stamp}.pdf"))
}

/// Guarda un PDF en la carpeta Descargas del usuario y devuelve la ruta absoluta escrita.
#[tauri::command]
pub fn save_pdf_to_downloads(filename: String, bytes: Vec<u8>) -> Result<String, String> {
    let downloads = dirs::download_dir()
        .ok_or_else(|| "Downloads folder not available on this system".to_string())?;
    let safe = sanitize_pdf_filename(&filename)?;
    let path = unique_download_path(&downloads, &safe);
    std::fs::write(&path, &bytes).map_err(|e| format!("Failed to save PDF: {e}"))?;
    Ok(path.to_string_lossy().to_string())
}

/// Abre la carpeta que contiene `path` (si es un archivo, abre su directorio padre).
#[tauri::command]
pub fn open_path_in_file_manager(path: String) -> Result<(), String> {
    let p = PathBuf::from(path);
    let target = if p.is_file() {
        p.parent()
            .map(|parent| parent.to_path_buf())
            .unwrap_or(p)
    } else {
        p
    };
    open::that(&target).map_err(|e| format!("Could not open folder: {e}"))
}

#[tauri::command]
pub fn get_flowsight_user_paths() -> Result<serde_json::Value, String> {
    let dir = app_data_dir()?;
    Ok(json!({
        "appDataDir": dir.to_string_lossy(),
        "serverLog": server_log_path()?.to_string_lossy(),
        "authLog": auth_log_path()?.to_string_lossy(),
        "agentErrorLog": agent_error_log_path()?.to_string_lossy(),
    }))
}
