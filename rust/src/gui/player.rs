use super::{
    arrangement_mailbox::ArrangementMailbox,
    audio_source::{ABSource, LivePath},
    live_arrangement_path::LiveArrangementPath,
    live_stem_path::{LiveStemPath, MixMailbox},
    playback_loop::{FrameRange, LoopControl},
    playback_tap::PlaybackTap,
};
#[path = "player_arrangement.rs"]
mod arrangement;
use crate::{
    audio::Audio,
    dsp::{eq_chain::EqParameters, realtime_eq::RealtimeEq},
    meters::lin,
    studio::{
        realtime_mix::{MixTargets, RealtimeMix},
        stem_mix::{MixSettings, StemSet},
    },
};
use anyhow::Result;
use rodio::{OutputStream, OutputStreamBuilder, Sink};
use std::sync::{
    Arc,
    atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering},
    mpsc::{self, Sender},
};

pub struct Controls {
    pub audible: AtomicBool,
    pub playing: AtomicBool,
    pub after: AtomicBool,
    pub matched: AtomicBool,
    pub volume: AtomicU32,
    pub position: AtomicU64,
    pub seek: AtomicU64,
    pub match_a: AtomicU32,
    pub match_b: AtomicU32,
    pub failure: AtomicBool,
    pub(super) audition_loop: LoopControl,
}
impl Default for Controls {
    fn default() -> Self {
        Self {
            audible: AtomicBool::new(true),
            playing: AtomicBool::new(false),
            after: AtomicBool::new(true),
            matched: AtomicBool::new(true),
            volume: AtomicU32::new(0.5_f32.to_bits()),
            position: AtomicU64::new(0),
            seek: AtomicU64::new(u64::MAX),
            match_a: AtomicU32::new(1_f32.to_bits()),
            match_b: AtomicU32::new(1_f32.to_bits()),
            failure: AtomicBool::new(false),
            audition_loop: LoopControl::default(),
        }
    }
}
#[derive(Default)]
pub struct Player {
    stream: Option<OutputStream>,
    sink: Option<Sink>,
    pub controls: Arc<Controls>,
    pub duration: f64,
    pub rate: u32,
    frames: usize,
    pub tap: Arc<PlaybackTap>,
    live_sender: Option<Sender<EqParameters>>,
    mix_sender: Option<MixMailbox>,
    arrangement_sender: Option<ArrangementMailbox>,
    #[cfg(test)]
    test_arrangement_source: Option<ABSource>,
    #[cfg(test)]
    without_output_device: bool,
    #[cfg(test)]
    test_live_receiver: Option<mpsc::Receiver<EqParameters>>,
}
enum RealtimeInput {
    Eq(Arc<Audio>, Box<EqParameters>),
    Stems(Arc<StemSet>, MixSettings),
    Arrangement(Box<crate::studio::arrangement::ArrangementPlayer>),
}
impl Player {
    #[cfg(test)]
    pub(crate) fn for_processing_tests() -> Self {
        Self {
            without_output_device: true,
            ..Self::default()
        }
    }
    #[cfg(test)]
    pub(super) fn restored_mix_mailbox(&self) -> Option<MixMailbox> {
        self.mix_sender.clone()
    }
    pub fn unload(&mut self) {
        if let Some(mailbox) = self.arrangement_sender.take() {
            mailbox.close();
        }
        #[cfg(test)]
        {
            self.test_arrangement_source = None;
        }
        self.controls.playing.store(false, Ordering::Relaxed);
        self.controls.audition_loop.set(None);
        if let Some(sink) = self.sink.take() {
            sink.stop();
        }
        self.live_sender = None;
        self.mix_sender = None;
        #[cfg(test)]
        {
            self.test_live_receiver = None;
        }
        self.duration = 0.;
        self.rate = 0;
        self.frames = 0;
        self.controls.position.store(0, Ordering::Relaxed);
        self.tap.clear();
    }
    pub fn load(
        &mut self,
        before: Arc<Audio>,
        after: Option<Arc<Audio>>,
        before_lufs: f64,
        after_lufs: f64,
        live_input: Option<(Arc<Audio>, EqParameters)>,
    ) -> Result<()> {
        self.load_inner(
            before,
            after,
            before_lufs,
            after_lufs,
            live_input.map(|(audio, parameters)| RealtimeInput::Eq(audio, Box::new(parameters))),
        )
    }
    pub fn load_stems(
        &mut self,
        before: Arc<Audio>,
        after: Arc<Audio>,
        levels: (f64, f64),
        bank: Arc<StemSet>,
        settings: &MixSettings,
    ) -> Result<()> {
        settings.validate(&bank)?;
        anyhow::ensure!(
            bank.tracks[0].audio.sample_rate == before.sample_rate
                && bank.tracks[0].audio.channels == before.channels
                && bank.tracks[0].audio.frames() == before.frames(),
            "Live mix and comparison must be aligned"
        );
        self.load_inner(
            before,
            Some(after),
            levels.0,
            levels.1,
            Some(RealtimeInput::Stems(bank, settings.clone())),
        )
    }
    fn load_inner(
        &mut self,
        before: Arc<Audio>,
        after: Option<Arc<Audio>>,
        before_lufs: f64,
        after_lufs: f64,
        live_input: Option<RealtimeInput>,
    ) -> Result<()> {
        let frames = match &live_input {
            Some(RealtimeInput::Arrangement(player)) => usize::try_from(player.duration_frames())?,
            _ => before.frames(),
        };
        if let Some(mailbox) = self.arrangement_sender.take() {
            mailbox.close();
        }
        #[cfg(test)]
        {
            self.test_arrangement_source = None;
        }
        self.controls.playing.store(false, Ordering::Relaxed);
        self.controls.audition_loop.set(None);
        self.controls.position.store(0, Ordering::Relaxed);
        self.controls.seek.store(u64::MAX, Ordering::Relaxed);
        #[cfg(test)]
        if self.without_output_device {
            // Controller/DSP tests own and verify their rendered Audio buffers.
            // They must not open a real native device or claim audible playback.
            self.duration = frames as f64 / before.sample_rate as f64;
            self.rate = before.sample_rate;
            self.frames = frames;
            self.live_sender = None;
            self.mix_sender = None;
            self.test_live_receiver = None;
            if matches!(live_input, Some(RealtimeInput::Eq(..))) {
                // Keep a real control channel for UI tests; no audio stream is opened.
                let (sender, receiver) = mpsc::channel();
                self.live_sender = Some(sender);
                self.test_live_receiver = Some(receiver);
            }
            if matches!(live_input, Some(RealtimeInput::Stems(..))) {
                self.mix_sender = Some(MixMailbox::default());
            }
            if let Some(RealtimeInput::Arrangement(prepared)) = live_input {
                let path = LiveArrangementPath::new(*prepared, self.controls.clone())?;
                self.arrangement_sender = Some(path.updates.clone());
                self.test_arrangement_source = Some(ABSource {
                    before,
                    after,
                    controls: self.controls.clone(),
                    tap: self.tap.clone(),
                    live: None,
                    stems: None,
                    arrangement: Some(path),
                    cursor: 0,
                    blend: 1.,
                    gain: 0.,
                    channels: self.arrangement_sender.as_ref().unwrap().0.channels,
                    gain_a: 1.,
                    gain_b: 1.,
                    cached_frame: None,
                    frame_a: [0.; 2],
                    frame_b: [0.; 2],
                    loop_range: None,
                    control_frames: 0,
                });
            }
            return Ok(());
        }
        if self.stream.is_none() {
            self.stream = Some(OutputStreamBuilder::open_default_stream()?);
        }
        if let Some(sink) = self.sink.take() {
            sink.stop();
        }
        self.duration = frames as f64 / before.sample_rate as f64;
        self.rate = before.sample_rate;
        self.frames = frames;
        let sink = Sink::connect_new(self.stream.as_ref().unwrap().mixer());
        self.controls.failure.store(false, Ordering::Relaxed);
        self.tap.clear();
        self.live_sender = None;
        self.mix_sender = None;
        let mut live = None;
        let mut stems = None;
        let mut arrangement = None;
        if let Some(input) = live_input {
            match input {
                RealtimeInput::Eq(raw, parameters) => {
                    let (tx, rx) = mpsc::channel();
                    self.live_sender = Some(tx);
                    live = Some(LivePath {
                        processor: RealtimeEq::new(raw.sample_rate, raw.channels, *parameters),
                        raw,
                        updates: rx,
                    });
                }
                RealtimeInput::Stems(bank, settings) => {
                    let mailbox = MixMailbox::default();
                    stems = Some(LiveStemPath {
                        processor: RealtimeMix::new(bank, &settings)?,
                        updates: mailbox.clone(),
                        active: false,
                    });
                    self.mix_sender = Some(mailbox);
                }
                RealtimeInput::Arrangement(prepared) => {
                    let path = LiveArrangementPath::new(*prepared, self.controls.clone())?;
                    self.arrangement_sender = Some(path.updates.clone());
                    arrangement = Some(path);
                }
            }
        }
        let common = before_lufs.min(after_lufs);
        let a = lin((common - before_lufs).min(0.)) as f32;
        let b = lin((common - after_lufs).min(0.)) as f32;
        let channels = before.channels;
        self.controls.match_a.store(a.to_bits(), Ordering::Relaxed);
        self.controls.match_b.store(b.to_bits(), Ordering::Relaxed);
        let mut source = ABSource {
            before,
            after,
            controls: self.controls.clone(),
            cursor: 0,
            blend: 1.,
            gain: 0.,
            channels,
            gain_a: a,
            gain_b: b,
            live,
            stems,
            arrangement,
            tap: self.tap.clone(),
            cached_frame: None,
            frame_a: [0.; 2],
            frame_b: [0.; 2],
            loop_range: None,
            control_frames: 0,
        };
        source.prime(0);
        sink.append(source);
        self.sink = Some(sink);
        Ok(())
    }
    pub fn set_eq(&self, parameters: EqParameters) -> bool {
        self.live_sender
            .as_ref()
            .is_some_and(|sender| sender.send(parameters).is_ok())
    }
    pub fn set_match(&self, before: f64, after: f64) {
        let common = before.min(after);
        self.controls.match_a.store(
            (lin((common - before).min(0.)) as f32).to_bits(),
            Ordering::Relaxed,
        );
        self.controls.match_b.store(
            (lin((common - after).min(0.)) as f32).to_bits(),
            Ordering::Relaxed,
        );
    }
    pub fn set_mix(&self, bank: &StemSet, settings: &MixSettings) -> Result<bool> {
        let targets = MixTargets::new(bank, settings)?;
        Ok(self
            .mix_sender
            .as_ref()
            .is_some_and(|sender| sender.send(targets)))
    }
    pub fn has_live_mix(&self) -> bool {
        self.mix_sender.is_some()
    }
    /// Set an end-exclusive audition loop, rounding seconds to the nearest frame.
    /// Invalid or sub-frame-empty ranges leave the existing loop unchanged.
    /// Toggling never changes position, a pending seek, or playing state. While
    /// playing, a position/seek outside the range enters at its start on the next
    /// complete frame boundary; while paused it stays put until playback resumes.
    pub fn set_loop(&self, range: Option<(f64, f64)>) -> Result<()> {
        let frames = range
            .map(|(start, end)| FrameRange::from_seconds(start, end, self.rate, self.frames))
            .transpose()?;
        self.controls.audition_loop.set(frames);
        Ok(())
    }
    /// Effective, frame-quantized loop endpoints in seconds, with an exclusive end.
    pub fn loop_range(&self) -> Option<(f64, f64)> {
        self.controls.audition_loop.range().map(|range| {
            let rate = self.rate.max(1) as f64;
            (range.start as f64 / rate, range.end as f64 / rate)
        })
    }
    pub fn toggle(&self) {
        let now = self.controls.playing.load(Ordering::Relaxed);
        self.controls.playing.store(!now, Ordering::Relaxed);
    }
    pub fn set_audible(&self, audible: bool) {
        let previous = self.controls.audible.swap(audible, Ordering::Relaxed);
        if !audible || !previous {
            self.controls.playing.store(false, Ordering::Relaxed);
        }
    }
    pub fn stop(&self) {
        self.controls.playing.store(false, Ordering::Relaxed);
        self.controls.seek.store(0, Ordering::Relaxed);
    }
    pub fn seek(&self, seconds: f64) {
        self.controls.position.store(
            (seconds * self.rate as f64).max(0.) as u64,
            Ordering::Relaxed,
        );
        self.controls.seek.store(
            (seconds * self.rate as f64).max(0.) as u64,
            Ordering::Relaxed,
        );
    }
    pub fn seconds(&self) -> f64 {
        self.controls.position.load(Ordering::Relaxed) as f64 / self.rate.max(1) as f64
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn processing_fixture_prepares_metadata_without_opening_a_native_device() {
        assert!(!Player::default().without_output_device);
        let mut player = Player::for_processing_tests();
        let audio = Arc::new(Audio::new(vec![0.05; 16000], 16000, 1).unwrap());
        player
            .load(audio.clone(), Some(audio), -20., -20., None)
            .unwrap();
        assert!(player.stream.is_none() && player.sink.is_none());
        assert_eq!(player.duration, 1.);
        assert_eq!(player.rate, 16000);
    }
    #[test]
    fn processing_fixture_captures_live_control_messages_without_claiming_audio_output() {
        let mut player = Player::for_processing_tests();
        let audio = Arc::new(Audio::new(vec![0.05; 16000], 16000, 1).unwrap());
        let parameters = EqParameters::new(&crate::config::Settings::default(), 16000, 0.);
        player
            .load(
                audio.clone(),
                Some(audio.clone()),
                -20.,
                -20.,
                Some((audio.clone(), parameters)),
            )
            .unwrap();
        let mut changed = parameters;
        changed.ceiling_dbtp = -3.;
        assert!(player.set_eq(changed));
        assert_eq!(
            player
                .test_live_receiver
                .as_ref()
                .unwrap()
                .try_recv()
                .unwrap()
                .ceiling_dbtp,
            -3.
        );
        assert!(player.stream.is_none() && player.sink.is_none());
        player
            .load(audio.clone(), Some(audio), -20., -20., None)
            .unwrap();
        assert!(!player.set_eq(changed));
        assert!(player.test_live_receiver.is_none());
    }
    fn source() -> ABSource {
        let controls = Arc::new(Controls::default());
        controls.playing.store(true, Ordering::Relaxed);
        controls.volume.store(1_f32.to_bits(), Ordering::Relaxed);
        controls.match_b.store(0.5_f32.to_bits(), Ordering::Relaxed);
        ABSource {
            before: Arc::new(Audio::new(vec![0.2; 96000], 48000, 2).unwrap()),
            after: Some(Arc::new(Audio::new(vec![0.4; 96000], 48000, 2).unwrap())),
            controls,
            cursor: 0,
            blend: 0.,
            gain: 1.,
            channels: 2,
            gain_a: 1.,
            gain_b: 0.5,
            live: None,
            stems: None,
            arrangement: None,
            tap: Arc::new(PlaybackTap::default()),
            cached_frame: None,
            frame_a: [0.; 2],
            frame_b: [0.; 2],
            loop_range: None,
            control_frames: 0,
        }
    }
    #[test]
    fn loudness_matched_switch_has_no_level_jump() {
        let mut s = source();
        for _ in 0..10000 {
            assert!((s.next().unwrap() - 0.2).abs() < 1e-6);
        }
    }
    #[test]
    fn paused_seek_updates_position_and_resumes_at_correct_frame() {
        let mut s = source();
        s.controls.playing.store(false, Ordering::Relaxed);
        s.gain = 0.;
        s.controls.seek.store(12000, Ordering::Relaxed);
        assert_eq!(s.next(), Some(0.));
        assert_eq!(s.controls.position.load(Ordering::Relaxed), 12000);
        assert_eq!(s.cursor, 24000);
        s.controls.playing.store(true, Ordering::Relaxed);
        assert!(s.next().unwrap() > 0.);
    }
    #[test]
    fn pause_always_finishes_stereo_frame_before_stopping() {
        let mut s = source();
        s.cursor = 5;
        s.gain = 0.;
        s.controls.playing.store(false, Ordering::Relaxed);
        s.next();
        assert_eq!(s.cursor % 2, 0);
        s.controls.seek.store(0, Ordering::Relaxed);
        s.next();
        assert_eq!(s.cursor, 0);
    }
}
