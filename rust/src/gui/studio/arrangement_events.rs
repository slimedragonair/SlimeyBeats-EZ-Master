//! Publish only a current, fully prepared arrangement revision; stale work stays private.
use super::arrangement_state::Inspector;
use super::{
    arrangement_runtime::Change,
    state::{LoadedSource, Page, Revision, Studio, Target},
};
use crate::studio::stem_mix::MixSettings;
use anyhow::{Result, ensure};
use eframe::egui;
use std::{path::PathBuf, sync::atomic::Ordering};

impl Studio {
    pub(super) fn accept_arrangement(&mut self, change: Change, _ctx: &egui::Context) {
        let result = (|| -> Result<()> {
            match change {
                Change::ImportPlan(prepared) => {
                    self.bulk_import =
                        Some(super::stem_bulk_import::Review::for_arrangement(prepared));
                    self.page = Page::Tracks;
                    self.status =
                        "Choose tracks and encoding, then add them to the timeline.".into();
                }
                Change::Session(ready) => {
                    let ready = *ready;
                    ensure!(
                        ready.expected_epoch == self.arrangement.editor.epoch,
                        "The timeline changed while importing. The current session was preserved."
                    );
                    let loaded = ready.loaded_project;
                    self.remember_legacy_playback();
                    let playback = self
                        .player
                        .load_arrangement(ready.reference.clone(), ready.player);
                    if loaded {
                        self.arrangement.waveforms.clear();
                        self.source = None;
                        self.current = None;
                        self.bank = None;
                        self.mix = MixSettings::default();
                        self.mix_dirty = false;
                        self.undo.clear();
                        self.redo.clear();
                        self.arrangement.rendered_audio = None;
                    }
                    for (id, waveform) in ready.waveforms {
                        self.arrangement.waveforms.insert(id, waveform);
                    }
                    self.arrangement.editor = ready.editor;
                    self.arrangement.reference = Some(ready.reference);
                    self.arrangement.origin = Some(ready.origin);
                    self.arrangement.options = ready.options;
                    self.arrangement.gesture = None;
                    self.arrangement.sent_epoch =
                        playback.is_ok().then_some(self.arrangement.editor.epoch);
                    self.arrangement.rendered_epoch = None;
                    self.arrangement.saved_epoch = loaded.then_some(self.arrangement.editor.epoch);
                    self.bulk_import = None;
                    self.page = Page::Tracks;
                    if std::mem::take(&mut self.arrangement.capture_eq_on_load) {
                        let effect = self
                            .arrangement
                            .editor
                            .document()
                            .and_then(|document| document.eq.first())
                            .cloned();
                        if let Some(effect) = effect {
                            super::arrangement_eq_effects::select(
                                &mut self.arrangement.editor,
                                &effect,
                            )?;
                        } else {
                            self.arrangement.editor.inspector = Inspector::Eq;
                            self.arrangement.editor.refresh_eq();
                        }
                    }
                    self.player.controls.after.store(true, Ordering::Relaxed);
                    self.player.controls.matched.store(false, Ordering::Relaxed);
                    if let Err(error) = playback {
                        self.error = Some(format!(
                            "Tracks are ready; playback device is unavailable: {error:#}"
                        ));
                    }
                    self.status =
                        "Timeline ready · move titles, trim edges, or EQ a selected part.".into();
                }
                Change::Player {
                    epoch,
                    player,
                    reference,
                } => {
                    ensure!(
                        epoch == self.arrangement.editor.epoch && self.page == Page::Tracks,
                        "Playback preparation belongs to an older timeline or workspace"
                    );
                    self.remember_legacy_playback();
                    self.player.load_arrangement(reference.clone(), *player)?;
                    self.arrangement.reference = Some(reference);
                    self.arrangement.sent_epoch = Some(epoch);
                    self.player
                        .controls
                        .after
                        .store(self.arrangement.after, Ordering::Relaxed);
                    self.player.controls.matched.store(false, Ordering::Relaxed);
                    self.player.seek(self.arrangement.position);
                    self.status = "Timeline listening ready. Press Play to audition.".into();
                }
                Change::Suggestion {
                    epoch,
                    profile,
                    target,
                    proposal,
                } => {
                    ensure!(
                        profile == self.arrangement.profile,
                        "The tonal profile changed. Suggest again for the current profile."
                    );
                    self.arrangement
                        .accept_suggestion(epoch, &target, *proposal)?;
                    self.status =
                        "EQ proposal ready for the visible target. Apply curve to hear it.".into();
                }
                Change::Rendered {
                    epoch,
                    audio,
                    loudness,
                    promote,
                } => {
                    ensure!(
                        epoch == self.arrangement.editor.epoch,
                        "Timeline changed while rendering; prepare the current revision again"
                    );
                    self.arrangement.rendered_epoch = Some(epoch);
                    self.arrangement.rendered_audio = Some(audio.clone());
                    self.current = Some(Revision { before: audio.clone(), audio: audio.clone(),
                        label: "Timeline mix".into(), notes: vec!["Positions, source trims, track controls and scoped EQ rendered from the current immutable-source recipe.".into()],
                        before_lufs: loudness, after_lufs: loudness, bank: None, mix: MixSettings::default() });
                    self.source = Some(LoadedSource {
                        audio,
                        path: PathBuf::from("Timeline mix.wav"),
                        loudness,
                    });
                    self.bank = None;
                    self.mix = MixSettings::default();
                    self.mix_dirty = false;
                    self.target = Target::Preview;
                    self.arrangement.promote = promote;
                    if !promote && let Some(revision) = self.current.take() {
                        self.accept(revision, false);
                        self.page = Page::Voice;
                    }
                    self.status = if promote {
                        "Timeline mix ready for mastering"
                    } else {
                        "Timeline snapshot ready for Studio tools"
                    }
                    .into();
                }
                Change::Saved { epoch, path } => {
                    if epoch == self.arrangement.editor.epoch {
                        self.arrangement.saved_epoch = Some(epoch);
                    }
                    self.status = format!("Saved clip project: {}", path.display());
                }
            }
            Ok(())
        })();
        if let Err(error) = result {
            self.error = Some(error.to_string());
        }
    }
}
