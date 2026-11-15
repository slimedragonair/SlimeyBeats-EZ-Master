use super::{scan::Findings, thresholds::*, types::*};
use crate::audio::Audio;
use anyhow::{Result, ensure};
use rustfft::{FftPlanner, num_complex::Complex64};
use serde_json::json;
use std::f64::consts::TAU;

struct Measurement {
    band_fraction: f64,
    upper_fraction: f64,
    top_three_fraction: f64,
    occupied: usize,
}
impl Measurement {
    fn candidate(&self) -> bool {
        self.band_fraction >= LOW_MID_FRACTION
            && self.upper_fraction >= UPPER_FRACTION
            && self.top_three_fraction <= MAX_TOP_THREE_FRACTION
            && self.occupied >= 6
    }
}

struct Region {
    first: usize,
    last: usize,
    windows: usize,
    band_sum: f64,
    upper_sum: f64,
    top_sum: f64,
}

pub(super) fn inspect(
    audio: &Audio,
    cancel: &dyn Fn() -> bool,
    findings: &mut Findings,
) -> Result<String> {
    if audio.duration() < MIN_LOW_MID_SECONDS || audio.frames() < FFT_SIZE {
        return Ok("Low-mid measurement abstained: insufficient duration for sustained broadband evidence.".into());
    }
    let possible = audio.frames() / FFT_SIZE;
    let count = possible.min(MAX_SPECTRAL_WINDOWS);
    let mut planner = FftPlanner::<f64>::new();
    let fft = planner.plan_fft_forward(FFT_SIZE);
    let mut buffer = vec![Complex64::default(); FFT_SIZE];
    let mut scratch = vec![Complex64::default(); fft.get_inplace_scratch_len()];
    let window: Vec<_> = (0..FFT_SIZE)
        .map(|i| 0.5 - 0.5 * (TAU * i as f64 / FFT_SIZE as f64).cos())
        .collect();
    let mut power = vec![0_f64; FFT_SIZE / 2 + 1];
    let mut active = 0;
    let mut region: Option<Region> = None;
    for index in 0..count {
        ensure!(!cancel(), "Cancelled");
        let start = if count == 1 {
            0
        } else {
            ((audio.frames() - FFT_SIZE) as u128 * index as u128 / (count - 1) as u128) as usize
        };
        let mut channel_energy = [0_f64; 2];
        let mut scale = 0_f64;
        for frame in audio.samples[start * audio.channels..(start + FFT_SIZE) * audio.channels]
            .chunks_exact(audio.channels)
        {
            for (channel, &sample) in frame.iter().enumerate() {
                scale = scale.max((sample as f64).abs());
                channel_energy[channel] += (sample as f64).powi(2);
            }
        }
        if !channel_energy[..audio.channels]
            .iter()
            .any(|energy| *energy / FFT_SIZE as f64 >= ACTIVE_RMS.powi(2))
        {
            if let Some(previous) = region.take() {
                record_region(audio, previous, count, findings);
            }
            continue;
        }
        active += 1;
        power.fill(0.);
        for channel in 0..audio.channels {
            ensure!(!cancel(), "Cancelled");
            for i in 0..FFT_SIZE {
                // Normalize only this measurement buffer to avoid FFT overflow;
                // power ratios remain unchanged and PCM is never normalized.
                buffer[i] = Complex64::new(
                    audio.samples[(start + i) * audio.channels + channel] as f64 / scale
                        * window[i],
                    0.,
                );
            }
            fft.process_with_scratch(&mut buffer, &mut scratch);
            for (bin, value) in power.iter_mut().enumerate() {
                let one_sided = if bin == 0 || bin == FFT_SIZE / 2 {
                    1.
                } else {
                    2.
                };
                *value += one_sided * buffer[bin].norm_sqr();
            }
        }
        let measurement = measure(&power, audio.sample_rate);
        if measurement.candidate() {
            if let Some(previous) = region.as_ref()
                && quiet_between(audio, previous.last, start, cancel)?
            {
                record_region(audio, region.take().unwrap(), count, findings);
            }
            let current = region.get_or_insert(Region {
                first: start,
                last: start,
                windows: 0,
                band_sum: 0.,
                upper_sum: 0.,
                top_sum: 0.,
            });
            current.last = start + FFT_SIZE;
            current.windows += 1;
            current.band_sum += measurement.band_fraction;
            current.upper_sum += measurement.upper_fraction;
            current.top_sum += measurement.top_three_fraction;
        } else if let Some(previous) = region.take() {
            record_region(audio, previous, count, findings);
        }
    }
    if let Some(previous) = region {
        record_region(audio, previous, count, findings);
    }
    Ok(format!(
        "Low-mid heuristic measured {count} bounded, distributed FFT windows ({active} active). Regions split at noncandidate windows and measured quiet gaps; their coarse spans can contain unsampled audio. A finding is not an objective sound-quality judgment."
    ))
}

