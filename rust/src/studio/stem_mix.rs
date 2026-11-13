use crate::{audio::Audio, dsp::limiter, meters::lin};
use anyhow::{Context, Result, ensure};
use serde::{Deserialize, Serialize};
use std::{collections::BTreeSet, sync::Arc};

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
#[serde(rename_all = "snake_case")]
#[cfg_attr(feature = "schemas", derive(schemars::JsonSchema))]
pub enum StemKind {
    Vocals,
    Instrumental,
    Drums,
    Bass,
    Other,
}
impl StemKind {
    pub fn name(self) -> &'static str {
        match self {
            Self::Vocals => "vocals",
            Self::Instrumental => "instrumental",
            Self::Drums => "drums",
            Self::Bass => "bass",
            Self::Other => "other",
        }
    }
}

#[derive(Clone)]
pub struct Stem {
    pub kind: StemKind,
    pub audio: Arc<Audio>,
}

#[derive(Clone)]
pub struct StemSet {
    pub tracks: Vec<Stem>,
}
impl StemSet {
    pub fn new(tracks: Vec<Stem>) -> Result<Self> {
        ensure!(
            (2..=4).contains(&tracks.len()),
            "Import two to four aligned stems"
        );
        let first = &tracks[0].audio;
        let mut names = BTreeSet::new();
        for stem in &tracks {
            ensure!(names.insert(stem.kind), "Each stem kind must be unique");
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
        Ok(Self { tracks })
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
        Self::new(tracks)
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
        !set.tracks.iter().any(|s| s.kind == StemKind::Instrumental),
        "Four-stem mode expects vocals, drums and bass estimates"
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
