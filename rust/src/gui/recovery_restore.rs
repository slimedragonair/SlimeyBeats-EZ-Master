//! Validate every relationship in a worker before replacing the live editing session.
use super::{
    app::{MasterCraftApp, Track},
    plots::Waveform,
    recovery_document::*,
    studio::state::{LoadedSource, Revision},
    worker::TrackResult,
};
use crate::{
    analysis::Analysis,
    audio::Audio,
    studio::stem_mix::{Stem, StemSet},
};
use anyhow::{Context, Result, ensure};
use std::sync::{Arc, atomic::Ordering};

pub(super) struct Prepared {
    pub doc: Document,
    pub tracks: Vec<Track>,
    pub studio: PreparedStudio,
    waveform: Waveform,
    eq_waveform: Waveform,
}
pub(super) struct PreparedStudio {
    pub arrangement: Option<super::studio::PreparedArrangement>,
    pub source: Option<LoadedSource>,
    pub bank: Option<Arc<StemSet>>,
    pub current: Option<Revision>,
    pub waveform: Waveform,
    pub stem_waveforms: Vec<(crate::studio::stem_mix::StemKind, Waveform)>,
}
fn aligned(a: &Audio, b: &Audio) -> bool {
    a.sample_rate == b.sample_rate && a.channels == b.channels && a.frames() == b.frames()
}
fn measured(a: &Analysis, audio: &Audio) -> Result<()> {
    ensure!(
        a.sample_rate == audio.sample_rate
            && a.channels == audio.channels
            && (a.duration_s - audio.duration()).abs() < 0.001,
        "Recovery measurements do not match their audio"
    );
    ensure!(
        a.measured_db.len() == crate::profiles::BANDS.len()
            && a.deviation_db.len() == a.measured_db.len()
            && a.valid.len() == a.measured_db.len(),
        "Invalid recovery measurements"
    );
    Ok(())
}
fn audio(assets: &[Arc<Audio>], index: usize) -> Result<Arc<Audio>> {
    assets
        .get(index)
        .cloned()
        .context("Recovery audio reference is missing")
}
pub(super) fn validate_draft(settings: &crate::config::Settings) -> Result<()> {
    let mut validation = settings.clone();
    // Choosing output formats can temporarily leave none selected. Preserve that
    // draft exactly; the ordinary Export handler still requires a valid choice.
    if validation.formats.is_empty() {
        validation.formats.push("wav24".into());
    }
    validation.validate()
}
fn bank(assets: &[Arc<Audio>], saved: &SavedBank) -> Result<Arc<StemSet>> {
    let mut set = StemSet::new(
        saved
            .stems
            .iter()
            .map(|s| {
                Ok(Stem {
                    kind: s.kind,
                    audio: audio(assets, s.audio)?,
                })
            })
            .collect::<Result<_>>()?,
    )?;
    for stem in &saved.stems {
        set.set_display_name(stem.kind, stem.name.clone())?;
    }
    Ok(Arc::new(set))
}
#[cfg(test)]
pub(super) fn prepare(payload: serde_json::Value, assets: Vec<Arc<Audio>>) -> Result<Prepared> {
    prepare_cancellable(payload, assets, &|| false)
}
pub(super) fn prepare_cancellable(
    payload: serde_json::Value,
    assets: Vec<Arc<Audio>>,
    cancel: &dyn Fn() -> bool,
) -> Result<Prepared> {
    ensure!(!cancel(), "Cancelled");
    let mut doc: Document =
        serde_json::from_value(payload).context("Unsupported or damaged recovery editing state")?;
    ensure!(
        doc.version == 1 && doc.tracks.len() <= 4096 && doc.tab <= 3 && doc.selected_band < 7,
        "Unsupported recovery editing state"
    );
    ensure!(
        doc.selected.is_none_or(|i| i < doc.tracks.len()),
        "Invalid recovery queue selection"
    );
    validate_draft(&doc.settings)?;
    for v in [doc.volume, doc.studio.volume] {
        ensure!(
            v.is_finite() && (0.0..=1.0).contains(&v),
            "Invalid recovery listening volume"
        );
    }
    for v in [doc.position, doc.studio.position] {
        ensure!(v.is_finite() && v >= 0.0, "Invalid recovery playhead");
    }
    ensure!(
        crate::config::FORMATS.contains(&doc.studio.export_format.as_str()),
        "Unsupported recovery export format"
    );
    ensure!(
        ["tracks", "stems", "voice", "repair", "plugins"].contains(&doc.studio.page.as_str()),
        "Unknown recovery workspace"
    );
    ensure!(
        doc.studio
            .plugin_values
            .iter()
            .all(|v| v.normalized.is_finite() && (0.0..=1.0).contains(&v.normalized)),
        "Invalid recovered plugin controls"
    );
    let tracks = doc
        .tracks
        .iter()
        .map(|track| {
            let result = track
                .result
                .as_ref()
                .map(|r| {
                    let before = audio(&assets, r.before)?;
                    let raw = r.raw.map(|i| audio(&assets, i)).transpose()?;
                    let after = r.after.map(|i| audio(&assets, i)).transpose()?;
                    measured(&r.analysis, &before)?;
                    if let Some(a) = &raw {
                        ensure!(aligned(a, &before), "Recovery pre-EQ source is not aligned");
                    }
                    if let Some(a) = &after {
                        ensure!(aligned(a, &before), "Recovery master is not aligned");
                    }
                    if let Some(a) = &r.raw_analysis {
                        measured(
                            a,
                            raw.as_deref()
                                .context("Recovery raw measurements have no source")?,
                        )?;
                    }
                    if let Some(report) = &r.report {
                        measured(&report.input, &before)?;
                        measured(
                            &report.output,
                            after.as_deref().context("Recovery report has no preview")?,
                        )?;
                    }
                    let eq_parameters = r.eq.as_ref().map(|e| e.restore()).transpose()?;
                    Ok(Arc::new(TrackResult {
                        before,
                        raw,
                        after,
                        raw_analysis: r.raw_analysis.clone().map(Arc::new),
                        eq_parameters,
                        analysis: r.analysis.clone(),
                        report: r.report.clone(),
                    }))
                })
                .transpose()?;
            let status = match &result {
                Some(r) if r.after.is_some() => "Recovered preview",
                Some(_) => "Recovered analysis",
                None => "Queued · analyze when ready",
            }
            .into();
            Ok(Track {
                path: track.path.clone(),
                status,
                recovered_source: result.as_ref().map(|r| r.before.clone()),
                result,
            })
        })
        .collect::<Result<Vec<_>>>()?;
    let s = &doc.studio;
    let source = s
        .source
        .as_ref()
        .map(|s| {
            ensure!(s.loudness.is_finite(), "Invalid recovery source loudness");
            Ok(LoadedSource {
                path: s.path.clone(),
                audio: audio(&assets, s.audio)?,
                loudness: s.loudness,
            })
        })
        .transpose()?;
    let active_bank = s.bank.as_ref().map(|b| bank(&assets, b)).transpose()?;
    if let Some(b) = &active_bank {
        s.mix.validate(b)?;
    }
    let current = s
        .current
        .as_ref()
        .map(|r| {
            let before = audio(&assets, r.before)?;
            let rendered = audio(&assets, r.audio)?;
            ensure!(
                aligned(&before, &rendered)
                    && r.before_lufs.is_finite()
                    && r.after_lufs.is_finite(),
                "Invalid recovered Studio preview"
            );
            let bank = r.bank.as_ref().map(|b| bank(&assets, b)).transpose()?;
            if let Some(b) = &bank {
                r.mix.validate(b)?;
                ensure!(
                    aligned(&b.tracks[0].audio, &rendered),
                    "Recovered preview has an unrelated stem layout"
                );
            }
            Ok(Revision {
                before,
                audio: rendered,
                label: r.label.clone(),
                notes: r.notes.clone(),
                before_lufs: r.before_lufs,
                after_lufs: r.after_lufs,
                bank,
                mix: r.mix.clone(),
            })
        })
        .transpose()?;
    if let SavedTarget::Stem(kind) = s.target {
        active_bank
            .as_ref()
            .context("Recovered stem target has no bank")?
            .track(kind)?;
    }
    let waveform = current
        .as_ref()
        .map(|r| Waveform::new(&r.before, Some(&r.audio)))
        .unwrap_or_default();
    let stem_waveforms = active_bank
        .as_ref()
        .map(|b| {
            b.tracks
                .iter()
                .map(|s| (s.kind, Waveform::new(&s.audio, None)))
                .collect()
        })
        .unwrap_or_default();
    let (main_wave, eq_wave) = doc
        .selected
        .and_then(|i| tracks.get(i))
        .and_then(|t| t.result.as_ref())
        .map(|r| {
            (
                Waveform::new(&r.before, r.after.as_deref()),
                Waveform::new(r.raw.as_deref().unwrap_or(&r.before), r.after.as_deref()),
            )
        })
        .unwrap_or_default();
    let arrangement = doc
        .studio
        .arrangement
        .take()
        .map(|saved| super::studio::Studio::prepare_recovery_arrangement(saved, &assets, cancel))
        .transpose()?;
    ensure!(!cancel(), "Cancelled");
    Ok(Prepared {
        doc,
        tracks,
        studio: PreparedStudio {
            arrangement,
            source,
            bank: active_bank,
            current,
            waveform,
            stem_waveforms,
        },
        waveform: main_wave,
        eq_waveform: eq_wave,
    })
}
impl Prepared {
    pub fn adopt(self, app: &mut MasterCraftApp) {
        app.player.unload();
        app.studio.player.stop();
        app.tracks = self.tracks;
        app.settings = self.doc.settings.clone();
        app.selected = self.doc.selected;
        app.selected_band = self.doc.selected_band;
        app.volume = self.doc.volume;
        app.match_levels = self.doc.match_levels;
        app.loaded = None;
        app.eq_live = Default::default();
        app.eq_dirty = false;
        app.waveform = self.waveform;
        app.eq_waveform = self.eq_waveform;
        app.playback_spectrum.reset();
        app.studio.adopt_recovery(self.studio, self.doc.studio);
        app.activate_workspace(self.doc.tab);
        app.status = "Recovery restored · playback is paused; export only when ready".into();
        app.error = None;
        if let Some((index, result)) = app.current() {
            let eq = app.tab == 1;
            let before = if eq {
                result.raw.as_ref().unwrap_or(&result.before).clone()
            } else {
                result.before.clone()
            };
            let before_lufs = if eq {
                result
                    .raw_analysis
                    .as_ref()
                    .map(|a| a.metrics.integrated_lufs)
                    .unwrap_or(result.analysis.metrics.integrated_lufs)
            } else {
                result.analysis.metrics.integrated_lufs
            };
            let after_lufs = result
                .report
                .as_ref()
                .map(|r| r.output.metrics.integrated_lufs)
                .unwrap_or(before_lufs);
            let live = result
                .raw
                .as_ref()
                .zip(result.eq_parameters)
                .map(|(raw, eq)| (raw.clone(), eq));
            if let Err(error) =
                app.player
                    .load(before, result.after.clone(), before_lufs, after_lufs, live)
            {
                app.error = Some(format!(
                    "Session recovered; playback device is unavailable: {error:#}"
                ));
            }
            app.loaded = Some(index);
            app.loaded_eq = eq;
        }
        app.player.controls.playing.store(false, Ordering::Relaxed);
        app.player.seek(self.doc.position);
    }
}
