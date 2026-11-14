//! Project/import sidebar; active editors are in the workspace and actions stay pinned.
pub(super) use super::preview_actions::preview_unavailable;
use super::state::{Page, Studio};
use crate::gui::{crystal_button, theme};
use eframe::egui::{self, RichText};
use std::path::PathBuf;

pub fn show(
    studio: &mut Studio,
    ui: &mut egui::Ui,
    selected: Option<PathBuf>,
    ctx: &egui::Context,
    _advanced: bool,
) {
    super::preview_actions::dock(studio, ui, ctx);
    ui.horizontal_wrapped(|ui| {
        if ui.add_enabled(!studio.busy(),crystal_button::secondary("Open project…")).clicked() && let Some(path)=rfd::FileDialog::new().add_filter("Studio project",&["json"]).pick_file(){studio.load_project(path,ctx);}
        if ui.add_enabled(preview_unavailable(studio).is_none(),crystal_button::secondary("Save project…")).on_hover_text("Save self-contained source/stem/preview working copies, settings and hashes in a new folder. No model download is needed to reopen it.").clicked() && let Some(folder)=rfd::FileDialog::new().pick_folder(){studio.save_project(folder,ctx);}
    });

    ui.separator();
    egui::ScrollArea::vertical().id_salt("studio-source-and-import").auto_shrink([false, false]).show(ui, |ui| {
        if matches!(studio.page, Page::Voice | Page::Repair) || studio.source.is_none() {
            super::tool_inputs::source(studio, ui, selected.clone(), ctx);
        } else {
            ui.collapsing("Studio source", |ui| super::tool_inputs::source(studio, ui, selected.clone(), ctx));
        }
        ui.separator();
        let available = !studio.busy();
        ui.add_enabled_ui(available, |ui| match studio.page {
            Page::Stems => stems(studio, ui, ctx),
            Page::Plugins => super::plugin_controls::show(studio, ui, ctx),
            Page::Voice => { ui.small("Edit voice cleanup in the workspace. Preview prepares audio; Export saves it."); },
            Page::Repair => { ui.small("Set the time and frequency region in the workspace, then Preview spectral repair."); },
        });
    });
}

fn stems(studio: &mut Studio, ui: &mut egui::Ui, ctx: &egui::Context) {
    if studio.bank.is_some() && studio.bulk_import.is_none() {
        ui.label(format!("{} stems loaded", studio.mix.tracks.len()));
        ui.small("Adjust each part beside its waveform in the workspace. Update the mix preview to hear changes.");
        ui.collapsing("Import another stem set", |ui| {
            super::stem_bulk_import::show(studio, ui, ctx);
            super::stem_import::show(studio, ui, ctx);
        });
        ui.collapsing("Optional local stem engine", |ui| {
            stem_engine(studio, ui, ctx)
        });
        return;
    }
    super::stem_bulk_import::show(studio, ui, ctx);
    if studio.bulk_import.is_some() {
        ui.small(
            "Review the parts in the workspace. No stem engine download is needed for import.",
        );
        return;
    }
    super::stem_import::show(studio, ui, ctx);
    ui.separator();
    ui.collapsing("Optional: separate a full mix locally", |ui| {
        stem_engine(studio, ui, ctx)
    });
}

fn stem_engine(studio: &mut Studio, ui: &mut egui::Ui, ctx: &egui::Context) {
    theme::eyebrow(ui, "LOCAL STEM ENGINE");
    let ready = studio.runtime.is_ready();
    ui.label(
        RichText::new(if ready {
            "● Ready · local CPU"
        } else {
            "○ Optional download required"
        })
        .color(if ready { theme::SLIME } else { theme::MUTED }),
    );
    if !ready {
        ui.label(
            "Optional: only needed to split a full mix. Importing existing stems needs no model.",
        );
        ui.small("Install explicitly downloads an isolated engine and model. Audio stays on this computer.");
        if ui.add_enabled(!studio.busy(),crystal_button::secondary("Install stem engine")).on_hover_text("One-time download using a private Python environment, separate from other apps. Requires internet, disk space and several minutes. No audio is uploaded.").clicked() {studio.install_engine(false,ctx);}
    }
    if ready {
        ui.collapsing("Engine maintenance", |ui| {
            if ui.add(crystal_button::secondary("Verify installed model")).on_hover_text("Check every model file against the verified installation manifest without downloading it again.").clicked() { studio.install_engine(false, ctx); }
            if ui.add(crystal_button::secondary("Repair stem engine")).on_hover_text("Reinstall the private dependencies and verified model. Unknown files are preserved in a backup; your audio is not touched.").clicked() { studio.install_engine(true, ctx); }
        });
    }
    ui.add_enabled_ui(!studio.busy(),|ui| {
        ui.radio_value(&mut studio.two_stems,true,"Vocal + instrumental");
        ui.radio_value(&mut studio.two_stems,false,"Four stems").on_hover_text("Vocals, drums, bass and other. Separation estimates can contain bleed; compare before using them.");
    });
    if ui
        .add_enabled(
            ready && !studio.busy() && studio.source.is_some(),
            crystal_button::primary("Separate source"),
        )
        .clicked()
    {
        studio.separate(ctx);
    }
    if !ready {
        ui.small("Install the optional engine before separating audio.");
    } else if studio.source.is_none() {
        ui.small("Choose a full music mix as the source first.");
    }
}
