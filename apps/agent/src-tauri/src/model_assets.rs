//! First-run acquisition of the local vision weights.
//!
//! Los GGUF (~1.3 GB) ya no viajan dentro del bundle: el `.app` solo lleva el
//! runtime de llama.cpp (`local_llm/bin/`), que es código y por tanto debe ir
//! firmado y notarizado. Los pesos son datos: se descargan una vez al
//! directorio de datos de la app (`app_data_dir()/models`, es decir
//! `~/Library/Application Support/ai.flowsight.agent/models` en macOS) y se
//! verifican por sha256 antes de darlos por buenos.
//!
//! Orden de resolución: pesos ya descargados → árbol del repo (`local_llm/`,
//! conveniencia de desarrollo y compatibilidad con instalaciones antiguas que
//! sí los llevaban bundlados) → descarga.

use std::fs::{self, File, OpenOptions};
use std::io::{Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::{Duration, Instant};

use serde::Serialize;
use sha2::{Digest, Sha256};
use tauri::{AppHandle, Emitter, Manager};

use crate::vision_model::{VISION_GGUF_FILENAME, VISION_MMPROJ_FILENAME};

const MODELS_SUBDIR: &str = "models";
const DEFAULT_MODELS_REPO: &str = "Mancasvel/FlowSight.AI";
const DEFAULT_MODELS_TAG: &str = "models-v0.1.0";
const DOWNLOAD_PROGRESS_EVENT: &str = "local-model-download";
const CONNECT_TIMEOUT_SECS: u64 = 30;
const TCP_KEEPALIVE_SECS: u64 = 30;
const STREAM_BUFFER_BYTES: usize = 1 << 20;
const PROGRESS_EMIT_INTERVAL: Duration = Duration::from_millis(400);

/// Un peso descargable del release de modelos.
///
/// `size_bytes` y `sha256` se obtuvieron de los assets publicados en
/// `<repo>/releases/download/models-v0.1.0/` y coinciden byte a byte con los
/// ficheros que usa `scripts/fetch-models.mjs` en `local_llm/`.
struct WeightAsset {
    filename: &'static str,
    size_bytes: u64,
    sha256: &'static str,
}

const VISION_ASSETS: [WeightAsset; 2] = [
    WeightAsset {
        filename: VISION_GGUF_FILENAME,
        size_bytes: 939_540_160,
        sha256: "d4346b52a40d103ed6892b09fd3643e0a11b2dd26d3234f37ec68a94ec20ae24",
    },
    WeightAsset {
        filename: VISION_MMPROJ_FILENAME,
        size_bytes: 445_053_216,
        sha256: "f9a68fabba69c3b81e153367b2c7521030b0fa8bb0de400c9599c8e6725f9c82",
    },
];

/// Serializa las descargas: dos llamadas concurrentes no deben escribir el
/// mismo `.part`. La segunda re-comprueba y sale sin bajar nada.
static DOWNLOAD_LOCK: Mutex<()> = Mutex::new(());

#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
struct DownloadProgress {
    /// `downloading` | `verifying` | `ready` | `error`
    phase: &'static str,
    file: String,
    downloaded_bytes: u64,
    total_bytes: u64,
    percent: u8,
    /// Posición 1-based del peso en curso, para "1 de 2" en la UI.
    index: usize,
    count: usize,
    #[serde(skip_serializing_if = "Option::is_none")]
    error: Option<String>,
}

fn total_download_bytes() -> u64 {
    VISION_ASSETS.iter().map(|a| a.size_bytes).sum()
}

fn asset_position(asset: &WeightAsset) -> usize {
    VISION_ASSETS
        .iter()
        .position(|a| a.filename == asset.filename)
        .map(|i| i + 1)
        .unwrap_or(1)
}

fn emit_progress(app: &AppHandle, payload: DownloadProgress) {
    if let Err(e) = app.emit(DOWNLOAD_PROGRESS_EVENT, payload) {
        log::warn!("[LocalModel] progress emit failed: {}", e);
    }
}

fn asset_progress(
    asset: &WeightAsset,
    phase: &'static str,
    downloaded: u64,
    total: u64,
    error: Option<&str>,
) -> DownloadProgress {
    DownloadProgress {
        phase,
        file: asset.filename.to_string(),
        downloaded_bytes: downloaded,
        total_bytes: total,
        percent: percent_of(downloaded, total),
        index: asset_position(asset),
        count: VISION_ASSETS.len(),
        error: error.map(str::to_string),
    }
}

fn models_repo() -> String {
    std::env::var("FLOWSIGHT_MODELS_REPO")
        .ok()
        .filter(|v| !v.trim().is_empty())
        .unwrap_or_else(|| DEFAULT_MODELS_REPO.to_string())
}

fn models_tag() -> String {
    std::env::var("FLOWSIGHT_MODELS_TAG")
        .ok()
        .filter(|v| !v.trim().is_empty())
        .unwrap_or_else(|| DEFAULT_MODELS_TAG.to_string())
}

/// Base de descarga del release de modelos, sin el nombre del asset.
fn release_base_url() -> String {
    format!(
        "https://github.com/{}/releases/download/{}",
        models_repo(),
        models_tag()
    )
}

/// Directorio escribible donde viven los pesos descargados (creado si no existe).
///
/// Usa la API de rutas de Tauri en lugar de una ruta fija para que Windows y
/// Linux resuelvan a su equivalente sin ramas por plataforma.
pub fn models_dir(app: &AppHandle) -> Result<PathBuf, String> {
    let dir = app
        .path()
        .app_data_dir()
        .map_err(|e| format!("app data dir unavailable: {}", e))?
        .join(MODELS_SUBDIR);
    fs::create_dir_all(&dir).map_err(|e| format!("Failed to create {:?}: {}", dir, e))?;
    Ok(dir)
}

/// Un peso solo cuenta como usable si su tamaño es exactamente el esperado.
///
/// No re-hasheamos en cada arranque (1.3 GB ≈ 6 s de I/O): el sha256 se
/// comprueba tras descargar y el fichero solo llega a su ruta final por
/// `rename` atómico, así que un fichero presente ya pasó la verificación.
/// El tamaño exacto sigue detectando el fallo realista (truncado / borrado).
fn weight_complete(path: &Path, asset: &WeightAsset) -> bool {
    fs::metadata(path)
        .map(|m| m.len() == asset.size_bytes)
        .unwrap_or(false)
}

/// Pesos en el árbol del repo (dev) o en `Resources/local_llm` (builds viejos).
fn fallback_weight_path(app: &AppHandle, asset: &WeightAsset) -> Option<PathBuf> {
    let path = crate::paths::resource_local_llm_dir(app)
        .ok()?
        .join(asset.filename);
    weight_complete(&path, asset).then_some(path)
}

fn resolve_weight(app: &AppHandle, asset: &WeightAsset) -> Option<PathBuf> {
    if let Ok(dir) = models_dir(app) {
        let managed = dir.join(asset.filename);
        if weight_complete(&managed, asset) {
            return Some(managed);
        }
    }
    fallback_weight_path(app, asset)
}

/// `(modelo, projector)` si ambos pesos están completos; `None` si falta alguno.
pub fn resolved_vision_weights(app: &AppHandle) -> Option<(PathBuf, PathBuf)> {
    let model = resolve_weight(app, &VISION_ASSETS[0])?;
    let mmproj = resolve_weight(app, &VISION_ASSETS[1])?;
    Some((model, mmproj))
}

/// Garantiza los pesos en disco (descargándolos si hace falta) y devuelve sus rutas.
///
/// Bloqueante: llamar desde un hilo de trabajo, nunca desde el hilo de UI.
pub fn ensure_vision_weights(app: &AppHandle) -> Result<(PathBuf, PathBuf), String> {
    if let Some(paths) = resolved_vision_weights(app) {
        return Ok(paths);
    }

    let _guard = DOWNLOAD_LOCK
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());

    // Otra llamada pudo completar la descarga mientras esperábamos el lock.
    if let Some(paths) = resolved_vision_weights(app) {
        return Ok(paths);
    }

    let dir = models_dir(app)?;
    let base_url = release_base_url();
    for asset in VISION_ASSETS.iter() {
        if resolve_weight(app, asset).is_some() {
            continue;
        }
        let dest = dir.join(asset.filename);
        log::info!("[LocalModel] downloading {} -> {:?}", asset.filename, dest);

        let mut last_emit = Instant::now() - PROGRESS_EMIT_INTERVAL;
        let mut on_event = |event: FetchEvent| match event {
            FetchEvent::Progress { downloaded, total } => {
                if last_emit.elapsed() < PROGRESS_EMIT_INTERVAL {
                    return;
                }
                last_emit = Instant::now();
                emit_progress(
                    app,
                    asset_progress(asset, "downloading", downloaded, total, None),
                );
            }
            FetchEvent::Verifying => emit_progress(
                app,
                asset_progress(
                    asset,
                    "verifying",
                    asset.size_bytes,
                    asset.size_bytes,
                    None,
                ),
            ),
        };

        if let Err(e) = fetch_asset(&base_url, asset, &dest, &mut on_event) {
            emit_progress(app, asset_progress(asset, "error", 0, asset.size_bytes, Some(&e)));
            return Err(e);
        }
    }

    let paths = resolved_vision_weights(app).ok_or_else(|| {
        "Local AI model download finished but the weights are still missing.".to_string()
    })?;

    emit_progress(
        app,
        DownloadProgress {
            phase: "ready",
            file: String::new(),
            downloaded_bytes: total_download_bytes(),
            total_bytes: total_download_bytes(),
            percent: 100,
            index: VISION_ASSETS.len(),
            count: VISION_ASSETS.len(),
            error: None,
        },
    );
    Ok(paths)
}

