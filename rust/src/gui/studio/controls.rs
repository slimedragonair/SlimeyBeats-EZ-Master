use super::state::{Page, Studio, Target};
use crate::gui::{crystal_button, crystal_slider::CrystalSlider, theme};
use crate::studio::{spectral_repair::RepairMode, voice_cleanup::NoiseRegion};
use eframe::egui::{self, RichText};
use std::path::PathBuf;

pub fn show(
    studio: &mut Studio,
    ui: &mut egui::Ui,
    selected: Option<PathBuf>,
    ctx: &egui::Context,
    advanced: bool,
) {
    // Saving stays reachable while the settings or a large stem bank scrolls.
    egui::Panel::bottom("studio-export")
        .frame(egui::Frame::NONE)
        .show(ui, |ui| export_controls(studio, ui, ctx));
    ui.horizontal_wrapped(|ui| {
        if ui.add_enabled(!studio.busy(),crystal_button::secondary("Open project…")).clicked() && let Some(path)=rfd::FileDialog::new().add_filter("Studio project",&["json"]).pick_file(){studio.load_project(path,ctx);}
        if ui.add_enabled(preview_unavailable(studio).is_none(),crystal_button::secondary("Save project…")).on_hover_text("Save self-contained source/stem/preview working copies, settings and hashes in a new folder. No model download is needed to reopen it.").clicked() && let Some(folder)=rfd::FileDialog::new().pick_folder(){studio.save_project(folder,ctx);}
    });
    if (studio.page == Page::Stems || studio.mix_dirty)
        && studio.bank.is_some()
        && studio.bulk_import.is_none()
    {
        let label = if studio.mix_dirty {
            "Update mix preview · pending"
        } else {
            "Update mix preview"
        };
        if ui.add_enabled(!studio.busy(), crystal_button::primary(label)).on_hover_text("Render the current fader, mute, solo and balance settings from the immutable stem bank. This does not export.").clicked() {
            studio.render_mix(ctx);
        }
        if studio.mix_dirty {
            ui.small("Fader changes pending. Update the mix before another edit, project save or preview export.");
        }
    }
    ui.separator();
    let page_id = match studio.page {
        Page::Stems => "stems",
        Page::Voice => "voice",
        Page::Repair => "repair",
        Page::Plugins => "plugins",
    };
    egui::ScrollArea::vertical()
        .id_salt(("studio-settings", page_id))
        .auto_shrink([false, false])
        .show(ui, |ui| {
            let mixing =
                studio.page == Page::Stems && studio.bank.is_some() && studio.bulk_import.is_none();
            if !mixing && (studio.source.is_some() || studio.bulk_import.is_some()) {
                ui.collapsing("Studio source", |ui| {
                    source_controls(studio, ui, selected.clone(), ctx)
                });
            } else if !mixing {
                source_controls(studio, ui, selected.clone(), ctx);
            }
            if !mixing {
                ui.separator();
            }
            let busy = studio.busy();
            ui.add_enabled_ui(!busy, |ui| match studio.page {
                Page::Stems => stems(studio, ui, ctx),
                Page::Voice => voice(studio, ui, ctx, advanced),
                Page::Repair => repair(studio, ui, ctx, advanced),
                Page::Plugins => super::plugin_controls::show(studio, ui, ctx),
            });
            if mixing {
                ui.collapsing("Studio source", |ui| {
                    source_controls(studio, ui, selected, ctx)
                });
            }
            if studio.page != Page::Stems && matches!(studio.target, Target::Stem(_)) {
                let ready = !busy && !studio.mix_dirty && studio.can_apply_preview_to_stem();
                if ui
                    .add_enabled(
                        ready,
                        crystal_button::secondary("Apply preview to selected stem").wrap(),
                    )
                    .clicked()
                {
                    studio.apply_preview_to_stem(ctx);
                }
                ui.small(if ready {
                    "Apply replaces this part and rebuilds the mix. Export then saves that mix."
                } else {
                    "Preview an edit from this stem first, then apply it to rebuild the full mix."
                });
            }
        });
}

