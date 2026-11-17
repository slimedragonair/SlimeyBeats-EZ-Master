//! Output-root ownership receipts and bounded, link-free canonical project access.
use crate::state::Engine;
use anyhow::{Context, Result, ensure};
use mastercraft::{
    source_identity::SourceIdentity,
    studio::arrangement_project::{ArrangementManifest, MAX_PROJECT_MANIFEST_BYTES},
};
use serde::{Deserialize, Serialize};
use std::{
    fs,
    io::{Read, Write},
    path::{Component, Path, PathBuf},
};

const DIRECTORY: &str = ".slimeybeats-arrangement-projects";
const OWNER: &str = "owner.json";
const RECEIPT: &str = "mcp-project.json";
const RECEIPT_BYTES: usize = 8192;

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct Owner {
    version: u32,
    id: String,
    output_root: PathBuf,
}

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct Receipt {
    version: u32,
    owner: String,
    project_id: String,
    manifest: String,
    identity: SourceIdentity,
}

#[derive(Debug)]
pub struct SavedProject {
    pub id: String,
    pub manifest: PathBuf,
    pub identity: SourceIdentity,
}

pub struct Reservation {
    directory: tempfile::TempDir,
    root: PathBuf,
    owner: String,
}

fn no_link(metadata: &fs::Metadata) -> bool {
    if metadata.file_type().is_symlink() {
        return false;
    }
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        const REPARSE_POINT: u32 = 0x400;
        if metadata.file_attributes() & REPARSE_POINT != 0 {
            return false;
        }
    }
    true
}

/// Every component below the configured canonical output root must be ordinary.
fn contained(engine: &Engine, path: &Path, directory: bool) -> Result<()> {
    engine.policy.check_output()?;
    let relative = path.strip_prefix(&engine.policy.output)?;
    let mut current = engine.policy.output.clone();
    let root = fs::symlink_metadata(&current)?;
    ensure!(
        no_link(&root) && root.is_dir(),
        "Project output root must be an ordinary directory"
    );
    for component in relative.components() {
        let Component::Normal(part) = component else {
            anyhow::bail!("Project paths must be contained ordinary components");
        };
        current.push(part);
        let metadata = fs::symlink_metadata(&current)?;
        ensure!(
            no_link(&metadata),
            "Project symlinks and reparse points are not allowed"
        );
        if current != path {
            ensure!(metadata.is_dir(), "Project parent is not a directory");
        }
    }
    let metadata = fs::symlink_metadata(path)?;
    ensure!(
        if directory {
            metadata.is_dir()
        } else {
            metadata.is_file()
        },
        "Project path has the wrong file type"
    );
    ensure!(
        path.canonicalize()? == path,
        "Project path changed or escaped its output root"
    );
    Ok(())
}

fn read_bounded(engine: &Engine, path: &Path, limit: usize) -> Result<Vec<u8>> {
    contained(engine, path, false)?;
    ensure!(
        fs::metadata(path)?.len() <= limit as u64,
        "Project metadata exceeds its byte budget"
    );
    let mut bytes = Vec::new();
    fs::File::open(path)?
        .take(limit as u64 + 1)
        .read_to_end(&mut bytes)?;
    ensure!(
        bytes.len() <= limit,
        "Project metadata grew beyond its byte budget"
    );
    contained(engine, path, false)?;
    Ok(bytes)
}

fn write_new(path: &Path, bytes: &[u8]) -> Result<()> {
    let mut file = fs::File::create_new(path)?;
    file.write_all(bytes)?;
    file.sync_all()?;
    Ok(())
}

fn root(engine: &Engine, create: bool) -> Result<(PathBuf, Owner)> {
    engine.policy.check_output()?;
    let root = engine.policy.output.join(DIRECTORY);
    if create {
        match fs::create_dir(&root) {
            Ok(()) => {}
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {}
            Err(error) => return Err(error.into()),
        }
    }
    contained(engine, &root, true).context("Owned arrangement project store is unavailable")?;
    let path = root.join(OWNER);
    if create && !path.try_exists()? {
        let owner = Owner {
            version: 1,
            id: crate::session_cache::connection_id(),
            output_root: engine.policy.output.clone(),
        };
        match write_new(&path, &serde_json::to_vec(&owner)?) {
            Ok(()) => {}
            Err(error)
                if error
                    .downcast_ref::<std::io::Error>()
                    .is_some_and(|error| error.kind() == std::io::ErrorKind::AlreadyExists) => {}
            Err(error) => return Err(error),
        }
    }
    let owner: Owner = serde_json::from_slice(&read_bounded(engine, &path, RECEIPT_BYTES)?)?;
    ensure!(
        owner.version == 1
            && owner.output_root == engine.policy.output
            && owner.id.len() == 16
            && owner.id.bytes().all(|b| b.is_ascii_hexdigit()),
        "Project store belongs to a different output root or has invalid ownership"
    );
    Ok((root, owner))
}

