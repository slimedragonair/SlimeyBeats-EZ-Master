use mastercraft::{audio::Audio, studio::arrangement::*};
use std::sync::Arc;

fn fixture() -> (Arrangement, AssetLibrary) {
    let assets = AssetLibrary::from([
        (
            "short".into(),
            Arc::new(Audio::new(vec![0.1; 4000], 8000, 1).unwrap()),
        ),
        (
            "long".into(),
            Arc::new(Audio::new(vec![0.2; 8000], 8000, 1).unwrap()),
        ),
    ]);
    let mut value = Arrangement::new(8000, 2);
    let mut lane = Lane::new("lane", "Voice");
    lane.clips.push(Clip {
        id: "first".into(),
        name: None,
        source_id: "short".into(),
        start_frame: 100,
        source_start_frame: 500,
        end_frame: 3100,
        gain_db: 0.,
        fade_in_frames: 0,
        fade_out_frames: 0,
    });
    value.lanes.push(lane);
    value.lanes.push(Lane::new("second", "Voice"));
    (value, assets)
}

#[test]
fn a_thousand_lanes_and_bulk_edits_are_limited_by_metadata_bytes_not_part_count() {
    let (_, assets) = fixture();
    let initial = Arrangement::new(8000, 2);
    let edits: Vec<_> = (0..1000)
        .flat_map(|i| {
            [
                Edit::AddLane {
                    lane: Lane::new(format!("lane_{i}"), "Part"),
                },
                Edit::AddClip {
                    lane_id: format!("lane_{i}"),
                    clip: Clip {
                        id: format!("clip_{i}"),
                        name: None,
                        source_id: "short".into(),
                        start_frame: 0,
                        source_start_frame: 0,
                        end_frame: 100,
                        gain_db: 0.,
                        fade_in_frames: 0,
                        fade_out_frames: 0,
                    },
                },
            ]
        })
        .collect();
    let result = apply_edits(&initial, &edits, &assets).unwrap();
    assert_eq!(result.lanes.len(), 1000);
    assert_eq!(result.duration_frames(), 100);
    assert_eq!(Arc::strong_count(&assets["short"]), 1);
}

#[test]
fn differing_durations_mono_upmix_and_roundtrip_need_no_pcm_copies() {
    let (value, assets) = fixture();
    let count = Arc::strong_count(&assets["short"]);
    value.validate(&assets).unwrap();
    let copy: Arrangement = serde_json::from_slice(&serde_json::to_vec(&value).unwrap()).unwrap();
    assert_eq!(copy, value);
    assert_eq!(value.duration_frames(), 3100);
    assert_eq!(
        value.clip("first").unwrap().source_end_frame().unwrap(),
        3500
    );
    assert_eq!(Arc::strong_count(&assets["short"]), count);
    let mut mono = value.clone();
    mono.channels = 1;
    let mut stereo_assets = assets.clone();
    stereo_assets.insert(
        "short".into(),
        Arc::new(Audio::new(vec![0.1; 8000], 8000, 2).unwrap()),
    );
    assert!(mono.validate(&stereo_assets).is_err());
}

#[test]
fn moves_trims_splits_duplicates_and_deletes_preserve_contiguous_source_coordinates() {
    let (value, assets) = fixture();
    let moved = apply_edit(
        &value,
        &Edit::MoveClip {
            clip_id: "first".into(),
            lane_id: "second".into(),
            start_frame: 1000,
        },
        &assets,
    )
    .unwrap();
    assert_eq!(moved.clip("first").unwrap().end_frame, 4000);
    assert_eq!(moved.clip("first").unwrap().source_start_frame, 500);
    let trimmed = apply_edit(
        &moved,
        &Edit::TrimClip {
            clip_id: "first".into(),
            start_frame: 1500,
            source_start_frame: 1000,
            end_frame: 3500,
        },
        &assets,
    )
    .unwrap();
    let split = apply_edit(
        &trimmed,
        &Edit::SplitClip {
            clip_id: "first".into(),
            at_frame: 2500,
            new_clip_id: "right".into(),
        },
        &assets,
    )
    .unwrap();
    assert_eq!(
        split.clip("first").unwrap().source_end_frame().unwrap(),
        split.clip("right").unwrap().source_start_frame
    );
    let duplicated = apply_edit(
        &split,
        &Edit::DuplicateClip {
            clip_id: "right".into(),
            new_clip_id: "copy".into(),
            lane_id: "lane".into(),
            start_frame: 5000,
        },
        &assets,
    )
    .unwrap();
    assert_eq!(duplicated.clip("copy").unwrap().source_start_frame, 2000);
    assert_eq!(duplicated.clip("copy").unwrap().end_frame, 6000);
    let deleted = apply_edit(
        &duplicated,
        &Edit::DeleteClip {
            clip_id: "copy".into(),
        },
        &assets,
    )
    .unwrap();
    assert!(deleted.clip("copy").is_err());
    assert_eq!(value.clip("first").unwrap().start_frame, 100);
    assert!(assets["short"].samples.iter().all(|&sample| sample == 0.1));
}

