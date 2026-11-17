use super::arrangement_state::{ArrangementEditor, EqTarget, LaneInit};
use crate::{
    audio::Audio,
    studio::arrangement::{Edit, EqScope, FrameRange},
};
use std::sync::Arc;

fn editor() -> ArrangementEditor {
    let mut editor = ArrangementEditor::default();
    editor
        .add_sources_with_controls(vec![
            (
                "First source".into(),
                Arc::new(Audio::new(vec![0.05; 8000], 8000, 1).unwrap()),
                LaneInit::default(),
            ),
            (
                "Longer source".into(),
                Arc::new(Audio::new(vec![-0.03; 12000 * 2], 8000, 2).unwrap()),
                LaneInit::default(),
            ),
        ])
        .unwrap();
    editor
}

#[test]
fn different_lengths_and_mono_sources_form_tracks_without_changing_their_samples() {
    let editor = editor();
    let document = editor.document().unwrap();
    assert_eq!(document.channels, 2);
    assert_eq!(document.lanes.len(), 2);
    assert_eq!(document.duration_frames(), 12000);
    assert_eq!(editor.assets.len(), 2);
    assert_eq!(
        editor
            .assets
            .values()
            .filter(|audio| audio.channels == 1)
            .count(),
        1
    );
    assert_eq!(
        editor
            .assets
            .values()
            .filter(|audio| audio.frames() == 12000)
            .count(),
        1
    );
    assert!(editor.assets.values().all(|audio| {
        audio
            .samples
            .iter()
            .all(|sample| *sample == 0.05 || *sample == -0.03)
    }));
}

#[test]
fn a_drag_is_one_history_step_and_keeps_the_source_pool_shared() {
    let mut editor = editor();
    let clip = editor.document().unwrap().lanes[0].clips[0].clone();
    let lane = editor.document().unwrap().lanes[0].id.clone();
    let source = editor.assets[&clip.source_id].clone();
    let committed = editor.history.as_ref().unwrap().current().clone();
    for frame in [80, 160, 240, 320] {
        editor
            .preview_edit(Edit::MoveClip {
                clip_id: clip.id.clone(),
                lane_id: lane.clone(),
                start_frame: frame,
            })
            .unwrap();
        assert_eq!(editor.history.as_ref().unwrap().current(), &committed);
    }
    editor.commit_draft().unwrap();
    assert_eq!(
        editor
            .document()
            .unwrap()
            .clip(&clip.id)
            .unwrap()
            .start_frame,
        320
    );
    editor.history_step(false).unwrap();
    assert_eq!(editor.document().unwrap(), &committed);
    assert!(Arc::ptr_eq(&source, &editor.assets[&clip.source_id]));
    editor.history_step(true).unwrap();
    assert_eq!(
        editor
            .document()
            .unwrap()
            .clip(&clip.id)
            .unwrap()
            .start_frame,
        320
    );
}

#[test]
fn clip_section_eq_maps_song_time_to_the_actual_trimmed_source() {
    let mut editor = editor();
    let clip = editor.document().unwrap().lanes[0].clips[0].clone();
    editor
        .apply(Edit::TrimClip {
            clip_id: clip.id.clone(),
            start_frame: 800,
            source_start_frame: 400,
            end_frame: 7200,
        })
        .unwrap();
    editor.select_clip(&clip.id).unwrap();
    editor.eq_target = EqTarget::ClipRange;
    editor.selection = Some(FrameRange {
        start_frame: 1600,
        end_frame: 3200,
    });
    let effect = editor.eq_effect().unwrap();
    assert_eq!(effect.scope, EqScope::Clip { clip_id: clip.id });
    assert_eq!(
        effect.range,
        Some(FrameRange {
            start_frame: 1200,
            end_frame: 2800
        })
    );
}

#[test]
fn repeated_eq_gestures_edit_one_effect_instead_of_stacking_filters() {
    let mut editor = editor();
    editor.eq_target = EqTarget::Mix;
    for gain in [-0.5, -1.0, -1.5] {
        editor.eq_bands[2].gain = gain;
        let effect = editor.eq_effect().unwrap();
        editor.preview_edit(Edit::SetEq { effect }).unwrap();
        editor.commit_draft().unwrap();
        assert_eq!(editor.document().unwrap().eq.len(), 1);
    }
    assert_eq!(editor.document().unwrap().eq[0].bands[2].gain, -1.5);
    editor.history_step(false).unwrap();
    assert_eq!(editor.document().unwrap().eq[0].bands[2].gain, -1.0);
    assert_eq!(editor.eq_bands[2].gain, -1.0);
}

#[test]
fn source_failure_and_cancelled_drafts_preserve_the_committed_recipe() {
    let mut editor = editor();
    let previous = editor.document().unwrap().clone();
    let source_count = editor.assets.len();
    assert!(
        editor
            .add_sources_with_controls(vec![(
                "Wrong rate".into(),
                Arc::new(Audio::new(vec![0.1; 4800], 48000, 1).unwrap()),
                LaneInit::default()
            )])
            .is_err()
    );
    assert_eq!(editor.document().unwrap(), &previous);
    assert_eq!(editor.assets.len(), source_count);
    let clip = previous.lanes[0].clips[0].clone();
    editor
        .preview_edit(Edit::DeleteClip { clip_id: clip.id })
        .unwrap();
    editor.cancel_draft();
    assert_eq!(editor.document().unwrap(), &previous);
}

