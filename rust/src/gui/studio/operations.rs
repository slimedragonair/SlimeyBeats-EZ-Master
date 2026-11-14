use super::{
    jobs::Change,
    state::{LoadedSource, Revision, Studio},
};
use crate::{
    audio::Audio,
    config::Settings,
    studio::{spectral_repair, spectrogram, stem_mix, voice_cleanup},
    studio_native::{audio_import, inference::SeparationMode, runtime::StemRuntime},
};
use anyhow::{Context, Result, ensure};
use eframe::egui;
use std::{
    path::PathBuf,
    sync::{Arc, atomic::Ordering},
    time::Duration,
};

fn revision(
    before: Arc<Audio>,
    audio: Audio,
    label: &str,
    notes: Vec<String>,
    bank: Option<Arc<stem_mix::StemSet>>,
    mix: stem_mix::MixSettings,
) -> Result<Revision> {
    let before_lufs = crate::meters::integrated(&before)?;
    let after_lufs = crate::meters::integrated(&audio)?;
    Ok(Revision {
        before,
        audio: Arc::new(audio),
        label: label.into(),
        notes,
        before_lufs,
        after_lufs,
        bank,
        mix,
    })
}
impl Studio {
    pub fn apply_preview_to_stem(&mut self, ctx: &egui::Context) {
        let super::state::Target::Stem(kind) = self.target else {
            return;
        };
        let (Some(bank), Some(current)) = (self.bank.clone(), self.current.clone()) else {
            return;
        };
        if !self.can_apply_preview_to_stem() {
            self.error=Some("Preview an edit from the selected stem first; apply each stem edit before chaining another.".into());
            return;
        }
        let source = self
            .source
            .as_ref()
            .map(|s| s.audio.clone())
            .unwrap_or_else(|| current.before.clone());
        let mix = self.mix.clone();
        self.begin(
            ctx,
            "Applying the prepared edit to its stem",
            move |cancel, _| {
                let bank = Arc::new(bank.replace(kind, current.audio.clone())?);
                let audio = stem_mix::render(&bank, &mix, &|| cancel.load(Ordering::Relaxed))?;
                Ok(Change::Preview(revision(
                    source,
                    audio,
                    "Mix with edited stem",
                    vec![format!(
                        "The prepared edit replaced the {} stem; other stems were retained.",
                        kind.name()
                    )],
                    Some(bank),
                    mix,
                )?))
            },
        );
    }
    pub fn export_stems(&mut self, ctx: &egui::Context) {
        let Some(bank) = self.bank.clone() else {
            return;
        };
        let destination = self.output_dir.clone();
        self.begin(
            ctx,
            "Saving aligned individual float stems",
            move |cancel, _| {
                let result =
                    crate::studio_native::stem_files::export(&bank, &destination, &cancel)?;
                Ok(Change::Exported {
                    paths: vec![
                        result["folder"]
                            .as_str()
                            .unwrap_or("Saved stem folder")
                            .into(),
                    ],
                    warning: None,
                })
            },
        );
    }
    pub fn probe_plugin(&mut self, path: PathBuf, ctx: &egui::Context) {
        let Some(host) = self.plugin_host.clone() else {
            self.error = Some("Install the optional plugin host helper first".into());
            return;
        };
        self.begin(
            ctx,
            "Reading VST3 controls in an isolated helper",
            move |cancel, _| {
                let probe = host.probe(&path, cancel)?;
                Ok(Change::PluginProbe(path, probe))
            },
        );
    }
    pub fn render_plugin(&mut self, ctx: &egui::Context) {
        let audio = match self.editing_target() {
            Ok(a) => a,
            Err(e) => {
                self.error = Some(format!("{e:#}"));
                return;
            }
        };
        let (Some(host), Some(path), Some(cache)) = (
            self.plugin_host.clone(),
            self.plugin_path.clone(),
            self.cache.clone(),
        ) else {
            return;
        };
        let class = self.plugin_class.clone();
        let values = self.plugin_values.clone();
        let bank = self.bank.clone();
        let mix = self.mix.clone();
        self.begin(ctx,"Rendering the trusted VST3 effect in its helper",move|cancel,_|{
            let (processed,metadata)=host.render(&path,&class,&audio,&values,cache.path(),cancel)?;
            Ok(Change::Preview(revision(audio,processed,"VST3 effect",vec![format!("Isolated plugin render · latency compensated: {} frames · tails trimmed to the source duration",metadata["latency_frames_compensated"])],bank,mix)?))
        });
    }
    pub fn analyze(&mut self, ctx: &egui::Context) {
        let audio = match self.audio_target() {
            Ok(a) => a,
            Err(e) => {
                self.error = Some(format!("{e:#}"));
                return;
            }
        };
        self.begin(
            ctx,
            "Measuring the selected Studio source",
            move |cancel, _| {
                let profile = crate::profiles::resolve("neutral", std::path::Path::new(""))?;
                let analysis = crate::analysis::analyze(&audio, &profile)?;
                ensure!(!cancel.load(Ordering::Relaxed), "Cancelled");
                Ok(Change::Analysis(Box::new(analysis)))
            },
        );
    }
    pub fn load(&mut self, path: PathBuf, ctx: &egui::Context) {
        self.begin(ctx, "Loading a local Studio source", move |cancel, _| {
            let (audio, _) =
                audio_import::load(&path, 512 * 1024 * 1024, Duration::from_secs(300), cancel)?;
            let loudness = crate::meters::integrated(&audio)?;
            Ok(Change::Source(LoadedSource {
                audio: Arc::new(audio),
                path,
                loudness,
            }))
        });
    }
    pub fn install_engine(&mut self, repair: bool, ctx: &egui::Context) {
        let runtime = self.runtime.clone();
        self.begin(
            ctx,
            "Preparing the optional local stem engine",
            move |cancel, progress| {
                runtime.install_with_repair(repair, progress, cancel)?;
                Ok(Change::EngineReady)
            },
        );
    }
    pub fn separate(&mut self, ctx: &egui::Context) {
        let Some(source) = &self.source else {
            self.error = Some("Load a music source first".into());
            return;
        };
        let Some(cache) = self.cache.clone() else {
            return;
        };
        if !self.runtime.is_ready() {
            self.error=Some("Install the optional stem engine first. Voice cleanup and spectral repair work without it.".into());
            return;
        }
        let input = source.audio.clone();
        let runtime = StemRuntime::from(&self.runtime);
        let two = self.two_stems;
        self.begin(ctx,"Separating the source locally",move|cancel,progress| {
            let separated=runtime.separate(&input,if two {SeparationMode::VocalInstrumental}else{SeparationMode::FourStems},cache.path(),progress,cancel.clone())?;
            let bank=Arc::new(separated.stems);let mix=bank.defaults();
            let audio=stem_mix::render(&bank,&mix,&||cancel.load(Ordering::Relaxed))?;
            let preview=revision(input,audio,"Neutral stem mix",vec!["Stem separation is an estimate; listen for bleed/artifacts. The neutral mix retains the original through reconstruction residual in instrumental/other.".into()],Some(bank.clone()),mix)?;
            Ok(Change::Stems(bank,preview))
        });
    }
    pub fn render_mix(&mut self, ctx: &egui::Context) {
        let Some(bank) = self.bank.clone() else {
            return;
        };
        let before = self
            .source
            .as_ref()
            .map(|s| s.audio.clone())
            .or_else(|| self.current.as_ref().map(|r| r.before.clone()));
        let Some(before) = before else {
            return;
        };
        let mix = self.mix.clone();
        self.begin(ctx,"Preparing the rebalanced mix",move|cancel,_| {
            let audio=stem_mix::render(&bank,&mix,&||cancel.load(Ordering::Relaxed))?;
            Ok(Change::Preview(revision(before,audio,"Rebalanced stem mix",vec!["Fader changes were rendered from the immutable stems; previous mix renders were not stacked.".into()],Some(bank),mix)?))
        });
    }
    pub fn clean_voice(&mut self, ctx: &egui::Context) {
        let input = match self.editing_target() {
            Ok(a) => a,
            Err(e) => {
                self.error = Some(format!("{e:#}"));
                return;
            }
        };
        let settings = self.voice.clone();
        let bank = self.bank.clone();
        let mix = self.mix.clone();
        self.begin(
            ctx,
            "Cleaning the selected voice source",
            move |cancel, progress| {
                progress(
                    0.1,
                    "Reducing rumble, stationary noise and strong sibilance",
                );
                let cleaned =
                    voice_cleanup::render(&input, &settings, &|| cancel.load(Ordering::Relaxed))?;
                Ok(Change::Preview(revision(
                    input,
                    cleaned.audio,
                    "Voice cleanup",
                    cleaned.notes,
                    bank,
                    mix,
                )?))
            },
        );
    }
    pub fn repair_audio(&mut self, ctx: &egui::Context) {
        let input = match self.editing_target() {
            Ok(a) => a,
            Err(e) => {
                self.error = Some(format!("{e:#}"));
                return;
            }
        };
        let selection = self.repair.clone();
        let bank = self.bank.clone();
        let mix = self.mix.clone();
        self.begin(ctx,"Repairing the selected time/frequency region",move|cancel,_| {
            let audio=spectral_repair::render(&input,&selection,&||cancel.load(Ordering::Relaxed))?;
            Ok(Change::Preview(revision(input,audio,"Spectral repair",vec!["Samples outside the selected time were kept. Compare before accepting; spectral healing estimates surrounding magnitudes.".into()],bank,mix)?))
        });
    }
    pub fn build_spectrogram(&mut self, ctx: &egui::Context) {
        let input = match self.audio_target() {
            Ok(a) => a,
            Err(e) => {
                self.error = Some(format!("{e:#}"));
                return;
            }
        };
        self.begin(
            ctx,
            "Reading the selected source's time/frequency content",
            move |cancel, _| {
                Ok(Change::Spectrogram(Arc::new(spectrogram::build(
                    &input,
                    384,
                    128,
                    &|| cancel.load(Ordering::Relaxed),
                )?)))
            },
        );
    }
    pub fn export_preview(&mut self, ctx: &egui::Context) {
        if self.mix_dirty {
            self.error =
                Some("Update the mix preview before exporting the new fader settings".into());
            return;
        }
        let Some(preview) = self.current.clone() else {
            return;
        };
        let input = self
            .source
            .as_ref()
            .map(|s| s.path.clone())
            .unwrap_or_else(|| PathBuf::from("Studio mix.wav"));
        let settings = Settings {
            out_dir: self.output_dir.clone(),
            formats: vec![self.export_format.clone()],
            suffix: "_studio".into(),
            ..Default::default()
        };
        self.begin(ctx,"Saving the selected Studio preview",move|cancel,_| {
            let results=crate::export::export(&preview.audio,&input,&settings,&||cancel.load(Ordering::Relaxed))?;
            let report=serde_json::json!({"kind":"studio_audio","label":preview.label,"notes":preview.notes,"exports":results});
            let warning=results.first().and_then(|r|r["path"].as_str()).and_then(|path|crate::export::write_json(&PathBuf::from(path).with_extension("studio-report.json"),&report).err()).map(|error|format!("Audio saved, but the report could not be written: {error:#}"));
            Ok(Change::Exported {paths:results.iter().filter_map(|r|r["path"].as_str().map(str::to_string)).collect(),warning})
        });
    }
    pub fn working_copy(&self) -> Result<PathBuf> {
        ensure!(!self.mix_dirty, "Update the mix preview first");
        let audio = &self
            .current
            .as_ref()
            .context("Prepare a Studio preview first")?
            .audio;
        let cache = self
            .cache
            .as_ref()
            .context("Private Studio cache is unavailable")?;
        let mut file = tempfile::Builder::new()
            .prefix("Studio-mix-")
            .suffix(".wav")
            .tempfile_in(cache.path())?;
        crate::wav_encoding::write_wav(audio, file.as_file_mut(), "wav32f", false)?;
        // The parent Studio cache owns this working copy until this app closes.
        let (_file, path) = file.keep()?;
        Ok(path)
    }
    pub(super) fn begin(
        &mut self,
        ctx: &egui::Context,
        message: &str,
        work: impl FnOnce(Arc<std::sync::atomic::AtomicBool>, &dyn Fn(f32, &str)) -> Result<Change>
        + Send
        + 'static,
    ) {
        self.error = None;
        self.status = message.into();
        self.progress = 0.;
        if let Err(error) = self.jobs.start(ctx, work) {
            self.error = Some(format!("{error:#}"));
        }
    }
}
