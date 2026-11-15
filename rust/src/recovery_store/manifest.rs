use super::{
    CheckpointInfo, CheckpointRef, MAX_AUDIO_ATTACHMENTS, MAX_METADATA_BYTES, filesystem as io,
};
use anyhow::{Result, ensure};
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::{collections::HashSet, path::Path};

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Owner {
    pub kind: String,
    pub version: u32,
}
impl Owner {
    pub fn new() -> Self {
        Self {
            kind: super::KIND.into(),
            version: super::VERSION,
        }
    }
    pub fn validate(&self) -> Result<()> {
        ensure!(
            self.kind == super::KIND && self.version == super::VERSION,
            "Not a supported owned recovery store"
        );
        Ok(())
    }
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Run {
    pub owner: Owner,
    pub run_id: String,
    pub created_unix_ms: u64,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct BlobRef {
    pub sha256: String,
    pub bytes: u64,
    pub sample_rate: u32,
    pub channels: usize,
    pub samples: u64,
}
impl BlobRef {
    pub fn validate(&self) -> Result<()> {
        ensure!(valid_hash(&self.sha256), "Invalid recovery blob digest");
        ensure!(
            (8000..=384000).contains(&self.sample_rate) && (1..=2).contains(&self.channels),
            "Invalid recovery audio layout"
        );
        ensure!(
            self.samples > 0 && self.samples.is_multiple_of(self.channels as u64),
            "Invalid recovery audio frames"
        );
        ensure!(
            self.bytes <= super::MAX_ASSET_BYTES
                && self.samples.checked_mul(4).and_then(|n| n.checked_add(44)) == Some(self.bytes),
            "Recovery audio exceeds its asset limit"
        );
        Ok(())
    }
}
pub(super) fn valid_hash(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|c| c.is_ascii_digit() || (b'a'..=b'f').contains(&c))
}
pub(super) fn valid_run(value: &str) -> bool {
    value.starts_with("run-")
        && (8..=96).contains(&value.len())
        && value
            .bytes()
            .all(|c| c.is_ascii_alphanumeric() || c == b'-' || c == b'_')
}
pub(super) fn validate_run(path: &Path, run_id: &str) -> Result<Run> {
    ensure!(valid_run(run_id), "Invalid recovery run ID");
    io::directory(path)?;
    let run: Run = io::read_json(&path.join("run.json"), 4096)?;
    run.owner.validate()?;
    ensure!(run.run_id == run_id, "Recovery run identity mismatch");
    Ok(run)
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Data {
    pub run_id: String,
    pub generation: u64,
    pub captured_unix_ms: u64,
    pub payload: Value,
    pub audio: Vec<BlobRef>,
}
pub(super) struct Checkpoint {
    pub owner: Owner,
    pub sha256: String,
    pub data: Data,
    data_json: String,
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct WireCheckpoint {
    owner: Owner,
    sha256: String,
    data_json: String,
}
impl Serialize for Checkpoint {
    fn serialize<S: Serializer>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error> {
        use serde::ser::SerializeStruct;
        let mut state = serializer.serialize_struct("WireCheckpoint", 3)?;
        state.serialize_field("owner", &self.owner)?;
        state.serialize_field("sha256", &self.sha256)?;
        state.serialize_field("data_json", &self.data_json)?;
        state.end()
    }
}
impl<'de> Deserialize<'de> for Checkpoint {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> std::result::Result<Self, D::Error> {
        use serde::de::Error;
        let wire = WireCheckpoint::deserialize(deserializer)?;
        wire.owner.validate().map_err(D::Error::custom)?;
        if wire.data_json.len() as u64 > MAX_METADATA_BYTES
            || !valid_hash(&wire.sha256)
            || hash(wire.data_json.as_bytes()) != wire.sha256
        {
            return Err(D::Error::custom("Recovery checkpoint checksum mismatch"));
        }
        // Integrity covers the exact saved UTF-8 bytes, before parsing any f64.
        // Parser precision/features and object ordering cannot change this digest.
        let data = serde_json::from_str(&wire.data_json).map_err(D::Error::custom)?;
        Ok(Self {
            owner: wire.owner,
            sha256: wire.sha256,
            data,
            data_json: wire.data_json,
        })
    }
}
impl Checkpoint {
    pub fn new(data: Data) -> Result<Self> {
        let encoded = io::json_bytes(&data, MAX_METADATA_BYTES)?;
        let sha256 = hash(&encoded);
        let data_json = String::from_utf8(encoded)?;
        Ok(Self {
            owner: Owner::new(),
            sha256,
            data,
            data_json,
        })
    }
    pub fn validate(&self, id: &CheckpointRef) -> Result<()> {
        self.owner.validate()?;
        ensure!(
            valid_run(&id.run_id) && id.generation > 0,
            "Invalid recovery checkpoint reference"
        );
        ensure!(
            self.data.run_id == id.run_id && self.data.generation == id.generation,
            "Recovery checkpoint identity mismatch"
        );
        ensure!(
            self.data.audio.len() <= MAX_AUDIO_ATTACHMENTS,
            "Too many recovery audio attachments"
        );
        for audio in &self.data.audio {
            audio.validate()?;
        }
        let decoded_bytes: u64 = self.data.audio.iter().map(|audio| audio.samples * 4).sum();
        ensure!(
            decoded_bytes <= super::DEFAULT_BUDGET_BYTES,
            "Recovery decoded attachments exceed 8 GiB; duplicate content indices still count"
        );
        ensure!(
            hash(self.data_json.as_bytes()) == self.sha256,
            "Recovery checkpoint checksum mismatch"
        );
        Ok(())
    }
    pub fn info(&self, interrupted: bool) -> CheckpointInfo {
        let mut seen = HashSet::new();
        CheckpointInfo {
            checkpoint: CheckpointRef {
                run_id: self.data.run_id.clone(),
                generation: self.data.generation,
            },
            captured_unix_ms: self.data.captured_unix_ms,
            interrupted,
            audio_attachments: self.data.audio.len(),
            unique_audio_bytes: self
                .data
                .audio
                .iter()
                .filter(|a| seen.insert(&a.sha256))
                .map(|a| a.bytes)
                .sum(),
            decoded_audio_bytes: self.data.audio.iter().map(|audio| audio.samples * 4).sum(),
            maintenance_warning: None,
        }
    }
}
pub(super) fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}
pub(super) fn hash(bytes: &[u8]) -> String {
    hex(&Sha256::digest(bytes))
}
pub(super) fn filename(generation: u64) -> String {
    format!("{generation:020}.json")
}
pub(super) fn generation(name: &str) -> Option<u64> {
    let digits = name.strip_suffix(".json")?;
    let n = digits.parse::<u64>().ok()?;
    (n > 0 && filename(n) == name).then_some(n)
}
pub(super) fn read_checkpoint(root: &Path, id: &CheckpointRef) -> Result<Checkpoint> {
    ensure!(
        valid_run(&id.run_id) && id.generation > 0,
        "Invalid recovery checkpoint reference"
    );
    let record: Checkpoint = io::read_json(
        &root
            .join("runs")
            .join(&id.run_id)
            .join("checkpoints")
            .join(filename(id.generation)),
        MAX_METADATA_BYTES,
    )?;
    record.validate(id)?;
    Ok(record)
}
