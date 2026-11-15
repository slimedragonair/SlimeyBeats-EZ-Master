use super::*;
use crate::studio::{Revision, StemSet};
use mastercraft::audio::Audio;
use serde_json::{Value, json};

fn bank(rate: u32, channels: u32) -> BrowserStudio {
    let mut studio = BrowserStudio::new();
    for (kind, scale) in [("vocals", 0.01), ("part_24", 0.02)] {
        let samples = (0..rate / 10)
            .flat_map(|frame| {
                (0..channels).map(move |channel| {
                    let wave = [-0., 1., -0.5, 0.25, -1.][frame as usize % 5];
                    wave * scale * if channel == 0 { 1. } else { -0.5 }
                })
            })
            .collect();
        studio.stage_stem(kind, samples, rate, channels).unwrap();
    }
    studio.commit_stems().unwrap();
    studio
        .set_stem_display_name("vocals", "Lead vocal")
        .unwrap();
    studio
        .set_stem_display_name("part_24", "Backing take 24")
        .unwrap();
    studio
}

fn edit(operation: Value) -> String {
    json!({"start_seconds":0.02,"end_seconds":0.06,"operation":operation}).to_string()
}

fn identity(audio: &Option<Arc<Audio>>) -> Option<usize> {
    audio.as_ref().map(|audio| Arc::as_ptr(audio) as usize)
}

fn bank_state(bank: &Option<StemSet>) -> Value {
    json!(bank.as_ref().map(|bank| {
        bank.tracks
            .iter()
            .map(|stem| {
                json!([
                    stem.kind,
                    bank.display_name(stem.kind),
                    Arc::as_ptr(&stem.audio) as usize
                ])
            })
            .collect::<Vec<_>>()
    }))
}

fn revision_state(revision: &Revision) -> Value {
    json!({"audio":Arc::as_ptr(&revision.audio) as usize,
        "before":identity(&revision.before),"bank":bank_state(&revision.bank),
        "mix":revision.mix,"notes":revision.notes,"is_mix":revision.preview_is_mix})
}

fn state(studio: &BrowserStudio) -> Value {
    json!({"info":studio.info().unwrap(),"source":identity(&studio.source),
        "preview":identity(&studio.current),"before":identity(&studio.before),
        "bank":bank_state(&studio.bank),
        "staged":studio.staged.iter().map(|stem| (stem.kind, Arc::as_ptr(&stem.audio) as usize)).collect::<Vec<_>>(),
        "undo":studio.undo.iter().map(revision_state).collect::<Vec<_>>(),
        "redo":studio.redo.iter().map(revision_state).collect::<Vec<_>>()})
}

fn bits(audio: &Audio) -> Vec<u32> {
    audio
        .samples
        .iter()
        .map(|sample| sample.to_bits())
        .collect()
}

#[test]
fn shared_operations_preserve_layout_other_stems_names_and_current_mix_settings() {
    for rate in [8000, 48000] {
        for channels in [1, 2] {
            for operation in [
                json!({"kind":"silence"}),
                json!({"kind":"fade_in"}),
                json!({"kind":"fade_out"}),
                json!({"kind":"gain","db":-60.}),
                json!({"kind":"gain","db":12.}),
                json!({"kind":"reverse"}),
            ] {
                let mut studio = bank(rate, channels);
                let mut settings = studio.mix.clone();
                settings.output_gain_db = -3.;
                settings.tracks[0].gain_db = -2.;
                settings.tracks[0].balance = if channels == 2 { 0.3 } else { 0. };
                settings.tracks[1].gain_db = -9.;
                studio
                    .mix(&serde_json::to_string(&settings).unwrap())
                    .unwrap();
                let original = studio.audio("part_24").unwrap();
                let other = studio.audio("vocals").unwrap();
                let source = studio.source.clone();
                let names = bank_state(&studio.bank);
                let json = edit(operation);
                let expected =
                    region_edit::render(&original, &serde_json::from_str(&json).unwrap(), &|| {
                        false
                    })
                    .unwrap();

                // Exercise the exported method as well as the checked helper.
                studio.edit_region("part_24", &json).unwrap();
                let result = studio.audio("part_24").unwrap();
                assert!(!Arc::ptr_eq(&original, &result));
                assert_eq!(bits(&result), bits(&expected));
                assert_eq!(result.sample_rate, rate);
                assert_eq!(result.channels, channels as usize);
                assert_eq!(result.frames(), original.frames());
                for frame in
                    (0..rate as usize / 50).chain(rate as usize * 3 / 50..original.frames())
                {
                    for channel in 0..channels as usize {
                        let index = frame * channels as usize + channel;
                        assert_eq!(
                            result.samples[index].to_bits(),
                            original.samples[index].to_bits()
                        );
                    }
                }
                assert!(Arc::ptr_eq(&studio.audio("vocals").unwrap(), &other));
                assert_eq!(identity(&studio.source), identity(&source));
                assert_eq!(identity(&studio.before), identity(&source));
                let current_names = bank_state(&studio.bank);
                for index in 0..2 {
                    assert_eq!(current_names[index][1], names[index][1]);
                }
                assert_eq!(
                    serde_json::to_value(&studio.mix).unwrap(),
                    serde_json::to_value(&settings).unwrap()
                );
                let mixed =
                    stem_mix::render(studio.bank.as_ref().unwrap(), &settings, &|| false).unwrap();
                assert_eq!(bits(studio.current.as_ref().unwrap()), bits(&mixed));
                assert!(studio.preview_is_mix);
                assert!(studio.direct_edit_stems().is_empty());
            }
        }
    }
}

