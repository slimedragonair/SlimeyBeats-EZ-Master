//! Local, transactional recovery storage. No original input/project/export paths are opened.
mod blobs;
mod cleanup;
mod filesystem;
mod history;
mod initialization;
mod manifest;
mod storage;
mod tombstones;

#[cfg(test)]
mod cleanup_retry_tests;
#[cfg(test)]
mod deletion_tests;
#[cfg(test)]
mod float_tests;
#[cfg(test)]
mod resource_tests;
#[cfg(test)]
mod tests;

pub use history::{cleanup_unused, discard_run, inspect, list_candidates, list_history, load};
pub use storage::Store;

use crate::audio::Audio;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::sync::Arc;

pub const DEFAULT_BUDGET_BYTES: u64 = 8 * 1024 * 1024 * 1024;
pub const MAX_ASSET_BYTES: u64 = 512 * 1024 * 1024;
pub const MAX_AUDIO_ATTACHMENTS: usize = 4096;
pub const MAX_METADATA_BYTES: u64 = 8 * 1024 * 1024;
pub const RETAINED_CHECKPOINTS: usize = 3;
const MAX_RUNS: usize = 256;
const MAX_BLOBS: usize = 4096;
const EXIT_RESERVE_BYTES: u64 = 512;
const VERSION: u32 = 1;
const KIND: &str = "slimeybeats-private-recovery";

/// Each audio index is one logical buffer identity. Content-equal indices load as
/// distinct Arcs; the caller restores shared lineage by reusing one index.
#[derive(Clone, Debug)]
pub struct Snapshot {
    pub payload: Value,
    pub audio: Vec<Arc<Audio>>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CheckpointRef {
    run_id: String,
    generation: u64,
}
impl CheckpointRef {
    pub fn run_id(&self) -> &str {
        &self.run_id
    }
    pub fn generation(&self) -> u64 {
        self.generation
    }
}

#[derive(Clone, Debug)]
pub struct CheckpointInfo {
    pub checkpoint: CheckpointRef,
    pub captured_unix_ms: u64,
    pub interrupted: bool,
    pub audio_attachments: usize,
    pub unique_audio_bytes: u64,
    pub decoded_audio_bytes: u64,
    /// The checkpoint committed successfully; older cleanup may need attention.
    pub maintenance_warning: Option<String>,
}

/// Corrupt generations are reported without hiding other intact recovery points.
#[derive(Clone, Debug, Default)]
pub struct Listing {
    pub checkpoints: Vec<CheckpointInfo>,
    pub warnings: Vec<String>,
}
