//! Browser Studio uses the same DSP as desktop; model/plugin runtimes remain native.
use crate::{failure, wav_output};
use anyhow::{Context, Result, ensure};
use mastercraft::{
    audio::Audio,
    config::Settings,
    studio::{
        spectral_repair::{self, RepairSelection},
        spectrogram,
        stem_mix::{self, MixSettings, Stem, StemKind, StemSet},
        voice_cleanup::{self, VoiceSettings},
    },
};
use std::{io::Cursor, sync::Arc};
use wasm_bindgen::prelude::*;

const MAX_STUDIO_BYTES: usize = 128 * 1024 * 1024;
#[derive(Clone)]
struct Revision {
    audio: Arc<Audio>,
    before: Option<Arc<Audio>>,
    bank: Option<StemSet>,
    mix: MixSettings,
    notes: Vec<String>,
}
#[wasm_bindgen]
#[derive(Clone)]
pub struct BrowserStudio {
    source: Option<Arc<Audio>>,
    bank: Option<StemSet>,
    staged: Vec<Stem>,
    current: Option<Arc<Audio>>,
    before: Option<Arc<Audio>>,
    undo: Vec<Revision>,
    redo: Vec<Revision>,
    notes: Vec<String>,
    mix: MixSettings,
}
impl BrowserStudio {
    fn direct_edit_stems(&self) -> Vec<StemKind> {
        match (&self.bank, &self.before) {
            (Some(bank), Some(before)) => bank
                .tracks
                .iter()
                .filter(|stem| Arc::ptr_eq(before, &stem.audio))
                .map(|stem| stem.kind)
                .collect(),
            _ => vec![],
        }
    }
    fn checked(samples: Vec<f32>, rate: u32, channels: u32) -> Result<Audio> {
        crate::import_limits::validate(samples.len(), rate, channels)?;
        ensure!(
            samples.len() <= MAX_STUDIO_BYTES / 4,
            "Browser Studio input exceeds 128 MiB decoded PCM"
        );
        Audio::new(samples, rate, channels as usize)
    }
    fn stage_stem_checked(
        &mut self,
        kind: &str,
        samples: Vec<f32>,
        rate: u32,
        channels: u32,
    ) -> Result<()> {
        let kind = serde_json::from_value(serde_json::Value::String(kind.into()))?;
        ensure!(
            !self.staged.iter().any(|stem| stem.kind == kind),
            "Each stem kind must be unique; use a different part_N identity for each additional part"
        );
        let incoming = samples
            .len()
            .checked_mul(size_of::<f32>())
            .context("Stem import is too large")?;
        let total = self.staged.iter().try_fold(incoming, |total, stem| {
            stem.audio
                .samples
                .len()
                .checked_mul(size_of::<f32>())
                .and_then(|bytes| total.checked_add(bytes))
                .context("Stem import is too large")
        })?;
        ensure!(
            total <= MAX_STUDIO_BYTES,
            "Combined browser stems exceed 128 MiB decoded PCM. Use shorter/lower-rate stems or the desktop app."
        );
        let audio = Self::checked(samples, rate, channels)?;
        self.staged.push(Stem {
            kind,
            audio: Arc::new(audio),
        });
        Ok(())
    }
    fn commit_stems_checked(&mut self) -> Result<()> {
        let bank = StemSet::new(self.staged.clone())?;
        let controls = bank.defaults();
        let mut raw = controls.clone();
        raw.protect_peaks = false;
        let original = Arc::new(stem_mix::render(&bank, &raw, &|| false)?);
        let preview = Arc::new(stem_mix::render(&bank, &controls, &|| false)?);
        // Validate and render both mixes before replacing the committed workspace.
        self.staged.clear();
        self.bank = Some(bank);
        self.source = Some(original);
        self.before = self.source.clone();
        self.current = Some(preview);
        self.mix = controls;
        self.undo.clear();
        self.redo.clear();
        Ok(())
    }
    fn push(&mut self, audio: Audio) {
        if let Some(previous) = self.current.take() {
            self.undo.push(Revision {
                audio: previous,
                before: self.before.clone(),
                bank: self.bank.clone(),
                mix: self.mix.clone(),
                notes: self.notes.clone(),
            });
        }
        self.current = Some(Arc::new(audio));
        self.redo.clear();
        while self.undo.len() > 2
            || self
                .undo
                .iter()
                .map(|a| a.audio.samples.len() * 4)
                .sum::<usize>()
                > MAX_STUDIO_BYTES
        {
            if self.undo.is_empty() {
                break;
            }
            self.undo.remove(0);
        }
    }
    fn audio(&self, target: &str) -> Result<Arc<Audio>> {
        match target {
            "source" => self.source.clone().context("Load a Studio source first"),
            "preview" => self
                .current
                .clone()
                .context("Prepare a Studio preview first"),
            name => {
                let kind: StemKind =
                    serde_json::from_value(serde_json::Value::String(name.into()))?;
                Ok(self
                    .bank
                    .as_ref()
                    .context("Import aligned stems first")?
                    .track(kind)?
                    .audio
                    .clone())
            }
        }
    }
}
#[wasm_bindgen]
impl BrowserStudio {
    #[wasm_bindgen(constructor)]
    pub fn new() -> BrowserStudio {
        Self {
            source: None,
            bank: None,
            staged: vec![],
            current: None,
            before: None,
            undo: vec![],
            redo: vec![],
            notes: vec![],
            mix: MixSettings::default(),
        }
    }
    pub fn fork(&self) -> BrowserStudio {
        self.clone()
    }
    pub fn load(&mut self, samples: Vec<f32>, rate: u32, channels: u32) -> Result<(), JsValue> {
        let audio = Arc::new(Self::checked(samples, rate, channels).map_err(failure)?);
        self.source = Some(audio.clone());
        self.current = Some(audio.clone());
        self.before = Some(audio);
        self.bank = None;
        self.staged.clear();
        self.undo.clear();
        self.redo.clear();
        self.notes.clear();
        Ok(())
    }
    pub fn stage_stem(
        &mut self,
        kind: &str,
        samples: Vec<f32>,
        rate: u32,
        channels: u32,
    ) -> Result<(), JsValue> {
        self.stage_stem_checked(kind, samples, rate, channels)
            .map_err(failure)
    }
    pub fn commit_stems(&mut self) -> Result<String, JsValue> {
        self.commit_stems_checked().map_err(failure)?;
        self.info()
    }
    /// Attach plain text metadata after commit_stems; stage_stem remains compatible.
    pub fn set_stem_display_name(&mut self, kind: &str, name: &str) -> Result<(), JsValue> {
        let kind =
            serde_json::from_value(serde_json::Value::String(kind.into())).map_err(failure)?;
        self.bank
            .as_mut()
            .ok_or_else(|| failure("Import stems first"))?
            .set_display_name(kind, name)
            .map_err(failure)
    }
    pub fn mix(&mut self, json: &str) -> Result<String, JsValue> {
        let settings: MixSettings = serde_json::from_str(json).map_err(failure)?;
        let bank = self
            .bank
            .as_ref()
            .ok_or_else(|| failure("Import stems first"))?;
        let audio = stem_mix::render(bank, &settings, &|| false).map_err(failure)?;
        self.push(audio);
        self.before = self.source.clone();
        self.mix = settings;
        self.notes.clear();
        self.info()
    }
    pub fn voice(&mut self, target: &str, json: &str) -> Result<String, JsValue> {
        let input = self.audio(target).map_err(failure)?;
        let settings: VoiceSettings = serde_json::from_str(json).map_err(failure)?;
        let result = voice_cleanup::render(&input, &settings, &|| false).map_err(failure)?;
        self.push(result.audio);
        self.before = Some(input);
        self.notes = result.notes;
        self.info()
    }
    pub fn repair(&mut self, target: &str, json: &str) -> Result<String, JsValue> {
        let input = self.audio(target).map_err(failure)?;
        let selection: RepairSelection = serde_json::from_str(json).map_err(failure)?;
        let result = spectral_repair::render(&input, &selection, &|| false).map_err(failure)?;
        self.push(result);
        self.before = Some(input);
        self.notes.clear();
        self.info()
    }
    pub fn replace_stem(&mut self, kind: &str) -> Result<String, JsValue> {
        let kind =
            serde_json::from_value(serde_json::Value::String(kind.into())).map_err(failure)?;
        let audio = self
            .current
            .clone()
            .ok_or_else(|| failure("Prepare an edited stem preview first"))?;
        if !self.direct_edit_stems().contains(&kind) {
            return Err(failure(
                "Apply a direct edit from this stem; a full mix cannot replace a stem",
            ));
        }
        let bank = self
            .bank
            .as_ref()
            .ok_or_else(|| failure("Import stems first"))?
            .replace(kind, audio)
            .map_err(failure)?;
        let mixed = stem_mix::render(&bank, &self.mix, &|| false).map_err(failure)?;
        self.push(mixed);
        self.before = self.source.clone();
        self.bank = Some(bank);
        self.info()
    }
    pub fn history(&mut self, redo: bool) -> Result<String, JsValue> {
        let next = if redo {
            self.redo.pop()
        } else {
            self.undo.pop()
        }
        .ok_or_else(|| failure("No retained history step"))?;
        if let Some(current) = self.current.replace(next.audio) {
            let current = Revision {
                audio: current,
                before: self.before.take(),
                bank: self.bank.take(),
                mix: self.mix.clone(),
                notes: self.notes.clone(),
            };
            if redo {
                self.undo.push(current);
            } else {
                self.redo.push(current);
            }
        }
        self.bank = next.bank;
        self.before = next.before;
        self.mix = next.mix;
        self.notes = next.notes;
        self.info()
    }
    pub fn spectrogram(&self, target: &str) -> Result<String, JsValue> {
        let audio = self.audio(target).map_err(failure)?;
        serde_json::to_string(&spectrogram::build(&audio, 256, 96, &|| false).map_err(failure)?)
            .map_err(failure)
    }
    pub fn analyze(&self, target: &str) -> Result<String, JsValue> {
        let audio = self.audio(target).map_err(failure)?;
        let analysis =
            mastercraft::analysis::analyze(&audio, &crate::profile("neutral").map_err(failure)?)
                .map_err(failure)?;
        serde_json::to_string(&analysis.metrics).map_err(failure)
    }
    pub fn info(&self) -> Result<String, JsValue> {
        let a = self.current.as_ref();
        Ok(serde_json::json!({
            "loaded":a.is_some(),
            "duration_seconds":a.map(|a|a.duration()),
            "sample_rate":a.map(|a|a.sample_rate),
            "channels":a.map(|a|a.channels),
            "controls":self.mix,
            "notes":self.notes,
            "direct_edit_stems":self.direct_edit_stems(),
            "can_undo":!self.undo.is_empty(),
            "can_redo":!self.redo.is_empty(),
            "stems":self.bank.as_ref().map(|b|b.tracks.iter().map(|s|s.kind.name().into_owned()).collect::<Vec<_>>()).unwrap_or_default(),
            "stem_names":self.bank.as_ref().map(|b|b.tracks.iter().map(|s|(s.kind.name().into_owned(),b.display_name(s.kind).into_owned())).collect::<std::collections::BTreeMap<_,_>>()).unwrap_or_default(),
            "max_stems":null,
            "max_combined_decoded_bytes":MAX_STUDIO_BYTES,
            "max_stem_seconds":crate::import_limits::MAX_SECONDS
        }).to_string())
    }
    pub fn preview_ptr(&self) -> Result<*const f32, JsValue> {
        Ok(self
            .current
            .as_ref()
            .ok_or_else(|| failure("Load Studio audio first"))?
            .samples
            .as_ptr())
    }
    pub fn before_ptr(&self) -> Result<*const f32, JsValue> {
        Ok(self
            .before
            .as_ref()
            .ok_or_else(|| failure("Load Studio audio first"))?
            .samples
            .as_ptr())
    }
    pub fn before_len(&self) -> Result<usize, JsValue> {
        Ok(self
            .before
            .as_ref()
            .ok_or_else(|| failure("Load Studio audio first"))?
            .samples
            .len())
    }
    pub fn preview_len(&self) -> Result<usize, JsValue> {
        Ok(self
            .current
            .as_ref()
            .ok_or_else(|| failure("Load Studio audio first"))?
            .samples
            .len())
    }
    pub fn export(&self, format: &str) -> Result<Vec<u8>, JsValue> {
        let audio = self
            .current
            .as_ref()
            .ok_or_else(|| failure("Prepare a Studio preview first"))?;
        let settings = Settings::default();
        let (bytes, _, _, _) =
            wav_output::encode_checked((**audio).clone(), format, &settings).map_err(failure)?;
        Ok(bytes)
    }
    pub fn stem_wav(&self, kind: &str) -> Result<Vec<u8>, JsValue> {
        let audio = self.audio(kind).map_err(failure)?;
        let mut cursor = Cursor::new(Vec::with_capacity(audio.samples.len() * 4 + 128));
        mastercraft::wav_encoding::write_wav(&audio, &mut cursor, "wav32f", false)
            .map_err(failure)?;
        Ok(cursor.into_inner())
    }
    pub fn current_wav32(&self) -> Result<Vec<u8>, JsValue> {
        let audio = self
            .current
            .as_ref()
            .ok_or_else(|| failure("Load Studio audio first"))?;
        let mut cursor = Cursor::new(Vec::with_capacity(audio.samples.len() * 4 + 128));
        mastercraft::wav_encoding::write_wav(audio, &mut cursor, "wav32f", false)
            .map_err(failure)?;
        Ok(cursor.into_inner())
    }
    pub fn parameter_defaults() -> String {
        serde_json::json!({
            "min_stems":2,
            "max_stems":null,
            "stem_kinds":StemKind::ALL,
            "generic_stem_kind_pattern":"^part_[1-9][0-9]*$",
            "max_combined_decoded_bytes":MAX_STUDIO_BYTES,
            "max_stem_seconds":crate::import_limits::MAX_SECONDS,
            "voice":VoiceSettings::default(),
            "repair":{"start_seconds":0.0,"end_seconds":1.0,"low_hz":2000.0,"high_hz":8000.0,"reduction_db":-12.0,"mode":"attenuate"}
        }).to_string()
    }
}
impl Default for BrowserStudio {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
#[path = "studio_large_stem_tests.rs"]
mod large_stem_tests;

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::{Value, json};

