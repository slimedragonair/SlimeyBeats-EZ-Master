//! Background import, analysis, save and export actions for the native clip workspace.
use super::{
    arrangement_loading::{self, ImportContext},
    arrangement_runtime::{self, Change},
    arrangement_state::LaneInit,
    jobs::Change as StudioChange,
    state::Studio,
};
use crate::{
    config::Settings,
    studio::{
        arrangement::{ArrangementPlayer, RenderOptions, ScopedEq},
        stem_import_plan::{PartSelection, PreferredFormat},
    },
    studio_native::{arrangement_project, stem_bundle::PreparedImport},
};
use anyhow::{Context, Result, ensure};
use eframe::egui;
use std::{
    path::PathBuf,
    sync::{Arc, atomic::Ordering},
};

impl Studio {
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
    pub(super) fn prepare_arrangement_player(&mut self, ctx: &egui::Context) {
        if self.busy() {
            return;
        }
        let Some(document) = self.arrangement.editor.document().cloned() else {
            return;
        };
        let assets = self.arrangement.editor.assets.clone();
        let options = self.arrangement.options.clone();
        let reference = self.arrangement.reference.clone();
        let origin = self
            .arrangement
            .origin
            .clone()
            .unwrap_or_else(|| document.clone());
        let epoch = self.arrangement.editor.epoch;
        self.begin(ctx, "Preparing timeline listening", move |cancel, _| {
            let cancelled = || cancel.load(Ordering::Relaxed);
            let player =
                ArrangementPlayer::new_cancellable(&document, &assets, &options, &cancelled)?;
            let reference = match reference {
                Some(reference) => reference,
                None => Arc::new(crate::studio::arrangement::render(
                    &origin, &assets, &options, &cancelled,
                )?),
            };
            Ok(StudioChange::Arrangement(Box::new(Change::Player {
                epoch,
                player: Box::new(player),
                reference,
            })))
        });
    }
    pub(super) fn suggest_arrangement_eq(&mut self, target: ScopedEq, ctx: &egui::Context) {
        let Some(document) = self.arrangement.editor.document().cloned() else {
            return;
        };
        let assets = self.arrangement.editor.assets.clone();
        let options = RenderOptions {
            protect_peaks: false,
            ..self.arrangement.options.clone()
        };
        let epoch = self.arrangement.editor.epoch;
        let profile = self.arrangement.profile.clone();
        self.begin(ctx, "Measuring the selected EQ target", move |cancel, _| {
            let cancelled = || cancel.load(Ordering::Relaxed);
            let excluded = document.eq.iter().any(|effect| effect.id == target.id).then_some(target.id.as_str());
            let selected = crate::studio::arrangement::render_eq_target(&document, &assets,
                &target.scope, target.range, excluded, &options, &cancelled)?;
            let tonal = crate::profiles::resolve(&profile, &crate::paths::app_root().join("profiles"))?;
            let measured = crate::analysis::analyze(&selected.audio, &tonal)?;
            let mut proposal = crate::dsp::eq_suggestion::proposal(&measured, &tonal)?;
            proposal.notes.push("This curve targets the selected scope. Other clip, track and song effects can add to it.".into());
            ensure!(!cancelled(), "Cancelled");
            Ok(StudioChange::Arrangement(Box::new(Change::Suggestion { epoch, profile, target, proposal: Box::new(proposal) })))
        });
    }
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
    pub(super) fn render_arrangement(&mut self, promote: bool, ctx: &egui::Context) {
        if let Err(error) = self.arrangement.editor.commit_names() {
            self.error = Some(error.to_string());
            return;
        }
        let Some(document) = self.arrangement.editor.document().cloned() else {
            return;
        };
        let assets = self.arrangement.editor.assets.clone();
        let options = self.arrangement.options.clone();
        let epoch = self.arrangement.editor.epoch;
        self.begin(
            ctx,
            "Preparing the current timeline mix",
            move |cancel, _| {
                let audio = Arc::new(crate::studio::arrangement::render(
                    &document,
                    &assets,
                    &options,
                    &|| cancel.load(Ordering::Relaxed),
                )?);
                let loudness = crate::meters::integrated(&audio)?;
                Ok(StudioChange::Arrangement(Box::new(Change::Rendered {
                    epoch,
                    audio,
                    loudness,
                    promote,
                })))
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
    pub(in crate::gui) fn set_arrangement_profile(&mut self, profile: &str) {
        if self.arrangement.profile != profile {
            self.arrangement.profile = profile.into();
            self.arrangement.editor.eq_proposal = None;
            self.arrangement.editor.eq_notice = None;
        }
    }
}
