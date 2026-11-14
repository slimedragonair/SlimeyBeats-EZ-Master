use super::ToolKind;
use crate::source_identity::SourceIdentity;
use anyhow::{Result, ensure};
use serde::{Deserialize, Serialize};
use std::{
    collections::{BTreeMap, HashMap},
    fs,
    io::Read,
    path::{Path, PathBuf},
    sync::{Mutex, OnceLock},
    time::SystemTime,
};

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Receipt {
    pub owner: String,
    pub tool: String,
    pub protocol: u32,
    pub directory: String,
    pub files: BTreeMap<String, String>,
}
pub(super) fn directory(root: &Path, kind: ToolKind) -> PathBuf {
    root.join(kind.key())
}
pub(super) fn owned(root: &Path, kind: ToolKind) -> Result<PathBuf> {
    let base = directory(root, kind);
    crate::optional_tools::archive::validate_cache_path(&base)?;
    fs::create_dir_all(&base)?;
    let marker = base.join("owner.json");
    if marker.exists() {
        let bytes = bounded_read(&marker, 1024)?;
        ensure!(
            bytes == format!("SlimeyBeats-EZ-Master:{}:1", kind.key()).as_bytes(),
            "Helper cache belongs to another owner"
        );
    } else {
        ensure!(
            fs::read_dir(&base)?.next().is_none(),
            "Unowned helper folder is not empty; its files were preserved"
        );
        let mut file = fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(marker)?;
        use std::io::Write;
        file.write_all(format!("SlimeyBeats-EZ-Master:{}:1", kind.key()).as_bytes())?;
        file.sync_all()?;
    }
    Ok(base)
}
pub(super) fn bounded_read(path: &Path, limit: usize) -> Result<Vec<u8>> {
    crate::optional_tools::archive::validate_cache_path(path)?;
    let mut bytes = Vec::new();
    fs::File::open(path)?
        .take(limit as u64 + 1)
        .read_to_end(&mut bytes)?;
    ensure!(
        bytes.len() <= limit,
        "Helper metadata exceeds its byte bound"
    );
    Ok(bytes)
}
type Verified = HashMap<PathBuf, (u64, Option<SystemTime>, String)>;
static VERIFIED: OnceLock<Mutex<Verified>> = OnceLock::new();
fn verify_binary(path: &Path, expected: &str) -> Result<()> {
    crate::optional_tools::archive::validate_cache_path(path)?;
    let metadata = fs::metadata(path)?;
    ensure!(
        metadata.is_file() && metadata.len() <= 128 * 1024 * 1024,
        "Invalid helper executable"
    );
    ensure!(
        expected.len() == 64 && expected.bytes().all(|b| b.is_ascii_hexdigit()),
        "Invalid helper fingerprint"
    );
    let stamp = (
        metadata.len(),
        metadata.modified().ok(),
        expected.to_owned(),
    );
    let verified = VERIFIED.get_or_init(|| Mutex::new(HashMap::new()));
    if stamp.1.is_some()
        && verified
            .lock()
            .is_ok_and(|cache| cache.get(path) == Some(&stamp))
    {
        return Ok(());
    }
    ensure!(
        SourceIdentity::read(path, &|| false)?.sha256 == expected,
        "Cached helper failed verification; prepare it again"
    );
    if let Ok(mut cache) = verified.lock() {
        if cache.len() >= 32 {
            cache.clear();
        }
        cache.insert(path.into(), stamp);
    }
    Ok(())
}
pub(super) fn ready(root: &Path, kind: ToolKind) -> Result<Option<PathBuf>> {
    let base = directory(root, kind);
    let receipt_path = base.join("current.json");
    if !receipt_path.exists() {
        return Ok(None);
    }
    ensure!(
        bounded_read(&base.join("owner.json"), 1024)?
            == format!("SlimeyBeats-EZ-Master:{}:1", kind.key()).as_bytes(),
        "Invalid helper cache owner"
    );
    let receipt: Receipt = serde_json::from_slice(&bounded_read(&receipt_path, 65536)?)?;
    ensure!(
        receipt.owner == "SlimeyBeats-EZ-Master"
            && receipt.tool == kind.key()
            && receipt.protocol == kind.protocol(),
        "Incompatible helper receipt"
    );
    ensure!(
        !receipt.directory.is_empty()
            && receipt.directory.len() <= 128
            && receipt
                .directory
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b"-_.".contains(&b))
            && receipt.directory != "."
            && receipt.directory != "..",
        "Invalid helper cache directory"
    );
    let target = base.join(&receipt.directory).join(kind.executable());
    let expected = receipt
        .files
        .get(kind.executable())
        .ok_or_else(|| anyhow::anyhow!("Helper receipt has no executable fingerprint"))?;
    verify_binary(&target, expected)?;
    Ok(Some(target))
}
