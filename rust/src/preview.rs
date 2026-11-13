//! Shared native preview/EQ preparation for the desktop UI and MCP companion.
use crate::{
    analysis::{self, Analysis},
    audio::Audio,
    clock::Instant,
    config::{Settings, default_eq},
    dsp::{eq_chain::EqParameters, manual_eq},
    engine,
    report::Report,
};
use anyhow::{Result, ensure};
use serde_json::Value;
use std::sync::Arc;

#[derive(Clone)]
pub struct Preview {
    pub raw: Arc<Audio>,
    pub raw_analysis: Arc<Analysis>,
    pub audio: Arc<Audio>,
    pub parameters: EqParameters,
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
) -> Result<Preview> {
    let mut automatic = settings.clone();
    automatic.manual_eq = default_eq();
    automatic.eq_output_db = 0.;
    let mut mastered = engine::master(
        input, &automatic, profile, reference, file, progress, cancel,
    )?;
    mastered.report.configuration = serde_json::to_value(settings)?;
    let raw = Arc::new(mastered.audio);
    let raw_analysis = Arc::new(mastered.report.output.clone());
    let (audio, parameters, report) =
        resolve_eq(&raw, settings, profile, &mastered.report, cancel)?;
    Ok(Preview {
        raw,
        raw_analysis,
        audio: Arc::new(audio),
        parameters,
        report,
    })
}

pub fn resolve_eq(
    raw: &Audio,
    settings: &Settings,
    profile: &Value,
    report: &Report,
    cancel: &dyn Fn() -> bool,
) -> Result<(Audio, EqParameters, Report)> {
    settings.validate()?;
    let start = Instant::now();
    let rendered = manual_eq::render(raw, settings, cancel)?;
    ensure!(!cancel(), "Cancelled");
    let mut report = report.clone();
    // EQ revisions must not attribute newly selected automatic settings to an
    // existing master that has not been re-mastered with those settings.
    report.configuration["manual_eq"] = serde_json::to_value(&settings.manual_eq)?;
    report.configuration["eq_output_db"] = settings.eq_output_db.into();
    report.configuration["eq_hold_loudness"] = settings.eq_hold_loudness.into();
    report.configuration["ceiling_dbtp"] = settings.ceiling_dbtp.into();
    report.configuration["eq_hold_gain_db"] = rendered.hold_gain_db.into();
    report.configuration["eq_final_trim_db"] = rendered.trim_db.into();
    report.ceiling_dbtp = settings.ceiling_dbtp;
    report.output = analysis::analyze(&rendered.audio, profile)?;
    ensure!(!cancel(), "Cancelled");
    report.exports.clear();
    report.processing_seconds += start.elapsed().as_secs_f64();
    report
        .notes
        .retain(|note| !note.starts_with("Manual EQ:") && !note.starts_with("Manual EQ preview"));
    report.notes.push(format!("Manual EQ: shared streaming processor; hold {:.2} dB, final peak trim {:.3} dB, loudness hold {}.", rendered.hold_gain_db, rendered.trim_db, if rendered.converged { "resolved" } else { "limited by the peak/gain constraints" }));
    Ok((rendered.audio, rendered.parameters, report))
}
