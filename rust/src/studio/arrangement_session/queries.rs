use super::{ArrangementSession, EqResponse, Waveform};
use crate::{
    dsp::biquad,
    studio::arrangement::{Edit, FrameRange, ScopedEq, apply_edit},
};
use anyhow::{Context, Result, ensure};

const MAX_ENVELOPE_BINS: usize = 16_384;

impl ArrangementSession {
    pub fn waveform(&self, asset_id: &str, range: FrameRange, bins: usize) -> Result<Waveform> {
        let audio = self
            .assets
            .get(asset_id)
            .context("Arrangement asset ID is unavailable")?;
        ensure!(
            range.start_frame < range.end_frame && range.end_frame <= audio.frames() as u64,
            "Waveform bounds must select existing immutable source frames"
        );
        ensure!(
            (1..=MAX_ENVELOPE_BINS).contains(&bins),
            "Waveform bins must be 1 to 16384"
        );
        let frames = (range.end_frame - range.start_frame) as usize;
        let bins = bins.min(frames);
        let peaks = (0..bins)
            .map(|index| {
                let first = range.start_frame as usize + index * frames / bins;
                let last = range.start_frame as usize + (index + 1) * frames / bins;
                audio.samples[first * audio.channels..last * audio.channels]
                    .iter()
                    .fold(
                        [f32::INFINITY, f32::NEG_INFINITY],
                        |[low, high], &sample| [low.min(sample), high.max(sample)],
                    )
            })
            .collect();
        Ok(Waveform {
            peaks,
            start_frame: range.start_frame,
            end_frame: range.end_frame,
        })
    }

    pub fn eq_response(&self, effect: &ScopedEq) -> Result<EqResponse> {
        apply_edit(
            self.document(),
            &Edit::SetEq {
                effect: effect.clone(),
            },
            &self.assets,
        )?;
        let rate = self.document().sample_rate;
        let high = 20_000_f64.min(rate as f64 * 0.49);
        let frequencies: Vec<_> = (0..240)
            .map(|index| 20. * (high / 20.).powf(index as f64 / 239.))
            .collect();
        let gain_db = frequencies
            .iter()
            .map(|&frequency| {
                if effect.bypass {
                    0.
                } else {
                    effect.output_gain_db
                        + effect
                            .bands
                            .iter()
                            .filter(|band| band.on)
                            .map(|band| biquad::from_band(band, rate).response_db(frequency, rate))
                            .sum::<f64>()
                }
            })
            .collect();
        Ok(EqResponse {
            frequencies,
            gain_db,
        })
    }
}
