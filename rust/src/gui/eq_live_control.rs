use super::{app::MasterCraftApp, eq_jobs, worker::TrackResult};
use crate::{
    config::EqBand,
    dsp::{eq_chain::EqParameters, eq_loudness::EqLoudnessModel},
};
use eframe::egui;
use std::{
    sync::{Arc, atomic::Ordering},
    time::{Duration, Instant},
};

#[derive(Clone, PartialEq)]
struct Key {
    source: usize,
    bands: Vec<EqBand>,
    hold: bool,
    output: f64,
    ceiling: f64,
}
#[derive(Default)]
pub struct EqLiveState {
    key: Option<Key>,
    model: Option<EqLoudnessModel>,
    changed: Option<Instant>,
    pub source: usize,
    pub revision: u64,
    pub ready: u64,
    pub pending: Option<u64>,
    pub parameters: Option<EqParameters>,
}
impl EqLiveState {
    pub fn reset_source(&mut self) {
        self.key = None;
        self.model = None;
        self.changed = None;
        self.source = 0;
        self.ready = 0;
        self.parameters = None;
    }
    pub fn accepts(
        &self,
        index: usize,
        selected: Option<usize>,
        revision: u64,
        incoming: &TrackResult,
        current: Option<&TrackResult>,
    ) -> bool {
        selected == Some(index)
            && revision == self.revision
            && revision > self.ready
            && incoming
                .raw
                .as_ref()
                .is_some_and(|raw| Arc::as_ptr(raw) as usize == self.source)
            && current.is_some_and(|current| {
                incoming
                    .raw
                    .as_ref()
                    .zip(current.raw.as_ref())
                    .is_some_and(|(a, b)| Arc::ptr_eq(a, b))
            })
    }
    pub fn parameters_for(&self, result: &TrackResult) -> Option<EqParameters> {
        if result
            .raw
            .as_ref()
            .is_some_and(|raw| Arc::as_ptr(raw) as usize == self.source)
        {
            self.parameters
        } else {
            result.eq_parameters
        }
    }
}

impl MasterCraftApp {
    pub(super) fn sync_live_eq(&mut self, ctx: &egui::Context) {
        if self.player.controls.failure.swap(false, Ordering::Relaxed) {
            self.error=Some("Audio preview stopped because the DSP returned an invalid sample. Re-master or reload the track before resuming.".into());
        }
        let Some((index, result)) = self.current() else {
            return;
        };
        let (Some(raw), Some(analysis)) = (&result.raw, &result.raw_analysis) else {
            return;
        };
        let source = Arc::as_ptr(raw) as usize;
        let key = Key {
            source,
            bands: self.settings.manual_eq.clone(),
            hold: self.settings.eq_hold_loudness,
            output: self.settings.eq_output_db,
            ceiling: self.settings.ceiling_dbtp,
        };
        if self.eq_live.key.as_ref() != Some(&key) {
            let fresh = self.eq_live.source != source;
            if fresh {
                self.eq_live.model = Some(EqLoudnessModel::new(
                    &analysis.spectrum,
                    raw.sample_rate,
                    analysis.metrics.integrated_lufs,
                ));
                self.eq_live.source = source;
            }
            self.eq_live.revision += 1;
            self.eq_live.changed = Some(Instant::now());
            let hold = self
                .eq_live
                .model
                .as_ref()
                .map(|m| m.hold_gain(&self.settings))
                .unwrap_or(0.);
            let mut parameters = if eq_jobs::settings_match(&result, &self.settings) {
                self.eq_live.ready = self.eq_live.revision;
                self.eq_dirty = false;
                result.eq_parameters.unwrap()
            } else {
                self.eq_dirty = true;
                EqParameters::new(&self.settings, raw.sample_rate, hold)
            };
            if !parameters.active() {
                parameters.post_gain_db =
                    (self.settings.ceiling_dbtp - 0.08 - analysis.metrics.true_peak_dbtp).min(0.);
            }
            self.eq_live.parameters = Some(parameters);
            self.eq_live.key = Some(key);
            if !self.player.set_eq(parameters) && self.error.is_none() {
                self.error = Some(
                    "Live EQ playback stream is unavailable; reload the track to reconnect it."
                        .into(),
                );
            }
            let delta = self
                .eq_live
                .model
                .as_ref()
                .map(|m| m.delta_db(&self.settings))
                .unwrap_or(0.);
            let estimated = analysis.metrics.integrated_lufs
                + delta
                + hold
                + self.settings.eq_output_db
                + parameters.post_gain_db;
            let before = if self.tab == 1 {
                analysis.metrics.integrated_lufs
            } else {
                result.analysis.metrics.integrated_lufs
            };
            self.player.set_match(before, estimated);
            if !fresh && self.eq_dirty {
                self.status =
                    "Live EQ active — preparing export measurements in the background".into();
            }
        }
        if !self.busy
            && self.eq_live.ready != self.eq_live.revision
            && self.eq_live.pending.is_none()
            && self
                .eq_live
                .changed
                .is_some_and(|time| time.elapsed() >= Duration::from_millis(180))
        {
            let revision = self.eq_live.revision;
            self.eq_live.pending = Some(revision);
            eq_jobs::render_revision(
                index,
                revision,
                result,
                self.settings.clone(),
                self.tx.clone(),
                ctx.clone(),
            );
        }
        if self.eq_dirty || self.eq_live.pending.is_some() {
            ctx.request_repaint_after(Duration::from_millis(60));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{analysis, audio::Audio, profiles};
    use std::path::Path;
    #[test]
    fn stale_render_cannot_replace_a_different_track_after_queue_rows_shift() {
        let raw = Arc::new(Audio::new(vec![0.; 48000], 48000, 1).unwrap());
        let profile = profiles::resolve("neutral", Path::new(".")).unwrap();
        let analysis = analysis::analyze(&raw, &profile).unwrap();
        let old = TrackResult {
            before: raw.clone(),
            raw: Some(raw.clone()),
            raw_analysis: Some(Arc::new(analysis.clone())),
            eq_parameters: None,
            after: Some(raw.clone()),
            analysis,
            report: None,
        };
        let mut replacement = old.clone();
        replacement.raw = Some(Arc::new(raw.as_ref().clone()));
        let state = EqLiveState {
            source: Arc::as_ptr(&raw) as usize,
            revision: 2,
            ready: 1,
            ..Default::default()
        };
        assert!(!state.accepts(0, Some(0), 2, &old, Some(&replacement)));
        assert!(state.accepts(0, Some(0), 2, &old, Some(&old)));
        assert!(!state.accepts(0, Some(0), 1, &old, Some(&old)));
        assert!(!state.accepts(0, Some(1), 2, &old, Some(&old)));
    }
}
