use super::*;
use crate::{region_edit_test_support::Workspace, studio_state::stem_id};
use mastercraft::studio::{doctor::RepairAction, stem_mix::StemKind};
use std::sync::{Arc, atomic::Ordering};

fn request(strength: f64) -> RepairRequest {
    RepairRequest {
        action: RepairAction::ReduceLowMids,
        start_seconds: 0.05,
        end_seconds: 0.15,
        strength,
    }
}

#[test]
fn scan_reports_exact_target_without_mutating_audio_bank_or_history() {
    let workspace = Workspace::new();
    let before = workspace.engine.session().unwrap();
    let files_before: Vec<_> = std::fs::read_dir(workspace.root.path().join("exports"))
        .unwrap()
        .map(|entry| entry.unwrap().path())
        .collect();
    let id = stem_id(&workspace.bank.id, StemKind::part(64).unwrap());
    let product = scan(workspace.context("scan"), id.clone()).unwrap();
    assert!(matches!(product.change, Change::None));
    assert!(!product.committed);
    assert_eq!(product.result["audio_id"], id);
    assert_eq!(product.result["report"]["schema_version"], 1);
    assert_eq!(product.result["report"]["sample_rate"], 8000);
    assert!(product.result["report"]["issues"].is_array());
    assert!(product.result["report"]["thresholds"].is_object());
    assert_eq!(workspace.engine.session().unwrap(), before);
    let files_after: Vec<_> = std::fs::read_dir(workspace.root.path().join("exports"))
        .unwrap()
        .map(|entry| entry.unwrap().path())
        .collect();
    assert_eq!(files_before, files_after);
}

#[test]
fn repair_is_a_direct_temporary_preview_and_preserves_bank_and_outside_samples() {
    let workspace = Workspace::new();
    let before = workspace.engine.session().unwrap();
    let kind = StemKind::part(64).unwrap();
    let input = &workspace.bank.stems.track(kind).unwrap().audio;
    let id = stem_id(&workspace.bank.id, kind);
    let product = preview(workspace.context("doctor"), id.clone(), request(0.5)).unwrap();
    assert!(!product.committed);
    let Change::Studio(studio) = &product.change else {
        panic!("Missing preview")
    };
    assert!(Arc::ptr_eq(studio.bank.as_ref().unwrap(), &workspace.bank));
    let current = studio.current.as_ref().unwrap();
    assert!(Arc::ptr_eq(&current.before, input));
    assert_eq!(current.id, "studio-doctor");
    assert_eq!(current.audio.samples.len(), input.samples.len());
    for index in (0..800).chain(2400..3200) {
        assert_eq!(
            current.audio.samples[index].to_bits(),
            input.samples[index].to_bits()
        );
    }
    assert!(
        product.result["doctor"]["changed_samples"]
            .as_u64()
            .unwrap()
            > 0
    );
    assert_eq!(current.steps.last().unwrap()["operation"], "stem_doctor");
    assert_eq!(current.steps.last().unwrap()["input_audio_id"], id);
    assert!(current.file.is_some());
    assert!(
        product.result["next"]
            .as_str()
            .unwrap()
            .contains("apply_preview_to_stem")
    );
    assert_eq!(workspace.engine.session().unwrap(), before);
    workspace.commit(product);
    let store = workspace.engine.lock().unwrap();
    assert_eq!(store.session.studio.undo.len(), 1);
    assert!(store.session.studio.redo.is_empty());
}

#[test]
fn zero_strength_keeps_every_bit_and_cancellation_publishes_nothing() {
    let workspace = Workspace::new();
    let id = stem_id(&workspace.bank.id, StemKind::part(64).unwrap());
    let product = preview(workspace.context("bypass"), id.clone(), request(0.)).unwrap();
    let Change::Studio(studio) = product.change else {
        panic!("Missing preview")
    };
    let current = studio.current.unwrap();
    assert_eq!(product.result["doctor"]["changed_samples"], 0);
    for (actual, original) in current.audio.samples.iter().zip(&current.before.samples) {
        assert_eq!(actual.to_bits(), original.to_bits());
    }
    let before = workspace.engine.session().unwrap();
    let ctx = workspace.context("cancelled");
    ctx.cancel.store(true, Ordering::Relaxed);
    assert!(preview(ctx, id.clone(), request(1.)).is_err());
    let ctx = workspace.context("cancelled-scan");
    ctx.cancel.store(true, Ordering::Relaxed);
    assert!(scan(ctx, id).is_err());
    assert_eq!(workspace.engine.session().unwrap(), before);
}
