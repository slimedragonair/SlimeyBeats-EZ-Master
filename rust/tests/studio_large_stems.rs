use mastercraft::{
    audio::Audio,
    studio::{
        stem_archive,
        stem_import_plan::{self, ImportFile, PartSelection, PreferredFormat},
        stem_mix::{self, Stem, StemKind},
    },
    studio_native::{
        session::{self, StudioDocument, StudioSnapshot},
        stem_bundle::PreparedImport,
        stem_files,
    },
};
use std::{
    borrow::Cow,
    collections::BTreeSet,
    io::{Cursor, Write},
    sync::{Arc, atomic::AtomicBool},
    time::Duration,
};
use zip::{ZipWriter, write::SimpleFileOptions};

fn files(names: &[String]) -> Vec<ImportFile> {
    names
        .iter()
        .enumerate()
        .map(|(index, name)| ImportFile {
            index,
            name: name.clone(),
            format: stem_import_plan::file_format(name).unwrap_or_default(),
            bytes: 1,
            crc32: None,
        })
        .collect()
}
fn selection(count: usize) -> Vec<PartSelection> {
    (0..count)
        .map(|part_index| PartSelection {
            part_index,
            kind: None,
        })
        .collect()
}
fn audio(level: f32) -> Audio {
    Audio::new(vec![level; 800], 8000, 1).unwrap()
}
fn wav(audio: &Audio) -> Vec<u8> {
    let mut output = Cursor::new(Vec::new());
    mastercraft::wav_encoding::write_wav(audio, &mut output, "wav32f", false).unwrap();
    output.into_inner()
}
fn archive(entries: impl IntoIterator<Item = (String, Vec<u8>)>) -> Vec<u8> {
    let mut writer = ZipWriter::new(Cursor::new(Vec::new()));
    for (name, bytes) in entries {
        writer
            .start_file(name, SimpleFileOptions::default())
            .unwrap();
        writer.write_all(&bytes).unwrap();
    }
    writer.finish().unwrap().into_inner()
}

#[test]
fn canonical_ids_extend_legacy_strings_without_aliases_or_wrapping() {
    for kind in StemKind::ALL {
        assert_eq!(serde_json::to_value(kind).unwrap(), kind.name().as_ref());
        assert_eq!(
            serde_json::from_value::<StemKind>(kind.name().into_owned().into()).unwrap(),
            kind
        );
    }
    assert!(matches!(StemKind::Vocals.name(), Cow::Borrowed("vocals")));
    for index in [1, 9, 10, 64, 100, 1000, u32::MAX] {
        let kind = StemKind::part(index).unwrap();
        assert_eq!(kind.name(), format!("part_{index}"));
        assert_eq!(
            serde_json::from_str::<StemKind>(&serde_json::to_string(&kind).unwrap()).unwrap(),
            kind
        );
    }
    assert_eq!(StemKind::part(9).unwrap(), StemKind::Part9);
    assert_eq!(StemKind::part(10).unwrap(), StemKind::ExtraPart(10));
    assert!(StemKind::part(0).is_err());
    for name in [
        "part_0",
        "part_00",
        "part_01",
        "part_010",
        "part_+1",
        "part_-1",
        "part_1.0",
        "part_4294967296",
        "part_9999999999",
        "PART_10",
        "part_10\n",
        " vocals",
        "vocals ",
    ] {
        assert!(
            serde_json::from_value::<StemKind>(name.into()).is_err(),
            "{name}"
        );
    }
    for value in [
        serde_json::json!(10),
        serde_json::json!({"extra_part":10}),
        serde_json::Value::Null,
    ] {
        assert!(serde_json::from_value::<StemKind>(value).is_err());
    }
    for index in 0..=9 {
        assert!(serde_json::to_value(StemKind::ExtraPart(index)).is_err());
        assert!(stem_mix::validate_kinds([StemKind::Vocals, StemKind::ExtraPart(index)]).is_err());
    }
    let mut used: BTreeSet<_> = (1..=9)
        .map(|index| StemKind::part(index).unwrap())
        .collect();
    used.insert(StemKind::ExtraPart(1000));
    assert_eq!(
        stem_import_plan::available_kind(StemKind::Part1, &used),
        StemKind::ExtraPart(1001)
    );
    used.insert(StemKind::ExtraPart(u32::MAX));
    assert_eq!(
        stem_import_plan::available_kind(StemKind::Part1, &used),
        StemKind::ExtraPart(10)
    );
}

