use super::{Runtime, cache, pins, probes};
use anyhow::{Context, Result, ensure};
use std::{
    fs,
    path::{Component, Path, PathBuf},
    sync::{Arc, atomic::AtomicBool},
};

#[derive(Clone, Debug)]
pub struct Paths {
    pub(super) root: PathBuf,
}

impl Paths {
    /// A dedicated absolute folder; a nonempty unowned folder is never adopted.
    pub fn with_root(root: impl Into<PathBuf>) -> Self {
        Self { root: root.into() }
    }

    pub fn user_cache() -> Result<Self> {
        let base = crate::paths::cache_root()
            .context("No absolute per-user cache directory is available for optional tools")?;
        Ok(Self::with_root(base.join("optional-tools/ffmpeg")))
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    pub fn locate(&self, program: &str) -> Option<PathBuf> {
        let name = probes::program(program)?;
        self.private_runtime().map(|r| {
            if name == "ffmpeg" {
                r.ffmpeg
            } else {
                r.ffprobe
            }
        })
    }

    pub fn available(&self) -> bool {
        self.locate("ffmpeg").is_some() && self.locate("ffprobe").is_some()
    }

    pub(super) fn private_runtime(&self) -> Option<Runtime> {
        cache::ready(self, pins::current()?, &Arc::new(AtomicBool::new(false)))
            .ok()
            .flatten()
    }
}

pub(super) fn safe_path(path: &Path) -> Result<()> {
    ensure!(
        path.is_absolute() && !path.components().any(|c| matches!(c, Component::ParentDir)),
        "Optional tools require an absolute cache path without parent traversal"
    );
    #[cfg(windows)]
    ensure!(
        !path
            .components()
            .any(|c| matches!(c, Component::Normal(part) if part.to_string_lossy().contains(':'))),
        "Cache paths must not contain alternate data streams"
    );
    for ancestor in path.ancestors() {
        match fs::symlink_metadata(ancestor) {
            Ok(metadata) => {
                ensure!(
                    !metadata.file_type().is_symlink(),
                    "Optional-tool paths must not contain symlinks"
                );
                #[cfg(windows)]
                {
                    use std::os::windows::fs::MetadataExt;
                    ensure!(
                        metadata.file_attributes() & 0x400 == 0,
                        "Optional-tool paths must not contain reparse points"
                    );
                }
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => return Err(error.into()),
        }
    }
    Ok(())
}
