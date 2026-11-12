use crate::{
    analysis::{self, Analysis},
    audio::{self, Audio},
    config::{Settings, default_eq},
    dsp::manual_eq,
    engine, export, profiles,
    report::Report,
};
use anyhow::{Result, ensure};
use eframe::egui;
use std::{
    path::{Path, PathBuf},
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
        mpsc::Sender,
    },
};

#[derive(Clone)]
pub struct TrackResult {
    pub before: Arc<Audio>,
    pub raw: Option<Arc<Audio>>,
    pub raw_analysis: Option<Arc<Analysis>>,
    pub eq_parameters: Option<crate::dsp::eq_chain::EqParameters>,
    pub after: Option<Arc<Audio>>,
    pub analysis: Analysis,
    pub report: Option<Report>,
}
pub enum Event {
    Suggestion(usize, Vec<crate::config::EqBand>, String),
    SuggestionUnavailable(String),
    Progress(usize, f32, String),
    Ready(usize, Arc<TrackResult>),
    Failed(usize, String),
    Finished,
    EqRendered(usize, u64, Arc<TrackResult>),
    EqRenderFailed(usize, u64, String),
}

pub fn suggest_eq(
    index: usize,
    result: Arc<TrackResult>,
    settings: Settings,
    tx: Sender<Event>,
    cancel: Arc<AtomicBool>,
    ctx: egui::Context,
) {
    std::thread::spawn(move || {
        let work = (|| -> Result<_> {
            ensure!(!cancel.load(Ordering::Relaxed), "Cancelled");
            let p = profiles::resolve(
                &settings.profile,
                &crate::paths::app_root().join("profiles"),
            )?;
            let source = result.raw.as_deref().unwrap_or(&result.before);
            let measured = analysis::analyze(source, &p)?;
            let bands = crate::dsp::eq_suggestion::suggest(&measured, &p)?;
            ensure!(!cancel.load(Ordering::Relaxed), "Cancelled");
            Ok((
                bands,
                format!(
                    "Proposed from {} · {} starting point · 25% profile influence · limits ±2.5 dB per band (+1 dB bass/air boost) · apply to hear it live",
                    if result.raw.is_some() {
                        "automatic master"
                    } else {
                        "original"
                    },
                    p["display_name"].as_str().unwrap_or("Custom")
                ),
            ))
        })();
        let event = match work {
            Ok((bands, note)) => Event::Suggestion(index, bands, note),
            Err(e) => Event::SuggestionUnavailable(format!("{e:#}")),
        };
        let _ = tx.send(event);
        let _ = tx.send(Event::Finished);
        ctx.request_repaint();
    });
}
pub fn batch(
    files: Vec<(usize, PathBuf)>,
    settings: Settings,
    analyze_only: bool,
    tx: Sender<Event>,
    cancel: Arc<AtomicBool>,
    ctx: egui::Context,
    root: PathBuf,
) {
    std::thread::spawn(move || {
        let profile = profiles::resolve(&settings.profile, &root.join("profiles"));
        let reference: Result<_> = profile
            .as_ref()
            .map_err(|e| anyhow::anyhow!("{e:#}"))
            .and_then(|p| {
                settings
                    .reference
                    .as_ref()
                    .map(|f| analysis::analyze(&audio::load(f)?, p))
                    .transpose()
            });
        for (index, file) in files {
            if cancel.load(Ordering::Relaxed) {
                break;
            }
            let result = (|| -> Result<TrackResult> {
                let p = profile.as_ref().map_err(|e| anyhow::anyhow!("{e:#}"))?;
                let r = reference
                    .as_ref()
                    .map_err(|e| anyhow::anyhow!("Reference: {e:#}"))?;
                let before = Arc::new(audio::load(&file)?);
                if analyze_only {
                    return Ok(TrackResult {
                        analysis: analysis::analyze(&before, p)?,
                        before,
                        raw: None,
                        raw_analysis: None,
                        eq_parameters: None,
                        after: None,
                        report: None,
                    });
                }
                let mut auto_settings = settings.clone();
                auto_settings.manual_eq = default_eq();
                auto_settings.eq_output_db = 0.;
                let progress = |fraction, msg: &str| {
                    let _ = tx.send(Event::Progress(index, fraction, msg.into()));
                    ctx.request_repaint();
                };
                let mut mastered = engine::master(
                    &before,
                    &auto_settings,
                    p,
                    r.as_ref(),
                    &file.to_string_lossy(),
                    &progress,
                    &|| cancel.load(Ordering::Relaxed),
                )?;
                let raw = Arc::new(mastered.audio);
                let raw_analysis = Arc::new(mastered.report.output.clone());
                let eq_start = std::time::Instant::now();
                let eq_render =
                    manual_eq::render(&raw, &settings, &|| cancel.load(Ordering::Relaxed))?;
                let parameters = eq_render.parameters;
                let eq_hold = eq_render.hold_gain_db;
                let eq_trim = eq_render.trim_db;
                let after = eq_render.audio;
                mastered.report.output = analysis::analyze(&after, p)?;
                mastered.report.configuration = serde_json::to_value(&settings)?;
                mastered.report.configuration["eq_hold_gain_db"] = serde_json::json!(eq_hold);
                mastered.report.configuration["eq_final_trim_db"] = serde_json::json!(eq_trim);
                mastered.report.processing_seconds += eq_start.elapsed().as_secs_f64();
                mastered.report.notes.push("Preview is in memory. Tweak or re-master, then use Export selected master to save it.".into());
                Ok(TrackResult {
                    analysis: mastered.report.input.clone(),
                    before,
                    raw: Some(raw),
                    raw_analysis: Some(raw_analysis),
                    eq_parameters: Some(parameters),
                    after: Some(Arc::new(after)),
                    report: Some(mastered.report),
                })
            })();
            let event = match result {
                Ok(value) => Event::Ready(index, Arc::new(value)),
                Err(e) => Event::Failed(index, format!("{e:#}")),
            };
            let _ = tx.send(event);
            ctx.request_repaint();
        }
        let _ = tx.send(Event::Finished);
        ctx.request_repaint();
    });
}

pub(super) fn save_report(report: &Report) -> Result<()> {
    if let Some(first) = report.exports.first().and_then(|e| e["path"].as_str()) {
        let path = Path::new(first);
        let dir = path.parent().unwrap();
        let mut temp = tempfile::NamedTempFile::new_in(dir)?;
        serde_json::to_writer_pretty(&mut temp, report)?;
        temp.as_file().sync_all()?;
        export::persist_unique(
            temp,
            dir,
            &format!("{}.report", path.file_stem().unwrap().to_string_lossy()),
            "json",
        )?;
    }
    Ok(())
}

#[cfg(test)]
#[path = "worker_tests.rs"]
mod tests;
