use crate::config::{MODULES, Settings};
use crate::gui::{
    app::MasterCraftApp,
    control_layout::{action_row, bounded_slider, section_header},
    crystal_button, help,
};
use eframe::egui;

pub(in crate::gui) fn show(app: &mut MasterCraftApp, ui: &mut egui::Ui) {
    section_header(ui, "Processing modules & gain limits").show(ui, |ui| {
                    let label = ui.label("Limiter GR / dB");
                    bounded_slider(ui, &mut app.settings.max_limiter_gr_db, 0.0..=12.0, "", None)
                        .labelled_by(label.id).on_hover_text("Maximum allowed limiter gain reduction. Lower values preserve more transient impact but may miss the loudness target. Higher values can make the mix denser or flatter. Start conservatively.");
                    let label = ui.label("Makeup / dB");
                    bounded_slider(ui, &mut app.settings.max_makeup_db, 0.0..=40.0, "", None)
                        .labelled_by(label.id).on_hover_text("Maximum gain added by the loudness stage. This stops very quiet recordings or noise from being raised excessively. Leave the default unless you know the source contains useful quiet music.");
                    for &module in MODULES {
                        let mut on = app.settings.enabled(module);
                        if ui.checkbox(&mut on, module.replace('_', " ")).on_hover_text(help::module(module)).changed() {
                            if on {
                                app.settings.disabled.remove(module);
                            } else {
                                app.settings.disabled.insert(module.into());
                            }
                            if app.settings.modules[module].is_object() {
                                app.settings.modules[module]["enabled"] = serde_json::json!(on);
                            }
                        }
                    }
                });
    ui.separator();
    action_row(ui, |ui| {
        if ui.add(crystal_button::secondary("Save settings")).clicked() {
            app.save_settings();
        }
        if ui
            .add(crystal_button::secondary("Import settings…"))
            .clicked()
            && let Some(path) = rfd::FileDialog::new()
                .add_filter("Settings", &["json"])
                .pick_file()
        {
            match Settings::load(&path) {
                Ok(s) => {
                    app.settings = s;
                    app.eq_dirty = true;
                    app.status =
                        "Settings imported; valid changes are remembered automatically".into();
                }
                Err(e) => app.error = Some(format!("Import settings: {e:#}")),
            }
        }
    });
}
