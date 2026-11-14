use std::path::PathBuf;

#[cfg(target_os = "linux")]
const APP_DIRECTORY: &str = "slimeybeats-ez-master";

/// Linux uses the user's config directory; Windows retains its portable layout.
pub fn settings_file() -> PathBuf {
    #[cfg(target_os = "linux")]
    if let Some(root) = xdg_directory(
        std::env::var_os("XDG_CONFIG_HOME").as_deref(),
        std::env::var_os("HOME").as_deref(),
        ".config",
    ) {
        return root.join("rust-settings.json");
    }
    app_root().join("user_data/rust-settings.json")
}

pub fn desktop_preferences_file() -> PathBuf {
    settings_file().with_file_name("desktop-preferences.json")
}

pub fn interface_preferences_file() -> PathBuf {
    settings_file().with_file_name("interface-preferences.json")
}

pub fn app_root() -> PathBuf {
    #[cfg(target_os = "linux")]
    if let Some(root) = xdg_directory(
        std::env::var_os("XDG_DATA_HOME").as_deref(),
        std::env::var_os("HOME").as_deref(),
        ".local/share",
    ) {
        return root;
    }
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

#[cfg(target_os = "linux")]
fn xdg_directory(
    configured: Option<&std::ffi::OsStr>,
    home: Option<&std::ffi::OsStr>,
    fallback: &str,
) -> Option<PathBuf> {
    // The XDG specification requires absolute paths; ignore relative environment values.
    configured
        .map(PathBuf::from)
        .filter(|path| path.is_absolute())
        .or_else(|| {
            home.map(PathBuf::from)
                .filter(|path| path.is_absolute())
                .map(|path| path.join(fallback))
        })
        .map(|path| path.join(APP_DIRECTORY))
}

#[cfg(all(test, target_os = "linux"))]
mod tests {
    use super::*;
    use std::ffi::OsStr;

    #[test]
    fn absolute_xdg_path_takes_precedence_and_preserves_spaces() {
        assert_eq!(
            xdg_directory(
                Some(OsStr::new("/tmp/custom data")),
                Some(OsStr::new("/home/test")),
                ".local/share"
            ),
            Some(PathBuf::from("/tmp/custom data/slimeybeats-ez-master"))
        );
    }

    #[test]
    fn relative_xdg_values_are_ignored_for_the_home_fallback() {
        assert_eq!(
            xdg_directory(
                Some(OsStr::new("relative")),
                Some(OsStr::new("/home/test")),
                ".config"
            ),
            Some(PathBuf::from("/home/test/.config/slimeybeats-ez-master"))
        );
        assert_eq!(
            xdg_directory(None, Some(OsStr::new("relative-home")), ".config"),
            None
        );
        assert_eq!(xdg_directory(None, None, ".config"), None);
    }
}
