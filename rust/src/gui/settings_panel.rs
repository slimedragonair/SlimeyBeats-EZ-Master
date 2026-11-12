use super::{
    app::MasterCraftApp, crystal_button, crystal_slider::CrystalSlider, help, skin_icons::Icon,
    theme,
};
use crate::{
    config::{FORMATS, MODULES, Settings},
    profiles,
};
use eframe::egui::{self, RichText};
use std::{ops::RangeInclusive, path::PathBuf};

const SCROLL_BOTTOM_INSET: f32 = 10.;

pub fn show(app: &mut MasterCraftApp, ui: &mut egui::Ui, ctx: &egui::Context) {
    let previous_profile = app.settings.profile.clone();
    ui.add_enabled_ui(!app.busy && !app.tracks.is_empty(), |ui| {
        let has_master = app.current().is_some_and(|(_, r)| r.after.is_some());
        if ui
            .add_sized(
                [ui.available_width(), 42.],
                theme::primary(if has_master {
                    "Re-master queue"
                } else {
                    "Master queue"
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
                has_master,
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
            egui::RichText::new("Master → tweak → export when ready")
                .size(13.)
                .color(theme::MUTED),
        );
    });
    ui.add_space(8.);
    theme::eyebrow(ui, "MASTERING DIRECTION");
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
            ui.add_enabled_ui(!app.busy, |ui| {
                help::label(ui,"Mastering profile",help::PROFILE);
                egui::ComboBox::from_id_salt("profile")
                    .width(ui.available_width())
                    .truncate()
                    .selected_text(profile_label(&app.settings.profile, &app.root.join("profiles")))
                    .show_ui(ui, |ui| {
                        for (key, value) in profiles::builtin() {
                            ui.selectable_value(
                                &mut app.settings.profile,
                                key,
                                value["display_name"].as_str().unwrap_or("Custom"),
                            );
                        }
                        if let Ok(entries) = std::fs::read_dir(app.root.join("profiles")) {
                            for file in entries.flatten() {
                                if file.path().extension().is_some_and(|x| x == "json") {
                                    let name = format!(
                                        "custom:{}",
                                        file.path().file_stem().unwrap().to_string_lossy()
                                    );
                                    ui.selectable_value(
                                        &mut app.settings.profile,
                                        name.clone(),
                                        name,
                                    );
                                }
                            }
                        }
                    });
                section_header(ui, "Blend / custom profile").show(ui, |ui| {
                    ui.add(egui::TextEdit::singleline(&mut app.settings.profile)
                        .desired_width(ui.available_width()).clip_text(true));
                    ui.small("Example: metal:0.6+edm:0.4");
                    if ui.add(crystal_button::secondary("Import profile JSON")).clicked()
                        && let Some(path) = rfd::FileDialog::new()
                            .add_filter("Profile", &["json"])
                            .pick_file()
                    {
                        let result = (|| -> anyhow::Result<()> {
                            let source: serde_json::Value = serde_json::from_str(
                                std::fs::read_to_string(&path)?.trim_start_matches('\u{feff}'),
                            )?;
                            profiles::target_curve(&source)?;
                            let dest = app.root.join("profiles").join(path.file_name().unwrap());
                            crate::export::write_json(&dest, &source)?;
                            app.settings.profile =
                                format!("custom:{}", path.file_stem().unwrap().to_string_lossy());
                            Ok(())
                        })();
                        if let Err(e) = result {
                            app.error = Some(format!("Import profile: {e:#}"));
                        }
                    }
                });
                ui.add_space(6.);
                help::label(ui,"Loudness mode",help::MODE);
                ui.horizontal(|ui| {
                    let width = (ui.available_width() - ui.spacing().item_spacing.x) / 2.;
                    for (mode, label) in [("streaming_safe", "Gentler"), ("loud", "Louder")] {
                        if ui.add(crystal_button::secondary(label)
                            .selected(app.settings.mode == mode)
                            .min_size(egui::vec2(width, 34.)))
                            .on_hover_text(help::MODE).clicked()
                        {
                            app.settings.mode = mode.into();
                        }
                    }
                });
                let mut override_target = app.settings.target_lufs.is_some();
                if ui
                    .checkbox(&mut override_target, "Custom target / integrated LUFS").on_hover_text(help::LUFS)
                    .changed()
                {
                    app.settings.target_lufs = if override_target { Some(-14.) } else { None };
                }
                if let Some(lufs) = &mut app.settings.target_lufs {
                    bounded_slider(ui, lufs, -30.0..=-5.0, " LUFS", None).on_hover_text(help::LUFS);
                }
                let label = help::label(ui,"True-peak ceiling",help::CEILING);
                bounded_slider(ui, &mut app.settings.ceiling_dbtp, -6.0..=0.0, " dBTP", None)
                    .labelled_by(label.id).on_hover_text(help::CEILING);
                let label = help::label(ui,"Correction intensity",help::INTENSITY);
                bounded_slider(ui, &mut app.settings.intensity, 0.0..=1.5, "", Some(2))
                    .labelled_by(label.id).on_hover_text(help::INTENSITY);
                ui.checkbox(&mut app.settings.safety, "Check punch & tonal regressions").on_hover_text(help::SAFETY);
                ui.add_space(7.);
                ui.separator();
                theme::eyebrow(ui, "REFERENCE TRACK");
                action_row(ui, |ui| {
                    let width = ui.available_width() - if app.settings.reference.is_some() { 72. } else { 0. };
                    if ui.add(crystal_button::secondary("Choose reference…")
                        .min_size(egui::vec2(width, 34.)).wrap()).on_hover_text(help::REFERENCE).clicked()
                        && let Some(p) = rfd::FileDialog::new()
                            .add_filter("Audio", crate::audio::EXTENSIONS)
                            .pick_file()
                    {
                        app.settings.reference = Some(p);
                    }
                    if app.settings.reference.is_some() && ui.add(crystal_button::secondary("Clear")).clicked() {
                        app.settings.reference = None;
                    }
                });
                if let Some(reference) = &app.settings.reference {
                    ui.add(egui::Label::new(
                        RichText::new(reference.file_name().unwrap_or_default().to_string_lossy())
                            .color(theme::SLIME),
                    ).truncate())
                    .on_hover_text(reference.display().to_string());
                    let label = help::label(ui, "Strength", help::MATCH);
                    bounded_slider(ui, &mut app.settings.match_strength, 0.0..=1.5, "", None)
                        .labelled_by(label.id).on_hover_text(help::MATCH);
                    ui.checkbox(&mut app.settings.match_loudness, "Match loudness").on_hover_text(help::MATCH_LOUDNESS);
                    ui.checkbox(&mut app.settings.match_width, "Match stereo width").on_hover_text(help::MATCH_WIDTH);
                } else {
                    ui.small("Optional: use a finished track as the tonal target.");
                }
                ui.add_space(7.);
                ui.separator();
                let export = section_header(ui, "Export settings")
                    .default_open(false)
                    .open(app.capture_settings_scroll.map(|_| true))
                    .show(ui, |ui| {
                egui::Grid::new("formats").num_columns(2)
                    .max_col_width((ui.available_width() - ui.spacing().item_spacing.x) / 2.)
                    .show(ui, |ui| {
                    for (i, &format) in FORMATS.iter().enumerate() {
                        let mut selected = app.settings.formats.iter().any(|s| s == format);
                        if ui
                            .checkbox(
                                &mut selected,
                                format_label(format),
                            )
                            .on_hover_text(help::format(format))
                            .changed()
                        {
                            if selected {
                                app.settings.formats.push(format.into());
                            } else {
                                app.settings.formats.retain(|s| s != format);
                            }
                        }
                        if i % 2 == 1 {
                            ui.end_row();
                        }
                    }
                });
                ui.checkbox(&mut app.settings.dither, "TPDF dither for integer exports").on_hover_text(help::DITHER);
                help::label(ui,"Sample rate",help::SAMPLE_RATE);
                egui::ComboBox::from_id_salt("sample-rate")
                    .width(ui.available_width())
                    .truncate()
                    .selected_text(if app.settings.output_sr == 0 {
                        "Keep source rate".into()
                    } else {
                        format!("{} Hz", app.settings.output_sr)
                    })
                    .show_ui(ui, |ui| {
                        for sr in [0, 44100, 48000, 88200, 96000, 192000] {
                            ui.selectable_value(
                                &mut app.settings.output_sr,
                                sr,
                                if sr == 0 {
                                    "Keep source rate".into()
                                } else {
                                    format!("{sr} Hz")
                                },
                            );
                        }
                    });
                help::label(ui,"Output folder","Where Export saves the selected master and its measurement report. Master and EQ preview do not write files here.");
                let mut directory = app.settings.out_dir.to_string_lossy().into_owned();
                if ui
                    .add(
                        egui::TextEdit::singleline(&mut directory)
                            .desired_width(ui.available_width()).clip_text(true),
                    )
                    .changed()
                {
                    app.settings.out_dir = PathBuf::from(directory);
                }
                action_row(ui, |ui| {
                    if ui.add(crystal_button::secondary("Browse…")).clicked()
                        && let Some(path) = rfd::FileDialog::new().pick_folder()
                    {
                        app.settings.out_dir = path;
                    }
                    if ui.add(crystal_button::secondary("Open folder")).clicked()
                        && let Err(e) = crate::audio::external_command("explorer.exe")
                            .arg(&app.settings.out_dir)
                            .spawn()
                    {
                        app.error = Some(format!("Open output: {e}"));
                    }
                });
                help::label(ui, "File name suffix", "Text added to exported file names, such as _master. Existing exports are preserved. Do not include folder separators or .. in the suffix.");
                ui.add(egui::TextEdit::singleline(&mut app.settings.suffix)
                    .desired_width(ui.available_width()).clip_text(true));
                ui.small("Existing files are preserved. Every export is decoded and peak-checked.");
                });
                if export.body_returned.is_none() {
                    let formats = if app.settings.formats.is_empty() {
                        "No formats selected".to_owned()
                    } else {
                        app.settings.formats.iter().map(|format| format_label(format))
                            .collect::<Vec<_>>().join(" · ")
                    };
                    ui.add(egui::Label::new(RichText::new(format!("Formats: {formats}"))
                        .size(13.).color(theme::MUTED)).truncate()).on_hover_text(formats);
                }
                ui.add_space(10.);
                ui.add_space(7.);
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
                    if ui.add(crystal_button::secondary("Import settings…")).clicked()
                        && let Some(path) = rfd::FileDialog::new()
                            .add_filter("Settings", &["json"])
                            .pick_file()
                    {
                        match Settings::load(&path) {
                            Ok(s) => {
                                app.settings = s;
                                app.eq_dirty = true;
                                app.status =
                                    "Settings imported; stored in Rust only when saved".into();
                            }
                            Err(e) => app.error = Some(format!("Import settings: {e:#}")),
                        }
                    }
                });
            });
        });
    if app.settings.profile != previous_profile {
        app.suggested_eq = None;
        app.suggestion_note = None;
    }
}

