use super::*;
use crate::{
    audio::Audio,
    studio::{
        arrangement::{Arrangement, ArrangementHistory, AssetLibrary, Clip, Lane, RenderOptions},
        stem_mix::MixSettings,
    },
};
use std::{collections::BTreeMap, sync::Arc};

fn snapshot() -> ArrangementSnapshot {
    let audio = Arc::new(Audio::new([-0.0, 0.125, -0.25, 0.5].repeat(32), 8000, 1).unwrap());
    let assets: AssetLibrary = [("source".into(), audio)].into_iter().collect();
    let mut document = Arrangement::new(8000, 2);
    let mut lane = Lane::new("lane", "Imported track");
    lane.clips.push(Clip {
        id: "clip".into(),
        name: None,
        source_id: "source".into(),
        start_frame: 400,
        source_start_frame: 0,
        end_frame: 528,
        gain_db: 0.,
        fade_in_frames: 0,
        fade_out_frames: 0,
    });
    document.lanes.push(lane);
    let history = ArrangementHistory::new(document, &assets, 32).unwrap();
    ArrangementSnapshot::new(
        history.snapshot(),
        assets,
        BTreeMap::from([("source".into(), "Imported track".into())]),
        RenderOptions::default(),
    )
    .unwrap()
}
#[test]
fn native_v2_saves_reopens_exact_float_sources_and_rejects_tampering() {
    let root = tempfile::tempdir().unwrap();
    let snapshot = snapshot();
    let path = save(&snapshot, root.path(), &|| false).unwrap();
    let loaded = load(&path, 1024 * 1024, &|| false).unwrap();
    assert_eq!(
        serde_json::to_value(&loaded.history).unwrap(),
        serde_json::to_value(&snapshot.history).unwrap()
    );
    assert_eq!(
        loaded.assets["source"].samples[0].to_bits(),
        (-0.0_f32).to_bits()
    );
    assert!(matches!(
        load_any(&path, 1024 * 1024, &|| false).unwrap(),
        LoadedStudioProject::Arrangement(_)
    ));
    let manifest = ArrangementManifest::from_json(&std::fs::read(&path).unwrap()).unwrap();
    let source = path.parent().unwrap().join(&manifest.sources[0].file);
    let mut bytes = std::fs::read(&source).unwrap();
    let last = bytes.len() - 1;
    bytes[last] ^= 1;
    std::fs::write(source, bytes).unwrap();
    assert!(load(&path, 1024 * 1024, &|| false).is_err());
}
#[test]
fn legacy_v1_dispatch_still_uses_its_unchanged_aligned_reader() {
    let root = tempfile::tempdir().unwrap();
    let audio = Arc::new(Audio::new(vec![0.1; 800], 8000, 1).unwrap());
    let legacy = super::super::session::StudioSnapshot {
        source_name: "Legacy".into(),
        source: audio.clone(),
        stems: None,
        mix: MixSettings::default(),
        before: audio.clone(),
        preview: audio,
        label: "Legacy".into(),
        notes: vec![],
        view: "voice".into(),
    };
    let path = super::super::session::save(&legacy, root.path(), &|| false).unwrap();
    assert!(matches!(
        load_any(&path, 1024 * 1024, &|| false).unwrap(),
        LoadedStudioProject::Legacy(_)
    ));
}

#[test]
fn duplicate_content_is_stored_once_and_output_budget_is_local_policy() {
    let root = tempfile::tempdir().unwrap();
    let mut snapshot = snapshot();
    snapshot
        .assets
        .insert("second-id".into(), snapshot.assets["source"].clone());
    snapshot
        .asset_names
        .insert("second-id".into(), "Another name".into());
    snapshot.render_options.max_output_bytes = 4 * 1024 * 1024 * 1024;
    let path = save(&snapshot, root.path(), &|| false).unwrap();
    let manifest = ArrangementManifest::from_json(&std::fs::read(&path).unwrap()).unwrap();
    assert_eq!(manifest.sources.len(), 2);
    assert_eq!(manifest.sources[0].file, manifest.sources[1].file);
    assert_eq!(
        std::fs::read_dir(path.parent().unwrap().join("assets"))
            .unwrap()
            .count(),
        1
    );
    let loaded = load_with_limits(
        &path,
        ProjectLimits {
            max_output_bytes: 1024,
            ..Default::default()
        },
        &|| false,
    )
    .unwrap();
    assert_eq!(loaded.render_options.max_output_bytes, 1024);
    assert_eq!(loaded.assets.len(), 2);
}

#[test]
fn cancelled_or_over_budget_project_operations_leave_no_published_partial_folder() {
    let root = tempfile::tempdir().unwrap();
    assert!(save(&snapshot(), root.path(), &|| true).is_err());
    assert!(
        save_with_limits(
            &snapshot(),
            root.path(),
            ProjectLimits {
                max_asset_bytes: 1,
                ..Default::default()
            },
            &|| false
        )
        .is_err()
    );
    assert_eq!(std::fs::read_dir(root.path()).unwrap().count(), 0);
    let path = save(&snapshot(), root.path(), &|| false).unwrap();
    let mut manifest = ArrangementManifest::from_json(&std::fs::read(&path).unwrap()).unwrap();
    manifest.sources[0].file = "../outside.wav".into();
    std::fs::write(&path, serde_json::to_vec(&manifest).unwrap()).unwrap();
    assert!(load(&path, 1024 * 1024, &|| false).is_err());
}
