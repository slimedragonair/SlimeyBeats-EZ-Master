//! Prepare multiple loose files or a Suno-style ZIP without changing the loaded bank.
use crate::studio::{
    stem_archive,
    stem_import_plan::{self, ImportFile, ImportPlan, PartSelection, PreferredFormat},
    stem_mix::StemSet,
};
use anyhow::{Context, Result, ensure};
use std::{
    fs::File,
    io::{BufWriter, Write},
    path::PathBuf,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    time::Duration,
};

#[derive(Clone)]
pub enum ImportSource {
    Files(Vec<PathBuf>),
    Archive(PathBuf),
}
#[derive(Clone)]
pub struct PreparedImport {
    pub source: ImportSource,
    pub plan: ImportPlan,
}
impl PreparedImport {
    pub fn files(paths: Vec<PathBuf>) -> Result<Self> {
        Self::files_with_mode(paths, false)
    }
    pub fn files_for_arrangement(paths: Vec<PathBuf>) -> Result<Self> {
        Self::files_with_mode(paths, true)
    }
    fn files_with_mode(paths: Vec<PathBuf>, arrangement: bool) -> Result<Self> {
        let mut metadata_bytes = 0;
        let mut files = Vec::new();
        for (index, path) in paths.iter().enumerate() {
            // Retain source folders so equal basenames in different folders stay independent.
            let name = path.to_str().context("Audio filename must be UTF-8")?;
            metadata_bytes = stem_import_plan::charge_metadata(metadata_bytes, name.len())?;
            files.push(ImportFile {
                index,
                format: stem_import_plan::file_format(name).unwrap_or_default(),
                name: name.into(),
                bytes: path.metadata()?.len(),
                crc32: None,
            });
        }
        let mut plan = if arrangement {
            stem_import_plan::plan_audio(files)?
        } else {
            stem_import_plan::plan(files)?
        };
        for part in &mut plan.parts {
            for version in &mut part.versions {
                version.name = paths[version.index]
                    .file_name()
                    .context("Audio filename is missing")?
                    .to_str()
                    .context("Audio filename must be UTF-8")?
                    .to_owned();
            }
        }
        Ok(Self {
            source: ImportSource::Files(paths),
            plan,
        })
    }
    pub fn archive(path: PathBuf) -> Result<Self> {
        Self::archive_with_mode(path, false)
    }
    pub fn archive_for_arrangement(path: PathBuf) -> Result<Self> {
        Self::archive_with_mode(path, true)
    }
    fn archive_with_mode(path: PathBuf, arrangement: bool) -> Result<Self> {
        ensure!(
            path.metadata()?.len() <= stem_archive::MAX_ARCHIVE_BYTES,
            "Desktop stem ZIP exceeds 1 GiB; import selected audio files instead"
        );
        let plan = if arrangement {
            let (files, skipped) =
                stem_archive::inspect_files(File::open(&path)?, stem_archive::MAX_ENTRY_BYTES)?;
            let mut plan = stem_import_plan::plan_audio(files)?;
            plan.skipped_files += skipped;
            plan
        } else {
            stem_archive::inspect(File::open(&path)?, stem_archive::MAX_ENTRY_BYTES)?
        };
        Ok(Self {
            source: ImportSource::Archive(path),
            plan,
        })
    }
    pub fn import(
        &self,
        selected: &[PartSelection],
        preferred: &PreferredFormat,
        max_decoded_bytes: usize,
        timeout: Duration,
        cancel: Arc<AtomicBool>,
        progress: &dyn Fn(f32, &str),
    ) -> Result<StemSet> {
        let selection = stem_import_plan::select(&self.plan, selected, preferred)?;
        let versions: Vec<_> = selection
            .iter()
            .map(|(_, version)| version.clone())
            .collect();
        self.with_versions(&versions, cancel.clone(), progress, |paths| {
            let files: Vec<_> = selection
                .iter()
                .zip(paths)
                .map(|((kind, _), path)| (*kind, path.clone()))
                .collect();
            let mut bank = super::stem_files::import_with_limits(
                &files,
                max_decoded_bytes,
                timeout,
                cancel,
                &|fraction, text| progress(0.2 + fraction * 0.8, text),
            )?;
            for ((kind, _), part) in selection.iter().zip(selected) {
                bank.set_display_name(*kind, self.plan.parts[part.part_index].name.clone())?;
            }
            Ok(bank)
        })
    }

    /// Arrangement imports use the same safe extraction and alternate-format grouping,
    /// while permitting one file and different durations. Paths live only for this callback.
    pub fn with_audio_files<T>(
        &self,
        selected: &[usize],
        preferred: &PreferredFormat,
        cancel: Arc<AtomicBool>,
        progress: &dyn Fn(f32, &str),
        work: impl FnOnce(&[(String, PathBuf)]) -> Result<T>,
    ) -> Result<T> {
        ensure!(!selected.is_empty(), "Choose at least one audio part");
        let mut seen = std::collections::BTreeSet::new();
        let mut names = Vec::new();
        let mut versions = Vec::new();
        for &index in selected {
            ensure!(
                seen.insert(index),
                "An audio part was selected more than once"
            );
            let part = self
                .plan
                .parts
                .get(index)
                .context("Unknown audio part index")?;
            names.push(part.name.clone());
            versions.push(part.preferred(preferred).clone());
        }
        self.with_versions(&versions, cancel, progress, |paths| {
            let named: Vec<_> = names.into_iter().zip(paths.iter().cloned()).collect();
            work(&named)
        })
    }

    fn with_versions<T>(
        &self,
        versions: &[ImportFile],
        cancel: Arc<AtomicBool>,
        progress: &dyn Fn(f32, &str),
        work: impl FnOnce(&[PathBuf]) -> Result<T>,
    ) -> Result<T> {
        let stage = tempfile::Builder::new()
            .prefix("slimey-stem-import-")
            .tempdir()?;
        let mut files = Vec::new();
        for (index, version) in versions.iter().enumerate() {
            ensure!(!cancel.load(Ordering::Relaxed), "Cancelled");
            let path = match &self.source {
                ImportSource::Files(paths) => paths
                    .get(version.index)
                    .context("Import file is missing")?
                    .clone(),
                ImportSource::Archive(path) => {
                    ensure!(
                        path.metadata()?.len() <= stem_archive::MAX_ARCHIVE_BYTES,
                        "Stem ZIP exceeds 1 GiB"
                    );
                    let destination = stage
                        .path()
                        .join(format!("part-{index}.{}", version.format));
                    let mut output = BufWriter::new(File::create(&destination)?);
                    progress(
                        index as f32 / versions.len() as f32 * 0.2,
                        &format!("Preparing {} from ZIP", version.name),
                    );
                    stem_archive::copy_audio(
                        File::open(path)?,
                        version,
                        stem_archive::MAX_ENTRY_BYTES,
                        &mut output,
                        &|| cancel.load(Ordering::Relaxed),
                    )?;
                    output.flush()?;
                    destination
                }
            };
            files.push(path);
        }
        work(&files)
    }
}

#[cfg(test)]
#[path = "arrangement_import_tests.rs"]
mod arrangement_import_tests;
