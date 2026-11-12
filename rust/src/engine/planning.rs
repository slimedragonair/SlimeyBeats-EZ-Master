use crate::{
    analysis::Analysis,
    config::{EqBand, Settings},
    dsp::dynamics::{Compression, DynamicBand},
    profiles::number,
};
use serde_json::Value;

pub fn resonance_eq(rep: &Analysis, s: &Settings, intensity: f64) -> Vec<EqBand> {
    if !s.enabled("resonance") {
        return Vec::new();
    }
    let mut peaks = Vec::new();
    for i in 3..rep.spectrum.freqs.len().saturating_sub(3) {
        let f = rep.spectrum.freqs[i];
        if !(2000.0..=8000.).contains(&f) {
            continue;
        }
        let db = &rep.spectrum.db;
        let prominence = db[i] - (db[i - 3] + db[i + 3]) * 0.5;
        if prominence > 4. && db[i] > db[i - 1] && db[i] > db[i + 1] {
            peaks.push((
                prominence,
                EqBand {
                    kind: "bell".into(),
                    freq: f,
                    gain: -prominence.min(4.) * intensity,
                    q: 5.,
                    on: true,
                },
            ));
        }
    }
    peaks.sort_by(|a, b| b.0.total_cmp(&a.0));
    peaks
        .into_iter()
        .take(s.number("resonance", "max_notches", 4.) as usize)
        .map(|(_, b)| b)
        .collect()
}
pub fn dynamic_bands(
    rep: &Analysis,
    s: &Settings,
    p: &Value,
    intensity: f64,
    static_filters: &[EqBand],
) -> Vec<DynamicBand> {
    let mut out = Vec::new();
    let mut push = |lo, hi, max_cut: f64, ratio, attack, release, over, relative| {
        let available = super::tonal_correction::remaining_cut(
            static_filters,
            rep.sample_rate,
            lo,
            hi,
            s.number(
                "dynamic_eq",
                "total_cut_max_db",
                super::tonal_correction::DEFAULT_TOTAL_CUT_DB,
            ) * intensity,
        );
        let cut = if relative {
            max_cut * intensity
        } else {
            (max_cut * intensity).min(available)
        };
        if cut <= 0.001 {
            return;
        }
        out.push(DynamicBand {
            lo,
            hi,
            max_cut: cut,
            ratio,
            attack,
            release,
            over,
            relative,
        })
    };
    if s.enabled("dynamic_eq") {
        if rep.severity("mud") >= 30. {
            push(
                180.,
                500.,
                s.number("dynamic_eq", "mud_max_db", 4.),
                2.,
                10.,
                120.,
                2.,
                false,
            );
        }
        if rep.severity("harshness") >= 30. {
            push(
                2000.,
                5000.,
                s.number("dynamic_eq", "harsh_max_db", 3.).min(number(
                    p,
                    &["punch", "harsh_max_db"],
                    3.,
                )),
                3.,
                6.,
                80.,
                2.,
                false,
            );
        }
        if rep.severity("fizz") >= 30. {
            push(
                8000.,
                14000.,
                number(p, &["punch", "fizz_max_db"], 2.),
                3.,
                6.,
                80.,
                3.,
                false,
            );
        }
    }
    if s.enabled("deesser") && rep.severity("sibilance") >= 30. {
        push(
            s.number("deesser", "lo_hz", 5000.),
            s.number("deesser", "hi_hz", 10000.),
            s.number("deesser", "max_db", 6.)
                .min(number(p, &["punch", "deesser_max_db"], 4.)),
            4.,
            2.,
            50.,
            6.,
            true,
        );
    }
    out
}
pub fn glue(s: &Settings, p: &Value, intensity: f64) -> Compression {
    Compression {
        threshold_db: 0.,
        ratio: s.number("glue", "ratio", number(p, &["glue", "ratio"], 1.5)),
        attack_ms: s.number("glue", "attack_ms", number(p, &["glue", "attack_ms"], 30.)),
        release_ms: s.number(
            "glue",
            "release_ms",
            number(p, &["glue", "release_ms"], 150.),
        ),
        max_gr_db: (number(p, &["glue", "target_gr_db"], 1.) * intensity * 2.).min(4.),
        auto_release: true,
    }
}
pub fn multiband(s: &Settings, p: &Value, intensity: f64) -> Option<(Vec<f64>, Vec<Compression>)> {
    if !s.enabled("multiband") || !p["multiband"].is_object() {
        return None;
    }
    let mb = &p["multiband"];
    let crossovers: Vec<_> = mb["xover_hz"]
        .as_array()?
        .iter()
        .filter_map(Value::as_f64)
        .collect();
    let names = if crossovers.len() == 1 {
        vec!["low", "high"]
    } else {
        vec!["low", "mid", "high"]
    };
    let configs = names
        .iter()
        .map(|name| Compression {
            threshold_db: 0.,
            ratio: number(&mb[*name], &["ratio"], 1.),
            attack_ms: number(&mb[*name], &["attack_ms"], 30.),
            release_ms: number(&mb[*name], &["release_ms"], 150.),
            max_gr_db: number(&mb[*name], &["max_gr_db"], 0.) * intensity,
            auto_release: true,
        })
        .collect();
    Some((crossovers, configs))
}