#[test]
fn invalid_transactions_are_atomic_and_cover_ids_layout_finiteness_bounds_and_overflow() {
    let (value, assets) = fixture();
    let before = serde_json::to_vec(&value).unwrap();
    for edit in [
        Edit::MoveClip {
            clip_id: "first".into(),
            lane_id: "second".into(),
            start_frame: u64::MAX,
        },
        Edit::TrimClip {
            clip_id: "first".into(),
            start_frame: 100,
            source_start_frame: 3999,
            end_frame: 300,
        },
        Edit::DuplicateClip {
            clip_id: "first".into(),
            new_clip_id: "first".into(),
            lane_id: "second".into(),
            start_frame: 0,
        },
        Edit::SetClipGain {
            clip_id: "first".into(),
            gain_db: f64::NAN,
        },
        Edit::ReorderLane {
            lane_id: "lane".into(),
            index: 2,
        },
    ] {
        assert!(
            apply_edits(
                &value,
                &[
                    Edit::DeleteClip {
                        clip_id: "first".into()
                    },
                    edit.clone()
                ],
                &assets
            )
            .is_err()
        );
        assert!(apply_edit(&value, &edit, &assets).is_err());
        assert_eq!(before, serde_json::to_vec(&value).unwrap());
    }
}

#[test]
fn history_is_metadata_only_bounded_and_rejects_failed_edits_without_losing_redo() {
    let (value, assets) = fixture();
    let count = Arc::strong_count(&assets["short"]);
    let mut history = ArrangementHistory::new(value.clone(), &assets, 2).unwrap();
    for gain in [-1., -2., -3.] {
        history
            .apply(
                &Edit::SetClipGain {
                    clip_id: "first".into(),
                    gain_db: gain,
                },
                &assets,
            )
            .unwrap();
    }
    assert_eq!(history.undo().unwrap().clip("first").unwrap().gain_db, -2.);
    assert_eq!(history.undo().unwrap().clip("first").unwrap().gain_db, -1.);
    assert!(history.undo().is_err());
    assert!(
        history
            .apply(
                &Edit::DeleteClip {
                    clip_id: "missing".into()
                },
                &assets
            )
            .is_err()
    );
    assert!(history.can_redo());
    assert_eq!(history.redo().unwrap().clip("first").unwrap().gain_db, -2.);
    assert_eq!(
        history.referenced_sources().into_iter().collect::<Vec<_>>(),
        ["short"]
    );
    assert_eq!(Arc::strong_count(&assets["short"]), count);
}

#[test]
fn scoped_eq_ranges_follow_moves_and_split_without_retargeting_other_lanes() {
    let (mut value, assets) = fixture();
    let mut effect = ScopedEq::new(
        "eq",
        EqScope::Clip {
            clip_id: "first".into(),
        },
    );
    effect.range = Some(FrameRange {
        start_frame: 200,
        end_frame: 3000,
    });
    value.eq.push(effect);
    let moved = apply_edit(
        &value,
        &Edit::MoveClip {
            clip_id: "first".into(),
            lane_id: "second".into(),
            start_frame: 1100,
        },
        &assets,
    )
    .unwrap();
    assert_eq!(moved.eq[0].range, value.eq[0].range);
    let split = apply_edit(
        &moved,
        &Edit::SplitClip {
            clip_id: "first".into(),
            at_frame: 2500,
            new_clip_id: "right".into(),
        },
        &assets,
    )
    .unwrap();
    assert_eq!(split.eq.len(), 2);
    assert_eq!(split.eq[0].range, value.eq[0].range);
    assert_eq!(split.eq[1].range, value.eq[0].range);
    let empty = apply_edit(
        &split,
        &Edit::RemoveLane {
            lane_id: "second".into(),
        },
        &assets,
    )
    .unwrap();
    assert!(empty.eq.is_empty());
    assert_eq!(empty.duration_frames(), 0);
}
