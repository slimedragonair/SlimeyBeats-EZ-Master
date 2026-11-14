//! Explicit, optional preparation of private FFmpeg tools. Lookup never downloads.
pub mod archive;
mod cache;
mod install;
mod paths;
mod pins;
mod probes;
mod readiness;
mod smoke;
mod tar;

use anyhow::Result;
pub use paths::Paths;
use std::{
    path::PathBuf,
    sync::{Arc, atomic::AtomicBool},
};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Runtime {
    pub ffmpeg: PathBuf,
    pub ffprobe: PathBuf,
    pub private: bool,
}

/// Whether this target has a pinned private bundle (Linux also needs system xz).
pub fn supported() -> bool {
    pins::current().is_some()
}

/// Prefer a functionally prepared PATH pair, then a receipt/hash-verified private pair.
/// Private lookup works in fresh processes without ensure, network or subprocesses.
pub fn locate(program: &str) -> Option<PathBuf> {
    let name = probes::program(program)?;
    probes::system(program).or_else(|| {
        Paths::user_cache().ok()?.private_runtime().map(|r| {
            if name == "ffmpeg" {
                r.ffmpeg
            } else {
                r.ffprobe
            }
        })
    })
}

pub fn available() -> bool {
    locate("ffmpeg").is_some() && locate("ffprobe").is_some()
}

/// The caller owns download consent/policy. In particular MCP must opt in explicitly.
pub fn ensure(progress: &dyn Fn(f32, &str), cancel: Arc<AtomicBool>) -> Result<Runtime> {
    if let Some(runtime) = probes::system_runtime(&cancel)? {
        progress(1.0, "Using system FFmpeg and ffprobe");
        return Ok(runtime);
    }
    Paths::user_cache()?.ensure(progress, cancel)
}

#[cfg(test)]
mod archive_tests;
#[cfg(test)]
mod readiness_tests;
#[cfg(test)]
mod tests;
