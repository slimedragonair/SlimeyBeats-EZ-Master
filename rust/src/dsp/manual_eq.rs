use crate::{
    audio::Audio,
    config::Settings,
    dsp::{biquad, eq_chain::EqParameters, limiter, realtime_eq::RealtimeEq},
    meters,
};
use anyhow::{Result, ensure};

pub struct EqRender {
    pub audio: Audio,
    pub parameters: EqParameters,
    pub hold_gain_db: f64,
    pub trim_db: f64,
    pub converged: bool,
}

const HOLD_TOLERANCE_LU: f64 = 0.05;

fn compensation_converged(input: &Audio, output: &Audio, settings: &Settings) -> Result<bool> {
    if !settings.eq_hold_loudness {
        return Ok(true);
    }
    let before = meters::integrated(input)?;
    Ok(before <= -69.5
        || (meters::integrated(output)? - before - settings.eq_output_db).abs()
            <= HOLD_TOLERANCE_LU)
}

pub fn render_fixed(
    input: &Audio,
    parameters: EqParameters,
    cancel: &dyn Fn() -> bool,
) -> Result<Audio> {
    let mut processor = RealtimeEq::new(input.sample_rate, input.channels, parameters);
    let delay = processor.delay();
    let mut output = vec![0_f32; input.samples.len()];
    for frame in 0..input.frames() + delay {
        if frame.is_multiple_of(4096) {
            ensure!(!cancel(), "Cancelled");
        }
        let mut source = [0.; 2];
        if frame < input.frames() {
            for (ch, value) in source.iter_mut().enumerate().take(input.channels) {
                *value = input.samples[frame * input.channels + ch] as f64;
            }
        }
        let result = processor.process(source);
        ensure!(
            result.iter().all(|v| v.is_finite()),
            "Non-finite streaming EQ output"
        );
        if frame >= delay {
            let index = (frame - delay) * input.channels;
            for ch in 0..input.channels {
                output[index + ch] = result[ch] as f32;
            }
        }
    }
    Ok(Audio {
        samples: output,
        sample_rate: input.sample_rate,
        channels: input.channels,
    })
}

pub fn render(input: &Audio, settings: &Settings, cancel: &dyn Fn() -> bool) -> Result<EqRender> {
    settings.validate()?;
    let active = settings
        .manual_eq
        .iter()
        .any(|b| b.on && (b.gain.abs() > 0.001 || b.kind.ends_with("cut")));
    if !active && settings.eq_output_db.abs() < 0.001 {
        let mut audio = input.clone();
        let trim = limiter::guard(&mut audio, settings.ceiling_dbtp)?;
        let mut parameters = EqParameters::new(settings, input.sample_rate, 0.);
        parameters.post_gain_db = trim;
        let converged = compensation_converged(input, &audio, settings)?;
        return Ok(EqRender {
            audio,
            parameters,
            hold_gain_db: 0.,
            trim_db: trim,
            converged,
        });
    }
    let mut hold = 0.;
    let mut zero_fader = settings.clone();
    zero_fader.eq_output_db = 0.;
    let mut held = None;
    if settings.eq_hold_loudness {
        let target = meters::integrated(input)?;
        if target > -69.5 {
            let mut filtered = input.clone();
            for band in &settings.manual_eq {
                if band.on {
                    biquad::from_band(band, input.sample_rate).apply(&mut filtered);
                }
            }
            hold = (target - meters::integrated(&filtered)?).clamp(-18., 18.);
            drop(filtered);
            for _ in 0..4 {
                let candidate = render_fixed(
                    input,
                    EqParameters::new(&zero_fader, input.sample_rate, hold),
                    cancel,
                )?;
                let error = target - meters::integrated(&candidate)?;
                if error.abs() <= HOLD_TOLERANCE_LU {
                    held = Some(candidate);
                    break;
                }
                let next = (hold + error).clamp(-18., 18.);
                if (next - hold).abs() < 0.001 {
                    held = Some(candidate);
                    break;
                }
                hold = next;
            }
        }
    }
    let mut parameters = EqParameters::new(settings, input.sample_rate, hold);
    let mut output = if settings.eq_output_db.abs() < 1e-9 {
        if let Some(audio) = held {
            audio
        } else {
            render_fixed(input, parameters, cancel)?
        }
    } else {
        render_fixed(input, parameters, cancel)?
    };
    let trim = limiter::guard(&mut output, settings.ceiling_dbtp)?;
    parameters.post_gain_db = trim;
    let converged = compensation_converged(input, &output, settings)?;
    Ok(EqRender {
        audio: output,
        parameters,
        hold_gain_db: hold,
        trim_db: trim,
        converged,
    })
}

pub fn apply(audio: &mut Audio, settings: &Settings) -> Result<()> {
    if !settings
        .manual_eq
        .iter()
        .any(|b| b.on && (b.gain.abs() > 0.001 || b.kind.ends_with("cut")))
        && settings.eq_output_db.abs() < 0.001
    {
        return Ok(());
    }
    *audio = render(audio, settings, &|| false)?.audio;
    Ok(())
}
