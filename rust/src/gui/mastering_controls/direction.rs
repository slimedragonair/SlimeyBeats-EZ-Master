use crate::gui::theme;
use crate::gui::{
    app::MasterCraftApp,
    control_layout::{action_row, bounded_slider, section_header},
    crystal_button, help,
};
use crate::profiles;
use eframe::egui::{self, RichText};

pub(in crate::gui) fn show(app: &mut MasterCraftApp, ui: &mut egui::Ui) {
    let advanced = app.experience.advanced();
    let profile_label_response = help::label(ui, "Mastering profile", help::PROFILE);
    egui::ComboBox::from_id_salt("profile")
        .width(ui.available_width())
        .truncate()
        .selected_text(profile_label(
            &app.settings.profile,
            &app.root.join("profiles"),
        ))
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
                        ui.selectable_value(&mut app.settings.profile, name.clone(), name);
                    }
                }
            }
        })
        .response
        .labelled_by(profile_label_response.id);
    if advanced {
        section_header(ui, "Blend / custom profile").show(ui, |ui| {
            ui.add(
                egui::TextEdit::singleline(&mut app.settings.profile)
                    .desired_width(ui.available_width())
                    .clip_text(true),
            );
            ui.small("Example: metal:0.6+edm:0.4");
            if ui
                .add(crystal_button::secondary("Import profile JSON"))
                .clicked()
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
    }
    ui.add_space(6.);
    help::label(ui, "Loudness mode", help::MODE);
    ui.horizontal(|ui| {
        let width = (ui.available_width() - ui.spacing().item_spacing.x) / 2.;
        for (mode, label) in [("streaming_safe", "Gentler"), ("loud", "Louder")] {
            if ui
                .add(
                    crystal_button::secondary(label)
                        .selected(app.settings.mode == mode)
                        .min_size(egui::vec2(width, 34.)),
                )
                .on_hover_text(help::MODE)
                .clicked()
            {
                app.settings.mode = mode.into();
            }
        }
    });
    if app.settings.target_lufs.is_some() && !advanced {
        ui.label(
            RichText::new(
                "A custom loudness target overrides the mode. Review Advanced to change it.",
            )
            .color(theme::BONE),
        );
    } else if !advanced {
        ui.small("Gentler keeps more contrast. Louder can sound denser.");
    }
    if advanced {
        let mut override_target = app.settings.target_lufs.is_some();
        if ui
            .checkbox(&mut override_target, "Custom target / integrated LUFS")
            .on_hover_text(help::LUFS)
            .changed()
        {
            app.settings.target_lufs = if override_target { Some(-14.) } else { None };
        }
        if let Some(lufs) = &mut app.settings.target_lufs {
            bounded_slider(ui, lufs, -30.0..=-5.0, " LUFS", None).on_hover_text(help::LUFS);
        }
        let label = help::label(ui, "True-peak ceiling", help::CEILING);
        bounded_slider(
            ui,
            &mut app.settings.ceiling_dbtp,
            -6.0..=0.0,
            " dBTP",
            None,
        )
        .labelled_by(label.id)
        .on_hover_text(help::CEILING);
    }
    let label = help::label(
        ui,
        if advanced {
            "Correction intensity"
        } else {
            "Correction amount"
        },
        help::INTENSITY,
    );
    if advanced {
        bounded_slider(ui, &mut app.settings.intensity, 0.0..=1.5, "", Some(2))
            .labelled_by(label.id)
            .on_hover_text(help::INTENSITY);
    } else {
        let mut percent = app.settings.intensity * 100.;
        if bounded_slider(ui, &mut percent, 0.0..=150.0, "%", Some(0))
            .labelled_by(label.id)
            .on_hover_text(help::INTENSITY)
            .changed()
        {
            app.settings.intensity = percent / 100.;
        }
        ui.small("100% is the normal strength. Lower it for a subtler result.");
    }
    if advanced {
        ui.checkbox(&mut app.settings.safety, "Check punch & tonal regressions")
            .on_hover_text(help::SAFETY);
    } else {
        ui.small(format!(
            "Peak ceiling: {:.1} dBTP",
            app.settings.ceiling_dbtp
        ));
    }
    ui.add_space(7.);
    ui.separator();
    section_header(ui, "Reference track (optional)")
        .default_open(false)
        .show(ui, |ui| {
            action_row(ui, |ui| {
                let width = ui.available_width()
                    - if app.settings.reference.is_some() {
                        72.
                    } else {
                        0.
                    };
                if ui
                    .add(
                        crystal_button::secondary("Choose reference…")
                            .min_size(egui::vec2(width, 34.))
                            .wrap(),
                    )
                    .on_hover_text(help::REFERENCE)
                    .clicked()
                    && let Some(p) = rfd::FileDialog::new()
                        .add_filter("Audio", crate::audio::EXTENSIONS)
                        .pick_file()
                {
                    app.settings.reference = Some(p);
                }
                if app.settings.reference.is_some()
                    && ui.add(crystal_button::secondary("Clear")).clicked()
                {
                    app.settings.reference = None;
                }
            });
            if let Some(reference) = &app.settings.reference {
                ui.add(
                    egui::Label::new(
                        RichText::new(reference.file_name().unwrap_or_default().to_string_lossy())
                            .color(theme::SLIME),
                    )
                    .truncate(),
                )
                .on_hover_text(reference.display().to_string());
                if advanced {
                    let label = help::label(ui, "Strength", help::MATCH);
                    bounded_slider(ui, &mut app.settings.match_strength, 0.0..=1.5, "", None)
                        .labelled_by(label.id)
                        .on_hover_text(help::MATCH);
                    ui.checkbox(&mut app.settings.match_loudness, "Match loudness")
                        .on_hover_text(help::MATCH_LOUDNESS);
                    ui.checkbox(&mut app.settings.match_width, "Match stereo width")
                        .on_hover_text(help::MATCH_WIDTH);
                }
            } else {
                ui.small("Optional: use a finished track as the tonal target.");
            }
        });
    ui.add_space(7.);
    ui.separator();
}

fn profile_label(name: &str, directory: &std::path::Path) -> String {
    profiles::resolve(name, directory)
        .ok()
        .and_then(|profile| profile["display_name"].as_str().map(str::to_owned))
        .unwrap_or_else(|| name.replace('_', " "))
}
