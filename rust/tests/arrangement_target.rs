use mastercraft::{
    audio::Audio,
    studio::{
        arrangement::{
            Arrangement, AssetLibrary, Clip, EqScope, FrameRange, Lane, RenderOptions, ScopedEq,
            render_eq_target,
        },
        arrangement_session::{ArrangementSession, OwnedAsset, SessionLimits},
    },
};
use std::sync::Arc;

fn fixture() -> (Arrangement, AssetLibrary, EqScope, FrameRange) {
    let assets = AssetLibrary::from([(
        "source".into(),
        Arc::new(Audio::new(vec![0.1; 4000], 48000, 1).unwrap()),
    )]);
    let mut document = Arrangement::new(48000, 2);
    let mut lane = Lane::new("lane", "Track");
    lane.mute = true;
    lane.clips.push(Clip {
        id: "clip".into(),
        name: None,
        source_id: "source".into(),
        start_frame: 2000,
        source_start_frame: 500,
        end_frame: 5500,
        gain_db: 0.,
        fade_in_frames: 0,
        fade_out_frames: 0,
    });
    document.lanes.push(lane);
    let scope = EqScope::Clip {
        clip_id: "clip".into(),
    };
    let range = FrameRange {
        start_frame: 1000,
        end_frame: 2000,
    };
    (document, assets, scope, range)
}
#[test]
fn excludes_only_an_effect_with_the_same_scope_and_exact_source_frame_range() {
    let (mut document, assets, scope, range) = fixture();
    let mut effect = ScopedEq::new("existing", scope.clone());
    effect.range = Some(range);
    effect.output_gain_db = -6.;
    document.eq.push(effect);
    let options = RenderOptions {
        protect_peaks: false,
        max_output_bytes: 16000,
        ..Default::default()
    };
    let selected = render_eq_target(
        &document,
        &assets,
        &scope,
        Some(range),
        Some("existing"),
        &options,
        &|| false,
    )
    .unwrap();
    assert_eq!(selected.audio.frames(), 1000);
    assert_eq!(selected.offset_frame, 1000);
    assert!(
        selected
            .audio
            .samples
            .iter()
            .all(|sample| (*sample - 0.1).abs() < 1e-7)
    );
    assert!(
        render_eq_target(
            &document,
            &assets,
            &EqScope::Lane {
                lane_id: "lane".into()
            },
            Some(range),
            Some("existing"),
            &options,
            &|| false
        )
        .is_err()
    );
    assert!(
        render_eq_target(
            &document,
            &assets,
            &scope,
            None,
            Some("existing"),
            &options,
            &|| false
        )
        .is_err()
    );
    let wrong_range = FrameRange {
        start_frame: 1100,
        end_frame: 2100,
    };
    assert!(
        render_eq_target(
            &document,
            &assets,
            &scope,
            Some(wrong_range),
            Some("existing"),
            &options,
            &|| false
        )
        .is_err()
    );
    let retained = render_eq_target(
        &document,
        &assets,
        &scope,
        Some(range),
        None,
        &options,
        &|| false,
    )
    .unwrap();
    assert!(retained.audio.samples[1000] < 0.06);
    assert_eq!(document.eq.len(), 1);
    assert!(document.lanes[0].mute);
}
#[test]
fn project_restore_clamps_untrusted_output_budget_and_preserves_stricter_stored_budget() {
    let mut session =
        ArrangementSession::new(48000, 2, "epoch".into(), SessionLimits::default()).unwrap();
    let source = Arc::new(Audio::new(vec![0.1; 1000], 48000, 1).unwrap());
    session
        .add_assets(
            0,
            vec![OwnedAsset {
                id: "source".into(),
                name: "Source".into(),
                audio: source.clone(),
            }],
            &[],
            "Add source",
        )
        .unwrap();
    let host_budget = session.render_options().max_output_bytes;
    let mut snapshot = session.project_snapshot().unwrap();
    snapshot.render_options.max_output_bytes = u64::MAX;
    let restored =
        ArrangementSession::from_project(snapshot, "restored".into(), SessionLimits::default())
            .unwrap();
    assert_eq!(restored.render_options().max_output_bytes, host_budget);
    assert!(Arc::ptr_eq(&source, &restored.assets()["source"]));
    let mut snapshot = restored.project_snapshot().unwrap();
    snapshot.render_options.max_output_bytes = 65536;
    let restored =
        ArrangementSession::from_project(snapshot, "bounded".into(), SessionLimits::default())
            .unwrap();
    assert_eq!(restored.render_options().max_output_bytes, 65536);
}
