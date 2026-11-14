use super::*;
use serde_json::{Value, json};

const RATE: u32 = 8000;
const FRAMES: usize = 16000;

fn pcm(index: usize) -> Vec<f32> {
    let level = index as f32 / 4096.;
    let wave = [1., 0.5, 0., -0.5, -1., -0.5, 0., 0.5];
    (0..FRAMES)
        .flat_map(|frame| {
            let sample = level * wave[frame % wave.len()];
            [sample, -sample / 2.]
        })
        .collect()
}

fn bank(count: usize) -> BrowserStudio {
    let mut studio = BrowserStudio::new();
    for index in 1..=count {
        studio
            .stage_stem(&format!("part_{index}"), pcm(index), RATE, 2)
            .unwrap();
    }
    studio.commit_stems().unwrap();
    studio
}

fn wav_samples(bytes: Vec<u8>) -> Vec<f32> {
    let mut reader = hound::WavReader::new(Cursor::new(bytes)).unwrap();
    assert_eq!(reader.spec().sample_rate, RATE);
    assert_eq!(reader.spec().channels, 2);
    assert_eq!(reader.spec().sample_format, hound::SampleFormat::Float);
    assert_eq!(reader.spec().bits_per_sample, 32);
    reader.samples::<f32>().map(Result::unwrap).collect()
}

fn verify_large_binding_bank(count: usize) {
    let mut studio = bank(count);
    let ids: Vec<_> = (1..=count).map(|index| format!("part_{index}")).collect();
    for (index, kind) in ids.iter().enumerate() {
        studio
            .set_stem_display_name(kind, &format!("Independent layer {}", index + 1))
            .unwrap();
    }
    let initial: Value = serde_json::from_str(&studio.info().unwrap()).unwrap();
    assert_eq!(initial["stems"], json!(ids));
    assert_eq!(
        initial["controls"]["tracks"].as_array().unwrap().len(),
        count
    );
    assert!(initial["max_stems"].is_null());
    for (index, kind) in ids.iter().enumerate() {
        assert_eq!(initial["controls"]["tracks"][index]["kind"], *kind);
        assert_eq!(
            initial["stem_names"][kind],
            format!("Independent layer {}", index + 1)
        );
        assert_eq!(wav_samples(studio.stem_wav(kind).unwrap()), pcm(index + 1));
    }
    let unity = wav_samples(studio.current_wav32().unwrap());
    assert_eq!(unity, pcm(count * (count + 1) / 2));

    let mut solo = studio.mix.clone();
    solo.protect_peaks = false;
    solo.tracks.last_mut().unwrap().solo = true;
    let result: Value =
        serde_json::from_str(&studio.mix(&serde_json::to_string(&solo).unwrap()).unwrap()).unwrap();
    assert_eq!(result["stems"], json!(ids));
    assert_eq!(result["stem_names"], initial["stem_names"]);
    assert_eq!(wav_samples(studio.export("wav32f").unwrap()), pcm(count));
    for (index, kind) in ids.iter().enumerate() {
        assert_eq!(wav_samples(studio.stem_wav(kind).unwrap()), pcm(index + 1));
    }
    assert_eq!(
        wav_samples(
            studio
                .history(false)
                .and_then(|_| studio.current_wav32())
                .unwrap()
        ),
        unity
    );
    studio.history(true).unwrap();
    assert_eq!(wav_samples(studio.current_wav32().unwrap()), pcm(count));
}

#[test]
fn twenty_four_binding_stems_keep_identity_mix_and_float_exports() {
    verify_large_binding_bank(24);
}

#[test]
fn sixty_four_binding_stems_keep_identity_mix_and_float_exports() {
    verify_large_binding_bank(64);
}

#[test]
fn sixty_four_loose_files_receive_distinct_string_identities_in_the_binding_plan() {
    let files: Vec<_> = (0..64)
        .map(|index| json!({"name":format!("Guitar take {index:02}.wav"),"size":128}))
        .collect();
    let plan: Value = serde_json::from_str(
        &crate::stem_import::plan_stem_files(&serde_json::to_string(&files).unwrap()).unwrap(),
    )
    .unwrap();
    let parts = plan["parts"].as_array().unwrap();
    assert_eq!(parts.len(), 64);
    let kinds: std::collections::BTreeSet<_> = parts
        .iter()
        .map(|part| {
            let kind: StemKind = serde_json::from_value(part["kind"].clone()).unwrap();
            assert_eq!(serde_json::to_value(kind).unwrap(), part["kind"]);
            part["kind"].as_str().unwrap()
        })
        .collect();
    assert_eq!(kinds.len(), 64);
    assert!(kinds.contains("guitar"));
    assert!(kinds.contains("part_63"));
}