fn format_label(format: &str) -> &str {
    match format {
        "wav24" => "WAV 24-bit",
        "wav16" => "WAV 16-bit",
        "wav32f" => "WAV float",
        "flac24" => "FLAC 24-bit",
        "flac16" => "FLAC 16-bit",
        "mp3" => "MP3 320k",
        _ => format,
    }
}

fn action_row<R>(
    ui: &mut egui::Ui,
    add: impl FnOnce(&mut egui::Ui) -> R,
) -> egui::InnerResponse<R> {
    ui.horizontal_wrapped(|ui| {
        ui.style_mut().wrap_mode = Some(egui::TextWrapMode::Extend);
        add(ui)
    })
}

fn section_header(ui: &egui::Ui, text: &str) -> egui::CollapsingHeader {
    // CollapsingHeader otherwise forces Extend, even in a wrapping settings pane.
    let width =
        (ui.available_width() - ui.spacing().indent - ui.spacing().button_padding.x).max(1.);
    let text = egui::WidgetText::from(text).into_galley(
        ui,
        Some(egui::TextWrapMode::Wrap),
        width,
        egui::TextStyle::Button,
    );
    egui::CollapsingHeader::new(text)
}

fn configured_slider<'a>(
    value: &'a mut f64,
    range: RangeInclusive<f64>,
    suffix: &'a str,
    decimals: Option<usize>,
) -> CrystalSlider<'a> {
    let slider = CrystalSlider::new(value, range).suffix(suffix);
    if let Some(decimals) = decimals {
        slider.fixed_decimals(decimals)
    } else {
        slider
    }
}

