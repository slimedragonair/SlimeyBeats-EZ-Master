//! Synthetic native source output: no output device or physical playback claim.
#[path = "live_arrangement_allocation_tests.rs"]
mod allocations;
use super::*;
use crate::{
    audio::Audio,
    gui::{audio_source::ABSource, player::Player},
    studio::arrangement::{
        Arrangement, AssetLibrary, Clip, EqScope, Lane, RenderOptions, ScopedEq, render,
    },
};
use std::time::{Duration, Instant};

fn tone(rate: u32, frames: usize, channels: usize, frequency: f64, amplitude: f64) -> Arc<Audio> {
    Arc::new(
        Audio::new(
            (0..frames)
                .flat_map(|frame| {
                    (0..channels).map(move |channel| {
                        (amplitude
                            * (std::f64::consts::TAU * frequency * frame as f64 / rate as f64)
                                .sin()
                            * if channel == 0 { 1. } else { 0.7 }) as f32
                    })
                })
                .collect(),
            rate,
            channels,
        )
        .unwrap(),
    )
}
fn clip(id: &str, source: &str, frames: u64) -> Clip {
    Clip {
        id: id.into(),
        name: None,
        source_id: source.into(),
        start_frame: 0,
        source_start_frame: 0,
        end_frame: frames,
        gain_db: 0.,
        fade_in_frames: 0,
        fade_out_frames: 0,
    }
}
fn data(rate: u32, channels: usize) -> (Arrangement, AssetLibrary, RenderOptions, Arc<Audio>) {
    let assets = AssetLibrary::from([
        (
            "voice".into(),
            tone(rate, rate as usize * 2, channels, 440., 0.04),
        ),
        (
            "music".into(),
            tone(rate, rate as usize * 2, channels, 1100., 0.03),
        ),
    ]);
    let mut document = Arrangement::new(rate, channels);
    let mut voice = Lane::new("voice_lane", "Voice");
    voice
        .clips
        .push(clip("voice_clip", "voice", rate as u64 * 2));
    let mut music = Lane::new("music_lane", "Music");
    music
        .clips
        .push(clip("music_clip", "music", rate as u64 * 2));
    document.lanes = vec![voice, music];
    let mut effect = ScopedEq::new("eq", EqScope::Mix {});
    for band in &mut effect.bands {
        band.on = false;
    }
    document.eq.push(effect);
    let options = RenderOptions {
        protect_peaks: false,
        ..Default::default()
    };
    let before = tone(rate, rate as usize, channels, 2700., 0.2);
    (document, assets, options, before)
}
fn load(
    document: &Arrangement,
    assets: &AssetLibrary,
    options: &RenderOptions,
    before: Arc<Audio>,
) -> (Player, ABSource) {
    let prepared = ArrangementPlayer::new(document, assets, options).unwrap();
    let mut player = Player::for_processing_tests();
    player.load_arrangement(before, prepared).unwrap();
    player.controls.playing.store(true, Ordering::Relaxed);
    player.controls.matched.store(false, Ordering::Relaxed);
    player
        .controls
        .volume
        .store(1_f32.to_bits(), Ordering::Relaxed);
    let mut source = player.take_test_arrangement_source().unwrap();
    source.gain = 1.;
    (player, source)
}
fn advance(source: &mut ABSource, frames: usize) {
    for _ in 0..frames * source.channels {
        assert!(source.next().unwrap().is_finite());
    }
}
fn wait_prepared(player: &Player, source: &mut ABSource) {
    let deadline = Instant::now() + Duration::from_secs(5);
    while player.arrangement_preparing() {
        assert!(
            Instant::now() < deadline,
            "Preparation did not settle: {:?}",
            player.arrangement_error()
        );
        advance(source, 64);
        std::thread::sleep(Duration::from_millis(1));
    }
}
fn assert_render(
    source: &mut ABSource,
    document: &Arrangement,
    assets: &AssetLibrary,
    options: &RenderOptions,
) {
    let expected = render(document, assets, options, &|| false).unwrap();
    let start = source.controls.position.load(Ordering::Relaxed) as usize * source.channels;
    for &value in &expected.samples[start..start + 256 * source.channels] {
        let actual = source.next().unwrap();
        assert!((actual - value).abs() < 1e-7, "{actual} vs {value}");
    }
}

#[test]
fn native_live_pcm_matches_render_and_outlives_shorter_before_source() {
    for (rate, channels) in [(8000, 1), (48_000, 2)] {
        let (document, assets, options, before) = data(rate, channels);
        let (player, mut source) = load(&document, &assets, &options, before);
        assert_eq!(player.duration, 2.);
        assert!(player.has_live_arrangement());
        assert_render(&mut source, &document, &assets, &options);
        advance(&mut source, rate as usize);
        assert!(player.controls.playing.load(Ordering::Relaxed));
        assert_render(&mut source, &document, &assets, &options);
        assert!(!player.controls.failure.load(Ordering::Relaxed));
    }
}

