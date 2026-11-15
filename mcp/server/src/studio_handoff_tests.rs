use super::*;
use crate::{
    arguments::Arguments,
    state::{Engine, Input},
    studio_state::{AudioTarget, EditedAudio, StemBank, StudioOrigin},
};
use clap::Parser;
use mastercraft::{
    audio::Audio,
    session_document::SessionSource,
    source_identity::SourceIdentity,
    studio::stem_mix::{Stem, StemKind, StemSet},
};
use std::sync::atomic::AtomicBool;

fn audio(frames: usize) -> Arc<Audio> {
    Arc::new(Audio::new(vec![0.125; frames], 8000, 1).unwrap())
}

fn bank(source: Arc<Audio>, stem: Arc<Audio>) -> Arc<StemBank> {
    let stems = StemSet::new(
        [StemKind::Vocals, StemKind::Instrumental]
            .into_iter()
            .map(|kind| Stem {
                kind,
                audio: stem.clone(),
            })
            .collect(),
    )
    .unwrap();
    Arc::new(StemBank {
        id: "bank-B".into(),
        source_name: "B.wav".into(),
        controls: stems.defaults(),
        stems,
        model: json!({}),
        original_mix: source,
    })
}

fn context(root: &std::path::Path, origin: Arc<Audio>, before: Arc<Audio>) -> ContextData {
    let args = Arguments::parse_from([
        "test",
        "--input-root",
        root.to_str().unwrap(),
        "--output-root",
        root.join("exports").to_str().unwrap(),
    ]);
    let engine = Engine::new(&args).unwrap();
    let mut state = engine.lock().unwrap().session.clone();
    state.studio.current = Some(Arc::new(EditedAudio {
        id: "preview-A".into(),
        label: "Edited A".into(),
        audio: audio(8000),
        before,
        file: None,
        steps: vec![],
        notes: vec![],
        bank_snapshot: None,
        origin_snapshot: StudioOrigin {
            id: "source-A".into(),
            label: "A.wav".into(),
            audio: origin,
        },
    }));
    ContextData {
        engine,
        id: "test".into(),
        snapshot: state,
        cancel: Arc::new(AtomicBool::new(false)),
    }
}

fn rejects_without_persisting(ctx: ContextData, expected: &str) {
    let output = ctx.engine.policy.output.clone();
    let error = match save(ctx, false) {
        Err(error) => error,
        Ok(_) => panic!("Invalid project was saved"),
    };
    assert!(error.to_string().contains(expected), "{error:#}");
    assert!(std::fs::read_dir(output).unwrap().all(|entry| {
        !entry
            .unwrap()
            .file_name()
            .to_string_lossy()
            .starts_with("Studio-session-")
    }));
}

#[test]
fn source_only_preview_excludes_unrelated_bank_even_with_identical_pcm_and_layout() {
    let root = tempfile::tempdir().unwrap();
    let origin = audio(8000);
    let mut ctx = context(root.path(), origin.clone(), origin.clone());
    ctx.snapshot.studio.bank = Some(bank(audio(8000), audio(8000)));
    let saved = snapshot(&ctx).unwrap();
    assert!(Arc::ptr_eq(&saved.source, &origin));
    assert_eq!(saved.source_name, "A.wav");
    assert!(saved.stems.is_none());
    assert!(saved.mix.tracks.is_empty());
}

#[test]
fn source_with_actual_bank_identity_retains_bank_association() {
    let root = tempfile::tempdir().unwrap();
    let origin = audio(8000);
    let mut ctx = context(root.path(), origin.clone(), origin.clone());
    let bank = bank(origin.clone(), audio(8000));
    ctx.snapshot.studio.bank = Some(bank.clone());
    ctx.snapshot.source = Some(Arc::new(Input {
        id: "loaded-source".into(),
        audio: origin,
        source: SessionSource {
            path: "A.wav".into(),
            identity: SourceIdentity {
                bytes: 0,
                sha256: String::new(),
            },
        },
        generated: None,
        studio_preview: None,
    }));
    let target = ctx.snapshot.studio_audio("loaded-source").unwrap();
    assert!(Arc::ptr_eq(&target.bank_snapshot.unwrap(), &bank));
    assert!(Arc::ptr_eq(&target.origin.audio, &bank.original_mix));
    assert_eq!(AudioTarget::bank_mix(&bank, "Mix").origin.label, "B.wav");
}

