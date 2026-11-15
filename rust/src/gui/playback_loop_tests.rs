use super::*;
use crate::{
    audio::Audio,
    config::Settings,
    dsp::{eq_chain::EqParameters, realtime_eq::RealtimeEq},
    gui::{
        audio_source::{ABSource, LivePath},
        live_stem_path::{LiveStemPath, MixMailbox},
        playback_tap::PlaybackTap,
        player::Player,
    },
    studio::{
        realtime_mix::{MixTargets, RealtimeMix},
        stem_mix::{Stem, StemKind, StemSet},
    },
};
use std::sync::{Arc, mpsc};

const RATE: u32 = 8000;

fn fixture(channels: usize, frames: usize) -> (Player, ABSource) {
    let before = Arc::new(
        Audio::new(
            (0..frames * channels)
                .map(|i| -(i as f32 + 1.) / 1024.)
                .collect(),
            RATE,
            channels,
        )
        .unwrap(),
    );
    let after = Arc::new(
        Audio::new(
            (0..frames * channels)
                .map(|i| (i as f32 + 1.) / 2048.)
                .collect(),
            RATE,
            channels,
        )
        .unwrap(),
    );
    let mut player = Player::for_processing_tests();
    player
        .load(before.clone(), Some(after.clone()), -20., -20., None)
        .unwrap();
    player.controls.playing.store(true, Ordering::Relaxed);
    player.controls.matched.store(false, Ordering::Relaxed);
    player
        .controls
        .volume
        .store(1_f32.to_bits(), Ordering::Relaxed);
    let source = ABSource {
        before,
        after: Some(after),
        controls: player.controls.clone(),
        tap: Arc::new(PlaybackTap::default()),
        live: None,
        stems: None,
        cursor: 0,
        blend: 1.,
        gain: 1.,
        channels,
        gain_a: 1.,
        gain_b: 1.,
        cached_frame: None,
        frame_a: [0.; 2],
        frame_b: [0.; 2],
        loop_range: None,
        control_frames: 0,
    };
    (player, source)
}

fn set_frames(player: &Player, start: usize, end: usize) {
    player
        .set_loop(Some((start as f64 / RATE as f64, end as f64 / RATE as f64)))
        .unwrap();
}

fn assert_pcm(source: &mut ABSource, expected: &[f32]) {
    for (sample, &expected) in expected.iter().enumerate() {
        assert_eq!(source.next(), Some(expected), "sample {sample}");
    }
}

#[test]
fn api_rejects_nonfinite_out_of_bounds_reversed_and_frame_empty_ranges_atomically() {
    let (player, _) = fixture(2, 80);
    set_frames(&player, 3, 7);
    let previous = player.loop_range();
    for range in [
        (f64::NAN, 0.005),
        (0., f64::NAN),
        (f64::NEG_INFINITY, 0.005),
        (0., f64::INFINITY),
        (-0.00001, 0.005),
        (0.005, 0.004),
        (0.005, 0.005),
        (0., 0.010000001),
        (0., 0.4 / RATE as f64),
        (1.1 / RATE as f64, 1.4 / RATE as f64),
    ] {
        assert!(player.set_loop(Some(range)).is_err(), "{range:?}");
        assert_eq!(player.loop_range(), previous);
    }
    let empty = Player::default();
    assert!(empty.set_loop(Some((0., 1.))).is_err());
    empty.set_loop(None).unwrap();
    assert_eq!(empty.loop_range(), None);
}

#[test]
fn api_quantizes_to_nearest_frame_and_accepts_exact_duration_at_common_rates() {
    for rate in [8000, 44100, 48000, 96000, 384000] {
        let mut player = Player::for_processing_tests();
        let audio = Arc::new(Audio::new(vec![0.; 29], rate, 1).unwrap());
        player.load(audio, None, -20., -20., None).unwrap();
        player
            .set_loop(Some((28. / rate as f64, player.duration)))
            .unwrap();
        assert_eq!(
            player.controls.audition_loop.range(),
            Some(FrameRange { start: 28, end: 29 })
        );
        player
            .set_loop(Some((2.2 / rate as f64, 5.8 / rate as f64)))
            .unwrap();
        assert_eq!(
            player.loop_range(),
            Some((2. / rate as f64, 6. / rate as f64))
        );
    }
}