#[test]
fn a_thousand_repeated_instrument_parts_plan_and_select_without_a_count_cap() {
    let names: Vec<_> = (0..1000)
        .rev()
        .map(|index| format!("Guitar take {index:04}.wav"))
        .collect();
    let plan = stem_import_plan::plan(files(&names)).unwrap();
    assert_eq!(plan.parts.len(), 1000);
    assert_eq!(plan.parts[0].kind, StemKind::Guitar);
    assert_eq!(plan.parts[1].kind, StemKind::Part1);
    assert_eq!(plan.parts[9].kind, StemKind::Part9);
    assert_eq!(plan.parts[10].kind, StemKind::ExtraPart(10));
    assert_eq!(plan.parts[999].kind, StemKind::ExtraPart(999));
    assert_eq!(
        plan.parts
            .iter()
            .map(|part| part.kind)
            .collect::<BTreeSet<_>>()
            .len(),
        1000
    );
    let selected =
        stem_import_plan::select(&plan, &selection(1000), &PreferredFormat::Wav).unwrap();
    assert_eq!(selected.len(), 1000);
    for (_, file) in selected {
        assert_eq!(names[file.index], file.name);
    }
}

#[test]
fn source_folders_disambiguate_repeated_names_and_preserve_suno_pairing() {
    let names: Vec<_> = [
        "SongA/WAV/Vocals.wav",
        "SongA/MP3/Vocals.mp3",
        "SongB/WAV/Vocals.wav",
        "SongB/MP3/Vocals.mp3",
    ]
    .map(String::from)
    .into();
    let plan = stem_import_plan::plan(files(&names)).unwrap();
    assert_eq!(plan.parts.len(), 2);
    assert_eq!(plan.parts[0].name, "SongA / Vocals");
    assert_eq!(plan.parts[1].name, "SongB / Vocals");
    assert_eq!(plan.parts[0].kind, StemKind::Vocals);
    assert_eq!(plan.parts[1].kind, StemKind::Part1);
    assert!(plan.parts.iter().all(|part| part.versions.len() == 2));
    for part in &plan.parts {
        for version in &part.versions {
            assert_eq!(names[version.index], version.name);
        }
    }
    let roles = [
        "Backing Vocals",
        "Bass",
        "Brass",
        "Drums",
        "FX",
        "Guitar",
        "Keyboard",
        "Percussion",
        "Strings",
        "Synth",
        "Vocals",
    ];
    let names: Vec<_> = roles
        .into_iter()
        .flat_map(|role| {
            [
                format!("Suno/WAV/Song ({role}).wav"),
                format!("Suno/MP3/Song ({role}).mp3"),
            ]
        })
        .collect();
    let plan = stem_import_plan::plan(files(&names)).unwrap();
    assert_eq!(plan.parts.len(), 11);
    assert!(
        plan.parts
            .iter()
            .all(|part| part.versions.len() == 2 && !part.name.contains(" / "))
    );
    for format in [PreferredFormat::Wav, PreferredFormat::Mp3] {
        assert_eq!(
            stem_import_plan::select(&plan, &selection(11), &format)
                .unwrap()
                .len(),
            11
        );
    }
    let private: Vec<_> = [
        "C:/Private/User/Album/Vocals.wav",
        "D:/Private/User/Album/Vocals.wav",
    ]
    .map(String::from)
    .into();
    let plan = stem_import_plan::plan(files(&private)).unwrap();
    assert_ne!(plan.parts[0].name, plan.parts[1].name);
    assert!(
        plan.parts
            .iter()
            .all(|part| !part.name.contains("Private") && !part.name.contains(':'))
    );
}

