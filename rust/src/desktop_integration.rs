//! Launch the platform's file manager without shell expansion or option-like relative paths.
use anyhow::{Context, Result, ensure};
use std::{path::Path, process::Command};

fn folder_command(path: &Path) -> Result<Command> {
    ensure!(
        path.is_dir(),
        "Output folder does not exist yet: {}",
        path.display()
    );
    let absolute = std::path::absolute(path).context("Resolve output folder")?;
    let program = if cfg!(target_os = "windows") {
        "explorer.exe"
    } else if cfg!(target_os = "macos") {
        "open"
    } else {
        "xdg-open"
    };
    let mut command = crate::audio::external_command(program);
    command.arg(absolute);
    Ok(command)
}

pub fn open_folder(path: &Path) -> Result<()> {
    folder_command(path)?
        .spawn()
        .context("Start the desktop file manager")?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn folder_is_one_literal_absolute_argument() {
        let root = tempfile::tempdir().unwrap();
        let path = root.path().join("-mix with spaces;literal");
        std::fs::create_dir(&path).unwrap();
        let command = folder_command(&path).unwrap();
        assert_eq!(
            command.get_args().collect::<Vec<_>>(),
            vec![path.as_os_str()]
        );
        #[cfg(target_os = "linux")]
        assert_eq!(command.get_program(), "xdg-open");
        #[cfg(windows)]
        assert_eq!(command.get_program(), "explorer.exe");
    }

    #[test]
    fn missing_output_folder_is_an_error_without_launching_a_file_manager() {
        let root = tempfile::tempdir().unwrap();
        assert!(folder_command(&root.path().join("not-exported-yet")).is_err());
    }
}
