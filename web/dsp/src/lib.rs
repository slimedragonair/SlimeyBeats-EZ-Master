mod import_limits;
mod live_eq;
mod session;
mod studio;
mod wav_output;

use mastercraft::{config::Settings, profiles};
use wasm_bindgen::prelude::*;

fn failure(error: impl std::fmt::Display) -> JsValue {
    JsValue::from_str(&error.to_string())
}

pub(crate) fn settings(json: &str) -> anyhow::Result<Settings> {
    let settings: Settings = serde_json::from_str(json)?;
    settings.validate()?;
    Ok(settings)
}

// Only bundled profiles are reachable; browser jobs never try filesystem resolution.
pub(crate) fn profile(name: &str) -> anyhow::Result<serde_json::Value> {
    let all = profiles::builtin();
    for part in name.split('+') {
        let key = part.split(':').next().unwrap_or("");
        anyhow::ensure!(all.contains_key(key), "Choose a bundled tonal profile");
    }
    profiles::resolve(name, std::path::Path::new(""))
}

#[wasm_bindgen]
pub fn defaults() -> String {
    serde_json::json!({"settings":Settings::default(),"profiles":profiles::builtin(),"import_limits":{
        "max_seconds":import_limits::MAX_SECONDS,
        "max_decoded_bytes":import_limits::MAX_DECODED_BYTES,
        "max_file_bytes":import_limits::MAX_FILE_BYTES
    }}).to_string()
}
