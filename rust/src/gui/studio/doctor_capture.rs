//! Opt-in renderer orchestration; normal Studio contexts contain no capture request.
use super::state::{Page, Studio, Target};
use eframe::egui;

#[derive(Clone, Copy, Default, PartialEq, Eq)]
enum Phase {
    #[default]
    Loading,
    Scanning,
    Ready,
    Failed,
}

#[derive(Clone, Default, serde::Serialize)]
pub(in crate::gui) struct Status {
    pub doctor_scan_ready: bool,
    pub doctor_issue_count: usize,
    #[serde(skip)]
    pub error: Option<String>,
    #[serde(skip)]
    phase: Phase,
}

fn key() -> egui::Id {
    egui::Id::unique("diagnostic-doctor-capture")
}

pub(in crate::gui) fn request(ctx: &egui::Context) {
    ctx.data_mut(|data| data.insert_temp(key(), Status::default()));
    ctx.request_repaint();
}

pub(in crate::gui) fn status(ctx: &egui::Context) -> Option<Status> {
    ctx.data(|data| data.get_temp(key()))
}

pub(super) fn tick(studio: &mut Studio, ctx: &egui::Context) {
    let Some(mut state) = status(ctx) else {
        return;
    };
    if studio.busy() || state.phase == Phase::Failed {
        return;
    }
    if let Some(error) = &studio.error {
        state.error = Some(format!("Doctor renderer capture: {error}"));
        state.phase = Phase::Failed;
        state.doctor_scan_ready = false;
    } else {
        match state.phase {
            Phase::Loading if studio.source.is_some() => {
                studio.page = Page::Repair;
                studio.doctor.visible = true;
                studio.target = Target::Source;
                studio.scan_doctor(ctx);
                state.phase = Phase::Scanning;
            }
            Phase::Scanning | Phase::Ready => {
                if let Some(findings) = &studio.doctor.findings
                    && findings.input.matches(studio)
                {
                    state.doctor_issue_count = findings.report.issues.len();
                    state.doctor_scan_ready = true;
                    if state.phase == Phase::Scanning {
                        let selected = findings
                            .report
                            .issues
                            .iter()
                            .position(|issue| issue.repair.is_some())
                            .or_else(|| (!findings.report.issues.is_empty()).then_some(0));
                        if let Some(index) = selected {
                            studio.select_doctor_issue(index);
                        }
                    }
                    state.phase = Phase::Ready;
                } else {
                    state.doctor_scan_ready = false;
                    state.error =
                        Some("Doctor renderer capture ended without a current issue report".into());
                    state.phase = Phase::Failed;
                }
            }
            _ => {}
        }
    }
    ctx.data_mut(|data| data.insert_temp(key(), state));
}

#[cfg(test)]
#[path = "doctor_capture_tests.rs"]
mod tests;
