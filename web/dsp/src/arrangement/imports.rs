use super::*;
use mastercraft::audio::Audio;
use std::{io::Cursor, sync::Arc};

impl BrowserArrangement {
    fn stage_checked(&mut self, id: &str, name: &str, audio: Audio) -> Result<()> {
        ensure!(
            id.len() <= 128 && !id.is_empty() && name.len() <= 512 && !name.trim().is_empty(),
            "Invalid arrangement source ID or label"
        );
        ensure!(
            !self.staged.iter().any(|asset| asset.id == id)
                && !self.session.assets().contains_key(id),
            "Arrangement source ID already exists"
        );
        crate::import_limits::validate(
            audio.samples.len(),
            audio.sample_rate,
            audio.channels as u32,
        )?;
        let existing: u64 = self
            .session
            .assets()
            .values()
            .map(|audio| audio.samples.len() as u64 * 4)
            .sum();
        let pending: u64 = self
            .staged
            .iter()
            .map(|asset| asset.audio.samples.len() as u64 * 4)
            .sum();
        ensure!(
            existing
                .checked_add(pending)
                .and_then(|bytes| bytes.checked_add(audio.samples.len() as u64 * 4))
                .is_some_and(|bytes| bytes <= MAX_SOURCE_BYTES),
            "Arrangement sources exceed 128 MiB decoded PCM"
        );
        ensure!(
            self.staged_revision
                .is_none_or(|revision| revision == self.session.revision()),
            "Pending import is stale; discard it and review again"
        );
        self.staged_revision = Some(self.session.revision());
        self.staged.push(OwnedAsset {
            id: id.into(),
            name: name.into(),
            audio: Arc::new(audio),
        });
        Ok(())
    }
    pub(super) fn commit_import_checked(&mut self, expected: u64, json: &str) -> Result<String> {
        ensure!(
            self.staged_revision == Some(expected),
            "Pending import revision is stale"
        );
        let edits = edits(json)?;
        self.session
            .add_assets(expected, self.staged.clone(), &edits, "Add tracks")?;
        self.staged.clear();
        self.staged_revision = None;
        self.player_revision = None;
        self.snapshot()
    }
}

#[wasm_bindgen]
impl BrowserArrangement {
    pub fn stage_asset(
        &mut self,
        id: &str,
        name: &str,
        samples: Vec<f32>,
        rate: u32,
        channels: u32,
    ) -> Result<(), JsValue> {
        let audio = Audio::new(samples, rate, channels as usize).map_err(failure)?;
        self.stage_checked(id, name, audio).map_err(failure)
    }
    pub fn stage_wav(&mut self, id: &str, name: &str, bytes: Vec<u8>) -> Result<(), JsValue> {
        let work = (|| -> Result<()> {
            ensure!(
                bytes.len() as u64 <= MAX_SOURCE_BYTES + 4096,
                "Arrangement WAV exceeds the browser input budget"
            );
            let mut reader = hound::WavReader::new(Cursor::new(bytes))?;
            let spec = reader.spec();
            ensure!(
                (reader.len() as u64)
                    .checked_mul(4)
                    .is_some_and(|bytes| bytes <= MAX_SOURCE_BYTES),
                "Decoded WAV exceeds 128 MiB"
            );
            let samples = if spec.sample_format == hound::SampleFormat::Float {
                reader
                    .samples::<f32>()
                    .collect::<std::result::Result<Vec<_>, _>>()?
            } else {
                let scale = 2_f64.powi(spec.bits_per_sample as i32 - 1);
                reader
                    .samples::<i32>()
                    .map(|sample| sample.map(|value| (value as f64 / scale) as f32))
                    .collect::<std::result::Result<Vec<_>, _>>()?
            };
            self.stage_checked(
                id,
                name,
                Audio::new(samples, spec.sample_rate, spec.channels as usize)?,
            )
        })();
        work.map_err(failure)
    }
    pub fn commit_import(&mut self, expected: u64, json: &str) -> Result<String, JsValue> {
        self.commit_import_checked(expected, json).map_err(failure)
    }
    pub fn discard_import(&mut self) {
        self.staged.clear();
        self.staged_revision = None;
    }
}