    fn names(studio: &BrowserStudio) -> Value {
        serde_json::from_str::<Value>(&studio.info().unwrap()).unwrap()["stem_names"].clone()
    }

    #[test]
    fn display_names_survive_browser_fork_edit_replace_and_history() {
        let mut studio = BrowserStudio::new();
        assert_eq!(names(&studio), json!({}));
        studio
            .stage_stem("vocals", vec![0.02; 800], 8000, 1)
            .unwrap();
        studio
            .stage_stem("part_1", vec![0.04; 800], 8000, 1)
            .unwrap();
        studio.commit_stems().unwrap();
        assert_eq!(
            names(&studio),
            json!({"vocals":"vocals", "part_1":"part_1"})
        );
        studio
            .set_stem_display_name("vocals", "Song (Vocals)")
            .unwrap();
        studio
            .set_stem_display_name("part_1", "Song (Backing Vocals)")
            .unwrap();
        let expected = names(&studio);
        let mut fork = studio.fork();
        fork.set_stem_display_name("part_1", "Different backing name")
            .unwrap();
        assert_eq!(names(&studio), expected);
        assert_ne!(names(&fork), expected);
        let mut mix = studio.mix.clone();
        mix.protect_peaks = false;
        mix.tracks[0].solo = true;
        studio.mix(&serde_json::to_string(&mix).unwrap()).unwrap();
        assert_eq!(studio.current.as_ref().unwrap().samples, vec![0.02; 800]);
        assert_eq!(names(&studio), expected);
        studio.before = Some(studio.audio("vocals").unwrap());
        studio.current = Some(Arc::new(Audio::new(vec![0.01; 800], 8000, 1).unwrap()));
        studio.replace_stem("vocals").unwrap();
        assert_eq!(studio.audio("vocals").unwrap().samples, vec![0.01; 800]);
        assert_eq!(names(&studio), expected);
        studio.history(false).unwrap();
        assert_eq!(studio.audio("vocals").unwrap().samples, vec![0.02; 800]);
        assert_eq!(names(&studio), expected);
        studio.history(true).unwrap();
        assert_eq!(studio.audio("vocals").unwrap().samples, vec![0.01; 800]);
        assert_eq!(names(&studio), expected);
    }
}
