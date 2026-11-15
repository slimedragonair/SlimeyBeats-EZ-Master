use super::{
    CheckpointRef, Listing, Snapshot, cleanup, filesystem as io,
    manifest::{self, Checkpoint},
    storage::{self, Clean},
};
use anyhow::{Context, Result, ensure};
use std::path::Path;

pub fn list_candidates(root: &Path, cancel: &dyn Fn() -> bool) -> Result<Listing> {
    list(root, false, cancel)
}
pub fn list_history(root: &Path, cancel: &dyn Fn() -> bool) -> Result<Listing> {
    list(root, true, cancel)
}
fn list(root: &Path, include_clean: bool, cancel: &dyn Fn() -> bool) -> Result<Listing> {
    io::safe_path(root)?;
    if !root.try_exists()? {
        return Ok(Listing::default());
    }
    storage::validate_root(root)?;
    let _global = io::lock(&root.join("store.lock"), cancel)?;
    let mut result = Listing::default();
    for run_id in io::entries(&root.join("runs"), super::MAX_RUNS, cancel)? {
        io::cancelled(cancel)?;
        if super::tombstones::is_discarded_run(&run_id) {
            continue;
        }
        match list_run(root, &run_id, include_clean, cancel) {
            Ok(mut part) => {
                result.checkpoints.append(&mut part.checkpoints);
                result.warnings.append(&mut part.warnings);
            }
            Err(error) => {
                io::cancelled(cancel)?;
                result.warnings.push(format!(
                    "Recovery run {run_id} was preserved and could not be listed: {error:#}"
                ));
            }
        }
    }
    result.checkpoints.sort_by(|a, b| {
        b.captured_unix_ms
            .cmp(&a.captured_unix_ms)
            .then_with(|| b.checkpoint.generation.cmp(&a.checkpoint.generation))
    });
    Ok(result)
}
fn list_run(
    root: &Path,
    id: &str,
    include_clean: bool,
    cancel: &dyn Fn() -> bool,
) -> Result<Listing> {
    let path = root.join("runs").join(id);
    manifest::validate_run(&path, id)?;
    let Some(_run_lock) = io::try_lock(&path.join("run.lock"))? else {
        return Ok(Listing::default());
    };
    let names = io::entries(&path.join("checkpoints"), 32, cancel)?;
    let latest = names
        .iter()
        .filter_map(|name| manifest::generation(name))
        .max()
        .unwrap_or(0);
    let mut result = Listing::default();
    let interrupted = if path.join("clean.json").try_exists()? {
        match clean_run(&path, id, latest) {
            Ok(()) => false,
            Err(error) => {
                result
                    .warnings
                    .push(format!("Clean-exit marker for {id} is damaged: {error:#}"));
                true
            }
        }
    } else {
        true
    };
    if !interrupted && !include_clean {
        return Ok(result);
    }
    for name in names {
        io::cancelled(cancel)?;
        let record = (|| -> Result<Checkpoint> {
            let generation = manifest::generation(&name).context("Unknown checkpoint filename")?;
            manifest::read_checkpoint(
                root,
                &CheckpointRef {
                    run_id: id.into(),
                    generation,
                },
            )
        })();
        match record {
            Ok(record) => result.checkpoints.push(record.info(interrupted)),
            Err(error) => result
                .warnings
                .push(format!("Checkpoint {id}/{name} was preserved: {error:#}")),
        }
    }
    Ok(result)
}
fn clean_run(path: &Path, id: &str, generation: u64) -> Result<()> {
    let clean: Clean = io::read_json(&path.join("clean.json"), 4096)?;
    clean.owner.validate()?;
    ensure!(
        clean.run_id == id && clean.generation == generation,
        "Clean-exit marker identity mismatch"
    );
    Ok(())
}
pub fn load(
    root: &Path,
    checkpoint: &CheckpointRef,
    cancel: &dyn Fn() -> bool,
) -> Result<Snapshot> {
    let (_run_lock, record) = validated_checkpoint(root, checkpoint, cancel)?;
    let audio = super::blobs::load_all(root, &record.data.audio, cancel)?;
    io::cancelled(cancel)?;
    Ok(Snapshot {
        payload: record.data.payload,
        audio,
    })
}
/// Inspect private local session metadata without opening or verifying PCM files.
pub fn inspect(
    root: &Path,
    checkpoint: &CheckpointRef,
    cancel: &dyn Fn() -> bool,
) -> Result<serde_json::Value> {
    let (_run_lock, record) = validated_checkpoint(root, checkpoint, cancel)?;
    io::cancelled(cancel)?;
    Ok(record.data.payload)
}
fn validated_checkpoint(
    root: &Path,
    checkpoint: &CheckpointRef,
    cancel: &dyn Fn() -> bool,
) -> Result<(std::fs::File, Checkpoint)> {
    io::cancelled(cancel)?;
    storage::validate_root(root)?;
    ensure!(
        manifest::valid_run(&checkpoint.run_id) && checkpoint.generation > 0,
        "Invalid recovery reference"
    );
    let global = io::lock(&root.join("store.lock"), cancel)?;
    let run = root.join("runs").join(&checkpoint.run_id);
    manifest::validate_run(&run, &checkpoint.run_id)?;
    let run_lock = io::try_lock(&run.join("run.lock"))?
        .context("This recovery session is still open in another window")?;
    let record = manifest::read_checkpoint(root, checkpoint)?;
    let bytes: u64 = record
        .data
        .audio
        .iter()
        .map(|entry| entry.samples * 4)
        .sum();
    ensure!(
        bytes <= super::DEFAULT_BUDGET_BYTES,
        "Recovery decoded attachments exceed 8 GiB"
    );
    // The run lock prevents dismissal; immutable referenced blobs cannot be collected.
    // Release the global lock before large PCM reads so another window can autosave.
    drop(global);
    io::cancelled(cancel)?;
    Ok((run_lock, record))
}
/// Explicitly dismiss all generations of one unlocked run. No generic recursive delete.
pub fn discard_run(root: &Path, run_id: &str, cancel: &dyn Fn() -> bool) -> Result<()> {
    io::cancelled(cancel)?;
    storage::validate_root(root)?;
    ensure!(manifest::valid_run(run_id), "Invalid recovery run ID");
    let _global = io::lock(&root.join("store.lock"), cancel)?;
    storage::validate_root(root)?;
    io::require_names(root, &["owner.json", "store.lock", "runs", "blobs"], cancel)?;
    let run = root.join("runs").join(run_id);
    io::safe_path(&run)?;
    if !run.try_exists()? {
        return Ok(());
    }
    manifest::validate_run(&run, run_id)?;
    let run_lock = io::try_lock(&run.join("run.lock"))?
        .context("Cannot dismiss recovery while its window is still open")?;
    let files = cleanup::run_files(root, run_id, cancel)?;
    for name in io::entries(&run.join("checkpoints"), 32, cancel)? {
        let generation = manifest::generation(&name).context("Unknown checkpoint preserved")?;
        manifest::read_checkpoint(
            root,
            &CheckpointRef {
                run_id: run_id.into(),
                generation,
            },
        )?;
    }
    if run.join("clean.json").try_exists()? {
        let latest = io::entries(&run.join("checkpoints"), 32, cancel)?
            .iter()
            .filter_map(|name| manifest::generation(name))
            .max()
            .unwrap_or(0);
        clean_run(&run, run_id, latest)?;
    }
    io::cancelled(cancel)?;
    // Windows cannot unlink an open lock. The global lock prevents a competing
    // loader from acquiring this run between releasing its lock and removing it.
    drop(run_lock);
    cleanup::remove_run(root, run_id, files)?;
    cleanup::garbage_collect(root, cancel)
        .context("Run dismissed; unused blob cleanup needs attention")
}

/// Retry private garbage cleanup independently of any run that may already have
/// been removed. It never dismisses live or historical checkpoints to free their
/// references; unknown or malformed material is preserved with an error.
pub fn cleanup_unused(root: &Path, cancel: &dyn Fn() -> bool) -> Result<()> {
    io::safe_path(root)?;
    if !root.try_exists()? {
        return Ok(());
    }
    io::cancelled(cancel)?;
    storage::validate_root(root)?;
    let _global = io::lock(&root.join("store.lock"), cancel)?;
    storage::validate_root(root)?;
    io::require_names(root, &["owner.json", "store.lock", "runs", "blobs"], cancel)?;
    super::tombstones::clear(root, cancel)?;
    // Verify the remaining namespace before pruning rather than report a false
    // "cleared" while unidentified private files still need attention.
    cleanup::inventory(root, cancel)?;
    cleanup::garbage_collect(root, cancel)?;
    io::cancelled(cancel)
}