#[test]
fn toggles_preserve_position_playing_and_pending_seek() {
    let (player, _) = fixture(1, 80);
    for playing in [true, false] {
        player.controls.playing.store(playing, Ordering::Relaxed);
        player.controls.position.store(31, Ordering::Relaxed);
        player.controls.seek.store(47, Ordering::Relaxed);
        for range in [Some((0.001, 0.005)), None] {
            player.set_loop(range).unwrap();
            assert_eq!(player.controls.position.load(Ordering::Relaxed), 31);
            assert_eq!(player.controls.seek.load(Ordering::Relaxed), 47);
            assert_eq!(player.controls.playing.load(Ordering::Relaxed), playing);
        }
    }
}

#[test]
fn load_and_unload_clear_previous_song_bounds() {
    let (mut player, _) = fixture(2, 80);
    set_frames(&player, 30, 70);
    let next = Arc::new(Audio::new(vec![0.; 8], RATE, 1).unwrap());
    player.load(next, None, -20., -20., None).unwrap();
    assert_eq!(player.loop_range(), None);
    assert!(player.set_loop(Some((0., 9. / RATE as f64))).is_err());
    set_frames(&player, 0, 8);
    player.unload();
    assert_eq!(player.loop_range(), None);
    assert!(player.set_loop(Some((0., 1. / RATE as f64))).is_err());
}

#[test]
fn selected_frames_repeat_exactly_for_stereo_and_mono_without_a_or_old_after_bleed() {
    for channels in [1, 2] {
        for after in [true, false] {
            let (player, mut source) = fixture(channels, 12);
            source.controls.after.store(after, Ordering::Relaxed);
            source.blend = if after { 1. } else { 0. };
            let audio = if after {
                source.after.as_ref().unwrap()
            } else {
                &source.before
            };
            let selected = audio.samples[2 * channels..5 * channels].to_vec();
            set_frames(&player, 2, 5);
            assert_eq!(source.cursor, 0);
            for _ in 0..100 {
                assert_pcm(&mut source, &selected);
            }
            assert!(source.controls.playing.load(Ordering::Relaxed));
            assert_eq!(source.controls.position.load(Ordering::Relaxed), 5);
        }
    }
}

#[test]
fn full_file_tail_and_last_frame_loop_without_an_eof_gap() {
    for channels in [1, 2] {
        for (start, end) in [(0, 9), (7, 9), (8, 9)] {
            let (player, mut source) = fixture(channels, 9);
            set_frames(&player, start, end);
            let selected = source.after.as_ref().unwrap().samples[start * channels..].to_vec();
            for _ in 0..100 {
                assert_pcm(&mut source, &selected);
                assert!(source.controls.playing.load(Ordering::Relaxed));
            }
        }
    }
}

#[test]
fn loop_changes_between_channels_finish_the_existing_stereo_frame() {
    let (player, mut source) = fixture(2, 12);
    let audio = source.after.as_ref().unwrap().clone();
    assert_eq!(source.next(), Some(audio.samples[0]));
    set_frames(&player, 4, 7);
    assert_eq!(source.next(), Some(audio.samples[1]));
    assert_pcm(&mut source, &audio.samples[8..14]);
    assert_eq!(source.next(), Some(audio.samples[8]));
    set_frames(&player, 8, 10);
    assert_eq!(source.next(), Some(audio.samples[9]));
    assert_pcm(&mut source, &audio.samples[16..20]);
}

#[test]
fn disabling_at_the_exclusive_end_continues_from_the_same_playhead() {
    let (player, mut source) = fixture(2, 12);
    set_frames(&player, 2, 5);
    let audio = source.after.as_ref().unwrap().clone();
    assert_pcm(&mut source, &audio.samples[4..10]);
    player.set_loop(None).unwrap();
    assert_pcm(&mut source, &audio.samples[10..14]);
}

