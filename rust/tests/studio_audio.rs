use mastercraft::{
    audio::Audio,
    studio::{
        spectral_repair::{self, RepairMode, RepairSelection},
        spectrogram,
        stem_mix::{self, Stem, StemKind, StemSet},
        voice_cleanup::{self, NoiseRegion, VoiceSettings},
    },
};
use std::{f64::consts::TAU, sync::Arc};

fn tone(seconds: f64, frequency: f64, level: f64) -> Audio {
    Audio::new(
        (0..(seconds * 48000.) as usize)
            .map(|i| (level * (i as f64 / 48000. * frequency * TAU).sin()) as f32)
            .collect(),
        48000,
        1,
    )
    .unwrap()
}
fn band_amplitude(audio: &Audio, start: usize, end: usize, frequency: f64) -> f64 {
    let (mut a, mut b) = (0., 0.);
    for i in start..end {
        let phase = i as f64 / audio.sample_rate as f64 * frequency * TAU;
        let sample = audio.samples[i * audio.channels] as f64;
        a += sample * phase.cos();
        b += sample * phase.sin();
    }
    2. * a.hypot(b) / (end - start) as f64
}

#[test]
fn stem_controls_are_independent_and_revisions_do_not_accumulate() {
    let vocal = Arc::new(tone(1., 750., 0.1));
    let backing = Arc::new(tone(1., 3000., 0.05));
    let stems = StemSet::new(vec![
        Stem {
            kind: StemKind::Vocals,
            audio: vocal.clone(),
        },
        Stem {
            kind: StemKind::Instrumental,
            audio: backing,
        },
    ])
    .unwrap();
    let mut controls = stems.defaults();
    controls.protect_peaks = false;
    controls.tracks[0].gain_db = -6.;
    let mix = stem_mix::render(&stems, &controls, &|| false).unwrap();
    assert!(
        (band_amplitude(&mix, 0, mix.frames(), 750.) - 0.1 * 10_f64.powf(-6. / 20.)).abs() < 1e-6
    );
    assert!((band_amplitude(&mix, 0, mix.frames(), 3000.) - 0.05).abs() < 1e-6);
    assert_eq!(
        mix.samples,
        stem_mix::render(&stems, &controls, &|| false)
            .unwrap()
            .samples
    );
    controls.tracks[0].solo = true;
    controls.tracks[0].gain_db = 0.;
    assert_eq!(
        stem_mix::render(&stems, &controls, &|| false)
            .unwrap()
            .samples,
        vocal.samples
    );
    controls.tracks[0].mute = true;
    assert!(
        stem_mix::render(&stems, &controls, &|| false)
            .unwrap()
            .samples
            .iter()
            .all(|v| *v == 0.)
    );
    assert!(stem_mix::render(&stems, &controls, &|| true).is_err());
}

#[test]
fn neutral_consistent_stems_reconstruct_original_and_reject_misalignment() {
    let original = tone(0.5, 750., 0.2);
    let estimates = [
        StemKind::Vocals,
        StemKind::Drums,
        StemKind::Bass,
        StemKind::Other,
    ]
    .into_iter()
    .enumerate()
    .map(|(i, kind)| Stem {
        kind,
        audio: Arc::new(tone(0.5, 750., 0.01 * (i + 1) as f64)),
    })
    .collect::<Vec<_>>();
    for two in [false, true] {
        let stems = stem_mix::consistent(&original, estimates.clone(), two).unwrap();
        let mut settings = stems.defaults();
        settings.protect_peaks = false;
        let output = stem_mix::render(&stems, &settings, &|| false).unwrap();
        assert_eq!(output.frames(), original.frames());
        assert!(
            output
                .samples
                .iter()
                .zip(&original.samples)
                .all(|(a, b)| (a - b).abs() < 1e-7)
        );
    }
    assert!(
        StemSet::new(vec![
            estimates[0].clone(),
            Stem {
                kind: StemKind::Other,
                audio: Arc::new(tone(0.7, 750., 0.1))
            }
        ])
        .is_err()
    );
}

#[test]
fn spectral_selection_attenuates_its_band_and_preserves_unselected_samples() {
    let mut input = tone(2., 1500., 0.1);
    let high = tone(2., 6000., 0.1);
    for (a, b) in input.samples.iter_mut().zip(high.samples) {
        *a += b;
    }
    let selection = RepairSelection {
        start_seconds: 0.5,
        end_seconds: 1.5,
        low_hz: 5500.,
        high_hz: 6500.,
        reduction_db: -12.,
        mode: RepairMode::Attenuate,
    };
    let output = spectral_repair::render(&input, &selection, &|| false).unwrap();
    assert_eq!(&output.samples[..24000], &input.samples[..24000]);
    assert_eq!(&output.samples[72000..], &input.samples[72000..]);
    assert!((band_amplitude(&output, 36000, 60000, 1500.) - 0.1).abs() < 0.002);
    assert!((band_amplitude(&output, 36000, 60000, 6000.) - 0.02512).abs() < 0.003);
    let mut zero = selection.clone();
    zero.reduction_db = 0.;
    assert_eq!(
        spectral_repair::render(&input, &zero, &|| false)
            .unwrap()
            .samples,
        input.samples
    );
    assert!(spectral_repair::render(&input, &selection, &|| true).is_err());
}

