//! Locate installed companions/examples, or their explicit development paths.
use std::path::{Path, PathBuf};

pub(super) fn file(root: &Path, names: &[PathBuf]) -> Option<PathBuf> {
    let executable = std::env::current_exe().ok()?;
    candidates(root, &executable, names)
        .into_iter()
        .find(|path| path.is_file())
}

fn candidates(root: &Path, executable: &Path, names: &[PathBuf]) -> Vec<PathBuf> {
    let mut bases = vec![root.to_path_buf()];
    if let Some(parent) = executable.parent() {
        bases.insert(0, parent.to_path_buf());
        for ancestor in parent.ancestors().take(5) {
            if ancestor
                .join("mastercraft/data/builtin_profiles.json")
                .is_file()
            {
                bases.push(ancestor.to_path_buf());
            }
        }
    }
    bases
        .into_iter()
        .flat_map(|base| names.iter().map(move |name| base.join(name)))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn only_known_development_roots_add_ancestor_resource_paths() {
        let folder = tempfile::tempdir().unwrap();
        let executable = folder.path().join("rust/target/debug/App.exe");
        let names = [PathBuf::from("Examples/demo-input.wav")];
        let initial = candidates(Path::new("other"), &executable, &names);
        assert!(!initial.contains(&folder.path().join(&names[0])));
        let marker = folder.path().join("mastercraft/data/builtin_profiles.json");
        std::fs::create_dir_all(marker.parent().unwrap()).unwrap();
        std::fs::write(marker, "{}").unwrap();
        let known = candidates(Path::new("other"), &executable, &names);
        assert!(known.contains(&folder.path().join(&names[0])));
    }
}
