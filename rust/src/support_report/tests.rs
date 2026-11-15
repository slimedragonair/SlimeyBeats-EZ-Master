use super::*;

#[test]
fn unchecked_tools_stay_unknown_and_known_observations_keep_both_boolean_values() {
    for mut context in [
        ReportContext::current(),
        ReportContext::for_mcp("0.2.0-beta.3").unwrap(),
    ] {
        let value = serde_json::to_value(&context).unwrap();
        assert_eq!(
            value["tools"],
            serde_json::json!({
                "audio_codecs_ready": null, "stem_engine_ready": null,
                "vst3_host_ready": null, "mcp_bridge_ready": null,
            })
        );
        let draft = ReportDraft::new(IssueCategory::Other, "", "").unwrap();
        let preview = build(&context, &draft).unwrap();
        assert!(preview.markdown().contains("yes/no/unknown"));
        assert!(preview.markdown().contains("null means unknown"));
        // Fixture observations are supplied explicitly, never inferred by the
        // constructor or by rendering the report.
        context.tools.audio_codecs_ready = Some(true);
        context.tools.stem_engine_ready = Some(false);
        let value = serde_json::to_value(&context).unwrap();
        assert_eq!(value["tools"]["audio_codecs_ready"], true);
        assert_eq!(value["tools"]["stem_engine_ready"], false);
        assert!(value["tools"]["vst3_host_ready"].is_null());
        assert!(value["tools"]["mcp_bridge_ready"].is_null());
    }
}

#[test]
fn companion_identity_keeps_linked_engine_separate_and_desktop_unknown() {
    let context = ReportContext::for_mcp("0.2.0-beta.3").unwrap();
    let value = serde_json::to_value(&context).unwrap();
    assert_eq!(value["component"], "mcp_companion");
    assert_eq!(value["companion_version"], "0.2.0-beta.3");
    assert_eq!(
        value["linked_engine_build"],
        serde_json::to_value(crate::app_update::current()).unwrap()
    );
    assert!(value["installed_desktop_version"].is_null());
    assert!(value.get("application").is_none());
    assert_eq!(value["tool_readiness_probed"], false);
    let report = build(
        &context,
        &ReportDraft::new(IssueCategory::Other, "", "").unwrap(),
    )
    .unwrap();
    assert!(report.markdown().contains("may reach its model provider"));
    assert!(
        report
            .markdown()
            .contains("installed desktop version is unknown")
    );
    assert!(!ReportContext::current().is_mcp());
    for bad in ["not a version", "0.2.0\nsecret", "C:/private", ""] {
        assert!(ReportContext::for_mcp(bad).is_err());
    }
}

#[test]
fn identity_is_the_running_app_and_not_a_companion_version() {
    let context = ReportContext::current();
    assert_eq!(context.application(), &crate::app_update::current());
    assert_eq!(context.operating_system(), OperatingSystem::current());
    assert_eq!(context.architecture(), Architecture::current());
    let rendered = build(
        &context,
        &ReportDraft::new(IssueCategory::Other, "", "").unwrap(),
    )
    .unwrap();
    assert!(rendered.markdown().contains(&format!(
        "\"build_revision\": {}",
        crate::app_update::current().build_revision
    )));
    assert!(rendered.markdown().contains(&format!(
        "\"version\": \"{}\"",
        crate::app_update::current().version
    )));
}

#[test]
fn report_is_deterministic_and_exactly_matches_saved_bytes() {
    let root = tempfile::tempdir().unwrap();
    let mut context = ReportContext::current();
    context.workflow = Workflow::StemStudio;
    context.counts = Counts {
        tracks: 2,
        stems: 9,
        eq_bands: 7,
    };
    context.activity.preview_available = true;
    let draft = ReportDraft::new(
        IssueCategory::Stems,
        "Solo a stem",
        "1. Load stems\n2. Press Solo",
    )
    .unwrap();
    let first = build(&context, &draft).unwrap();
    assert_eq!(
        first.markdown(),
        build(&context, &draft).unwrap().markdown()
    );
    assert!(!first.text_was_changed());
    assert!(first.markdown().contains("\"stems\": 9"));
    assert!(first.markdown().contains("    2. Press Solo"));
    let destination = root.path().join("support.md");
    first.write_new(&destination).unwrap();
    assert_eq!(
        std::fs::read(destination).unwrap(),
        first.markdown().as_bytes()
    );
    assert_eq!(std::fs::read_dir(root.path()).unwrap().count(), 1);
}

#[test]
fn existing_file_is_never_overwritten_and_staging_is_cleaned() {
    let root = tempfile::tempdir().unwrap();
    let destination = root.path().join("support.md");
    std::fs::write(&destination, b"previous report").unwrap();
    let report = build(
        &ReportContext::current(),
        &ReportDraft::new(IssueCategory::Other, "", "").unwrap(),
    )
    .unwrap();
    assert!(report.write_new(&destination).is_err());
    assert_eq!(std::fs::read(&destination).unwrap(), b"previous report");
    assert_eq!(std::fs::read_dir(root.path()).unwrap().count(), 1);
}

