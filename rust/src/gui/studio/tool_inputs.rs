//! Studio source loading and processing-target choice.
use super::state::{Studio, Target};
use crate::gui::{crystal_button, theme};
use eframe::egui;
use std::path::PathBuf;

pub(super) fn source(
    studio: &mut Studio,
    ui: &mut egui::Ui,
    selected: Option<PathBuf>,
    ctx: &egui::Context,
) {
    theme::eyebrow(ui, "STUDIO SOURCE");
    ui.add_enabled_ui(!studio.busy(), |ui| {
        if ui
            .add(crystal_button::secondary("Choose audio…"))
            .on_hover_text(
                "Load a local music mix or voice recording. The original file is kept unchanged.",
            )
            .clicked()
            && let Some(path) = rfd::FileDialog::new()
                .add_filter("Audio", crate::audio::EXTENSIONS)
                .pick_file()
        {
            studio.load(path, ctx);
        }
        if let Some(path) = selected
            && ui
                .add(crystal_button::secondary("Use selected track"))
                .clicked()
        {
            studio.load(path, ctx);
        }
    });
    if let Some(source) = &studio.source {
        let name = source
            .path
            .file_name()
            .unwrap_or_default()
            .to_string_lossy();
        ui.add(egui::Label::new(name.as_ref()).truncate())
            .on_hover_text(source.path.to_string_lossy());
        ui.small(format!(
            "{:.1} s · {} Hz · {} channel(s)",
            source.audio.duration(),
            source.audio.sample_rate,
            source.audio.channels
        ));
    }
}

pub(super) fn target(studio: &mut Studio, ui: &mut egui::Ui) {
    let previous = studio.target;
    let name = match studio.target {
        Target::Source => "Original source".into(),
        Target::Preview => "Current preview".into(),
        Target::Stem(kind) => studio.bank.as_ref().map_or_else(
            || kind.name().to_string(),
            |bank| bank.display_name(kind).to_string(),
        ),
    };
    ui.horizontal_wrapped(|ui| {
        let label = ui.label("Process this source");
        egui::ComboBox::from_id_salt("studio-target")
            .selected_text(name)
            .width((ui.available_width() - 24.).clamp(160., 360.))
            .truncate()
            .show_ui(ui, |ui| {
                ui.selectable_value(&mut studio.target, Target::Source, "Original source");
                ui.selectable_value(&mut studio.target, Target::Preview, "Current preview");
                if let Some(bank) = &studio.bank {
                    for stem in &bank.tracks {
                        ui.selectable_value(
                            &mut studio.target,
                            Target::Stem(stem.kind),
                            bank.display_name(stem.kind),
                        );
                    }
                }
            })
            .response
            .labelled_by(label.id);
    });
    if studio.target != previous {
        studio.analysis = None;
        studio.status = "Input changed · Analyze to measure the selected source".into();
        studio.spectrogram = None;
        studio.texture = None;
    }
}
