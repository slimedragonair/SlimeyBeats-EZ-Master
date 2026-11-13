use super::jobs::{Change, Event, Jobs};
use crate::{
    audio::Audio,
    gui::{player::Player, plots::Waveform},
    studio::{
        spectral_repair::{RepairMode, RepairSelection},
        spectrogram::Spectrogram,
        stem_mix::{MixSettings, StemKind, StemSet},
        voice_cleanup::VoiceSettings,
    },
    studio_native::runtime::RuntimePaths,
};
use eframe::egui;
use std::{
    path::PathBuf,
    sync::{Arc, atomic::Ordering},
};

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Page {
    Stems,
    Voice,
    Repair,
    Plugins,
}
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Target {
    Source,
    Preview,
    Stem(StemKind),
}
#[derive(Clone, Copy)]
pub enum AutoAction {
    Separate,
    Voice,
    Spectrogram,
    Repair,
}
pub struct LoadedSource {
    pub audio: Arc<Audio>,
    pub path: PathBuf,
    pub loudness: f64,
}
#[derive(Clone)]
pub struct Revision {
    pub before: Arc<Audio>,
    pub audio: Arc<Audio>,
    pub label: String,
    pub notes: Vec<String>,
    pub before_lufs: f64,
    pub after_lufs: f64,
    pub bank: Option<Arc<StemSet>>,
    pub mix: MixSettings,
}

