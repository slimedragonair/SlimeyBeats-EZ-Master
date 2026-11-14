use super::state::Studio;
use crate::gui::{crystal_button, crystal_slider::CrystalSlider, theme};
use eframe::egui;

pub fn show(studio: &mut Studio, ui: &mut egui::Ui, ctx: &egui::Context) {
    super::tool_inputs::target(studio, ui);
    ui.separator();
    theme::eyebrow(ui, "OPTIONAL VST3 EFFECT");
    ui.label("Optional: run a trusted VST3 audio effect in a separate local helper. This is not a security sandbox.");
    ui.small("Mono/stereo float32 effects only. No vendor editor, MIDI or instruments.");
    if studio.plugin_host.is_none() {
        ui.label("The optional plugin helper is missing. Install the full desktop Studio package to use effects.");
        return;
    }
    ui.horizontal_wrapped(|ui| {
        if ui
            .add(crystal_button::secondary("Choose VST3 file…"))
            .clicked()
            && let Some(path) = rfd::FileDialog::new()
                .add_filter("VST3", &["vst3", "dll", "so"])
                .pick_file()
        {
            studio.probe_plugin(path, ctx);
        }
        if ui
            .add(crystal_button::secondary("Choose bundle folder…"))
            .clicked()
            && let Some(path) = rfd::FileDialog::new().pick_folder()
        {
            studio.probe_plugin(path, ctx);
        }
    });
    if let Some(path) = &studio.plugin_path {
        ui.label(
            path.file_name()
                .unwrap_or_default()
                .to_string_lossy()
                .as_ref(),
        );
    }
    if let Some(probe) = &studio.plugin_probe {
        let old = studio.plugin_class.clone();
        let selected = probe
            .plugins
            .iter()
            .find(|p| p.class.id == studio.plugin_class)
            .map_or("Choose an effect", |p| p.class.name.as_str());
        egui::ComboBox::from_id_salt("studio-vst-class")
            .selected_text(selected)
            .width((ui.available_width() - 16.).max(80.))
            .truncate()
            .show_ui(ui, |ui| {
                for info in &probe.plugins {
                    if info.error.is_none() {
                        ui.selectable_value(
                            &mut studio.plugin_class,
                            info.class.id.clone(),
                            &info.class.name,
                        );
                    }
                }
            });
        if old != studio.plugin_class {
            studio.reset_plugin_values();
        }
    }
    if let Some(info) = studio
        .plugin_probe
        .as_ref()
        .and_then(|p| p.plugins.iter().find(|p| p.class.id == studio.plugin_class))
    {
        ui.small("Controls use the plugin's 0–1 scale. Preview prepares audio to compare; Export saves that preview.");
        egui::ScrollArea::vertical()
            .id_salt("studio-plugin-parameters")
            .max_height(340.)
            .show(ui, |ui| {
                for parameter in &info.parameters {
                    ui.push_id(parameter.id, |ui| {
                        if let Some(value) = studio
                            .plugin_values
                            .iter_mut()
                            .find(|v| v.id == parameter.id)
                        {
                            ui.label(&parameter.name).on_hover_text(format!(
                    "Plugin control · {}. Values use the plugin's normalized 0–1 mapping.",
                    parameter.units
                ));
                            ui.add_enabled(
                                !parameter.read_only,
                                CrystalSlider::new(&mut value.normalized, 0. ..=1.),
                            );
                            if parameter.read_only {
                                ui.small("Read-only · reported by the plugin");
                            }
                        }
                    });
                }
            });
        if ui
            .add(crystal_button::secondary("Reset plugin controls"))
            .clicked()
        {
            studio.reset_plugin_values();
        }
    }
}