fn bounded_slider(
    ui: &mut egui::Ui,
    value: &mut f64,
    range: RangeInclusive<f64>,
    suffix: &str,
    decimals: Option<usize>,
) -> egui::Response {
    let width = ui.available_width();
    // Slider's response includes its native DragValue. Measure that actual overhead
    // on a disabled, invisible copy: suffixes, fonts and numeric precision all affect it.
    // A fixed allowance can exceed the pane by a few pixels and grow it every repaint.
    let mut measured_value = *value;
    let mut measure = ui.new_child(
        egui::UiBuilder::new()
            .id_salt("slider-width-measure")
            .sizing_pass()
            .invisible(),
    );
    measure.spacing_mut().slider_width = width;
    let measured_width = measure
        .add(configured_slider(
            &mut measured_value,
            range.clone(),
            suffix,
            decimals,
        ))
        .rect
        .width();
    let value_width = (measured_width - width).max(0.);
    ui.scope(|ui| {
        // Leave one physical pixel for independent layout rounding in the scroll area.
        ui.spacing_mut().slider_width = (width - value_width - 1. / ui.pixels_per_point()).max(1.);
        ui.add(configured_slider(value, range, suffix, decimals))
    })
    .inner
}

fn profile_label(name: &str, directory: &std::path::Path) -> String {
    profiles::resolve(name, directory)
        .ok()
        .and_then(|profile| profile["display_name"].as_str().map(str::to_owned))
        .unwrap_or_else(|| name.replace('_', " "))
}

#[cfg(test)]
#[path = "settings_panel_tests.rs"]
mod tests;
