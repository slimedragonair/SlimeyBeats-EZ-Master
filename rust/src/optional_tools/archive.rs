//! Fixed-file ZIP extraction for pinned application components, never whole archives.
//! Call in a fresh private staging directory; discard staging on any error.
use super::paths::safe_path;
use anyhow::{Context, Result, ensure};
use sha2::{Digest, Sha256};
use std::{
    collections::HashSet,
    fs::{self, File, OpenOptions},
    io::{Read, Write},
    path::Path,
    sync::atomic::{AtomicBool, Ordering},
};
use zip::{ZipReadOptions, read::Config, unstable::read::ZipIterable};

#[derive(Clone, Copy)]
pub struct FixedEntry<'a> {
    pub archive_path: &'a str,
    pub file_name: &'a str,
}

#[derive(Clone, Copy)]
pub struct Limits {
    pub max_archive_bytes: u64,
    pub max_entries: usize,
    pub max_entry_bytes: u64,
    /// Includes skipped members' advertised expanded sizes.
    pub max_total_bytes: u64,
}

/// Reject relative/traversing paths and existing symlink/reparse-point ancestors.
/// Does not create folders or establish ownership; callers do that separately.
pub fn validate_cache_path(path: &Path) -> Result<()> {
    safe_path(path)
}

pub(super) const FFMPEG_LIMITS: Limits = Limits {
    max_archive_bytes: 192 * 1024 * 1024,
    max_entries: 256,
    max_entry_bytes: 192 * 1024 * 1024,
    max_total_bytes: 640 * 1024 * 1024,
};

