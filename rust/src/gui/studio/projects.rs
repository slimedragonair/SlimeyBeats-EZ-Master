use super::{
    jobs::Change,
    state::{LoadedSource, Page, Revision, Studio},
};
use crate::studio_native::session::{self, StudioDocument, StudioSnapshot};
use anyhow::{Context, Result, ensure};
use eframe::egui;
use std::{
    path::PathBuf,
    sync::{Arc, atomic::Ordering},
};

impl Studio {
    pub fn snapshot(&self) -> Result<StudioSnapshot> {
        ensure!(
            !self.mix_dirty,
            "Update the mix preview before saving the project"
        );
        let source = self
            .source
            .as_ref()
            .context("Load a source or import stems first")?;
        let preview = self
            .current
            .as_ref()
            .context("Prepare a Studio preview first")?;
        Ok(StudioSnapshot {
            source_name: source
                .path
                .file_name()
                .unwrap_or_default()
                .to_string_lossy()
                .into_owned(),
            source: source.audio.clone(),
            stems: self.bank.as_ref().map(|b| (**b).clone()),
            mix: self.mix.clone(),
            before: preview.before.clone(),
            preview: preview.audio.clone(),
            label: preview.label.clone(),
            notes: preview.notes.clone(),
            view: match self.page {
                Page::Stems => "stems",
                Page::Voice => "voice",
                Page::Repair => "repair",
                Page::Plugins => "plugins",
            }
            .into(),
        })
    }
    pub fn save_project(&mut self, destination: PathBuf, ctx: &egui::Context) {
        let snapshot = match self.snapshot() {
            Ok(s) => s,
            Err(error) => {
                self.error = Some(format!("{error:#}"));
                return;
            }
        };
        self.begin(
            ctx,
            "Saving a self-contained Studio project",
            move |cancel, _| {
                let path =
                    session::save(&snapshot, &destination, &|| cancel.load(Ordering::Relaxed))?;
                Ok(Change::Exported {
                    paths: vec![path.to_string_lossy().into_owned()],
                    warning: None,
                })
            },
        );
    }
    pub fn load_project(&mut self, path: PathBuf, ctx: &egui::Context) {
        self.begin(
            ctx,
            "Verifying and loading Studio project assets",
            move |cancel, _| {
                let snapshot = StudioDocument::load(&path)?;
                ensure!(!cancel.load(Ordering::Relaxed), "Cancelled");
                Ok(Change::Session(Box::new(snapshot)))
            },
        );
    }
    pub(super) fn adopt_project(&mut self, snapshot: StudioSnapshot) -> Result<()> {
        let bank = snapshot.stems.map(Arc::new);
        let before_lufs = crate::meters::integrated(&snapshot.before)?;
        let after_lufs = crate::meters::integrated(&snapshot.preview)?;
        let source_lufs = crate::meters::integrated(&snapshot.source)?;
        self.source = Some(LoadedSource {
            audio: snapshot.source,
            path: PathBuf::from(snapshot.source_name),
            loudness: source_lufs,
        });
        self.bank = bank.clone();
        self.bulk_import = None;
        self.undo.clear();
        self.redo.clear();
        self.target = super::state::Target::Preview;
        self.player.stop();
        self.player.seek(0.);
        self.page = match snapshot.view.as_str() {
            "voice" => Page::Voice,
            "repair" => Page::Repair,
            "plugins" => Page::Plugins,
            _ => Page::Stems,
        };
        self.accept(
            Revision {
                before: snapshot.before,
                audio: snapshot.preview,
                label: snapshot.label,
                notes: snapshot.notes,
                before_lufs,
                after_lufs,
                bank,
                mix: snapshot.mix,
            },
            false,
        );
        Ok(())
    }
}
