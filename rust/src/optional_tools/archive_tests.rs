use super::{
    archive::{self, FixedEntry, Limits},
    tar,
};
use sha2::{Digest, Sha256};
use std::{
    fs,
    io::{Cursor, Write},
    path::Path,
    sync::atomic::AtomicBool,
};
use zip::{ZipWriter, write::SimpleFileOptions};

pub(super) fn hash(bytes: &[u8]) -> String {
    Sha256::digest(bytes)
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}
pub(super) fn zip_bytes(files: &[(&str, &[u8])]) -> Vec<u8> {
    let mut zip = ZipWriter::new(Cursor::new(Vec::new()));
    for (name, bytes) in files {
        zip.start_file(
            *name,
            SimpleFileOptions::default().compression_method(zip::CompressionMethod::Stored),
        )
        .unwrap();
        zip.write_all(bytes).unwrap();
    }
    zip.finish().unwrap().into_inner()
}
fn limits() -> Limits {
    Limits {
        max_archive_bytes: 1_000_000,
        max_entries: 10,
        max_entry_bytes: 1000,
        max_total_bytes: 2000,
    }
}
fn extraction(bytes: &[u8], limits: Limits) -> anyhow::Result<Vec<u8>> {
    let root = tempfile::tempdir()?;
    let input = root.path().join("input.zip");
    let output = root.path().join("out");
    fs::write(&input, bytes)?;
    fs::create_dir(&output)?;
    archive::extract_zip_fixed(
        &input,
        &hash(bytes),
        &output,
        &[FixedEntry {
            archive_path: "bin/tool",
            file_name: "tool",
        }],
        limits,
        &AtomicBool::new(false),
    )?;
    assert_eq!(fs::read_dir(&output)?.count(), 1);
    Ok(fs::read(output.join("tool"))?)
}

#[test]
fn zip_writes_only_the_fixed_requested_file() {
    assert_eq!(
        extraction(
            &zip_bytes(&[("bin/tool", b"tool"), ("ignored", b"skip")]),
            limits()
        )
        .unwrap(),
        b"tool"
    );
}
#[test]
fn zip_rejects_unsafe_skipped_paths_and_case_aliases() {
    for name in [
        "../escape",
        "/absolute",
        "C:/drive",
        "a\\escape",
        "a/../escape",
        "./file",
        "a//file",
        "a/file:stream",
        "CON.txt",
        "a/trailing.",
        "a/trailing ",
        "BIN/TOOL",
    ] {
        assert!(
            extraction(
                &zip_bytes(&[("bin/tool", b"tool"), (name, b"bad")]),
                limits()
            )
            .is_err(),
            "{name}"
        );
    }
}
#[test]
fn zip_rejects_skipped_symlinks() {
    let mut zip = ZipWriter::new(Cursor::new(Vec::new()));
    zip.start_file("bin/tool", SimpleFileOptions::default())
        .unwrap();
    zip.write_all(b"tool").unwrap();
    zip.add_symlink(
        "ignored-link",
        "../../elsewhere",
        SimpleFileOptions::default(),
    )
    .unwrap();
    assert!(extraction(&zip.finish().unwrap().into_inner(), limits()).is_err());
}
#[test]
fn zip_enforces_archive_entry_member_and_total_budgets() {
    let bytes = zip_bytes(&[("bin/tool", b"12345678"), ("ignored", b"12345678")]);
    for tight in [
        Limits {
            max_archive_bytes: 1,
            ..limits()
        },
        Limits {
            max_entries: 1,
            ..limits()
        },
        Limits {
            max_entry_bytes: 7,
            ..limits()
        },
        Limits {
            max_total_bytes: 15,
            ..limits()
        },
    ] {
        assert!(extraction(&bytes, tight).is_err());
    }
}

#[test]
fn zip_rejects_local_header_disagreement_and_crc_corruption() {
    let mut disagreement = zip_bytes(&[("bin/tool", b"unique payload")]);
    disagreement[30 + 4] = b'f';
    assert!(extraction(&disagreement, limits()).is_err());
    let mut corrupt = zip_bytes(&[("bin/tool", b"unique payload")]);
    let start = corrupt
        .windows(14)
        .position(|bytes| bytes == b"unique payload")
        .unwrap();
    corrupt[start] ^= 1;
    assert!(extraction(&corrupt, limits()).is_err());
}

#[test]
fn cache_validation_is_read_only_and_rejects_relative_or_parent_paths() {
    let root = tempfile::tempdir().unwrap();
    let absent = root.path().join("absent/child");
    archive::validate_cache_path(&absent).unwrap();
    assert!(!root.path().join("absent").exists());
    assert!(archive::validate_cache_path(Path::new("relative")).is_err());
    assert!(archive::validate_cache_path(&root.path().join("../outside")).is_err());
}

