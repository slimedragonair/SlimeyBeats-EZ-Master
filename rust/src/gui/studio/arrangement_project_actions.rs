//! Canonical project persistence and final delivery exports, separate from live audition.
use super::{
    arrangement_loading,
    arrangement_runtime::{self, Change},
    jobs::Change as StudioChange,
    state::Studio,
};
use crate::{config::Settings, studio_native::arrangement_project};
use eframe::egui;
use std::{path::PathBuf, sync::atomic::Ordering};

impl Studio {
    pub(super) fn save_arrangement(&mut self, destination: PathBuf, ctx: &egui::Context) {
        if let Err(error) = self.arrangement.editor.commit_names() {
            self.error = Some(error.to_string());
            return;
        }
        let snapshot = match self.arrangement.snapshot() {
            Ok(snapshot) => snapshot,
            Err(error) => {
                self.error = Some(error.to_string());
                return;
            }
        };
        let epoch = self.arrangement.editor.epoch;
        self.begin(
            ctx,
            "Saving sources, clips, EQ and undo history",
            move |cancel, _| {
                let path = arrangement_project::save_with_limits(
                    &snapshot,
                    &destination,
                    arrangement_project::ProjectLimits {
                        max_output_bytes: arrangement_runtime::NATIVE_SOURCE_BUDGET,
                        ..Default::default()
                    },
                    &|| cancel.load(Ordering::Relaxed),
                )?;
                Ok(StudioChange::Arrangement(Box::new(Change::Saved {
                    epoch,
                    path,
                })))
            },
        );
    }
    pub(super) fn open_arrangement(&mut self, path: PathBuf, ctx: &egui::Context) {
        let epoch = self.arrangement.editor.epoch;
        self.begin(
            ctx,
            "Verifying clip project sources",
            move |cancel, progress| {
                let cancelled = || cancel.load(Ordering::Relaxed);
                match arrangement_project::load_any_with_limits(
                    &path,
                    arrangement_project::ProjectLimits {
                        max_output_bytes: arrangement_runtime::NATIVE_SOURCE_BUDGET,
                        ..Default::default()
                    },
                    &cancelled,
                )? {
                    arrangement_project::LoadedStudioProject::Legacy(snapshot) => {
                        Ok(StudioChange::Session(snapshot))
                    }
                    arrangement_project::LoadedStudioProject::Arrangement(snapshot) => {
                        let ready = arrangement_loading::prepare_project(
                            *snapshot, epoch, &cancelled, progress,
                        )?;
                        Ok(StudioChange::Arrangement(Box::new(Change::Session(
                            Box::new(ready),
                        ))))
                    }
                }
            },
        );
    }
    pub(super) fn export_arrangement(&mut self, ctx: &egui::Context) {
        if let Err(error) = self.arrangement.editor.commit_names() {
            self.error = Some(error.to_string());
            return;
        }
        let Some(document) = self.arrangement.editor.document().cloned() else {
            return;
        };
        if self.arrangement.editor.draft.is_some() {
            self.error = Some("Finish the current edit before exporting".into());
            return;
        }
        let assets = self.arrangement.editor.assets.clone();
        let options = self.arrangement.options.clone();
        let settings = Settings {
            out_dir: self.output_dir.clone(),
            formats: vec![self.export_format.clone()],
            suffix: "_arrangement".into(),
            ceiling_dbtp: options.ceiling_dbtp,
            ..Default::default()
        };
        self.begin(ctx, "Rendering and exporting the current timeline snapshot", move |cancel, progress| {
            let cancelled = || cancel.load(Ordering::Relaxed);
            crate::gui::tool_setup::for_export(&settings, document.sample_rate, cancel.clone(), progress)?;
            let audio = crate::studio::arrangement::render(&document, &assets, &options, &cancelled)?;
            let exports = crate::export::export(&audio, &PathBuf::from("Timeline mix.wav"), &settings, &cancelled)?;
            let report = serde_json::json!({"kind":"arrangement_audio","arrangement":document,"exports":exports});
            let warning = exports.first().and_then(|entry| entry["path"].as_str()).and_then(|path|
                crate::export::write_json(&PathBuf::from(path).with_extension("arrangement-report.json"), &report).err())
                .map(|error| format!("Audio saved, but the report could not be written: {error:#}"));
            Ok(StudioChange::Exported { paths: exports.iter().filter_map(|entry| entry["path"].as_str().map(str::to_owned)).collect(), warning })
        });
    }
}
