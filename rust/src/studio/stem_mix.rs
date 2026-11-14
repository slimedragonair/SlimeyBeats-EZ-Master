use crate::{audio::Audio, dsp::limiter, meters::lin};
use anyhow::{Context, Result, ensure};
use serde::{Deserialize, Serialize};
use std::{
    collections::{BTreeMap, BTreeSet},
    sync::Arc,
};

pub const MAX_STEMS: usize = 9;

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
#[serde(rename_all = "snake_case")]
#[cfg_attr(feature = "schemas", derive(schemars::JsonSchema))]
pub enum StemKind {
    Vocals,
    Instrumental,
    Drums,
    Bass,
    Other,
    Guitar,
    Piano,
    Synth,
    Strings,
    Percussion,
    #[serde(rename = "part_1")]
    Part1,
    #[serde(rename = "part_2")]
    Part2,
    #[serde(rename = "part_3")]
    Part3,
    #[serde(rename = "part_4")]
    Part4,
    #[serde(rename = "part_5")]
    Part5,
    #[serde(rename = "part_6")]
    Part6,
    #[serde(rename = "part_7")]
    Part7,
    #[serde(rename = "part_8")]
    Part8,
    #[serde(rename = "part_9")]
    Part9,
}
impl StemKind {
    pub const MULTITRACK: [Self; MAX_STEMS] = [
        Self::Vocals,
        Self::Drums,
        Self::Bass,
        Self::Guitar,
        Self::Piano,
        Self::Synth,
        Self::Strings,
        Self::Percussion,
        Self::Other,
    ];
    pub const ALL: [Self; 19] = [
        Self::Vocals,
        Self::Instrumental,
        Self::Drums,
        Self::Bass,
        Self::Other,
        Self::Guitar,
        Self::Piano,
        Self::Synth,
        Self::Strings,
        Self::Percussion,
        Self::Part1,
        Self::Part2,
        Self::Part3,
        Self::Part4,
        Self::Part5,
        Self::Part6,
        Self::Part7,
        Self::Part8,
        Self::Part9,
    ];
    pub fn name(self) -> &'static str {
        match self {
            Self::Vocals => "vocals",
            Self::Instrumental => "instrumental",
            Self::Drums => "drums",
            Self::Bass => "bass",
            Self::Other => "other",
            Self::Guitar => "guitar",
            Self::Piano => "piano",
            Self::Synth => "synth",
            Self::Strings => "strings",
            Self::Percussion => "percussion",
            Self::Part1 => "part_1",
            Self::Part2 => "part_2",
            Self::Part3 => "part_3",
            Self::Part4 => "part_4",
            Self::Part5 => "part_5",
            Self::Part6 => "part_6",
            Self::Part7 => "part_7",
            Self::Part8 => "part_8",
            Self::Part9 => "part_9",
        }
    }
}

/// Validate identity/count before allocating or decoding a bank's audio.
pub fn validate_kinds(kinds: impl IntoIterator<Item = StemKind>) -> Result<()> {
    let mut names = BTreeSet::new();
    for kind in kinds {
        ensure!(
            names.insert(kind),
            "Each stem kind must be unique; use part_1 through part_9 for additional parts of the same instrument"
        );
        ensure!(names.len() <= MAX_STEMS, "Import two to nine aligned stems");
    }
    ensure!(names.len() >= 2, "Import two to nine aligned stems");
    Ok(())
}

#[derive(Clone)]
pub struct Stem {
    pub kind: StemKind,
    pub audio: Arc<Audio>,
}

