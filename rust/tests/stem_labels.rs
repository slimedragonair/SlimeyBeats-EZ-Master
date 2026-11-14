use mastercraft::{
    audio::Audio,
    studio::{
        stem_import_plan::{self, ImportFile, PartSelection, PreferredFormat},
        stem_mix::{self, Stem, StemKind, StemSet},
    },
    studio_native::{
        session::{self, StudioDocument, StudioSnapshot},
        stem_bundle::PreparedImport,
        stem_files,
    },
};
use std::{
    io::{Cursor, Write},
    sync::{Arc, atomic::AtomicBool},
    time::Duration,
};
use zip::{ZipWriter, write::SimpleFileOptions};

fn plan(names: &[&str]) -> stem_import_plan::ImportPlan {
    stem_import_plan::plan(
        names
            .iter()
            .enumerate()
            .map(|(index, name)| ImportFile {
                index,
                name: (*name).into(),
                format: stem_import_plan::file_format(name).unwrap(),
                bytes: 0,
                crc32: None,
            })
            .collect(),
    )
    .unwrap()
}

#[test]
fn backing_background_and_harmony_never_take_the_lead_vocals_identity() {
    for backing in [
        "Backing Vocals",
        "BackingVocals",
        "Background Vocals",
        "BackgroundVocals",
        "Harmony",
        "Harmony Vocals",
        "Harmonies",
    ] {
        for lead in ["Vocals", "Lead Vocals", "LeadVocals", "Voice"] {
            let backing = format!("Song ({backing}).wav");
            let lead = format!("Song ({lead}).wav");
            let plan = plan(&[&backing, &lead]);
            let backing = plan
                .parts
                .iter()
                .find(|part| part.name == backing.trim_end_matches(".wav"))
                .unwrap();
            let lead = plan
                .parts
                .iter()
                .find(|part| part.name == lead.trim_end_matches(".wav"))
                .unwrap();
            assert!(backing.kind.name().starts_with("part_"), "{}", backing.name);
            assert_eq!(lead.kind, StemKind::Vocals, "{}", lead.name);
        }
    }
    let plan = plan(&["BackingVocals.wav", "Vocals.wav"]);
    assert_eq!(
        plan.parts
            .iter()
            .find(|part| part.name == "BackingVocals")
            .unwrap()
            .kind,
        StemKind::Part1
    );
    assert_eq!(
        plan.parts
            .iter()
            .find(|part| part.name == "Vocals")
            .unwrap()
            .kind,
        StemKind::Vocals
    );
}

#[test]
fn final_suno_role_overrides_instrument_and_voice_words_in_song_titles() {
    let plan = plan(&[
        "Vocals and Bass (Drums).wav",
        "Drums and Guitar (Vocals).wav",
        "Lead Vocals (Backing Vocals).wav",
        "Harmony Song (Piano).wav",
    ]);
    for (name, expected) in [
        ("Vocals and Bass (Drums)", StemKind::Drums),
        ("Drums and Guitar (Vocals)", StemKind::Vocals),
        ("Lead Vocals (Backing Vocals)", StemKind::Part1),
        ("Harmony Song (Piano)", StemKind::Piano),
    ] {
        assert_eq!(
            plan.parts
                .iter()
                .find(|part| part.name == name)
                .unwrap()
                .kind,
            expected
        );
    }
}