pub struct Studio {
    pub import_files: std::collections::BTreeMap<StemKind, PathBuf>,
    pub plugin_host: Option<crate::studio_native::plugins::VstHost>,
    pub plugin_path: Option<PathBuf>,
    pub plugin_probe: Option<crate::studio_native::plugins::Probe>,
    pub plugin_class: String,
    pub plugin_values: Vec<crate::studio_native::plugins::PluginValue>,
    pub auto_action: Option<AutoAction>,
    pub analysis: Option<crate::analysis::Analysis>,
    pub page: Page,
    pub target: Target,
    pub source: Option<LoadedSource>,
    pub bank: Option<Arc<StemSet>>,
    pub mix: MixSettings,
    pub voice: VoiceSettings,
    pub repair: RepairSelection,
    pub current: Option<Revision>,
    pub undo: Vec<Revision>,
    pub redo: Vec<Revision>,
    pub player: Player,
    pub waveform: Waveform,
    pub spectrogram: Option<Arc<Spectrogram>>,
    pub texture: Option<egui::TextureHandle>,
    pub runtime: RuntimePaths,
    pub cache: Option<Arc<tempfile::TempDir>>,
    pub jobs: Jobs,
    pub progress: f32,
    pub status: String,
    pub error: Option<String>,
    pub two_stems: bool,
    pub mix_dirty: bool,
    pub volume: f32,
    pub match_levels: bool,
    pub output_dir: PathBuf,
    pub export_format: String,
    pub stem_waveforms: Vec<(StemKind, Waveform)>,
    pub selection_anchor: Option<egui::Pos2>,
}
impl Studio {
    pub fn new(output_dir: PathBuf) -> Self {
        let cache = tempfile::Builder::new()
            .prefix("slimey-studio-")
            .tempdir()
            .map(Arc::new);
        let error = cache
            .as_ref()
            .err()
            .map(|e| format!("Cannot create a private Studio cache: {e}"));
        Self {
            import_files: Default::default(),
            plugin_host: crate::studio_native::plugins::VstHost::find(),
            plugin_path: None,
            plugin_probe: None,
            plugin_class: String::new(),
            plugin_values: vec![],
            auto_action: None,
            analysis: None,
            page: Page::Stems,
            target: Target::Source,
            source: None,
            bank: None,
            mix: MixSettings::default(),
            voice: VoiceSettings::default(),
            repair: RepairSelection {
                start_seconds: 0.,
                end_seconds: 1.,
                low_hz: 2000.,
                high_hz: 8000.,
                reduction_db: -12.,
                mode: RepairMode::Attenuate,
            },
            current: None,
            undo: vec![],
            redo: vec![],
            player: Player::default(),
            waveform: Waveform::default(),
            spectrogram: None,
            texture: None,
            runtime: RuntimePaths::default_location(),
            cache: cache.ok(),
            jobs: Jobs::default(),
            progress: 0.,
            status: "Load a track or import aligned stems".into(),
            error,
            two_stems: true,
            mix_dirty: false,
            volume: 0.5,
            match_levels: false,
            output_dir,
            export_format: "wav24".into(),
            stem_waveforms: vec![],
            selection_anchor: None,
        }
    }
    pub fn busy(&self) -> bool {
        self.jobs.active.is_some()
    }
    pub fn audio_target(&self) -> anyhow::Result<Arc<Audio>> {
        use anyhow::Context;
        match self.target {
            Target::Source => Ok(self
                .source
                .as_ref()
                .context("Load a source first")?
                .audio
                .clone()),
            Target::Preview => Ok(self
                .current
                .as_ref()
                .context("Prepare a preview first")?
                .audio
                .clone()),
            Target::Stem(kind) => Ok(self
                .bank
                .as_ref()
                .context("Separate/import stems first")?
                .track(kind)?
                .audio
                .clone()),
        }
    }
    pub fn poll(&mut self, ctx: &egui::Context) {
        while let Ok(event) = self.jobs.receiver.try_recv() {
            match event {
                Event::Progress(id, fraction, message) if self.jobs.active == Some(id) => {
                    self.progress = fraction;
                    self.status = message;
                }
                Event::Done(id, result) if self.jobs.active == Some(id) => {
                    self.jobs.active = None;
                    match result {
                        Err(error) => {
                            self.error = Some(error);
                            self.status = "Studio job stopped; previous audio preserved".into();
                        }
                        Ok(Change::Source(source)) => {
                            self.bank = None;
                            self.stem_waveforms.clear();
                            self.undo.clear();
                            self.redo.clear();
                            self.mix_dirty = false;
                            self.target = Target::Source;
                            self.player.stop();
                            let revision = Revision {
                                before: source.audio.clone(),
                                audio: source.audio.clone(),
                                label: "Original source".into(),
                                notes: vec![],
                                before_lufs: source.loudness,
                                after_lufs: source.loudness,
                                bank: None,
                                mix: MixSettings::default(),
                            };
                            self.repair.end_seconds = source.audio.duration().min(1.);
                            self.repair.high_hz = (source.audio.sample_rate as f64 / 2.).min(8000.);
                            self.source = Some(source);
                            self.current = Some(revision);
                            self.load_player();
                            self.status = "Source ready · choose a Studio tool".into();
                            if let Some(action) = self.auto_action.take() {
                                match action {
                                    AutoAction::Separate => self.separate(ctx),
                                    AutoAction::Voice => self.clean_voice(ctx),
                                    AutoAction::Spectrogram => self.build_spectrogram(ctx),
                                    AutoAction::Repair => self.repair_audio(ctx),
                                }
                            }
                        }
                        Ok(Change::Preview(revision)) => {
                            self.accept(revision, true);
                        }
                        Ok(Change::Session(snapshot)) => {
                            if let Err(error) = self.adopt_project(*snapshot) {
                                self.error = Some(format!("Cannot adopt project: {error:#}"));
                            }
                        }
                        Ok(Change::Stems(bank, revision)) => {
                            self.stem_waveforms = bank
                                .tracks
                                .iter()
                                .map(|s| (s.kind, Waveform::new(&s.audio, None)))
                                .collect();
                            self.bank = Some(bank.clone());
                            self.mix = bank.defaults();
                            self.target = Target::Stem(StemKind::Vocals);
                            self.undo.clear();
                            self.redo.clear();
                            self.accept(revision, false);
                        }
                        Ok(Change::EngineReady) => {
                            self.status = "Local stem engine is ready".into();
                        }
                        Ok(Change::PluginProbe(path, probe)) => {
                            self.plugin_path = Some(path);
                            self.plugin_class = probe
                                .plugins
                                .iter()
                                .find(|p| p.error.is_none())
                                .map(|p| p.class.id.clone())
                                .unwrap_or_default();
                            self.plugin_probe = Some(probe);
                            self.reset_plugin_values();
                            self.status =
                                "Plugin controls ready · render a preview to apply them".into();
                        }
                        Ok(Change::Analysis(analysis)) => {
                            self.analysis = Some(*analysis);
                            self.status = "Selected Studio source analyzed".into();
                        }
                        Ok(Change::Spectrogram(image)) => {
                            self.spectrogram = Some(image);
                            self.texture = None;
                            self.status = "Spectrogram ready · select a region to refine".into();
                        }
                        Ok(Change::Exported { paths, warning }) => {
                            self.error = warning;
                            self.status = format!(
                                "Saved {} file(s) in {}",
                                paths.len(),
                                self.output_dir.display()
                            );
                        }
                    }
                    if !self.busy() {
                        self.progress = 1.;
                    }
                    ctx.request_repaint();
                }
                _ => {}
            }
        }
    }
    pub fn accept(&mut self, revision: Revision, remember: bool) {
        if remember && let Some(previous) = self.current.take() {
            self.undo.push(previous);
            while self.undo.len() > 4 {
                self.undo.remove(0);
            }
        }
        self.redo.clear();
        self.bank = revision.bank.clone();
        self.mix = revision.mix.clone();
        self.current = Some(revision);
        self.mix_dirty = false;
        self.load_player();
        self.status = "Preview ready · compare, refine, then export".into();
    }
    pub fn history(&mut self, redo: bool) {
        let next = if redo {
            self.redo.pop()
        } else {
            self.undo.pop()
        };
        if let Some(next) = next {
            if let Some(previous) = self.current.replace(next.clone()) {
                if redo {
                    self.undo.push(previous);
                } else {
                    self.redo.push(previous);
                }
            }
            self.bank = next.bank.clone();
            self.mix = next.mix.clone();
            self.mix_dirty = false;
            self.load_player();
        }
    }
    fn load_player(&mut self) {
        self.analysis = None;
        if let Some(revision) = &self.current {
            let seconds = self.player.seconds();
            let playing = self.player.controls.playing.load(Ordering::Relaxed);
            self.waveform = Waveform::new(&revision.before, Some(&revision.audio));
            self.spectrogram = None;
            self.texture = None;
            if let Err(error) = self.player.load(
                revision.before.clone(),
                Some(revision.audio.clone()),
                revision.before_lufs,
                revision.after_lufs,
                None,
            ) {
                self.error = Some(format!(
                    "Playback unavailable: {error:#}. Processing/export remain available."
                ));
            } else {
                self.player.seek(seconds.min(self.player.duration));
                if playing {
                    self.player.toggle();
                }
            }
        }
    }
    pub fn reset_plugin_values(&mut self) {
        self.plugin_values = self
            .plugin_probe
            .as_ref()
            .and_then(|p| p.plugins.iter().find(|p| p.class.id == self.plugin_class))
            .map_or_else(Vec::new, |p| {
                p.parameters
                    .iter()
                    .filter(|p| !p.read_only)
                    .map(|p| crate::studio_native::plugins::PluginValue {
                        id: p.id,
                        normalized: p.default_normalized,
                    })
                    .collect()
            });
    }
}
