use anyhow::{Context, Result, ensure};
use mastercraft::audio;
use serde_json::{Value, json};
use std::path::{Path, PathBuf};

pub struct FilePolicy {
    pub inputs: Vec<PathBuf>,
    pub output: PathBuf,
}

impl FilePolicy {
    pub fn new(inputs: &[PathBuf], output: &Path) -> Result<Self> {
        ensure!(!inputs.is_empty(), "Configure at least one input folder");
        let inputs = inputs
            .iter()
            .map(|p| {
                let path = p.canonicalize().context("Input folder is unavailable")?;
                ensure!(path.is_dir(), "Input roots must be folders");
                Ok(path)
            })
            .collect::<Result<Vec<_>>>()?;
        std::fs::create_dir_all(output).context("Cannot create the configured output folder")?;
        let output = output.canonicalize()?;
        ensure!(output.is_dir(), "Output root must be a folder");
        Ok(Self { inputs, output })
    }

    pub fn resolve(&self, library: usize, relative: &str) -> Result<PathBuf> {
        ensure!(relative.len() <= 4096, "Audio path is too long");
        let root = self.inputs.get(library).context("Unknown library index")?;
        let path = root
            .join(relative)
            .canonicalize()
            .context("The selected path is unavailable")?;
        ensure!(
            path.starts_with(root),
            "Path is outside the configured input library"
        );
        Ok(path)
    }

    pub fn audio(&self, library: usize, relative: &str) -> Result<PathBuf> {
        let path = self.resolve(library, relative)?;
        ensure!(
            path.is_file() && audio::supported(&path),
            "Choose a supported audio file"
        );
        Ok(path)
    }

    pub fn check_output(&self) -> Result<()> {
        ensure!(
            self.output.canonicalize()? == self.output,
            "Output folder changed; restart the companion"
        );
        Ok(())
    }

    pub fn list(&self, library: usize, folder: &str, offset: usize, limit: usize) -> Result<Value> {
        ensure!(
            (1..=100).contains(&limit) && offset <= 10000,
            "List limit must be 1–100 and offset at most 10000"
        );
        let dir = self.resolve(library, folder)?;
        ensure!(dir.is_dir(), "Choose a library subfolder");
        let root = self.inputs.get(library).context("Unknown library")?;
        let mut entries = Vec::new();
        let mut count = 0;
        for entry in std::fs::read_dir(dir)? {
            let entry = entry?;
            count += 1;
            ensure!(
                count <= 10000,
                "Folder has too many entries; select a smaller subfolder"
            );
            let Ok(path) = entry.path().canonicalize() else {
                continue;
            };
            if !path.starts_with(root) {
                continue;
            }
            if path.is_dir() {
                entries.push(
                    json!({"path":path.strip_prefix(root)?.to_string_lossy(),"directory":true}),
                );
            } else if path.is_file()
                && (audio::supported(&path)
                    || path
                        .extension()
                        .is_some_and(|ext| ext.eq_ignore_ascii_case("zip")))
            {
                let archive = path
                    .extension()
                    .is_some_and(|ext| ext.eq_ignore_ascii_case("zip"));
                entries.push(json!({"path":path.strip_prefix(root)?.to_string_lossy(),"bytes":path.metadata()?.len(),"stem_archive":archive}));
            }
        }
        entries.sort_by(|a, b| a["path"].as_str().cmp(&b["path"].as_str()));
        let next = (offset + limit < entries.len()).then_some(offset + limit);
        let page = entries
            .into_iter()
            .skip(offset)
            .take(limit)
            .collect::<Vec<_>>();
        let folders = page
            .iter()
            .filter(|p| p["directory"] == true)
            .map(|p| p["path"].clone())
            .collect::<Vec<_>>();
        let files = page
            .into_iter()
            .filter(|p| p["directory"] != true)
            .collect::<Vec<_>>();
        Ok(json!({"library_index":library,"files":files,"folders":folders,"next_offset":next}))
    }

    pub fn relative_output(&self, path: &Path) -> Result<String> {
        Ok(path
            .strip_prefix(&self.output)?
            .to_string_lossy()
            .into_owned())
    }

    pub fn redact(&self, text: &str) -> String {
        let mut text = text.replace(&self.output.to_string_lossy().to_string(), "<output>");
        for input in &self.inputs {
            text = text.replace(&input.to_string_lossy().to_string(), "<library>");
        }
        text
    }
}

pub fn export_stem(value: &str) -> Result<String> {
    ensure!(
        !value.is_empty()
            && value.len() <= 180
            && !value.starts_with('.')
            && !value.ends_with(['.', ' ']),
        "Use a short output name without leading/trailing dots or spaces"
    );
    ensure!(
        !value
            .chars()
            .any(|c| c.is_control() || "<>:\"/\\|?*".contains(c))
            && !value.contains(".."),
        "Output name must be a filename stem, not a path"
    );
    let head = value
        .split('.')
        .next()
        .unwrap_or_default()
        .to_ascii_uppercase();
    ensure!(
        !["CON", "PRN", "AUX", "NUL"].contains(&head.as_str())
            && !(head.len() == 4
                && (head.starts_with("COM") || head.starts_with("LPT"))
                && head.ends_with(|c: char| c.is_ascii_digit())),
        "Reserved output name"
    );
    Ok(value.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn traversal_and_output_paths_are_restricted() {
        let dir = tempfile::tempdir().unwrap();
        let library = dir.path().join("library");
        std::fs::create_dir(&library).unwrap();
        std::fs::write(dir.path().join("outside.wav"), b"x").unwrap();
        let policy = FilePolicy::new(&[library], &dir.path().join("exports")).unwrap();
        assert!(policy.audio(0, "../outside.wav").is_err());
        assert!(policy.audio(1, "anything.wav").is_err());
        for bad in ["../escape", "nested/name", "CON", "a:b", "ending.", ""] {
            assert!(export_stem(bad).is_err(), "{bad}");
        }
        assert_eq!(export_stem("My Song – Mix 2").unwrap(), "My Song – Mix 2");
    }

    #[cfg(unix)]
    #[test]
    fn symlinks_cannot_escape_a_library() {
        let dir = tempfile::tempdir().unwrap();
        let library = dir.path().join("library");
        std::fs::create_dir(&library).unwrap();
        std::fs::write(dir.path().join("outside.wav"), b"x").unwrap();
        std::os::unix::fs::symlink(dir.path().join("outside.wav"), library.join("linked.wav"))
            .unwrap();
        let policy = FilePolicy::new(&[library], &dir.path().join("exports")).unwrap();
        assert!(policy.audio(0, "linked.wav").is_err());
    }
}
