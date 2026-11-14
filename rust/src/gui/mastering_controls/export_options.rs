use crate::config::FORMATS;
use crate::gui::theme;
use crate::gui::{
    app::MasterCraftApp,
    control_layout::{action_row, section_header},
    crystal_button, help,
};
use eframe::egui::{self, RichText};
use std::path::PathBuf;

pub(in crate::gui) fn show(app: &mut MasterCraftApp, ui: &mut egui::Ui) {
    let advanced = app.experience.advanced();
    let export = section_header(ui, "Export settings")
                    .id_salt(if app.tab == 1 { "eq-export-options" } else { "master-export-options" })
                    .default_open(!advanced && app.tab != 1)
                    .open(app.capture_settings_scroll.map(|_| true))
                    .show(ui, |ui| {
if advanced {
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
} else { basic_format(app, ui); }
if advanced {
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
}
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
                        && let Some(path) = rfd::FileDialog::new().set_directory(&app.settings.out_dir).pick_folder()
                    {
                        app.settings.out_dir = path;
                    }
                    if ui.add(crystal_button::secondary("Open folder")).clicked()
                        && let Err(e) = crate::desktop_integration::open_folder(&app.settings.out_dir)
                    {
                        app.error = Some(format!("Open output: {e}"));
                    }
                });
if advanced {
                help::label(ui, "File name suffix", "Text added to exported file names, such as _master. Existing exports are preserved. Do not include folder separators or .. in the suffix.");
                ui.add(egui::TextEdit::singleline(&mut app.settings.suffix)
                    .desired_width(ui.available_width()).clip_text(true));
}
                ui.small("Existing files are preserved. Every export is decoded and peak-checked.");
                });
    if export.body_returned.is_none() {
        let formats = if app.settings.formats.is_empty() {
            "No formats selected".to_owned()
        } else {
            app.settings
                .formats
                .iter()
                .map(|format| format_label(format))
                .collect::<Vec<_>>()
                .join(" · ")
        };
        ui.add(
            egui::Label::new(
                RichText::new(format!("Formats: {formats}"))
                    .size(13.)
                    .color(theme::MUTED),
            )
            .truncate(),
        )
        .on_hover_text(formats);
    }
}

pub(in crate::gui) fn format_label(format: &str) -> &str {
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

fn basic_format(app: &mut MasterCraftApp, ui: &mut egui::Ui) {
    ui.label("Format");
    let selected = match app.settings.formats.as_slice() {
        [value] => match value.as_str() {
            "wav24" => "WAV · recommended".into(),
            "mp3" => "MP3 · smaller file".into(),
            "flac24" => "FLAC · lossless compressed".into(),
            _ => format_label(value).to_owned(),
        },
        [] => "Choose a format".into(),
        _ => format!("Multiple formats ({})", app.settings.formats.len()),
    };
    egui::ComboBox::from_id_salt("basic-format")
        .width(ui.available_width())
        .truncate()
        .selected_text(selected)
        .show_ui(ui, |ui| {
            for (format, label) in [
                ("wav24", "WAV · recommended"),
                ("mp3", "MP3 · smaller file"),
                ("flac24", "FLAC · lossless compressed"),
            ] {
                if ui
                    .selectable_label(app.settings.formats.as_slice() == [format], label)
                    .on_hover_text(help::format(format))
                    .clicked()
                {
                    app.settings.formats = vec![format.into()];
                }
            }
        });
    ui.small("Advanced offers bit depth, multiple formats and sample rate.");
}