fn part_path(dest: &Path) -> PathBuf {
    let mut name = dest.file_name().unwrap_or_default().to_os_string();
    name.push(".part");
    dest.with_file_name(name)
}

/// Lo que el descargador reporta hacia fuera, sin depender de Tauri.
enum FetchEvent {
    Progress { downloaded: u64, total: u64 },
    Verifying,
}

/// Descarga a `<dest>.part`, verifica sha256 y solo entonces renombra a `dest`.
fn fetch_asset(
    base_url: &str,
    asset: &WeightAsset,
    dest: &Path,
    on_event: &mut dyn FnMut(FetchEvent),
) -> Result<(), String> {
    let part = part_path(dest);
    stream_to_part(base_url, asset, &part, on_event)?;
    on_event(FetchEvent::Verifying);

    if let Err(e) = verify_part(&part, asset) {
        let _ = fs::remove_file(&part);
        return Err(e);
    }

    fs::rename(&part, dest).map_err(|e| {
        format!(
            "Downloaded {} but could not move it into place ({}).",
            asset.filename, e
        )
    })
}

fn verify_part(part: &Path, asset: &WeightAsset) -> Result<(), String> {
    let size = fs::metadata(part)
        .map(|m| m.len())
        .map_err(|e| format!("Cannot read downloaded {}: {}", asset.filename, e))?;
    if size != asset.size_bytes {
        return Err(format!(
            "Download of {} is incomplete ({} of {} bytes). Check your connection and retry.",
            asset.filename, size, asset.size_bytes
        ));
    }

    let actual = sha256_file(part)?;
    if actual != asset.sha256 {
        return Err(format!(
            "Integrity check failed for {} (sha256 {} != {}). The download was discarded; retry.",
            asset.filename, actual, asset.sha256
        ));
    }
    Ok(())
}