#[test]
fn metadata_budgets_replace_file_counts_and_cover_ignored_zip_entries() {
    let names: Vec<_> = (0..600)
        .map(|index| format!("Part {index:04}.wav"))
        .collect();
    let zipped = archive(names.iter().cloned().map(|name| (name, vec![1])));
    assert_eq!(
        stem_archive::inspect(Cursor::new(&zipped), 1024)
            .unwrap()
            .parts
            .len(),
        600
    );
    let limit = stem_import_plan::MAX_IMPORT_METADATA_BYTES;
    let overhead = stem_import_plan::IMPORT_FILE_METADATA_OVERHEAD;
    assert_eq!(
        stem_import_plan::charge_metadata(0, limit - overhead).unwrap(),
        limit
    );
    assert!(stem_import_plan::charge_metadata(limit, 1).is_err());
    assert!(stem_import_plan::charge_metadata(usize::MAX, 0).is_err());
    let ignored = "x".repeat(limit);
    assert!(
        stem_import_plan::plan(vec![ImportFile {
            index: 0,
            name: ignored,
            format: String::new(),
            bytes: 0,
            crc32: None
        }])
        .unwrap_err()
        .to_string()
        .contains("metadata")
    );
    let prefix = "x".repeat(60000);
    let zipped = archive((0..150).map(|index| (format!("{prefix}{index}.txt"), vec![0])));
    assert!(
        stem_archive::inspect(Cursor::new(zipped), 1024)
            .unwrap_err()
            .to_string()
            .contains("metadata")
    );
    let changed = archive([
        ("Song (Vocals).wav".into(), vec![1]),
        ("Song (Bass).wav".into(), vec![2]),
    ]);
    let plan = stem_archive::inspect(Cursor::new(&changed), 1024).unwrap();
    let mut sink = Vec::new();
    assert!(
        stem_archive::copy_audio(
            Cursor::new(&changed),
            &plan.parts[0].versions[0],
            1024,
            &mut sink,
            &|| true
        )
        .is_err()
    );
    assert!(sink.is_empty());
}

#[test]
fn sixty_four_and_one_hundred_parts_mix_export_and_reopen_with_names_and_edit_identity() {
    for count in [64, 100] {
        let root = tempfile::tempdir().unwrap();
        let paths: Vec<_> = (0..count)
            .map(|index| {
                let path = root.path().join(format!("Guitar take {index:03}.wav"));
                std::fs::write(&path, wav(&audio((index + 1) as f32 * 0.0001))).unwrap();
                path
            })
            .collect();
        let prepared = PreparedImport::files(paths).unwrap();
        assert!(
            prepared
                .plan
                .parts
                .iter()
                .flat_map(|part| &part.versions)
                .all(|file| !file.name.contains(['/', '\\']))
        );
        let bank = prepared
            .import(
                &selection(count),
                &PreferredFormat::Wav,
                1024 * 1024,
                Duration::from_secs(5),
                Arc::new(AtomicBool::new(false)),
                &|_, _| {},
            )
            .unwrap();
        assert_eq!(bank.tracks.len(), count);
        assert_eq!(bank.defaults().tracks.len(), count);
        let mut mix = bank.defaults();
        mix.protect_peaks = false;
        let rendered = stem_mix::render(&bank, &mix, &|| false).unwrap();
        let expected: f32 = bank.tracks.iter().map(|stem| stem.audio.samples[0]).sum();
        assert!(rendered.samples.iter().all(|sample| *sample == expected));
        mix.tracks[count - 1].solo = true;
        assert_eq!(
            stem_mix::render(&bank, &mix, &|| false).unwrap().samples,
            bank.tracks[count - 1].audio.samples
        );
        mix.tracks[count - 1].solo = false;
        mix.tracks[count - 1].gain_db = -4.;
        let preview = Arc::new(stem_mix::render(&bank, &mix, &|| false).unwrap());
        let exported = stem_files::export(&bank, root.path(), &AtomicBool::new(false)).unwrap();
        assert_eq!(
            exported["manifest"]["stems"].as_array().unwrap().len(),
            count
        );
        let folder = std::path::Path::new(exported["folder"].as_str().unwrap());
        for stem in &bank.tracks {
            let bytes = std::fs::read(folder.join(format!("{}.wav", stem.kind.name()))).unwrap();
            assert_eq!(bytes, wav(&stem.audio));
        }
        let last = &bank.tracks[count - 1];
        let snapshot = StudioSnapshot {
            source_name: "Synthetic arrangement.wav".into(),
            source: Arc::new(rendered),
            stems: Some(bank.clone()),
            mix: mix.clone(),
            before: last.audio.clone(),
            preview,
            label: "Large imported bank".into(),
            notes: vec![],
            view: "stems".into(),
        };
        let project = session::save(&snapshot, root.path(), &|| false).unwrap();
        let reopened = StudioDocument::load(&project).unwrap();
        let restored = reopened.stems.unwrap();
        assert_eq!(restored.tracks.len(), count);
        assert_eq!(
            serde_json::to_value(&reopened.mix).unwrap(),
            serde_json::to_value(mix).unwrap()
        );
        assert!(Arc::ptr_eq(
            &reopened.before,
            &restored.track(last.kind).unwrap().audio
        ));
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
        let replaced = restored.replace(last.kind, last.audio.clone()).unwrap();
        assert_eq!(
            replaced.display_name(last.kind),
            bank.display_name(last.kind)
        );
        assert!(stem_mix::consistent(&snapshot.source, bank.tracks.clone(), false).is_err());
        let misaligned = Arc::new(Audio::new(vec![0.; 20], 8000, 1).unwrap());
        assert!(bank.replace(last.kind, misaligned).is_err());
    }
}

