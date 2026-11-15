use super::{
    cleanup, filesystem as io,
    manifest::{self, Owner},
};
use anyhow::{Result, ensure};
use std::{fs, path::Path};

/// Finish only missing fixed scaffolding for an owned run whose creator died.
/// No user material or malformed ownership marker is adopted.
pub(super) fn repair_runs(root: &Path, cancel: &dyn Fn() -> bool) -> Result<()> {
    for id in io::entries(&root.join("runs"), super::MAX_RUNS, cancel)? {
        let path = root.join("runs").join(&id);
        manifest::validate_run(&path, &id)?;
        io::require_names(
            &path,
            &[
                "run.json",
                "run.lock",
                "checkpoints",
                "pending",
                "clean.json",
            ],
            cancel,
        )?;
        if !path.join("run.lock").try_exists()? {
            io::create_bytes(&path.join("run.lock"), &[])?;
        }
        let Some(lock) = io::try_lock(&path.join("run.lock"))? else {
            continue;
        };
        for name in ["checkpoints", "pending"] {
            if !path.join(name).try_exists()? {
                fs::create_dir(path.join(name))?;
            }
            io::directory(&path.join(name))?;
        }
        let pending = path.join("pending");
        if !pending.join("owner.json").try_exists()? {
            ensure!(
                io::entries(&pending, 1, cancel)?.is_empty(),
                "Unmarked recovery staging material was preserved"
            );
            io::create_json(&pending.join("owner.json"), &Owner::new())?;
        }
        // A run without a committed checkpoint has no user state to recover.
        // Reclaim its bounded, validated scaffold after releasing the Windows lock.
        if io::entries(&path.join("checkpoints"), 32, cancel)?.is_empty() {
            if path.join("clean.json").try_exists()? {
                let clean: super::storage::Clean = io::read_json(&path.join("clean.json"), 4096)?;
                clean.owner.validate()?;
                ensure!(
                    clean.run_id == id && clean.generation == 0,
                    "Damaged clean marker was preserved"
                );
            }
            let files = cleanup::run_files(root, &id, cancel)?;
            drop(lock);
            cleanup::remove_run(root, &id, files)?;
        }
    }
    Ok(())
}