pub(super) fn preview_unavailable(studio: &Studio) -> Option<&'static str> {
    if studio.busy() {
        Some("Wait for the current job or cancel it first.")
    } else if studio.mix_dirty {
        Some("Update mix preview to include pending fader changes.")
    } else if studio.current.is_none() {
        Some("Load audio or import stems to prepare a preview.")
    } else {
        None
    }
}

pub(super) fn edit_unavailable(studio: &Studio) -> Option<String> {
    if studio.busy() {
        Some("Wait for the current job or cancel it first.".into())
    } else {
        studio.editing_target().err().map(|error| error.to_string())
    }
}

fn export_controls(studio: &mut Studio, ui: &mut egui::Ui, ctx: &egui::Context) {
    ui.separator();
    let reason = preview_unavailable(studio);
    if ui.add_enabled(reason.is_none(), crystal_button::primary("Export current preview")).on_hover_text("Save the prepared Studio preview and a report. Preview renders audio; Export writes it to your chosen folder.").clicked() { studio.export_preview(ctx); }
    if let Some(reason) = reason {
        ui.small(reason);
    }
    ui.horizontal_wrapped(|ui| {
        ui.label("Format")
            .on_hover_text("WAV needs no extra codecs. FLAC/MP3 need FFmpeg.");
        egui::ComboBox::from_id_salt("studio-format")
            .selected_text(&studio.export_format)
            .width(88.)
            .show_ui(ui, |ui| {
                for format in crate::config::FORMATS {
                    ui.selectable_value(&mut studio.export_format, (*format).into(), *format);
                }
            });
        if ui
            .add(crystal_button::secondary("Folder…"))
            .on_hover_text(
                "Choose the Studio export folder. This is remembered separately from mastering.",
            )
            .clicked()
            && let Some(path) = rfd::FileDialog::new()
                .set_directory(&studio.output_dir)
                .pick_folder()
        {
            studio.output_dir = path;
        }
    });
    ui.add(
        egui::Label::new(egui::RichText::new(studio.output_dir.to_string_lossy()).small())
            .truncate(),
    )
    .on_hover_text(studio.output_dir.to_string_lossy());
    if studio.bank.is_some() {
        if ui.add_enabled(!studio.busy(), crystal_button::secondary("Export raw stems")).on_hover_text("Save the current bank as aligned 32-bit float WAV files. Includes applied stem edits, ignores faders/mute/solo, and preserves existing folders.").clicked() { studio.export_stems(ctx); }
        ui.small("Individual files · ignores faders, mute and solo.");
    }
}

fn source_controls(
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
        ui.label(
            source
                .path
                .file_name()
                .unwrap_or_default()
                .to_string_lossy()
                .as_ref(),
        );
        ui.small(format!(
            "{:.1} s · {} Hz · {} channel(s)",
            source.audio.duration(),
            source.audio.sample_rate,
            source.audio.channels
        ));
    }
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

