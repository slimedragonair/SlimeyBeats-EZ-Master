//! Strict ustar reader for the pinned Linux bundle. No archive member paths are used as outputs.
use super::archive::{self, FixedEntry, Limits};
use anyhow::{Context, Result, ensure};
use std::{
    collections::HashSet,
    fs::OpenOptions,
    io::{Read, Write},
    path::Path,
    sync::atomic::AtomicBool,
};

pub(super) fn extract(
    input: &mut impl Read,
    destination: &Path,
    entries: &[FixedEntry<'_>],
    limits: Limits,
    cancel: &AtomicBool,
) -> Result<()> {
    archive::validate_destination(destination, entries)?;
    let mut seen = HashSet::new();
    let mut found = HashSet::new();
    let mut total = 0_u64;
    let mut headers = 0;
    loop {
        archive::check_cancel(cancel)?;
        let mut header = [0; 512];
        input
            .read_exact(&mut header)
            .context("Truncated tar header")?;
        if header == [0; 512] {
            let mut second = [0; 512];
            input.read_exact(&mut second)?;
            ensure!(second == [0; 512], "Invalid tar end marker");
            // Refuse hidden concatenated archives/nonzero trailing payload.
            let mut tail = [0; 8192];
            let mut trailing = 0;
            loop {
                archive::check_cancel(cancel)?;
                let n = input.read(&mut tail)?;
                if n == 0 {
                    break;
                }
                trailing += n;
                ensure!(
                    trailing <= 1024 * 1024 && tail[..n].iter().all(|&b| b == 0),
                    "Unexpected trailing tar payload"
                );
            }
            break;
        }
        headers += 1;
        ensure!(headers <= limits.max_entries, "Tar entry budget exceeded");
        let checksum = octal(&header[148..156])?;
        let actual: u64 = header
            .iter()
            .enumerate()
            .map(|(i, &b)| {
                if (148..156).contains(&i) {
                    32
                } else {
                    b as u64
                }
            })
            .sum();
        ensure!(checksum == actual, "Invalid tar header checksum");
        ensure!(&header[257..262] == b"ustar", "Unsupported tar format");
        let base = field(&header[..100])?;
        let prefix = field(&header[345..500])?;
        let name = if prefix.is_empty() {
            base.into()
        } else {
            format!("{prefix}/{base}")
        };
        archive::valid_name(&name)?;
        ensure!(
            seen.insert(name.to_ascii_lowercase()),
            "Duplicate tar member"
        );
        ensure!(
            matches!(header[156], 0 | b'0' | b'5') && field(&header[157..257])?.is_empty(),
            "Tar links, special files and extensions are forbidden"
        );
        let size = octal(&header[124..136])?;
        ensure!(
            size <= limits.max_entry_bytes,
            "Tar member exceeds output budget"
        );
        total = total.checked_add(size).context("Tar size overflow")?;
        ensure!(
            total <= limits.max_total_bytes,
            "Tar expanded-size budget exceeded"
        );
        ensure!(
            header[156] != b'5' || size == 0,
            "Tar directory has content"
        );
        if let Some(entry) = entries.iter().find(|e| e.archive_path == name) {
            ensure!(header[156] != b'5', "Required tar file is a directory");
            found.insert(name);
            let mut file = OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(destination.join(entry.file_name))?;
            archive::copy_exact(input, &mut file, size, cancel)?;
            file.flush()?;
            file.sync_all()?;
        } else {
            archive::copy_exact(input, &mut std::io::sink(), size, cancel)?;
        }
        let padding = (512 - size % 512) % 512;
        archive::copy_exact(input, &mut std::io::sink(), padding, cancel)?;
    }
    ensure!(
        found.len() == entries.len(),
        "Tar is missing a required fixed file"
    );
    Ok(())
}

fn field(bytes: &[u8]) -> Result<&str> {
    let end = bytes.iter().position(|&b| b == 0).unwrap_or(bytes.len());
    ensure!(
        bytes[end..].iter().all(|&b| b == 0),
        "Ambiguous tar header string"
    );
    Ok(std::str::from_utf8(&bytes[..end])?)
}

fn octal(bytes: &[u8]) -> Result<u64> {
    let text = std::str::from_utf8(bytes)?.trim_matches(['\0', ' ']);
    if text.is_empty() {
        return Ok(0);
    }
    ensure!(
        text.bytes().all(|b| matches!(b, b'0'..=b'7')),
        "Unsupported tar size encoding"
    );
    Ok(u64::from_str_radix(text, 8)?)
}