#[test]
fn invalid_origin_is_rejected_before_persistence() {
    let root = tempfile::tempdir().unwrap();
    rejects_without_persisting(
        context(root.path(), audio(4000), audio(8000)),
        "source and preview",
    );
}

#[test]
fn invalid_comparison_is_rejected_before_persistence() {
    let root = tempfile::tempdir().unwrap();
    rejects_without_persisting(
        context(root.path(), audio(8000), audio(4000)),
        "comparison assets",
    );
}

#[test]
fn wrong_bank_origin_is_rejected_even_with_identical_pcm_and_layout() {
    let root = tempfile::tempdir().unwrap();
    let mut ctx = context(root.path(), audio(8000), audio(8000));
    Arc::get_mut(ctx.snapshot.studio.current.as_mut().unwrap())
        .unwrap()
        .bank_snapshot = Some(bank(audio(8000), audio(8000)));
    rejects_without_persisting(ctx, "origin does not match");
}

#[test]
fn unaligned_bank_is_rejected_before_persistence() {
    let root = tempfile::tempdir().unwrap();
    let origin = audio(8000);
    let mut ctx = context(root.path(), origin.clone(), origin.clone());
    Arc::get_mut(ctx.snapshot.studio.current.as_mut().unwrap())
        .unwrap()
        .bank_snapshot = Some(bank(origin, audio(4000)));
    rejects_without_persisting(ctx, "stem bank is not aligned");
}

#[test]
fn region_mix_project_reopens_with_original_edited_stem_comparison_and_live_controls() {
    use crate::{region_edit_operations, region_edit_test_support::Workspace};
    use mastercraft::studio::region_edit::{RegionEdit, RegionOperation};
    let workspace = Workspace::new();
    let comparison = workspace
        .engine
        .lock()
        .unwrap()
        .session
        .studio
        .current
        .as_ref()
        .unwrap()
        .audio
        .clone();
    workspace.commit(
        region_edit_operations::render(
            workspace.context("region-project"),
            workspace.bank.id.clone(),
            StemKind::part(64).unwrap(),
            RegionEdit {
                start_seconds: 0.05,
                end_seconds: 0.15,
                operation: RegionOperation::Silence,
            },
        )
        .unwrap(),
    );
    let expected = snapshot(&workspace.context("snapshot")).unwrap();
    let saved = save(workspace.context("save"), false).unwrap();
    let path = workspace
        .engine
        .policy
        .output
        .join(saved.result["project_path"].as_str().unwrap());
    let restored = session::StudioDocument::load(&path).unwrap();
    assert_eq!(restored.view, "stems");
    assert_eq!(restored.source_name, workspace.bank.source_name);
    assert_eq!(restored.source.samples, workspace.bank.original_mix.samples);
    assert_eq!(restored.before.samples, comparison.samples);
    assert_eq!(restored.preview.samples, expected.preview.samples);
    assert_eq!(restored.notes, expected.notes);
    assert_eq!(
        serde_json::to_value(&restored.mix).unwrap(),
        serde_json::to_value(&workspace.bank.controls).unwrap()
    );
    let stems = restored.stems.unwrap();
    assert_eq!(
        stems.display_name(StemKind::part(64).unwrap()),
        "Backing vocal take"
    );
    for stem in &stems.tracks {
        let actual = &stem.audio;
        let expected = &expected
            .stems
            .as_ref()
            .unwrap()
            .track(stem.kind)
            .unwrap()
            .audio;
        assert_eq!(actual.samples, expected.samples);
        assert_eq!(
            (actual.sample_rate, actual.channels, actual.frames()),
            (8000, 2, 1600)
        );
    }
    let remixed = mastercraft::studio::stem_mix::render(&stems, &restored.mix, &|| false).unwrap();
    assert_eq!(remixed.samples, restored.preview.samples);
}