pub fn target(studio: &mut Studio, ui: &mut egui::Ui) {
    let previous = studio.target;
    ui.label("Process this source").on_hover_text("Choose the original, a separated stem, or the current preview. Choosing the current preview intentionally adds another edit on top.");
    let name = match studio.target {
        Target::Source => "Original source".into(),
        Target::Preview => "Current preview".into(),
        Target::Stem(kind) => studio.bank.as_ref().map_or_else(
            || kind.name().to_string(),
            |bank| bank.display_name(kind).to_string(),
        ),
    };
    egui::ComboBox::from_id_salt("studio-target")
        .selected_text(name)
        .width((ui.available_width() - 16.).max(80.))
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
        });
    if studio.target != previous {
        studio.analysis = None;
        studio.status = "Input changed · Analyze to measure the selected source".into();
        studio.spectrogram = None;
        studio.texture = None;
    }
}
fn voice(studio: &mut Studio, ui: &mut egui::Ui, ctx: &egui::Context, advanced: bool) {
    target(studio, ui);
    ui.separator();
    theme::eyebrow(ui, "VOICE CLEANUP");
    ui.label("Use a voice recording or vocal stem. This reduces noise and harsh S sounds; it does not separate a voice from music.");
    let custom = voice_custom_settings(studio);
    if !advanced && !custom.is_empty() {
        ui.colored_label(
            theme::BONE,
            format!(
                "Custom voice settings active · {}. Advanced shows these controls.",
                custom.join(" · ")
            ),
        );
    }
    if advanced {
        slider(
            ui,
            "Rumble removal",
            &mut studio.voice.highpass_hz,
            0. ..=300.,
            " Hz",
            "High-pass filter: removes low rumble. Start around 75 Hz; 0 switches it off. Avoid cutting into a low voice.",
        );
    }
    slider(
        ui,
        "Noise reduction",
        &mut studio.voice.noise_reduction,
        0. ..=1.,
        "",
        "Conservative stationary-noise reduction. Higher is stronger. If auto learning is unreliable, it is skipped with a note.",
    );
    slider(
        ui,
        "Soften sharp S sounds (de-ess)",
        &mut studio.voice.deess,
        0. ..=1.,
        "",
        "Reduce unusually strong sharp S/SH sounds without turning down all the treble.",
    );
    if advanced {
        let mut learned = studio.voice.noise_region.is_some();
        if ui.checkbox(&mut learned,"Use noise-only region").on_hover_text("Select room tone or hiss containing no words/music. The engine learns noise from this region.").changed() {studio.voice.noise_region=learned.then_some(NoiseRegion {start_seconds:0.,end_seconds:0.5});}
        if let Some(region) = &mut studio.voice.noise_region {
            ui.horizontal_wrapped(|ui| {
                ui.label("From");
                ui.add(
                    egui::DragValue::new(&mut region.start_seconds)
                        .speed(0.01)
                        .suffix(" s"),
                );
                ui.label("to");
                ui.add(
                    egui::DragValue::new(&mut region.end_seconds)
                        .speed(0.01)
                        .suffix(" s"),
                );
            });
        }
        ui.checkbox(&mut studio.voice.hold_loudness,"Hold loudness").on_hover_text("Compensate the level change by at most 3 dB for a fairer comparison, while respecting the peak ceiling.");
        let mut gate = studio.voice.gate_threshold_db.is_some();
        if ui.checkbox(&mut gate, "Quiet-gap gate").on_hover_text("Gentle downward expansion in quiet gaps. Keep the threshold below the voice to avoid cutting off words.").changed() { studio.voice.gate_threshold_db = gate.then_some(-55.); }
        if let Some(threshold) = &mut studio.voice.gate_threshold_db {
            slider(
                ui,
                "Gate threshold",
                threshold,
                -80. ..=-15.,
                " dBFS",
                "Signals below this threshold are gently reduced. A lower value preserves more quiet speech.",
            );
        }
        ui.collapsing("Voice output & peak safety", |ui| {
            slider(
                ui,
                "Output gain",
                &mut studio.voice.output_gain_db,
                -12. ..=12.,
                " dB",
                "Level adjustment applied to the next voice preview.",
            );
            ui.checkbox(&mut studio.voice.protect_peaks, "Protect peaks");
            slider(
                ui,
                "Peak ceiling",
                &mut studio.voice.ceiling_dbtp,
                -12. ..=0.,
                " dBTP",
                "Maximum true peak when protection is enabled.",
            );
        });
    }
    ui.small("Preview creates audio for A/B listening. Export saves the prepared preview; changed settings need another preview.");
    let reason = edit_unavailable(studio);
    if ui
        .add_enabled(
            reason.is_none(),
            crystal_button::primary("Preview voice cleanup"),
        )
        .clicked()
    {
        studio.clean_voice(ctx);
    }
    if let Some(reason) = reason {
        ui.small(reason);
    }
}
fn voice_custom_settings(studio: &Studio) -> Vec<String> {
    let voice = &studio.voice;
    let defaults = crate::studio::voice_cleanup::VoiceSettings::default();
    let mut custom = vec![];
    if voice.highpass_hz != defaults.highpass_hz {
        custom.push(format!("rumble {:.0} Hz", voice.highpass_hz));
    }
    if let Some(region) = &voice.noise_region {
        custom.push(format!(
            "noise-only {:.2}–{:.2} s",
            region.start_seconds, region.end_seconds
        ));
    }
    if let Some(threshold) = voice.gate_threshold_db {
        custom.push(format!("quiet-gap gate {threshold:.0} dBFS"));
    }
    if !voice.hold_loudness {
        custom.push("loudness compensation off".into());
    }
    if voice.output_gain_db != 0. {
        custom.push(format!("output {:+.1} dB", voice.output_gain_db));
    }
    if !voice.protect_peaks || voice.ceiling_dbtp != defaults.ceiling_dbtp {
        custom.push(format!(
            "peak protection {} · {:.1} dBTP",
            if voice.protect_peaks { "on" } else { "off" },
            voice.ceiling_dbtp
        ));
    }
    custom
}

