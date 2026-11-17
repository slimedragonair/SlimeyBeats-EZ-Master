use super::*;
use mastercraft::studio::arrangement::{ArrangementPlayer, Clip, Edit, Lane};
use serde_json::Value;

fn fixture() -> BrowserArrangement {
    let mut session = BrowserArrangement::new(8000, 2, "browser-epoch").unwrap();
    session
        .stage_asset("a", "Same label", vec![0.1; 8000], 8000, 1)
        .unwrap();
    session
        .stage_asset("b", "Same label", vec![0.02; 4000], 8000, 1)
        .unwrap();
    let lane = |id: &str, source: &str, frames| {
        let mut lane = Lane::new(id, "Same label");
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
        Edit::AddLane { lane }
    };
    session
        .commit_import_checked(
            0,
            &serde_json::to_string(&[lane("one", "a", 8000), lane("two", "b", 4000)]).unwrap(),
        )
        .unwrap();
    session
}

#[test]
fn metadata_import_and_edits_return_no_song_pcm_and_preserve_sources() {
    let mut browser = fixture();
    let before = browser.session.assets()["a"].clone();
    let state: Value = serde_json::from_str(&browser.read().unwrap()).unwrap();
    assert_eq!(state["assets"].as_array().unwrap().len(), 2);
    assert!(state.get("samples").is_none());
    assert!(browser.player.is_none());
    let next = browser
        .edit_checked(
            1,
            r#"{"kind":"set_clip_gain","clip_id":"one-clip","gain_db":-6}"#,
        )
        .unwrap();
    assert!(
        serde_json::from_str::<Value>(&next)
            .unwrap()
            .get("samples")
            .is_none()
    );
    assert!(browser.player.is_none());
    assert!(std::sync::Arc::ptr_eq(
        &before,
        &browser.session.assets()["a"]
    ));
    assert!(
        browser
            .edit_checked(1, r#"{"kind":"delete_clip","clip_id":"one-clip"}"#)
            .is_err()
    );
    assert_eq!(before.samples, vec![0.1; 8000]);
}

#[test]
fn actual_core_blocks_match_offline_player_and_zero_fill_the_last_block() {
    let mut browser = fixture();
    let mut reference = ArrangementPlayer::new(
        browser.session.document(),
        browser.session.assets(),
        browser.session.render_options(),
    )
    .unwrap();
    browser.prepare_checked(1, "after", 0).unwrap();
    for frames in [1, 127, 511, 2048, 2048, 2048, 2048] {
        let mut expected = vec![0.; frames * 2];
        let count = reference.process_block(&mut expected).unwrap();
        assert_eq!(browser.block_checked(frames).unwrap(), count);
        assert_eq!(browser.block[..frames * 2], expected);
    }
}

#[test]
fn live_fixed_scope_controls_update_existing_player_without_full_render() {
    let mut browser = fixture();
    browser.prepare_checked(1, "after", 0).unwrap();
    browser.block_checked(512).unwrap();
    browser
        .edit_checked(
            1,
            r#"{"kind":"set_lane","lane_id":"one","gain_db":-6,"mute":false,"solo":true}"#,
        )
        .unwrap();
    assert_eq!(browser.player_revision, Some(2));
    browser.block_checked(1024).unwrap();
    let expected = 0.1 * 10_f32.powf(-6. / 20.);
    assert!((browser.block[2046] - expected).abs() < 1e-5);
    assert_eq!(browser.session.assets()["a"].samples, vec![0.1; 8000]);
}
