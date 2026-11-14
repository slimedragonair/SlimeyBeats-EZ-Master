//! Voice settings only; preview and saving live in the pinned action dock.
use super::{state::Studio, tool_fields::slider};
use crate::{gui::theme, studio::voice_cleanup::NoiseRegion};
use eframe::egui;

pub(super) fn show(studio: &mut Studio, ui: &mut egui::Ui, advanced: bool) {
    theme::eyebrow(ui, "VOICE CLEANUP");
    ui.small("Reduce steady noise and sharp S sounds. This does not separate speech from music.");
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
    let columns = if advanced { 3 } else { 2 };
    ui.columns(columns, |fields| {
        slider(
            &mut fields[0],
            "Noise reduction",
            &mut studio.voice.noise_reduction,
            0. ..=1.,
            "",
            "Conservative stationary-noise reduction. Higher is stronger.",
        );
        slider(
            &mut fields[1],
            "Soften S sounds (de-ess)",
            &mut studio.voice.deess,
            0. ..=1.,
            "",
            "Reduce unusually sharp S/SH sounds without lowering all the treble.",
        );
        if advanced {
            slider(
                &mut fields[2],
                "Rumble removal",
                &mut studio.voice.highpass_hz,
                0. ..=300.,
                " Hz",
                "Remove low rumble. Around 75 Hz is a starting point; 0 is off.",
            );
        }
    });
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