fn sha256_file(path: &Path) -> Result<String, String> {
    let mut file =
        File::open(path).map_err(|e| format!("Cannot open {:?} for hashing: {}", path, e))?;
    let mut hasher = Sha256::new();
    let mut buf = vec![0u8; STREAM_BUFFER_BYTES];
    loop {
        let read = file
            .read(&mut buf)
            .map_err(|e| format!("Cannot read {:?} while hashing: {}", path, e))?;
        if read == 0 {
            break;
        }
        hasher.update(&buf[..read]);
    }
    Ok(hasher
        .finalize()
        .iter()
        .map(|b| format!("{:02x}", b))
        .collect())
}

fn http_client() -> Result<reqwest::blocking::Client, String> {
    reqwest::blocking::Client::builder()
        .connect_timeout(Duration::from_secs(CONNECT_TIMEOUT_SECS))
        // Sin deadline global: un peso de ~900 MB puede tardar minutos en
        // conexiones lentas y eso no es un error. El keepalive es lo que
        // termina rompiendo una conexión que se quedó muerta a mitad.
        .timeout(None)
        .tcp_keepalive(Duration::from_secs(TCP_KEEPALIVE_SECS))
        .user_agent("flowsight-agent")
        .build()
        .map_err(|e| format!("Cannot create HTTP client: {}", e))
}

