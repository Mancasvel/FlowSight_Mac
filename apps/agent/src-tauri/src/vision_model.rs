//! Local vision stack: on-disk filenames and llama.cpp API `model` id.
//! Filenames must match artifacts produced by `setup_llm.py` in `local_llm/`.

/// Stored config / abstract model label (shown in UI and persisted settings).
pub const CONFIG_VISION_MODEL_ID: &str = "FlowSight/local-vision";

/// GGUF weights filename under `local_llm/`.
pub const VISION_GGUF_FILENAME: &str = "Qwen3-VL-2B-Instruct-Q3_K_M.gguf";

/// Multimodal projector GGUF filename under `local_llm/`.
pub const VISION_MMPROJ_FILENAME: &str = "mmproj-Qwen3VL-2B-Instruct-Q8_0.gguf";

/// OpenAI-compatible `model` field for `POST /v1/chat/completions` to localhost llama-server.
/// Must match the id the server registers for the loaded checkpoint.
pub const LLAMA_CHAT_MODEL_ID: &str = "Qwen3-VL-2B-Instruct";

/// Label returned in health/status JSON for the renderer (no vendor name).
pub const VISION_STATUS_LABEL: &str = "FlowSight Local Vision";
