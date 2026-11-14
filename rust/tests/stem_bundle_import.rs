use mastercraft::{
    audio::Audio,
    studio::{
        stem_archive,
        stem_import_plan::{self, PartSelection, PreferredFormat},
        stem_mix::MAX_STEMS,
    },
    studio_native::stem_bundle::PreparedImport,
};
use std::{
    io::{Cursor, Write},
    sync::{Arc, atomic::AtomicBool},
    time::Duration,
};
use zip::{CompressionMethod, ZipWriter, write::SimpleFileOptions};

fn archive(entries: &[(&str, &[u8])]) -> Vec<u8> {
    let mut writer = ZipWriter::new(Cursor::new(Vec::new()));
    for (name, bytes) in entries {
        writer
            .start_file(
                *name,
                SimpleFileOptions::default().compression_method(CompressionMethod::Deflated),
            )
            .unwrap();
        writer.write_all(bytes).unwrap();
    }
    writer.finish().unwrap().into_inner()
}
fn wav(level: f32) -> Vec<u8> {
    let audio = Audio::new(vec![level; 3200], 16000, 2).unwrap();
    let mut output = Cursor::new(Vec::new());
    mastercraft::wav_encoding::write_wav(&audio, &mut output, "wav32f", false).unwrap();
    output.into_inner()
}
fn selection(count: usize) -> Vec<PartSelection> {
    (0..count)
        .map(|part_index| PartSelection {
            part_index,
            kind: None,
        })
        .collect()
}

#[test]
fn suno_pairs_are_one_part_and_only_the_preferred_encoding_is_read() {
    let vocal = wav(0.1);
    let bass = wav(0.05);
    let bytes = archive(&[
        ("WAV/Vocals.WAV", &vocal),
        ("MP3/Vocals.mp3", b"mp3 vocal fixture"),
        ("WAV/Bass.wav", &bass),
        ("MP3/Bass.MP3", b"mp3 bass fixture"),
        ("cover.png", b"ignored"),
        ("__MACOSX/._Vocals.wav", b"ignored"),
    ]);
    let plan = stem_archive::inspect(Cursor::new(&bytes), 1024 * 1024).unwrap();
    assert_eq!(plan.parts.len(), 2);
    assert_eq!(plan.skipped_files, 2);
    assert!(plan.parts.iter().all(|part| part.versions.len() == 2));
    for format in [PreferredFormat::Wav, PreferredFormat::Mp3] {
        let picked = stem_import_plan::select(&plan, &selection(2), &format).unwrap();
        assert!(
            picked
                .iter()
                .all(|(_, file)| file.format == format.extension())
        );
        for (_, file) in picked {
            let mut extracted = Vec::new();
            stem_archive::copy_audio(
                Cursor::new(&bytes),
                &file,
                1024 * 1024,
                &mut extracted,
                &|| false,
            )
            .unwrap();
            if file.format == "wav" {
                assert!(extracted.starts_with(b"RIFF"));
            } else {
                assert!(extracted.starts_with(b"mp3"));
            }
        }
    }
    let root = tempfile::tempdir().unwrap();
    let path = root.path().join("Suno stems.zip");
    std::fs::write(&path, &bytes).unwrap();
    let prepared = PreparedImport::archive(path).unwrap();
    let bank = prepared
        .import(
            &selection(2),
            &PreferredFormat::Wav,
            1024 * 1024,
            Duration::from_secs(5),
            Arc::new(AtomicBool::new(false)),
            &|_, _| {},
        )
        .unwrap();
    assert_eq!(bank.tracks.len(), 2);
    assert!(
        bank.track(mastercraft::studio::stem_mix::StemKind::Vocals)
            .unwrap()
            .audio
            .samples
            .iter()
            .all(|sample| *sample == 0.1)
    );
}