fn record_region(audio: &Audio, region: Region, total_windows: usize, findings: &mut Findings) {
    let observed_seconds = region.windows as f64 * FFT_SIZE as f64 / audio.sample_rate as f64;
    if region.windows >= MIN_SPECTRAL_WINDOWS && observed_seconds >= MIN_LOW_MID_SECONDS {
        let span_seconds = (region.last - region.first) as f64 / audio.sample_rate as f64;
        let unsampled_seconds = (span_seconds - observed_seconds).max(0.);
        findings.push(Issue {
            id: format!("low-mid-buildup-{}-{}", region.first, region.last), kind: IssueKind::LowMidBuildup,
            start_seconds: region.first as f64 / audio.sample_rate as f64,
            end_seconds: region.last as f64 / audio.sample_rate as f64,
            severity: "info".into(), title: "Sustained low-mid energy concentration".into(),
            description: "Consecutive sampled active windows concentrate broadband energy in 200–500 Hz. This coarse sampled span may include unsampled audio and does not establish continuous buildup or audible mud. Tonal and quiet windows split findings; audition any optional EQ.".into(),
            evidence: json!({"measurement": "hann_window_integrated_one_sided_bin_power_ratio",
                "band_hz": [200., 500.], "sampled_windows": region.windows,
                "total_track_sampled_windows": total_windows, "active_windows": region.windows,
                "matching_windows": region.windows, "observed_active_seconds": observed_seconds,
                "sampled_span_seconds": span_seconds, "unsampled_seconds_in_span": unsampled_seconds,
                "continuous_buildup_confirmed": false,
                "mean_low_mid_power_fraction": region.band_sum / region.windows as f64,
                "mean_above_700_hz_power_fraction": region.upper_sum / region.windows as f64,
                "mean_top_three_band_bins_fraction": region.top_sum / region.windows as f64,
                "sampling_capped": audio.frames() / FFT_SIZE > MAX_SPECTRAL_WINDOWS,
                "audible_mud_confirmed": false}),
            repair: Some(RepairAction::ReduceLowMids),
        });
    }
}

fn quiet_between(
    audio: &Audio,
    start: usize,
    end: usize,
    cancel: &dyn Fn() -> bool,
) -> Result<bool> {
    let required = (SILENCE_SECONDS * audio.sample_rate as f64).ceil() as usize;
    if end.saturating_sub(start) < required {
        return Ok(false);
    }
    let mut quiet_frames = 0;
    for frame in start..end {
        if (frame - start).is_multiple_of(CANCEL_FRAMES) {
            ensure!(!cancel(), "Cancelled");
        }
        let quiet = audio.samples[frame * audio.channels..(frame + 1) * audio.channels]
            .iter()
            .all(|sample| (*sample as f64).abs() <= QUIET_PEAK);
        quiet_frames = if quiet { quiet_frames + 1 } else { 0 };
        if quiet_frames >= required {
            return Ok(true);
        }
    }
    Ok(false)
}

fn measure(power: &[f64], sample_rate: u32) -> Measurement {
    let (mut total, mut band, mut upper) = (0., 0., 0.);
    let mut top = [0_f64; 3];
    let mut band_bins = 0;
    for (index, &energy) in power.iter().enumerate() {
        let frequency = index as f64 * sample_rate as f64 / FFT_SIZE as f64;
        if frequency < 20. {
            continue;
        }
        total += energy;
        if frequency >= 700. {
            upper += energy;
        }
        if (200. ..=500.).contains(&frequency) {
            band += energy;
            band_bins += 1;
            let mut value = energy;
            for item in &mut top {
                if value > *item {
                    std::mem::swap(&mut value, item);
                }
            }
        }
    }
    let minimum = band / band_bins.max(1) as f64 * 0.08;
    let occupied = power
        .iter()
        .enumerate()
        .filter(|(index, energy)| {
            let frequency = *index as f64 * sample_rate as f64 / FFT_SIZE as f64;
            (200. ..=500.).contains(&frequency) && **energy > minimum
        })
        .count();
    Measurement {
        band_fraction: band / total.max(1e-100),
        upper_fraction: upper / total.max(1e-100),
        top_three_fraction: top.iter().sum::<f64>() / band.max(1e-100),
        occupied,
    }
}
