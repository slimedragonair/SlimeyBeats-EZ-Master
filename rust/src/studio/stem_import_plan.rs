//! Pair alternate encodings by basename, then assign stable, independent part IDs.
use super::stem_mix::{self, StemKind};
use anyhow::{Result, ensure};
use serde::{Deserialize, Serialize};
use std::{
    collections::{BTreeMap, BTreeSet},
    path::Path,
};

pub const MAX_IMPORT_FILES: usize = 512;
pub const MAX_IMPORT_PARTS: usize = 64;

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[cfg_attr(feature = "schemas", derive(schemars::JsonSchema))]
pub enum PreferredFormat {
    #[default]
    Wav,
    Mp3,
}
impl PreferredFormat {
    pub fn extension(&self) -> &'static str {
        match self {
            Self::Wav => "wav",
            Self::Mp3 => "mp3",
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ImportFile {
    pub index: usize,
    pub name: String,
    pub format: String,
    pub bytes: u64,
    pub crc32: Option<u32>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ImportPart {
    pub name: String,
    pub kind: StemKind,
    pub versions: Vec<ImportFile>,
}
impl ImportPart {
    pub fn preferred(&self, format: &PreferredFormat) -> &ImportFile {
        self.versions
            .iter()
            .find(|file| file.format == format.extension())
            .or_else(|| self.versions.iter().find(|file| file.format == "wav"))
            .unwrap_or(&self.versions[0])
    }
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ImportPlan {
    pub parts: Vec<ImportPart>,
    pub skipped_files: usize,
}

pub fn file_format(name: &str) -> Option<String> {
    let extension = Path::new(name).extension()?.to_str()?.to_ascii_lowercase();
    crate::audio::EXTENSIONS
        .contains(&extension.as_str())
        .then_some(extension)
}

pub fn available_kind(preferred: StemKind, used: &BTreeSet<StemKind>) -> StemKind {
    if !used.contains(&preferred) {
        return preferred;
    }
    StemKind::ALL
        .into_iter()
        .filter(|kind| kind.name().starts_with("part_"))
        .find(|kind| !used.contains(kind))
        .unwrap_or(StemKind::Part9)
}

fn suggested_role(name: &str) -> (StemKind, u8) {
    // Suno puts the part role last; words in the song title are not stem roles.
    let role = name
        .trim_end()
        .strip_suffix(')')
        .and_then(|name| name.rsplit_once('(').map(|(_, role)| role))
        .unwrap_or(name);
    let words: BTreeSet<_> = role
        .split(|c: char| !c.is_alphanumeric())
        .map(str::to_ascii_lowercase)
        .collect();
    if words.iter().any(|word| {
        matches!(
            word.as_str(),
            "backing" | "background" | "harmony" | "harmonies"
        ) || ["backing", "background", "harmony", "harmonies"]
            .iter()
            .any(|prefix| {
                word.strip_prefix(prefix)
                    .is_some_and(|rest| matches!(rest, "vocal" | "vocals" | "voice" | "voices"))
            })
    }) {
        return (StemKind::Part1, 8);
    }
    for (tokens, kind, priority) in [
        (
            &[
                "vocals",
                "vocal",
                "voice",
                "voices",
                "leadvocals",
                "leadvocal",
                "leadvoice",
            ][..],
            StemKind::Vocals,
            0,
        ),
        (&["drums", "drum"][..], StemKind::Drums, 1),
        (&["bass"][..], StemKind::Bass, 2),
        (&["guitar", "guitars"][..], StemKind::Guitar, 3),
        (&["piano", "keys", "keyboard"][..], StemKind::Piano, 4),
        (&["synth", "synthesizer", "synths"][..], StemKind::Synth, 5),
        (&["strings", "violin", "cello"][..], StemKind::Strings, 6),
        (&["percussion"][..], StemKind::Percussion, 7),
        (&["other"][..], StemKind::Other, 9),
        (
            &["instrumental", "accompaniment"][..],
            StemKind::Instrumental,
            10,
        ),
    ] {
        if tokens.iter().any(|word| words.contains(*word)) {
            return (kind, priority);
        }
    }
    let priority = if ["brass", "fx", "effects"]
        .iter()
        .any(|word| words.contains(*word))
    {
        11
    } else {
        10
    };
    (StemKind::Part1, priority)
}

pub fn plan(files: Vec<ImportFile>) -> Result<ImportPlan> {
    ensure!(
        files.len() <= MAX_IMPORT_FILES,
        "Choose at most 512 import files"
    );
    let mut groups: BTreeMap<String, ImportPart> = BTreeMap::new();
    let mut skipped_files = 0;
    for file in files {
        if file_format(&file.name).as_deref() != Some(file.format.as_str()) {
            skipped_files += 1;
            continue;
        }
        let name = file.name.rsplit(['/', '\\']).next().unwrap_or(&file.name);
        let name = Path::new(name)
            .file_stem()
            .and_then(|name| name.to_str())
            .unwrap_or("");
        ensure!(
            !name.is_empty() && name.len() <= 512,
            "Stem filenames must contain a name of at most 512 bytes"
        );
        let group = groups
            .entry(name.to_lowercase())
            .or_insert_with(|| ImportPart {
                name: name.into(),
                kind: suggested_role(name).0,
                versions: vec![],
            });
        ensure!(
            !group
                .versions
                .iter()
                .any(|other| other.format == file.format),
            "More than one {} file is named '{}'. Rename those parts before importing so each stem is unambiguous",
            file.format,
            name
        );
        group.versions.push(file);
        ensure!(
            groups.len() <= MAX_IMPORT_PARTS,
            "This bundle has more than 64 distinct parts; select a smaller collection"
        );
    }
    ensure!(
        groups.len() >= 2,
        "Choose at least two distinct audio stems (WAV/MP3 versions of one part count once)"
    );
    let mut used = BTreeSet::new();
    let mut parts: Vec<_> = groups.into_values().collect();
    // Review defaults prefer the core arrangement while keeping every part available.
    // ImportFile.index still identifies the original loose file or ZIP entry.
    parts.sort_by_cached_key(|part| (suggested_role(&part.name).1, part.name.to_lowercase()));
    for part in &mut parts {
        part.kind = available_kind(part.kind, &used);
        used.insert(part.kind);
    }
    Ok(ImportPlan {
        parts,
        skipped_files,
    })
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[cfg_attr(feature = "schemas", derive(schemars::JsonSchema))]
pub struct PartSelection {
    pub part_index: usize,
    #[serde(default)]
    pub kind: Option<StemKind>,
}
pub fn select(
    plan: &ImportPlan,
    selection: &[PartSelection],
    format: &PreferredFormat,
) -> Result<Vec<(StemKind, ImportFile)>> {
    let mut indices = BTreeSet::new();
    let mut used = BTreeSet::new();
    let mut files = Vec::new();
    for selected in selection {
        ensure!(
            indices.insert(selected.part_index),
            "A stem part was selected more than once"
        );
        let part = plan
            .parts
            .get(selected.part_index)
            .ok_or_else(|| anyhow::anyhow!("Unknown stem part index"))?;
        let kind = selected
            .kind
            .unwrap_or_else(|| available_kind(part.kind, &used));
        used.insert(kind);
        files.push((kind, part.preferred(format).clone()));
    }
    stem_mix::validate_kinds(files.iter().map(|(kind, _)| *kind))?;
    Ok(files)
}