#[test]
fn native_per_input_budget_stays_independent_from_combined_bank_budget_and_folder_identity() {
    assert_eq!(stem_files::DEFAULT_INPUT_DECODED_BYTES, 512 * 1024 * 1024);
    assert_eq!(
        stem_files::DEFAULT_BANK_DECODED_BYTES,
        4 * 1024 * 1024 * 1024
    );
    const {
        assert!(759_475_200 < stem_files::DEFAULT_BANK_DECODED_BYTES);
    }
    let root = tempfile::tempdir().unwrap();
    let mut paths = Vec::new();
    for (folder, level) in [("SongA", 0.02), ("SongB", 0.04)] {
        let directory = root.path().join(folder);
        std::fs::create_dir(&directory).unwrap();
        let path = directory.join("Vocals.wav");
        std::fs::write(&path, wav(&audio(level))).unwrap();
        paths.push(path);
    }
    let prepared = PreparedImport::files(paths).unwrap();
    let budget = 800 * size_of::<f32>();
    let imported = prepared
        .import(
            &selection(2),
            &PreferredFormat::Wav,
            budget,
            Duration::from_secs(5),
            Arc::new(AtomicBool::new(false)),
            &|_, _| {},
        )
        .unwrap();
    assert_eq!(imported.tracks.len(), 2);
    assert_eq!(
        imported
            .tracks
            .iter()
            .map(|stem| stem.audio.samples.len() * 4)
            .sum::<usize>(),
        budget * 2
    );
    assert_eq!(imported.display_name(StemKind::Vocals), "SongA / Vocals");
    assert_eq!(imported.display_name(StemKind::Part1), "SongB / Vocals");
    assert_eq!(
        imported.track(StemKind::Vocals).unwrap().audio.samples,
        audio(0.02).samples
    );
    assert_eq!(
        imported.track(StemKind::Part1).unwrap().audio.samples,
        audio(0.04).samples
    );
    assert!(
        prepared
            .import(
                &selection(2),
                &PreferredFormat::Wav,
                budget - 4,
                Duration::from_secs(5),
                Arc::new(AtomicBool::new(false)),
                &|_, _| {}
            )
            .is_err()
    );
}

#[test]
fn extending_import_ids_preserves_two_and_four_stem_model_outputs() {
    let estimates: Vec<_> = [
        StemKind::Vocals,
        StemKind::Drums,
        StemKind::Bass,
        StemKind::Other,
    ]
    .into_iter()
    .map(|kind| Stem {
        kind,
        audio: Arc::new(audio(0.01)),
    })
    .collect();
    let source = audio(0.04);
    for (two, count) in [(true, 2), (false, 4)] {
        let bank = stem_mix::consistent(&source, estimates.clone(), two).unwrap();
        assert_eq!(bank.tracks.len(), count);
        let mut mix = bank.defaults();
        mix.protect_peaks = false;
        let result = stem_mix::render(&bank, &mix, &|| false).unwrap();
        assert!(
            result
                .samples
                .iter()
                .all(|sample| (*sample - 0.04).abs() < 1e-7)
        );
    }
}