#[test]
fn paused_loop_at_end_stays_stationary_and_resumes_without_splitting_channels() {
    let (player, mut source) = fixture(2, 9);
    set_frames(&player, 7, 9);
    let audio = source.after.as_ref().unwrap().clone();
    assert_pcm(&mut source, &audio.samples[14..]);
    source.controls.playing.store(false, Ordering::Relaxed);
    source.gain = 0.;
    let cursor = source.cursor;
    assert!((0..1000).all(|_| source.next() == Some(0.)));
    assert_eq!(source.cursor, cursor);
    assert_eq!(player.seconds(), 9. / RATE as f64);
    source.controls.playing.store(true, Ordering::Relaxed);
    source.gain = 1.;
    assert_pcm(&mut source, &audio.samples[14..]);
}

#[test]
fn paused_seek_outside_the_loop_is_retained_until_resume() {
    let (player, mut source) = fixture(2, 12);
    set_frames(&player, 4, 7);
    source.controls.playing.store(false, Ordering::Relaxed);
    source.gain = 0.;
    player.seek(10. / RATE as f64);
    assert_eq!(source.next(), Some(0.));
    assert_eq!(source.cursor, 20);
    assert_eq!(player.seconds(), 10. / RATE as f64);
    assert!((0..100).all(|_| source.next() == Some(0.)));
    assert_eq!(source.cursor, 20);
    source.controls.playing.store(true, Ordering::Relaxed);
    source.gain = 1.;
    let expected = source.after.as_ref().unwrap().samples[8..14].to_vec();
    assert_pcm(&mut source, &expected);
}

#[test]
fn playing_seeks_within_and_outside_the_loop_keep_the_existing_gain_ramp() {
    for channels in [1, 2] {
        for seek in [0, 5, 8, 12] {
            let (player, mut source) = fixture(channels, 12);
            set_frames(&player, 4, 8);
            player.seek(seek as f64 / RATE as f64);
            let start = if (4..8).contains(&seek) { seek } else { 4 };
            let audio = source.after.as_ref().unwrap().clone();
            for sample in 0..2000 * channels {
                let frame = 4 + (start - 4 + sample / channels) % 4;
                let actual = source.next().unwrap();
                assert_eq!(
                    actual,
                    audio.samples[frame * channels + sample % channels] * source.gain
                );
                assert!(source.controls.playing.load(Ordering::Relaxed));
            }
            assert_eq!(source.gain, 1.);
        }
    }
}

#[test]
fn audio_uses_previous_snapshot_during_partial_publication_without_waiting_on_writer() {
    let (player, mut source) = fixture(1, 12);
    set_frames(&player, 2, 4);
    let audio = source.after.as_ref().unwrap().clone();
    assert_pcm(&mut source, &audio.samples[2..4]);
    let controls = source.controls.clone();
    let publication = &controls.audition_loop;
    let writer = publication.writer.lock().unwrap();
    publication.version.fetch_add(1, Ordering::SeqCst);
    publication.start.store(7, Ordering::SeqCst);
    assert_eq!(publication.try_range(), None);
    for _ in 0..20 {
        assert_pcm(&mut source, &audio.samples[2..4]);
    }
    publication.end.store(9, Ordering::SeqCst);
    publication.version.fetch_add(1, Ordering::SeqCst);
    drop(writer);
    assert_pcm(&mut source, &audio.samples[7..9]);
}

#[test]
fn concurrent_writers_never_publish_hybrid_frame_bounds() {
    let control = LoopControl::default();
    let ranges = [
        FrameRange { start: 2, end: 5 },
        FrameRange { start: 13, end: 19 },
    ];
    std::thread::scope(|scope| {
        for range in ranges {
            let control = &control;
            scope.spawn(move || {
                for _ in 0..10000 {
                    control.set(Some(range));
                    control.set(None);
                }
            });
        }
        for _ in 0..100000 {
            if let Some(Some(range)) = control.try_range() {
                assert!(ranges.contains(&range), "hybrid snapshot {range:?}");
            }
        }
    });
}

