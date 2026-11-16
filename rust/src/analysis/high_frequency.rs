//! Bounded, level-independent evidence for a conservative upper-treble EQ proposal.
//!
//! This measures frequency concentration, not instruments, AI provenance or damage.
use crate::meters::percentile;
use serde::{Deserialize, Serialize};

pub const LOW_HZ: f64 = 10_000.;
pub const MAX_HIGH_HZ: f64 = 16_000.;
const MIN_WIDTH_HZ: f64 = 4_000.;
const REFERENCE_LOW_HZ: f64 = 2_000.;
const REFERENCE_HIGH_HZ: f64 = 8_000.;
const MIN_DURATION_SECONDS: f64 = 2.;
const MIN_ANCHOR_FRACTION: f64 = 0.01;
const MIN_DENSITY_CONTRAST_DB: f64 = 3.;
const MIN_HIGH_FRACTION: f64 = 0.18;
const MIN_FLATNESS: f64 = 0.18;
const MAX_TOP_THREE_FRACTION: f64 = 0.35;
const MIN_SUPPORT_SECONDS: f64 = 0.5;
const MIN_CANDIDATE_WINDOWS: usize = 8;
const MIN_GROUP_WINDOWS: usize = 2;
const MIN_RECURRENT_GROUPS: usize = 3;
const MIN_RECURRENT_FRACTION: f64 = 0.05;
const MIN_PERSISTENT_FRACTION: f64 = 0.3;
const MIN_PERSISTENT_WINDOWS: usize = 6;
pub const MAX_CUT_DB: f64 = 2.5;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct HighFrequencySummary {
    pub version: u32,
    pub band_low_hz: f64,
    pub band_high_hz: f64,
    pub reference_low_hz: f64,
    pub reference_high_hz: f64,
    pub sampled_windows: usize,
    pub active_windows: usize,
    pub candidate_windows: usize,
    /// Runs separated by measured noncandidate windows, each with at least two matches.
    pub candidate_groups: usize,
    pub candidate_fraction: f64,
    /// Union of measured matching windows; excludes overlapping and unsampled time.
    pub observed_candidate_seconds: f64,
    pub mean_high_power_fraction: f64,
    /// Percentile across active windows, using bandwidth-normalized power once.
    pub p90_high_to_presence_db: f64,
    pub mean_high_flatness: f64,
    pub excess_candidate: bool,
    pub suggested_cut_db: f64,
    pub sampling_capped: bool,
}

impl HighFrequencySummary {
    pub fn valid(&self, sample_rate: u32, duration_s: f64) -> bool {
        let finite = [
            self.band_low_hz,
            self.band_high_hz,
            self.reference_low_hz,
            self.reference_high_hz,
            self.candidate_fraction,
            self.observed_candidate_seconds,
            self.mean_high_power_fraction,
            self.p90_high_to_presence_db,
            self.mean_high_flatness,
            self.suggested_cut_db,
        ]
        .iter()
        .all(|v| v.is_finite());
        finite
            && duration_s.is_finite()
            && duration_s >= MIN_DURATION_SECONDS
            && self.version == 1
            && self.band_low_hz == LOW_HZ
            && (LOW_HZ + MIN_WIDTH_HZ..=MAX_HIGH_HZ).contains(&self.band_high_hz)
            && self.band_high_hz <= sample_rate as f64 * 0.49 + 0.001
            && self.reference_low_hz == REFERENCE_LOW_HZ
            && self.reference_high_hz == REFERENCE_HIGH_HZ
            && self.sampled_windows <= 3072
            && self.active_windows <= self.sampled_windows
            && self.candidate_windows <= self.active_windows
            && self.candidate_groups <= self.candidate_windows / MIN_GROUP_WINDOWS
            && (0. ..=1.).contains(&self.candidate_fraction)
            && (self.candidate_fraction
                - self.candidate_windows as f64 / self.active_windows.max(1) as f64)
                .abs()
                < 1e-6
            && (0. ..=duration_s + 0.001).contains(&self.observed_candidate_seconds)
            && (0. ..=1.).contains(&self.mean_high_power_fraction)
            && (-120. ..=120.).contains(&self.p90_high_to_presence_db)
            && (0. ..=1.).contains(&self.mean_high_flatness)
            && (-MAX_CUT_DB..=0.).contains(&self.suggested_cut_db)
            && if self.excess_candidate {
                self.candidate_windows >= MIN_CANDIDATE_WINDOWS
                    && self.observed_candidate_seconds >= MIN_SUPPORT_SECONDS
                    && self.candidate_groups > 0
                    && self.suggested_cut_db <= -0.75
            } else {
                self.suggested_cut_db == 0.
            }
    }

    pub fn note(&self) -> String {
        if self.excess_candidate {
            format!(
                "Repeated or sustained broadband energy is concentrated at {:.0}–{:.0} kHz in {:.0}% of sampled active windows. This can sound like brittle percussion or hiss; it does not identify an instrument or prove a defect. Audition a gentle high-shelf cut.",
                self.band_low_hz / 1000.,
                self.band_high_hz / 1000.,
                self.candidate_fraction * 100.
            )
        } else {
            "No supported repeated or sustained upper-treble concentration was found. Intentional brightness is not proof of a problem; the normal tonal proposal still applies.".into()
        }
    }
}

pub(super) struct Accumulator {
    sample_rate: u32,
    fft_size: usize,
    frames: usize,
    possible_windows: usize,
    high_hz: f64,
    starts: usize,
    active: usize,
    candidates: usize,
    groups: usize,
    run: usize,
    longest_run: usize,
    observed_frames: usize,
    observed_end: usize,
    fractions: f64,
    flatness: f64,
    contrasts: Vec<f64>,
}

