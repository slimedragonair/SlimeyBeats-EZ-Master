use super::{
    player::Player,
    plots::Waveform,
    theme,
    worker::{self, Event, TrackResult},
};
use crate::{
    analysis::playback_spectrum::PlaybackSpectrum,
    audio,
    config::{EqBand, Settings},
};
use eframe::egui;
use std::{
    path::PathBuf,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
        mpsc::{self, Receiver, Sender},
    },
};

pub struct Track {
    pub path: PathBuf,
    pub status: String,
    pub result: Option<Arc<TrackResult>>,
}
#[derive(Clone, Copy, PartialEq)]
pub(super) enum JobKind {
    Analyze,
    Master,
    Export,
    Suggest,
}
pub struct MasterCraftApp {
    pub studio: super::studio::Studio,
    pub brand: super::brand_art::BrandArt,
    pub settings: Settings,
    pub root: PathBuf,
    pub tracks: Vec<Track>,
    pub selected: Option<usize>,
    pub busy: bool,
    pub status: String,
    pub progress: f32,
    pub error: Option<String>,
    pub eq_dirty: bool,
    pub eq_live: super::eq_live_control::EqLiveState,
    pub tab: usize,
    pub selected_band: usize,
    pub(super) tx: Sender<Event>,
    pub(super) rx: Receiver<Event>,
    pub(super) active_job: Option<JobKind>,
    pub(super) cancel: Arc<AtomicBool>,
    pub player: Player,
    pub waveform: Waveform,
    pub eq_waveform: Waveform,
    pub playback_spectrum: PlaybackSpectrum,
    pub suggested_eq: Option<Vec<EqBand>>,
    pub suggestion_note: Option<String>,
    pub loaded: Option<usize>,
    pub loaded_eq: bool,
    pub volume: f32,
    pub match_levels: bool,
    pub(super) capture: Option<super::ui_capture::UiCapture>,
    pub(super) capture_settings_scroll: Option<f32>,
    pub(super) capture_workspace_scroll: Option<f32>,
}
impl MasterCraftApp {
    pub fn new(cc: &eframe::CreationContext<'_>, files: Vec<PathBuf>, auto_master: bool) -> Self {
        Self::with_initial_settings(cc, files, auto_master, None)
    }
    pub fn with_initial_settings(
        cc: &eframe::CreationContext<'_>,
        files: Vec<PathBuf>,
        auto_master: bool,
        initial: Option<Settings>,
    ) -> Self {
        theme::apply(&cc.egui_ctx);
        let root = crate::paths::app_root();
        let settings_path = crate::paths::settings_file();
        let (mut settings, error) = if let Some(settings) = initial {
            (settings, None)
        } else if settings_path.is_file() {
            match Settings::load(&settings_path) {
                Ok(s) => (s, None),
                Err(e) => (
                    Settings::default(),
                    Some(format!(
                        "Settings could not be loaded; file preserved: {e:#}"
                    )),
                ),
            }
        } else {
            (Settings::default(), None)
        };
        if settings.out_dir == *"output" {
            settings.out_dir = root.join("output");
        }
        let (tx, rx) = mpsc::channel();
        let studio = super::studio::Studio::new(settings.out_dir.clone());
        let mut app = Self {
            studio,
            brand: super::brand_art::BrandArt::new(&cc.egui_ctx),
            settings,
            root,
            tracks: vec![],
            selected: None,
            busy: false,
            status: "Ready for a new session".into(),
            progress: 0.,
            error,
            eq_dirty: false,
            eq_live: Default::default(),
            tab: 0,
            selected_band: 3,
            tx,
            rx,
            active_job: None,
            cancel: Arc::new(AtomicBool::new(false)),
            player: Player::default(),
            waveform: Waveform::default(),
            eq_waveform: Waveform::default(),
            playback_spectrum: PlaybackSpectrum::default(),
            suggested_eq: None,
            suggestion_note: None,
            loaded: None,
            loaded_eq: false,
            volume: 0.5,
            match_levels: true,
            capture: None,
            capture_settings_scroll: None,
            capture_workspace_scroll: None,
        };
        app.add_paths(files);
        if auto_master && !app.tracks.is_empty() {
            app.start(false, &cc.egui_ctx);
        }
        app
    }
    pub fn capture_ui_once(&mut self, destination: PathBuf, failed: Arc<AtomicBool>) {
        self.capture = Some(super::ui_capture::UiCapture::new(destination, failed));
    }
    pub fn capture_settings_at(&mut self, offset: f32) {
        self.capture_settings_scroll = Some(offset);
    }
    pub fn capture_workspace_at(&mut self, offset: f32) {
        self.capture_workspace_scroll = Some(offset);
    }
    pub fn add_paths(&mut self, files: Vec<PathBuf>) {
        for file in files {
            if file.is_dir() {
                if let Ok(entries) = std::fs::read_dir(&file) {
                    self.add_paths(
                        entries
                            .filter_map(|e| e.ok().map(|e| e.path()))
                            .filter(|p| p.is_file())
                            .collect(),
                    );
                }
                continue;
            }
            if audio::supported(&file) && !self.tracks.iter().any(|t| t.path == file) {
                self.tracks.push(Track {
                    path: file,
                    status: "Ready".into(),
                    result: None,
                });
            }
        }
        if self.selected.is_none() && !self.tracks.is_empty() {
            self.selected = Some(0);
        }
    }
    pub fn pick_files(&mut self) {
        if let Some(files) = rfd::FileDialog::new()
            .add_filter("Audio", audio::EXTENSIONS)
            .pick_files()
        {
            self.add_paths(files);
        }
    }
    pub fn start(&mut self, analysis: bool, ctx: &egui::Context) {
        if self.busy || self.tracks.is_empty() {
            return;
        }
        if let Err(e) = self.settings.validate() {
            self.error = Some(format!("{e:#}"));
            return;
        }
        self.busy = true;
        self.active_job = Some(if analysis {
            JobKind::Analyze
        } else {
            JobKind::Master
        });
        self.error = None;
        self.cancel.store(false, Ordering::Relaxed);
        self.progress = 0.;
        self.player.stop();
        self.status = if analysis {
            "Analyzing queue"
        } else {
            "Mastering queue"
        }
        .into();
        let files = self
            .tracks
            .iter()
            .enumerate()
            .map(|(i, t)| (i, t.path.clone()))
            .collect();
        worker::batch(
            files,
            self.settings.clone(),
            analysis,
            self.tx.clone(),
            self.cancel.clone(),
            ctx.clone(),
            self.root.clone(),
        );
    }
    pub fn export_revision(&mut self, ctx: &egui::Context) {
        if let Some((i, result)) = self.current() {
            self.busy = true;
            self.active_job = Some(JobKind::Export);
            self.error = None;
            self.cancel.store(false, Ordering::Relaxed);
            self.status = "Exporting selected revision".into();
            super::eq_jobs::export_selected(
                i,
                self.tracks[i].path.clone(),
                result,
                self.settings.clone(),
                self.tx.clone(),
                self.cancel.clone(),
                ctx.clone(),
            );
        }
    }
    pub fn current(&self) -> Option<(usize, Arc<TrackResult>)> {
        self.selected.and_then(|i| {
            self.tracks
                .get(i)
                .and_then(|t| t.result.clone())
                .map(|r| (i, r))
        })
    }
    pub fn suggest_eq(&mut self, ctx: &egui::Context) {
        if let Some((index, result)) = self.current() {
            self.busy = true;
            self.active_job = Some(JobKind::Suggest);
            self.error = None;
            self.cancel.store(false, Ordering::Relaxed);
            self.status = "Measuring a deterministic EQ suggestion".into();
            worker::suggest_eq(
                index,
                result,
                self.settings.clone(),
                self.tx.clone(),
                self.cancel.clone(),
                ctx.clone(),
            );
        }
    }
    pub fn save_settings(&mut self) {
        let result = (|| -> anyhow::Result<()> {
            self.settings.validate()?;
            let path = crate::paths::settings_file();
            std::fs::create_dir_all(path.parent().unwrap())?;
            let mut temp = tempfile::NamedTempFile::new_in(path.parent().unwrap())?;
            serde_json::to_writer_pretty(&mut temp, &self.settings)?;
            temp.as_file().sync_all()?;
            temp.persist(path).map_err(|e| e.error)?;
            Ok(())
        })();
        match result {
            Ok(()) => self.status = "Rust settings saved".into(),
            Err(e) => self.error = Some(format!("Save settings: {e:#}")),
        }
    }
}
