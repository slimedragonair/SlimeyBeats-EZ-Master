use anyhow::{Context, Result, ensure};
use mastercraft::{
    config::{MODULES, Settings},
    profiles,
};
use serde_json::Value;
use std::path::Path;

pub fn parse(value: &Value, out_dir: &Path) -> Result<Settings> {
    let object = value.as_object().context("settings must be an object")?;
    let defaults = serde_json::to_value(Settings::default())?;
    for (key, value) in object {
        ensure!(defaults.get(key).is_some(), "Unknown settings field: {key}");
        if key == "manual_eq" {
            for band in value.as_array().context("manual_eq must be an array")? {
                for field in band
                    .as_object()
                    .context("EQ band must be an object")?
                    .keys()
                {
                    ensure!(
                        ["type", "freq", "gain", "q", "on"].contains(&field.as_str()),
                        "Unknown EQ band field"
                    );
                }
            }
        }
        if key == "modules" {
            for (module, controls) in value.as_object().context("modules must be an object")? {
                ensure!(MODULES.contains(&module.as_str()), "Unknown DSP module");
                super::module_settings::validate(module, controls)?;
            }
        }
    }
    let mut settings: Settings = serde_json::from_value(value.clone())?;
    ensure!(
        settings.reference.is_none(),
        "Use reference_analysis, not a reference file path"
    );
    ensure!(
        !settings.profile.contains(['/', '\\'])
            && !settings.profile.contains("..")
            && !settings.profile.starts_with("custom:"),
        "Only bundled profiles or blends are supported"
    );
    settings.out_dir = out_dir.to_path_buf();
    settings.validate()?;
    Ok(settings)
}

pub fn profile(settings: &Settings) -> Result<Value> {
    // An empty private directory retains shared aliases/blends while preventing
    // its custom-file fallback from reading any caller-selected profile file.
    let directory = tempfile::Builder::new()
        .prefix(".profile-")
        .tempdir_in(&settings.out_dir)?;
    profiles::resolve(&settings.profile, directory.path())
}

pub fn before_eq(settings: &Settings) -> Settings {
    let mut result = settings.clone();
    for band in &mut result.manual_eq {
        band.on = false;
        band.gain = 0.;
    }
    result.eq_output_db = 0.;
    result
}
