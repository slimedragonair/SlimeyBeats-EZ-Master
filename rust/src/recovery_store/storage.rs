use super::{
    CheckpointInfo, Snapshot,
    blobs::{self, Memo},
    cleanup, filesystem as io,
    manifest::{self, Checkpoint, Data, Owner, Run},
};
use anyhow::{Result, ensure};
use serde::{Deserialize, Serialize};
use std::{
    collections::HashSet,
    fs::{self, File},
    path::{Path, PathBuf},
    sync::atomic::{AtomicU64, Ordering},
};

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Root {
    owner: Owner,
    pub budget_bytes: u64,
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Clean {
    pub owner: Owner,
    pub run_id: String,
    pub generation: u64,
}

pub struct Store {
    root: PathBuf,
    run_id: String,
    budget: u64,
    generation: u64,
    finished: bool,
    run_lock: Option<File>,
    memo: Memo,
}
pub(super) fn validate_root(root: &Path) -> Result<Root> {
    let marker = read_root_owner(root)?;
    io::directory(&root.join("runs"))?;
    io::directory(&root.join("blobs"))?;
    io::regular(&root.join("store.lock"))?;
    Ok(marker)
}
fn read_root_owner(root: &Path) -> Result<Root> {
    io::directory(root)?;
    let marker: Root = io::read_json(&root.join("owner.json"), 4096)?;
    marker.owner.validate()?;
    ensure!(
        marker.budget_bytes >= 4096 && marker.budget_bytes <= super::DEFAULT_BUDGET_BYTES,
        "Invalid recovery storage budget"
    );
    Ok(marker)
}
impl Store {
    pub fn open(root: &Path, budget_bytes: u64, cancel: &dyn Fn() -> bool) -> Result<Self> {
        io::cancelled(cancel)?;
        io::safe_path(root)?;
        ensure!(
            (4096..=super::DEFAULT_BUDGET_BYTES).contains(&budget_bytes),
            "Recovery budget must be between 4 KiB and 8 GiB"
        );
        if !root.try_exists()? {
            if let Some(parent) = root.parent() {
                fs::create_dir_all(parent)?;
                io::directory(parent)?;
            }
            fs::create_dir(root)?;
            io::private_root(root)?;
            io::create_json(
                &root.join("owner.json"),
                &Root {
                    owner: Owner::new(),
                    budget_bytes,
                },
            )?;
        }
        let marker = read_root_owner(root)?;
        io::private_root(root)?;
        ensure!(
            marker.budget_bytes == budget_bytes,
            "Recovery store uses a different global budget; preserve it or explicitly clear its owned history first"
        );
        if !root.join("store.lock").try_exists()? {
            match fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(root.join("store.lock"))
            {
                Ok(file) => file.sync_all()?,
                Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => (),
                Err(error) => return Err(error.into()),
            }
        }
        let _global = io::lock(&root.join("store.lock"), cancel)?;
        for name in ["runs", "blobs"] {
            if !root.join(name).try_exists()? {
                fs::create_dir(root.join(name))?;
            }
            io::directory(&root.join(name))?;
        }
        io::sync_directory(root)?;
        super::tombstones::clear(root, cancel)?;
        super::initialization::repair_runs(root, cancel)?;
        let (used, _) = cleanup::inventory(root, cancel)?;
        ensure!(
            io::entries(&root.join("runs"), super::MAX_RUNS, cancel)?.len() < super::MAX_RUNS,
            "Recovery history has too many runs; clear an older run"
        );
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)?
            .as_nanos();
        let run_id = format!(
            "run-{nanos}-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        );
        let run = Run {
            owner: Owner::new(),
            run_id: run_id.clone(),
            created_unix_ms: io::now_ms()?,
        };
        let run_bytes = io::json_bytes(&run, 4096)?;
        let pending_marker = io::json_bytes(&Owner::new(), 4096)?;
        ensure!(
            used + run_bytes.len() as u64
                + pending_marker.len() as u64
                + cleanup::exit_reserve(root, cancel)?
                + super::EXIT_RESERVE_BYTES
                <= budget_bytes,
            "Recovery storage is full; clear an older recovery run"
        );
        let path = root.join("runs").join(&run_id);
        fs::create_dir(&path)?;
        io::create_bytes(&path.join("run.json"), &run_bytes)?;
        io::create_bytes(&path.join("run.lock"), &[])?;
        let run_lock = io::lock(&path.join("run.lock"), cancel)?;
        fs::create_dir(path.join("checkpoints"))?;
        fs::create_dir(path.join("pending"))?;
        io::create_bytes(&path.join("pending").join("owner.json"), &pending_marker)?;
        io::sync_directory(&path)?;
        io::sync_directory(&root.join("runs"))?;
        Ok(Self {
            root: root.into(),
            run_id,
            budget: budget_bytes,
            generation: 0,
            finished: false,
            run_lock: Some(run_lock),
            memo: Memo::default(),
        })
    }
    pub fn root(&self) -> &Path {
        &self.root
    }
    pub fn run_id(&self) -> &str {
        &self.run_id
    }
    pub fn save(
        &mut self,
        snapshot: &Snapshot,
        cancel: &dyn Fn() -> bool,
    ) -> Result<CheckpointInfo> {
        ensure!(!self.finished, "This recovery run has already finished");
        io::cancelled(cancel)?;
        let refs = self.memo.resolve(snapshot, cancel)?;
        let generation = self
            .generation
            .checked_add(1)
            .ok_or_else(|| anyhow::anyhow!("Recovery generation overflow"))?;
        let checkpoint = Checkpoint::new(Data {
            run_id: self.run_id.clone(),
            generation,
            captured_unix_ms: io::now_ms()?,
            payload: snapshot.payload.clone(),
            audio: refs,
        })?;
        let encoded = io::json_bytes(&checkpoint, super::MAX_METADATA_BYTES)?;
        let _global = io::lock(&self.root.join("store.lock"), cancel)?;
        ensure!(
            validate_root(&self.root)?.budget_bytes == self.budget,
            "Recovery store budget changed; reopen the recovery worker"
        );
        super::tombstones::clear(&self.root, cancel)?;
        let pending = self.root.join("runs").join(&self.run_id).join("pending");
        cleanup::clear_pending(&pending, cancel)?;
        // Interrupted saves can publish complete blobs without a checkpoint. Reclaim
        // only provably unreferenced owned blobs before checking available space.
        let prior_cleanup = cleanup::garbage_collect(&self.root, cancel).err();
        io::cancelled(cancel)?;
        let (used, blob_count) = cleanup::inventory(&self.root, cancel)?;
        let mut seen = HashSet::new();
        let mut additional = encoded.len() as u64;
        let mut new_count = 0;
        for entry in &checkpoint.data.audio {
            if seen.insert(&entry.sha256) {
                if self.root.join("blobs").join(&entry.sha256).try_exists()? {
                    self.memo.check_existing(&self.root, entry, cancel)?;
                } else {
                    additional = additional
                        .checked_add(blobs::estimated_bytes(entry)?)
                        .ok_or_else(|| anyhow::anyhow!("Recovery storage size overflow"))?;
                    new_count += 1;
                }
            }
        }
        ensure!(
            blob_count + new_count <= super::MAX_BLOBS,
            "Recovery storage has too many audio assets; clear older recovery runs"
        );
        let exit_reserve = cleanup::exit_reserve(&self.root, cancel)?;
        ensure!(
            used.checked_add(additional)
                .and_then(|n| n.checked_add(exit_reserve))
                .is_some_and(|n| n <= self.budget),
            "Recovery storage is full; editing can continue, but clear older recovery runs to resume autosave"
        );
        for (audio, entry) in snapshot.audio.iter().zip(&checkpoint.data.audio) {
            io::cancelled(cancel)?;
            blobs::write(&self.root, &pending, audio, entry, cancel)?;
            self.memo.remember_written(&self.root, entry)?;
        }
        io::create_bytes(&pending.join("checkpoint.json"), &encoded)?;
        io::cancelled(cancel)?;
        let folder = self
            .root
            .join("runs")
            .join(&self.run_id)
            .join("checkpoints");
        let destination = folder.join(manifest::filename(generation));
        ensure!(
            !destination.try_exists()?,
            "Recovery checkpoint already exists; refusing to overwrite it"
        );
        fs::rename(pending.join("checkpoint.json"), destination)?;
        // Once rename commits, report a successful checkpoint even if retention fails.
        self.generation = generation;
        let mut info = checkpoint.info(true);
        let maintenance = io::sync_directory(&folder)
            .and_then(|_| cleanup::trim_run(&self.root, &self.run_id, cancel))
            .and_then(|_| cleanup::garbage_collect(&self.root, cancel));
        if let Some(error) = maintenance.err().or(prior_cleanup) {
            info.maintenance_warning = Some(format!(
                "Recovery saved, but maintenance needs attention: {error:#}"
            ));
        }
        Ok(info)
    }
    pub fn finish(&mut self, cancel: &dyn Fn() -> bool) -> Result<()> {
        if self.finished {
            return Ok(());
        }
        let _global = io::lock(&self.root.join("store.lock"), cancel)?;
        ensure!(
            validate_root(&self.root)?.budget_bytes == self.budget,
            "Recovery store budget changed; reopen the recovery worker"
        );
        super::tombstones::clear(&self.root, cancel)?;
        let run = self.root.join("runs").join(&self.run_id);
        manifest::validate_run(&run, &self.run_id)?;
        cleanup::clear_pending(&run.join("pending"), cancel)?;
        if self.generation == 0 {
            let files = cleanup::run_files(&self.root, &self.run_id, cancel)?;
            io::cancelled(cancel)?;
            self.run_lock.take();
            cleanup::remove_run(&self.root, &self.run_id, files)?;
            self.finished = true;
            return Ok(());
        }
        let marker = Clean {
            owner: Owner::new(),
            run_id: self.run_id.clone(),
            generation: self.generation,
        };
        let encoded = io::json_bytes(&marker, 4096)?;
        ensure!(
            encoded.len() as u64 <= super::EXIT_RESERVE_BYTES,
            "Clean-exit marker exceeded its reserved space"
        );
        let (used, _) = cleanup::inventory(&self.root, cancel)?;
        ensure!(
            used + encoded.len() as u64 <= self.budget,
            "Recovery storage is too full to record a clean exit"
        );
        io::create_bytes(&run.join("pending").join("clean.json"), &encoded)?;
        io::cancelled(cancel)?;
        fs::rename(
            run.join("pending").join("clean.json"),
            run.join("clean.json"),
        )?;
        self.finished = true;
        io::sync_directory(&run)?;
        Ok(())
    }
}