#[test]
fn a_partial_track_name_commits_when_selection_moves() {
    let mut editor = editor();
    let lane = editor.document().unwrap().lanes[0].id.clone();
    editor.lane_name_draft = Some((lane.clone(), "Renamed track".into()));
    assert_ne!(
        editor.document().unwrap().lane(&lane).unwrap().name,
        "Renamed track"
    );
    let clip = editor.document().unwrap().lanes[0].clips[0].id.clone();
    editor.select_clip(&clip).unwrap();
    assert_eq!(
        editor.document().unwrap().lane(&lane).unwrap().name,
        "Renamed track"
    );
}

#[test]
fn revisiting_an_earlier_overlapping_effect_keeps_its_identity_and_source_coordinates() {
    let mut editor = editor();
    let clip = editor.document().unwrap().lanes[0].clips[0].clone();
    editor.select_clip(&clip.id).unwrap();
    editor.eq_target = EqTarget::ClipRange;
    editor.selection = Some(FrameRange {
        start_frame: 800,
        end_frame: 2400,
    });
    let mut first = editor.eq_effect().unwrap();
    first.bands[2].gain = -2.;
    editor
        .apply(Edit::SetEq {
            effect: first.clone(),
        })
        .unwrap();
    let mut second = first.clone();
    second.id = "another_effect".into();
    second.bands[2].gain = 1.;
    editor.apply(Edit::SetEq { effect: second }).unwrap();
    editor
        .apply(Edit::MoveClip {
            clip_id: clip.id.clone(),
            lane_id: editor.selected_lane.clone().unwrap(),
            start_frame: 4000,
        })
        .unwrap();
    super::arrangement_eq_effects::select(&mut editor, &first).unwrap();
    assert_eq!(
        editor.selection,
        Some(FrameRange {
            start_frame: 4800,
            end_frame: 6400
        })
    );
    assert_eq!(editor.eq_id.as_deref(), Some(first.id.as_str()));
    assert_eq!(editor.eq_bands[2].gain, -2.);
    editor.eq_bands[2].gain = -3.;
    let effect = editor.eq_effect().unwrap();
    editor.apply(Edit::SetEq { effect }).unwrap();
    assert_eq!(editor.document().unwrap().eq.len(), 2);
    assert_eq!(editor.document().unwrap().eq[0].bands[2].gain, -3.);
    assert_eq!(editor.document().unwrap().eq[1].bands[2].gain, 1.);
}

#[test]
fn a_timeline_derived_tool_snapshot_is_marked_stale_after_an_edit() {
    let audio = Arc::new(Audio::new(vec![0.; 800], 8000, 1).unwrap());
    let mut runtime = super::arrangement_runtime::NativeArrangement::default();
    runtime.rendered_audio = Some(audio.clone());
    runtime.rendered_epoch = Some(runtime.editor.epoch);
    assert!(!runtime.snapshot_is_stale(Some(&audio)));
    runtime.editor.epoch += 1;
    assert!(runtime.snapshot_is_stale(Some(&audio)));
    let independent = Arc::new((*audio).clone());
    assert!(!runtime.snapshot_is_stale(Some(&independent)));
}

#[test]
fn importing_more_sources_keeps_existing_mix_eq_as_one_editable_effect() {
    let mut editor = editor();
    editor.eq_target = EqTarget::Mix;
    editor.eq_bands[2].gain = -2.;
    let effect = editor.eq_effect().unwrap();
    editor
        .apply(Edit::SetEq {
            effect: effect.clone(),
        })
        .unwrap();
    let mut fork = editor.fork_for_import().unwrap();
    fork.add_sources_with_controls(vec![(
        "Another".into(),
        Arc::new(Audio::new(vec![0.; 800], 8000, 1).unwrap()),
        LaneInit::default(),
    )])
    .unwrap();
    assert_eq!(fork.eq_id.as_ref(), Some(&effect.id));
    fork.eq_bands[2].gain = -3.;
    let updated = fork.eq_effect().unwrap();
    fork.apply(Edit::SetEq { effect: updated }).unwrap();
    assert_eq!(fork.document().unwrap().eq.len(), 1);
}

#[test]
fn cancelling_live_eq_restores_committed_fields_before_the_next_gesture() {
    let mut editor = editor();
    editor.eq_bands[2].gain = -2.;
    let original = editor.eq_effect().unwrap();
    editor.apply(Edit::SetEq { effect: original }).unwrap();
    editor.eq_bands[2].gain = -8.;
    let draft = editor.eq_effect().unwrap();
    editor.preview_edit(Edit::SetEq { effect: draft }).unwrap();
    editor.cancel_draft();
    assert_eq!(editor.eq_bands[2].gain, -2.);
    assert_eq!(editor.document().unwrap().eq[0].bands[2].gain, -2.);
}

#[test]
fn partially_trimmed_section_eq_still_has_a_marker_for_its_audible_intersection() {
    let mut editor = editor();
    let clip = editor.document().unwrap().lanes[0].clips[0].clone();
    editor.select_clip(&clip.id).unwrap();
    editor.eq_target = EqTarget::ClipRange;
    editor.selection = Some(FrameRange {
        start_frame: 800,
        end_frame: 3200,
    });
    let effect = editor.eq_effect().unwrap();
    editor
        .apply(Edit::SetEq {
            effect: effect.clone(),
        })
        .unwrap();
    editor
        .apply(Edit::TrimClip {
            clip_id: clip.id,
            start_frame: 4000,
            source_start_frame: 1600,
            end_frame: 7200,
        })
        .unwrap();
    let document = editor.document().unwrap();
    assert!(super::arrangement_eq_effects::project_range(document, &effect).is_none());
    assert_eq!(
        super::arrangement_eq_effects::audible_range(document, &effect),
        Some(FrameRange {
            start_frame: 4000,
            end_frame: 5600
        })
    );
}
