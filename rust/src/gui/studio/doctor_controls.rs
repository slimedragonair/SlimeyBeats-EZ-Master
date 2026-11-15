//! Explain findings separately from preparing and applying an audio revision.
use super::state::{Page, Studio};
use crate::{
    gui::{crystal_button, crystal_slider::CrystalSlider, theme},
    studio::doctor::{IssueKind, RepairAction},
};
use eframe::egui;

pub(super) fn active(studio: &Studio) -> bool {
    studio.page == Page::Repair && studio.doctor.visible
}

pub(super) fn repair_unavailable(studio: &Studio) -> Option<String> {
    if let Some(reason) = super::preview_actions::edit_unavailable(studio) {
        return Some(reason);
    }
    let Some(findings) = &studio.doctor.findings else {
        return Some("Scan the selected audio, then select a finding.".into());
    };
    if !findings.input.matches(studio) {
        return Some("Input changed; scan again.".into());
    }
    let issue = studio
        .doctor
        .selected
        .and_then(|i| findings.report.issues.get(i));
    match issue {
        None => Some("Select a finding to inspect its repair options.".into()),
        Some(issue) if issue.repair.is_none() => {
            Some("Inspection only · no automatic cuts or reconstruction.".into())
        }
        _ => None,
    }
}

pub(super) fn actions(studio: &mut Studio, ui: &mut egui::Ui, ctx: &egui::Context) {
    studio.sync_doctor_input();
    if ui.add_enabled(!studio.busy() && studio.audio_target().is_ok(), crystal_button::primary("Scan selected audio").wrap())
        .on_hover_text("Read-only background scan of the chosen source, prepared preview or stem before live faders. No changes are applied.")
        .clicked()
    {
        studio.scan_doctor(ctx);
    }
    let reason = repair_unavailable(studio);
    if ui.add_enabled(reason.is_none(), crystal_button::secondary("Preview selected repair").wrap())
        .on_hover_text("Prepare only this finding's supported repair for A/B listening. The original and stem bank are preserved until you explicitly apply the preview.")
        .clicked()
    {
        studio.preview_doctor_repair(ctx);
    }
    if let Some(reason) = reason {
        ui.small(reason);
    }
    if studio.busy() {
        ui.horizontal_wrapped(|ui| {
            ui.spinner();
            if ui
                .add(crystal_button::secondary("Cancel doctor job"))
                .clicked()
            {
                studio.cancel();
            }
        });
    }
}

pub(super) fn settings(studio: &mut Studio, ui: &mut egui::Ui) {
    ui.small("Scan → inspect → preview → compare A/B → apply or export.");
    ui.add_enabled_ui(!studio.busy(), |ui| {
        let label = ui.label("Repair strength").on_hover_text("0 leaves samples unchanged; 1 is the conservative full correction. Default 0.50. Applies only when you request a repair preview.");
        ui.horizontal(|ui| {
            ui.spacing_mut().slider_width = (ui.available_width() - 70.).clamp(40., 150.);
            ui.add(CrystalSlider::new(&mut studio.doctor.strength, 0. ..=1.).show_value(false))
                .labelled_by(label.id)
                .on_hover_text("Bounded 0–1; start at 0.50 and audition the preview.");
            ui.add(egui::DragValue::new(&mut studio.doctor.strength).range(0. ..=1.).speed(0.01).fixed_decimals(2))
                .labelled_by(label.id);
        });
    });
    ui.small("Low-mid buildup is a tonal heuristic, not proof of damage. Bass instruments and warm mixes can trigger it. Listen before reducing it.");
    ui.small("Silence and possible endings are inspection only. Nothing is cut or trimmed automatically.");
}

pub(super) fn show(studio: &mut Studio, ui: &mut egui::Ui) {
    studio.sync_doctor_input();
    ui.label(theme::display("Stem Doctor", 24.));
    ui.small("Find possible problems, inspect the evidence, then choose a repair preview.");
    ui.add_enabled_ui(!studio.busy(), |ui| super::tool_inputs::target(studio, ui));
    if studio.mix_dirty {
        ui.small("Scan reads the stored audio before live faders. Update mix preview before preparing repairs.");
    }
    let Some(findings) = &studio.doctor.findings else {
        ui.label("Scan selected audio to see timestamped findings and waveform markers.");
        return;
    };
    ui.small(format!(
        "{} finding(s) · {:.2} s · {} Hz · {} channel(s)",
        findings.report.issues.len(),
        findings.report.duration_seconds,
        findings.report.sample_rate,
        findings.report.channels
    ));
    super::doctor_waveform::show(studio, ui);
    let mut select = None;
    let findings = studio.doctor.findings.as_ref().unwrap();
    if findings.report.issues.is_empty() {
        ui.label("No issues found at the conservative thresholds. This does not guarantee artifact-free audio.");
    }
    if findings.report.truncated {
        ui.colored_label(
            theme::BONE,
            "Report limited · additional findings may be omitted.",
        );
    }
    for (index, issue) in findings.report.issues.iter().enumerate() {
        ui.push_id((&issue.id, index), |ui| {
            ui.separator();
            let label = format!("{} · {} · {}", super::doctor_time::range_label(issue.start_seconds, issue.end_seconds, findings.report.sample_rate), issue.severity, issue.title);
            if ui.add(egui::Button::selectable(studio.doctor.selected == Some(index), egui::RichText::new(label).color(issue_color(issue.kind))).wrap())
                .on_hover_text("Select this finding, seek the shared comparison transport and select its time range. This does not change audio or start playback.").clicked()
            {
                select = Some(index);
            }
            ui.label(&issue.description);
            ui.small(match issue.repair {
                Some(RepairAction::Declip) => "Available preview: conservative declip. Lost detail cannot be recovered reliably.",
                Some(RepairAction::ReduceLowMids) => "Available preview: gentle low-mid reduction. Tonal balance is subjective; compare A/B.",
                None => "Inspection only · no repair or automatic cut.",
            });
            egui::CollapsingHeader::new("Measured evidence").show(ui, |ui| {
                ui.label(serde_json::to_string_pretty(&issue.evidence).unwrap_or_else(|error| format!("Evidence unavailable: {error}")));
            });
        });
    }
    egui::CollapsingHeader::new("Scan thresholds and notes").show(ui, |ui| {
        ui.label(
            serde_json::to_string_pretty(&findings.report.thresholds)
                .unwrap_or_else(|error| format!("Thresholds unavailable: {error}")),
        );
        for note in &findings.report.notes {
            ui.small(note);
        }
    });
    if let Some(index) = select {
        studio.select_doctor_issue(index);
    }
}

pub(super) fn issue_color(kind: IssueKind) -> egui::Color32 {
    match kind {
        IssueKind::Clipping => egui::Color32::from_rgb(245, 155, 151),
        IssueKind::Silence => theme::SLIME,
        IssueKind::PossibleEnding => theme::BONE,
        IssueKind::LowMidBuildup => theme::AMETHYST,
    }
}
