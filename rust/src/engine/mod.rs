mod chain;
mod planning;
mod tonal_correction;
use crate::clock::Instant;
use crate::{
    analysis::{self, Analysis, transients},
    audio::Audio,
    config::Settings,
    dsp::{limiter, manual_eq},
    profiles,
    report::Report,
};
use anyhow::{Result, ensure};
use serde_json::Value;

pub struct Mastered {
    pub audio: Audio,
    pub report: Report,
}
pub fn master(
    input: &Audio,
    settings: &Settings,
    profile: &Value,
    reference: Option<&Analysis>,
    file: &str,
    progress: &dyn Fn(f32, &str),
    cancel: &dyn Fn() -> bool,
) -> Result<Mastered> {
    settings.validate()?;
    ensure!(!cancel(), "Cancelled");
    let start = Instant::now();
    progress(0.02, "Analyzing input");
    let input_analysis = analysis::analyze(input, profile)?;
    let requested = settings
        .target_lufs
        .unwrap_or_else(|| profiles::number(profile, &["lufs", &settings.mode], -14.));
    let mut target = if settings.match_loudness {
        reference
            .map(|r| r.metrics.integrated_lufs)
            .unwrap_or(requested)
    } else {
        requested
    };
    target = target.min(input_analysis.metrics.integrated_lufs + settings.max_makeup_db);
    let mut notes=vec!["Native Rust processing uses revised detectors and DSP; it is not a bit-identical Python port. Genre targets are heuristics, not a listening verdict.".into()];
    if (target - requested).abs() > 0.1 {
        notes.push(format!("Requested {requested:.1} LUFS; effective target {target:.1} LUFS after reference/gain limits."));
    }
    if input.frames() < 1024 || input_analysis.metrics.integrated_lufs <= -69.5 {
        let mut audio = input.clone();
        limiter::guard(&mut audio, settings.ceiling_dbtp)?;
        notes.push("Silent or very short input: correction and makeup gain skipped; peak ceiling still enforced.".into());
        let out = analysis::analyze(&audio, profile)?;
        return Ok(Mastered {
            audio,
            report: Report {
                configuration: serde_json::to_value(settings)?,
                version: env!("CARGO_PKG_VERSION").into(),
                file: file.into(),
                profile: profile["display_name"].as_str().unwrap_or("Custom").into(),
                input: input_analysis,
                output: out,
                target_lufs: target,
                ceiling_dbtp: settings.ceiling_dbtp,
                limiter: Default::default(),
                log: vec![],
                processing_seconds: start.elapsed().as_secs_f64(),
                punch_loss_db: 0.,
                safety_attempts: 0,
                notes,
                exports: vec![],
            },
        });
    }
    let hits = transients::detect(input);
    let protection = transients::protect(input.frames(), input.sample_rate, &hits);
    let punch_in = if settings.safety {
        transients::punch(input, &hits)
    } else {
        0.
    };
    let mut best = None;
    let tries = if settings.safety { 3 } else { 1 };
    let mut actual_tries = 0;
    for attempt in 0..tries {
        actual_tries += 1;
        let scale = 0.6_f64.powi(attempt);
        let cb =
            |p: f32, msg: &str| progress(0.18 + 0.65 * (attempt as f32 + p) / tries as f32, msg);
        let context = chain::ChainContext {
            settings,
            profile,
            input_analysis: &input_analysis,
            reference,
            hits: &hits,
            protect: &protection,
            target,
            intensity: settings.intensity * scale,
            cancel,
            progress: &cb,
        };
        let (mut audio, mut log, info) = chain::run(input, &context)?;
        ensure!(!cancel(), "Cancelled");
        let eq_start = Instant::now();
        manual_eq::apply(&mut audio, settings)?;
        if settings
            .manual_eq
            .iter()
            .any(|b| b.on && b.gain.abs() > 0.001)
            || settings.eq_output_db.abs() > 0.001
        {
            log.push(crate::report::StageLog {
                module: "manual_eq".into(),
                seconds: eq_start.elapsed().as_secs_f64(),
                detail: serde_json::json!({"hold_loudness":settings.eq_hold_loudness}),
            });
        }
        let out = analysis::analyze(&audio, profile)?;
        ensure!(
            out.metrics.true_peak_dbtp <= settings.ceiling_dbtp + 0.01,
            "True peak verification failed"
        );
        let loss = if settings.safety {
            (punch_in - transients::punch(&audio, &hits)).max(0.)
        } else {
            0.
        };
        let severity_loss = ["harshness", "fizz"]
            .iter()
            .map(|k| (out.severity(k) - input_analysis.severity(k) - 15.).max(0.))
            .sum::<f64>();
        let penalty = (loss - 2.).max(0.) * 10. + severity_loss;
        let report = Report {
            configuration: serde_json::to_value(settings)?,
            version: env!("CARGO_PKG_VERSION").into(),
            file: file.into(),
            profile: profile["display_name"].as_str().unwrap_or("Custom").into(),
            input: input_analysis.clone(),
            output: out,
            target_lufs: target,
            ceiling_dbtp: settings.ceiling_dbtp,
            limiter: info,
            log,
            processing_seconds: 0.,
            punch_loss_db: loss,
            safety_attempts: actual_tries,
            notes: vec![],
            exports: vec![],
        };
        if best
            .as_ref()
            .is_none_or(|(_, _, score): &(Audio, Report, f64)| penalty < *score)
        {
            best = Some((audio, report, penalty));
        }
        if penalty <= 0.01 {
            break;
        }
    }
    let (audio, mut report, penalty) = best.unwrap();
    if actual_tries > 1 {
        notes.push(format!("Safety evaluated {actual_tries} intensity levels and retained the lowest measured penalty."));
    }
    if penalty > 0.01 {
        notes.push(format!("Safety could not eliminate the measured regression (punch loss {:.1} dB). Audition before accepting this master.",report.punch_loss_db));
    }
    if !report.limiter.reachable {
        notes.push(format!(
            "Target not reached within gain and limiter limits. Result: {:.2} LUFS.",
            report.output.metrics.integrated_lufs
        ));
    }
    report.safety_attempts = actual_tries;
    report.notes = notes;
    report.processing_seconds = start.elapsed().as_secs_f64();
    progress(1., "Master complete");
    Ok(Mastered { audio, report })
}
