//! Explicit listening and analysis jobs; no action here exports a delivery file.
use super::{arrangement_runtime::Change, jobs::Change as StudioChange, state::Studio};
use crate::studio::arrangement::{ArrangementPlayer, RenderOptions, ScopedEq};
use anyhow::ensure;
use eframe::egui;
use std::sync::{Arc, atomic::Ordering};

impl Studio {
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
    pub(super) fn render_arrangement(&mut self, promote: bool, ctx: &egui::Context) {
        if let Err(error) = self.arrangement.editor.commit_names() {
            self.error = Some(error.to_string());
            return;
        }
        self.player.controls.playing.store(false, Ordering::Relaxed);
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
    pub(in crate::gui) fn set_arrangement_profile(&mut self, profile: &str) {
        if self.arrangement.profile != profile {
            self.arrangement.profile = profile.into();
            self.arrangement.editor.eq_proposal = None;
            self.arrangement.editor.eq_notice = None;
        }
    }
}
