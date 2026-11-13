use super::{planning, tonal_correction};
use crate::clock::Instant;
use crate::{
    analysis::Analysis,
    audio::Audio,
    config::Settings,
    dsp::{
        biquad, convolution, dynamics,
        limiter::{self, LimiterConfig, LimiterInfo},
        low_end, repair, stereo, tone,
    },
    meters,
    profiles::number,
    report::StageLog,
};
use anyhow::{Result, ensure};
use serde_json::{Value, json};

pub struct ChainContext<'a> {
    pub settings: &'a Settings,
    pub profile: &'a Value,
    pub input_analysis: &'a Analysis,
    pub reference: Option<&'a Analysis>,
    pub hits: &'a [usize],
    pub protect: &'a [f32],
    pub target: f64,
    pub intensity: f64,
    pub cancel: &'a dyn Fn() -> bool,
    pub progress: &'a dyn Fn(f32, &str),
}
pub fn run(input: &Audio, c: &ChainContext<'_>) -> Result<(Audio, Vec<StageLog>, LimiterInfo)> {
    let (s, p, rep, i) = (c.settings, c.profile, c.input_analysis, c.intensity);
    let mut y = input.clone();
    let mut log = Vec::new();
    macro_rules! stage {
        ($name:expr,$body:expr) => {{
            ensure!(!(c.cancel)(), "Cancelled");
            (c.progress)(log.len() as f32 / 18., $name);
            let start = Instant::now();
            let detail: Value = $body;
            ensure!(
                y.samples.iter().all(|x| x.is_finite()),
                "Non-finite audio after {}",
                $name
            );
            log.push(StageLog {
                module: $name.into(),
                seconds: start.elapsed().as_secs_f64(),
                detail,
            });
        }};
    }
    if s.enabled("headroom") {
        stage!("headroom", {
            y.gain(s.number("headroom", "gain_db", -3.));
            json!({})
        });
    }
    if s.enabled("declip") && rep.metrics.clipped_samples > 0 {
        stage!("declip", {
            json!({"repaired_runs":repair::declip(&mut y,input,s.number("declip","max_run",32.) as usize)})
        });
    }
    if s.enabled("cleanup") {
        stage!("cleanup", {
            repair::cleanup(
                &mut y,
                s.number("cleanup", "hpf_hz", 22.),
                rep.metrics.correlation < -0.7,
            );
            json!({})
        });
    }
    if s.enabled("denoise") && rep.spectrum.noise_candidate {
        stage!("denoise", {
            repair::denoise(
                &mut y,
                s.number("denoise", "amount", 1.) * 0.4 * i,
                c.protect,
            );
            json!({"max_bin_reduction_db":2.0})
        });
    }
    if s.enabled("hf_rebuild")
        && let Some(cutoff) = rep.spectrum.cutoff_hz.filter(|&f| f < 16500.)
    {
        stage!("hf_rebuild", {
            let source = convolution::band(&y, cutoff * 0.4, cutoff * 0.7);
            let mut wet = super::super::dsp::oversampling::nonlinear(&source, 2, |x| {
                (x * 4.).tanh() - x * 4.
            });
            biquad::highpass(&mut wet, cutoff * 0.9);
            wet.gain(s.number("hf_rebuild", "mix_db", -23.));
            for (v, w) in y.samples.iter_mut().zip(wet.samples) {
                *v += w;
            }
            json!({"synthetic_harmonics":true})
        });
    }
    let reference = if s.enabled("reference_match") {
        c.reference
    } else {
        None
    };
    let filters = tonal_correction::corrective_eq(rep, s, i, reference);
    if !filters.is_empty() {
        stage!(
            if reference.is_some() {
                "reference_match"
            } else {
                "static_eq"
            },
            {
                for band in &filters {
                    biquad::from_band(band, y.sample_rate).apply(&mut y);
                }
                json!({"filters":filters})
            }
        );
    }
    let notches = planning::resonance_eq(rep, s, i);
    if !notches.is_empty() {
        stage!("resonance", {
            for band in &notches {
                biquad::from_band(band, y.sample_rate).apply(&mut y);
            }
            json!({"filters":notches})
        });
    }
    if s.enabled("low_end")
        && reference.is_none()
        && rep.deviation_db[1] < -3.
        && rep.deviation_db[2] < -3.
    {
        stage!("low_end", {
            tone::psycho_bass(&mut y, s.number("low_end", "psycho_bass_db", -17.));
            json!({"harmonics":true})
        });
    }
    for cfg in planning::dynamic_bands(rep, s, p, i, &filters) {
        stage!(
            if cfg.relative {
                "deesser"
            } else {
                "dynamic_eq"
            },
            {
                json!({"band":cfg,"gain_reduction_db":dynamics::dynamic_band(&mut y,&cfg,c.protect)})
            }
        );
    }
    if let Some((crossovers, configs)) = planning::multiband(s, p, i) {
        stage!("multiband", {
            json!({"reduction_db":dynamics::multiband(&mut y,&crossovers,&configs)})
        });
    }
    if s.enabled("glue") {
        stage!("glue", {
            let mut cfg = planning::glue(s, p, i);
            cfg.threshold_db = dynamics::auto_threshold(&y, cfg.ratio, cfg.max_gr_db * 0.5);
            let gr = dynamics::compress(&mut y, &cfg);
            json!({"config":cfg,"gain_reduction_db":gr})
        });
    }
    if s.enabled("saturation") {
        stage!("saturation", {
            tone::saturate(
                &mut y,
                s.number(
                    "saturation",
                    "drive_db",
                    number(p, &["saturation", "drive_db"], 1.),
                ),
                s.number("saturation", "mix", number(p, &["saturation", "mix"], 0.1)) * i.min(1.),
                c.protect,
            );
            json!({"oversampling":2})
        });
    }
    if s.enabled("low_punch") && i > 0. {
        stage!("low_punch", {
            let detail = low_end::shape(
                &mut y,
                (s.number("low_punch", "attack_db", 2.) * i).min(number(
                    p,
                    &["punch", "low_punch_attack_max_db"],
                    3.,
                )),
                s.number("low_punch", "sustain_db", 1.5) * i,
            );
            low_end::duck(&mut y, c.hits, s.number("low_punch", "sub_duck_db", 2.) * i);
            detail
        });
    }
    if s.enabled("stereo") && input.channels == 2 {
        stage!("stereo", {
            let corr = rep.metrics.correlation;
            let upper = number(p, &["stereo", "corr_window", "1"], 0.9);
            let mut side_db = if corr < 0.7 {
                -2. * i.min(1.)
            } else if corr > upper {
                number(p, &["stereo", "max_side_gain_db"], 2.) * i.min(1.)
            } else {
                0.
            };
            if s.match_width
                && let Some(r) = reference
            {
                side_db = ((corr - r.metrics.correlation) * 6.).clamp(-3., 3.) * s.match_strength;
            }
            stereo::process(
                &mut y,
                number(p, &["stereo", "side_hpf_hz"], 100.),
                number(p, &["stereo", "mono_amount"], 1.),
                side_db,
            );
            json!({"side_gain_db":side_db})
        });
    }
    let clip = if s.enabled("clipper") {
        s.number(
            "clipper",
            "drive_db",
            number(p, &["clipper_db", &s.mode], 0.),
        ) * i.min(1.)
    } else {
        0.
    };
    if clip > 0.05 {
        stage!("clipper", {
            let gain = (c.target - meters::integrated(&y)?).clamp(-40., s.max_makeup_db);
            y.gain(gain);
            tone::soft_clip(&mut y, clip, meters::lin(s.ceiling_dbtp - 0.2 + clip / 2.));
            y.gain(-gain);
            json!({"drive_db":clip})
        });
    }
    let max_gr = if s.mode == "loud" {
        s.max_limiter_gr_db.min(number(
            p,
            &["punch", "loud_max_limiter_gr_db"],
            s.max_limiter_gr_db,
        ))
    } else {
        s.max_limiter_gr_db
    };
    let cfg = LimiterConfig {
        target: c.target,
        ceiling: s.ceiling_dbtp,
        max_gr,
        max_makeup: s.max_makeup_db,
        lookahead_ms: s.number("limiter", "lookahead_ms", 2.),
        release_ms: s.number("limiter", "release_ms", 30.),
        slow_ms: s.number("limiter", "release_slow_ms", 250.),
        enabled: s.enabled("limiter"),
    };
    let info;
    stage!("limiter", {
        let (out, lim) = limiter::limit(&y, &cfg, c.cancel)?;
        y = out;
        info = lim;
        json!(&info)
    });
    Ok((y, log, info))
}
