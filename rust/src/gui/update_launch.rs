//! Open the already verified interactive installer; never patch a running executable.
use crate::app_update::{VerifiedInstaller, WINDOWS_X86_64};
use anyhow::{Context, Result, ensure};
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};

pub(super) fn managed_installation() -> bool {
    if !cfg!(windows) {
        return false;
    }
    let Ok(executable) = std::env::current_exe() else {
        return false;
    };
    let Some(folder) = executable.parent() else {
        return false;
    };
    executable
        .file_name()
        .and_then(|name| name.to_str())
        .is_some_and(|name| name.eq_ignore_ascii_case("SlimeyBeats-EZ-Master.exe"))
        && folder.join("install-info.ini").is_file()
        && folder.join("Uninstall.exe").is_file()
}

pub(super) fn open(installer: &VerifiedInstaller, cancel: Arc<AtomicBool>) -> Result<()> {
    ensure!(
        cfg!(all(windows, target_arch = "x86_64")) && installer.platform == WINDOWS_X86_64,
        "Open the release page for installation instructions on this platform"
    );
    installer.verify(cancel.clone())?;
    ensure!(!cancel.load(Ordering::Relaxed), "Cancelled");
    // The existing setup restores the registered custom directory. No /D parsing,
    // silent replacement, elevation request, shell expansion or PATH changes.
    crate::audio::external_command(
        installer
            .path
            .to_str()
            .context("Installer path is not Unicode")?,
    )
    .spawn()
    .context("Cannot open the verified installer; your application remains open")?;
    Ok(())
}
