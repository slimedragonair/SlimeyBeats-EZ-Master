#[path = "external_master.rs"]
mod external_master;

use crate::{failure, import_limits, profile, settings, wav_output};
use mastercraft::{
    analysis::{self, Analysis},
    audio::Audio,
    config::{Settings, default_eq},
    dsp::{eq_suggestion, manual_eq},
    engine,
    report::Report,
};
use wasm_bindgen::prelude::*;

#[wasm_bindgen]
pub struct Session {
    input: Audio,
    reference: Option<Analysis>,
    raw: Option<Audio>,
    report: Option<Report>,
    raw_reference_stale: bool,
}

impl Session {
    fn export_source(&self) -> anyhow::Result<&Audio> {
        let raw = self
            .raw
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("Master the track first"))?;
        anyhow::ensure!(
            !self.raw_reference_stale,
            "Reference changed. Master the track again before exporting."
        );
        Ok(raw)
    }
}

fn master_settings(json: &str) -> anyhow::Result<Settings> {
    let mut s = settings(json)?;
    // Both local and server masters remain the immutable pre-manual-EQ source.
    s.manual_eq = default_eq();
    s.eq_output_db = 0.;
    Ok(s)
}

#[wasm_bindgen]
impl Session {
    #[wasm_bindgen(constructor)]
    pub fn new(samples: Vec<f32>, rate: u32, channels: u32) -> Result<Session, JsValue> {
        import_limits::validate(samples.len(), rate, channels).map_err(failure)?;
        // Own the binding's input allocation instead of copying a whole track again.
        let input = Audio::new(samples, rate, channels as usize).map_err(failure)?;
        Ok(Self {
            input,
            reference: None,
            raw: None,
            report: None,
            raw_reference_stale: false,
        })
    }

    pub fn set_reference(
        &mut self,
        samples: Vec<f32>,
        rate: u32,
        channels: u32,
    ) -> Result<(), JsValue> {
        import_limits::validate(samples.len(), rate, channels).map_err(failure)?;
        let audio = Audio::new(samples, rate, channels as usize).map_err(failure)?;
        // Recovery and audition retain the current raw master, but the worker
        // must reject stale exports independently of the editor's UI guard.
        let reference =
            analysis::analyze(&audio, &profile("neutral").map_err(failure)?).map_err(failure)?;
        let changed = serde_json::to_value(&self.reference).map_err(failure)?
            != serde_json::to_value(&reference).map_err(failure)?;
        self.reference = Some(reference);
        // Recovery reloads the unchanged baseline reference after rebuilding
        // its master. That must preserve a current cache, never revive a stale one.
        self.raw_reference_stale |= changed && self.raw.is_some();
        Ok(())
    }

    pub fn analyze(&self, json: &str) -> Result<String, JsValue> {
        let s = settings(json).map_err(failure)?;
        let p = profile(&s.profile).map_err(failure)?;
        let a = analysis::analyze(&self.input, &p).map_err(failure)?;
        serde_json::to_string(&a).map_err(failure)
    }

    pub fn reference_analysis(&self) -> Result<String, JsValue> {
        serde_json::to_string(&self.reference).map_err(failure)
    }

    pub fn master(&mut self, json: &str, progress: &js_sys::Function) -> Result<String, JsValue> {
        let s = master_settings(json).map_err(failure)?;
        let p = profile(&s.profile).map_err(failure)?;
        // Always rebuild from the untouched original and retain a pre-manual-EQ source.
        let cb = |value: f32, text: &str| {
            let _ = progress.call2(&JsValue::NULL, &value.into(), &text.into());
        };
        let mastered = engine::master(
            &self.input,
            &s,
            &p,
            self.reference.as_ref(),
            "browser-track",
            &cb,
            &|| false,
        )
        .map_err(failure)?;
        let raw_analysis = mastered.report.output.clone();
        self.raw = Some(mastered.audio);
        self.report = Some(mastered.report);
        self.raw_reference_stale = false;
        Ok(serde_json::json!({"report":self.report,"raw_analysis":raw_analysis}).to_string())
    }

    pub fn preview_ptr(&self) -> Result<*const f32, JsValue> {
        Ok(self
            .raw
            .as_ref()
            .ok_or_else(|| failure("Master the track first"))?
            .samples
            .as_ptr())
    }

    pub fn preview_len(&self) -> Result<usize, JsValue> {
        Ok(self
            .raw
            .as_ref()
            .ok_or_else(|| failure("Master the track first"))?
            .samples
            .len())
    }

    pub fn suggest(&self, json: &str) -> Result<String, JsValue> {
        let s = settings(json).map_err(failure)?;
        let p = profile(&s.profile).map_err(failure)?;
        let source = self.raw.as_ref().unwrap_or(&self.input);
        let measurement = analysis::analyze(source, &p).map_err(failure)?;
        let bands = eq_suggestion::suggest(&measurement, &p).map_err(failure)?;
        serde_json::to_string(&bands).map_err(failure)
    }

    pub fn export_wav(&mut self, json: &str, format: &str) -> Result<Vec<u8>, JsValue> {
        let s = settings(json).map_err(failure)?;
        let raw = self.export_source().map_err(failure)?;
        let rendered = manual_eq::render(raw, &s, &|| false).map_err(failure)?;
        let (bytes, decoded, metrics, passes) =
            wav_output::encode_checked(rendered.audio, format, &s).map_err(failure)?;
        let output_analysis =
            analysis::analyze(&decoded, &profile(&s.profile).map_err(failure)?).map_err(failure)?;
        let sample_rate = raw.sample_rate;
        if let Some(report) = &mut self.report {
            report.configuration["manual_eq"] =
                serde_json::to_value(&s.manual_eq).map_err(failure)?;
            report.configuration["eq_output_db"] = s.eq_output_db.into();
            report.configuration["eq_hold_loudness"] = s.eq_hold_loudness.into();
            report.configuration["eq_hold_gain_db"] = rendered.hold_gain_db.into();
            report.configuration["eq_final_trim_db"] = rendered.trim_db.into();
            report.output = output_analysis;
            report.exports = vec![
                serde_json::json!({"format":format,"sample_rate":sample_rate,"decoded_metrics":metrics,"encode_passes":passes,"dither":s.dither&&format!="wav32f"}),
            ];
            report
                .notes
                .retain(|note| !note.starts_with("Browser export:"));
            report.notes.push("Browser export: shared streaming EQ, resolved loudness hold and decoded-WAV peak verification. Listening volume is excluded.".into());
        }
        Ok(bytes)
    }

    pub fn report(&self) -> Result<String, JsValue> {
        serde_json::to_string(&self.report).map_err(failure)
    }
}