#[test]
fn simultaneous_saves_have_one_winner_without_overwriting_or_leftovers() {
    let root = tempfile::tempdir().unwrap();
    let destination = root.path().join("support.md");
    let barrier = std::sync::Arc::new(std::sync::Barrier::new(2));
    let mut handles = Vec::new();
    for title in ["first report", "second report"] {
        let report = build(
            &ReportContext::current(),
            &ReportDraft::new(IssueCategory::Other, title, "").unwrap(),
        )
        .unwrap();
        let destination = destination.clone();
        let barrier = barrier.clone();
        handles.push(std::thread::spawn(move || {
            barrier.wait();
            (
                report.write_new(&destination).is_ok(),
                report.markdown().to_owned(),
            )
        }));
    }
    let results: Vec<_> = handles
        .into_iter()
        .map(|handle| handle.join().unwrap())
        .collect();
    assert_eq!(results.iter().filter(|(success, _)| *success).count(), 1);
    let saved = std::fs::read_to_string(destination).unwrap();
    assert_eq!(
        &saved,
        &results.iter().find(|(success, _)| *success).unwrap().1
    );
    assert_eq!(std::fs::read_dir(root.path()).unwrap().count(), 1);
}

#[test]
fn missing_parent_and_relative_paths_do_not_create_anything() {
    let root = tempfile::tempdir().unwrap();
    let report = build(
        &ReportContext::current(),
        &ReportDraft::new(IssueCategory::Other, "", "").unwrap(),
    )
    .unwrap();
    assert!(
        report
            .write_new(std::path::Path::new("support.md"))
            .is_err()
    );
    assert!(
        report
            .write_new(&root.path().join("missing/report.md"))
            .is_err()
    );
    assert_eq!(std::fs::read_dir(root.path()).unwrap().count(), 0);
}

#[test]
fn input_bounds_are_bytes_not_just_character_counts() {
    for (title, details) in [
        ("a".repeat(MAX_TITLE_BYTES + 1), String::new()),
        (String::new(), "a".repeat(MAX_DETAILS_BYTES + 1)),
        ("🟣".repeat(MAX_TITLE_BYTES / 4 + 1), String::new()),
    ] {
        assert!(ReportDraft::new(IssueCategory::Other, &title, &details).is_err());
    }
    let draft = ReportDraft::new(
        IssueCategory::Other,
        &"a".repeat(MAX_TITLE_BYTES),
        &"a".repeat(MAX_DETAILS_BYTES),
    )
    .unwrap();
    assert!(
        build(&ReportContext::current(), &draft)
            .unwrap()
            .markdown()
            .len()
            <= MAX_REPORT_BYTES
    );
}

#[test]
fn many_short_paths_cannot_exhaust_the_stack_or_report_limit() {
    let paths = "\"/a\" ".repeat(MAX_DETAILS_BYTES / 5);
    let draft = ReportDraft::new(IssueCategory::Other, "", &paths).unwrap();
    assert!(!draft.details().contains("/a"));
    let report = build(&ReportContext::current(), &draft).unwrap();
    assert!(report.markdown().len() <= MAX_REPORT_BYTES);
}

#[test]
fn formatted_document_cannot_exceed_the_total_byte_bound() {
    let draft =
        ReportDraft::new(IssueCategory::Other, "", &"\n".repeat(MAX_DETAILS_BYTES)).unwrap();
    assert!(build(&ReportContext::current(), &draft).is_err());
}

#[test]
fn user_markdown_remains_an_indented_description() {
    let draft = ReportDraft::new(
        IssueCategory::Other,
        "# Pretend heading\nhello",
        "```\n<script>payload</script>\n## fake diagnostics",
    )
    .unwrap();
    let report = build(&ReportContext::current(), &draft).unwrap();
    assert!(report.markdown().contains("    # Pretend heading hello"));
    assert!(report.markdown().contains("    <script>payload</script>"));
    assert!(!report.markdown().contains("\n## fake diagnostics"));
}

#[cfg(unix)]
#[test]
fn existing_symlinks_are_not_followed_and_private_mode_is_used() {
    use std::os::unix::fs::{PermissionsExt, symlink};
    let root = tempfile::tempdir().unwrap();
    let previous = root.path().join("previous.md");
    let link = root.path().join("link.md");
    std::fs::write(&previous, b"preserve").unwrap();
    symlink(&previous, &link).unwrap();
    let report = build(
        &ReportContext::current(),
        &ReportDraft::new(IssueCategory::Other, "", "").unwrap(),
    )
    .unwrap();
    assert!(report.write_new(&link).is_err());
    assert_eq!(std::fs::read(previous).unwrap(), b"preserve");
    let destination = root.path().join("new.md");
    report.write_new(&destination).unwrap();
    assert_eq!(
        std::fs::metadata(destination).unwrap().permissions().mode() & 0o777,
        0o600
    );
}