#[test]
fn more_than_nine_parts_remain_available_for_explicit_selection() {
    let audio = wav(0.03);
    let names: Vec<_> = (0..12).map(|i| format!("guitar take {i:02}.wav")).collect();
    let entries: Vec<_> = names
        .iter()
        .map(|name| (name.as_str(), audio.as_slice()))
        .collect();
    let bytes = archive(&entries);
    let plan = stem_archive::inspect(Cursor::new(&bytes), 1024 * 1024).unwrap();
    assert_eq!(plan.parts.len(), 12);
    assert!(stem_import_plan::select(&plan, &selection(10), &PreferredFormat::Wav).is_err());
    let last_nine: Vec<_> = (3..12)
        .map(|part_index| PartSelection {
            part_index,
            kind: None,
        })
        .collect();
    let picked = stem_import_plan::select(&plan, &last_nine, &PreferredFormat::Wav).unwrap();
    assert_eq!(picked.len(), MAX_STEMS);
    assert_eq!(
        picked
            .iter()
            .map(|(kind, _)| kind)
            .collect::<std::collections::BTreeSet<_>>()
            .len(),
        9
    );
}

#[test]
fn loose_multiple_files_pair_encodings_and_fall_back_to_the_available_format() {
    let root = tempfile::tempdir().unwrap();
    for (name, bytes) in [
        ("Guitar.wav", wav(0.1)),
        ("Guitar.mp3", b"alternate".to_vec()),
        ("Piano.wav", wav(0.02)),
    ] {
        std::fs::write(root.path().join(name), bytes).unwrap();
    }
    let paths = ["Guitar.mp3", "Piano.wav", "Guitar.wav"].map(|name| root.path().join(name));
    let prepared = PreparedImport::files(paths.to_vec()).unwrap();
    assert_eq!(prepared.plan.parts.len(), 2);
    let picked =
        stem_import_plan::select(&prepared.plan, &selection(2), &PreferredFormat::Mp3).unwrap();
    assert_eq!(
        picked
            .iter()
            .find(|(kind, _)| *kind == mastercraft::studio::stem_mix::StemKind::Guitar)
            .unwrap()
            .1
            .format,
        "mp3"
    );
    assert_eq!(
        picked
            .iter()
            .find(|(kind, _)| *kind == mastercraft::studio::stem_mix::StemKind::Piano)
            .unwrap()
            .1
            .format,
        "wav"
    );
}

#[test]
fn unsafe_ambiguous_oversized_and_changed_archives_fail_without_extracting() {
    let valid = archive(&[("vocals.wav", b"first"), ("bass.wav", b"second")]);
    let long_path = format!("{}vocals.wav", "folder/".repeat(600));
    assert!(
        stem_archive::inspect(
            Cursor::new(archive(&[(&long_path, b"bad"), ("bass.wav", b"safe")])),
            1024
        )
        .is_err()
    );
    for bad in [
        "../vocals.wav",
        "/vocals.wav",
        "C:/vocals.wav",
        "nested/../../vocals.wav",
    ] {
        assert!(
            stem_archive::inspect(
                Cursor::new(archive(&[(bad, b"bad"), ("bass.wav", b"safe")])),
                1024
            )
            .is_err(),
            "{bad}"
        );
    }
    assert!(
        stem_archive::inspect(
            Cursor::new(archive(&[
                ("a/guitar.wav", b"one"),
                ("b/guitar.wav", b"two")
            ])),
            1024
        )
        .is_err()
    );
    assert!(stem_archive::inspect(Cursor::new(&valid), 2).is_err());
    assert!(stem_archive::inspect(Cursor::new(b"not a zip"), 1024).is_err());
    let plan = stem_archive::inspect(Cursor::new(&valid), 1024).unwrap();
    let selected = &plan
        .parts
        .iter()
        .find(|part| part.kind == mastercraft::studio::stem_mix::StemKind::Bass)
        .unwrap()
        .versions[0];
    assert!(
        stem_archive::copy_audio(
            Cursor::new(&valid),
            selected,
            1024,
            &mut Vec::new(),
            &|| true
        )
        .is_err()
    );
    let changed = archive(&[("vocals.wav", b"first"), ("bass.wav", b"edited")]);
    assert!(
        stem_archive::copy_audio(
            Cursor::new(changed),
            selected,
            1024,
            &mut Vec::new(),
            &|| false
        )
        .is_err()
    );
}
