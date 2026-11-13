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
) {
    if ui.add_enabled(!studio.busy()&&studio.current.is_some()&&!studio.mix_dirty,crystal_button::primary("Export current preview")).on_hover_text("Save the materialized Studio preview and a report. Rendering/preview and exporting are separate.").clicked() {studio.export_preview(ctx);}
    ui.separator();
    egui::ScrollArea::vertical().show(ui,|ui| {
        theme::eyebrow(ui,"STUDIO SOURCE");
        ui.add_enabled_ui(!studio.busy(),|ui| {
            if ui.add(crystal_button::secondary("Choose audio…")).on_hover_text("Load a local music mix or voice recording. The original file is kept unchanged.").clicked()
                && let Some(path)=rfd::FileDialog::new().add_filter("Audio",crate::audio::EXTENSIONS).pick_file() {studio.load(path,ctx);}
            if let Some(path)=selected && ui.add(crystal_button::secondary("Use selected track")).clicked() {studio.load(path,ctx);}
        });
        if let Some(source)=&studio.source {ui.label(source.path.file_name().unwrap_or_default().to_string_lossy().as_ref());ui.small(format!("{:.1} s · {} Hz · {} channel(s)",source.audio.duration(),source.audio.sample_rate,source.audio.channels));}
        ui.separator();
        let busy=studio.busy();
        ui.add_enabled_ui(!busy,|ui|match studio.page {Page::Stems=>stems(studio,ui,ctx),Page::Voice=>voice(studio,ui,ctx),Page::Repair=>repair(studio,ui,ctx),Page::Plugins=>super::plugin_controls::show(studio,ui,ctx)});
        ui.separator();theme::eyebrow(ui,"SAVE WHEN READY");
        if studio.mix_dirty {ui.label(RichText::new("Fader changes pending · update the preview first").color(theme::BONE));}
        ui.horizontal_wrapped(|ui| {
            ui.label("Format").on_hover_text("WAV works without extra codecs. FLAC/MP3 need FFmpeg. This saves the current Studio preview, not an automatic master.");
            egui::ComboBox::from_id_salt("studio-format").selected_text(&studio.export_format).width(104.).show_ui(ui,|ui|{for format in crate::config::FORMATS {ui.selectable_value(&mut studio.export_format,(*format).into(),*format);}});
        });
        if ui.add(crystal_button::secondary("Choose export folder…")).clicked() && let Some(path)=rfd::FileDialog::new().pick_folder() {studio.output_dir=path;}
        ui.small(studio.output_dir.to_string_lossy());
        if studio.bank.is_some() && ui.add_enabled(!studio.busy(),crystal_button::secondary("Export individual stems")).on_hover_text("Save an aligned raw bank as 32-bit float WAVs. Relative levels are retained and existing folders are preserved. Fader/solo settings affect the mix preview, not these raw stem files.").clicked(){studio.export_stems(ctx);}
    });
}

fn stems(studio: &mut Studio, ui: &mut egui::Ui, ctx: &egui::Context) {
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
        ui.label("First setup downloads an isolated engine and model. Your audio stays on this computer.");
        if ui.add_enabled(!studio.busy(),crystal_button::secondary("Install stem engine")).on_hover_text("One-time download using a private Python environment, separate from other apps. Requires internet, disk space and several minutes. No audio is uploaded.").clicked() {studio.install_engine(ctx);}
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
    ui.separator();
    theme::eyebrow(ui, "REBALANCE");
    let busy = studio.busy();
    ui.add_enabled_ui(!busy,|ui| {
        for track in &mut studio.mix.tracks {
            ui.label(RichText::new(track.kind.name()).color(theme::AMETHYST));
            studio.mix_dirty|=ui.add(CrystalSlider::new(&mut track.gain_db,-60. ..=12.).suffix(" dB")).on_hover_text("Stem level: negative lowers this part, positive raises it. This changes the saved mix after Update preview.").changed();
            ui.horizontal(|ui| {
                studio.mix_dirty|=ui.checkbox(&mut track.mute,"Mute").on_hover_text("Remove this part from the preview mix. Its original stem remains intact.").changed();
                studio.mix_dirty|=ui.checkbox(&mut track.solo,"Solo").on_hover_text("Hear only soloed parts. A muted part stays muted even if soloed.").changed();
            });
            if studio.bank.as_ref().is_some_and(|b|b.tracks[0].audio.channels==2) {studio.mix_dirty|=ui.add(CrystalSlider::new(&mut track.balance,-1. ..=1.).text("Balance")).on_hover_text("Stereo balance: -1 keeps the left side, +1 keeps the right; 0 leaves both unchanged.").changed();}
            ui.add_space(3.);
        }
    });
    if studio.bank.is_some() {
        if ui
            .add_enabled(!busy, crystal_button::primary("Update mix preview"))
            .clicked()
        {
            studio.render_mix(ctx);
        }
        ui.small(
            "Renders from the original stem bank; repeated fader edits do not stack processing.",
        );
    }
}

