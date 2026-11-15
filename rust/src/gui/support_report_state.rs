//! A report stays local and immutable between preview, review, copy and save.
use super::{app::MasterCraftApp, studio::state::Page};
use crate::support_report::{self, IssueCategory, Report, ReportContext, ReportDraft, Workflow};

#[derive(Default)]
pub(super) struct SupportReportState {
    pub title: String,
    pub details: String,
    pub category: IssueCategory,
    pub preview: Option<Report>,
    pub preview_text: String,
    pub reviewed: bool,
    pub reveal_preview: bool,
    pub message: String,
    pub error: Option<String>,
}
impl SupportReportState {
    pub fn invalidate(&mut self) {
        self.preview = None;
        self.preview_text.clear();
        self.reviewed = false;
        self.reveal_preview = false;
        self.message.clear();
        self.error = None;
    }
    pub fn build(&mut self, context: ReportContext) {
        self.invalidate();
        match ReportDraft::new(self.category, &self.title, &self.details)
            .and_then(|draft| support_report::build(&context, &draft))
        {
            Ok(report) => {
                self.preview_text = report.markdown().into();
                self.preview = Some(report);
                self.reveal_preview = true;
            }
            Err(error) => self.error = Some(error.to_string()),
        }
    }
    pub fn save(&mut self, path: &std::path::Path) {
        if !self.reviewed {
            self.error = Some("Review the preview before saving or copying it.".into());
            return;
        }
        let Some(report) = &self.preview else {
            return;
        };
        match report.write_new(path) {
            Ok(()) => {
                self.message = "Report saved locally. Nothing was submitted.".into();
                self.error = None;
            }
            Err(error) => self.error = Some(error.to_string()),
        }
    }
}
pub(super) fn context(app: &MasterCraftApp) -> ReportContext {
    let mut context = ReportContext::current();
    context.workflow = match app.tab {
        0 => Workflow::Overview,
        1 => Workflow::ManualEq,
        2 => Workflow::ProcessingReport,
        3 => match app.studio.page {
            Page::Stems => Workflow::StemStudio,
            Page::Voice => Workflow::VoiceCleanup,
            Page::Repair => Workflow::SpectralRepair,
            Page::Plugins => Workflow::PluginProcessing,
        },
        _ => Workflow::Other,
    };
    context.counts.tracks = app.tracks.len().min(u32::MAX as usize) as u32;
    context.counts.stems = app
        .studio
        .bank
        .as_ref()
        .map(|b| b.tracks.len().min(u32::MAX as usize) as u32)
        .unwrap_or(0);
    context.counts.eq_bands = app.settings.manual_eq.len().min(u32::MAX as usize) as u32;
    context.activity.session_loaded = !app.tracks.is_empty() || app.studio.source.is_some();
    context.activity.preview_available =
        app.current().is_some_and(|(_, r)| r.after.is_some()) || app.studio.current.is_some();
    context.activity.processing_active = app.busy || app.studio.busy() || app.updates.busy();
    context.activity.recovery_available =
        app.recovery.last_saved.is_some() || !app.recovery.listing.checkpoints.is_empty();
    // Readiness is not probed here. Unknown tools retain their explicit default
    // instead of hashing large binaries or launching processes on the UI thread.
    context
}