#[test]
fn native_live_faders_mute_solo_and_eq_change_samples_without_geometry_or_file_render() {
    let (mut document, assets, options, before) = data(8000, 2);
    let (player, mut source) = load(&document, &assets, &options, before);
    let original = assets["voice"].samples.clone();
    assert!(
        player
            .set_arrangement_lane_controls("voice_lane", 0., 0., true, true)
            .unwrap()
    );
    advance(&mut source, 800);
    assert!(!player.arrangement_preparing());
    assert!(source.by_ref().take(200).all(|value| value == 0.));
    player
        .set_arrangement_lane_controls("voice_lane", 0., 0., false, true)
        .unwrap();
    player.set_arrangement_clip_gain("voice_clip", -6.).unwrap();
    document.lanes[0].solo = true;
    document.lanes[0].clips[0].gain_db = -6.;
    advance(&mut source, 800);
    assert_render(&mut source, &document, &assets, &options);
    document.eq[0].output_gain_db = -6.;
    player.set_arrangement_eq(&document.eq[0]).unwrap();
    advance(&mut source, 800);
    assert_render(&mut source, &document, &assets, &options);
    assert_eq!(original, assets["voice"].samples);
    assert!(!player.arrangement_preparing());
}

#[test]
fn native_before_and_arrangement_b_are_exclusive_and_stop_is_silent() {
    let (document, assets, options, before) = data(8000, 2);
    let (player, mut source) = load(&document, &assets, &options, before.clone());
    advance(&mut source, 100);
    player.controls.after.store(false, Ordering::Relaxed);
    advance(&mut source, 400);
    let start = player.controls.position.load(Ordering::Relaxed) as usize * 2;
    for &value in &before.samples[start..start + 400] {
        assert!((source.next().unwrap() - value).abs() < 1e-7);
    }
    player.controls.after.store(true, Ordering::Relaxed);
    advance(&mut source, 400);
    assert_render(&mut source, &document, &assets, &options);
    player.stop();
    assert!(source.by_ref().take(500).all(|value| value == 0.));
    assert_eq!(player.controls.position.load(Ordering::Relaxed), 0);
}

#[test]
fn geometry_prepares_while_old_audio_plays_and_obsolete_epoch_never_commits() {
    let (mut document, assets, options, before) = data(8000, 1);
    let (mut player, mut source) = load(&document, &assets, &options, before);
    let original = document.clone();
    let shared = source.arrangement.as_ref().unwrap().updates.0.clone();
    shared.hold_preparation.store(true, Ordering::Release);
    document.lanes[0].clips[0].start_frame = 100;
    document.lanes[0].clips[0].end_frame += 100;
    player
        .set_arrangement(&document, &assets, &options)
        .unwrap();
    let deadline = Instant::now() + Duration::from_secs(3);
    while !shared.preparation_entered.load(Ordering::Acquire) {
        assert!(Instant::now() < deadline);
        std::thread::sleep(Duration::from_millis(1));
    }
    assert_render(&mut source, &original, &assets, &options);
    document.lanes[0].clips[0].start_frame = 300;
    document.lanes[0].clips[0].end_frame += 200;
    player
        .set_arrangement(&document, &assets, &options)
        .unwrap();
    shared.hold_preparation.store(false, Ordering::Release);
    wait_prepared(&player, &mut source);
    advance(&mut source, 400);
    assert_eq!(
        source
            .arrangement
            .as_ref()
            .unwrap()
            .processor
            .arrangement()
            .clip("voice_clip")
            .unwrap()
            .start_frame,
        300
    );
    assert_render(&mut source, &document, &assets, &options);
    assert!(!player.controls.failure.load(Ordering::Relaxed));
}

#[test]
fn callback_catchup_is_strictly_bounded_and_converges_while_playing() {
    let (document, assets, options, before) = data(8000, 1);
    let (player, mut source) = load(&document, &assets, &options, before);
    let path = source.arrangement.as_mut().unwrap();
    path.processor.seek(1000, &|| false).unwrap();
    let mut next = document.clone();
    next.lanes[0].gain_db = -6.;
    let pending = ArrangementPlayer::new(&next, &assets, &options).unwrap();
    path.pending = Some(Prepared {
        epoch: 0,
        seek: false,
        player: Box::new(pending),
    });
    path.maintenance(1000);
    assert_eq!(path.pending.as_ref().unwrap().player.position(), 4);
    for index in 1000..1400 {
        path.maintenance(index);
        path.next(index);
    }
    assert!(path.pending.is_none());
    assert_eq!(path.processor.arrangement().lanes[0].gain_db, -6.);
    assert!(!player.controls.failure.load(Ordering::Relaxed));
}

