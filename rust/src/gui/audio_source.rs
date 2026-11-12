use super::{playback_tap::PlaybackTap, player::Controls};
use crate::{
    audio::Audio,
    dsp::{
        eq_chain::{EqParameters, Frame},
        realtime_eq::RealtimeEq,
    },
};
use rodio::Source;
use std::{
    sync::{Arc, atomic::Ordering, mpsc::Receiver},
    time::Duration,
};

pub(super) struct LivePath {
    pub raw: Arc<Audio>,
    pub processor: RealtimeEq,
    pub updates: Receiver<EqParameters>,
}
pub(super) struct ABSource {
    pub before: Arc<Audio>,
    pub after: Option<Arc<Audio>>,
    pub controls: Arc<Controls>,
    pub tap: Arc<PlaybackTap>,
    pub live: Option<LivePath>,
    pub cursor: usize,
    pub blend: f32,
    pub gain: f32,
    pub channels: usize,
    pub gain_a: f32,
    pub gain_b: f32,
    pub cached_frame: Option<usize>,
    pub frame_a: Frame,
    pub frame_b: Frame,
}
fn frame(audio: &Audio, index: usize) -> Frame {
    if index >= audio.frames() {
        return [0.; 2];
    }
    [
        audio.samples[index * audio.channels] as f64,
        if audio.channels == 2 {
            audio.samples[index * 2 + 1] as f64
        } else {
            0.
        },
    ]
}
impl ABSource {
    pub fn prime(&mut self, position: usize) {
        self.tap.clear();
        self.cached_frame = None;
        if let Some(live) = &mut self.live {
            let parameters = live.processor.parameters();
            live.processor.reset(parameters);
            let end = position + live.processor.delay();
            let start = position.saturating_sub(live.processor.warm_frames());
            for index in start..end {
                live.processor.process(frame(&live.raw, index));
            }
            self.cursor = end * self.channels;
        } else {
            self.cursor = position * self.channels;
        }
        self.controls
            .position
            .store(position as u64, Ordering::Relaxed);
    }
    fn prepare_frame(&mut self, index: usize) {
        if self.cached_frame == Some(index) {
            return;
        }
        let delay = if let Some(live) = &mut self.live {
            let mut latest = None;
            if index.is_multiple_of(64) {
                while let Ok(parameters) = live.updates.try_recv() {
                    latest = Some(parameters);
                }
            }
            if let Some(parameters) = latest {
                live.processor.set_parameters(parameters);
            }
            self.frame_b = live.processor.process(frame(&live.raw, index));
            live.processor.delay()
        } else {
            self.frame_b = self
                .after
                .as_ref()
                .map(|a| frame(a, index))
                .unwrap_or_else(|| frame(&self.before, index));
            0
        };
        self.frame_a = if index >= delay {
            frame(&self.before, index - delay)
        } else {
            [0.; 2]
        };
        if !self.frame_b.iter().all(|v| v.is_finite()) {
            self.controls.failure.store(true, Ordering::Relaxed);
            self.controls.playing.store(false, Ordering::Relaxed);
            self.frame_b = [0.; 2];
        }
        self.tap.push(self.frame_b);
        self.cached_frame = Some(index);
    }
}
impl Iterator for ABSource {
    type Item = f32;
    fn next(&mut self) -> Option<f32> {
        if self.cursor.is_multiple_of(self.channels) {
            let seek = self.controls.seek.swap(u64::MAX, Ordering::Relaxed);
            if seek != u64::MAX {
                self.prime((seek as usize).min(self.before.frames()));
                self.gain = 0.;
            }
        }
        let index = self.cursor / self.channels;
        let channel = self.cursor % self.channels;
        let delay = self.live.as_ref().map(|l| l.processor.delay()).unwrap_or(0);
        let end = index >= self.before.frames() + delay;
        if end {
            self.controls.playing.store(false, Ordering::Relaxed);
        }
        let playing = self.controls.playing.load(Ordering::Relaxed);
        let desired = if playing && !end {
            f32::from_bits(self.controls.volume.load(Ordering::Relaxed))
        } else {
            0.
        };
        let step = 1. / (0.080 * self.before.sample_rate as f32 * self.channels as f32);
        self.gain += (desired - self.gain).clamp(-step, step);
        self.prepare_frame(index);
        let target = if self.controls.after.load(Ordering::Relaxed) {
            1.
        } else {
            0.
        };
        let change = 1. / (0.020 * self.before.sample_rate as f32 * self.channels as f32);
        self.blend += (target - self.blend).clamp(-change, change);
        let matched = self.controls.matched.load(Ordering::Relaxed);
        let target_a = if matched {
            f32::from_bits(self.controls.match_a.load(Ordering::Relaxed))
        } else {
            1.
        };
        let target_b = if matched {
            f32::from_bits(self.controls.match_b.load(Ordering::Relaxed))
        } else {
            1.
        };
        self.gain_a += (target_a - self.gain_a).clamp(-step, step);
        self.gain_b += (target_b - self.gain_b).clamp(-step, step);
        let a = self.frame_a[channel] as f32 * self.gain_a;
        let b = self.frame_b[channel] as f32 * self.gain_b;
        let output = ((1. - self.blend) * a + self.blend * b) * self.gain;
        if playing || self.gain > 1e-6 || !self.cursor.is_multiple_of(self.channels) {
            self.cursor = self.cursor.saturating_add(1);
            if self.cursor.is_multiple_of(self.channels) {
                self.controls.position.store(
                    (self.cursor / self.channels).saturating_sub(delay) as u64,
                    Ordering::Relaxed,
                );
            }
        }
        Some(output.clamp(-1., 1.))
    }
}
impl Source for ABSource {
    fn current_span_len(&self) -> Option<usize> {
        None
    }
    fn channels(&self) -> u16 {
        self.channels as u16
    }
    fn sample_rate(&self) -> u32 {
        self.before.sample_rate
    }
    fn total_duration(&self) -> Option<Duration> {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{config::Settings, dsp::manual_eq};
    use std::sync::mpsc;
    fn source(settings: &Settings) -> (ABSource, std::sync::mpsc::Sender<EqParameters>) {
        let input = Arc::new(
            Audio::new(
                (0..48000)
                    .flat_map(|i| {
                        vec![
                            (0.01 * (std::f64::consts::TAU * 1000. * i as f64 / 48000.).sin())
                                as f32;
                            2
                        ]
                    })
                    .collect(),
                48000,
                2,
            )
            .unwrap(),
        );
        let controls = Arc::new(Controls::default());
        controls.playing.store(true, Ordering::Relaxed);
        controls.volume.store(1_f32.to_bits(), Ordering::Relaxed);
        controls.matched.store(false, Ordering::Relaxed);
        let (tx, rx) = mpsc::channel();
        let parameters = EqParameters::new(settings, 48000, 0.);
        let mut source = ABSource {
            before: input.clone(),
            after: Some(input.clone()),
            controls,
            tap: Arc::new(PlaybackTap::default()),
            live: Some(LivePath {
                raw: input,
                processor: RealtimeEq::new(48000, 2, parameters),
                updates: rx,
            }),
            cursor: 0,
            blend: 1.,
            gain: 1.,
            channels: 2,
            gain_a: 1.,
            gain_b: 1.,
            cached_frame: None,
            frame_a: [0.; 2],
            frame_b: [0.; 2],
        };
        source.prime(0);
        (source, tx)
    }
    #[test]
    fn audible_live_source_matches_the_shared_export_processor() {
        let mut settings = Settings {
            eq_hold_loudness: false,
            ..Default::default()
        };
        settings.manual_eq[3].freq = 1000.;
        settings.manual_eq[3].gain = 6.;
        let (mut source, _) = source(&settings);
        let input = source.before.clone();
        let expected =
            manual_eq::render_fixed(&input, EqParameters::new(&settings, 48000, 0.), &|| false)
                .unwrap();
        let actual: Vec<_> = source.by_ref().take(input.samples.len()).collect();
        let maximum = actual
            .iter()
            .zip(&expected.samples)
            .map(|(a, b)| (a - b).abs())
            .fold(0_f32, f32::max);
        assert!(maximum < 1e-7, "source/export alignment error {maximum}");
    }
    #[test]
    fn parameter_messages_change_playback_without_rendering_a_file() {
        let settings = Settings {
            eq_hold_loudness: false,
            ..Default::default()
        };
        let (mut source, tx) = source(&settings);
        for _ in 0..9600 {
            source.next();
        }
        let mut louder = settings.clone();
        louder.eq_output_db = 6.;
        tx.send(EqParameters::new(&louder, 48000, 0.)).unwrap();
        for _ in 0..12000 {
            source.next();
        }
        let samples: Vec<_> = source.by_ref().take(9600).collect();
        let rms = (samples.iter().map(|&v| (v as f64).powi(2)).sum::<f64>() / samples.len() as f64)
            .sqrt();
        assert!((rms - 0.01 * crate::meters::lin(6.) / 2_f64.sqrt()).abs() < 0.00001);
        assert!(
            source.after.as_ref().unwrap().samples == source.before.samples,
            "Static file buffer was not changed"
        );
    }
    #[test]
    fn analyzer_tap_is_before_the_listening_volume() {
        let (mut source, _) = source(&Settings::default());
        source.gain = 0.25;
        source
            .controls
            .volume
            .store(0.25_f32.to_bits(), Ordering::Relaxed);
        let played: Vec<_> = source.by_ref().take(12000).collect();
        let tap = source.tap.snapshot(48000, 2).unwrap();
        let offset = played.len() - tap.samples.len();
        let difference = played[offset..]
            .iter()
            .zip(tap.samples)
            .map(|(a, b)| (a - b * 0.25).abs())
            .fold(0_f32, f32::max);
        assert!(difference < 1e-7);
    }
}
