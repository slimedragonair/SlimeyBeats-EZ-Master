use crate::{
    audio::Audio,
    dsp::{
        convolution,
        dynamics::{self, HOP},
    },
    meters::percentile,
};
pub fn detect(audio: &Audio) -> Vec<usize> {
    let band = convolution::band(audio, 50., 180.);
    let levels = dynamics::block_db(&band, 4.);
    let slow = dynamics::smooth(&levels, audio.sample_rate, 30., 150., false);
    let floor = percentile(&levels, 0.8) - 20.;
    let separation = (audio.sample_rate as f64 * 0.065) as usize;
    let mut hits = Vec::new();
    for i in 1..levels.len() {
        let sample = i * HOP;
        if levels[i] > floor
            && levels[i] - slow[i] > 4.
            && levels[i] > levels[i - 1] + 0.25
            && hits.last().is_none_or(|&last| sample - last > separation)
        {
            hits.push(sample);
        }
    }
    hits
}
pub fn protect(frames: usize, sr: u32, hits: &[usize]) -> Vec<f32> {
    let mut mask = vec![0_f32; frames.div_ceil(HOP)];
    let length = (sr as f64 * 0.040 / HOP as f64) as usize;
    for &hit in hits {
        let start = (hit / HOP).saturating_sub(1);
        let end = (start + length).min(mask.len());
        for (i, value) in mask[start..end].iter_mut().enumerate() {
            let fade = if i < length * 3 / 4 {
                1.
            } else {
                (length - i) as f32 / (length / 4).max(1) as f32
            };
            *value = value.max(fade);
        }
    }
    mask
}
pub fn punch(audio: &Audio, hits: &[usize]) -> f64 {
    if hits.is_empty() {
        return 0.;
    }
    let band = convolution::band(audio, 50., 250.);
    let early = (audio.sample_rate as f64 * 0.025) as usize;
    let late = (audio.sample_rate as f64 * 0.100) as usize;
    let mut ratios = Vec::new();
    for &hit in hits {
        let end = (hit + late).min(audio.frames());
        if hit + early >= end {
            continue;
        }
        let energy = |a: usize, b: usize| {
            band.samples[a * audio.channels..b * audio.channels]
                .iter()
                .map(|&s| (s as f64).powi(2))
                .sum::<f64>()
                / ((b - a) * audio.channels) as f64
        };
        ratios.push(
            10. * (energy(hit, hit + early) / energy(hit + early, end).max(1e-20))
                .max(1e-20)
                .log10(),
        );
    }
    percentile(&ratios, 0.5)
}
