//! Monitor metadata follows history without treating cleanup PCM as a stem mix.
use super::*;

#[test]
fn monitor_mode_and_immutable_stem_views_follow_revisions() {
    let mut studio = BrowserStudio::new();
    for (kind, level) in [("vocals", 0.02), ("instrumental", 0.04)] {
        studio.stage_stem(kind, vec![level; 8000], 8000, 1).unwrap();
    }
    studio.commit_stems().unwrap();
    assert!(studio.preview_is_mix);
    let pointer = studio.stem_ptr("vocals").unwrap();
    assert_eq!(studio.stem_len("vocals").unwrap(), 8000);
    assert_eq!(studio.fork().stem_ptr("vocals").unwrap(), pointer);
    let mut mix = studio.mix.clone();
    mix.tracks[0].mute = true;
    studio.mix(&serde_json::to_string(&mix).unwrap()).unwrap();
    assert_eq!(studio.stem_ptr("vocals").unwrap(), pointer);
    studio
        .voice(
            "vocals",
            &serde_json::to_string(&VoiceSettings::default()).unwrap(),
        )
        .unwrap();
    assert!(!studio.preview_is_mix);
    studio.history(false).unwrap();
    assert!(studio.preview_is_mix);
    studio.history(true).unwrap();
    assert!(!studio.preview_is_mix);
    studio.replace_stem("vocals").unwrap();
    assert!(studio.preview_is_mix);
    assert_ne!(studio.stem_ptr("vocals").unwrap(), pointer);
    studio.history(false).unwrap();
    assert!(!studio.preview_is_mix);
    assert_eq!(studio.stem_ptr("vocals").unwrap(), pointer);
}
