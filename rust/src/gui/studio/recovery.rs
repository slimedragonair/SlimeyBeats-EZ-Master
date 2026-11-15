//! Adopt verified recovery data without executing plugins, processing audio or playing it.
use super::state::{Page, Studio, Target};
use crate::gui::{
    recovery_document::{SavedStudio, SavedTarget},
    recovery_restore::PreparedStudio,
};
use std::sync::{Arc, atomic::Ordering};

impl Studio {
    pub(in crate::gui) fn adopt_recovery(&mut self, data: PreparedStudio, saved: SavedStudio) {
        self.doctor = Default::default();
        self.player.stop();
        self.source = data.source;
        self.bank = data.bank;
        self.editor = Default::default();
        self.editor_waveforms = Default::default();
        self.current = data.current;
        self.bulk_import = None;
        self.undo.clear();
        self.redo.clear();
        self.auto_action = None;
        self.voice = saved.voice;
        self.repair = saved.repair;
        self.mix = saved.mix;
        self.mix_dirty = saved.mix_dirty;
        self.two_stems = saved.two_stems;
        self.volume = saved.volume;
        self.match_levels = saved.match_levels;
        self.output_dir = saved.output_dir;
        self.export_format = saved.export_format;
        self.plugin_path = saved.plugin_path;
        self.plugin_class = saved.plugin_class;
        self.plugin_values = saved.plugin_values;
        self.plugin_probe = saved.plugin_probe;
        self.target = match saved.target {
            SavedTarget::Source => Target::Source,
            SavedTarget::Preview => Target::Preview,
            SavedTarget::Stem(k) => Target::Stem(k),
        };
        self.page = match saved.page.as_str() {
            "voice" => Page::Voice,
            "repair" => Page::Repair,
            "plugins" => Page::Plugins,
            _ => Page::Stems,
        };
        self.waveform = data.waveform;
        self.stem_waveforms = data.stem_waveforms;
        self.waveform_bank = self.bank.clone();
        self.spectrogram = None;
        self.texture = None;
        self.analysis = None;
        self.selection_anchor = None;
        self.reveal_spectrogram = false;
        self.monitoring_mix = false;
        self.error = None;
        if let Some(revision) = &mut self.current {
            // Equal PCM is insufficient: only the preserved attachment identities
            // establish that this is the same bank used by the prepared preview.
            let bound = revision
                .bank
                .as_ref()
                .zip(self.bank.as_ref())
                .is_some_and(|(a, b)| {
                    a.tracks.len() == b.tracks.len()
                        && a.tracks
                            .iter()
                            .zip(&b.tracks)
                            .all(|(a, b)| a.kind == b.kind && Arc::ptr_eq(&a.audio, &b.audio))
                });
            if bound {
                revision.bank = self.bank.clone();
            }
            let live = saved.monitoring_mix || bound;
            let result = if live && let Some(bank) = &self.bank {
                self.player.load_stems(
                    revision.before.clone(),
                    revision.audio.clone(),
                    (revision.before_lufs, revision.after_lufs),
                    bank.clone(),
                    &self.mix,
                )
            } else {
                self.player.load(
                    revision.before.clone(),
                    Some(revision.audio.clone()),
                    revision.before_lufs,
                    revision.after_lufs,
                    None,
                )
            };
            match result {
                Ok(()) => {
                    if saved.monitoring_mix
                        && let Some(bank) = &self.bank
                    {
                        match self.player.set_mix(bank,&self.mix) {
                            Ok(true)=>{
                                self.monitoring_mix=true;
                                self.player.controls.after.store(true,Ordering::Relaxed);
                                self.player.controls.matched.store(false,Ordering::Relaxed);
                            }
                            Ok(false)=>self.error=Some("Studio recovered, but live mix controls could not be connected. Update the mix preview before auditioning.".into()),
                            Err(error)=>self.error=Some(format!("Studio recovered; live mix controls need attention: {error:#}")),
                        }
                    }
                }
                Err(error) => {
                    self.error = Some(format!(
                        "Studio recovered; playback device is unavailable: {error:#}"
                    ))
                }
            }
        }
        self.player.controls.playing.store(false, Ordering::Relaxed);
        self.player.seek(saved.position);
        self.status = if self.mix_dirty {
            "Recovered live edits · update mix preview before saving"
        } else {
            "Studio recovered · playback is paused"
        }
        .into();
    }
}
