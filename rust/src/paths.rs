use std::path::PathBuf;

pub fn app_root() -> PathBuf {
    let executable = std::env::current_exe().unwrap_or_else(|_| PathBuf::from("mastercraft.exe"));
    if let Some(directory) = executable.parent() {
        // Development builds live in rust/target/release; installed copies keep data beside the EXE.
        for ancestor in directory.ancestors().take(4) {
            if ancestor
                .join("mastercraft/data/builtin_profiles.json")
                .is_file()
            {
                return ancestor.to_path_buf();
            }
        }
        return directory.to_path_buf();
    }
    PathBuf::from(".")
}