/// Reanuda `<dest>.part` con `Range` si ya había bytes; si el servidor ignora
/// el rango (200 en vez de 206), reempieza desde cero.
fn stream_to_part(
    base_url: &str,
    asset: &WeightAsset,
    part: &Path,
    on_event: &mut dyn FnMut(FetchEvent),
) -> Result<(), String> {
    let mut resume_from = fs::metadata(part).map(|m| m.len()).unwrap_or(0);
    if resume_from >= asset.size_bytes {
        resume_from = 0;
    }

    let url = format!("{}/{}", base_url, asset.filename);
    let mut request = http_client()?.get(url);
    if resume_from > 0 {
        request = request.header(reqwest::header::RANGE, format!("bytes={}-", resume_from));
    }

    let mut response = request.send().map_err(|e| download_error(asset, &e))?;
    let status = response.status();
    if !status.is_success() {
        return Err(format!(
            "Could not download {} ({}). The model release may be unavailable; retry later.",
            asset.filename, status
        ));
    }

    let offset = if status == reqwest::StatusCode::PARTIAL_CONTENT {
        resume_from
    } else {
        0
    };
    let total = response
        .content_length()
        .map(|n| n + offset)
        .unwrap_or(asset.size_bytes);

    let mut file = OpenOptions::new()
        .create(true)
        .write(true)
        .open(part)
        .map_err(|e| format!("Cannot write {:?}: {}", part, e))?;
    file.set_len(offset)
        .map_err(|e| format!("Cannot resize {:?}: {}", part, e))?;
    file.seek(SeekFrom::End(0))
        .map_err(|e| format!("Cannot seek {:?}: {}", part, e))?;

    let mut downloaded = offset;
    let mut buf = vec![0u8; STREAM_BUFFER_BYTES];

    loop {
        let read = response
            .read(&mut buf)
            .map_err(|e| format!("Connection lost while downloading {}: {}", asset.filename, e))?;
        if read == 0 {
            break;
        }
        file.write_all(&buf[..read])
            .map_err(|e| format!("Cannot write {:?} (disk full?): {}", part, e))?;
        downloaded += read as u64;
        on_event(FetchEvent::Progress {
            downloaded,
            total,
        });
    }

    file.flush()
        .map_err(|e| format!("Cannot flush {:?}: {}", part, e))?;
    Ok(())
}

fn download_error(asset: &WeightAsset, e: &reqwest::Error) -> String {
    if e.is_connect() || e.is_timeout() {
        format!(
            "No connection while downloading the local AI model ({}). Connect to the internet and retry.",
            asset.filename
        )
    } else {
        format!("Could not download {}: {}", asset.filename, e)
    }
}

fn percent_of(done: u64, total: u64) -> u8 {
    if total == 0 {
        return 0;
    }
    ((done.min(total) * 100) / total) as u8
}

/// Estado de los pesos para que la UI decida si mostrar la primera descarga.
#[tauri::command]
pub fn local_model_status(app: AppHandle) -> Result<serde_json::Value, String> {
    let ready = resolved_vision_weights(&app).is_some();
    let missing: Vec<&str> = VISION_ASSETS
        .iter()
        .filter(|a| resolve_weight(&app, a).is_none())
        .map(|a| a.filename)
        .collect();
    Ok(serde_json::json!({
        "ready": ready,
        "missing": missing,
        "totalBytes": total_download_bytes(),
        "modelsDir": models_dir(&app).ok().map(|d| d.to_string_lossy().to_string()),
    }))
}

