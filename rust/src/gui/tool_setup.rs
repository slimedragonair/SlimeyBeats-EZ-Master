//! Desktop action policy: prepare codecs only when the requested operation needs them.
use crate::{
    config::Settings,
    studio::{
        stem_import_plan::{self, PartSelection, PreferredFormat},
        stem_mix::StemKind,
    },
    studio_native::stem_bundle::PreparedImport,
};
use anyhow::Result;
use std::{
    path::{Path, PathBuf},
    sync::{Arc, atomic::AtomicBool},
};

pub(super) fn input_needs_codecs(path: &Path) -> bool {
    path.extension()
        .and_then(|e| e.to_str())
        .is_some_and(|ext| {
            !ext.eq_ignore_ascii_case("wav")
                && crate::audio::EXTENSIONS
                    .iter()
                    .any(|known| ext.eq_ignore_ascii_case(known))
        })
}
pub(super) fn export_needs_codecs(settings: &Settings, sample_rate: u32) -> bool {
    settings
        .formats
        .iter()
        .any(|format| !format.starts_with("wav"))
        || (settings.output_sr != 0 && settings.output_sr != sample_rate)
}
pub(super) fn prepare(
    needed: bool,
    cancel: Arc<AtomicBool>,
    progress: &dyn Fn(f32, &str),
) -> Result<()> {
    if needed {
        crate::optional_tools::ensure(
            &|fraction, message| progress(fraction * 0.06, message),
            cancel,
        )?;
    }
    Ok(())
}
pub(super) fn for_inputs(
    paths: impl IntoIterator<Item = impl AsRef<Path>>,
    cancel: Arc<AtomicBool>,
    progress: &dyn Fn(f32, &str),
) -> Result<()> {
    prepare(
        paths
            .into_iter()
            .any(|path| input_needs_codecs(path.as_ref())),
        cancel,
        progress,
    )
}
pub(super) fn for_export(
    settings: &Settings,
    sample_rate: u32,
    cancel: Arc<AtomicBool>,
    progress: &dyn Fn(f32, &str),
) -> Result<()> {
    prepare(export_needs_codecs(settings, sample_rate), cancel, progress)
}
pub(super) fn for_stems(
    files: &[(StemKind, PathBuf)],
    cancel: Arc<AtomicBool>,
    progress: &dyn Fn(f32, &str),
) -> Result<()> {
    for_inputs(files.iter().map(|(_, path)| path), cancel, progress)
}
pub(super) fn bundle_needs_codecs(
    prepared: &PreparedImport,
    selected: &[PartSelection],
    preference: &PreferredFormat,
) -> Result<bool> {
    Ok(
        stem_import_plan::select(&prepared.plan, selected, preference)?
            .iter()
            .any(|(_, version)| version.format != "wav"),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn native_wav_work_does_not_prepare_codecs_until_compressed_export_or_real_resampling() {
        assert!(!input_needs_codecs(Path::new("Mix.WAV")));
        assert!(input_needs_codecs(Path::new("Reference.Mp3")));
        assert!(input_needs_codecs(Path::new("Suno vocal.flac")));
        assert!(!input_needs_codecs(Path::new("not-audio.exe")));
        let mut settings = Settings {
            formats: vec!["wav24".into()],
            output_sr: 48000,
            ..Default::default()
        };
        assert!(!export_needs_codecs(&settings, 48000));
        settings.output_sr = 96000;
        assert!(export_needs_codecs(&settings, 48000));
        settings.output_sr = 0;
        settings.formats = vec!["mp3".into()];
        assert!(export_needs_codecs(&settings, 48000));
        assert!(
            !input_needs_codecs(Path::new("Mix.wav")),
            "Selecting MP3 export must not make WAV analysis download codecs"
        );
    }
    #[test]
    fn paired_zip_alternates_do_not_prepare_codecs_until_mp3_is_actually_selected() {
        let root = tempfile::tempdir().unwrap();
        let mut paths = Vec::new();
        for name in ["Vocals.wav", "Vocals.mp3", "Drums.wav", "Bass.wav"] {
            let path = root.path().join(name);
            std::fs::write(&path, b"metadata fixture").unwrap();
            paths.push(path);
        }
        let prepared = PreparedImport::files(paths).unwrap();
        let all: Vec<_> = (0..prepared.plan.parts.len())
            .map(|part_index| PartSelection {
                part_index,
                kind: None,
            })
            .collect();
        assert!(!bundle_needs_codecs(&prepared, &all, &PreferredFormat::Wav).unwrap());
        assert!(bundle_needs_codecs(&prepared, &all, &PreferredFormat::Mp3).unwrap());
        let wav_only: Vec<_> = prepared
            .plan
            .parts
            .iter()
            .enumerate()
            .filter(|(_, part)| part.versions.iter().all(|v| v.format == "wav"))
            .map(|(part_index, _)| PartSelection {
                part_index,
                kind: None,
            })
            .collect();
        assert_eq!(wav_only.len(), 2);
        assert!(!bundle_needs_codecs(&prepared, &wav_only, &PreferredFormat::Mp3).unwrap());
        assert!(bundle_needs_codecs(&prepared, &wav_only[..1], &PreferredFormat::Wav).is_err());
    }
}
