//! Route real temporary file paths through the native importer without opening a device.
use super::*;
use crate::{audio::Audio, gui::test_fixture, wav_encoding};
use std::time::{Duration, Instant};

fn input(root: &std::path::Path, name: &str, frames: usize) -> PathBuf {
    let path = root.join(name);
    let file = std::fs::File::create(&path).unwrap();
    wav_encoding::write_wav(
        &Audio::new(vec![0.02; frames], 8000, 1).unwrap(),
        file,
        "wav32f",
        false,
    )
    .unwrap();
    path
}
fn finish(app: &mut MasterCraftApp, ctx: &egui::Context) {
    let deadline = Instant::now() + Duration::from_secs(5);
    while app.studio.busy() && Instant::now() < deadline {
        app.studio.poll(ctx);
        std::thread::sleep(Duration::from_millis(2));
    }
    assert!(!app.studio.busy());
    assert!(app.studio.error.is_none(), "{:?}", app.studio.error);
}

#[test]
fn one_or_many_dropped_files_review_as_tracks_without_changing_the_main_queue() {
    for count in [1, 2] {
        let root = tempfile::tempdir().unwrap();
        let ctx = egui::Context::default();
        let mut app = test_fixture::app(&ctx);
        app.tab = 3;
        app.studio.page = Page::Tracks;
        let original_queue = app.tracks.len();
        let paths: Vec<_> = (0..count)
            .map(|index| {
                input(
                    root.path(),
                    &format!("Track {index}.wav"),
                    8000 + index * 2000,
                )
            })
            .collect();
        let bytes: Vec<_> = paths
            .iter()
            .map(|path| std::fs::read(path).unwrap())
            .collect();
        app.route_file_drop(paths.clone(), &ctx);
        finish(&mut app, &ctx);
        assert_eq!(app.tracks.len(), original_queue);
        assert_eq!(app.studio.page, Page::Tracks);
        let review = app.studio.bulk_import.as_ref().unwrap();
        assert!(review.is_arrangement());
        assert_eq!(review.selected_count(), count);
        for (path, expected) in paths.iter().zip(bytes) {
            assert_eq!(std::fs::read(path).unwrap(), expected);
        }
    }
}

#[test]
fn an_audio_drop_in_mastering_still_enters_its_existing_queue() {
    let root = tempfile::tempdir().unwrap();
    let ctx = egui::Context::default();
    let mut app = test_fixture::app(&ctx);
    app.tab = 0;
    let count = app.tracks.len();
    let path = input(root.path(), "Another.wav", 800);
    app.route_file_drop(vec![path.clone()], &ctx);
    assert_eq!(app.tracks.len(), count + 1);
    assert_eq!(app.tracks.last().unwrap().path, path);
    assert!(!app.studio.busy());
}

#[test]
fn mixed_zip_and_loose_drop_reports_a_clear_error_without_starting_work() {
    let ctx = egui::Context::default();
    let mut app = test_fixture::app(&ctx);
    app.tab = 3;
    app.studio.page = Page::Tracks;
    app.route_file_drop(vec!["Parts.zip".into(), "Extra.wav".into()], &ctx);
    assert!(!app.studio.busy());
    assert!(
        app.studio
            .error
            .as_deref()
            .unwrap()
            .contains("one audio ZIP")
    );
    assert!(app.studio.bulk_import.is_none());
}
