use super::{ArrangementManifest, ArrangementSnapshot, ProjectSource, decode_source};
use crate::studio::arrangement::AssetLibrary;
use anyhow::{Context, Result, ensure};
use std::sync::Arc;

/// All entry points admit source bytes through this hash/geometry gate before publishing state.
pub struct ProjectLoader {
    manifest: ArrangementManifest,
    assets: AssetLibrary,
    remaining: u64,
}
impl ProjectLoader {
    pub fn new(manifest: ArrangementManifest, max_decoded_bytes: u64) -> Result<Self> {
        manifest.validate()?;
        ensure!(
            max_decoded_bytes > 0,
            "Project decoded budget must be positive"
        );
        Ok(Self {
            manifest,
            assets: AssetLibrary::new(),
            remaining: max_decoded_bytes,
        })
    }
    pub fn sources(&self) -> &[ProjectSource] {
        &self.manifest.sources
    }
    pub fn remaining_bytes(&self) -> u64 {
        self.remaining
    }
    pub fn add_source(&mut self, id: &str, bytes: &[u8]) -> Result<()> {
        ensure!(
            !self.assets.contains_key(id),
            "Project source has already been admitted"
        );
        let descriptor = self
            .manifest
            .sources
            .iter()
            .find(|source| source.id == id)
            .context("Project source ID is not in its manifest")?;
        let audio = decode_source(descriptor, bytes, self.remaining)?;
        let remaining = self
            .remaining
            .checked_sub(audio.samples.len() as u64 * 4)
            .context("Project source pool exceeds the decoded budget")?;
        self.assets.insert(id.into(), Arc::new(audio));
        self.remaining = remaining;
        Ok(())
    }
    pub fn finish(self) -> Result<ArrangementSnapshot> {
        self.manifest.restore(self.assets)
    }
}