#[test]
fn eleven_suno_filename_parts_keep_role_order_and_raw_entry_indices() {
    let names = [
        "Song (Backing Vocals).wav",
        "Song (Bass).wav",
        "Song (Brass).wav",
        "Song (Drums).wav",
        "Song (FX).wav",
        "Song (Guitar).wav",
        "Song (Percussion).wav",
        "Song (Keyboard).wav",
        "Song (Strings).wav",
        "Song (Synth).wav",
        "Song (Vocals).wav",
    ];
    let inspected = plan(&names);
    assert_eq!(inspected.parts.len(), 11);
    let expected = [
        "Vocals",
        "Drums",
        "Bass",
        "Guitar",
        "Keyboard",
        "Synth",
        "Strings",
        "Percussion",
        "Backing Vocals",
        "Brass",
        "FX",
    ];
    assert_eq!(
        inspected
            .parts
            .iter()
            .map(|part| part.name.as_str())
            .collect::<Vec<_>>(),
        expected
            .iter()
            .map(|role| format!("Song ({role})"))
            .collect::<Vec<_>>()
    );
    let selection: Vec<_> = (0..9)
        .map(|part_index| PartSelection {
            part_index,
            kind: None,
        })
        .collect();
    let selected = stem_import_plan::select(&inspected, &selection, &PreferredFormat::Wav).unwrap();
    assert_eq!(selected.len(), 9);
    assert_eq!(
        selected
            .iter()
            .find(|(kind, _)| *kind == StemKind::Vocals)
            .unwrap()
            .1
            .name,
        "Song (Vocals).wav"
    );
    assert!(
        selected
            .iter()
            .any(|(_, file)| file.name == "Song (Backing Vocals).wav")
    );
    assert!(
        selected
            .iter()
            .all(|(_, file)| !file.name.contains("Brass") && !file.name.contains("FX"))
    );
    for part in &inspected.parts {
        for version in &part.versions {
            assert_eq!(names[version.index], version.name);
        }
    }
    let reversed: Vec<_> = names.iter().rev().copied().collect();
    assert_eq!(
        plan(&reversed)
            .parts
            .iter()
            .map(|part| &part.name)
            .collect::<Vec<_>>(),
        inspected
            .parts
            .iter()
            .map(|part| &part.name)
            .collect::<Vec<_>>()
    );
    let all: Vec<_> = (0..11)
        .map(|part_index| PartSelection {
            part_index,
            kind: None,
        })
        .collect();
    assert_eq!(
        stem_import_plan::select(&inspected, &all, &PreferredFormat::Wav)
            .unwrap()
            .len(),
        11
    );
    let paired = plan_names_with_alternate_encodings();
    assert_eq!(paired.parts.len(), 2);
    assert!(paired.parts.iter().all(|part| part.versions.len() == 2));
    assert_eq!(
        paired
            .parts
            .iter()
            .find(|part| part.name == "Song (Vocals)")
            .unwrap()
            .kind,
        StemKind::Vocals
    );
}

#[test]
fn role_priority_uses_suffixes_then_names_without_discarding_generic_parts() {
    let inspected = plan(&[
        "Vocals in title (FX).wav",
        "Song (Other).wav",
        "Song (Instrumental).wav",
        "Song (Unclassified).wav",
        "Song (Harmony).wav",
        "Song (Background Vocals).wav",
        "Vocals in title (Brass).wav",
        "Song (Piano).wav",
        "Song (Keyboard).wav",
    ]);
    assert_eq!(
        inspected
            .parts
            .iter()
            .map(|part| part.name.as_str())
            .collect::<Vec<_>>(),
        [
            "Song (Keyboard)",
            "Song (Piano)",
            "Song (Background Vocals)",
            "Song (Harmony)",
            "Song (Other)",
            "Song (Instrumental)",
            "Song (Unclassified)",
            "Vocals in title (Brass)",
            "Vocals in title (FX)",
        ]
    );
    assert!(
        inspected
            .parts
            .iter()
            .all(|part| part.kind != StemKind::Vocals)
    );
}

fn plan_names_with_alternate_encodings() -> stem_import_plan::ImportPlan {
    plan(&[
        "WAV/Song (Backing Vocals).wav",
        "MP3/Song (Backing Vocals).mp3",
        "WAV/Song (Vocals).wav",
        "MP3/Song (Vocals).mp3",
    ])
}