fn attach_eq(source: &mut ABSource) -> mpsc::Sender<EqParameters> {
    let (sender, receiver) = mpsc::channel();
    let parameters = EqParameters::new(&Settings::default(), RATE, 0.);
    source.live = Some(LivePath {
        raw: source.after.as_ref().unwrap().clone(),
        processor: RealtimeEq::new(RATE, source.channels, parameters),
        updates: receiver,
    });
    source.after = Some(Arc::new(
        Audio::new(
            vec![0.7; source.before.samples.len()],
            RATE,
            source.channels,
        )
        .unwrap(),
    ));
    source.prime(0);
    sender
}

#[test]
fn live_eq_delay_is_aligned_at_loop_start_and_eof_for_both_channels() {
    for channels in [1, 2] {
        for (start, end) in [(0, 3), (37, 41), (71, 73), (72, 73)] {
            let (player, mut source) = fixture(channels, 73);
            let expected =
                source.after.as_ref().unwrap().samples[start * channels..end * channels].to_vec();
            let _sender = attach_eq(&mut source);
            assert!(source.live.as_ref().unwrap().processor.delay() > 0);
            set_frames(&player, start, end);
            for _ in 0..20 {
                assert_pcm(&mut source, &expected);
            }
            assert!(source.controls.playing.load(Ordering::Relaxed));
            assert!(!source.controls.failure.load(Ordering::Relaxed));
        }
    }
}

#[test]
fn one_frame_eq_loop_still_receives_live_parameter_messages() {
    let (player, mut source) = fixture(2, 73);
    let raw = source.after.as_ref().unwrap().clone();
    let sender = attach_eq(&mut source);
    set_frames(&player, 37, 38);
    assert_pcm(&mut source, &raw.samples[74..76]);
    let settings = Settings {
        eq_output_db: 6.,
        ..Default::default()
    };
    sender.send(EqParameters::new(&settings, RATE, 0.)).unwrap();
    for _ in 0..130 * 2 {
        source.next();
    }
    for expected in &raw.samples[74..76] {
        assert!((source.next().unwrap() - *expected * crate::meters::lin(6.) as f32).abs() < 1e-7);
    }
}

#[test]
fn delayed_stem_loop_retains_live_mute_and_never_replays_a_or_the_static_preview() {
    for channels in [1, 2] {
        let (player, mut source) = fixture(channels, 73);
        let desired = source.after.as_ref().unwrap().clone();
        let bank = Arc::new(
            StemSet::new(vec![
                Stem {
                    kind: StemKind::Vocals,
                    audio: desired.clone(),
                },
                Stem {
                    kind: StemKind::Instrumental,
                    audio: source.before.clone(),
                },
            ])
            .unwrap(),
        );
        let mut settings = bank.defaults();
        settings.protect_peaks = false;
        settings.tracks[1].mute = true;
        let mailbox = MixMailbox::default();
        source.stems = Some(LiveStemPath {
            processor: RealtimeMix::new(bank.clone(), &settings).unwrap(),
            updates: mailbox.clone(),
            active: false,
        });
        source.prime(0);
        set_frames(&player, 71, 73);
        // The initial prepared B preview stays delay-aligned while no edit exists.
        assert_pcm(&mut source, &desired.samples[71 * channels..]);
        mailbox.send(MixTargets::new(&bank, &settings).unwrap());
        for _ in 0..130 * channels {
            source.next();
        }
        assert!(source.stems.as_ref().unwrap().active);
        for _ in 0..20 {
            assert_pcm(&mut source, &desired.samples[71 * channels..]);
        }
        // A one-frame loop excludes every old absolute-index polling boundary.
        set_frames(&player, 72, 73);
        settings.tracks[0].mute = true;
        mailbox.send(MixTargets::new(&bank, &settings).unwrap());
        for _ in 0..130 * channels {
            source.next();
        }
        assert!((0..200 * channels).all(|_| source.next() == Some(0.)));
        assert!(source.controls.playing.load(Ordering::Relaxed));
        assert!(mailbox.take().is_none());
        assert!(desired.samples.iter().any(|sample| *sample != 0.));
    }
}