#[test]
fn late_project_seek_is_prepared_off_callback_and_does_not_clamp_to_before_length() {
    let (mut document, assets, options, before) = data(8000, 1);
    let start = 8000 * 300;
    document.lanes[0].clips[0].start_frame = start;
    document.lanes[0].clips[0].end_frame += start;
    let (player, mut source) = load(&document, &assets, &options, before);
    let shared = source.arrangement.as_ref().unwrap().updates.0.clone();
    shared.hold_preparation.store(true, Ordering::Release);
    let requested = start + 2000;
    player.seek(requested as f64 / 8000.);
    assert_eq!(source.next().unwrap(), 0.);
    assert_eq!(
        source.arrangement.as_ref().unwrap().processor.position(),
        0,
        "Callback must not replay project prefix"
    );
    assert_eq!(source.cursor, requested as usize);
    shared.hold_preparation.store(false, Ordering::Release);
    wait_prepared(&player, &mut source);
    advance(&mut source, 800);
    let source_frame = (player.controls.position.load(Ordering::Relaxed) - start) as usize;
    for &value in &assets["voice"].samples[source_frame..source_frame + 200] {
        assert!((source.next().unwrap() - value).abs() < 1e-7);
    }
    assert!(!player.controls.failure.load(Ordering::Relaxed));
}

#[test]
fn failed_background_source_admission_reports_error_and_preserves_previous_playback() {
    let (document, mut assets, options, before) = data(8000, 1);
    let (mut player, mut source) = load(&document, &assets, &options, before);
    let old_assets = assets.clone();
    assets.insert(
        "voice".into(),
        Arc::new(Audio {
            samples: vec![f32::NAN; 16_000],
            sample_rate: 8000,
            channels: 1,
        }),
    );
    player
        .set_arrangement(&document, &assets, &options)
        .unwrap();
    wait_prepared(&player, &mut source);
    assert!(player.arrangement_error().unwrap().contains("nonfinite"));
    assert_render(&mut source, &document, &old_assets, &options);
    assert!(!player.controls.failure.load(Ordering::Relaxed));
}

#[test]
fn paused_seek_adopts_prepared_state_without_resuming_transport() {
    let (document, assets, options, before) = data(8000, 2);
    let (player, mut source) = load(&document, &assets, &options, before);
    advance(&mut source, 100);
    player.controls.playing.store(false, Ordering::Relaxed);
    player.seek(0.5);
    wait_prepared_after_request(&player, &mut source);
    assert!(!player.controls.playing.load(Ordering::Relaxed));
    assert_eq!(player.controls.position.load(Ordering::Relaxed), 4000);
    assert!(source.by_ref().take(500).all(|value| value == 0.));
    player.toggle();
    advance(&mut source, 800);
    assert_render(&mut source, &document, &assets, &options);
}

fn wait_prepared_after_request(player: &Player, source: &mut ABSource) {
    source.next();
    wait_prepared(player, source);
}

#[test]
fn arrangement_loop_wraps_keep_controls_polling_and_stop_does_not_resume() {
    let (document, assets, options, before) = data(8000, 1);
    let (player, mut source) = load(&document, &assets, &options, before);
    player.set_loop(Some((0.2, 0.28))).unwrap();
    wait_prepared_after_request(&player, &mut source);
    player
        .set_arrangement_lane_controls("voice_lane", 0., 0., true, true)
        .unwrap();
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        advance(&mut source, 64);
        if source
            .arrangement
            .as_ref()
            .unwrap()
            .processor
            .arrangement()
            .lanes[0]
            .mute
        {
            break;
        }
        assert!(Instant::now() < deadline);
        std::thread::sleep(Duration::from_millis(1));
    }
    for _ in 0..20 {
        advance(&mut source, 64);
        std::thread::sleep(Duration::from_millis(1));
    }
    assert!(source.by_ref().take(256).all(|value| value == 0.));
    player.stop();
    assert!(source.by_ref().take(256).all(|value| value == 0.));
    assert!(!player.controls.playing.load(Ordering::Relaxed));
    assert_eq!(player.controls.position.load(Ordering::Relaxed), 0);
}

#[test]
fn shorter_arrangement_stops_before_long_before_source_without_comparison_tail() {
    let (mut document, assets, options, before) = data(8000, 1);
    for lane in &mut document.lanes {
        lane.clips[0].end_frame = 400;
    }
    let (player, mut source) = load(&document, &assets, &options, before);
    player.controls.after.store(false, Ordering::Relaxed);
    source.blend = 0.;
    advance(&mut source, 400);
    assert!(source.by_ref().take(1000).all(|value| value == 0.));
    assert!(!player.controls.playing.load(Ordering::Relaxed));
    assert_eq!(player.controls.position.load(Ordering::Relaxed), 400);
}
