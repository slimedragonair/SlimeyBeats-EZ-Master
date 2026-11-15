use super::{
    CheckpointRef, MAX_BLOBS, MAX_RUNS, RETAINED_CHECKPOINTS, blobs, filesystem as io,
    manifest::{self, Owner},
};
use anyhow::{Result, ensure};
use std::{collections::HashSet, fs, path::Path};

pub(super) fn pending_files(
    path: &Path,
    cancel: &dyn Fn() -> bool,
) -> Result<Vec<std::path::PathBuf>> {
    io::directory(path)?;
    let owner: Owner = io::read_json(&path.join("owner.json"), 4096)?;
    owner.validate()?;
    io::require_names(
        path,
        &["owner.json", "blob", "checkpoint.json", "clean.json"],
        cancel,
    )?;
    let mut files = vec![path.join("owner.json")];
    for name in ["checkpoint.json", "clean.json"] {
        let file = path.join(name);
        if file.try_exists()? {
            io::regular(&file)?;
            files.push(file);
        }
    }
    let blob = path.join("blob");
    if blob.try_exists()? {
        io::directory(&blob)?;
        let owner: Owner = io::read_json(&blob.join("owner.json"), 4096)?;
        owner.validate()?;
        io::require_names(&blob, &["owner.json", "blob.json", "audio.wav"], cancel)?;
        for name in ["owner.json", "blob.json", "audio.wav"] {
            let file = blob.join(name);
            if file.try_exists()? {
                io::regular(&file)?;
                files.push(file);
            }
        }
    }
    Ok(files)
}
pub(super) fn clear_pending(path: &Path, cancel: &dyn Fn() -> bool) -> Result<()> {
    let files = pending_files(path, cancel)?;
    io::cancelled(cancel)?;
    for file in files {
        if file != path.join("owner.json") {
            fs::remove_file(file)?;
        }
    }
    let blob = path.join("blob");
    if blob.try_exists()? {
        fs::remove_dir(blob)?;
    }
    Ok(())
}
pub(super) fn run_files(
    root: &Path,
    id: &str,
    cancel: &dyn Fn() -> bool,
) -> Result<Vec<std::path::PathBuf>> {
    let run = root.join("runs").join(id);
    manifest::validate_run(&run, id)?;
    io::require_names(
        &run,
        &[
            "run.json",
            "run.lock",
            "checkpoints",
            "pending",
            "clean.json",
        ],
        cancel,
    )?;
    let mut files = vec![run.join("run.json"), run.join("run.lock")];
    for name in io::entries(&run.join("checkpoints"), 32, cancel)? {
        ensure!(
            manifest::generation(&name).is_some(),
            "Unknown material preserved in recovery checkpoints"
        );
        let path = run.join("checkpoints").join(name);
        ensure!(
            io::regular(&path)? <= super::MAX_METADATA_BYTES,
            "Oversized recovery checkpoint preserved"
        );
        files.push(path);
    }
    files.extend(pending_files(&run.join("pending"), cancel)?);
    if run.join("clean.json").try_exists()? {
        io::regular(&run.join("clean.json"))?;
        files.push(run.join("clean.json"));
    }
    for file in &files {
        io::regular(file)?;
    }
    Ok(files)
}
/// Logical file bytes, including incomplete owned transactions, across all windows.
pub(super) fn inventory(root: &Path, cancel: &dyn Fn() -> bool) -> Result<(u64, usize)> {
    io::require_names(root, &["owner.json", "store.lock", "runs", "blobs"], cancel)?;
    let mut total = io::regular(&root.join("owner.json"))? + io::regular(&root.join("store.lock"))?;
    for run in io::entries(&root.join("runs"), MAX_RUNS, cancel)? {
        ensure!(
            manifest::valid_run(&run),
            "Unknown recovery run directory preserved"
        );
        for file in run_files(root, &run, cancel)? {
            total = total
                .checked_add(io::regular(&file)?)
                .ok_or_else(|| anyhow::anyhow!("Recovery storage size overflow"))?;
        }
    }
    let blobs = io::entries(&root.join("blobs"), MAX_BLOBS, cancel)?;
    for name in &blobs {
        ensure!(
            manifest::valid_hash(name),
            "Unknown recovery blob directory preserved"
        );
        let path = root.join("blobs").join(name);
        let entry = blobs::validate_directory(&path, None)?;
        ensure!(
            &entry.sha256 == name,
            "Recovery blob directory identity mismatch"
        );
        for file in ["owner.json", "blob.json", "audio.wav"] {
            total = total
                .checked_add(io::regular(&path.join(file))?)
                .ok_or_else(|| anyhow::anyhow!("Recovery storage size overflow"))?;
        }
    }
    Ok((total, blobs.len()))
}
/// Every unfinished run retains room for its tiny clean-exit marker. Another
/// window cannot consume that headroom with a later save or newly opened run.
pub(super) fn exit_reserve(root: &Path, cancel: &dyn Fn() -> bool) -> Result<u64> {
    let mut reserved = 0;
    for run in io::entries(&root.join("runs"), MAX_RUNS, cancel)? {
        if !root
            .join("runs")
            .join(run)
            .join("clean.json")
            .try_exists()?
        {
            reserved += super::EXIT_RESERVE_BYTES;
        }
    }
    Ok(reserved)
}
/// Caller holds the global transaction lock and has released the run's OS lock.
/// `files` must come from run_files after any checkpoint/clean-marker validation.
pub(super) fn remove_run(root: &Path, id: &str, _files: Vec<std::path::PathBuf>) -> Result<()> {
    super::tombstones::discard_run(root, id)
}
pub(super) fn trim_run(root: &Path, id: &str, cancel: &dyn Fn() -> bool) -> Result<()> {
    let path = root.join("runs").join(id).join("checkpoints");
    let names = io::entries(&path, 32, cancel)?;
    let mut old = Vec::new();
    for name in &names {
        let generation = manifest::generation(name)
            .ok_or_else(|| anyhow::anyhow!("Unknown checkpoint preserved"))?;
        manifest::read_checkpoint(
            root,
            &CheckpointRef {
                run_id: id.into(),
                generation,
            },
        )?;
    }
    for name in names
        .iter()
        .take(names.len().saturating_sub(RETAINED_CHECKPOINTS))
    {
        old.push(path.join(name));
    }
    io::cancelled(cancel)?;
    for file in old {
        fs::remove_file(file)?;
    }
    io::sync_directory(&path)
}
pub(super) fn garbage_collect(root: &Path, cancel: &dyn Fn() -> bool) -> Result<()> {
    super::tombstones::clear(root, cancel)?;
    let mut referenced = HashSet::new();
    for run in io::entries(&root.join("runs"), MAX_RUNS, cancel)? {
        manifest::validate_run(&root.join("runs").join(&run), &run)?;
        for name in io::entries(
            &root.join("runs").join(&run).join("checkpoints"),
            32,
            cancel,
        )? {
            let generation = manifest::generation(&name)
                .ok_or_else(|| anyhow::anyhow!("Unknown checkpoint prevents safe cleanup"))?;
            let record = manifest::read_checkpoint(
                root,
                &CheckpointRef {
                    run_id: run.clone(),
                    generation,
                },
            )?;
            referenced.extend(record.data.audio.into_iter().map(|audio| audio.sha256));
        }
    }
    // Validate the entire deletion plan before removing even one directory.
    let mut unused = Vec::new();
    for name in io::entries(&root.join("blobs"), MAX_BLOBS, cancel)? {
        ensure!(
            manifest::valid_hash(&name),
            "Unknown blob prevents safe cleanup"
        );
        let path = root.join("blobs").join(&name);
        let entry = blobs::validate_directory(&path, None)?;
        ensure!(
            entry.sha256 == name,
            "Blob identity mismatch prevents safe cleanup"
        );
        if !referenced.contains(&name) {
            unused.push(path);
        }
    }
    for path in unused {
        io::cancelled(cancel)?;
        super::tombstones::discard_blob(root, &path, cancel)?;
    }
    io::sync_directory(&root.join("blobs"))
}
