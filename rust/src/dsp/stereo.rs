use crate::{audio::Audio, dsp::convolution, meters::lin};

pub fn process(audio: &mut Audio, mono_hz: f64, mono_amount: f64, side_db: f64) {
    if audio.channels != 2 {
        return;
    }
    let side = Audio {
        samples: audio
            .samples
            .as_chunks::<2>()
            .0
            .iter()
            .map(|f| (f[0] - f[1]) * 0.5)
            .collect(),
        channels: 1,
        sample_rate: audio.sample_rate,
    };
    let mut s = side.clone();
    if mono_hz > 0. && mono_amount > 0. {
        let low = convolution::lowpass(&side, mono_hz);
        for (v, l) in s.samples.iter_mut().zip(low.samples) {
            *v -= l * mono_amount as f32;
        }
    }
    if side_db.abs() > 0.05 {
        let low = convolution::lowpass(&s, 300.);
        let gain = lin(side_db) as f32;
        for (v, l) in s.samples.iter_mut().zip(low.samples) {
            *v = l + (*v - l) * gain;
        }
    }
    for (frame, side) in audio
        .samples
        .as_chunks_mut::<2>()
        .0
        .iter_mut()
        .zip(s.samples)
    {
        let mid = (frame[0] + frame[1]) * 0.5;
        frame[0] = mid + side;
        frame[1] = mid - side;
    }
}
