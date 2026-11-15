use super::{declip, low_mids, thresholds::*, types::*, validation};
use crate::audio::Audio;
use anyhow::{Result, ensure};

/// Produce a separate revision from unchanged input. Frame times round to the
/// nearest frame, with half frames upward and an exclusive end. Safety measures
/// concern PCM sample peaks only; they do not certify reconstructed true peaks.
pub fn repair(
    input: &Audio,
    request: &RepairRequest,
    cancel: &dyn Fn() -> bool,
) -> Result<RepairResult> {
    ensure!(!cancel(), "Cancelled");
    let range = request.frame_range(input)?;
    let mut audio = validation::copy(input, cancel)?;
    if request.strength == 0. {
        return Ok(RepairResult {
            audio,
            notes: vec!["Zero strength: bit-exact identity revision.".into()],
            changed_samples: 0,
            peak_trim_db: 0.,
        });
    }
    let (mut notes, peak_trim_db) = match request.action {
        RepairAction::Declip => {
            declip::apply(input, &mut audio, range.clone(), request.strength, cancel)?
        }
        RepairAction::ReduceLowMids => {
            low_mids::apply(input, &mut audio, range.clone(), request.strength, cancel)?
        }
    };
    let mut changed_samples = 0;
    for (chunk_index, samples) in audio.samples
        [range.start * input.channels..range.end * input.channels]
        .chunks(CANCEL_FRAMES * input.channels)
        .enumerate()
    {
        ensure!(!cancel(), "Cancelled");
        let start = range.start * input.channels + chunk_index * CANCEL_FRAMES * input.channels;
        for (&after, &before) in samples
            .iter()
            .zip(&input.samples[start..start + samples.len()])
        {
            ensure!(after.is_finite(), "Doctor produced non-finite audio");
            changed_samples += usize::from(after.to_bits() != before.to_bits());
        }
    }
    ensure!(!cancel(), "Cancelled");
    if changed_samples == 0 {
        notes.push(
            "No samples changed; the selected region had no safely applicable change.".into(),
        );
    }
    notes.push("Duration, sample rate and channels are preserved. Samples outside the selected frames are bit-exact. Sample-peak protection is not true-peak certification.".into());
    Ok(RepairResult {
        audio,
        notes,
        changed_samples,
        peak_trim_db,
    })
}

/// Protect a changed sample without altering any other sample or increasing an
/// existing above-unity input peak. Preserve exact dry bits whenever unchanged.
pub(super) fn protected(dry: f32, proposed: f64, peak_trim_db: &mut f64) -> f32 {
    let ceiling = (dry as f64).abs().max(1.);
    let bounded = proposed.clamp(-ceiling, ceiling);
    if proposed.abs() > ceiling {
        *peak_trim_db = peak_trim_db.max(20. * (proposed.abs() / ceiling).log10());
    }
    if bounded == dry as f64 {
        dry
    } else {
        bounded as f32
    }
}
