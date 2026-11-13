use super::worker::{Event, TrackResult, save_report};
use crate::{config::Settings, export, preview, profiles};
use anyhow::{Context, Result, ensure};
use eframe::egui;
use std::{
    path::PathBuf,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
        mpsc::Sender,
    },
};

pub fn settings_match(result: &TrackResult, settings: &Settings) -> bool {
    result
        .eq_parameters
        .is_some_and(|p| p.ceiling_dbtp == settings.ceiling_dbtp)
        && result.report.as_ref().is_some_and(|report| {
            report.configuration["manual_eq"]
                == serde_json::to_value(&settings.manual_eq).unwrap_or_default()
                && report.configuration["eq_output_db"].as_f64() == Some(settings.eq_output_db)
                && report.configuration["eq_hold_loudness"].as_bool()
                    == Some(settings.eq_hold_loudness)
        })
}
fn resolve(
    result: &TrackResult,
    settings: &Settings,
    cancel: &dyn Fn() -> bool,
) -> Result<TrackResult> {
    settings.validate()?;
    let raw = result.raw.as_ref().context("Master the track first")?;
    let mut next = result.clone();
    let profile_name = result
        .report
        .as_ref()
        .and_then(|r| r.configuration["profile"].as_str())
        .unwrap_or(&settings.profile);
    let profile = profiles::resolve(profile_name, &crate::paths::app_root().join("profiles"))?;
    let report = next.report.as_ref().context("Master the track first")?;
    let (audio, parameters, report) = preview::resolve_eq(raw, settings, &profile, report, cancel)?;
    next.report = Some(report);
    next.eq_parameters = Some(parameters);
    next.after = Some(Arc::new(audio));
    Ok(next)
}
pub fn render_revision(
    index: usize,
    revision: u64,
    result: Arc<TrackResult>,
    settings: Settings,
    tx: Sender<Event>,
    ctx: egui::Context,
) {
    std::thread::spawn(move || {
        let event = match resolve(&result, &settings, &|| false) {
            Ok(result) => Event::EqRendered(index, revision, Arc::new(result)),
            Err(error) => Event::EqRenderFailed(index, revision, format!("{error:#}")),
        };
        let _ = tx.send(event);
        ctx.request_repaint();
    });
}
pub fn export_selected(
    index: usize,
    file: PathBuf,
    result: Arc<TrackResult>,
    settings: Settings,
    tx: Sender<Event>,
    cancel: Arc<AtomicBool>,
    ctx: egui::Context,
) {
    std::thread::spawn(move || {
        let work = (|| -> Result<TrackResult> {
            settings.validate()?;
            ensure!(!cancel.load(Ordering::Relaxed), "Cancelled");
            let mut next = if settings_match(&result, &settings) {
                result.as_ref().clone()
            } else {
                resolve(&result, &settings, &|| cancel.load(Ordering::Relaxed))?
            };
            let after = next.after.as_ref().context("Master the track first")?;
            if let Some(report) = &mut next.report {
                report.exports =
                    export::export(after, &file, &settings, &|| cancel.load(Ordering::Relaxed))?;
                save_report(report)?;
            }
            Ok(next)
        })();
        let _ = tx.send(match work {
            Ok(result) => Event::Ready(index, Arc::new(result)),
            Err(error) => Event::Failed(index, format!("{error:#}")),
        });
        let _ = tx.send(Event::Finished);
        ctx.request_repaint();
    });
}
