use super::{app::MasterCraftApp, crystal_button, eq_curve, help, theme};
use crate::config::default_eq;
use eframe::egui::{self, RichText};

pub fn show(app: &mut MasterCraftApp, ui: &mut egui::Ui, ctx: &egui::Context) {
    let result = app.current().map(|(_, r)| r);
    let has_master = result.as_ref().is_some_and(|r| r.raw.is_some());
    let sr = result
        .as_ref()
        .map(|r| r.before.sample_rate)
        .unwrap_or(48000);
    ui.add_enabled_ui(!app.busy,|ui|{
        ui.horizontal_wrapped(|ui|{
            if ui.add_enabled(result.is_some(),crystal_button::secondary("Suggest EQ")).on_hover_text("Propose small tone changes using the profile. Compare before applying: this is a deterministic rule-based starting point. Uses the automatic master if available, otherwise the original. Limits are per band; overlapping bands can add together. The bone-colored proposal leaves your current EQ unchanged until applied.").clicked(){app.suggest_eq(ctx);}
            let tip=app.suggestion_note.as_deref().unwrap_or("Apply the proposed bands, then adjust any control. EQ changes are audible immediately during playback. No audio is exported.");
            if let Some(suggested) = &app.suggested_eq && ui.add(crystal_button::primary("Apply curve")).on_hover_text(tip).clicked(){app.settings.manual_eq=suggested.clone();app.eq_dirty=true;}
            ui.menu_button("Presets",|ui|preset_menu(app,ui));
        });
        if !has_master {
            ui.label(RichText::new("Master a track first to hear EQ changes live.").size(14.).color(theme::MUTED));
        } else if !app.player.controls.after.load(std::sync::atomic::Ordering::Relaxed) {
            ui.label(RichText::new("A is selected. Choose B to hear your EQ.").size(14.).color(theme::BONE));
        }
        eq_curve::show(app,ui,sr);
    });
}

pub(super) fn level_controls(app: &mut MasterCraftApp, ui: &mut egui::Ui) {
    if app.experience.advanced() {
        ui.add_space(6.);
        app.eq_dirty |= ui
            .checkbox(&mut app.settings.eq_hold_loudness, "Compensate EQ level")
            .on_hover_text(help::HOLD)
            .changed();
        let label = help::label(ui, "EQ output level", help::EQ_OUTPUT);
        app.eq_dirty |= super::control_layout::bounded_slider(
            ui,
            &mut app.settings.eq_output_db,
            -12.0..=12.0,
            " dB",
            Some(1),
        )
        .labelled_by(label.id)
        .on_hover_text(help::EQ_OUTPUT)
        .changed();
    } else if !app.settings.eq_hold_loudness || app.settings.eq_output_db != 0. {
        ui.label(RichText::new("Custom EQ level settings are active.").color(theme::BONE));
        if ui
            .add(crystal_button::secondary("Review advanced EQ"))
            .clicked()
        {
            app.experience.preferences.advanced = true;
            app.experience.persist(true, false);
        }
    }
}
fn preset_menu(app: &mut MasterCraftApp, ui: &mut egui::Ui) {
    if ui
        .button("Reset EQ")
        .on_hover_text(
            "Restore flat bands and 0 dB output. Playback updates smoothly without re-mastering.",
        )
        .clicked()
    {
        app.settings.manual_eq = default_eq();
        app.settings.eq_output_db = 0.;
        app.eq_dirty = true;
        ui.close();
    }
    if ui
        .button("Load preset…")
        .on_hover_text("Load saved EQ settings as an editable starting point.")
        .clicked()
        && let Some(path) = rfd::FileDialog::new()
            .add_filter("EQ preset", &["json"])
            .pick_file()
    {
        match app.settings.import_eq_preset(&path) {
            Ok(()) => app.eq_dirty = true,
            Err(e) => app.error = Some(format!("EQ preset: {e:#}")),
        }
        ui.close();
    }
    if ui.button("Save preset…").on_hover_text("Save EQ controls as JSON. Audio is not exported and existing preset files are preserved.").clicked()&&let Some(path)=rfd::FileDialog::new().set_file_name("slimeybeats-eq.json").save_file(){
        let value=serde_json::json!({"version":2,"bands":app.settings.manual_eq,"hold_loudness":app.settings.eq_hold_loudness,"out_gain_db":app.settings.eq_output_db,"bypass":false});
        if let Err(e)=crate::export::write_json(&path,&value){app.error=Some(format!("Save preset: {e:#}"));}ui.close();
    }
}