fn bank() -> StemSet {
    StemSet::new(
        [StemKind::Vocals, StemKind::Part1]
            .into_iter()
            .enumerate()
            .map(|(index, kind)| Stem {
                kind,
                audio: Arc::new(Audio::new(vec![0.02 * (index + 1) as f32; 800], 8000, 1).unwrap()),
            })
            .collect(),
    )
    .unwrap()
}

#[test]
fn checked_names_survive_clone_and_replace_without_changing_controls_or_pcm() {
    let mut bank = bank();
    let mut mix = bank.defaults();
    mix.protect_peaks = false;
    let original = stem_mix::render(&bank, &mix, &|| false).unwrap();
    assert_eq!(bank.display_name(StemKind::Vocals), "vocals");
    bank.set_display_name(StemKind::Vocals, "Song (Vocals)")
        .unwrap();
    bank.set_display_name(StemKind::Part1, "Song (Backing Vocals)")
        .unwrap();
    let clone = bank.clone();
    let replaced = clone
        .replace(
            StemKind::Vocals,
            clone.track(StemKind::Vocals).unwrap().audio.clone(),
        )
        .unwrap();
    assert_eq!(replaced.display_name(StemKind::Vocals), "Song (Vocals)");
    assert_eq!(
        replaced.display_name(StemKind::Part1),
        "Song (Backing Vocals)"
    );
    assert_eq!(
        stem_mix::render(&replaced, &mix, &|| false)
            .unwrap()
            .samples,
        original.samples
    );
    assert_eq!(
        serde_json::to_value(bank.defaults()).unwrap(),
        serde_json::to_value(replaced.defaults()).unwrap()
    );
    for invalid in [
        "".to_owned(),
        " ".into(),
        "x".repeat(513),
        "é".repeat(257),
        "bad\nlabel".into(),
        "bad\0label".into(),
    ] {
        assert!(bank.set_display_name(StemKind::Vocals, invalid).is_err());
        assert_eq!(bank.display_name(StemKind::Vocals), "Song (Vocals)");
    }
    assert!(bank.set_display_name(StemKind::Drums, "Absent").is_err());
    bank.set_display_name(StemKind::Part1, "é".repeat(256))
        .unwrap();
    assert_eq!(bank.display_name(StemKind::Part1).len(), 512);
    // Markup characters are literal text, never a fragment to render as HTML.
    bank.set_display_name(StemKind::Part1, "Song <Harmony> & Friends")
        .unwrap();
    assert_eq!(
        bank.display_name(StemKind::Part1),
        "Song <Harmony> & Friends"
    );
}