#[test]
fn undo_redo_restore_banks_comparison_and_direct_preview_before_region() {
    let mut studio = bank(8000, 2);
    let old_bank = bank_state(&studio.bank);
    let source = identity(&studio.source);
    studio.before = Some(studio.audio("vocals").unwrap());
    studio.current = Some(Arc::new(Audio::new(vec![0.03; 1600], 8000, 2).unwrap()));
    studio.preview_is_mix = false;
    studio.notes = vec!["Previous direct edit".into()];
    let old_preview = identity(&studio.current);
    let old_before = identity(&studio.before);

    studio
        .edit_region_checked("part_24", &edit(json!({"kind":"reverse"})))
        .unwrap();
    let edited_bank = bank_state(&studio.bank);
    let edited_preview = identity(&studio.current);
    assert_ne!(edited_bank, old_bank);
    assert!(studio.preview_is_mix);
    assert!(studio.notes.is_empty());
    assert_eq!(identity(&studio.before), source);

    studio.history(false).unwrap();
    assert_eq!(bank_state(&studio.bank), old_bank);
    assert_eq!(identity(&studio.current), old_preview);
    assert_eq!(identity(&studio.before), old_before);
    assert!(!studio.preview_is_mix);
    assert_eq!(studio.notes, ["Previous direct edit"]);
    studio.history(true).unwrap();
    assert_eq!(bank_state(&studio.bank), edited_bank);
    assert_eq!(identity(&studio.current), edited_preview);
    assert_eq!(identity(&studio.before), source);
    assert!(studio.preview_is_mix);
    assert!(studio.notes.is_empty());
    assert_eq!(identity(&studio.source), source);
}

#[test]
fn invalid_kind_operation_json_range_and_gain_leave_all_state_unchanged() {
    let mut studio = bank(8000, 1);
    studio
        .edit_region_checked("vocals", &edit(json!({"kind":"reverse"})))
        .unwrap();
    studio.history(false).unwrap();
    studio
        .stage_stem("part_25", vec![0.01; 800], 8000, 1)
        .unwrap();
    let original = state(&studio);
    let valid = edit(json!({"kind":"silence"}));
    for kind in [
        "unknown", "part_0", "part_024", "drums", "source", "preview",
    ] {
        assert!(studio.edit_region_checked(kind, &valid).is_err(), "{kind}");
        assert_eq!(state(&studio), original);
    }
    for json in [
        "{}".to_string(),
        "not JSON".to_string(),
        edit(json!({"kind":"unknown"})),
        edit(json!({"kind":"silence","db":0})),
        edit(json!({"kind":"gain","db":-60.001})),
        edit(json!({"kind":"gain","db":12.001})),
        edit(json!({"kind":"gain","db":null})),
        edit(json!({"kind":"gain","db":0})).replace("\"db\":0", "\"db\":1e309"),
        valid.replace("0.02", "-0.02"),
        valid.replace("0.06", "0.02"),
        valid.replace("0.06", "0.01"),
        valid.replace("0.06", "0.1001"),
        valid.replace("0.06", "0.020001"),
    ] {
        assert!(
            studio.edit_region_checked("vocals", &json).is_err(),
            "{json}"
        );
        assert_eq!(state(&studio), original);
    }
}

#[test]
fn missing_bank_or_failed_mix_cannot_commit_an_edited_bank() {
    let json = edit(json!({"kind":"silence"}));
    let mut studio = BrowserStudio::new();
    let original = state(&studio);
    assert!(studio.edit_region_checked("vocals", &json).is_err());
    assert_eq!(state(&studio), original);
    studio.load(vec![0.01; 800], 8000, 1).unwrap();
    let original = state(&studio);
    assert!(studio.edit_region_checked("vocals", &json).is_err());
    assert_eq!(state(&studio), original);

    let mut studio = bank(8000, 1);
    studio.mix.output_gain_db = f64::INFINITY;
    let original = state(&studio);
    assert!(studio.edit_region_checked("vocals", &json).is_err());
    assert_eq!(state(&studio), original);
}

#[test]
fn discarded_region_fork_preserves_every_committed_field_and_history() {
    let mut studio = bank(8000, 2);
    studio
        .edit_region_checked("vocals", &edit(json!({"kind":"reverse"})))
        .unwrap();
    studio.history(false).unwrap();
    let original = state(&studio);
    let mut candidate = studio.fork();
    candidate
        .edit_region_checked("part_24", &edit(json!({"kind":"silence"})))
        .unwrap();
    assert_ne!(state(&candidate), original);
    assert_eq!(state(&studio), original);
    drop(candidate);
    assert_eq!(state(&studio), original);
}

#[test]
fn repeated_regions_keep_the_existing_two_revision_undo_limit_and_bank_budget() {
    let mut studio = bank(8000, 1);
    let original_bytes: usize = studio
        .bank
        .as_ref()
        .unwrap()
        .tracks
        .iter()
        .map(|stem| stem.audio.samples.len() * size_of::<f32>())
        .sum();
    for _ in 0..4 {
        studio
            .edit_region_checked("vocals", &edit(json!({"kind":"reverse"})))
            .unwrap();
    }
    assert_eq!(studio.undo.len(), 2);
    assert!(studio.redo.is_empty());
    let current_bytes: usize = studio
        .bank
        .as_ref()
        .unwrap()
        .tracks
        .iter()
        .map(|stem| stem.audio.samples.len() * size_of::<f32>())
        .sum();
    assert_eq!(current_bytes, original_bytes);
    let info: Value = serde_json::from_str(&studio.info().unwrap()).unwrap();
    assert_eq!(info["max_combined_decoded_bytes"], 128 * 1024 * 1024);
    assert_eq!(info["max_stem_seconds"], 900.);
    studio.history(false).unwrap();
    studio.history(false).unwrap();
    assert!(studio.undo.is_empty());
}
