//! Read canonical native-compatible project ZIPs without extracting paths.
use super::{ArrangementManifest, ArrangementSnapshot, MAX_PROJECT_MANIFEST_BYTES, ProjectLoader};
use crate::studio::stem_import_plan::charge_metadata;
use anyhow::{Context, Result, ensure};
use std::{
    collections::BTreeMap,
    io::{Cursor, Read},
};
use zip::{ZipReadOptions, read::Config, unstable::read::ZipIterable};

pub fn load_zip(
    bytes: &[u8],
    max_archive_bytes: u64,
    max_decoded_bytes: u64,
) -> Result<ArrangementSnapshot> {
    ensure!(
        bytes.len() as u64 <= max_archive_bytes,
        "Project ZIP exceeds the encoded budget"
    );
    let mut archive = ZipIterable::try_new(Cursor::new(bytes), Config::default())?
        .enforce_header_consistency(true);
    let mut files = BTreeMap::new();
    let mut metadata = 0;
    for entry in archive.files()? {
        let entry = entry?;
        let name = entry.name()?.into_owned();
        metadata = charge_metadata(metadata, name.len())?;
        ensure!(
            !name.starts_with('/')
                && !name.contains(['\\', '\0', ':'])
                && name.split('/').all(|part| part != "." && part != "..")
                && entry.enclosed_name().is_some()
                && !entry.is_symlink()
                && !entry.encrypted(),
            "Project ZIP contains an unsafe entry"
        );
        if entry.is_dir() {
            continue;
        }
        ensure!(
            files.insert(name, entry.size()).is_none(),
            "Project ZIP contains duplicate member names"
        );
    }
    let manifest_bytes = member(bytes, "arrangement.json", MAX_PROJECT_MANIFEST_BYTES as u64)?;
    let manifest = ArrangementManifest::from_json(&manifest_bytes)?;
    let descriptors = manifest.sources.clone();
    let mut loader = ProjectLoader::new(manifest, max_decoded_bytes)?;
    // Equal-content logical sources may share a member while retaining IDs.
    for source in descriptors {
        ensure!(
            files.get(&source.file) == Some(&source.identity.bytes),
            "Project source size differs from its manifest"
        );
        let payload = member(bytes, &source.file, source.identity.bytes)?;
        loader.add_source(&source.id, &payload)?;
    }
    loader.finish()
}

fn member(bytes: &[u8], selected: &str, limit: u64) -> Result<Vec<u8>> {
    let mut archive = ZipIterable::try_new(Cursor::new(bytes), Config::default())?
        .enforce_header_consistency(true);
    let entries = archive.files()?;
    while let Some(entry) = entries.next() {
        let entry = entry?;
        if entry.name()?.as_ref() != selected {
            continue;
        }
        ensure!(
            entry.size() <= limit && !entry.encrypted() && !entry.is_symlink(),
            "Project member exceeds its budget or is unsupported"
        );
        let size = entry.size();
        ensure!(
            matches!(
                entry.compression(),
                zip::CompressionMethod::Stored | zip::CompressionMethod::Deflated
            ),
            "Unsupported project ZIP compression"
        );
        let mut reader = entry.with_reader(entries.reader_mut(), ZipReadOptions::default())?;
        let mut output = Vec::new();
        (&mut reader)
            .take(limit.saturating_add(1))
            .read_to_end(&mut output)
            .context("Cannot decompress/verify project member")?;
        ensure!(
            output.len() as u64 == size && output.len() as u64 <= limit,
            "Project member size mismatch"
        );
        return Ok(output);
    }
    anyhow::bail!("Project ZIP is missing a required member")
}