#[test]
fn loose_and_zip_imports_use_original_basenames_and_raw_manifest_keeps_stable_filenames() {
    let root = tempfile::tempdir().unwrap();
    let expected = bank();
    // Raw input order differs from review order; each part has distinct PCM.
    let files = [
        (StemKind::Part1, "Song (Backing Vocals).wav"),
        (StemKind::Vocals, "Song (Vocals).wav"),
    ];
    let paths: Vec<_> = files
        .iter()
        .map(|(kind, name)| {
            let path = root.path().join(name);
            let mut wav = Cursor::new(Vec::new());
            mastercraft::wav_encoding::write_wav(
                &expected.track(*kind).unwrap().audio,
                &mut wav,
                "wav32f",
                false,
            )
            .unwrap();
            std::fs::write(&path, wav.get_ref()).unwrap();
            path
        })
        .collect();
    let direct = stem_files::import(
        &[
            (StemKind::Vocals, paths[1].clone()),
            (StemKind::Part1, paths[0].clone()),
        ],
        Arc::new(AtomicBool::new(false)),
        &|_, _| {},
    )
    .unwrap();
    assert_eq!(
        direct.display_name(StemKind::Part1),
        "Song (Backing Vocals)"
    );
    let archive = root.path().join("Synthetic parts.zip");
    let mut writer = ZipWriter::new(std::fs::File::create(&archive).unwrap());
    for ((_, name), path) in files.iter().zip(&paths) {
        writer
            .start_file(format!("WAV/{name}"), SimpleFileOptions::default())
            .unwrap();
        writer.write_all(&std::fs::read(path).unwrap()).unwrap();
    }
    writer.finish().unwrap();
    for prepared in [
        PreparedImport::files(paths).unwrap(),
        PreparedImport::archive(archive).unwrap(),
    ] {
        let selected: Vec<_> = (0..2)
            .map(|part_index| PartSelection {
                part_index,
                kind: None,
            })
            .collect();
        let imported = prepared
            .import(
                &selected,
                &PreferredFormat::Wav,
                1024 * 1024,
                Duration::from_secs(5),
                Arc::new(AtomicBool::new(false)),
                &|_, _| {},
            )
            .unwrap();
        assert_eq!(imported.display_name(StemKind::Vocals), "Song (Vocals)");
        assert_eq!(
            imported.display_name(StemKind::Part1),
            "Song (Backing Vocals)"
        );
        let export = stem_files::export(&imported, root.path(), &AtomicBool::new(false)).unwrap();
        for entry in export["manifest"]["stems"].as_array().unwrap() {
            let kind: StemKind = serde_json::from_value(entry["kind"].clone()).unwrap();
            assert_eq!(entry["file"], format!("{}.wav", kind.name()));
            assert_eq!(entry["display_name"], imported.display_name(kind).as_ref());
            assert_eq!(
                imported.track(kind).unwrap().audio.samples,
                expected.track(kind).unwrap().audio.samples
            );
        }
    }
}

#[test]
fn project_roundtrip_keeps_names_edits_and_pcm_and_legacy_projects_fall_back() {
    let root = tempfile::tempdir().unwrap();
    let mut bank = bank();
    bank.set_display_name(StemKind::Vocals, "Song (Vocals)")
        .unwrap();
    bank.set_display_name(StemKind::Part1, "Song (Backing Vocals)")
        .unwrap();
    let source = bank.tracks[0].audio.clone();
    let mut mix = bank.defaults();
    mix.tracks[1].gain_db = -3.;
    let snapshot = StudioSnapshot {
        source_name: "Synthetic source.wav".into(),
        source: source.clone(),
        before: source.clone(),
        preview: source,
        stems: Some(bank.clone()),
        mix,
        label: "Synthetic project".into(),
        notes: vec![],
        view: "stems".into(),
    };
    let path = session::save(&snapshot, root.path(), &|| false).unwrap();
    let loaded = StudioDocument::load(&path).unwrap();
    let restored = loaded.stems.unwrap();
    for stem in &bank.tracks {
        assert_eq!(
            restored.display_name(stem.kind),
            bank.display_name(stem.kind)
        );
        assert_eq!(
            restored.track(stem.kind).unwrap().audio.samples,
            stem.audio.samples
        );
    }
    assert_eq!(loaded.mix.tracks[1].gain_db, -3.);
    assert!(Arc::ptr_eq(
        &loaded.before,
        &restored.track(StemKind::Vocals).unwrap().audio
    ));
    let mut document: serde_json::Value =
        serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
    for stem in document["stems"].as_array_mut().unwrap() {
        stem.as_object_mut().unwrap().remove("display_name");
    }
    std::fs::write(&path, serde_json::to_vec(&document).unwrap()).unwrap();
    let legacy = StudioDocument::load(&path).unwrap().stems.unwrap();
    assert_eq!(legacy.display_name(StemKind::Vocals), "vocals");
    assert_eq!(legacy.display_name(StemKind::Part1), "part_1");
    document["stems"][0]["display_name"] = "x".repeat(513).into();
    std::fs::write(&path, serde_json::to_vec(&document).unwrap()).unwrap();
    assert!(
        StudioDocument::load(&path)
            .err()
            .unwrap()
            .to_string()
            .contains("512")
    );
}