fn validate_id(id: &str) -> Result<()> {
    let suffix = id
        .strip_prefix("arrangement-")
        .context("Use an owned arrangement project_id, not a path")?;
    ensure!(
        (6..=64).contains(&suffix.len()) && suffix.bytes().all(|b| b.is_ascii_alphanumeric()),
        "Use the exact owned project_id; paths and foreign IDs are rejected"
    );
    Ok(())
}

impl Reservation {
    pub fn new(engine: &Engine) -> Result<Self> {
        let (root, owner) = root(engine, true)?;
        let directory = tempfile::Builder::new()
            .prefix("arrangement-")
            .tempdir_in(&root)?;
        contained(engine, directory.path(), true)?;
        Ok(Self {
            directory,
            root,
            owner: owner.id,
        })
    }
    pub fn path(&self) -> &Path {
        self.directory.path()
    }

    /// Called at the guarded commit point. Before this, Drop removes only this reservation.
    pub fn publish(self, engine: &Engine, manifest: &Path) -> Result<SavedProject> {
        let (root, owner) = root(engine, false)?;
        ensure!(
            root == self.root && owner.id == self.owner,
            "Project ownership changed while saving"
        );
        contained(engine, self.path(), true)?;
        contained(engine, manifest, false)?;
        let relative = manifest.strip_prefix(self.path())?;
        ensure!(
            relative.components().count() == 2
                && relative.file_name() == Some(std::ffi::OsStr::new("project.json")),
            "Unexpected canonical project manifest location"
        );
        let bytes = read_bounded(engine, manifest, MAX_PROJECT_MANIFEST_BYTES)?;
        ArrangementManifest::from_json(&bytes)?;
        let id = self
            .path()
            .file_name()
            .unwrap()
            .to_str()
            .context("Invalid generated project ID")?
            .to_owned();
        validate_id(&id)?;
        let identity = SourceIdentity::for_bytes(&bytes);
        let receipt = Receipt {
            version: 1,
            owner: self.owner.clone(),
            project_id: id.clone(),
            manifest: relative.to_string_lossy().replace('\\', "/"),
            identity: identity.clone(),
        };
        write_new(&self.path().join(RECEIPT), &serde_json::to_vec(&receipt)?)?;
        let manifest = manifest.to_path_buf();
        let _ = self.directory.keep();
        Ok(SavedProject {
            id,
            manifest,
            identity,
        })
    }
}

pub fn resolve(engine: &Engine, id: &str) -> Result<SavedProject> {
    validate_id(id)?;
    let (root, owner) = root(engine, false)?;
    let directory = root.join(id);
    contained(engine, &directory, true).context("Unknown or foreign arrangement project_id")?;
    let receipt: Receipt = serde_json::from_slice(&read_bounded(
        engine,
        &directory.join(RECEIPT),
        RECEIPT_BYTES,
    )?)?;
    ensure!(
        receipt.version == 1 && receipt.owner == owner.id && receipt.project_id == id,
        "Project receipt is foreign or invalid"
    );
    let relative = Path::new(&receipt.manifest);
    ensure!(
        relative.components().count() == 2
            && relative
                .components()
                .all(|part| matches!(part, Component::Normal(_)))
            && relative.file_name() == Some(std::ffi::OsStr::new("project.json")),
        "Invalid owned project manifest reference"
    );
    let manifest = directory.join(relative);
    let bytes = read_bounded(engine, &manifest, MAX_PROJECT_MANIFEST_BYTES)?;
    ensure!(
        SourceIdentity::for_bytes(&bytes) == receipt.identity,
        "Saved arrangement manifest hash mismatch"
    );
    let parsed = ArrangementManifest::from_json(&bytes)?;
    let parent = manifest.parent().unwrap();
    for source in &parsed.sources {
        contained(engine, &parent.join(&source.file), false)?;
    }
    Ok(SavedProject {
        id: id.into(),
        manifest,
        identity: receipt.identity,
    })
}
