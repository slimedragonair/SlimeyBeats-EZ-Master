use super::*;
use crate::{
    audio::Audio,
    studio::arrangement::{
        Arrangement, ArrangementHistory, AssetLibrary, Clip, Edit, Lane, RenderOptions,
    },
};
use std::{collections::BTreeMap, sync::Arc};

pub(crate) fn fixture() -> ArrangementSnapshot {
    let a = Arc::new(Audio::new([-0.0, 0.125, -0.125, 0.25].repeat(32), 8000, 1).unwrap());
    let b = Arc::new(Audio::new(vec![0.025; 64], 8000, 2).unwrap());
    let assets: AssetLibrary = [
        ("a".into(), a.clone()),
        ("b".into(), b),
        ("same-content".into(), Arc::new((*a).clone())),
    ]
    .into_iter()
    .collect();
    let mut document = Arrangement::new(8000, 2);
    let mut lane = Lane::new("lane", "Repeated label");
    lane.clips.push(Clip {
        id: "clip".into(),
        name: None,
        source_id: "a".into(),
        start_frame: 0,
        source_start_frame: 0,
        end_frame: 128,
        gain_db: 0.,
        fade_in_frames: 0,
        fade_out_frames: 0,
    });
    document.lanes.push(lane);
    let mut history = ArrangementHistory::new(document.clone(), &assets, 32).unwrap();
    history
        .apply(
            &Edit::SetClipGain {
                clip_id: "clip".into(),
                gain_db: -3.,
            },
            &assets,
        )
        .unwrap();
    history.undo().unwrap();
    let mut snapshot = ArrangementSnapshot::new(
        history.snapshot(),
        assets,
        [
            ("a".into(), "Repeated label".into()),
            ("b".into(), "Repeated label".into()),
            ("same-content".into(), "Repeated label".into()),
        ]
        .into_iter()
        .collect::<BTreeMap<_, _>>(),
        RenderOptions::default(),
    )
    .unwrap();
    snapshot.origin = Some(document.clone());
    snapshot.before = Some(document);
    snapshot.redo_labels = vec!["Update clip gain".into()];
    snapshot
}

#[test]
fn canonical_manifest_and_hash_checked_pool_roundtrip_without_aliasing_logical_sources() {
    let snapshot = fixture();
    let mut bytes = BTreeMap::new();
    let sources = snapshot
        .assets
        .iter()
        .map(|(id, audio)| {
            let wav = encode_source(audio).unwrap();
            let descriptor =
                describe_source(id.clone(), snapshot.asset_names[id].clone(), audio, &wav).unwrap();
            bytes.insert(id.clone(), wav);
            descriptor
        })
        .collect();
    let manifest = snapshot.manifest(sources).unwrap();
    let parsed = ArrangementManifest::from_json(&manifest.to_json().unwrap()).unwrap();
    let mut loader = ProjectLoader::new(parsed, 1024 * 1024).unwrap();
    for (id, wav) in bytes {
        loader.add_source(&id, &wav).unwrap();
    }
    let loaded = loader.finish().unwrap();
    assert_eq!(
        serde_json::to_value(&loaded.history).unwrap(),
        serde_json::to_value(&snapshot.history).unwrap()
    );
    for (id, audio) in &snapshot.assets {
        assert_eq!(
            loaded.assets[id]
                .samples
                .iter()
                .map(|sample| sample.to_bits())
                .collect::<Vec<_>>(),
            audio
                .samples
                .iter()
                .map(|sample| sample.to_bits())
                .collect::<Vec<_>>()
        );
    }
    assert!(!Arc::ptr_eq(
        &loaded.assets["a"],
        &loaded.assets["same-content"]
    ));
    assert_eq!(loaded.redo_labels, snapshot.redo_labels);
}

#[test]
fn pool_hash_tampering_paths_wrong_geometry_and_missing_assets_fail_closed() {
    let snapshot = fixture();
    let mut records = Vec::new();
    let mut encoded = BTreeMap::new();
    for (id, audio) in &snapshot.assets {
        let bytes = encode_source(audio).unwrap();
        records.push(describe_source(id.clone(), id.clone(), audio, &bytes).unwrap());
        encoded.insert(id.clone(), bytes);
    }
    let manifest = snapshot.manifest(records).unwrap();
    let mut bad = manifest.clone();
    bad.sources[0].file = "../private.wav".into();
    assert!(bad.validate().is_err());
    let mut loader = ProjectLoader::new(manifest.clone(), 1024 * 1024).unwrap();
    let id = manifest.sources[0].id.clone();
    let mut corrupt = encoded[&id].clone();
    let last = corrupt.len() - 1;
    corrupt[last] ^= 1;
    assert!(loader.add_source(&id, &corrupt).is_err());
    assert!(loader.finish().is_err());
    let mut bad = manifest;
    bad.sources[0].frames += 1;
    let mut loader = ProjectLoader::new(bad, 1024 * 1024).unwrap();
    assert!(loader.add_source(&id, &encoded[&id]).is_err());
}