/// Descarga los pesos que falten. Idempotente y seguro de reintentar.
///
/// Async para que Tauri lo despache fuera del hilo de UI: la descarga puede
/// durar minutos y el renderer necesita seguir pintando el progreso.
#[tauri::command]
pub async fn download_local_model(app: AppHandle) -> Result<serde_json::Value, String> {
    tauri::async_runtime::spawn_blocking(move || {
        ensure_vision_weights(&app).map(|(model, mmproj)| {
            serde_json::json!({
                "ready": true,
                "model": model.to_string_lossy(),
                "mmproj": mmproj.to_string_lossy(),
            })
        })
    })
    .await
    .map_err(|e| format!("Task join error: {}", e))?
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn part_path_appends_suffix_without_replacing_extension() {
        let dest = PathBuf::from("/models/Qwen3-VL-2B-Instruct-Q3_K_M.gguf");
        assert_eq!(
            part_path(&dest),
            PathBuf::from("/models/Qwen3-VL-2B-Instruct-Q3_K_M.gguf.part")
        );
    }

    #[test]
    fn weight_complete_requires_exact_size() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("w.gguf");
        fs::write(&path, b"1234").unwrap();

        let exact = WeightAsset {
            filename: "w.gguf",
            size_bytes: 4,
            sha256: "",
        };
        let truncated = WeightAsset {
            filename: "w.gguf",
            size_bytes: 8,
            sha256: "",
        };
        assert!(weight_complete(&path, &exact));
        assert!(!weight_complete(&path, &truncated));
        assert!(!weight_complete(&dir.path().join("missing.gguf"), &exact));
    }

    #[test]
    fn sha256_file_matches_known_digest() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("abc.bin");
        fs::write(&path, b"abc").unwrap();
        assert_eq!(
            sha256_file(&path).unwrap(),
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
    }

    #[test]
    fn verify_part_rejects_wrong_digest() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("w.gguf");
        fs::write(&path, b"abc").unwrap();
        let asset = WeightAsset {
            filename: "w.gguf",
            size_bytes: 3,
            sha256: "0000000000000000000000000000000000000000000000000000000000000000",
        };
        assert!(verify_part(&path, &asset).is_err());
    }

    #[test]
    fn percent_of_is_clamped() {
        assert_eq!(percent_of(0, 0), 0);
        assert_eq!(percent_of(50, 100), 50);
        assert_eq!(percent_of(200, 100), 100);
    }

    #[test]
    fn release_base_url_points_at_the_configured_tag() {
        temp_env::with_vars(
            [
                ("FLOWSIGHT_MODELS_REPO", Some("owner/repo")),
                ("FLOWSIGHT_MODELS_TAG", Some("models-v9")),
            ],
            || {
                assert_eq!(
                    release_base_url(),
                    "https://github.com/owner/repo/releases/download/models-v9"
                );
            },
        );
    }

    /// Servidor local que sirve un asset de prueba con soporte opcional de
    /// `Range`, para ejercitar descarga, reanudación e integridad sin red.
    struct AssetServer {
        base_url: String,
        _thread: std::thread::JoinHandle<()>,
    }

    impl AssetServer {
        fn start(filename: &'static str, body: Vec<u8>, honor_range: bool) -> Self {
            let server = tiny_http::Server::http("127.0.0.1:0").unwrap();
            let base_url = format!("http://{}", server.server_addr());
            let expected_path = format!("/{}", filename);
            let thread = std::thread::spawn(move || {
                while let Ok(request) = server.recv() {
                    if request.url() != expected_path {
                        let _ = request.respond(tiny_http::Response::empty(404));
                        continue;
                    }
                    let range_start = request
                        .headers()
                        .iter()
                        .find(|h| h.field.equiv("Range"))
                        .and_then(|h| h.value.as_str().strip_prefix("bytes=")?.split('-').next()?.parse::<usize>().ok())
                        .filter(|_| honor_range);

                    let response = match range_start {
                        Some(start) if start < body.len() => {
                            let slice = body[start..].to_vec();
                            let len = slice.len();
                            tiny_http::Response::new(
                                tiny_http::StatusCode(206),
                                vec![],
                                std::io::Cursor::new(slice),
                                Some(len),
                                None,
                            )
                        }
                        _ => {
                            let slice = body.clone();
                            let len = slice.len();
                            tiny_http::Response::new(
                                tiny_http::StatusCode(200),
                                vec![],
                                std::io::Cursor::new(slice),
                                Some(len),
                                None,
                            )
                        }
                    };
                    let _ = request.respond(response);
                }
            });
            Self {
                base_url,
                _thread: thread,
            }
        }
    }

    fn test_body(len: usize) -> Vec<u8> {
        (0..len).map(|i| (i % 251) as u8).collect()
    }

    fn sha256_bytes(bytes: &[u8]) -> String {
        let mut hasher = Sha256::new();
        hasher.update(bytes);
        hasher
            .finalize()
            .iter()
            .map(|b| format!("{:02x}", b))
            .collect()
    }

    fn noop_events(_: FetchEvent) {}

    #[test]
    fn fetch_asset_downloads_verifies_and_renames_atomically() {
        let body = test_body(300_000);
        let asset = WeightAsset {
            filename: "tiny.gguf",
            size_bytes: body.len() as u64,
            sha256: Box::leak(sha256_bytes(&body).into_boxed_str()),
        };
        let server = AssetServer::start(asset.filename, body.clone(), true);
        let dir = tempfile::tempdir().unwrap();
        let dest = dir.path().join(asset.filename);

        let mut seen_verifying = false;
        let mut max_downloaded = 0u64;
        let mut on_event = |event: FetchEvent| match event {
            FetchEvent::Progress { downloaded, .. } => max_downloaded = max_downloaded.max(downloaded),
            FetchEvent::Verifying => seen_verifying = true,
        };

        fetch_asset(&server.base_url, &asset, &dest, &mut on_event).unwrap();

        assert_eq!(fs::read(&dest).unwrap(), body);
        assert!(!part_path(&dest).exists());
        assert!(seen_verifying);
        assert_eq!(max_downloaded, body.len() as u64);
    }

    #[test]
    fn fetch_asset_resumes_from_existing_part() {
        let body = test_body(300_000);
        let asset = WeightAsset {
            filename: "resume.gguf",
            size_bytes: body.len() as u64,
            sha256: Box::leak(sha256_bytes(&body).into_boxed_str()),
        };
        let server = AssetServer::start(asset.filename, body.clone(), true);
        let dir = tempfile::tempdir().unwrap();
        let dest = dir.path().join(asset.filename);
        let already = 100_000;
        fs::write(part_path(&dest), &body[..already]).unwrap();

        let mut first_downloaded = None;
        let mut on_event = |event: FetchEvent| {
            if let FetchEvent::Progress { downloaded, .. } = event {
                first_downloaded.get_or_insert(downloaded);
            }
        };

        fetch_asset(&server.base_url, &asset, &dest, &mut on_event).unwrap();

        assert_eq!(fs::read(&dest).unwrap(), body);
        assert!(first_downloaded.unwrap() > already as u64);
    }

    #[test]
    fn fetch_asset_restarts_when_server_ignores_range() {
        let body = test_body(200_000);
        let asset = WeightAsset {
            filename: "norange.gguf",
            size_bytes: body.len() as u64,
            sha256: Box::leak(sha256_bytes(&body).into_boxed_str()),
        };
        let server = AssetServer::start(asset.filename, body.clone(), false);
        let dir = tempfile::tempdir().unwrap();
        let dest = dir.path().join(asset.filename);
        fs::write(part_path(&dest), &body[..50_000]).unwrap();

        fetch_asset(&server.base_url, &asset, &dest, &mut noop_events).unwrap();

        assert_eq!(fs::read(&dest).unwrap(), body);
    }

    #[test]
    fn fetch_asset_discards_content_that_fails_the_digest() {
        let body = test_body(120_000);
        let asset = WeightAsset {
            filename: "corrupt.gguf",
            size_bytes: body.len() as u64,
            sha256: "0000000000000000000000000000000000000000000000000000000000000000",
        };
        let server = AssetServer::start(asset.filename, body, true);
        let dir = tempfile::tempdir().unwrap();
        let dest = dir.path().join(asset.filename);

        let err = fetch_asset(&server.base_url, &asset, &dest, &mut noop_events).unwrap_err();

        assert!(err.contains("Integrity check failed"), "{err}");
        assert!(!dest.exists());
        assert!(!part_path(&dest).exists());
    }

    #[test]
    #[ignore = "hits the real models release (~445 MB)"]
    fn fetch_real_mmproj_asset_from_the_models_release() {
        let asset = &VISION_ASSETS[1];
        let dir = tempfile::tempdir().unwrap();
        let dest = dir.path().join(asset.filename);

        fetch_asset(&release_base_url(), asset, &dest, &mut noop_events).unwrap();

        assert!(weight_complete(&dest, asset));
    }

    #[test]
    fn fetch_asset_reports_a_missing_asset_instead_of_writing_junk() {
        let asset = WeightAsset {
            filename: "absent.gguf",
            size_bytes: 10,
            sha256: "0000000000000000000000000000000000000000000000000000000000000000",
        };
        let server = AssetServer::start("other.gguf", test_body(10), true);
        let dir = tempfile::tempdir().unwrap();
        let dest = dir.path().join(asset.filename);

        let err = fetch_asset(&server.base_url, &asset, &dest, &mut noop_events).unwrap_err();

        assert!(err.contains("Could not download"), "{err}");
        assert!(!dest.exists());
    }
}
