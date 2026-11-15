//! Atomic renames make interrupted deletion distinguishable from damaged live data.
use super::{
    filesystem as io,
    manifest::{self, BlobRef, Checkpoint, Owner, Run},
};
use anyhow::{Context, Result, ensure};
use std::{
    fs,
    path::{Path, PathBuf},
};

const BLOB_PREFIX: &str = "discarded-";
const RUN_PREFIX: &str = "discarded-";

pub(super) fn is_discarded_run(name: &str) -> bool {
    name.strip_prefix(RUN_PREFIX)
        .is_some_and(manifest::valid_run)
}
fn empty_or_owned(path: &Path, marker: &str) -> Result<bool> {
    io::directory(path)?;
    if io::entries(path, 1, &|| false).is_ok_and(|names| names.is_empty()) {
        fs::remove_dir(path)?;
        return Ok(true);
    }
    io::regular(&path.join(marker))?;
    Ok(false)
}
fn remove_blob_trash(path: &Path, cancel: &dyn Fn() -> bool) -> Result<()> {
    if empty_or_owned(path, "owner.json")? {
        return Ok(());
    }
    let owner: Owner = io::read_json(&path.join("owner.json"), 4096)?;
    owner.validate()?;
    io::require_names(path, &["owner.json", "blob.json", "audio.wav"], cancel)?;
    if path.join("blob.json").try_exists()? {
        let entry: BlobRef = io::read_json(&path.join("blob.json"), 4096)?;
        entry.validate()?;
        let expected = path
            .file_name()
            .and_then(|name| name.to_str())
            .and_then(|name| name.strip_prefix(BLOB_PREFIX))
            .context("Invalid discarded blob path")?;
        ensure!(entry.sha256 == expected, "Discarded blob identity mismatch");
        if path.join("audio.wav").try_exists()? {
            ensure!(
                io::regular(&path.join("audio.wav"))? == entry.bytes,
                "Malformed discarded audio was preserved"
            );
        }
    } else {
        ensure!(
            !path.join("audio.wav").try_exists()?,
            "Unidentified discarded audio was preserved"
        );
    }
    let mut files = Vec::new();
    for name in ["audio.wav", "blob.json", "owner.json"] {
        let file = path.join(name);
        if file.try_exists()? {
            io::regular(&file)?;
            files.push(file);
        }
    }
    io::cancelled(cancel)?;
    for file in files {
        fs::remove_file(file)?;
        #[cfg(test)]
        hold_deletion_for_test(path, "blob");
    }
    fs::remove_dir(path)?;
    Ok(())
}
fn run_trash_files(path: &Path, id: &str, cancel: &dyn Fn() -> bool) -> Result<Vec<PathBuf>> {
    io::require_names(
        path,
        &[
            "run.json",
            "run.lock",
            "clean.json",
            "pending",
            "checkpoints",
        ],
        cancel,
    )?;
    let marker: Run = io::read_json(&path.join("run.json"), 4096)?;
    marker.owner.validate()?;
    ensure!(marker.run_id == id, "Discarded run ownership mismatch");
    let mut files = Vec::new();
    for name in ["checkpoints", "pending"] {
        let dir = path.join(name);
        if !dir.try_exists()? {
            continue;
        }
        io::directory(&dir)?;
        if name == "checkpoints" {
            for file in io::entries(&dir, 32, cancel)? {
                ensure!(
                    manifest::generation(&file).is_some(),
                    "Unknown discarded checkpoint preserved"
                );
                let file = dir.join(file);
                ensure!(
                    io::regular(&file)? <= super::MAX_METADATA_BYTES,
                    "Oversized discarded checkpoint preserved"
                );
                let record: Checkpoint = io::read_json(&file, super::MAX_METADATA_BYTES)?;
                let generation = file
                    .file_name()
                    .and_then(|name| name.to_str())
                    .and_then(manifest::generation)
                    .context("Invalid discarded checkpoint name")?;
                record.validate(&super::CheckpointRef {
                    run_id: id.into(),
                    generation,
                })?;
                files.push(file);
            }
        } else {
            // The validated run marker owns this fixed staging namespace. Its own
            // marker may already have been removed by an interrupted dismissal.
            io::require_names(
                &dir,
                &["owner.json", "blob", "checkpoint.json", "clean.json"],
                cancel,
            )?;
            for file in ["owner.json", "checkpoint.json", "clean.json"] {
                if dir.join(file).try_exists()? {
                    io::regular(&dir.join(file))?;
                    if file == "owner.json" {
                        let owner: Owner = io::read_json(&dir.join(file), 4096)?;
                        owner.validate()?;
                    }
                    files.push(dir.join(file));
                }
            }
            let blob = dir.join("blob");
            if blob.try_exists()? {
                io::require_names(&blob, &["owner.json", "blob.json", "audio.wav"], cancel)?;
                if blob.join("owner.json").try_exists()? {
                    let owner: Owner = io::read_json(&blob.join("owner.json"), 4096)?;
                    owner.validate()?;
                }
                for file in ["audio.wav", "blob.json", "owner.json"] {
                    if blob.join(file).try_exists()? {
                        io::regular(&blob.join(file))?;
                        files.push(blob.join(file));
                    }
                }
            }
        }
    }
    for name in ["clean.json", "run.lock", "run.json"] {
        let file = path.join(name);
        if file.try_exists()? {
            io::regular(&file)?;
            if name == "clean.json" {
                let marker: super::storage::Clean = io::read_json(&file, 4096)?;
                marker.owner.validate()?;
                ensure!(
                    marker.run_id == id,
                    "Discarded clean marker identity mismatch"
                );
            }
            if name == "run.lock" {
                ensure!(
                    io::regular(&file)? == 0,
                    "Unexpected discarded lock material was preserved"
                );
            }
            files.push(file);
        }
    }
    Ok(files)
}
fn remove_run_trash(path: &Path, id: &str, cancel: &dyn Fn() -> bool) -> Result<()> {
    if empty_or_owned(path, "run.json")? {
        return Ok(());
    }
    let files = run_trash_files(path, id, cancel)?;
    io::cancelled(cancel)?;
    // Keep run.json until its subdirectories are gone, so any interrupted phase
    // remains provably owned. Unmarked nonempty directories are never removed.
    for file in files.iter().filter(|file| **file != path.join("run.json")) {
        fs::remove_file(file)?;
        #[cfg(test)]
        hold_deletion_for_test(path, "run");
    }
    for name in ["pending/blob", "pending", "checkpoints"] {
        if path.join(name).try_exists()? {
            fs::remove_dir(path.join(name))?;
        }
    }
    fs::remove_file(path.join("run.json"))?;
    fs::remove_dir(path)?;
    Ok(())
}
#[cfg(test)]
fn hold_deletion_for_test(path: &Path, stage: &str) {
    if std::env::var("SLIMEY_RECOVERY_TEST_HOLD_DELETE")
        .ok()
        .as_deref()
        == Some(stage)
    {
        let root = path.parent().unwrap().parent().unwrap();
        fs::write(root.parent().unwrap().join("ready"), stage).unwrap();
        loop {
            std::thread::sleep(std::time::Duration::from_millis(100));
        }
    }
}
pub(super) fn clear(root: &Path, cancel: &dyn Fn() -> bool) -> Result<()> {
    for name in io::entries(&root.join("runs"), super::MAX_RUNS, cancel)? {
        if let Some(id) = name
            .strip_prefix(RUN_PREFIX)
            .filter(|id| manifest::valid_run(id))
        {
            remove_run_trash(&root.join("runs").join(&name), id, cancel)?;
        }
    }
    for name in io::entries(&root.join("blobs"), super::MAX_BLOBS, cancel)? {
        if let Some(hash) = name.strip_prefix(BLOB_PREFIX) {
            ensure!(
                manifest::valid_hash(hash),
                "Unknown discarded blob preserved"
            );
            remove_blob_trash(&root.join("blobs").join(&name), cancel)?;
        }
    }
    Ok(())
}
pub(super) fn discard_blob(root: &Path, path: &Path, cancel: &dyn Fn() -> bool) -> Result<()> {
    let name = path
        .file_name()
        .and_then(|name| name.to_str())
        .context("Invalid blob name")?;
    ensure!(manifest::valid_hash(name), "Invalid discarded blob name");
    let trash = root.join("blobs").join(format!("{BLOB_PREFIX}{name}"));
    ensure!(
        !trash.try_exists()?,
        "Unfinished blob dismissal must be cleared first"
    );
    fs::rename(path, &trash)?;
    io::sync_directory(&root.join("blobs"))?;
    remove_blob_trash(&trash, cancel)
}
pub(super) fn discard_run(root: &Path, id: &str) -> Result<()> {
    ensure!(manifest::valid_run(id), "Invalid discarded run name");
    let trash = root.join("runs").join(format!("{RUN_PREFIX}{id}"));
    ensure!(
        !trash.try_exists()?,
        "Unfinished run dismissal must be cleared first"
    );
    fs::rename(root.join("runs").join(id), &trash)?;
    io::sync_directory(&root.join("runs"))?;
    remove_run_trash(&trash, id, &|| false)
}
