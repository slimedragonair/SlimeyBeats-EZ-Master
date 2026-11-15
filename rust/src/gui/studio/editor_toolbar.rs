//! Always-visible navigation for the stem timeline; it never renders audio.
use super::{editor_state::time_label, state::Studio};
use crate::gui::{crystal_button, theme};
use eframe::egui;

pub(super) fn view_selector(studio: &mut Studio, ui: &mut egui::Ui, compact: bool) {
    if studio.bank.is_none() || studio.bulk_import.is_some() {
        return;
    }
    if !compact {
        ui.separator();
        ui.label("View");
    }
    for (timeline, label) in [(true, "Timeline"), (false, "Mixer")] {
        if ui
            .add(crystal_button::secondary(label).selected(studio.editor.timeline == timeline))
            .clicked()
        {
            studio.editor.timeline = timeline;
        }
    }
}

pub(super) fn show(studio: &mut Studio, ui: &mut egui::Ui) {
    let Some(bank) = studio.bank.as_ref() else {
        return;
    };
    studio.editor.sync(bank);
    if !studio.editor.timeline {
        return;
    }
    let duration = bank.tracks[0].audio.duration();
    ui.horizontal_wrapped(|ui| {
        if ui
            .small_button("Fit song")
            .on_hover_text(
                "Show every stem from start to finish. Audio and time selection are unchanged.",
            )
            .clicked()
        {
            studio.editor.fit_song();
        }
        if ui
            .add_enabled(
                studio.editor.selection.is_some(),
                egui::Button::new("Fit selection").small(),
            )
            .clicked()
        {
            studio.editor.fit_selection(duration);
        }
        let focus = studio
            .editor
            .selection
            .map_or(studio.player.seconds(), |(a, b)| (a + b) * 0.5);
        for (label, factor, help) in [
            ("−", 2., "Zoom out around the selection or playhead"),
            ("+", 0.5, "Zoom in around the selection or playhead"),
        ] {
            if ui.small_button(label).on_hover_text(help).clicked() {
                studio.editor.zoom(duration, focus, factor);
            }
        }
        for (label, direction) in [("←", -1.), ("→", 1.)] {
            if ui
                .small_button(label)
                .on_hover_text("Move the visible time window without moving any audio")
                .clicked()
            {
                studio.editor.pan(duration, direction);
            }
        }
        if ui.small_button(format!("Waves {:.0}×", studio.editor.waveform_scale)).on_hover_text("Enlarge the waveform drawing only: 1×, 2×, 4× or 8×. Listening and saved samples do not change; the drawing can reach its lane edges without the audio clipping.").clicked() {
            studio.editor.waveform_scale = if studio.editor.waveform_scale >= 8. { 1. } else { studio.editor.waveform_scale * 2. };
        }
        if ui.small_button("Edit selection…").on_hover_text("Show exact time fields and edits for the selected stem in the right inspector.").clicked() {
            studio.editor.inspector_edits = true;
            studio.editor.reveal_edits = true;
        }
    });
    if studio.editor.timeline {
        let (start, end) = studio.editor.range(duration);
        ui.colored_label(
            theme::MUTED,
            egui::RichText::new(format!(
                "{} parts · {}–{} · click ruler to seek; drag audio to select time",
                bank.tracks.len(),
                time_label(start),
                time_label(end)
            ))
            .size(12.),
        );
        ui.small("Source waveforms · live fader levels are heard in B, not drawn here.");
    }
}
