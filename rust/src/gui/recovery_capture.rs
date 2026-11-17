//! Capture immutable buffer handles and small editing metadata without disk I/O.
use super::{
    app::MasterCraftApp,
    recovery_document::*,
    studio::state::{Page, Revision, Target},
};
use crate::{audio::Audio, studio::stem_mix::StemSet};
use anyhow::{Result, ensure};
use std::sync::Arc;

pub(super) struct Capture {
    pub payload: serde_json::Value,
    pub audio: Vec<Arc<Audio>>,
}
#[derive(Default)]
pub(super) struct Attachments(Vec<Arc<Audio>>);
impl Attachments {
    pub(super) fn add(&mut self, audio: &Arc<Audio>) -> usize {
        if let Some(index) = self.0.iter().position(|old| Arc::ptr_eq(old, audio)) {
            return index;
        }
        self.0.push(audio.clone());
        self.0.len() - 1
    }
    fn bank(&mut self, bank: &StemSet) -> SavedBank {
        SavedBank {
            stems: bank
                .tracks
                .iter()
                .map(|stem| SavedStem {
                    kind: stem.kind,
                    name: bank.display_name(stem.kind).into_owned(),
                    audio: self.add(&stem.audio),
                })
                .collect(),
        }
    }
    fn revision(&mut self, r: &Revision) -> SavedRevision {
        SavedRevision {
            before: self.add(&r.before),
            audio: self.add(&r.audio),
            bank: r.bank.as_ref().map(|b| self.bank(b)),
            mix: r.mix.clone(),
            label: r.label.clone(),
            notes: r.notes.clone(),
            before_lufs: r.before_lufs,
            after_lufs: r.after_lufs,
        }
    }
}
pub(super) fn capture(app: &MasterCraftApp) -> Result<Capture> {
    ensure!(
        app.tracks.len() <= 4096,
        "Too many queued inputs for a recovery snapshot"
    );
    super::recovery_restore::validate_draft(&app.settings)?;
    let mut assets = Attachments::default();
    let tracks = app
        .tracks
        .iter()
        .map(|t| SavedTrack {
            path: t.path.clone(),
            result: t.result.as_ref().map(|r| SavedResult {
                before: assets.add(&r.before),
                raw: r.raw.as_ref().map(|a| assets.add(a)),
                after: r.after.as_ref().map(|a| assets.add(a)),
                analysis: r.analysis.clone(),
                raw_analysis: r.raw_analysis.as_deref().cloned(),
                eq: r.eq_parameters.map(Into::into),
                report: r.report.clone(),
            }),
        })
        .collect();
    let s = &app.studio;
    let studio = SavedStudio {
        arrangement: s.capture_recovery_arrangement(&mut assets),
        source: s.source.as_ref().map(|s| SavedSource {
            path: s.path.clone(),
            audio: assets.add(&s.audio),
            loudness: s.loudness,
        }),
        bank: s.bank.as_ref().map(|b| assets.bank(b)),
        current: s.current.as_ref().map(|r| assets.revision(r)),
        mix: s.mix.clone(),
        mix_dirty: s.mix_dirty,
        monitoring_mix: s.monitoring_mix,
        page: match s.page {
            Page::Tracks => "tracks",
            Page::Stems => "stems",
            Page::Voice => "voice",
            Page::Repair => "repair",
            Page::Plugins => "plugins",
        }
        .into(),
        target: match s.target {
            Target::Source => SavedTarget::Source,
            Target::Preview => SavedTarget::Preview,
            Target::Stem(kind) => SavedTarget::Stem(kind),
        },
        voice: s.voice.clone(),
        repair: s.repair.clone(),
        two_stems: s.two_stems,
        volume: s.volume,
        match_levels: s.match_levels,
        position: s.player.seconds(),
        output_dir: s.output_dir.clone(),
        export_format: s.export_format.clone(),
        plugin_path: s.plugin_path.clone(),
        plugin_class: s.plugin_class.clone(),
        plugin_values: s.plugin_values.clone(),
        plugin_probe: s.plugin_probe.clone(),
    };
    let doc = Document {
        version: 1,
        settings: app.settings.clone(),
        tracks,
        selected: app.selected,
        tab: app.tab,
        selected_band: app.selected_band,
        volume: app.volume,
        match_levels: app.match_levels,
        position: app.player.seconds(),
        studio,
    };
    let payload = serde_json::to_value(doc)?;
    Ok(Capture {
        payload,
        audio: assets.0,
    })
}
