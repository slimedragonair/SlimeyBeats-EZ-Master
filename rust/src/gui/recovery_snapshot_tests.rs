use super::{
    recovery_capture::capture, recovery_key, recovery_restore, test_fixture, theme,
    ui_test_input::settled,
};
use crate::{
    audio::Audio,
    gui::studio::state::{LoadedSource, Revision, Target},
    studio::stem_mix::{self, Stem, StemKind, StemSet},
};
use eframe::egui;
use std::{
    sync::{Arc, atomic::Ordering},
    time::{Duration, Instant},
};

fn cloned_assets(assets: &[Arc<Audio>]) -> Vec<Arc<Audio>> {
    assets.iter().map(|a| Arc::new((**a).clone())).collect()
}
fn studio(app: &mut super::app::MasterCraftApp) {
    let make = |step: f32| {
        Arc::new(
            Audio::new(
                (0..16000).map(|i| (i as f32 * step).sin() * 0.04).collect(),
                16000,
                1,
            )
            .unwrap(),
        )
    };
    let bank = Arc::new(
        StemSet::new(vec![
            Stem {
                kind: StemKind::Vocals,
                audio: make(0.13),
            },
            Stem {
                kind: StemKind::Drums,
                audio: make(0.27),
            },
        ])
        .unwrap(),
    );
    let mix = bank.defaults();
    let sum = Arc::new(stem_mix::render(&bank, &mix, &|| false).unwrap());
    app.studio.source = Some(LoadedSource {
        path: "private original.wav".into(),
        audio: sum.clone(),
        loudness: -20.,
    });
    app.studio.accept(
        Revision {
            before: sum.clone(),
            audio: sum,
            label: "Imported stems".into(),
            notes: vec![],
            before_lufs: -20.,
            after_lufs: -20.,
            bank: Some(bank),
            mix,
        },
        false,
    );
}
#[test]
fn capture_restores_master_audio_and_pending_eq_controls_without_source_files() {
    let ctx = egui::Context::default();
    theme::apply(&ctx);
    let mut app = test_fixture::mastered(&ctx);
    settled(&ctx, &mut app, egui::vec2(1100., 720.));
    app.settings.manual_eq[3].gain = 3.12749;
    app.settings.eq_output_db = -0.125;
    app.volume = 0.27;
    let original = app.current().unwrap().1.before.samples.clone();
    let key = recovery_key::key(&app).unwrap();
    let saved = capture(&app).unwrap();
    assert_eq!(key, recovery_key::key(&app).unwrap());
    let loaded = recovery_restore::prepare(saved.payload, cloned_assets(&saved.audio)).unwrap();
    let mut target = test_fixture::app(&ctx);
    loaded.adopt(&mut target);
    assert_eq!(target.settings.manual_eq[3].gain, 3.12749);
    assert_eq!(target.settings.eq_output_db, -0.125);
    assert_eq!(target.volume, 0.27);
    assert_eq!(target.current().unwrap().1.before.samples, original);
    assert!(!target.player.controls.playing.load(Ordering::Relaxed));
    assert!(!target.tracks[0].path.exists());
    // Re-mastering uses the recovered immutable source, not the now-missing original pathname.
    target.start(false, &ctx);
    let until = Instant::now() + Duration::from_secs(20);
    while target.busy && Instant::now() < until {
        target.poll();
        std::thread::sleep(Duration::from_millis(5));
    }
    assert!(!target.busy && target.error.is_none(), "{:?}", target.error);
    assert!(target.current().unwrap().1.report.is_some());
}
#[test]
fn pending_stem_mute_solo_and_voice_controls_survive_without_committing_the_mix() {
    let ctx = egui::Context::default();
    let mut app = test_fixture::app(&ctx);
    studio(&mut app);
    app.studio.mix.tracks[0].gain_db = -5.75;
    app.studio.mix.tracks[0].mute = true;
    app.studio.mix.tracks[1].solo = true;
    app.studio.mix_dirty = true;
    app.studio.monitoring_mix = true;
    app.studio.voice.deess = 0.37;
    let previous = app.studio.current.as_ref().unwrap().audio.samples.clone();
    let saved = capture(&app).unwrap();
    let loaded = recovery_restore::prepare(saved.payload, cloned_assets(&saved.audio)).unwrap();
    let mut target = test_fixture::app(&ctx);
    loaded.adopt(&mut target);
    assert!(target.studio.mix_dirty);
    assert!(target.studio.mix.tracks[0].mute);
    assert!(target.studio.mix.tracks[1].solo);
    assert_eq!(target.studio.mix.tracks[0].gain_db, -5.75);
    assert_eq!(target.studio.voice.deess, 0.37);
    assert_eq!(
        target.studio.current.as_ref().unwrap().audio.samples,
        previous
    );
    assert!(target.studio.snapshot().is_err());
    assert!(
        !target
            .studio
            .player
            .controls
            .playing
            .load(Ordering::Relaxed)
    );
}
#[test]
fn direct_stem_lineage_survives_and_can_apply_after_restore() {
    let ctx = egui::Context::default();
    let mut app = test_fixture::app(&ctx);
    studio(&mut app);
    let bank = app.studio.bank.clone().unwrap();
    let before = bank.track(StemKind::Vocals).unwrap().audio.clone();
    let mut changed = (*before).clone();
    changed.gain(-3.);
    app.studio.accept(
        Revision {
            before,
            audio: Arc::new(changed),
            label: "Direct voice edit".into(),
            notes: vec![],
            before_lufs: -20.,
            after_lufs: -23.,
            bank: Some(bank.clone()),
            mix: bank.defaults(),
        },
        false,
    );
    app.studio.target = Target::Stem(StemKind::Vocals);
    let saved = capture(&app).unwrap();
    let loaded = recovery_restore::prepare(saved.payload, cloned_assets(&saved.audio)).unwrap();
    let mut target = test_fixture::app(&ctx);
    loaded.adopt(&mut target);
    let bank = target.studio.bank.clone().unwrap();
    assert!(Arc::ptr_eq(
        &target.studio.current.as_ref().unwrap().before,
        &bank.track(StemKind::Vocals).unwrap().audio
    ));
    target.studio.apply_preview_to_stem(&ctx);
    let until = Instant::now() + Duration::from_secs(20);
    while target.studio.busy() && Instant::now() < until {
        target.studio.poll(&ctx);
        std::thread::sleep(Duration::from_millis(5));
    }
    assert!(target.studio.error.is_none(), "{:?}", target.studio.error);
    assert_eq!(
        target
            .studio
            .bank
            .as_ref()
            .unwrap()
            .track(StemKind::Drums)
            .unwrap()
            .audio
            .samples,
        bank.track(StemKind::Drums).unwrap().audio.samples
    );
    assert_ne!(
        target
            .studio
            .bank
            .as_ref()
            .unwrap()
            .track(StemKind::Vocals)
            .unwrap()
            .audio
            .samples,
        bank.track(StemKind::Vocals).unwrap().audio.samples
    );
}
#[test]
fn invalid_payload_is_rejected_before_the_current_session_is_touched() {
    let ctx = egui::Context::default();
    let mut app = test_fixture::app(&ctx);
    studio(&mut app);
    let original = app.studio.source.as_ref().unwrap().audio.clone();
    let mut saved = capture(&app).unwrap();
    saved.payload["studio"]["source"]["audio"] = 9999.into();
    assert!(recovery_restore::prepare(saved.payload, saved.audio).is_err());
    assert!(Arc::ptr_eq(
        &original,
        &app.studio.source.as_ref().unwrap().audio
    ));
}