fn repair(studio: &mut Studio, ui: &mut egui::Ui, ctx: &egui::Context, advanced: bool) {
    target(studio, ui);
    if ui
        .add_enabled(
            studio.audio_target().is_ok(),
            crystal_button::secondary("Build spectrogram"),
        )
        .clicked()
    {
        studio.build_spectrogram(ctx);
    }
    ui.separator();
    theme::eyebrow(ui, "TIME / FREQUENCY SELECTION");
    ui.label(
        "Select an unwanted sound by time and pitch. Drag the graph or type exact values below.",
    );
    if !advanced && studio.repair.mode != RepairMode::Attenuate {
        ui.colored_label(
            theme::BONE,
            "Custom repair mode active · Heal from surroundings. Advanced shows the mode control.",
        );
    }
    ui.horizontal(|ui| {
        ui.label("Start");
        ui.add(
            egui::DragValue::new(&mut studio.repair.start_seconds)
                .speed(0.01)
                .suffix(" s"),
        );
    });
    ui.horizontal(|ui| {
        ui.label("End");
        ui.add(
            egui::DragValue::new(&mut studio.repair.end_seconds)
                .speed(0.01)
                .suffix(" s"),
        );
    });
    ui.horizontal(|ui| {ui.label("Low").on_hover_text("Lowest frequency to affect, in Hz. Low frequencies are bass/rumble; higher frequencies are brightness/hiss.");ui.add(egui::DragValue::new(&mut studio.repair.low_hz).speed(10.).suffix(" Hz"));});
    ui.horizontal(|ui| {
        ui.label("High");
        ui.add(
            egui::DragValue::new(&mut studio.repair.high_hz)
                .speed(10.)
                .suffix(" Hz"),
        );
    });
    slider(
        ui,
        "Reduction",
        &mut studio.repair.reduction_db,
        -60. ..=0.,
        " dB",
        "How much to reduce the selected region. -6 dB is roughly half amplitude; 0 is an exact bypass.",
    );
    if advanced {
        ui.radio_value(
            &mut studio.repair.mode,
            RepairMode::Attenuate,
            "Turn down selected sound",
        )
        .on_hover_text("Turn down the selected time/frequency region with soft boundaries.");
        ui.radio_value(&mut studio.repair.mode,RepairMode::Heal,"Heal from surroundings").on_hover_text("Estimate surrounding spectral magnitudes to soften a localized burst. It needs unselected time context and cannot recreate missing words.");
    }
    ui.small("Preview prepares the repair for A/B listening. Apply puts a stem edit into the mix. Export saves the prepared audio.");
    let reason = edit_unavailable(studio);
    if ui
        .add_enabled(
            reason.is_none(),
            crystal_button::primary("Preview spectral repair"),
        )
        .clicked()
    {
        studio.repair_audio(ctx);
    }
    if let Some(reason) = reason {
        ui.small(reason);
    }
}
fn slider(
    ui: &mut egui::Ui,
    label: &str,
    value: &mut f64,
    range: std::ops::RangeInclusive<f64>,
    suffix: &str,
    help: &str,
) {
    ui.label(label).on_hover_text(help);
    ui.add(CrystalSlider::new(value, range).suffix(suffix))
        .on_hover_text(help);
}