pub fn target(studio: &mut Studio, ui: &mut egui::Ui) {
    let previous = studio.target;
    ui.label("Process this source").on_hover_text("Choose the original, a separated stem, or the current preview. Choosing the current preview intentionally adds another edit on top.");
    let name = match studio.target {
        Target::Source => "Original source".into(),
        Target::Preview => "Current preview".into(),
        Target::Stem(kind) => kind.name().to_string(),
    };
    egui::ComboBox::from_id_salt("studio-target")
        .selected_text(name)
        .show_ui(ui, |ui| {
            ui.selectable_value(&mut studio.target, Target::Source, "Original source");
            ui.selectable_value(&mut studio.target, Target::Preview, "Current preview");
            if let Some(bank) = &studio.bank {
                for stem in &bank.tracks {
                    ui.selectable_value(
                        &mut studio.target,
                        Target::Stem(stem.kind),
                        stem.kind.name(),
                    );
                }
            }
        });
    if studio.target != previous {
        studio.spectrogram = None;
        studio.texture = None;
    }
}
fn voice(studio: &mut Studio, ui: &mut egui::Ui, ctx: &egui::Context) {
    target(studio, ui);
    ui.separator();
    theme::eyebrow(ui, "VOICE CLEANUP");
    ui.label("Use a voice recording or vocal stem. This reduces noise and harsh S sounds; it does not separate a voice from music.");
    slider(
        ui,
        "Rumble removal",
        &mut studio.voice.highpass_hz,
        0. ..=300.,
        " Hz",
        "High-pass filter: removes low rumble. Start around 75 Hz; 0 switches it off. Avoid cutting into a low voice.",
    );
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
        "De-ess",
        &mut studio.voice.deess,
        0. ..=1.,
        "",
        "Reduce unusually strong sharp S/SH sounds without turning down all the treble.",
    );
    let mut learned = studio.voice.noise_region.is_some();
    if ui.checkbox(&mut learned,"Use noise-only region").on_hover_text("Select room tone or hiss containing no words/music. The engine learns noise from this region.").changed() {studio.voice.noise_region=learned.then_some(NoiseRegion {start_seconds:0.,end_seconds:0.5});}
    if let Some(region) = &mut studio.voice.noise_region {
        ui.horizontal(|ui| {
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
    if ui
        .add_enabled(
            !studio.busy() && studio.audio_target().is_ok(),
            crystal_button::primary("Preview voice cleanup"),
        )
        .clicked()
    {
        studio.clean_voice(ctx);
    }
    if matches!(studio.target,Target::Stem(_)) && ui.add_enabled(!studio.busy()&&studio.current.is_some(),crystal_button::secondary("Apply preview to selected stem")).on_hover_text("Replace this stem with its prepared cleanup, then rebuild the full mix. Process the stem directly, apply it, then chain another edit if needed.").clicked(){studio.apply_preview_to_stem(ctx);}
}
fn repair(studio: &mut Studio, ui: &mut egui::Ui, ctx: &egui::Context) {
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
    ui.label("Drag on the spectrogram or enter an exact region below.");
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
    ui.radio_value(&mut studio.repair.mode, RepairMode::Attenuate, "Attenuate")
        .on_hover_text("Turn down the selected time/frequency region with soft boundaries.");
    ui.radio_value(&mut studio.repair.mode,RepairMode::Heal,"Heal from surroundings").on_hover_text("Estimate surrounding spectral magnitudes to soften a localized burst. It needs unselected time context and cannot recreate missing words.");
    if ui
        .add_enabled(
            !studio.busy() && studio.audio_target().is_ok(),
            crystal_button::primary("Preview spectral repair"),
        )
        .clicked()
    {
        studio.repair_audio(ctx);
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
