//! Region edits replace one immutable bank member and commit a complete mix revision.
use super::{
    jobs::Change,
    state::{Revision, Studio},
};
use crate::studio::{
    region_edit::{self, RegionEdit, RegionOperation},
    stem_mix,
};
use eframe::egui;
use std::sync::{Arc, atomic::Ordering};

impl Studio {
    pub(super) fn edit_selected_region(&mut self, operation: RegionOperation, ctx: &egui::Context) {
        if self.busy() {
            self.error = Some("Wait for the current Studio job or cancel it first.".into());
            return;
        }
        if self.mix_dirty {
            self.error = Some(
                "Update mix preview before editing a time selection; live controls are preserved."
                    .into(),
            );
            return;
        }
        let (Some(bank), Some(kind), Some((start_seconds, end_seconds)), Some(current)) = (
            self.bank.clone(),
            self.editor.selected,
            self.editor.selection,
            self.current.as_ref(),
        ) else {
            self.error = Some("Select a stem and drag a time selection first.".into());
            return;
        };
        let before = self
            .source
            .as_ref()
            .map_or_else(|| current.before.clone(), |source| source.audio.clone());
        let mix = self.mix.clone();
        let label = format!("{} · selected region", operation.label());
        let edit = RegionEdit {
            start_seconds,
            end_seconds,
            operation,
        };
        self.begin(ctx, "Editing the selected stem and preparing its mix", move |cancel, progress| {
            let stem = bank.track(kind)?;
            let audio = region_edit::render(&stem.audio, &edit, &|| cancel.load(Ordering::Relaxed))?;
            let bank = Arc::new(bank.replace(kind, Arc::new(audio))?);
            progress(0.5, "Preparing the edited mix; other parts keep their timing");
            let audio = stem_mix::render(&bank, &mix, &|| cancel.load(Ordering::Relaxed))?;
            let before_lufs = crate::meters::integrated(&before)?;
            let after_lufs = crate::meters::integrated(&audio)?;
            anyhow::ensure!(!cancel.load(Ordering::Relaxed), "Cancelled");
            Ok(Change::Preview(Revision {
                before, audio: Arc::new(audio), label,
                notes: vec![format!("{:?} · {:.3}–{:.3} s on {} only. Stem timing and other parts are unchanged. Undo restores the previous bank.", edit.operation, start_seconds, end_seconds, bank.display_name(kind))],
                before_lufs, after_lufs, bank: Some(bank), mix,
            }))
        });
    }

    pub(super) fn update_editor_loop(&mut self) {
        if self.editor.looping && !self.editor.can_loop() {
            self.editor.looping = false;
            self.status =
                "Loop audition needs at least 50 ms; sample edits remain available.".into();
        }
        let range = if self.editor.looping {
            self.editor.selection
        } else {
            None
        };
        if let Err(error) = self.player.set_loop(range) {
            self.editor.looping = false;
            self.error = Some(match self.player.set_loop(None) {
                Ok(()) => format!("Cannot loop this selection: {error:#}"),
                Err(clear) => format!(
                    "Cannot loop this selection: {error:#}; clearing the old loop also failed: {clear:#}"
                ),
            });
        }
    }
}
