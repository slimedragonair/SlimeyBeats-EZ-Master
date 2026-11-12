use mastercraft::{
    analysis,
    audio::Audio,
    config::Settings,
    dsp::{biquad, eq_loudness::EqLoudnessModel},
    meters,
};
use std::f64::consts::TAU;

fn white_noise(rate: u32) -> Audio {
    let mut state = 0x1a2b_3c4d_u32;
    let samples = (0..rate * 3)
        .map(|_| {
            state ^= state << 13;
            state ^= state >> 17;
            state ^= state << 5;
            (0.15 * (state as f64 / u32::MAX as f64 * 2. - 1.)) as f32
        })
        .collect();
    Audio::new(samples, rate, 1).unwrap()
}

#[test]
fn spectrum_is_mean_bin_power_while_octaves_integrate_bandwidth() {
    for rate in [32000, 48000, 96000] {
        let audio = white_noise(rate);
        let spectrum = analysis::spectrum(&audio);
        let fft_size = if rate > 50000 { 8192. } else { 4096. };
        let mean_square = audio
            .samples
            .iter()
            .map(|&x| (x as f64).powi(2))
            .sum::<f64>()
            / audio.samples.len() as f64;
        let expected_bin_db = 10. * (2. * mean_square / fft_size).log10();
        let bins: Vec<_> = spectrum
            .freqs
            .iter()
            .zip(&spectrum.db)
            .filter(|(f, _)| **f >= 400. && **f <= 6000.)
            .map(|(_, db)| *db)
            .collect();
        let mean_db = bins.iter().sum::<f64>() / bins.len() as f64;
        assert!(
            (mean_db - expected_bin_db).abs() < 0.4,
            "{rate}: {mean_db} vs {expected_bin_db}"
        );
        // Doubling the center doubles octave bandwidth, so white-noise octave power rises ~3 dB.
        let octave_ratio = 10. * (spectrum.octave_power[6] / spectrum.octave_power[5]).log10();
        assert!(
            (octave_ratio - 3.0103).abs() < 0.5,
            "{rate}: {octave_ratio}"
        );
    }
}

#[test]
fn fast_level_estimate_agrees_with_measured_broad_eq_delta_on_steady_signals() {
    for rate in [32000, 48000, 96000] {
        for tonal in [false, true] {
            let audio = if tonal {
                let samples = (0..rate * 3)
                    .map(|i| {
                        let t = i as f64 / rate as f64;
                        [80., 170., 390., 850., 1800., 4100., 7900.]
                            .iter()
                            .map(|f| 0.025 * (TAU * f * t).sin())
                            .sum::<f64>() as f32
                    })
                    .collect();
                Audio::new(samples, rate, 1).unwrap()
            } else {
                white_noise(rate)
            };
            let before = meters::measure(&audio).unwrap().integrated_lufs;
            let model = EqLoudnessModel::new(&analysis::spectrum(&audio), rate, before);
            let mut settings = Settings::default();
            for band in &mut settings.manual_eq {
                band.on = false;
            }
            settings.manual_eq[6].on = true;
            settings.manual_eq[6].freq = 3000.;
            settings.manual_eq[6].gain = 4.;
            let mut processed = audio.clone();
            biquad::from_band(&settings.manual_eq[6], rate).apply(&mut processed);
            let actual = meters::measure(&processed).unwrap().integrated_lufs - before;
            let estimate = model.delta_db(&settings);
            // This is a tested product bound for steady fixtures, not an EBU-conformant fast meter.
            assert!(
                (actual - estimate).abs() < 0.7,
                "{rate}/{tonal}: measured {actual}, estimate {estimate}"
            );
        }
    }
}
