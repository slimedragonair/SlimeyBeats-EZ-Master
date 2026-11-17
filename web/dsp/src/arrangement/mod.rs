//! Arrangement bindings keep metadata transactions separate from explicit audio work.
mod archive;
mod commands;
mod exports;
mod imports;
mod playback;
mod project;
mod queries;

use crate::failure;
use anyhow::{Result, ensure};
use mastercraft::studio::{
    arrangement::{ArrangementPlayer, AssetValidationCache, Edit, MAX_METADATA_BYTES},
    arrangement_session::{ArrangementSession, OwnedAsset, SessionLimits},
};
use serde::Deserialize;
use serde_json::json;
use wasm_bindgen::prelude::*;

const MAX_SOURCE_BYTES: u64 = 128 * 1024 * 1024;
const BLOCK_FRAMES: usize = 2048;

#[derive(Deserialize)]
#[serde(untagged)]
enum Edits {
    One(Edit),
    Many(Vec<Edit>),
}
fn edits(json: &str) -> Result<Vec<Edit>> {
    ensure!(
        json.len() <= MAX_METADATA_BYTES,
        "Arrangement command exceeds 16 MiB"
    );
    Ok(match serde_json::from_str(json)? {
        Edits::One(edit) => vec![edit],
        Edits::Many(edits) => edits,
    })
}

#[wasm_bindgen]
pub struct BrowserArrangement {
    session: ArrangementSession,
    staged: Vec<OwnedAsset>,
    staged_revision: Option<u64>,
    player: Option<Box<ArrangementPlayer>>,
    player_revision: Option<u64>,
    player_side: String,
    cache: AssetValidationCache,
    block: Vec<f32>,
    block_len: usize,
}

impl BrowserArrangement {
    fn snapshot(&self) -> Result<String> {
        let mut value = serde_json::to_value(self.session.read())?;
        let revision = self.session.revision();
        value["preview"] = json!({"identity":format!("{}:{revision}:after", self.session.epoch()),
            "prepared_revision":self.player_revision,
            "pending":self.player_revision != Some(revision),
            "sample_rate":self.session.document().sample_rate,"channels":self.session.document().channels,
            "frames":self.session.document().duration_frames()});
        value["capabilities"] = json!({"playback":true,"scoped_eq":true,"suggest_eq":true,
            "metadata_commands":true,"waveform":true,"wav_export":true,"project":false,
            "max_decoded_bytes":MAX_SOURCE_BYTES,"max_metadata_bytes":MAX_METADATA_BYTES});
        Ok(serde_json::to_string(&value)?)
    }
}

#[wasm_bindgen]
impl BrowserArrangement {
    #[wasm_bindgen(constructor)]
    pub fn new(rate: u32, channels: u32, epoch: &str) -> Result<BrowserArrangement, JsValue> {
        let session = ArrangementSession::new(
            rate,
            channels as usize,
            epoch.into(),
            SessionLimits {
                max_decoded_bytes: MAX_SOURCE_BYTES,
                ..SessionLimits::default()
            },
        )
        .map_err(failure)?;
        Ok(Self {
            session,
            staged: vec![],
            staged_revision: None,
            player: None,
            player_revision: None,
            player_side: "after".into(),
            cache: AssetValidationCache::new(),
            block: vec![0.; BLOCK_FRAMES * 2],
            block_len: 0,
        })
    }
    pub fn read(&self) -> Result<String, JsValue> {
        self.snapshot().map_err(failure)
    }
}

#[cfg(test)]
mod tests;
