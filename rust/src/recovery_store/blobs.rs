use super::{
    Snapshot, filesystem as io,
    manifest::{self, BlobRef, Owner},
};
use crate::audio::Audio;
use anyhow::{Context, Result, ensure};
use sha2::{Digest, Sha256};
use std::{
    collections::HashMap,
    fs::{self, File},
    io::{BufReader, BufWriter, Read, Write},
    path::Path,
    sync::{Arc, Weak},
};

#[derive(Default)]
pub(super) struct Memo {
    entries: HashMap<usize, (Weak<Audio>, BlobRef)>,
    verified_files: HashMap<String, (u64, std::time::SystemTime)>,
}
impl Memo {
    pub fn check_existing(
        &mut self,
        root: &Path,
        entry: &BlobRef,
        cancel: &dyn Fn() -> bool,
    ) -> Result<()> {
        inspect(root, entry)?;
        let meta = fs::metadata(root.join("blobs").join(&entry.sha256).join("audio.wav"))?;
        let identity = (meta.len(), meta.modified()?);
        if self.verified_files.get(&entry.sha256) != Some(&identity) {
            let mut file = File::open(root.join("blobs").join(&entry.sha256).join("audio.wav"))?
                .take(entry.bytes + 1);
            let mut hash = Sha256::new();
            let mut buffer = [0_u8; 256 * 1024];
            let mut bytes_read = 0_u64;
            loop {
                io::cancelled(cancel)?;
                let n = file.read(&mut buffer)?;
                if n == 0 {
                    break;
                }
                bytes_read += n as u64;
                hash.update(&buffer[..n]);
            }
            ensure!(
                bytes_read == entry.bytes,
                "Existing recovery WAV size changed during verification"
            );
            ensure!(
                manifest::hex(&hash.finalize()) == entry.sha256,
                "Existing recovery WAV checksum mismatch; damaged bytes were preserved"
            );
            self.verified_files.insert(entry.sha256.clone(), identity);
        }
        Ok(())
    }
    pub fn remember_written(&mut self, root: &Path, entry: &BlobRef) -> Result<()> {
        let meta = fs::metadata(root.join("blobs").join(&entry.sha256).join("audio.wav"))?;
        self.verified_files
            .insert(entry.sha256.clone(), (meta.len(), meta.modified()?));
        Ok(())
    }
    pub fn resolve(
        &mut self,
        snapshot: &Snapshot,
        cancel: &dyn Fn() -> bool,
    ) -> Result<Vec<BlobRef>> {
        ensure!(
            snapshot.audio.len() <= super::MAX_AUDIO_ATTACHMENTS,
            "Too many recovery audio attachments"
        );
        self.entries.retain(|_, (weak, _)| weak.strong_count() > 0);
        let mut result = Vec::with_capacity(snapshot.audio.len());
        let mut decoded_bytes = 0_u64;
        for audio in &snapshot.audio {
            io::cancelled(cancel)?;
            decoded_bytes = decoded_bytes
                .checked_add((audio.samples.len() as u64).saturating_mul(4))
                .context("Recovery decoded-size overflow")?;
            ensure!(
                decoded_bytes <= super::DEFAULT_BUDGET_BYTES,
                "Recovery decoded attachments exceed 8 GiB; each logical audio index counts independently"
            );
            let key = Arc::as_ptr(audio) as usize;
            if let Some((weak, entry)) = self.entries.get(&key)
                && weak.upgrade().is_some_and(|old| Arc::ptr_eq(&old, audio))
            {
                result.push(entry.clone());
                continue;
            }
            let entry = identify(audio, cancel)?;
            // A Weak also prevents Arc::get_mut from changing a memoized allocation.
            self.entries
                .insert(key, (Arc::downgrade(audio), entry.clone()));
            result.push(entry);
        }
        Ok(result)
    }
}
pub(super) fn header(audio: &BlobRef) -> [u8; 44] {
    let mut bytes = [0_u8; 44];
    bytes[..4].copy_from_slice(b"RIFF");
    bytes[4..8].copy_from_slice(&((audio.bytes - 8) as u32).to_le_bytes());
    bytes[8..16].copy_from_slice(b"WAVEfmt ");
    bytes[16..20].copy_from_slice(&16_u32.to_le_bytes());
    bytes[20..22].copy_from_slice(&3_u16.to_le_bytes());
    bytes[22..24].copy_from_slice(&(audio.channels as u16).to_le_bytes());
    bytes[24..28].copy_from_slice(&audio.sample_rate.to_le_bytes());
    bytes[28..32].copy_from_slice(&(audio.sample_rate * audio.channels as u32 * 4).to_le_bytes());
    bytes[32..34].copy_from_slice(&(audio.channels as u16 * 4).to_le_bytes());
    bytes[34..36].copy_from_slice(&32_u16.to_le_bytes());
    bytes[36..40].copy_from_slice(b"data");
    bytes[40..44].copy_from_slice(&((audio.samples * 4) as u32).to_le_bytes());
    bytes
}
fn sample_chunks(
    audio: &Audio,
    cancel: &dyn Fn() -> bool,
    mut consume: impl FnMut(&[u8]) -> Result<()>,
) -> Result<()> {
    let mut bytes = [0_u8; 64 * 1024];
    for samples in audio.samples.chunks(bytes.len() / 4) {
        io::cancelled(cancel)?;
        for (sample, dst) in samples.iter().zip(bytes.as_chunks_mut::<4>().0) {
            ensure!(
                sample.is_finite(),
                "Recovery audio contains nonfinite samples"
            );
            dst.copy_from_slice(&sample.to_le_bytes());
        }
        consume(&bytes[..samples.len() * 4])?;
    }
    Ok(())
}
fn identify(audio: &Audio, cancel: &dyn Fn() -> bool) -> Result<BlobRef> {
    let mut entry = BlobRef {
        sha256: "0".repeat(64),
        bytes: (audio.samples.len() as u64)
            .checked_mul(4)
            .and_then(|n| n.checked_add(44))
            .context("Recovery asset size overflow")?,
        sample_rate: audio.sample_rate,
        channels: audio.channels,
        samples: audio.samples.len() as u64,
    };
    entry.validate()?;
    let mut hash = Sha256::new();
    hash.update(header(&entry));
    sample_chunks(audio, cancel, |bytes| {
        hash.update(bytes);
        Ok(())
    })?;
    entry.sha256 = manifest::hex(&hash.finalize());
    Ok(entry)
}
pub(super) fn inspect(root: &Path, entry: &BlobRef) -> Result<()> {
    entry.validate()?;
    let path = root.join("blobs").join(&entry.sha256);
    validate_directory(&path, Some(entry))?;
    Ok(())
}
pub(super) fn validate_directory(path: &Path, expected: Option<&BlobRef>) -> Result<BlobRef> {
    io::directory(path)?;
    io::require_names(path, &["owner.json", "blob.json", "audio.wav"], &|| false)?;
    let owner: Owner = io::read_json(&path.join("owner.json"), 4096)?;
    owner.validate()?;
    let entry: BlobRef = io::read_json(&path.join("blob.json"), 4096)?;
    entry.validate()?;
    if let Some(expected) = expected {
        ensure!(
            &entry == expected,
            "Recovery blob metadata differs from its checkpoint"
        );
    }
    ensure!(
        io::regular(&path.join("audio.wav"))? == entry.bytes,
        "Recovery WAV is truncated or oversized"
    );
    Ok(entry)
}
pub(super) fn estimated_bytes(entry: &BlobRef) -> Result<u64> {
    Ok(entry.bytes
        + io::json_bytes(&Owner::new(), 4096)?.len() as u64
        + io::json_bytes(entry, 4096)?.len() as u64)
}
pub(super) fn write(
    root: &Path,
    pending: &Path,
    audio: &Audio,
    entry: &BlobRef,
    cancel: &dyn Fn() -> bool,
) -> Result<()> {
    let path = root.join("blobs").join(&entry.sha256);
    if path.try_exists()? {
        return inspect(root, entry);
    }
    let staged = pending.join("blob");
    fs::create_dir(&staged)?;
    io::create_json(&staged.join("owner.json"), &Owner::new())?;
    let mut file = BufWriter::with_capacity(
        256 * 1024,
        fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(staged.join("audio.wav"))?,
    );
    file.write_all(&header(entry))?;
    sample_chunks(audio, cancel, |bytes| {
        file.write_all(bytes)?;
        Ok(())
    })?;
    file.flush()?;
    file.get_ref().sync_all()?;
    drop(file);
    io::create_json(&staged.join("blob.json"), entry)?;
    io::sync_directory(&staged)?;
    io::cancelled(cancel)?;
    fs::rename(&staged, &path)?;
    io::sync_directory(&root.join("blobs"))?;
    Ok(())
}
pub(super) fn read(root: &Path, entry: &BlobRef, cancel: &dyn Fn() -> bool) -> Result<Arc<Audio>> {
    inspect(root, entry)?;
    let mut file = BufReader::with_capacity(
        256 * 1024,
        File::open(root.join("blobs").join(&entry.sha256).join("audio.wav"))?,
    );
    let mut wav_header = [0_u8; 44];
    file.read_exact(&mut wav_header)?;
    ensure!(
        wav_header == header(entry),
        "Recovery WAV layout is not canonical float32 PCM"
    );
    let mut hash = Sha256::new();
    hash.update(wav_header);
    let count: usize = entry
        .samples
        .try_into()
        .context("Recovery audio is too large for this machine")?;
    let mut samples = Vec::new();
    samples
        .try_reserve_exact(count)
        .context("Not enough memory to recover this audio")?;
    let mut remaining = count;
    let mut bytes = [0_u8; 64 * 1024];
    while remaining > 0 {
        io::cancelled(cancel)?;
        let n = remaining.min(bytes.len() / 4);
        file.read_exact(&mut bytes[..n * 4])?;
        hash.update(&bytes[..n * 4]);
        for sample in bytes[..n * 4].as_chunks::<4>().0 {
            let value = f32::from_le_bytes(*sample);
            ensure!(value.is_finite(), "Recovery WAV contains nonfinite audio");
            samples.push(value);
        }
        remaining -= n;
    }
    ensure!(
        manifest::hex(&hash.finalize()) == entry.sha256,
        "Recovery WAV checksum mismatch"
    );
    ensure!(
        file.read(&mut [0])? == 0,
        "Recovery WAV has unexpected trailing data"
    );
    Ok(Arc::new(Audio::new(
        samples,
        entry.sample_rate,
        entry.channels,
    )?))
}
pub(super) fn load_all(
    root: &Path,
    entries: &[BlobRef],
    cancel: &dyn Fn() -> bool,
) -> Result<Vec<Arc<Audio>>> {
    let mut audio = Vec::with_capacity(entries.len());
    for entry in entries {
        io::cancelled(cancel)?;
        // SHA equality is a disk optimization, not a stem/source relationship.
        // Independent attachment indices must remain independent allocations.
        audio.push(read(root, entry, cancel)?);
    }
    Ok(audio)
}
