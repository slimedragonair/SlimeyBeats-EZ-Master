use serde_json::{Value, json};
use std::io::Read;
use std::path::PathBuf;

pub const MODEL_SHA256: &str = "3adb4a50ad4eb18c7c4d65fcf4cf2367a07d48408a5eb7d03cd20067429dfaa8";
pub const MODEL_URL: &str =
    "https://github.com/deezer/spleeter/releases/download/v1.4.0/4stems.tar.gz";
pub const RUNTIME_API: u64 = 1;

#[derive(Clone)]
pub struct RuntimePaths {
    pub base: PathBuf,
}
impl RuntimePaths {
    pub fn default_location() -> Self {
        #[cfg(windows)]
        if let Some(root) = std::env::var_os("LOCALAPPDATA")
            .map(PathBuf::from)
            .filter(|p| p.is_absolute())
        {
            return Self {
                base: root.join("SlimeyBeats-EZ-Master/studio"),
            };
        }
        Self {
            base: crate::paths::app_root().join("studio"),
        }
    }
    pub fn python(&self) -> PathBuf {
        self.base.join(if cfg!(windows) {
            "env/Scripts/python.exe"
        } else {
            "env/bin/python"
        })
    }
    pub fn models(&self) -> PathBuf {
        self.base.join("models/4stems")
    }
    pub fn receipt(&self) -> PathBuf {
        self.base.join("engine.json")
    }
    pub fn is_ready(&self) -> bool {
        let receipt = (|| -> std::io::Result<Vec<u8>> {
            let mut bytes = Vec::new();
            std::fs::File::open(self.receipt())?
                .take(65537)
                .read_to_end(&mut bytes)?;
            Ok(bytes)
        })()
        .ok()
        .filter(|bytes| bytes.len() < 65536)
        .and_then(|bytes| serde_json::from_slice::<Value>(&bytes).ok());
        self.python().is_file()
            && self.models().join("model.index").is_file()
            && receipt.is_some_and(|r| {
                r["runtime_api"] == RUNTIME_API && r["model_sha256"] == MODEL_SHA256
            })
    }
    pub fn status(&self) -> Value {
        json!({"ready":self.is_ready(),"engine":"Spleeter 4-stem / local CPU","model_release":"v1.4.0","model_sha256":MODEL_SHA256,"separation_modes":["vocal_instrumental","four_stems"],"max_decoded_input_bytes":536870912_u64,"network":"Setup downloads the optional runtime and upstream model. Separation itself uses local files only.","license":"The Spleeter code is MIT. Model files are third-party material from the upstream release and are not relicensed by this app."})
    }
}

#[derive(Clone)]
pub struct StemRuntime {
    pub python: PathBuf,
    pub models: PathBuf,
    pub(super) lock_path: Option<PathBuf>,
}
impl StemRuntime {
    /// Explicitly configured runtimes are useful for development/import. The
    /// normal application cache additionally serializes inference across apps.
    pub fn configured(python: PathBuf, models: PathBuf) -> Self {
        Self {
            python,
            models,
            lock_path: None,
        }
    }
}
impl From<&RuntimePaths> for StemRuntime {
    fn from(paths: &RuntimePaths) -> Self {
        Self {
            python: paths.python(),
            models: paths.models(),
            lock_path: Some(paths.base.join("engine.lock")),
        }
    }
}
