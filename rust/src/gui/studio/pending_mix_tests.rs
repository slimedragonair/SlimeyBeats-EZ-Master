use super::state::{LoadedSource, Revision, Studio, Target};
use crate::{
    audio::Audio,
    studio::stem_mix::{self, Stem, StemKind, StemSet},
};
use eframe::egui;
use std::{
    path::PathBuf,
    sync::Arc,
    time::{Duration, Instant},
};

fn loaded_studio(output: PathBuf) -> Studio {
    let make_audio = |frequency: f32| {
        Arc::new(
            Audio::new(
                (0..16000)
                    .map(|frame| (frame as f32 * frequency).sin() * 0.04)
                    .collect(),
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
                audio: make_audio(0.13),
            },
            Stem {
                kind: StemKind::Instrumental,
                audio: make_audio(0.27),
            },
        ])
        .unwrap(),
    );
    let mix = bank.defaults();
    let audio = Arc::new(stem_mix::render(&bank, &mix, &|| false).unwrap());
    let mut studio = Studio::new(output);
    studio.source = Some(LoadedSource {
        audio: audio.clone(),
        path: "Original mix.wav".into(),
        loudness: -20.,
    });
    studio.accept(
        Revision {
            before: audio.clone(),
            audio,
            label: "Imported mix".into(),
            notes: vec![],
            before_lufs: -20.,
            after_lufs: -20.,
            bank: Some(bank),
            mix,
        },
        false,
    );
    studio.target = Target::Preview;
    studio
}

fn finish(studio: &mut Studio, context: &egui::Context) {
    let deadline = Instant::now() + Duration::from_secs(10);
    while studio.busy() && Instant::now() < deadline {
        studio.poll(context);
        std::thread::sleep(Duration::from_millis(2));
    }
    assert!(!studio.busy(), "Studio job did not finish");
    assert!(studio.error.is_none(), "{:?}", studio.error);
}

#[test]
fn pending_faders_block_non_mix_jobs_without_losing_controls_or_audio() {
    let root = tempfile::tempdir().unwrap();
    let context = egui::Context::default();
    for action in [
        Studio::clean_voice,
        Studio::repair_audio,
        Studio::render_plugin,
    ] {
        let mut studio = loaded_studio(root.path().into());
        let original = studio.current.as_ref().unwrap().audio.clone();
        studio.mix.tracks[0].gain_db = -6.;
        studio.mix_dirty = true;
        action(&mut studio, &context);
        assert!(
            !studio.busy(),
            "An edit was queued before the pending mix was rendered"
        );
        assert!(
            studio
                .error
                .as_deref()
                .unwrap()
                .contains("Update mix preview")
        );
        assert!(studio.mix_dirty);
        assert_eq!(studio.mix.tracks[0].gain_db, -6.);
        assert!(Arc::ptr_eq(
            &original,
            &studio.current.as_ref().unwrap().audio
        ));
        assert!(studio.snapshot().is_err());
        assert!(studio.working_copy().is_err());
        studio.export_preview(&context);
        assert!(!studio.busy());
        assert!(std::fs::read_dir(root.path()).unwrap().next().is_none());
    }
}

#[test]
fn rendering_pending_faders_then_editing_keeps_the_materialized_mix_and_project_consistent() {
    let root = tempfile::tempdir().unwrap();
    let context = egui::Context::default();
    let mut studio = loaded_studio(root.path().into());
    let bank = studio.bank.clone().unwrap();
    studio.mix.tracks[0].gain_db = -6.;
    studio.mix_dirty = true;
    // Read-only inspection remains available while rendering edits is blocked.
    for target in [
        Target::Source,
        Target::Preview,
        Target::Stem(StemKind::Vocals),
    ] {
        studio.target = target;
        assert!(studio.audio_target().is_ok());
        assert!(studio.editing_target().is_err());
    }
    studio.target = Target::Preview;
    studio.render_mix(&context);
    finish(&mut studio, &context);
    assert!(!studio.mix_dirty);
    let preview = studio.current.as_ref().unwrap();
    let vocals = &bank.track(StemKind::Vocals).unwrap().audio;
    let instrumental = &bank.track(StemKind::Instrumental).unwrap().audio;
    for (index, sample) in preview.audio.samples.iter().enumerate() {
        let expected = vocals.samples[index] * 10f32.powf(-6. / 20.) + instrumental.samples[index];
        assert!((sample - expected).abs() < 1e-7);
    }
    let applied = preview.audio.clone();
    studio.clean_voice(&context);
    finish(&mut studio, &context);
    assert!(!studio.mix_dirty);
    let current = studio.current.as_ref().unwrap();
    assert!(Arc::ptr_eq(&current.before, &applied));
    assert_eq!(current.mix.tracks[0].gain_db, -6.);
    assert_eq!(studio.snapshot().unwrap().mix.tracks[0].gain_db, -6.);
}