#[derive(Clone)]
pub struct StemSet {
    pub tracks: Vec<Stem>,
    display_names: BTreeMap<StemKind, String>,
}
impl StemSet {
    pub fn new(tracks: Vec<Stem>) -> Result<Self> {
        validate_kinds(tracks.iter().map(|stem| stem.kind))?;
        let first = &tracks[0].audio;
        for stem in &tracks {
            ensure!(
                stem.audio.sample_rate == first.sample_rate
                    && stem.audio.channels == first.channels
                    && stem.audio.frames() == first.frames(),
                "Stems must have identical sample rate, channels and frame count; align/resample before import"
            );
            ensure!(
                stem.audio.samples.iter().all(|v| v.is_finite()),
                "A stem contains non-finite samples"
            );
        }
        Ok(Self {
            tracks,
            display_names: BTreeMap::new(),
        })
    }
    /// Names are plain text metadata; identity and audio remain keyed by kind.
    pub fn set_display_name(&mut self, kind: StemKind, name: impl Into<String>) -> Result<()> {
        self.track(kind)?;
        let name = name.into();
        ensure!(
            !name.trim().is_empty() && name.len() <= 512 && !name.chars().any(char::is_control),
            "Stem display names must be nonempty plain text of at most 512 UTF-8 bytes"
        );
        self.display_names.insert(kind, name);
        Ok(())
    }
    pub fn display_name(&self, kind: StemKind) -> &str {
        self.display_names
            .get(&kind)
            .map(String::as_str)
            .unwrap_or(kind.name())
    }
    pub fn track(&self, kind: StemKind) -> Result<&Stem> {
        self.tracks
            .iter()
            .find(|s| s.kind == kind)
            .context("This stem is not present in the current set")
    }
    pub fn replace(&self, kind: StemKind, audio: Arc<Audio>) -> Result<Self> {
        self.track(kind)?;
        let mut tracks = self.tracks.clone();
        tracks
            .iter_mut()
            .find(|s| s.kind == kind)
            .expect("validated stem kind")
            .audio = audio;
        let mut bank = Self::new(tracks)?;
        bank.display_names = self.display_names.clone();
        Ok(bank)
    }
    pub fn defaults(&self) -> MixSettings {
        MixSettings {
            tracks: self
                .tracks
                .iter()
                .map(|s| StemControl {
                    kind: s.kind,
                    gain_db: 0.,
                    balance: 0.,
                    mute: false,
                    solo: false,
                })
                .collect(),
            ..Default::default()
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[cfg_attr(feature = "schemas", derive(schemars::JsonSchema))]
pub struct StemControl {
    pub kind: StemKind,
    pub gain_db: f64,
    pub balance: f64,
    pub mute: bool,
    pub solo: bool,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
#[cfg_attr(feature = "schemas", derive(schemars::JsonSchema))]
pub struct MixSettings {
    pub tracks: Vec<StemControl>,
    pub output_gain_db: f64,
    pub protect_peaks: bool,
    pub ceiling_dbtp: f64,
}
impl Default for MixSettings {
    fn default() -> Self {
        Self {
            tracks: vec![],
            output_gain_db: 0.,
            protect_peaks: true,
            ceiling_dbtp: -1.,
        }
    }
}
impl MixSettings {
    pub fn validate(&self, stems: &StemSet) -> Result<()> {
        ensure!(
            self.output_gain_db.is_finite() && (-24. ..=12.).contains(&self.output_gain_db),
            "Mix output gain must be -24 to +12 dB"
        );
        ensure!(
            self.ceiling_dbtp.is_finite() && (-12. ..=0.).contains(&self.ceiling_dbtp),
            "Mix ceiling must be -12 to 0 dBTP"
        );
        ensure!(
            self.tracks.len() == stems.tracks.len(),
            "Provide one control for every current stem"
        );
        let mut names = BTreeSet::new();
        for control in &self.tracks {
            let stem = stems.track(control.kind)?;
            ensure!(
                names.insert(control.kind),
                "Stem controls must not repeat a kind"
            );
            ensure!(
                control.gain_db.is_finite() && (-60. ..=12.).contains(&control.gain_db),
                "Stem gain must be -60 to +12 dB"
            );
            ensure!(
                control.balance.is_finite() && (-1. ..=1.).contains(&control.balance),
                "Stereo balance must be -1 to +1"
            );
            ensure!(
                stem.audio.channels == 2 || control.balance == 0.,
                "Stereo balance is unavailable on mono stems"
            );
        }
        Ok(())
    }
}

/// Every revision starts from the immutable stems; muting takes precedence over solo.
pub fn render(stems: &StemSet, settings: &MixSettings, cancel: &dyn Fn() -> bool) -> Result<Audio> {
    settings.validate(stems)?;
    ensure!(!cancel(), "Cancelled");
    let first = &stems.tracks[0].audio;
    let solo = settings.tracks.iter().any(|s| s.solo);
    let mut output = Audio::new(
        vec![0.; first.samples.len()],
        first.sample_rate,
        first.channels,
    )?;
    for control in &settings.tracks {
        if control.mute || (solo && !control.solo) {
            continue;
        }
        let source = &stems.track(control.kind)?.audio;
        let gain = lin(control.gain_db + settings.output_gain_db) as f32;
        let left = if control.balance > 0. {
            (control.balance * std::f64::consts::FRAC_PI_2).cos() as f32
        } else {
            1.
        };
        let right = if control.balance < 0. {
            (-control.balance * std::f64::consts::FRAC_PI_2).cos() as f32
        } else {
            1.
        };
        for (i, (out, &sample)) in output.samples.iter_mut().zip(&source.samples).enumerate() {
            if i % 65536 == 0 {
                ensure!(!cancel(), "Cancelled");
            }
            *out += sample
                * gain
                * if first.channels == 2 && i % 2 == 1 {
                    right
                } else {
                    left
                };
        }
    }
    ensure!(
        output.samples.iter().all(|v| v.is_finite()),
        "Mix overflowed; reduce stem/output gain"
    );
    if settings.protect_peaks {
        limiter::guard(&mut output, settings.ceiling_dbtp)?;
    }
    ensure!(!cancel(), "Cancelled");
    Ok(output)
}

/// Assign model reconstruction residual to the accompaniment/other stem so a
/// neutral rebalanced mix retains the original, including unmodelled bandwidth.
pub fn consistent(original: &Audio, mut estimates: Vec<Stem>, two_stems: bool) -> Result<StemSet> {
    let vocals = estimates
        .iter()
        .find(|s| s.kind == StemKind::Vocals)
        .context("Model returned no vocals")?
        .audio
        .clone();
    ensure!(
        vocals.sample_rate == original.sample_rate
            && vocals.channels == original.channels
            && vocals.frames() == original.frames(),
        "Model vocal layout does not match input"
    );
    if two_stems {
        let samples = original
            .samples
            .iter()
            .zip(&vocals.samples)
            .map(|(&a, &v)| a - v)
            .collect();
        return StemSet::new(vec![
            Stem {
                kind: StemKind::Vocals,
                audio: vocals,
            },
            Stem {
                kind: StemKind::Instrumental,
                audio: Arc::new(Audio::new(
                    samples,
                    original.sample_rate,
                    original.channels,
                )?),
            },
        ]);
    }
    let set = StemSet::new(estimates.clone())?;
    for kind in [StemKind::Vocals, StemKind::Drums, StemKind::Bass] {
        set.track(kind)?;
    }
    ensure!(
        set.tracks.iter().all(|s| matches!(
            s.kind,
            StemKind::Vocals | StemKind::Drums | StemKind::Bass | StemKind::Other
        )),
        "Four-stem mode expects only vocals, drums, bass and optional other estimates"
    );
    let mut residual = original.clone();
    for stem in set.tracks.iter().filter(|s| s.kind != StemKind::Other) {
        for (sample, &value) in residual.samples.iter_mut().zip(&stem.audio.samples) {
            *sample -= value;
        }
    }
    estimates.retain(|s| s.kind != StemKind::Other);
    estimates.push(Stem {
        kind: StemKind::Other,
        audio: Arc::new(residual),
    });
    StemSet::new(estimates)
}
