use crate::{
    analysis::Analysis,
    config::{EqBand, Settings},
    dsp::biquad,
    meters::percentile,
    profiles::BANDS,
};

const DEFAULT_BAND_LIMIT_DB: f64 = 2.5;
const DEFAULT_BOOST_LIMIT_DB: f64 = 1.;
const DEFAULT_RESPONSE_LIMIT_DB: f64 = 2.5;
pub(super) const DEFAULT_TOTAL_CUT_DB: f64 = 3.;
const RESPONSE_POINTS: usize = 256;

pub fn response(filters: &[EqBand], rate: u32, frequency: f64) -> f64 {
    filters
        .iter()
        .map(|band| biquad::from_band(band, rate).response_db(frequency, rate))
        .sum()
}

pub fn corrective_eq(
    measurement: &Analysis,
    settings: &Settings,
    intensity: f64,
    reference: Option<&Analysis>,
) -> Vec<EqBand> {
    if intensity <= 0. || (!settings.enabled("static_eq") && reference.is_none()) {
        return Vec::new();
    }
    let matching = reference.is_some();
    let fraction = if matching {
        settings.match_strength
    } else {
        settings.number("static_eq", "fraction", 0.6)
    };
    let band_limit = if matching {
        settings.number("reference_match", "max_db", 6.)
    } else {
        settings.number("static_eq", "max_db", DEFAULT_BAND_LIMIT_DB)
    }
    .clamp(0., 18.);
    let boost_limit = if matching {
        band_limit
    } else {
        settings
            .number("static_eq", "max_boost_db", DEFAULT_BOOST_LIMIT_DB)
            .clamp(0., band_limit)
    };
    let response_limit = if matching {
        band_limit
    } else {
        settings
            .number("static_eq", "response_max_db", DEFAULT_RESPONSE_LIMIT_DB)
            .clamp(0., 18.)
    };
    let deviations = if let Some(reference) = reference {
        let difference: Vec<_> = measurement
            .measured_db
            .iter()
            .zip(&reference.measured_db)
            .map(|(a, b)| a - b)
            .collect();
        let core: Vec<_> = (2..9)
            .filter(|&i| measurement.valid[i] && reference.valid[i])
            .map(|i| difference[i])
            .collect();
        if core.len() < 3 {
            return Vec::new();
        }
        let offset = percentile(&core, 0.5);
        difference
            .iter()
            .map(|value| value - offset)
            .collect::<Vec<_>>()
    } else {
        measurement.deviation_db.clone()
    };
    let mut filters = Vec::new();
    for (i, &frequency) in BANDS.iter().enumerate() {
        if !measurement.valid[i]
            || reference.is_some_and(|value| !value.valid[i])
            || (!matching && deviations[i].abs() < settings.number("static_eq", "tolerance_db", 2.))
        {
            continue;
        }
        // Cap the full-strength proposal first: large deviations must not pin a low-intensity edit at its cap.
        let gain = (-deviations[i] * fraction).clamp(-band_limit, boost_limit);
        if (gain > 0.
            && (i == 0 || frequency >= measurement.spectrum.cutoff_hz.unwrap_or(f64::MAX) * 0.85))
            || gain.abs() < 0.2
        {
            continue;
        }
        filters.push(EqBand {
            kind: if i < 2 {
                "low_shelf"
            } else if i >= 8 {
                "high_shelf"
            } else {
                "bell"
            }
            .into(),
            freq: frequency,
            gain,
            q: 0.9,
            on: true,
        });
    }
    // Individual caps do not constrain a cascade of overlapping bells and shelves.
    let high = (measurement.sample_rate as f64 * 0.48).min(20000.);
    for _ in 0..8 {
        let worst = (0..RESPONSE_POINTS)
            .map(|i| 20. * (high / 20.).powf(i as f64 / (RESPONSE_POINTS - 1) as f64))
            .map(|frequency| response(&filters, measurement.sample_rate, frequency).abs())
            .fold(0., f64::max);
        if worst <= response_limit + 1e-5 {
            break;
        }
        let scale = response_limit / worst;
        for filter in &mut filters {
            filter.gain *= scale;
        }
    }
    for filter in &mut filters {
        filter.gain *= intensity;
    }
    for _ in 0..8 {
        let too_deep =
            [(100., -1.8), (125., -2.), (160., -2.6)]
                .into_iter()
                .any(|(frequency, floor)| {
                    response(&filters, measurement.sample_rate, frequency) < floor
                });
        if !too_deep {
            break;
        }
        for filter in &mut filters {
            if filter.freq < 500. && filter.gain < 0. {
                filter.gain *= 0.8;
            }
        }
    }
    filters
}

pub fn remaining_cut(filters: &[EqBand], rate: u32, lo: f64, hi: f64, total: f64) -> f64 {
    let worst = (0..24)
        .map(|i| lo * (hi / lo).powf(i as f64 / 23.))
        .map(|frequency| (-response(filters, rate, frequency)).max(0.))
        .fold(0., f64::max);
    (total - worst).max(0.)
}

#[cfg(test)]
#[path = "tonal_correction_tests.rs"]
mod tests;
