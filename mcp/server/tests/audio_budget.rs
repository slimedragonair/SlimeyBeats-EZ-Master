use slimeybeats_mcp::audio_input;
use std::{
    sync::{Arc, atomic::AtomicBool},
    time::Duration,
};

#[test]
fn wav_budget_and_cancel_are_checked_before_decode() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("tone.wav");
    let audio = mastercraft::audio::Audio::new(vec![0.01; 48000 * 2], 48000, 2).unwrap();
    mastercraft::wav_encoding::write_wav(
        &audio,
        std::fs::File::create(&path).unwrap(),
        "wav16",
        false,
    )
    .unwrap();
    let load = |limit, cancel| {
        audio_input::load(
            &path,
            limit,
            Duration::from_secs(5),
            Arc::new(AtomicBool::new(cancel)),
        )
    };
    assert!(load(300000, false).is_err());
    assert!(load(400000, true).is_err());
    let (decoded, _) = load(400000, false).unwrap();
    assert_eq!(decoded.frames(), 48000);
}