impl Accumulator {
    pub(super) fn new(
        sample_rate: u32,
        fft_size: usize,
        frames: usize,
        possible_windows: usize,
    ) -> Option<Self> {
        let high_hz = MAX_HIGH_HZ.min(sample_rate as f64 * 0.49);
        if high_hz - LOW_HZ < MIN_WIDTH_HZ
            || frames as f64 / (sample_rate as f64) < MIN_DURATION_SECONDS
        {
            return None;
        }
        Some(Self {
            sample_rate,
            fft_size,
            frames,
            possible_windows,
            high_hz,
            starts: 0,
            active: 0,
            candidates: 0,
            groups: 0,
            run: 0,
            longest_run: 0,
            observed_frames: 0,
            observed_end: 0,
            fractions: 0.,
            flatness: 0.,
            contrasts: vec![],
        })
    }

    fn end_group(&mut self) {
        if self.run >= MIN_GROUP_WINDOWS {
            self.groups += 1;
        }
        self.longest_run = self.longest_run.max(self.run);
        self.run = 0;
    }

    pub(super) fn observe(&mut self, power: &[f64], start: usize, active: bool) {
        self.starts += 1;
        if !active {
            self.end_group();
            return;
        }
        let df = self.sample_rate as f64 / self.fft_size as f64;
        let sum = |lo: f64, hi: f64| {
            power
                .iter()
                .enumerate()
                .filter(|(i, _)| *i as f64 * df >= lo && (*i as f64 * df) < hi)
                .map(|(_, value)| value)
                .sum::<f64>()
        };
        let total = sum(20., self.high_hz);
        let high = sum(LOW_HZ, self.high_hz);
        let anchor = sum(REFERENCE_LOW_HZ, REFERENCE_HIGH_HZ);
        if !total.is_finite() || total <= 0. || !high.is_finite() || !anchor.is_finite() {
            self.end_group();
            return;
        }
        self.active += 1;
        let fraction = (high / total).clamp(0., 1.);
        let contrast = (10.
            * ((high / (self.high_hz - LOW_HZ))
                / (anchor / (REFERENCE_HIGH_HZ - REFERENCE_LOW_HZ)).max(1e-100))
            .max(1e-100)
            .log10())
        .clamp(-120., 120.);
        let lo = (LOW_HZ / df).ceil() as usize;
        let hi = (self.high_hz / df).ceil().min(power.len() as f64) as usize;
        let bins = &power[lo..hi];
        let mean = high / bins.len().max(1) as f64;
        let flatness = if mean > 0. {
            ((bins.iter().map(|p| p.max(mean * 1e-10).ln()).sum::<f64>()
                / bins.len().max(1) as f64)
                .exp()
                / mean)
                .clamp(0., 1.)
        } else {
            0.
        };
        let mut top = [0_f64; 3];
        for &value in bins {
            let mut next = value;
            for item in &mut top {
                if next > *item {
                    std::mem::swap(&mut next, item);
                }
            }
        }
        self.fractions += fraction;
        self.flatness += flatness;
        self.contrasts.push(contrast);
        let candidate = anchor / total >= MIN_ANCHOR_FRACTION
            && contrast > MIN_DENSITY_CONTRAST_DB
            && fraction > MIN_HIGH_FRACTION
            && flatness > MIN_FLATNESS
            && top.iter().sum::<f64>() / high.max(1e-100) < MAX_TOP_THREE_FRACTION;
        if candidate {
            self.candidates += 1;
            self.run += 1;
            let end = (start + self.fft_size).min(self.frames);
            self.observed_frames += end.saturating_sub(start.max(self.observed_end));
            self.observed_end = end;
        } else {
            self.end_group();
        }
    }

    pub(super) fn finish(mut self, cutoff_hz: Option<f64>) -> Option<HighFrequencySummary> {
        if cutoff_hz.is_some_and(|cutoff| cutoff < LOW_HZ + MIN_WIDTH_HZ) {
            return None;
        }
        self.end_group();
        let fraction = self.candidates as f64 / self.active.max(1) as f64;
        let observed = self.observed_frames as f64 / self.sample_rate as f64;
        let persistent =
            fraction >= MIN_PERSISTENT_FRACTION && self.longest_run >= MIN_PERSISTENT_WINDOWS;
        let recurrent = fraction >= MIN_RECURRENT_FRACTION && self.groups >= MIN_RECURRENT_GROUPS;
        let excess = self.candidates >= MIN_CANDIDATE_WINDOWS
            && observed >= MIN_SUPPORT_SECONDS
            && (persistent || recurrent);
        let p90 = percentile(&self.contrasts, 0.9);
        let cut = if excess {
            let amount = (0.75 + 0.15 * (p90 - MIN_DENSITY_CONTRAST_DB).max(0.) + 0.4 * fraction)
                .clamp(0.75, MAX_CUT_DB);
            -(amount * 4.).round() / 4.
        } else {
            0.
        };
        Some(HighFrequencySummary {
            version: 1,
            band_low_hz: LOW_HZ,
            band_high_hz: self.high_hz,
            reference_low_hz: REFERENCE_LOW_HZ,
            reference_high_hz: REFERENCE_HIGH_HZ,
            sampled_windows: self.starts,
            active_windows: self.active,
            candidate_windows: self.candidates,
            candidate_groups: self.groups,
            candidate_fraction: fraction,
            observed_candidate_seconds: observed,
            mean_high_power_fraction: self.fractions / self.active.max(1) as f64,
            p90_high_to_presence_db: p90,
            mean_high_flatness: self.flatness / self.active.max(1) as f64,
            excess_candidate: excess,
            suggested_cut_db: cut,
            sampling_capped: self.possible_windows > self.starts,
        })
    }
}