#[test]
fn heal_reduces_a_localized_burst_without_editing_the_rest_of_the_track() {
    let original = tone(1., 1500., 0.1);
    let mut noisy = original.clone();
    for (i, sample) in noisy.samples.iter_mut().enumerate().take(25000).skip(22000) {
        *sample += (0.4 * (i as f64 / 48000. * 6000. * TAU).sin()) as f32;
    }
    let selection = RepairSelection {
        start_seconds: 0.42,
        end_seconds: 0.58,
        low_hz: 5500.,
        high_hz: 6500.,
        reduction_db: -30.,
        mode: RepairMode::Heal,
    };
    let cleaned = spectral_repair::render(&noisy, &selection, &|| false).unwrap();
    assert!(band_amplitude(&cleaned, 22500, 24500, 6000.) < 0.08);
    assert_eq!(&cleaned.samples[..19000], &noisy.samples[..19000]);
    assert_eq!(&cleaned.samples[29000..], &noisy.samples[29000..]);
}

#[test]
fn learned_voice_noise_is_reduced_with_finite_aligned_output() {
    let mut input = tone(1., 750., 0.1);
    let mut random = 1234567_u32;
    for (i, sample) in input.samples.iter_mut().enumerate() {
        if i < 12000 {
            *sample = 0.;
        }
        random ^= random << 13;
        random ^= random >> 17;
        random ^= random << 5;
        *sample += 0.01 * (random as f64 / u32::MAX as f64 * 2. - 1.) as f32;
    }
    let settings = VoiceSettings {
        noise_reduction: 0.8,
        noise_region: Some(NoiseRegion {
            start_seconds: 0.,
            end_seconds: 0.2,
        }),
        highpass_hz: 0.,
        deess: 0.,
        hold_loudness: false,
        protect_peaks: false,
        ..Default::default()
    };
    let result = voice_cleanup::render(&input, &settings, &|| false).unwrap();
    let rms = |a: &Audio| {
        a.samples[3000..9000]
            .iter()
            .map(|v| (*v as f64).powi(2))
            .sum::<f64>()
            .sqrt()
    };
    assert!(rms(&result.audio) < rms(&input) * 0.85);
    assert_eq!(result.audio.frames(), input.frames());
    assert!(result.audio.samples.iter().all(|v| v.is_finite()));
    assert!(voice_cleanup::render(&input, &settings, &|| true).is_err());
}

#[test]
fn silent_short_and_tonal_voice_inputs_do_not_generate_artifacts() {
    for audio in [
        Audio::new(vec![0.; 300], 48000, 1).unwrap(),
        tone(1., 750., 0.1),
    ] {
        let result = voice_cleanup::render(&audio, &VoiceSettings::default(), &|| false).unwrap();
        assert_eq!(result.audio.frames(), audio.frames());
        assert!(result.audio.samples.iter().all(|v| v.is_finite()));
        if audio.samples.iter().all(|v| *v == 0.) {
            assert!(result.audio.samples.iter().all(|v| *v == 0.));
        }
    }
}

#[test]
fn spectrogram_is_bounded_and_places_a_known_tone_at_the_correct_frequency() {
    let input = tone(1., 1500., 0.1);
    let image = spectrogram::build(&input, 64, 96, &|| false).unwrap();
    assert_eq!(image.levels_dbfs.len(), 64 * 96);
    let peak = (0..96)
        .max_by(|&a, &b| image.levels_dbfs[32 * 96 + a].total_cmp(&image.levels_dbfs[32 * 96 + b]))
        .unwrap();
    assert!((image.frequencies_hz[peak] / 1500.).log2().abs() < 0.12);
    assert!(image.levels_dbfs.iter().all(|v| v.is_finite()));
}

#[test]
fn full_band_full_file_repair_covers_dc_nyquist_and_short_boundaries() {
    let input = Audio::new(
        (0..127)
            .map(|i| 0.125 + if i % 2 == 0 { 0.1 } else { -0.1 })
            .collect(),
        48000,
        1,
    )
    .unwrap();
    let selection = RepairSelection {
        start_seconds: 0.,
        end_seconds: input.duration(),
        low_hz: 0.,
        high_hz: 24000.,
        reduction_db: -6.,
        mode: RepairMode::Attenuate,
    };
    let output = spectral_repair::render(&input, &selection, &|| false).unwrap();
    let gain = 10_f32.powf(-6. / 20.);
    assert!(
        input
            .samples
            .iter()
            .zip(&output.samples)
            .all(|(&a, &b)| (a * gain - b).abs() < 1e-6)
    );
}