#[test]
fn unfinished_export_format_choice_is_preserved_without_losing_other_edits() {
    let ctx = egui::Context::default();
    let mut app = test_fixture::app(&ctx);
    app.settings.formats.clear();
    app.settings.manual_eq[3].gain = 1.23456789012345;
    let saved = capture(&app).unwrap();
    let loaded = recovery_restore::prepare(saved.payload, saved.audio).unwrap();
    let mut target = test_fixture::app(&ctx);
    loaded.adopt(&mut target);
    assert!(target.settings.formats.is_empty());
    assert_eq!(
        target.settings.manual_eq[3].gain.to_bits(),
        app.settings.manual_eq[3].gain.to_bits()
    );
    assert!(
        target.settings.validate().is_err(),
        "Restore must not silently choose an export format"
    );
}

#[test]
fn restored_pending_mute_and_solo_change_actual_output_pcm_before_any_new_gesture() {
    use super::{live_stem_path::LiveStemPath, live_stem_tests};
    use crate::studio::realtime_mix::RealtimeMix;
    let ctx = egui::Context::default();
    let mut app = test_fixture::app(&ctx);
    let (mut source, _, bank, mix) = live_stem_tests::fixture(2);
    app.studio.source = Some(LoadedSource {
        audio: source.before.clone(),
        path: "Original mix.wav".into(),
        loudness: -20.,
    });
    app.studio.accept(
        Revision {
            before: source.before.clone(),
            audio: source.after.clone().unwrap(),
            label: "Prepared mix".into(),
            notes: vec![],
            before_lufs: -20.,
            after_lufs: -20.,
            bank: Some(bank),
            mix,
        },
        false,
    );
    app.studio.mix.tracks[0].mute = true;
    app.studio.mix.tracks[1].solo = true;
    app.studio.mix_dirty = true;
    app.studio.monitoring_mix = true;
    app.activate_workspace(3);
    let saved = capture(&app).unwrap();
    let restored = recovery_restore::prepare(saved.payload, cloned_assets(&saved.audio)).unwrap();
    let mut target = test_fixture::app(&ctx);
    restored.adopt(&mut target);
    assert!(
        !target
            .studio
            .player
            .controls
            .playing
            .load(Ordering::Relaxed)
    );
    let restored_bank = target.studio.bank.clone().unwrap();
    let restored_mix = target.studio.mix.clone();
    source.before = target.studio.current.as_ref().unwrap().before.clone();
    source.after = Some(target.studio.current.as_ref().unwrap().audio.clone());
    source.controls = target.studio.player.controls.clone();
    source.tap = target.studio.player.tap.clone();
    source.stems = Some(LiveStemPath {
        processor: RealtimeMix::new(restored_bank.clone(), &restored_mix).unwrap(),
        updates: target.studio.player.restored_mix_mailbox().unwrap(),
        active: false,
    });
    source.controls.playing.store(true, Ordering::Relaxed);
    source
        .controls
        .volume
        .store(1f32.to_bits(), Ordering::Relaxed);
    source.prime(0);
    // Resume/seek uses the real 80 ms transport fade. Inspect settled output,
    // independently of the shorter parameter/mix smoothing boundary.
    for _ in 0..(48000 / 8 * source.channels) {
        assert!(source.next().unwrap().is_finite());
    }
    live_stem_tests::matches_render(&mut source, &restored_bank, &restored_mix);
}
