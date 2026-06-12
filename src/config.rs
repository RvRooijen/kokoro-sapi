//! Resolution of the assets directory (model, voices, espeak data, onnxruntime).

use std::path::PathBuf;

/// Order: explicit value from the voice token, then env var, then the default
/// install location used by scripts/download-assets.ps1.
pub fn assets_dir(from_token: Option<String>) -> PathBuf {
    if let Some(dir) = from_token {
        if !dir.is_empty() {
            return PathBuf::from(dir);
        }
    }
    if let Ok(dir) = std::env::var("KOKORO_SAPI_ASSETS") {
        if !dir.is_empty() {
            return PathBuf::from(dir);
        }
    }
    let local = std::env::var("LOCALAPPDATA").unwrap_or_else(|_| ".".into());
    PathBuf::from(local).join("KokoroSapi")
}