#[test]
fn schema_exposes_resource_limits_without_a_fixed_stem_count() {
    let defaults: Value = serde_json::from_str(&BrowserStudio::parameter_defaults()).unwrap();
    assert_eq!(defaults["min_stems"], 2);
    assert!(defaults["max_stems"].is_null());
    assert_eq!(defaults["max_combined_decoded_bytes"], 128 * 1024 * 1024);
    assert_eq!(defaults["max_stem_seconds"], 900.);
    assert_eq!(defaults["generic_stem_kind_pattern"], "^part_[1-9][0-9]*$");
    for kind in ["vocals", "part_1", "part_24", "part_64", "part_4294967295"] {
        let parsed: StemKind = serde_json::from_value(json!(kind)).unwrap();
        assert_eq!(serde_json::to_value(parsed).unwrap(), kind);
    }
}

#[test]
fn rejected_duplicate_and_misaligned_imports_keep_the_committed_bank() {
    let mut studio = bank(24);
    let original = studio.info().unwrap();
    let preview = studio.current.clone().unwrap();
    studio.stage_stem("part_100", pcm(1), RATE, 2).unwrap();
    let error = studio
        .stage_stem_checked("part_100", pcm(2), RATE, 2)
        .unwrap_err();
    assert!(error.to_string().contains("unique"));
    assert_eq!(studio.staged.len(), 1);
    for invalid in ["part_0", "part_01", "part_4294967296"] {
        assert!(studio.stage_stem_checked(invalid, pcm(1), RATE, 2).is_err());
        assert_eq!(studio.staged.len(), 1);
    }
    studio
        .stage_stem("part_101", vec![0.01; 64], RATE, 2)
        .unwrap();
    let error = studio.commit_stems_checked().unwrap_err();
    assert!(error.to_string().contains("identical"));
    assert_eq!(studio.staged.len(), 2);
    assert_eq!(studio.info().unwrap(), original);
    assert!(Arc::ptr_eq(studio.current.as_ref().unwrap(), &preview));
}

#[test]
fn combined_pcm_budget_rejects_the_next_sample_after_sixty_four_staged_parts() {
    let mut studio = bank(24);
    let original = studio.info().unwrap();
    let preview = studio.current.clone().unwrap();
    let per_stem = MAX_STUDIO_BYTES / size_of::<f32>() / 64;
    for index in 1..=64 {
        studio
            .stage_stem(
                &format!("part_{}", index + 100),
                vec![0.; per_stem],
                48000,
                1,
            )
            .unwrap();
    }
    let error = studio
        .stage_stem_checked("part_1000", vec![0.], 48000, 1)
        .unwrap_err();
    assert!(error.to_string().contains("128 MiB decoded PCM"));
    assert_eq!(studio.staged.len(), 64);
    assert_eq!(studio.info().unwrap(), original);
    assert!(Arc::ptr_eq(studio.current.as_ref().unwrap(), &preview));
    studio.staged.pop();
    studio
        .stage_stem("part_1000", vec![0.; per_stem], 48000, 1)
        .unwrap();
    assert_eq!(studio.staged.len(), 64);
}

#[test]
fn fifteen_minute_guard_preserves_the_existing_preview_on_rejected_staging() {
    let mut studio = bank(24);
    let original = studio.info().unwrap();
    let preview = studio.current.clone().unwrap();
    let boundary = 900 * RATE as usize;
    assert!(BrowserStudio::checked(vec![0.; boundary], RATE, 1).is_ok());
    let error = studio
        .stage_stem_checked("part_100", vec![0.; boundary + 1], RATE, 1)
        .unwrap_err();
    assert!(error.to_string().contains("15 minutes"));
    assert!(studio.staged.is_empty());
    assert_eq!(studio.info().unwrap(), original);
    assert!(Arc::ptr_eq(studio.current.as_ref().unwrap(), &preview));
}
