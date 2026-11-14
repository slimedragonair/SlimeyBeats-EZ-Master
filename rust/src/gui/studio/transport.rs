use super::state::Studio;
use crate::gui::{crystal_button, crystal_slider::CrystalSlider, theme};
use eframe::egui;
use std::sync::atomic::Ordering;

pub fn dock(studio: &mut Studio, ui: &mut egui::Ui, compact: bool) {
    dock_layout(studio, ui, compact);
}
fn dock_layout(
    studio: &mut Studio,
    ui: &mut egui::Ui,
    compact: bool,
) -> egui::InnerResponse<egui::Rect> {
    egui::Panel::bottom("studio-listening")
        .frame(egui::Frame::NONE)
        .show(ui, |ui| {
            ui.separator();
            if !compact {
                comparison(studio, ui);
            }
            let controls = show(studio, ui);
            if compact {
                egui::CollapsingHeader::new("Comparison waveform")
                    .id_salt("studio-comparison-details")
                    .show(ui, |ui| comparison(studio, ui));
            }
            controls
        })
}

fn comparison(studio: &mut Studio, ui: &mut egui::Ui) {
    if let Some(current) = &studio.current
        && let Some(seconds) = studio.waveform.show_labeled(
            ui,
            studio.player.seconds(),
            current.audio.duration(),
            (
                "BEFORE",
                if studio.mix_dirty {
                    "PREVIOUS PREVIEW"
                } else {
                    "PREVIEW"
                },
            ),
            112.,
        )
    {
        studio.player.seek(seconds);
    }
}

fn show(studio: &mut Studio, ui: &mut egui::Ui) -> egui::Rect {
    let controls = ui.add_enabled_ui(studio.current.is_some(),|ui| {
        ui.horizontal_wrapped(|ui| {
            let playing=studio.player.controls.playing.load(Ordering::Relaxed);
            if ui.add(crystal_button::secondary(if playing{"Pause"}else{"Play"})).clicked() {if studio.player.seconds()>=studio.player.duration {studio.player.seek(0.);}studio.player.toggle();}
            if ui.add(crystal_button::secondary("Stop")).clicked() {studio.player.stop();}
            if ui.add(crystal_button::secondary("A · Before").selected(!studio.player.controls.after.load(Ordering::Relaxed))).on_hover_text("Compare with the input used for this revision, without changing saved samples.").clicked() {studio.player.controls.after.store(false,Ordering::Relaxed);}
            if ui.add(crystal_button::secondary(if studio.mix_dirty {"B · Previous"} else {"B · Preview"}).selected(studio.player.controls.after.load(Ordering::Relaxed))).clicked() {studio.player.controls.after.store(true,Ordering::Relaxed);}
        });
        ui.horizontal_wrapped(|ui| {
            ui.checkbox(&mut studio.match_levels,"Level match").on_hover_text("Listening-only loudness matching. It never alters the saved Studio audio.");
            ui.label("Volume").on_hover_text("Listening volume only; saved samples are unchanged.");
            ui.spacing_mut().slider_width=90.;
            ui.add(CrystalSlider::new(&mut studio.volume,0. ..=1.).show_value(false));
            ui.add(egui::Label::new(egui::RichText::new(format!("{:.1} / {:.1} s",studio.player.seconds(),studio.player.duration)).color(theme::MUTED)).wrap_mode(egui::TextWrapMode::Extend));
        });
    }).response.rect;
    studio
        .player
        .controls
        .volume
        .store(studio.volume.to_bits(), Ordering::Relaxed);
    studio
        .player
        .controls
        .matched
        .store(studio.match_levels, Ordering::Relaxed);
    controls
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{audio::Audio, gui::plots::Waveform, studio::stem_mix::MixSettings};
    use std::sync::Arc;

    #[test]
    fn loaded_studio_dock_keeps_all_controls_inside_its_panel_after_resizing() {
        let ctx = egui::Context::default();
        theme::apply(&ctx);
        let output = tempfile::tempdir().unwrap();
        let mut studio = Studio::new(output.path().into());
        let audio = Arc::new(Audio::new(vec![0.1; 48000], 48000, 1).unwrap());
        studio.waveform = Waveform::new(&audio, Some(&audio));
        studio.current = Some(crate::gui::studio::state::Revision {
            before: audio.clone(),
            audio,
            label: "Test".into(),
            notes: vec![],
            before_lufs: -20.,
            after_lufs: -20.,
            bank: None,
            mix: MixSettings::default(),
        });
        studio.player.duration = 16777.2;
        for (width, height) in [(450., 440.), (380., 400.), (650., 600.), (450., 440.)] {
            let mut bounds = None;
            for _ in 0..6 {
                let mut frame = ctx.run_ui(
                    egui::RawInput {
                        screen_rect: Some(egui::Rect::from_min_size(
                            egui::Pos2::ZERO,
                            egui::vec2(width, height),
                        )),
                        ..Default::default()
                    },
                    |ui| {
                        let clip = ui.clip_rect();
                        let panel = dock_layout(&mut studio, ui, true);
                        bounds = Some((
                            clip,
                            panel.response.rect,
                            panel.inner,
                            ui.available_rect_before_wrap(),
                        ));
                    },
                );
                frame.textures_delta.clear();
            }
            let (clip, panel, controls, remaining) = bounds.unwrap();
            assert!(
                clip.expand(0.5).contains_rect(controls),
                "controls clipped at {width}x{height}: {controls:?}, {clip:?}"
            );
            assert!(
                panel.expand(0.5).contains_rect(controls),
                "controls outside dock: {controls:?}, {panel:?}"
            );
            assert!(
                remaining.bottom() <= panel.top() + 0.5,
                "editing area overlaps playback dock"
            );
        }
    }
}
