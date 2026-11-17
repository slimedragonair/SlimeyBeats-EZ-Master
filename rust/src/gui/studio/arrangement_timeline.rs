//! Virtualized clip lanes with source-offset waveforms and independent move/select gestures.
use super::{
    arrangement_gestures::{self, Gesture, Kind, SelectionSnapshot},
    arrangement_state::{ArrangementEditor, EqTarget},
    arrangement_waveforms::Waveforms,
    editor_state::time_label_precise,
};
use crate::{
    gui::{crystal_button, theme},
    studio::arrangement::{Clip, Edit},
};
use anyhow::Result;
use eframe::egui::{self, Align2, Color32, FontId, Pos2, Rect, Sense, Stroke, StrokeKind};

const ROW_HEIGHT: f32 = 96.;
const CLIP_HEADER: f32 = 22.;

fn x(time: f64, plot: Rect, start: f64, span: f64) -> f32 {
    plot.left() + ((time - start) / span) as f32 * plot.width()
}
fn at(pointer: f32, plot: Rect, start: f64, span: f64, rate: u32) -> u64 {
    ((start + ((pointer - plot.left()) / plot.width().max(1.)).clamp(0., 1.) as f64 * span)
        * rate as f64)
        .round()
        .max(0.) as u64
}

pub(super) fn show(
    editor: &mut ArrangementEditor,
    waves: &Waveforms,
    gesture: &mut Option<Gesture>,
    ui: &mut egui::Ui,
    playhead: f64,
) -> Result<Option<u64>> {
    let Some(document) = editor.document() else {
        ui.label("Add tracks or a Suno ZIP. Each part becomes a movable waveform clip.");
        ui.small("Keep whole songs, stems or short recordings on separate tracks; source files stay unchanged.");
        return Ok(None);
    };
    let rate = document.sample_rate;
    let duration = document.duration_frames() as f64 / rate as f64;
    let lane_count = document.lanes.len();
    ui.horizontal_wrapped(|ui| {
        ui.selectable_value(&mut editor.move_tool, false, "Select time");
        ui.selectable_value(&mut editor.move_tool, true, "Move clips");
        if ui.add(crystal_button::secondary("Fit session")).clicked() {
            editor.fit();
        }
        if ui
            .add_enabled(
                editor.selection.is_some(),
                crystal_button::secondary("Fit selection"),
            )
            .clicked()
            && let Some(selection) = editor.selection
        {
            let first = selection.start_frame as f64 / rate as f64;
            let last = selection.end_frame as f64 / rate as f64;
            let width = (last - first).max(0.01);
            editor.view_start = (first - width * 0.1).max(0.);
            editor.view_end = editor.view_start + width * 1.2;
        }
        for (label, factor) in [("−", 2.), ("+", 0.5)] {
            if ui
                .small_button(label)
                .on_hover_text("Zoom around the current timeline view without changing audio.")
                .clicked()
            {
                let center = (editor.view_start + editor.view_end) / 2.;
                let span =
                    ((editor.view_end - editor.view_start) * factor).clamp(0.01, duration.max(1.));
                editor.view_start = (center - span / 2.).clamp(0., (duration - span).max(0.));
                editor.view_end = editor.view_start + span;
            }
        }
        if ui.add(crystal_button::secondary("Mix EQ")).clicked() {
            editor.eq_target = EqTarget::Mix;
            editor.eq_inspector = true;
            editor.refresh_eq();
        }
    });
    if ui.ctx().viewport_rect().height() >= 800. {
        ui.small("Drag a clip's title to move it; drag either edge to trim. Select time on the waveform for section EQ.");
    }
    let start = editor.view_start.max(0.);
    let span = (editor.view_end - start).max(0.01);
    let header_width = (ui.available_width() * 0.24).clamp(135., 200.);
    let (ruler, response) = ui.allocate_exact_size(
        egui::vec2(ui.available_width(), 30.),
        Sense::click_and_drag(),
    );
    let plot = Rect::from_min_max(
        Pos2::new(ruler.left() + header_width, ruler.top()),
        ruler.max,
    );
    let divisions = (plot.width() / 80.).floor().max(1.) as usize;
    let decimals = if span < 0.1 {
        4
    } else if span < 1. {
        3
    } else {
        1
    };
    for index in 0..=divisions {
        let fraction = index as f32 / divisions as f32;
        ui.painter().text(
            Pos2::new(plot.left() + plot.width() * fraction, ruler.top() + 2.),
            if index == 0 {
                Align2::LEFT_TOP
            } else if index == divisions {
                Align2::RIGHT_TOP
            } else {
                Align2::CENTER_TOP
            },
            time_label_precise(start + span * fraction as f64, decimals),
            FontId::proportional(12.),
            theme::MUTED,
        );
    }
    let mut seek = None;
    if (response.clicked() || response.dragged())
        && let Some(pointer) = response.interact_pointer_pos()
        && pointer.x >= plot.left()
    {
        seek = Some(at(pointer.x, plot, start, span, rate));
    }
    let any_solo = editor
        .document()
        .is_some_and(|document| document.lanes.iter().any(|lane| lane.solo));
    let mut rows = Vec::new();
    // Keep a real layout slot for the time scrollbar above the pinned transport.
    let lanes_height = (ui.available_height() - 32.).max(1.);
    let painted = egui::ScrollArea::vertical().id_salt("arrangement-lanes").auto_shrink([false, false]).max_height(lanes_height)
        .show_rows(ui, ROW_HEIGHT, lane_count, |ui, range| -> Result<()> {
            for index in range {
                let Some(lane) = editor.document().and_then(|document| document.lanes.get(index)) else { continue; };
                let id = lane.id.clone();
                let (mute, solo) = (lane.mute, lane.solo);
                let clips: Vec<Clip> = lane.clips.iter().filter(|clip|
                    clip.end_frame as f64 / rate as f64 >= start && clip.start_frame as f64 / rate as f64 <= start + span).cloned().collect();
                let selected = editor.selected_lane.as_ref() == Some(&id);
                let (row, _) = ui.allocate_exact_size(egui::vec2(ui.available_width(), ROW_HEIGHT), Sense::hover());
                rows.push((id.clone(), row));
                let painter = ui.painter().with_clip_rect(row.intersect(ui.clip_rect()));
                painter.rect_filled(row.shrink(1.), 4., if selected { Color32::from_rgb(40, 27, 53) } else { theme::SURFACE });
                let header = Rect::from_min_max(row.min + egui::vec2(6., 5.), Pos2::new(row.left() + header_width - 6., row.bottom() - 5.));
                let track_id = ui.make_persistent_id(("arrangement-track", &id));
                let mut controls = ui.new_child(egui::UiBuilder::new().scope_id(track_id).max_rect(header));
                controls.set_clip_rect(header.intersect(ui.clip_rect()));
                super::arrangement_lane_controls::show(editor, &mut controls, &id)?;
                let track_plot = Rect::from_min_max(Pos2::new(row.left() + header_width, row.top() + 5.), row.max - egui::vec2(3., 5.));
                let excluded = mute || (any_solo && !solo);
                if let Some(selection) = editor.selection && (selected || editor.eq_target == EqTarget::MixRange) {
                    let left = x(selection.start_frame as f64 / rate as f64, track_plot, start, span).max(track_plot.left());
                    let right = x(selection.end_frame as f64 / rate as f64, track_plot, start, span).min(track_plot.right());
                    if left < right { painter.rect_filled(Rect::from_x_y_ranges(left..=right, track_plot.y_range()), 0., theme::AMETHYST.gamma_multiply(0.2)); }
                }
                for clip in clips {
                    let left = x(clip.start_frame as f64 / rate as f64, track_plot, start, span);
                    let right = x(clip.end_frame as f64 / rate as f64, track_plot, start, span).max(left + 2.);
                    let original = Rect::from_min_max(Pos2::new(left, track_plot.top()), Pos2::new(right, track_plot.bottom()));
                    let rect = original.intersect(track_plot);
                    if !rect.is_positive() { continue; }
                    let chosen = editor.selected_clip.as_ref() == Some(&clip.id);
                    painter.rect_filled(rect, 4., if excluded { Color32::from_rgb(49, 43, 58) } else { Color32::from_rgb(80, 45, 112) });
                    painter.rect_stroke(rect, 4., Stroke::new(if chosen { 2. } else { 1. }, if chosen { theme::SLIME } else { theme::AMETHYST }), StrokeKind::Inside);
                    let title = clip.name.as_ref().or_else(|| editor.asset_names.get(&clip.source_id)).unwrap_or(&clip.source_id);
                    let header = Rect::from_min_max(rect.min + egui::vec2(5., 2.), Pos2::new(rect.right() - 4., rect.top() + CLIP_HEADER));
                    painter.with_clip_rect(header).text(header.left_center(), Align2::LEFT_CENTER, title, FontId::proportional(14.), theme::BONE);
                    let wave_rect = Rect::from_min_max(Pos2::new(rect.left() + 2., rect.top() + CLIP_HEADER + 2.), rect.max - egui::vec2(2., 3.));
                    let project_first = at(rect.left(), track_plot, start, span, rate).max(clip.start_frame);
                    let project_last = at(rect.right(), track_plot, start, span, rate).min(clip.end_frame);
                    waves.paint(&clip.source_id, &painter, wave_rect,
                        clip.source_start_frame + project_first - clip.start_frame,
                        clip.source_start_frame + project_last - clip.start_frame,
                        if excluded { theme::MUTED } else { Color32::from_rgb(208, 169, 239) });
                    let response = ui.interact(rect, ui.make_persistent_id(("arrangement-clip", &clip.id)), Sense::click_and_drag())
                        .on_hover_text(format!("{title}\nDrag title to move; edges trim; waveform selects a section. Right-click for clip actions."));
                    if response.clicked() { editor.select_clip(&clip.id)?; editor.eq_inspector = false; }
                    if response.drag_started() && let Some(pointer) = ui.input(|input| input.pointer.press_origin()) {
                        let previous_selection = SelectionSnapshot::capture(editor);
                        editor.select_clip(&clip.id)?;
                        let kind = if (pointer.x - original.left()).abs() <= 8. { Kind::TrimStart }
                            else if (pointer.x - original.right()).abs() <= 8. { Kind::TrimEnd }
                            else if editor.move_tool || pointer.y <= original.top() + CLIP_HEADER { Kind::Move } else { Kind::Select };
                        *gesture = Some(Gesture { kind, clip: clip.clone(), lane: id.clone(), pointer_start: pointer,
                            anchor_frame: at(pointer.x, track_plot, start, span, rate).clamp(clip.start_frame, clip.end_frame),
                            plot_width: track_plot.width(), span_seconds: span, rate, previous_selection });
                    }
                    let mut menu_result = Ok(());
                    response.context_menu(|ui| {
                        if ui.button("EQ this clip").clicked() {
                            menu_result = editor.select_clip(&clip.id);
                            if menu_result.is_ok() { editor.eq_target = EqTarget::Clip; editor.eq_inspector = true; editor.refresh_eq(); }
                            ui.close();
                        }
                        if ui.button("Delete clip").clicked() {
                            menu_result = editor.apply(Edit::DeleteClip { clip_id: clip.id.clone() }); ui.close();
                        }
                    });
                    menu_result?;
                }
                super::arrangement_eq_effects::markers(editor, ui, row, track_plot, &id, start, span);
                if playhead >= start && playhead <= start + span {
                    let position = x(playhead, track_plot, start, span);
                    painter.line_segment([Pos2::new(position, track_plot.top()), Pos2::new(position, track_plot.bottom())], Stroke::new(1.5, theme::SLIME));
                }
            }
            Ok(())
        });
    painted.inner?;
    arrangement_gestures::update(gesture, editor, &rows, ui)?;
    let maximum = (duration - span).max(0.);
    let mut position = start.min(maximum);
    if ui
        .add_enabled(
            maximum > 0.,
            egui::Slider::new(&mut position, 0. ..=maximum.max(0.001))
                .show_value(false)
                .text("Timeline view"),
        )
        .on_hover_text("Scroll the time view. This does not move clips or the playback position.")
        .changed()
    {
        editor.view_start = position;
        editor.view_end = position + span;
    }
    Ok(seek)
}
