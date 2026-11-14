//! Self-contained Studio projects and MCP-to-desktop handoff. Documents never execute code.
use crate::{
    audio::Audio,
    source_identity::SourceIdentity,
    studio::stem_mix::{self, MixSettings, Stem, StemKind, StemSet},
};
use anyhow::{Context, Result, ensure};
use serde::{Deserialize, Serialize};
use std::{
    io::Read,
    path::{Path, PathBuf},
    sync::Arc,
};

const MAX_DOCUMENT_BYTES: u64 = 1024 * 1024;
const MAX_ASSET_BYTES: u64 = 512 * 1024 * 1024 + 1024;

#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AssetFile {
    pub name: String,
    pub identity: SourceIdentity,
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct StemFile {
    pub kind: StemKind,
    pub file: AssetFile,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub display_name: Option<String>,
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct StudioDocument {
    pub format: String,
    pub version: u32,
    pub source_name: String,
    pub source: AssetFile,
    pub stems: Vec<StemFile>,
    pub mix: MixSettings,
    pub before: AssetFile,
    #[serde(default)]
    pub before_stem: Option<StemKind>,
    pub preview: AssetFile,
    pub label: String,
    pub notes: Vec<String>,
    pub view: String,
}
#[derive(Clone)]
pub struct StudioSnapshot {
    pub source_name: String,
    pub source: Arc<Audio>,
    pub stems: Option<StemSet>,
    pub mix: MixSettings,
    pub before: Arc<Audio>,
    pub preview: Arc<Audio>,
    pub label: String,
    pub notes: Vec<String>,
    pub view: String,
}

impl StudioDocument {
    pub fn load(path: &Path) -> Result<StudioSnapshot> {
        let mut bytes = Vec::new();
        std::fs::File::open(path)?
            .take(MAX_DOCUMENT_BYTES + 1)
            .read_to_end(&mut bytes)?;
        ensure!(
            bytes.len() as u64 <= MAX_DOCUMENT_BYTES,
            "Studio document exceeds 1 MiB"
        );
        let document: Self = serde_json::from_slice(&bytes)?;
        ensure!(
            document.format == "slimeybeats-studio" && document.version == 1,
            "Unsupported Studio document"
        );
        ensure!(
            document.notes.len() <= 64,
            "Studio document has too many notes"
        );
        ensure!(
            document.source_name.len() <= 512
                && document.label.len() <= 512
                && document.notes.iter().all(|n| n.len() <= 4096),
            "Studio labels/notes are too long"
        );
        ensure!(
            ["stems", "voice", "repair", "plugins"].contains(&document.view.as_str()),
            "Unknown Studio view"
        );
        if !document.stems.is_empty() {
            stem_mix::validate_kinds(document.stems.iter().map(|stem| stem.kind))?;
        }
        let root = path
            .parent()
            .context("Studio document has no directory")?
            .canonicalize()?;
        let source = Arc::new(read(&root, &document.source)?);
        let mut before = Arc::new(read(&root, &document.before)?);
        let preview = Arc::new(read(&root, &document.preview)?);
        ensure!(
            before.sample_rate == preview.sample_rate
                && before.channels == preview.channels
                && before.frames() == preview.frames(),
            "Studio comparison assets are not aligned"
        );
        ensure!(
            source.sample_rate == preview.sample_rate
                && source.channels == preview.channels
                && source.frames() == preview.frames(),
            "Studio source and preview are not aligned"
        );
        let stems = if document.stems.is_empty() {
            None
        } else {
            let mut tracks = Vec::new();
            let mut remaining = super::stem_files::DEFAULT_BANK_DECODED_BYTES;
            for stem in &document.stems {
                let limit = super::stem_files::DEFAULT_INPUT_DECODED_BYTES
                    .min(remaining.min(usize::MAX as u64) as usize);
                let audio = read_with_limit(&root, &stem.file, limit)?;
                remaining = remaining
                    .checked_sub(audio.samples.len() as u64 * size_of::<f32>() as u64)
                    .context("Studio stems exceed the decoded bank memory budget")?;
                tracks.push(Stem {
                    kind: stem.kind,
                    audio: Arc::new(audio),
                });
            }
            let mut bank = StemSet::new(tracks)?;
            for stem in &document.stems {
                if let Some(name) = &stem.display_name {
                    bank.set_display_name(stem.kind, name.clone())?;
                }
            }
            let first = &bank.tracks[0].audio;
            ensure!(
                first.sample_rate == source.sample_rate
                    && first.channels == source.channels
                    && first.frames() == source.frames(),
                "Studio stem bank is not aligned with its source"
            );
            document.mix.validate(&bank)?;
            if let Some(kind) = document.before_stem {
                let stem = &bank.track(kind)?.audio;
                ensure!(
                    before.samples == stem.samples,
                    "Studio comparison does not match its declared stem"
                );
                // Restore edit lineage after reading separate self-contained
                // WAVs, so a reopened direct stem edit can still be applied.
                before = stem.clone();
            }
            Some(bank)
        };
        ensure!(
            document.before_stem.is_none() || stems.is_some(),
            "Studio comparison refers to a missing stem bank"
        );
        Ok(StudioSnapshot {
            source_name: document.source_name,
            source,
            stems,
            mix: document.mix,
            before,
            preview,
            label: document.label,
            notes: document.notes,
            view: document.view,
        })
    }
}
fn read(root: &Path, file: &AssetFile) -> Result<Audio> {
    read_with_limit(root, file, super::stem_files::DEFAULT_INPUT_DECODED_BYTES)
}
fn read_with_limit(root: &Path, file: &AssetFile, max_decoded_bytes: usize) -> Result<Audio> {
    let name = Path::new(&file.name);
    ensure!(
        name.components().count() == 1
            && name.file_name().is_some_and(|n| n == name.as_os_str())
            && name
                .extension()
                .is_some_and(|e| e.eq_ignore_ascii_case("wav")),
        "Studio assets must be simple relative WAV filenames"
    );
    let path = root.join(name).canonicalize()?;
    ensure!(
        path.starts_with(root),
        "Studio asset escapes its project directory"
    );
    ensure!(
        path.metadata()?.is_file() && path.metadata()?.len() <= MAX_ASSET_BYTES,
        "Studio asset exceeds the supported size"
    );
    file.identity.verify(&path)?;
    let (audio, _) = super::audio_import::load(
        &path,
        max_decoded_bytes,
        std::time::Duration::from_secs(30),
        Arc::new(std::sync::atomic::AtomicBool::new(false)),
    )?;
    Ok(audio)
}

pub fn save(
    snapshot: &StudioSnapshot,
    destination: &Path,
    cancel: &dyn Fn() -> bool,
) -> Result<PathBuf> {
    ensure!(
        snapshot.source_name.len() <= 512 && snapshot.label.len() <= 512,
        "Studio labels are too long"
    );
    std::fs::create_dir_all(destination)?;
    let directory = tempfile::Builder::new()
        .prefix("Studio-session-")
        .tempdir_in(destination)?;
    let write = |name: &str, audio: &Audio| -> Result<AssetFile> {
        ensure!(!cancel(), "Cancelled");
        ensure!(
            audio.samples.len() <= 512 * 1024 * 1024 / 4,
            "Studio audio asset exceeds 512 MiB decoded PCM"
        );
        let path = directory.path().join(name);
        let file = std::fs::File::create(&path)?;
        crate::wav_encoding::write_wav(audio, &file, "wav32f", false)?;
        file.sync_all()?;
        drop(file);
        Ok(AssetFile {
            name: name.into(),
            identity: SourceIdentity::read(&path, cancel)?,
        })
    };
    let source = write("source.wav", &snapshot.source)?;
    let before = write("before.wav", &snapshot.before)?;
    let preview = write("preview.wav", &snapshot.preview)?;
    let mut stems = Vec::new();
    if let Some(bank) = &snapshot.stems {
        snapshot.mix.validate(bank)?;
        for stem in &bank.tracks {
            stems.push(StemFile {
                kind: stem.kind,
                display_name: Some(bank.display_name(stem.kind).into_owned()),
                file: write(&format!("stem-{}.wav", stem.kind.name()), &stem.audio)?,
            });
        }
    }
    let document = StudioDocument {
        format: "slimeybeats-studio".into(),
        version: 1,
        source_name: snapshot.source_name.clone(),
        source,
        stems,
        mix: snapshot.mix.clone(),
        before,
        before_stem: snapshot.stems.as_ref().and_then(|bank| {
            bank.tracks
                .iter()
                .find(|stem| Arc::ptr_eq(&snapshot.before, &stem.audio))
                .map(|stem| stem.kind)
        }),
        preview,
        label: snapshot.label.clone(),
        notes: snapshot.notes.clone(),
        view: snapshot.view.clone(),
    };
    let json = serde_json::to_vec_pretty(&document)?;
    ensure!(
        json.len() as u64 <= MAX_DOCUMENT_BYTES,
        "Studio document exceeds 1 MiB"
    );
    let path = directory.path().join("session.studio.json");
    std::fs::write(&path, json)?;
    ensure!(!cancel(), "Cancelled");
    let folder = directory.keep();
    Ok(folder.join("session.studio.json"))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn saved_project_reopens_without_external_sources_and_rejects_tampering() {
        let root = tempfile::tempdir().unwrap();
        let source = Arc::new(Audio::new(vec![0.1; 8000], 8000, 1).unwrap());
        let stems = StemSet::new(vec![
            Stem {
                kind: StemKind::Vocals,
                audio: source.clone(),
            },
            Stem {
                kind: StemKind::Instrumental,
                audio: source.clone(),
            },
        ])
        .unwrap();
        let mut mix = stems.defaults();
        mix.tracks[0].gain_db = -3.;
        let snapshot = StudioSnapshot {
            source_name: "Original.mp3".into(),
            source: source.clone(),
            stems: Some(stems),
            mix,
            before: source.clone(),
            preview: source,
            label: "Test edit".into(),
            notes: vec![],
            view: "stems".into(),
        };
        let path = save(&snapshot, root.path(), &|| false).unwrap();
        let loaded = StudioDocument::load(&path).unwrap();
        assert_eq!(loaded.mix.tracks[0].gain_db, -3.);
        assert_eq!(loaded.preview.samples, snapshot.preview.samples);
        assert!(Arc::ptr_eq(
            &loaded.before,
            &loaded
                .stems
                .as_ref()
                .unwrap()
                .track(StemKind::Vocals)
                .unwrap()
                .audio
        ));
        assert_eq!(loaded.stems.unwrap().tracks.len(), 2);
        std::fs::write(path.parent().unwrap().join("preview.wav"), b"tampered").unwrap();
        assert!(StudioDocument::load(&path).is_err());
    }
    #[test]
    fn traversal_and_unknown_versions_are_rejected() {
        let root = tempfile::tempdir().unwrap();
        let source = Arc::new(Audio::new(vec![0.; 10], 8000, 1).unwrap());
        let snapshot = StudioSnapshot {
            source_name: "test".into(),
            source: source.clone(),
            stems: None,
            mix: MixSettings::default(),
            before: source.clone(),
            preview: source,
            label: "test".into(),
            notes: vec![],
            view: "voice".into(),
        };
        let path = save(&snapshot, root.path(), &|| false).unwrap();
        let mut document: StudioDocument =
            serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
        document.source.name = "../source.wav".into();
        std::fs::write(&path, serde_json::to_vec(&document).unwrap()).unwrap();
        assert!(StudioDocument::load(&path).is_err());
        document.version = 2;
        std::fs::write(&path, serde_json::to_vec(&document).unwrap()).unwrap();
        assert!(StudioDocument::load(&path).is_err());
    }
}
