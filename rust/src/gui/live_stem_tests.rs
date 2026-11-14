//! Verify the final audible samples, not just fader state or the pre-comparison tap.
use super::{
    audio_source::ABSource,
    live_stem_path::{LiveStemPath, MixMailbox},
    playback_tap::PlaybackTap,
    player::Controls,
};
use crate::{
    audio::Audio,
    studio::{
        realtime_mix::{MixTargets, RealtimeMix},
        stem_mix::{self, Stem, StemKind, StemSet},
    },
};
use std::sync::{Arc, atomic::Ordering};

const RATE: u32 = 48000;
fn fixture(channels: usize) -> (ABSource, MixMailbox, Arc<StemSet>, stem_mix::MixSettings) {
    let tone = |frequency: f64, amplitude: f64| {
        Arc::new(
            Audio::new(
                (0..RATE * 2)
                    .flat_map(|frame| {
                        (0..channels).map(move |channel| {
                            (amplitude
                                * (std::f64::consts::TAU * frequency * frame as f64 / RATE as f64)
                                    .sin()
                                * if channel == 0 { 1. } else { 0.7 })
                                as f32
                        })
                    })
                    .collect(),
                RATE,
                channels,
            )
            .unwrap(),
        )
    };
    let bank = Arc::new(
        StemSet::new(vec![
            Stem {
                kind: StemKind::Vocals,
                audio: tone(440., 0.04),
            },
            Stem {
                kind: StemKind::Instrumental,
                audio: tone(1100., 0.03),
            },
        ])
        .unwrap(),
    );
    let mut mix = bank.defaults();
    mix.protect_peaks = false;
    let after = Arc::new(stem_mix::render(&bank, &mix, &|| false).unwrap());
    // A contains an entirely different frequency, making accidental A/B layering observable.
    let before = tone(2700., 0.3);
    let controls = Arc::new(Controls::default());
    controls.playing.store(true, Ordering::Relaxed);
    controls.matched.store(false, Ordering::Relaxed);
    controls.volume.store(1f32.to_bits(), Ordering::Relaxed);
    let mailbox = MixMailbox::default();
    let mut source = ABSource {
        before,
        after: Some(after),
        controls,
        tap: Arc::new(PlaybackTap::default()),
        live: None,
        stems: Some(LiveStemPath {
            processor: RealtimeMix::new(bank.clone(), &mix).unwrap(),
            updates: mailbox.clone(),
            active: false,
        }),
        cursor: 0,
        blend: 1.,
        gain: 1.,
        channels,
        gain_a: 1.,
        gain_b: 1.,
        cached_frame: None,
        frame_a: [0.; 2],
        frame_b: [0.; 2],
    };
    source.prime(0);
    (source, mailbox, bank, mix)
}
fn settle(source: &mut ABSource) {
    for _ in 0..(RATE as usize / 25 * source.channels) {
        assert!(source.next().unwrap().is_finite());
    }
}
fn matches_render(source: &mut ABSource, bank: &StemSet, mix: &stem_mix::MixSettings) {
    settle(source);
    let expected = stem_mix::render(bank, mix, &|| false).unwrap();
    let start = source.controls.position.load(Ordering::Relaxed) as usize * source.channels;
    for expected in &expected.samples[start..start + 4096] {
        let actual = source.next().unwrap();
        assert!(
            (actual - expected).abs() < 2e-7,
            "audible {actual}, expected {expected}"
        );
    }
}

#[test]
fn running_stem_mute_solo_gain_and_balance_match_render_without_transport_restart() {
    for channels in [1, 2] {
        let (mut source, mailbox, bank, mut mix) = fixture(channels);
        // Read the prepared preview first, then edit the same running iterator.
        settle(&mut source);
        let old_position = source.controls.position.load(Ordering::Relaxed);
        mix.tracks[0].solo = true;
        mailbox.send(MixTargets::new(&bank, &mix).unwrap());
        matches_render(&mut source, &bank, &mix);
        assert!(source.controls.position.load(Ordering::Relaxed) > old_position);
        mix.tracks[0].gain_db = -9.;
        if channels == 2 {
            mix.tracks[0].balance = 0.7;
        }
        mailbox.send(MixTargets::new(&bank, &mix).unwrap());
        matches_render(&mut source, &bank, &mix);
        // Multiple solos combine; muting a solo always wins.
        mix.tracks[1].solo = true;
        mailbox.send(MixTargets::new(&bank, &mix).unwrap());
        matches_render(&mut source, &bank, &mix);
        mix.tracks[0].mute = true;
        mailbox.send(MixTargets::new(&bank, &mix).unwrap());
        matches_render(&mut source, &bank, &mix);
        mix.tracks[1].mute = true;
        mailbox.send(MixTargets::new(&bank, &mix).unwrap());
        settle(&mut source);
        assert!(
            (0..4096).all(|_| source.next() == Some(0.)),
            "muted mix must not contain Before or previous audio"
        );
    }
}

#[test]
fn comparison_is_exclusive_and_returns_to_the_live_solo_after_short_crossfade() {
    let (mut source, mailbox, bank, mut mix) = fixture(2);
    mix.tracks[0].solo = true;
    mailbox.send(MixTargets::new(&bank, &mix).unwrap());
    source.controls.after.store(false, Ordering::Relaxed);
    settle(&mut source);
    let position = source.controls.position.load(Ordering::Relaxed) as usize * 2;
    let expected = source.before.samples[position..position + 4096].to_vec();
    for value in expected {
        assert!((source.next().unwrap() - value).abs() < 1e-7);
    }
    source.controls.after.store(true, Ordering::Relaxed);
    matches_render(&mut source, &bank, &mix);
}

#[test]
fn latest_control_wins_and_seek_retains_mutes_and_does_not_replay_previous_mix() {
    let (mut source, mailbox, bank, mut mix) = fixture(2);
    for revision in 0..1000 {
        mix.tracks[0].gain_db = -((revision % 60) as f64);
        assert!(mailbox.send(MixTargets::new(&bank, &mix).unwrap()));
    }
    for track in &mut mix.tracks {
        track.mute = true;
    }
    mailbox.send(MixTargets::new(&bank, &mix).unwrap());
    source.controls.seek.store(24000, Ordering::Relaxed);
    settle(&mut source);
    assert!((0..4096).all(|_| source.next() == Some(0.)));
    assert!(mailbox.take().is_none());
    assert!(
        bank.tracks
            .iter()
            .all(|stem| stem.audio.samples.iter().any(|v| *v != 0.))
    );
}

#[test]
fn inactive_workspace_gates_the_final_output_even_if_a_late_job_sets_playing() {
    let (mut source, _, _, _) = fixture(2);
    source.controls.audible.store(false, Ordering::Relaxed);
    source.controls.playing.store(true, Ordering::Relaxed);
    assert!((0..4096).all(|_| source.next() == Some(0.)));
}
