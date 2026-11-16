use anyhow::{Context, Result, ensure};
use std::{
    fs,
    path::{Component, Path, PathBuf},
};

pub struct ProjectPaths {
    pub root: PathBuf,
}

fn absolute_clean(path: &Path) -> Result<()> {
    ensure!(path.is_absolute(), "Paths must be absolute");
    ensure!(
        !path
            .components()
            .any(|c| matches!(c, Component::ParentDir | Component::CurDir)),
        "Path traversal is forbidden"
    );
    // Win32 alternate streams and device paths must not alias ordinary files.
    for part in path.components() {
        if let Component::Normal(name) = part {
            let text = name.to_string_lossy();
            ensure!(
                !text.contains(':') && !text.ends_with(['.', ' ']),
                "Invalid file path component"
            );
        }
    }
    Ok(())
}

fn linked(metadata: &fs::Metadata) -> bool {
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        if metadata.file_attributes() & 0x400 != 0 {
            return true;
        }
    }
    metadata.file_type().is_symlink()
}

pub fn no_links(path: &Path) -> Result<()> {
    absolute_clean(path)?;
    for ancestor in path.ancestors() {
        match fs::symlink_metadata(ancestor) {
            Ok(metadata) => ensure!(
                !linked(&metadata),
                "Symlink/reparse output paths are forbidden"
            ),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => return Err(error.into()),
        }
    }
    Ok(())
}

impl ProjectPaths {
    pub fn new(root: &Path) -> Result<Self> {
        no_links(root)?;
        let root = root.canonicalize().context("Resolve project root")?;
        ensure!(root.is_dir(), "Project root must exist and be a directory");
        Ok(Self { root })
    }
    pub fn input(&self, path: &Path) -> Result<PathBuf> {
        absolute_clean(path)?;
        let actual = path.canonicalize().context("Resolve private input")?;
        ensure!(
            actual.starts_with(&self.root) && actual.is_file(),
            "Input must be a regular file within project_root"
        );
        Ok(actual)
    }
    pub fn directory(&self, path: &Path) -> Result<PathBuf> {
        no_links(path)?;
        // Prove containment before creating any missing ancestors.
        let mut ancestor = path;
        while !ancestor.exists() {
            ancestor = ancestor.parent().context("Missing output parent")?;
        }
        ensure!(
            ancestor.canonicalize()?.starts_with(&self.root),
            "Output directory must be within project_root"
        );
        fs::create_dir_all(path)?;
        no_links(path)?;
        let actual = path.canonicalize()?;
        ensure!(
            actual.starts_with(&self.root),
            "Output directory escaped project_root"
        );
        Ok(actual)
    }
    pub fn response(&self, path: &Path, out_dir: &Path) -> Result<PathBuf> {
        no_links(path)?;
        ensure!(!path.exists(), "Response file already exists");
        let parent = path
            .parent()
            .context("Missing response directory")?
            .canonicalize()?;
        ensure!(
            parent.starts_with(out_dir),
            "Response must be under out_dir"
        );
        Ok(parent.join(path.file_name().context("Missing response filename")?))
    }
}
