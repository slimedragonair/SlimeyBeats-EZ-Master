//! Native one-or-many admission must not weaken the separate aligned-bank contract.
use super::*;
use crate::{audio::Audio, studio_native::arrangement_import::conform};

#[test]
fn a_single_source_or_alternate_pair_is_valid_only_for_the_arrangement_review() {
    let root = tempfile::tempdir().unwrap();
    let wav = root.path().join("Vocal.wav");
    let mp3 = root.path().join("Vocal.mp3");
    std::fs::write(&wav, b"wav payload").unwrap();
    std::fs::write(&mp3, b"mp3 payload").unwrap();
    assert!(PreparedImport::files(vec![wav.clone()]).is_err());
    let ready = PreparedImport::files_for_arrangement(vec![wav.clone(), mp3.clone()]).unwrap();
    assert_eq!(ready.plan.parts.len(), 1);
    let cancel = Arc::new(AtomicBool::new(false));
    for (format, expected) in [(PreferredFormat::Wav, &wav), (PreferredFormat::Mp3, &mp3)] {
        ready
            .with_audio_files(&[0], &format, cancel.clone(), &|_, _| {}, |files| {
                assert_eq!(files.len(), 1);
                assert_eq!(&files[0].1, expected);
                Ok(())
            })
            .unwrap();
    }
    assert!(
        ready
            .with_audio_files(&[0, 0], &PreferredFormat::Wav, cancel, &|_, _| {}, |_| Ok(
                ()
            ))
            .is_err()
    );
}

#[test]
fn zip_alternates_are_extracted_once_and_private_derivatives_are_cleaned() {
    let root = tempfile::tempdir().unwrap();
    let path = root.path().join("Parts.zip");
    let mut zip = zip::ZipWriter::new(File::create(&path).unwrap());
    for (name, bytes) in [
        ("wav/Vocal.wav", b"wav payload".as_slice()),
        ("mp3/Vocal.mp3", b"mp3 payload".as_slice()),
    ] {
        zip.start_file(
            name,
            zip::write::SimpleFileOptions::default()
                .compression_method(zip::CompressionMethod::Stored),
        )
        .unwrap();
        zip.write_all(bytes).unwrap();
    }
    zip.finish().unwrap();
    assert!(PreparedImport::archive(path.clone()).is_err());
    let ready = PreparedImport::archive_for_arrangement(path).unwrap();
    assert_eq!(ready.plan.parts.len(), 1);
    let extracted = ready
        .with_audio_files(
            &[0],
            &PreferredFormat::Wav,
            Arc::new(AtomicBool::new(false)),
            &|_, _| {},
            |files| {
                assert_eq!(files.len(), 1);
                assert_eq!(std::fs::read(&files[0].1)?, b"wav payload");
                Ok(files[0].1.clone())
            },
        )
        .unwrap();
    assert!(!extracted.exists());
    assert!(
        ready
            .with_audio_files(
                &[0],
                &PreferredFormat::Wav,
                Arc::new(AtomicBool::new(true)),
                &|_, _| {},
                |_| -> Result<()> { panic!("Cancelled import reached publication") }
            )
            .is_err()
    );
}

#[test]
fn same_rate_conformance_is_exact_and_budget_or_cancellation_fails_before_io() {
    let root = tempfile::tempdir().unwrap();
    let samples = vec![-0.0, 1.25, -1.1, 0.1];
    let input = Audio::new(samples.clone(), 8000, 1).unwrap();
    let output = conform(
        input,
        8000,
        16,
        root.path(),
        Arc::new(AtomicBool::new(false)),
    )
    .unwrap();
    assert_eq!(
        output
            .samples
            .iter()
            .map(|v| v.to_bits())
            .collect::<Vec<_>>(),
        samples.iter().map(|v| v.to_bits()).collect::<Vec<_>>()
    );
    let audio = || Audio::new(vec![0.1; 800], 8000, 1).unwrap();
    assert!(
        conform(
            audio(),
            8000,
            1,
            root.path(),
            Arc::new(AtomicBool::new(false))
        )
        .is_err()
    );
    assert!(
        conform(
            audio(),
            48000,
            3200,
            root.path(),
            Arc::new(AtomicBool::new(false))
        )
        .is_err()
    );
    assert!(
        conform(
            audio(),
            48000,
            32000,
            root.path(),
            Arc::new(AtomicBool::new(true))
        )
        .is_err()
    );
    assert_eq!(std::fs::read_dir(root.path()).unwrap().count(), 0);
}
