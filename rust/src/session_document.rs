//! Portable settings handoff between the local MCP companion and desktop UI.
use crate::{audio, config::Settings, source_identity::SourceIdentity};
use anyhow::{Result, ensure};
use serde::{Deserialize, Serialize};
use std::{
    io::Read,
    path::{Path, PathBuf},
};

const MAX_DOCUMENT_BYTES: u64 = 1024 * 1024;

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SessionSource {
    pub path: PathBuf,
    pub identity: SourceIdentity,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SessionDocument {
    pub format: String,
    pub version: u32,
    pub source: SessionSource,
    pub reference: Option<SessionSource>,
    pub settings: Settings,
}

impl SessionDocument {
    pub fn new(
        source: SessionSource,
        reference: Option<SessionSource>,
        mut settings: Settings,
    ) -> Self {
        settings.reference = reference.as_ref().map(|r| r.path.clone());
        Self {
            format: "slimeybeats-session".into(),
            version: 1,
            source,
            reference,
            settings,
        }
    }

    pub fn load(path: &Path) -> Result<Self> {
        let mut bytes = Vec::new();
        std::fs::File::open(path)?
            .take(MAX_DOCUMENT_BYTES + 1)
            .read_to_end(&mut bytes)?;
        ensure!(
            bytes.len() as u64 <= MAX_DOCUMENT_BYTES,
            "Session document exceeds 1 MiB"
        );
        let document: Self = serde_json::from_slice(&bytes)?;
        document.validate()?;
        Ok(document)
    }

    pub fn validate(&self) -> Result<()> {
        ensure!(
            self.format == "slimeybeats-session" && self.version == 1,
            "Unsupported session document"
        );
        self.settings.validate()?;
        ensure!(
            self.settings.reference == self.reference.as_ref().map(|r| r.path.clone()),
            "Reference settings do not match the session source"
        );
        for source in std::iter::once(&self.source).chain(self.reference.iter()) {
            ensure!(
                source.path.is_absolute() && audio::supported(&source.path),
                "Session sources must be absolute audio-file paths"
            );
            source.identity.verify(&source.path)?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn session_round_trip_preserves_settings_and_detects_changed_audio() {
        let dir = tempfile::tempdir().unwrap();
        let source = dir.path().join("same name.wav");
        std::fs::write(&source, b"original").unwrap();
        let source = source.canonicalize().unwrap();
        let identity = SourceIdentity::read(&source, &|| false).unwrap();
        let mut settings = Settings::default();
        settings.manual_eq[2].gain = -2.;
        let doc = SessionDocument::new(
            SessionSource {
                path: source.clone(),
                identity,
            },
            None,
            settings,
        );
        let saved = dir.path().join("session.json");
        std::fs::write(&saved, serde_json::to_vec(&doc).unwrap()).unwrap();
        assert_eq!(
            SessionDocument::load(&saved).unwrap().settings.manual_eq[2].gain,
            -2.
        );
        std::fs::write(source, b"modified").unwrap();
        assert!(SessionDocument::load(&saved).is_err());
    }

    #[test]
    fn unknown_versions_and_unbound_references_are_rejected() {
        let dir = tempfile::tempdir().unwrap();
        let source = dir.path().join("track.wav");
        std::fs::write(&source, b"audio").unwrap();
        let source = source.canonicalize().unwrap();
        let identity = SourceIdentity::read(&source, &|| false).unwrap();
        let mut doc = SessionDocument::new(
            SessionSource {
                path: source,
                identity,
            },
            None,
            Settings::default(),
        );
        doc.version = 2;
        assert!(doc.validate().is_err());
        doc.version = 1;
        doc.settings.reference = Some(PathBuf::from("different.wav"));
        assert!(doc.validate().is_err());
    }
}
