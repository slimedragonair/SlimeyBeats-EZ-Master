//! Bounded ZIP reading. Entry names are labels, never output paths or instructions.
use super::stem_import_plan::{self, ImportFile, ImportPlan};
use anyhow::{Context, Result, ensure};
use std::io::{Read, Seek, SeekFrom, Write};
use zip::{ZipReadOptions, read::Config, unstable::read::ZipIterable};

pub const MAX_ARCHIVE_BYTES: u64 = 1024 * 1024 * 1024;
pub const MAX_ENTRY_BYTES: u64 = 512 * 1024 * 1024;

fn safe_path(name: &str) -> bool {
    name.len() <= 4096
        && !name.starts_with('/')
        && !name.contains(['\\', '\0'])
        && name.as_bytes().get(1) != Some(&b':')
        && name.split('/').all(|part| part != ".." && part != ".")
}

fn open(mut reader: impl Read + Seek) -> Result<ZipIterable<impl Read + Seek>> {
    ensure!(
        reader.seek(SeekFrom::End(0))? <= MAX_ARCHIVE_BYTES,
        "Stem ZIP exceeds 1 GiB"
    );
    reader.seek(SeekFrom::Start(0))?;
    Ok(ZipIterable::try_new(reader, Config::default())
        .context("Cannot read this ZIP archive")?
        .enforce_header_consistency(true))
}

// This exact-pinned iterable API bounds metadata before indexing/decompression,
// and preserves duplicate archive names so ambiguity cannot disappear silently.
pub fn inspect(reader: impl Read + Seek, entry_limit: u64) -> Result<ImportPlan> {
    let mut archive = open(reader)?;
    let entry_limit = entry_limit.min(MAX_ENTRY_BYTES);
    let mut metadata_bytes = 0;
    let mut files = Vec::new();
    let mut skipped = 0;
    for (index, entry) in archive.files()?.enumerate() {
        let entry = entry?;
        let name = entry.name()?;
        metadata_bytes = stem_import_plan::charge_metadata(metadata_bytes, name.len())?;
        let name = name.into_owned();
        let format = stem_import_plan::file_format(&name);
        if entry.is_dir()
            || name
                .split(['/', '\\'])
                .any(|part| part == "__MACOSX" || part.starts_with("._"))
            || !matches!(format.as_deref(), Some("wav" | "mp3"))
        {
            skipped += 1;
            continue;
        }
        ensure!(
            safe_path(&name) && entry.enclosed_name().is_some() && !entry.is_symlink(),
            "ZIP audio entry has an unsafe path or is a link: {name}"
        );
        ensure!(
            !entry.encrypted(),
            "Password-protected stem ZIPs are not supported"
        );
        ensure!(
            entry.size() > 0 && entry.size() <= entry_limit,
            "ZIP audio entry exceeds the supported size: {name}"
        );
        ensure!(
            matches!(
                entry.compression(),
                zip::CompressionMethod::Stored | zip::CompressionMethod::Deflated
            ),
            "ZIP audio must use stored/deflate compression: {name}"
        );
        files.push(ImportFile {
            index,
            name,
            format: format.unwrap(),
            bytes: entry.size(),
            crc32: Some(entry.crc32()),
        });
    }
    let mut plan = stem_import_plan::plan(files)?;
    plan.skipped_files += skipped;
    Ok(plan)
}

pub fn copy_audio(
    reader: impl Read + Seek,
    selected: &ImportFile,
    limit: u64,
    output: &mut impl Write,
    cancel: &dyn Fn() -> bool,
) -> Result<()> {
    let limit = limit.min(MAX_ENTRY_BYTES);
    ensure!(
        selected.bytes <= limit,
        "ZIP audio entry exceeds the supported size"
    );
    let mut archive = open(reader)?;
    let mut metadata_bytes = 0;
    let entries = archive.files()?;
    let mut index = 0;
    while let Some(entry) = entries.next() {
        ensure!(!cancel(), "Cancelled");
        let entry = entry?;
        let name = entry.name()?;
        metadata_bytes = stem_import_plan::charge_metadata(metadata_bytes, name.len())?;
        if index == selected.index {
            ensure!(
                name == selected.name
                    && entry.size() == selected.bytes
                    && Some(entry.crc32()) == selected.crc32,
                "The stem ZIP changed; choose it again before importing"
            );
            ensure!(
                safe_path(&name)
                    && entry.enclosed_name().is_some()
                    && !entry.is_symlink()
                    && !entry.encrypted(),
                "Unsupported ZIP audio entry"
            );
            ensure!(
                matches!(
                    stem_import_plan::file_format(&name).as_deref(),
                    Some("wav" | "mp3")
                ),
                "Choose a WAV/MP3 ZIP entry"
            );
            let mut audio = entry.with_reader(entries.reader_mut(), ZipReadOptions::default())?;
            let mut block = vec![0; 64 * 1024];
            let mut written = 0u64;
            loop {
                ensure!(!cancel(), "Cancelled");
                let count = audio
                    .read(&mut block)
                    .context("Cannot decompress/verify ZIP audio")?;
                if count == 0 {
                    break;
                }
                written += count as u64;
                ensure!(
                    written <= limit && written <= selected.bytes,
                    "ZIP audio expanded beyond its declared size"
                );
                output.write_all(&block[..count])?;
            }
            ensure!(written == selected.bytes, "ZIP audio is truncated");
            return Ok(());
        }
        index += 1;
    }
    anyhow::bail!("The selected stem is no longer in this ZIP; choose it again")
}