#[cfg(windows)]
#[test]
fn cache_validation_rejects_windows_junction_ancestors_and_streams() {
    let root = tempfile::tempdir().unwrap();
    let target = root.path().join("target");
    let junction = root.path().join("junction");
    fs::create_dir(&target).unwrap();
    let mut command = super::probes::command("cmd.exe");
    command
        .args(["/D", "/C", "mklink", "/J"])
        .arg(&junction)
        .arg(&target);
    super::probes::collect(command, 8192, std::sync::Arc::new(AtomicBool::new(false))).unwrap();
    assert!(archive::validate_cache_path(&junction.join("child")).is_err());
    assert!(archive::validate_cache_path(&target.join("file:stream")).is_err());
    assert!(!target.join("child").exists());
    fs::remove_dir(junction).unwrap();
}
#[test]
fn zip_hash_failure_precedes_any_output_and_overwrites_are_forbidden() {
    let root = tempfile::tempdir().unwrap();
    let input = root.path().join("bad.zip");
    let output = root.path().join("out");
    fs::create_dir(&output).unwrap();
    fs::write(&input, b"not even a ZIP").unwrap();
    let entries = [FixedEntry {
        archive_path: "bin/tool",
        file_name: "tool",
    }];
    let error = archive::extract_zip_fixed(
        &input,
        &"a".repeat(64),
        &output,
        &entries,
        limits(),
        &AtomicBool::new(false),
    )
    .unwrap_err();
    assert!(error.to_string().contains("SHA-256"));
    assert_eq!(fs::read_dir(&output).unwrap().count(), 0);
    let zip = zip_bytes(&[("bin/tool", b"new")]);
    fs::write(&input, &zip).unwrap();
    fs::write(output.join("tool"), b"previous").unwrap();
    assert!(
        archive::extract_zip_fixed(
            &input,
            &hash(&zip),
            &output,
            &entries,
            limits(),
            &AtomicBool::new(false)
        )
        .is_err()
    );
    assert_eq!(fs::read(output.join("tool")).unwrap(), b"previous");
}
#[test]
fn zip_missing_files_and_unsafe_output_mappings_fail() {
    assert!(extraction(&zip_bytes(&[("different", b"payload")]), limits()).is_err());
    let root = tempfile::tempdir().unwrap();
    for output in ["../outside", "nested/tool", "tool:ads", "NUL"] {
        assert!(
            archive::validate_destination(
                root.path(),
                &[FixedEntry {
                    archive_path: "bin/tool",
                    file_name: output
                }]
            )
            .is_err()
        );
    }
}
#[test]
fn cancelled_zip_does_not_create_outputs() {
    let root = tempfile::tempdir().unwrap();
    let archive = root.path().join("archive.zip");
    let output = root.path().join("output");
    fs::create_dir(&output).unwrap();
    let bytes = zip_bytes(&[("bin/tool", b"tool")]);
    fs::write(&archive, &bytes).unwrap();
    assert!(
        archive::extract_zip_fixed(
            &archive,
            &hash(&bytes),
            &output,
            &[FixedEntry {
                archive_path: "bin/tool",
                file_name: "tool"
            }],
            limits(),
            &AtomicBool::new(true)
        )
        .is_err()
    );
    assert_eq!(fs::read_dir(output).unwrap().count(), 0);
}

fn tar_bytes(name: &str, kind: u8, content: &[u8]) -> Vec<u8> {
    let mut h = [0; 512];
    h[..name.len()].copy_from_slice(name.as_bytes());
    h[124..136].copy_from_slice(format!("{:011o}\0", content.len()).as_bytes());
    h[156] = kind;
    h[257..263].copy_from_slice(b"ustar\0");
    h[148..156].fill(b' ');
    let sum: u64 = h.iter().map(|&b| b as u64).sum();
    h[148..156].copy_from_slice(format!("{sum:06o}\0 ").as_bytes());
    let mut result = h.to_vec();
    result.extend_from_slice(content);
    result.resize(512 + content.len().div_ceil(512) * 512 + 1024, 0);
    result
}
fn extract_tar(bytes: &[u8]) -> anyhow::Result<Vec<u8>> {
    let root = tempfile::tempdir()?;
    tar::extract(
        &mut Cursor::new(bytes),
        root.path(),
        &[FixedEntry {
            archive_path: "bin/tool",
            file_name: "tool",
        }],
        limits(),
        &AtomicBool::new(false),
    )?;
    Ok(fs::read(root.path().join("tool"))?)
}
#[test]
fn tar_extracts_fixed_files_and_rejects_link_extension_and_traversal_headers() {
    assert_eq!(
        extract_tar(&tar_bytes("bin/tool", b'0', b"payload")).unwrap(),
        b"payload"
    );
    for kind in *b"12346xgLK" {
        assert!(extract_tar(&tar_bytes("bin/tool", kind, b"payload")).is_err());
    }
    for name in ["../bad", "/absolute", "a/../file", "a\\file", "C:drive"] {
        assert!(extract_tar(&tar_bytes(name, b'0', b"payload")).is_err());
    }
}
#[test]
fn tar_rejects_corruption_truncation_hidden_payload_and_large_members() {
    let mut corrupt = tar_bytes("bin/tool", b'0', b"payload");
    corrupt[10] ^= 1;
    assert!(extract_tar(&corrupt).is_err());
    assert!(extract_tar(&tar_bytes("bin/tool", b'0', &[0; 1001])).is_err());
    let valid = tar_bytes("bin/tool", b'0', b"payload");
    assert!(extract_tar(&valid[..valid.len() - 1]).is_err());
    let mut hidden = valid;
    hidden.push(1);
    assert!(extract_tar(&hidden).is_err());
}

#[cfg(unix)]
#[test]
fn output_symlink_or_linked_ancestor_is_refused() {
    let root = tempfile::tempdir().unwrap();
    let outside = tempfile::tempdir().unwrap();
    std::os::unix::fs::symlink(outside.path(), root.path().join("link")).unwrap();
    assert!(
        archive::validate_destination(
            &root.path().join("link"),
            &[FixedEntry {
                archive_path: "tool",
                file_name: "tool"
            }]
        )
        .is_err()
    );
}
