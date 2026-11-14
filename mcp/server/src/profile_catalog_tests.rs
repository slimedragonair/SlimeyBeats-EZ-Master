use super::*;
use crate::{arguments::Arguments, options::MasterOptions};
use clap::Parser;

fn custom() -> Value {
    let mut value = mastercraft::profiles::builtin()["neutral"].clone();
    value["display_name"] = json!("Owner curve");
    value["lufs"]["streaming_safe"] = json!(-16.);
    value
}

#[test]
fn configured_root_exposes_exact_custom_ids_without_authorizing_paths() {
    let root = tempfile::tempdir().unwrap();
    fs::write(
        root.path().join("Owner curve.json"),
        serde_json::to_vec(&custom()).unwrap(),
    )
    .unwrap();
    let catalog = ProfileCatalog::load(Some(root.path())).unwrap();
    assert_eq!(
        catalog.resolve("custom:Owner curve").unwrap()["lufs"]["streaming_safe"],
        -16.
    );
    assert!(
        catalog
            .describe()
            .iter()
            .any(|p| p["id"] == "custom:Owner curve" && p["source"] == "owner_configured")
    );
    for name in [
        "custom:../../outside",
        "custom:C:\\private",
        "custom:/private",
        "../../outside",
        "custom:Owner curve+neutral",
        "neutral+custom:Owner curve",
    ] {
        assert!(
            catalog.resolve(name).is_err(),
            "Accepted a path or unsupported custom blend: {name}"
        );
    }
    assert!(catalog.resolve("neutral:0.7+edm:0.3").is_ok());
    assert!(
        catalog
            .resolve(&format!("custom:{}", "x".repeat(MAX_PROFILE_ID_BYTES)))
            .is_err()
    );
    let options = MasterOptions {
        profile: Some("custom:Owner curve".into()),
        ..Default::default()
    };
    assert!(options.apply(&Default::default(), false).is_err());
    assert_eq!(
        options
            .apply_with_catalog(&Default::default(), false, &catalog)
            .unwrap()
            .profile,
        "custom:Owner curve"
    );
    assert!(
        ProfileCatalog::load(None)
            .unwrap()
            .resolve("custom:Owner curve")
            .is_err()
    );
}

#[test]
fn custom_data_is_snapshotted_and_invalid_profiles_have_explicit_discovery_errors() {
    let root = tempfile::tempdir().unwrap();
    let selected = root.path().join("Owner.json");
    fs::write(&selected, serde_json::to_vec(&custom()).unwrap()).unwrap();
    fs::write(root.path().join("broken.json"), b"invalid json").unwrap();
    fs::write(root.path().join("wrong.json"), b"{\"tonal_db\":[1,2]}").unwrap();
    fs::write(
        root.path().join("ambiguous+name.json"),
        serde_json::to_vec(&custom()).unwrap(),
    )
    .unwrap();
    fs::write(
        root.path().join("oversized.json"),
        vec![b' '; MAX_PROFILE_BYTES + 1],
    )
    .unwrap();
    let catalog = ProfileCatalog::load(Some(root.path())).unwrap();
    assert_eq!(catalog.policy()["warnings"].as_array().unwrap().len(), 4);
    assert!(catalog.resolve("custom:broken").is_err());
    fs::write(&selected, b"changed after startup").unwrap();
    assert_eq!(catalog.resolve("custom:Owner").unwrap(), custom());
}

#[test]
fn missing_profile_directory_is_empty_without_creating_it_and_file_roots_fail() {
    let root = tempfile::tempdir().unwrap();
    let missing = root.path().join("GUI profiles not created yet");
    let catalog = ProfileCatalog::load(Some(&missing)).unwrap();
    assert!(
        catalog.policy()["custom_root_configured"]
            .as_bool()
            .unwrap()
    );
    assert_eq!(
        catalog.describe().len(),
        mastercraft::profiles::builtin().len()
    );
    assert!(!missing.exists());
    let file = root.path().join("file");
    fs::write(&file, b"data").unwrap();
    assert!(ProfileCatalog::load(Some(&file)).is_err());
}

#[test]
fn generated_configuration_preserves_profiles_root_as_a_separate_absolute_argument() {
    let root = tempfile::tempdir().unwrap();
    let profiles = root
        .path()
        .join("Original GUI install/Profiles with spaces");
    let args = Arguments::parse_from([
        "test",
        "--input-root",
        root.path().to_str().unwrap(),
        "--output-root",
        root.path().join("Exports").to_str().unwrap(),
        "--profiles-root",
        profiles.to_str().unwrap(),
        "--print-config",
    ]);
    let config = args.client_config().unwrap();
    let values = config["mcpServers"]["slimeybeats"]["args"]
        .as_array()
        .unwrap();
    let index = values.iter().position(|v| v == "--profiles-root").unwrap();
    assert_eq!(
        std::path::PathBuf::from(values[index + 1].as_str().unwrap()),
        std::path::absolute(&profiles).unwrap()
    );
    assert!(
        !profiles.exists(),
        "Printing configuration must not create the profile folder"
    );
}

#[cfg(unix)]
#[test]
fn symbolic_links_cannot_expose_profiles_outside_the_authorized_root() {
    let root = tempfile::tempdir().unwrap();
    let outside = root.path().join("outside.json");
    fs::write(&outside, serde_json::to_vec(&custom()).unwrap()).unwrap();
    let profiles = root.path().join("profiles");
    fs::create_dir(&profiles).unwrap();
    std::os::unix::fs::symlink(outside, profiles.join("escape.json")).unwrap();
    let catalog = ProfileCatalog::load(Some(&profiles)).unwrap();
    assert!(catalog.resolve("custom:escape").is_err());
    assert_eq!(catalog.policy()["warnings"].as_array().unwrap().len(), 1);
}

#[test]
fn aggregate_budget_failure_cannot_silently_publish_a_partial_catalog() {
    let root = tempfile::tempdir().unwrap();
    let value = json!({"tonal_db":vec![0.;10],"notes":"x".repeat(MAX_PROFILE_BYTES - 512)});
    let text = serde_json::to_vec(&value).unwrap();
    assert!(text.len() < MAX_PROFILE_BYTES);
    for index in 0..(MAX_CUSTOM_BYTES / text.len() + 1) {
        fs::write(root.path().join(format!("Curve {index}.json")), &text).unwrap();
    }
    let error = match ProfileCatalog::load(Some(root.path())) {
        Ok(_) => panic!("Oversized catalog was accepted"),
        Err(error) => error,
    };
    assert!(error.to_string().contains("8 MiB"));
}
