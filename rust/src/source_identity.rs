use anyhow::{Result, ensure};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{fs::File, io::Read, path::Path};

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct SourceIdentity {
    pub bytes: u64,
    pub sha256: String,
}

impl SourceIdentity {
    pub fn read(path: &Path, cancel: &dyn Fn() -> bool) -> Result<Self> {
        let mut file = File::open(path)?;
        ensure!(file.metadata()?.is_file(), "Choose an ordinary audio file");
        let mut hash = Sha256::new();
        let mut buffer = [0_u8; 65536];
        let mut bytes = 0;
        loop {
            ensure!(!cancel(), "Cancelled");
            let count = file.read(&mut buffer)?;
            if count == 0 {
                break;
            }
            hash.update(&buffer[..count]);
            bytes += count as u64;
        }
        let sha256 = hash
            .finalize()
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect();
        Ok(Self { bytes, sha256 })
    }

    pub fn verify(&self, path: &Path) -> Result<()> {
        ensure!(
            self.sha256.len() == 64 && self.sha256.bytes().all(|b| b.is_ascii_hexdigit()),
            "Invalid source fingerprint"
        );
        ensure!(
            Self::read(path, &|| false)? == *self,
            "The source audio changed. Import it again before opening this session"
        );
        Ok(())
    }
}