/// Verifies archive SHA-256 before parsing, then writes only explicitly named files.
/// All member names/types are checked, including skipped members. No nested outputs,
/// symlinks, special files, duplicate names, overwrites, or archive-provided paths.
pub fn extract_zip_fixed(
    archive: &Path,
    sha256: &str,
    destination: &Path,
    entries: &[FixedEntry<'_>],
    limits: Limits,
    cancel: &AtomicBool,
) -> Result<()> {
    ensure!(
        limits.max_archive_bytes > 0
            && limits.max_entries > 0
            && limits.max_entries <= 4096
            && limits.max_entry_bytes > 0
            && limits.max_entry_bytes <= 1024 * 1024 * 1024
            && limits.max_total_bytes <= 2 * 1024 * 1024 * 1024,
        "Invalid archive budgets"
    );
    let mut file = verified_file(archive, sha256, limits.max_archive_bytes, cancel)?;
    use std::io::{Seek, SeekFrom};
    file.seek(SeekFrom::Start(0))?;
    validate_destination(destination, entries)?;
    let mut zip = ZipIterable::try_new(file, Config::default())?.enforce_header_consistency(true);
    let mut seen = HashSet::new();
    let mut total = 0_u64;
    let mut selected = Vec::new();
    // Validate the entire directory before creating any output.
    for (index, member) in zip.files()?.enumerate() {
        check_cancel(cancel)?;
        ensure!(index < limits.max_entries, "ZIP entry budget exceeded");
        let member = member?;
        let name = member.name()?;
        valid_name(&name)?;
        ensure!(
            seen.insert(name.to_ascii_lowercase()),
            "Duplicate ZIP member"
        );
        let kind = member.unix_mode().unwrap_or(0) & 0o170000;
        ensure!(
            kind == 0 || kind == 0o100000 || (kind == 0o040000 && member.is_dir()),
            "ZIP symlinks and special files are forbidden"
        );
        ensure!(
            !member.is_symlink() && !member.encrypted(),
            "ZIP links and encrypted files are forbidden"
        );
        ensure!(
            matches!(
                member.compression(),
                zip::CompressionMethod::Stored | zip::CompressionMethod::Deflated
            ),
            "Unsupported ZIP compression"
        );
        ensure!(
            member.size() <= limits.max_entry_bytes,
            "ZIP member exceeds its output budget"
        );
        total = total
            .checked_add(member.size())
            .context("ZIP size overflow")?;
        ensure!(
            total <= limits.max_total_bytes,
            "ZIP expanded-size budget exceeded"
        );
        if let Some(entry) = entries.iter().position(|e| e.archive_path == name) {
            ensure!(!member.is_dir(), "A required ZIP file is a directory");
            selected.push((index, entry));
        }
    }
    ensure!(
        selected.len() == entries.len(),
        "ZIP is missing a required fixed file"
    );
    let files = zip.files()?;
    let mut index = 0;
    while let Some(member) = files.next() {
        check_cancel(cancel)?;
        let member = member?;
        if let Some((_, entry)) = selected.iter().find(|(i, _)| *i == index) {
            let expected = member.size();
            let mut reader = member.with_reader(files.reader_mut(), ZipReadOptions::default())?;
            let mut output = OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(destination.join(entries[*entry].file_name))?;
            copy_exact(&mut reader, &mut output, expected, cancel)?;
            let mut extra = [0];
            ensure!(
                reader.read(&mut extra)? == 0,
                "ZIP member exceeds declared size"
            );
            output.sync_all()?;
        }
        index += 1;
    }
    check_cancel(cancel)
}

pub(super) fn validate_destination(destination: &Path, entries: &[FixedEntry<'_>]) -> Result<()> {
    safe_path(destination)?;
    ensure!(
        destination.is_dir() && fs::read_dir(destination)?.next().is_none(),
        "Archive output must be a fresh empty staging directory"
    );
    ensure!(!entries.is_empty(), "No fixed archive files were requested");
    let mut sources = HashSet::new();
    let mut outputs = HashSet::new();
    for entry in entries {
        valid_name(entry.archive_path)?;
        valid_name(entry.file_name)?;
        ensure!(
            !entry.file_name.contains('/')
                && !entry.file_name.ends_with('.')
                && !entry.file_name.ends_with(' '),
            "Outputs must be plain fixed filenames"
        );
        ensure!(
            sources.insert(entry.archive_path)
                && outputs.insert(entry.file_name.to_ascii_lowercase()),
            "Duplicate fixed archive files"
        );
    }
    Ok(())
}

pub(super) fn valid_name(name: &str) -> Result<()> {
    ensure!(
        !name.is_empty()
            && name.len() <= 1024
            && !name.starts_with('/')
            && !name.contains(['\\', ':'])
            && !name.bytes().any(|b| b.is_ascii_control()),
        "Unsafe archive member name"
    );
    for part in name.trim_end_matches('/').split('/') {
        ensure!(
            !part.is_empty() && part != "." && part != ".." && !part.ends_with(['.', ' ']),
            "Archive path traversal or ambiguous name"
        );
        let stem = part.split('.').next().unwrap_or("").to_ascii_uppercase();
        ensure!(
            !matches!(stem.as_str(), "CON" | "PRN" | "AUX" | "NUL")
                && !(stem.len() == 4
                    && (stem.starts_with("COM") || stem.starts_with("LPT"))
                    && stem.as_bytes()[3].is_ascii_digit()),
            "Reserved archive filename"
        );
    }
    Ok(())
}

pub(super) fn verified_file(
    path: &Path,
    sha256: &str,
    max: u64,
    cancel: &AtomicBool,
) -> Result<File> {
    safe_path(path)?;
    ensure!(
        sha256.len() == 64 && sha256.bytes().all(|b| b.is_ascii_hexdigit()),
        "Invalid SHA-256 pin"
    );
    let mut file = File::open(path)?;
    ensure!(
        file.metadata()?.is_file() && file.metadata()?.len() <= max,
        "File exceeds its byte budget"
    );
    let mut hash = Sha256::new();
    let mut buffer = [0; 65536];
    let mut total = 0_u64;
    loop {
        check_cancel(cancel)?;
        let n = file.read(&mut buffer)?;
        if n == 0 {
            break;
        }
        total = total.checked_add(n as u64).context("File size overflow")?;
        ensure!(total <= max, "File grew beyond its byte budget");
        hash.update(&buffer[..n]);
    }
    let actual: String = hash
        .finalize()
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect();
    ensure!(
        actual.eq_ignore_ascii_case(sha256),
        "SHA-256 verification failed; previous installation preserved"
    );
    Ok(file)
}

pub(super) fn copy_exact(
    input: &mut impl Read,
    output: &mut impl Write,
    mut bytes: u64,
    cancel: &AtomicBool,
) -> Result<()> {
    let mut buffer = [0; 65536];
    while bytes > 0 {
        check_cancel(cancel)?;
        let size = bytes.min(buffer.len() as u64) as usize;
        input.read_exact(&mut buffer[..size])?;
        output.write_all(&buffer[..size])?;
        bytes -= size as u64;
    }
    Ok(())
}

pub(super) fn check_cancel(cancel: &AtomicBool) -> Result<()> {
    ensure!(!cancel.load(Ordering::Relaxed), "Cancelled");
    Ok(())
}
