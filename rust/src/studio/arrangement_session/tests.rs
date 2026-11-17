use super::*;
use crate::{
    audio::Audio,
    config::default_eq,
    studio::arrangement::{Clip, Edit, EqScope, FrameRange, Lane, ScopedEq},
};
use std::sync::Arc;

fn session() -> ArrangementSession {
    ArrangementSession::new(
        8000,
        2,
        "epoch-one".into(),
        SessionLimits {
            max_decoded_bytes: 1024 * 1024,
            history_capacity: 8,
        },
    )
    .unwrap()
}
fn asset(id: &str, frames: usize) -> OwnedAsset {
    OwnedAsset {
        id: id.into(),
        name: "Repeated label".into(),
        audio: Arc::new(Audio::new(vec![0.1; frames], 8000, 1).unwrap()),
    }
}
fn lane(id: &str, source: &str, frames: u64) -> Lane {
    let mut lane = Lane::new(id, "Repeated label");
    lane.clips.push(Clip {
        id: format!("{id}-clip"),
        name: None,
        source_id: source.into(),
        start_frame: 0,
        source_start_frame: 0,
        end_frame: frames,
        gain_db: 0.,
        fade_in_frames: 0,
        fade_out_frames: 0,
    });
    lane
}

#[test]
fn one_source_and_differing_lengths_keep_id_labels_and_original_pcm_without_rendering() {
    let mut session = session();
    let first = asset("source-a", 800);
    let pointer = first.audio.clone();
    session
        .add_assets(
            0,
            vec![first],
            &[Edit::AddLane {
                lane: lane("lane-a", "source-a", 800),
            }],
            "Add tracks",
        )
        .unwrap();
    session
        .add_assets(
            1,
            vec![asset("source-b", 400)],
            &[Edit::AddLane {
                lane: lane("lane-b", "source-b", 400),
            }],
            "Add tracks",
        )
        .unwrap();
    assert!(Arc::ptr_eq(
        session.assets().get("source-a").unwrap(),
        &pointer
    ));
    assert_eq!(session.read().assets.len(), 2);
    assert_eq!(session.read().assets[0].name, session.read().assets[1].name);
    assert_eq!(session.document().duration_frames(), 800);
    assert_eq!(pointer.samples, vec![0.1; 800]);
}

#[test]
fn a_batch_is_one_undo_and_failed_or_stale_transactions_retain_history_and_assets() {
    let mut session = session();
    session
        .add_assets(
            0,
            vec![asset("source-a", 800)],
            &[Edit::AddLane {
                lane: lane("lane-a", "source-a", 800),
            }],
            "Add tracks",
        )
        .unwrap();
    let edits = [
        Edit::SetClipGain {
            clip_id: "lane-a-clip".into(),
            gain_db: -6.,
        },
        Edit::MoveClip {
            clip_id: "lane-a-clip".into(),
            lane_id: "lane-a".into(),
            start_frame: 100,
        },
    ];
    session.edit(1, &edits, "Move and gain").unwrap();
    assert_eq!(
        session.read().history.undo_label.as_deref(),
        Some("Move and gain")
    );
    session.undo(2).unwrap();
    assert_eq!(session.document().clip("lane-a-clip").unwrap().gain_db, 0.);
    assert_eq!(
        session.document().clip("lane-a-clip").unwrap().start_frame,
        0
    );
    let before = serde_json::to_value(session.read()).unwrap();
    assert!(session.edit(2, &edits, "Stale edit").is_err());
    assert!(
        session
            .add_assets(
                3,
                vec![asset("source-b", 400)],
                &[Edit::AddLane {
                    lane: lane("lane-b", "missing", 400)
                }],
                "Invalid import"
            )
            .is_err()
    );
    assert_eq!(serde_json::to_value(session.read()).unwrap(), before);
    assert!(session.read().history.can_redo);
    assert!(!session.assets().contains_key("source-b"));
}

#[test]
fn source_waveforms_include_right_channel_and_validate_exact_bounds() {
    let mut session = session();
    let audio = Arc::new(Audio::new(vec![0., -0.5, 0., 0.75, 0., 0.1], 8000, 2).unwrap());
    session
        .add_assets(
            0,
            vec![OwnedAsset {
                id: "right".into(),
                name: "Right".into(),
                audio,
            }],
            &[],
            "Import asset",
        )
        .unwrap();
    let waveform = session
        .waveform(
            "right",
            FrameRange {
                start_frame: 0,
                end_frame: 3,
            },
            2,
        )
        .unwrap();
    assert_eq!(waveform.peaks, vec![[-0.5, 0.], [0., 0.75]]);
    assert!(
        session
            .waveform(
                "right",
                FrameRange {
                    start_frame: 0,
                    end_frame: 4
                },
                2
            )
            .is_err()
    );
}

#[test]
fn response_uses_shared_filters_and_bypass_without_mutating_document() {
    let mut session = session();
    session
        .add_assets(
            0,
            vec![asset("a", 8000)],
            &[Edit::AddLane {
                lane: lane("lane", "a", 8000),
            }],
            "Add",
        )
        .unwrap();
    let before = serde_json::to_value(session.read()).unwrap();
    let mut effect = ScopedEq {
        id: "eq".into(),
        scope: EqScope::Mix {},
        range: None,
        bands: default_eq(),
        output_gain_db: 0.,
        bypass: false,
    };
    effect.bands[3].gain = 6.;
    let response = session.eq_response(&effect).unwrap();
    assert_eq!(response.frequencies.len(), 240);
    assert!(
        response
            .gain_db
            .iter()
            .copied()
            .fold(f64::NEG_INFINITY, f64::max)
            > 5.9
    );
    effect.bypass = true;
    assert!(
        session
            .eq_response(&effect)
            .unwrap()
            .gain_db
            .iter()
            .all(|value| *value == 0.)
    );
    assert_eq!(serde_json::to_value(session.read()).unwrap(), before);
}

#[test]
fn history_labels_follow_byte_eviction_and_the_remaining_undo_redo_states() {
    let mut session = session();
    session
        .add_assets(
            0,
            vec![asset("a", 800)],
            &[Edit::AddLane {
                lane: lane("lane", "a", 800),
            }],
            "Add",
        )
        .unwrap();
    let document = session.document().clone();
    let bytes = serde_json::to_vec(&document).unwrap().len();
    session.history = crate::studio::arrangement::ArrangementHistory::new_with_budget(
        document,
        &session.assets,
        8,
        bytes * 2 + 80,
    )
    .unwrap();
    session.undo_labels.clear();
    for number in 1..=5 {
        session
            .edit(
                session.revision(),
                &[Edit::SetClipGain {
                    clip_id: "lane-clip".into(),
                    gain_db: -(number as f64),
                }],
                &format!("Gain {number}"),
            )
            .unwrap();
        assert_eq!(session.undo_labels.len(), session.history.undo_len());
    }
    assert_eq!(session.history.undo_len(), 1);
    assert_eq!(session.read().history.undo_label.as_deref(), Some("Gain 5"));
    session.undo(session.revision()).unwrap();
    assert_eq!(session.document().clip("lane-clip").unwrap().gain_db, -4.);
    assert!(!session.read().history.can_undo);
    assert_eq!(session.read().history.redo_label.as_deref(), Some("Gain 5"));
    session.redo(session.revision()).unwrap();
    assert_eq!(session.document().clip("lane-clip").unwrap().gain_db, -5.);
}
