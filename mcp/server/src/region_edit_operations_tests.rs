use super::*;
use crate::region_edit_test_support::Workspace;
use mastercraft::studio::region_edit::RegionOperation;
use std::sync::atomic::Ordering;

fn edit(operation: RegionOperation) -> RegionEdit {
    RegionEdit {
        start_seconds: 0.05,
        end_seconds: 0.15,
        operation,
    }
}

#[test]
fn edits_replace_only_selected_pcm_and_publish_controlled_whole_mix_with_all_ids() {
    for operation in [
        RegionOperation::Silence,
        RegionOperation::FadeIn,
        RegionOperation::FadeOut,
        RegionOperation::Gain { db: -3. },
        RegionOperation::Reverse,
    ] {
        let workspace = Workspace::new();
        let ctx = workspace.context("region");
        let kind = StemKind::part(64).unwrap();
        let original = &workspace.bank.stems.track(kind).unwrap().audio;
        let comparison = ctx.snapshot.studio.current.as_ref().unwrap().audio.clone();
        let product = render(
            ctx,
            workspace.bank.id.clone(),
            kind,
            edit(operation.clone()),
        )
        .unwrap();
        assert!(!product.committed);
        let Change::Studio(studio) = &product.change else {
            panic!("Missing Studio change")
        };
        let bank = studio.bank.as_ref().unwrap();
        let preview = studio.current.as_ref().unwrap();
        assert_eq!(bank.id, "stems-region");
        assert!(Arc::ptr_eq(
            &bank.original_mix,
            &workspace.bank.original_mix
        ));
        assert_eq!(bank.source_name, workspace.bank.source_name);
        assert_eq!(bank.model, workspace.bank.model);
        assert_eq!(
            serde_json::to_value(&bank.controls).unwrap(),
            serde_json::to_value(&workspace.bank.controls).unwrap()
        );
        assert_eq!(bank.stems.display_name(kind), "Backing vocal take");
        let revised = &bank.stems.track(kind).unwrap().audio;
        assert_eq!(
            (revised.frames(), revised.channels, revised.sample_rate),
            (1600, 2, 8000)
        );
        for index in (0..800).chain(2400..3200) {
            assert_eq!(
                revised.samples[index].to_bits(),
                original.samples[index].to_bits()
            );
        }
        assert_ne!(&revised.samples[800..2400], &original.samples[800..2400]);
        for other in [StemKind::Vocals, StemKind::Drums] {
            assert!(Arc::ptr_eq(
                &bank.stems.track(other).unwrap().audio,
                &workspace.bank.stems.track(other).unwrap().audio
            ));
        }
        match operation {
            RegionOperation::Silence => {
                assert!(revised.samples[800..2400].iter().all(|s| *s == 0.))
            }
            RegionOperation::Reverse => {
                assert_eq!(&revised.samples[800..802], &original.samples[2398..2400])
            }
            RegionOperation::FadeIn => {
                assert_eq!(&revised.samples[800..802], &[0., 0.]);
                assert_eq!(&revised.samples[2398..2400], &original.samples[2398..2400]);
            }
            RegionOperation::FadeOut => {
                assert_eq!(&revised.samples[800..802], &original.samples[800..802]);
                assert_eq!(&revised.samples[2398..2400], &[0., 0.]);
            }
            RegionOperation::Gain { .. } => assert!(
                (revised.samples[801] as f64 / original.samples[801] as f64
                    - 10_f64.powf(-3. / 20.))
                .abs()
                    < 1e-6
            ),
        }
        let mixed = stem_mix::render(&bank.stems, &bank.controls, &|| false).unwrap();
        assert_eq!(preview.audio.samples, mixed.samples);
        assert_ne!(preview.audio.samples, revised.samples);
        assert_eq!(preview.before.samples, comparison.samples);
        assert_eq!(preview.steps[0]["fixture_provenance"], true);
        assert_eq!(
            preview.steps.last().unwrap()["input_audio_id"],
            "stems-baseline:part_64"
        );
        assert_eq!(product.result["stem_set_id"], bank.id);
        assert_eq!(product.result["stem_set"]["stem_set_id"], bank.id);
        assert_eq!(
            product.result["preview"]["studio_preview_id"],
            product.result["studio_preview_id"]
        );
        for stem in &bank.stems.tracks {
            assert!(
                product.result["assets"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .any(|a| a["audio_id"] == stem_id(&bank.id, stem.kind))
            );
        }
        assert!(
            workspace
                .engine
                .policy
                .output
                .join(product.result["relative_preview_path"].as_str().unwrap())
                .is_file()
        );
        assert_eq!(
            workspace
                .engine
                .lock()
                .unwrap()
                .session
                .studio
                .bank
                .as_ref()
                .unwrap()
                .id,
            workspace.bank.id
        );
    }
}

#[test]
fn history_restores_exact_banks_controls_original_and_preview_and_new_edit_clears_redo() {
    let workspace = Workspace::new();
    let kind = StemKind::part(64).unwrap();
    let before = workspace
        .engine
        .lock()
        .unwrap()
        .session
        .studio
        .current
        .clone()
        .unwrap();
    workspace.commit(
        render(
            workspace.context("first"),
            workspace.bank.id.clone(),
            kind,
            edit(RegionOperation::Silence),
        )
        .unwrap(),
    );
    let revised = workspace
        .engine
        .lock()
        .unwrap()
        .session
        .studio
        .bank
        .clone()
        .unwrap();
    let preview = workspace
        .engine
        .lock()
        .unwrap()
        .session
        .studio
        .current
        .clone()
        .unwrap();
    workspace.commit(studio_operations::history(workspace.context("undo"), false).unwrap());
    {
        let store = workspace.engine.lock().unwrap();
        assert!(Arc::ptr_eq(
            store.session.studio.bank.as_ref().unwrap(),
            &workspace.bank
        ));
        assert!(Arc::ptr_eq(
            store.session.studio.current.as_ref().unwrap(),
            &before
        ));
        assert!(Arc::ptr_eq(
            &store.session.studio.origin.as_ref().unwrap().audio,
            &workspace.bank.original_mix
        ));
    }
    workspace.commit(studio_operations::history(workspace.context("redo"), true).unwrap());
    {
        let store = workspace.engine.lock().unwrap();
        assert!(Arc::ptr_eq(
            store.session.studio.bank.as_ref().unwrap(),
            &revised
        ));
        assert!(Arc::ptr_eq(
            store.session.studio.current.as_ref().unwrap(),
            &preview
        ));
    }
    workspace.commit(studio_operations::history(workspace.context("undo-again"), false).unwrap());
    workspace.commit(
        render(
            workspace.context("branch"),
            workspace.bank.id.clone(),
            kind,
            edit(RegionOperation::Reverse),
        )
        .unwrap(),
    );
    assert!(
        workspace
            .engine
            .lock()
            .unwrap()
            .session
            .studio
            .redo
            .is_empty()
    );
}

#[test]
fn cancelled_render_and_invalid_pcm_preserve_bank_and_temporary_files() {
    let workspace = Workspace::new();
    let before = workspace.engine.session().unwrap();
    let entries = std::fs::read_dir(workspace.engine.cache.path())
        .unwrap()
        .count();
    let ctx = workspace.context("cancelled");
    ctx.cancel.store(true, Ordering::Relaxed);
    assert!(
        render(
            ctx,
            workspace.bank.id.clone(),
            StemKind::Vocals,
            edit(RegionOperation::Silence)
        )
        .err()
        .unwrap()
        .to_string()
        .contains("Cancelled")
    );
    let mut ctx = workspace.context("invalid");
    let mut stems = workspace.bank.stems.clone();
    let mut bad_audio = (*stems.tracks[0].audio).clone();
    bad_audio.samples[100] = f32::NAN;
    stems.tracks[0].audio = Arc::new(bad_audio);
    ctx.snapshot.studio.bank = Some(Arc::new(StemBank {
        id: workspace.bank.id.clone(),
        source_name: workspace.bank.source_name.clone(),
        stems,
        controls: workspace.bank.controls.clone(),
        model: workspace.bank.model.clone(),
        original_mix: workspace.bank.original_mix.clone(),
    }));
    assert!(
        render(
            ctx,
            workspace.bank.id.clone(),
            StemKind::Vocals,
            edit(RegionOperation::Silence)
        )
        .err()
        .unwrap()
        .to_string()
        .contains("NaN")
    );
    assert_eq!(workspace.engine.session().unwrap(), before);
    assert_eq!(
        std::fs::read_dir(workspace.engine.cache.path())
            .unwrap()
            .count(),
        entries
    );
    assert!(!workspace.root.path().join("unused-runtime").exists());
}

#[test]
fn chained_regions_keep_committed_provenance_but_ignore_unapplied_stem_preview() {
    let workspace = Workspace::new();
    let kind = StemKind::part(64).unwrap();
    workspace.commit(
        render(
            workspace.context("first"),
            workspace.bank.id.clone(),
            kind,
            edit(RegionOperation::Silence),
        )
        .unwrap(),
    );
    let bank = workspace
        .engine
        .lock()
        .unwrap()
        .session
        .studio
        .bank
        .clone()
        .unwrap();
    workspace.commit(studio_operations::repair(workspace.context("unapplied"),stem_id(&bank.id,kind),serde_json::from_value(json!({"start_seconds":0.,"end_seconds":0.2,"low_hz":0.,"high_hz":4000.,"reduction_db":0.,"mode":"attenuate"})).unwrap()).unwrap());
    let product = render(
        workspace.context("second"),
        bank.id.clone(),
        kind,
        edit(RegionOperation::Reverse),
    )
    .unwrap();
    let Change::Studio(studio) = product.change else {
        panic!("Missing Studio change")
    };
    let preview = studio.current.as_ref().unwrap();
    assert_eq!(preview.steps.len(), 3);
    assert_eq!(preview.steps[0]["fixture_provenance"], true);
    assert_eq!(preview.steps[1]["operation"], "edit_stem_region");
    assert_eq!(preview.steps[2]["operation"], "edit_stem_region");
    assert!(
        !preview
            .steps
            .iter()
            .any(|s| s["operation"] == "spectral_repair")
    );
}
