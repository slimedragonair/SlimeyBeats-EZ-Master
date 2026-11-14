use mastercraft::{
    audio::Audio,
    studio::stem_mix::{self, MAX_STEMS, Stem, StemKind, StemSet},
    studio_native::{
        session::{self, StudioDocument, StudioSnapshot},
        stem_files,
    },
};
use std::sync::{Arc, atomic::AtomicBool};

fn bank() -> StemSet {
    let mut kinds = StemKind::MULTITRACK;
    kinds[8] = StemKind::Part9;
    StemSet::new(
        kinds
            .into_iter()
            .enumerate()
            .map(|(index, kind)| Stem {
                kind,
                audio: Arc::new(
                    Audio::new(
                        (0..3200)
                            .map(|i| {
                                (0.015 * ((i / 2) as f32 * (index + 1) as f32 * 0.07).sin())
                                    * if i % 2 == 0 { 1. } else { 0.5 }
                            })
                            .collect(),
                        16000,
                        2,
                    )
                    .unwrap(),
                ),
            })
            .collect(),
    )
    .unwrap()
}

#[test]
fn nine_parts_mix_independently_and_count_identity_guards_still_hold() {
    let bank = bank();
    assert_eq!(bank.tracks.len(), MAX_STEMS);
    let mut mix = bank.defaults();
    mix.protect_peaks = false;
    let rendered = stem_mix::render(&bank, &mix, &|| false).unwrap();
    for (i, sample) in rendered.samples.iter().enumerate() {
        assert_eq!(
            *sample,
            bank.tracks
                .iter()
                .map(|stem| stem.audio.samples[i])
                .sum::<f32>()
        );
    }
    mix.tracks[8].solo = true;
    assert_eq!(
        stem_mix::render(&bank, &mix, &|| false).unwrap().samples,
        bank.tracks[8].audio.samples
    );
    mix.tracks[8].mute = true;
    assert!(
        stem_mix::render(&bank, &mix, &|| false)
            .unwrap()
            .samples
            .iter()
            .all(|s| *s == 0.)
    );
    mix.tracks[8].mute = false;
    mix.tracks[8].gain_db = -6.;
    mix.tracks[8].balance = -1.;
    let revision = stem_mix::render(&bank, &mix, &|| false).unwrap();
    assert_eq!(
        revision.samples,
        stem_mix::render(&bank, &mix, &|| false).unwrap().samples
    );
    assert!(
        revision
            .samples
            .as_chunks::<2>()
            .0
            .iter()
            .all(|s| s[1].abs() < 1e-8)
    );
    assert!(
        (revision.samples[200] - bank.tracks[8].audio.samples[200] * 10f32.powf(-6. / 20.)).abs()
            < 1e-8
    );
    let mut too_many = bank.tracks.clone();
    too_many.push(Stem {
        kind: StemKind::Instrumental,
        audio: bank.tracks[0].audio.clone(),
    });
    assert!(
        StemSet::new(too_many)
            .err()
            .unwrap()
            .to_string()
            .contains("nine")
    );
    let mut duplicate = bank.tracks.clone();
    duplicate[8].kind = duplicate[0].kind;
    assert!(StemSet::new(duplicate).is_err());
    assert!(
        bank.replace(
            StemKind::Part9,
            Arc::new(Audio::new(vec![0.; 20], 16000, 2).unwrap())
        )
        .is_err()
    );
    // A larger imported-bank cap must not expand the model's extraction contract.
    assert!(stem_mix::consistent(&bank.tracks[0].audio, bank.tracks.clone(), false).is_err());
    for kind in StemKind::ALL {
        assert_eq!(serde_json::to_value(kind).unwrap(), kind.name());
        assert_eq!(
            serde_json::from_value::<StemKind>(kind.name().into()).unwrap(),
            kind
        );
    }
}

#[test]
fn nine_parts_export_reimport_and_reopen_with_mix_and_direct_edit_identity() {
    let bank = bank();
    let root = tempfile::tempdir().unwrap();
    let cancellation = Arc::new(AtomicBool::new(false));
    let exported = stem_files::export(&bank, root.path(), &cancellation).unwrap();
    let folder = std::path::Path::new(exported["folder"].as_str().unwrap());
    assert_eq!(exported["manifest"]["stems"].as_array().unwrap().len(), 9);
    let files: Vec<_> = bank
        .tracks
        .iter()
        .map(|stem| (stem.kind, folder.join(format!("{}.wav", stem.kind.name()))))
        .collect();
    let imported = stem_files::import(&files, cancellation, &|_, _| {}).unwrap();
    for stem in &bank.tracks {
        assert_eq!(
            imported.track(stem.kind).unwrap().audio.samples,
            stem.audio.samples
        );
    }
    let mut mix = imported.defaults();
    mix.tracks[8].gain_db = -4.;
    mix.tracks[4].mute = true;
    let preview = Arc::new(stem_mix::render(&imported, &mix, &|| false).unwrap());
    let snapshot = StudioSnapshot {
        source_name: "Nine instrument arrangement.wav".into(),
        source: preview.clone(),
        before: imported.track(StemKind::Part9).unwrap().audio.clone(),
        preview,
        stems: Some(imported),
        mix: mix.clone(),
        label: "Nine parts".into(),
        notes: vec![],
        view: "stems".into(),
    };
    let path = session::save(&snapshot, root.path(), &|| false).unwrap();
    let restored = StudioDocument::load(&path).unwrap();
    let restored_bank = restored.stems.as_ref().unwrap();
    assert_eq!(restored_bank.tracks.len(), 9);
    assert_eq!(
        serde_json::to_value(&restored.mix).unwrap(),
        serde_json::to_value(mix).unwrap()
    );
    assert_eq!(restored.preview.samples, snapshot.preview.samples);
    assert!(Arc::ptr_eq(
        &restored.before,
        &restored_bank.track(StemKind::Part9).unwrap().audio
    ));
    for stem in &bank.tracks {
        assert_eq!(
            restored_bank.track(stem.kind).unwrap().audio.samples,
            stem.audio.samples
        );
    }
    // Reject a tenth project asset before trying to read it.
    let mut document: StudioDocument =
        serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
    let mut extra = document.stems[0].clone();
    extra.kind = StemKind::Instrumental;
    extra.file.name = "missing-extra.wav".into();
    document.stems.push(extra);
    std::fs::write(&path, serde_json::to_vec(&document).unwrap()).unwrap();
    assert!(
        StudioDocument::load(&path)
            .err()
            .unwrap()
            .to_string()
            .contains("too many assets")
    );
}
