//! Background admission of independent files, archives and existing immutable sources.
use super::{
    arrangement_loading::{self, ImportContext},
    arrangement_runtime::Change,
    arrangement_state::LaneInit,
    jobs::Change as StudioChange,
    state::Studio,
};
use crate::{
    studio::stem_import_plan::{PartSelection, PreferredFormat},
    studio_native::stem_bundle::PreparedImport,
};
use anyhow::{Context, Result, ensure};
use eframe::egui;
use std::{
    path::PathBuf,
    sync::{Arc, atomic::Ordering},
};

impl Studio {
    /// Review dropped/command-line files using the independent-track importer.
    pub fn prepare_track_files(&mut self, paths: Vec<PathBuf>, ctx: &egui::Context) {
        if paths.is_empty() || self.busy() {
            return;
        }
        let archives = paths
            .iter()
            .filter(|path| {
                path.extension()
                    .is_some_and(|ext| ext.eq_ignore_ascii_case("zip"))
            })
            .count();
        if archives > 0 && paths.len() != 1 {
            self.error =
                Some("Add one audio ZIP at a time, or choose several loose audio files.".into());
            return;
        }
        if paths.iter().any(|path| path.is_dir()) {
            self.error =
                Some("Choose audio files inside the folder, or add a single stem ZIP.".into());
            return;
        }
        self.player.controls.playing.store(false, Ordering::Relaxed);
        if archives == 1 {
            self.prepare_arrangement_archive(paths[0].clone(), ctx);
        } else {
            self.prepare_arrangement_files(paths, ctx);
        }
    }
    fn arrangement_import_context(&mut self) -> Result<ImportContext> {
        self.arrangement.editor.commit_names()?;
        Ok(ImportContext {
            editor: self.arrangement.editor.fork_for_import()?,
            expected_epoch: self.arrangement.editor.epoch,
            reference: self.arrangement.reference.clone(),
            origin: self.arrangement.origin.clone(),
            options: self.arrangement.options.clone(),
        })
    }
    pub(super) fn prepare_arrangement_files(&mut self, paths: Vec<PathBuf>, ctx: &egui::Context) {
        self.begin(
            ctx,
            "Reviewing tracks for the timeline",
            move |cancel, _| {
                ensure!(!cancel.load(Ordering::Relaxed), "Cancelled");
                Ok(StudioChange::Arrangement(Box::new(Change::ImportPlan(
                    PreparedImport::files_for_arrangement(paths)?,
                ))))
            },
        );
    }
    pub(super) fn prepare_arrangement_archive(&mut self, path: PathBuf, ctx: &egui::Context) {
        self.begin(ctx, "Reviewing audio in the ZIP", move |cancel, _| {
            ensure!(!cancel.load(Ordering::Relaxed), "Cancelled");
            Ok(StudioChange::Arrangement(Box::new(Change::ImportPlan(
                PreparedImport::archive_for_arrangement(path)?,
            ))))
        });
    }
    pub(super) fn import_arrangement_bundle(
        &mut self,
        prepared: PreparedImport,
        selected: Vec<PartSelection>,
        preferred: PreferredFormat,
        ctx: &egui::Context,
    ) {
        let context = match self.arrangement_import_context() {
            Ok(context) => context,
            Err(error) => {
                self.error = Some(error.to_string());
                return;
            }
        };
        let Some(cache) = self.cache.clone() else {
            self.error = Some("Private Studio cache is unavailable".into());
            return;
        };
        self.begin(
            ctx,
            "Loading selected timeline tracks",
            move |cancel, progress| {
                let ready = arrangement_loading::load_bundle(
                    context,
                    prepared,
                    selected.into_iter().map(|part| part.part_index).collect(),
                    preferred,
                    cache.path(),
                    cancel,
                    progress,
                )?;
                Ok(StudioChange::Arrangement(Box::new(Change::Session(
                    Box::new(ready),
                ))))
            },
        );
    }
    pub(super) fn copy_bank_to_arrangement(&mut self, ctx: &egui::Context) {
        let Some(bank) = self.bank.clone() else {
            return;
        };
        let mix = self.mix.clone();
        let mut context = match self.arrangement_import_context() {
            Ok(context) => context,
            Err(error) => {
                self.error = Some(error.to_string());
                return;
            }
        };
        if context.editor.document().is_none() {
            context.options.protect_peaks = mix.protect_peaks;
            context.options.ceiling_dbtp = mix.ceiling_dbtp;
        }
        self.begin(
            ctx,
            "Copying immutable stems and their live balance",
            move |cancel, progress| {
                mix.validate(&bank)?;
                let sources = bank
                    .tracks
                    .iter()
                    .map(|stem| {
                        let control = mix
                            .tracks
                            .iter()
                            .find(|control| control.kind == stem.kind)
                            .context("Missing imported stem control")?;
                        Ok((
                            bank.display_name(stem.kind).into_owned(),
                            stem.audio.clone(),
                            LaneInit {
                                gain_db: control.gain_db,
                                pan: control.balance,
                                mute: control.mute,
                                solo: control.solo,
                                clip_gain_db: mix.output_gain_db,
                            },
                        ))
                    })
                    .collect::<Result<Vec<_>>>()?;
                let ready = arrangement_loading::prepare_named(
                    context,
                    sources,
                    &|| cancel.load(Ordering::Relaxed),
                    progress,
                )?;
                Ok(StudioChange::Arrangement(Box::new(Change::Session(
                    Box::new(ready),
                ))))
            },
        );
    }
    pub(super) fn copy_source_to_arrangement(&mut self, ctx: &egui::Context) {
        let Some(source) = &self.source else {
            return;
        };
        let audio = source.audio.clone();
        let name = source
            .path
            .file_stem()
            .unwrap_or_default()
            .to_string_lossy()
            .into_owned();
        let context = match self.arrangement_import_context() {
            Ok(context) => context,
            Err(error) => {
                self.error = Some(error.to_string());
                return;
            }
        };
        let Some(cache) = self.cache.clone() else {
            return;
        };
        self.begin(
            ctx,
            "Adding the loaded mix to the timeline",
            move |cancel, progress| {
                let rate = context
                    .editor
                    .document()
                    .map_or(audio.sample_rate, |document| document.sample_rate);
                let audio = if rate != audio.sample_rate {
                    crate::gui::tool_setup::prepare(true, cancel.clone(), progress)?;
                    Arc::new(crate::studio_native::arrangement_import::conform(
                        (*audio).clone(),
                        rate,
                        crate::studio_native::stem_files::DEFAULT_INPUT_DECODED_BYTES,
                        cache.path(),
                        cancel.clone(),
                    )?)
                } else {
                    audio
                };
                let ready = arrangement_loading::prepare_named(
                    context,
                    vec![(name, audio, LaneInit::default())],
                    &|| cancel.load(Ordering::Relaxed),
                    progress,
                )?;
                Ok(StudioChange::Arrangement(Box::new(Change::Session(
                    Box::new(ready),
                ))))
            },
        );
    }
}
