use super::{
    app::{JobKind, MasterCraftApp},
    plots::Waveform,
    worker::Event,
};
use std::sync::{Arc, atomic::Ordering};

impl MasterCraftApp {
    pub(super) fn poll(&mut self) {
        let mut resume = None;
        while let Ok(event) = self.rx.try_recv() {
            match event {
                Event::EqRendered(index, revision, result) => {
                    if self.eq_live.pending == Some(revision) {
                        self.eq_live.pending = None;
                    }
                    let current = self
                        .tracks
                        .get(index)
                        .and_then(|track| track.result.as_deref());
                    if self
                        .eq_live
                        .accepts(index, self.selected, revision, &result, current)
                    {
                        if let Some(parameters) = result.eq_parameters {
                            self.player.set_eq(parameters);
                            self.eq_live.parameters = Some(parameters);
                        }
                        self.eq_live.ready = revision;
                        self.eq_dirty = false;
                        self.waveform = Waveform::new(&result.before, result.after.as_deref());
                        self.eq_waveform = Waveform::new(
                            result.raw.as_deref().unwrap_or(&result.before),
                            result.after.as_deref(),
                        );
                        self.playback_spectrum.reset();
                        let before = if self.tab == 1 {
                            result
                                .raw_analysis
                                .as_ref()
                                .map(|a| a.metrics.integrated_lufs)
                                .unwrap_or(result.analysis.metrics.integrated_lufs)
                        } else {
                            result.analysis.metrics.integrated_lufs
                        };
                        if let Some(report) = &result.report {
                            self.player
                                .set_match(before, report.output.metrics.integrated_lufs);
                        }
                        if let Some(track) = self.tracks.get_mut(index) {
                            track.result = Some(result);
                            track.status = "Live EQ / export ready".into();
                        }
                        self.status = "Live EQ active — export measurements ready".into();
                    }
                }
                Event::EqRenderFailed(index, revision, error) => {
                    if self.eq_live.pending == Some(revision) {
                        self.eq_live.pending = None;
                    }
                    if self.selected == Some(index) && self.eq_live.revision == revision {
                        self.error = Some(format!("Export preparation: {error}"));
                        self.eq_live.ready = revision;
                    }
                }
                Event::Progress(index, fraction, message) => {
                    self.progress =
                        ((index as f32 + fraction) / self.tracks.len().max(1) as f32).clamp(0., 1.);
                    self.status = message.clone();
                    if let Some(track) = self.tracks.get_mut(index) {
                        track.status = message;
                    }
                }
                Event::Ready(index, mut result) => {
                    let incoming_audio = result.after.is_some();
                    let mut refresh = true;
                    if let Some(track) = self.tracks.get_mut(index) {
                        if let Some(previous) = &track.result {
                            if !incoming_audio
                                && previous.after.is_some()
                                && result.before.sample_rate == previous.before.sample_rate
                                && result.before.channels == previous.before.channels
                                && result.before.samples == previous.before.samples
                            {
                                let mut retained = previous.as_ref().clone();
                                retained.analysis = result.analysis.clone();
                                result = Arc::new(retained);
                                refresh = false;
                            } else if Arc::ptr_eq(&previous.before, &result.before) {
                                let same_after = match (&previous.after, &result.after) {
                                    (Some(a), Some(b)) => Arc::ptr_eq(a, b),
                                    (None, None) => true,
                                    _ => false,
                                };
                                refresh = !same_after;
                                if incoming_audio
                                    && previous
                                        .raw
                                        .as_ref()
                                        .zip(result.raw.as_ref())
                                        .is_some_and(|(a, b)| Arc::ptr_eq(a, b))
                                {
                                    refresh = false;
                                }

                                if refresh && self.selected == Some(index) {
                                    resume = Some((
                                        self.player.seconds(),
                                        self.player.controls.playing.load(Ordering::Relaxed),
                                    ));
                                }
                            }
                        }
                        track.status = if result.after.is_some() {
                            if result
                                .report
                                .as_ref()
                                .is_some_and(|r| !r.exports.is_empty())
                            {
                                "Exported"
                            } else {
                                "Preview ready"
                            }
                        } else {
                            "Analyzed"
                        }
                        .into();
                        track.result = Some(result);
                    }
                    if incoming_audio {
                        self.eq_dirty = false;
                        if self.active_job == Some(JobKind::Export) {
                            self.eq_live.ready = self.eq_live.revision;
                            if let Some(parameters) = self
                                .tracks
                                .get(index)
                                .and_then(|t| t.result.as_ref())
                                .and_then(|r| r.eq_parameters)
                            {
                                self.eq_live.parameters = Some(parameters);
                                self.player.set_eq(parameters);
                            }
                        }
                    }
                    if incoming_audio
                        && self.selected == Some(index)
                        && let Some(value) = self.tracks.get(index).and_then(|t| t.result.as_ref())
                    {
                        self.waveform = Waveform::new(&value.before, value.after.as_deref());
                        self.eq_waveform = Waveform::new(
                            value.raw.as_deref().unwrap_or(&value.before),
                            value.after.as_deref(),
                        );
                        self.playback_spectrum.reset();
                    }
                    if self.selected == Some(index) && refresh {
                        self.loaded = None;
                    }
                    if self.active_job == Some(JobKind::Master) {
                        self.suggested_eq = None;
                        self.suggestion_note = None;
                    }
                }
                Event::SuggestionUnavailable(reason) => {
                    self.suggested_eq = None;
                    self.suggestion_note = Some(reason.clone());
                    self.status = reason;
                }
                Event::Suggestion(index, bands, note) => {
                    if self.selected == Some(index) {
                        self.suggested_eq = Some(bands);
                        self.suggestion_note = Some(note);
                    } else {
                        self.error = Some(
                            "Selection changed. Run Suggest EQ for the selected track.".into(),
                        );
                    }
                }
                Event::Failed(index, error) => {
                    if let Some(track) = self.tracks.get_mut(index) {
                        track.status = if error == "Cancelled" {
                            "Cancelled"
                        } else {
                            "Failed"
                        }
                        .into();
                    }
                    self.error = Some(error);
                }
                Event::Finished => {
                    let job = self.active_job.take();
                    self.busy = false;
                    self.progress = 1.;
                    self.status=if self.cancel.load(Ordering::Relaxed){"Cancelled; completed exports are preserved"}
                    else if self.error.is_some(){"Finished with errors — details above"}
                    else{match job {
                        Some(JobKind::Suggest) if self.suggested_eq.is_none()=>self.suggestion_note.as_deref().unwrap_or("No reliable EQ suggestion"),
                        Some(JobKind::Suggest)=>"EQ suggestion ready — inspect it, then Apply suggested EQ",
                        Some(JobKind::Export)=>"Export saved",
                        Some(JobKind::Analyze)=>"Analysis ready — existing master preserved when the source is unchanged",
                        _=>"Master ready — tweak, re-master, or export",
                    }}.into();
                }
            }
        }
        if let Some(index) = self
            .selected
            .filter(|i| Some(*i) != self.loaded || self.loaded_eq != (self.tab == 1))
            && let Some(result) = self.tracks.get(index).and_then(|t| t.result.clone())
        {
            if self.loaded == Some(index) && self.loaded_eq != (self.tab == 1) && resume.is_none() {
                resume = Some((
                    self.player.seconds(),
                    self.player.controls.playing.load(Ordering::Relaxed),
                ));
            }
            self.waveform = Waveform::new(&result.before, result.after.as_deref());
            self.eq_waveform = Waveform::new(
                result.raw.as_deref().unwrap_or(&result.before),
                result.after.as_deref(),
            );
            self.playback_spectrum.reset();
            self.loaded = Some(index);
            self.loaded_eq = self.tab == 1;
            let out = result
                .report
                .as_ref()
                .map(|r| r.output.metrics.integrated_lufs)
                .unwrap_or(result.analysis.metrics.integrated_lufs);
            let before = if self.loaded_eq {
                result.raw.as_ref().unwrap_or(&result.before).clone()
            } else {
                result.before.clone()
            };
            let before_lufs = if self.loaded_eq {
                result
                    .raw_analysis
                    .as_ref()
                    .map(|a| a.metrics.integrated_lufs)
                    .unwrap_or(result.analysis.metrics.integrated_lufs)
            } else {
                result.analysis.metrics.integrated_lufs
            };
            if let Err(error) = self.player.load(
                before,
                result.after.clone(),
                before_lufs,
                out,
                result
                    .raw
                    .as_ref()
                    .zip(self.eq_live.parameters_for(&result))
                    .map(|(raw, params)| (raw.clone(), params)),
            ) {
                self.error = Some(format!("Playback device: {error:#}"));
            }
            if let Some((seconds, playing)) = resume {
                self.player.seek(seconds);
                if playing {
                    self.player.toggle();
                }
            }
        }
    }
}
