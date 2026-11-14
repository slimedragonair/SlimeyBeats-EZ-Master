//! Mastering actions and progressive disclosure; focused editors live in mastering_controls.
use super::{
    app::MasterCraftApp, crystal_button, help, learning_ui, mastering_controls, skin_icons::Icon,
    theme,
};
use crate::config::Settings;
use eframe::egui::{self, RichText};

const SCROLL_BOTTOM_INSET: f32 = 10.;

pub fn show(app: &mut MasterCraftApp, ui: &mut egui::Ui, ctx: &egui::Context) {
    let previous_profile = app.settings.profile.clone();
    let direction_changed = app.mastering_direction_changed();
    ui.add_enabled_ui(!app.busy && !app.tracks.is_empty(), |ui| {
        let has_master = app.current().is_some_and(|(_, r)| r.after.is_some());
        if ui
            .add_sized(
                [ui.available_width(), 42.],
                theme::primary(match (has_master, app.tracks.len()) {
                    (true, 1) => "Re-master track".to_owned(),
                    (false, 0 | 1) => "Master track".to_owned(),
                    (true, count) => format!("Re-master {count} tracks"),
                    (false, count) => format!("Master {count} tracks"),
                })
                .icon(Icon::Layers),
            )
            .on_hover_text(if has_master {
                help::REMASTER
            } else {
                help::MASTER
            })
            .clicked()
        {
            app.start(false, ctx);
        }
        if ui
            .add_enabled(
                has_master && !direction_changed,
                crystal_button::secondary("Export selected master")
                    .icon(Icon::Export)
                    .min_size(egui::vec2(ui.available_width(), 36.)),
            )
            .on_hover_text(help::EXPORT)
            .clicked()
        {
            app.export_revision(ctx);
        }
        ui.label(
            egui::RichText::new("Master creates a preview. Export saves it.")
                .size(13.)
                .color(theme::MUTED),
        );
        if has_master {
            let formats = app
                .settings
                .formats
                .iter()
                .map(|name| mastering_controls::format_label(name))
                .collect::<Vec<_>>()
                .join(" + ");
            let folder = app
                .settings
                .out_dir
                .file_name()
                .map(|name| name.to_string_lossy().into_owned())
                .unwrap_or_else(|| app.settings.out_dir.display().to_string());
            ui.add(
                egui::Label::new(
                    RichText::new(format!("{formats} → {folder}"))
                        .size(13.)
                        .color(theme::SLIME),
                )
                .truncate(),
            )
            .on_hover_text(app.settings.out_dir.display().to_string());
        }
    });
    if direction_changed {
        ui.label(
            RichText::new("Direction changed · re-master before exporting")
                .size(13.)
                .color(theme::BONE),
        );
    }
    ui.add_space(8.);
    learning_ui::mode_selector(&mut app.experience, ui);
    ui.add_space(5.);
    theme::eyebrow(
        ui,
        if app.tab == 1 {
            "EQ CONTROLS"
        } else {
            "SHAPE YOUR MASTER"
        },
    );
    ui.label(
        RichText::new("Hover names or controls for help")
            .size(12.)
            .color(theme::MUTED),
    );
    ui.add_space(5.);
    let mut scroll = egui::ScrollArea::vertical()
        .id_salt("settings-scroll")
        // ScrollArea rounds its child rect to physical pixels; keep that rounding inside the pane.
        .max_width((ui.available_width() - 1. / ui.pixels_per_point()).max(0.))
        .max_height((ui.available_height() - SCROLL_BOTTOM_INSET).max(0.))
        .scroll_bar_visibility(egui::scroll_area::ScrollBarVisibility::AlwaysVisible);
    if let Some(offset) = app.capture_settings_scroll {
        scroll = scroll.vertical_scroll_offset(offset);
    }
    scroll.show(ui, |ui| {
        ui.style_mut().wrap_mode = Some(egui::TextWrapMode::Wrap);
        let choices = advanced_choices(&app.settings);
        if !app.experience.advanced() && !choices.is_empty() {
            ui.label(
                RichText::new(format!("Advanced choices active: {}", choices.join(", ")))
                    .size(13.)
                    .color(theme::BONE),
            );
            if ui
                .add(crystal_button::secondary("Review advanced"))
                .clicked()
            {
                app.experience.preferences.advanced = true;
                app.experience.persist(true, false);
            }
            ui.separator();
        }
        ui.add_enabled_ui(!app.busy, |ui| {
            if app.tab == 1 {
                super::eq_band_controls::show(app, ui);
                super::eq_panel::level_controls(app, ui);
                ui.separator();
                super::control_layout::section_header(ui, "Mastering options").show(ui, |ui| {
                    mastering_controls::show_direction(app, ui);
                    if app.experience.advanced() {
                        mastering_controls::show_processing(app, ui);
                    }
                });
            } else {
                mastering_controls::show_direction(app, ui);
            }
            mastering_controls::show_export(app, ui);
            if app.experience.advanced() && app.tab != 1 {
                ui.add_space(8.);
                mastering_controls::show_processing(app, ui);
            }
        });
    });
    if app.settings.profile != previous_profile {
        app.suggested_eq = None;
        app.suggestion_note = None;
    }
}

fn advanced_choices(settings: &Settings) -> Vec<&'static str> {
    let defaults = Settings::default();
    let mut choices = vec![];
    if settings.profile.contains(':') || settings.profile.contains('+') {
        choices.push("custom profile");
    }
    if settings.target_lufs.is_some() {
        choices.push("custom loudness");
    }
    if (settings.ceiling_dbtp - defaults.ceiling_dbtp).abs() > 1e-9 {
        choices.push("peak ceiling");
    }
    if settings.disabled != defaults.disabled
        || settings.modules != defaults.modules
        || !settings.safety
        || (settings.max_makeup_db - defaults.max_makeup_db).abs() > 1e-9
        || (settings.max_limiter_gr_db - defaults.max_limiter_gr_db).abs() > 1e-9
    {
        choices.push("processing");
    }
    if settings.reference.is_some()
        && ((settings.match_strength - defaults.match_strength).abs() > 1e-9
            || settings.match_loudness
            || !settings.match_width)
    {
        choices.push("reference matching");
    }
    if settings.formats.len() != 1
        || settings
            .formats
            .iter()
            .any(|value| !["wav24", "mp3", "flac24"].contains(&value.as_str()))
        || settings.output_sr != 0
        || settings.suffix != defaults.suffix
        || !settings.dither
    {
        choices.push("export");
    }
    choices
}

#[cfg(test)]
#[path = "settings_panel_tests.rs"]
mod tests;
