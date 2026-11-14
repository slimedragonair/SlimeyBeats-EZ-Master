use super::*;

#[test]
fn opening_a_session_does_not_rewrite_user_settings_or_unblock_a_damaged_file() {
    let folder = tempfile::tempdir().unwrap();
    let path = folder.path().join("rust-settings.json");
    let saved = Settings {
        out_dir: folder.path().join("User exports"),
        ..Settings::default()
    };
    let session = Settings {
        out_dir: folder.path().join("Session exports"),
        ..Settings::default()
    };
    let bytes = serde_json::to_vec_pretty(&saved).unwrap();
    std::fs::write(&path, &bytes).unwrap();
    let (active, error) = load_mastering_settings(&path, Some(session.clone()));
    assert!(error.is_none());
    assert_eq!(active.out_dir, session.out_dir);
    let mut store = PreferenceStore::new(path.clone(), &active, error.is_none());
    store.update(&active, Instant::now(), true, false).unwrap();
    assert_eq!(std::fs::read(&path).unwrap(), bytes);
    std::fs::write(&path, "damaged user settings").unwrap();
    let (mut active, error) = load_mastering_settings(&path, Some(session));
    assert!(error.is_some());
    let mut store = PreferenceStore::new(path.clone(), &active, error.is_none());
    active.intensity = 0.5;
    store.update(&active, Instant::now(), true, false).unwrap();
    assert_eq!(
        std::fs::read_to_string(path).unwrap(),
        "damaged user settings"
    );
}

#[test]
fn master_and_studio_export_preferences_restore_independently() {
    let folder = tempfile::tempdir().unwrap();
    let master_path = folder.path().join("rust-settings.json");
    let desktop_path = folder.path().join("desktop-preferences.json");
    let mut settings = Settings {
        out_dir: folder.path().join("Masters 龍 é"),
        formats: vec!["wav24".into(), "flac24".into()],
        suffix: "_final".into(),
        output_sr: 48000,
        ..Settings::default()
    };
    settings.manual_eq[2].gain = -1.5;
    let desktop = DesktopPreferences {
        studio_output_dir: Some(folder.path().join("Stems 別")),
        studio_export_format: "wav32f".into(),
        volume: 0.33,
        match_levels: false,
        studio_volume: 0.72,
        studio_match_levels: true,
        two_stems: false,
    };
    let mut storage = PreferencePersistence::new(
        master_path.clone(),
        &Settings::default(),
        true,
        desktop_path.clone(),
        &DesktopPreferences::default(),
        true,
    );
    storage
        .settings
        .update(&settings, Instant::now(), true, false)
        .unwrap();
    storage
        .desktop
        .update(&desktop, Instant::now(), true, false)
        .unwrap();
    drop(storage);
    let restored_settings = Settings::load(&master_path).unwrap();
    let restored_desktop = DesktopPreferences::load(&desktop_path).unwrap();
    let mut studio = Studio::new(restored_settings.out_dir.clone());
    restored_desktop.apply(&mut studio);
    assert_eq!(restored_settings.out_dir, settings.out_dir);
    assert_eq!(restored_settings.formats, settings.formats);
    assert_eq!(restored_settings.output_sr, 48000);
    assert_eq!(restored_settings.suffix, "_final");
    assert_eq!(restored_settings.manual_eq[2].gain, -1.5);
    assert_eq!(studio.output_dir, folder.path().join("Stems 別"));
    assert_eq!(studio.export_format, "wav32f");
    assert_eq!(restored_desktop.volume(), 0.33);
    assert!(!restored_desktop.match_levels());
    assert_eq!(studio.volume, 0.72);
    assert!(studio.match_levels);
    assert!(!studio.two_stems);
}

#[test]
fn older_preferences_keep_new_defaults_and_reject_invalid_values() {
    let folder = tempfile::tempdir().unwrap();
    let path = folder.path().join("prefs.json");
    std::fs::write(&path, "{\"studio_export_format\":\"flac24\"}").unwrap();
    let mut prefs = DesktopPreferences::load(&path).unwrap();
    prefs.resolve_output(std::path::Path::new("chosen master folder"));
    let mut studio = Studio::new(PathBuf::new());
    prefs.apply(&mut studio);
    assert_eq!(studio.output_dir, PathBuf::from("chosen master folder"));
    assert_eq!(studio.export_format, "flac24");
    assert!(prefs.match_levels());
    for broken in [
        "{",
        "{\"volume\":2}",
        "{\"studio_volume\":-1}",
        "{\"studio_export_format\":\"exe\"}",
        "{\"studio_output_dir\":\"\"}",
    ] {
        std::fs::write(&path, broken).unwrap();
        assert!(DesktopPreferences::load(&path).is_err(), "{broken}");
        assert_eq!(std::fs::read_to_string(&path).unwrap(), broken);
    }
}
