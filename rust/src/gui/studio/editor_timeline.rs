//! Shared time coordinates and virtualized lanes; mixer controls keep their existing live path.
use super::{
    editor_envelope::Envelope,
    editor_state::time_label_precise,
    mix_controls,
    state::{Studio, Target},
};
use crate::{
    audio::Audio,
    gui::theme,
    studio::stem_mix::{StemKind, StemSet},
};
use eframe::egui::{self, Align2, FontId, Pos2, Rect, Sense, Stroke};
use std::{collections::BTreeMap, sync::Arc};

struct Cached {
    audio: Arc<Audio>,
    envelope: Envelope,
}
#[derive(Default)]
pub(super) struct Waveforms {
    bank: Option<Arc<StemSet>>,
    tracks: BTreeMap<StemKind, Cached>,
}
impl Waveforms {
    fn sync(&mut self, bank: &Arc<StemSet>) {
        if self.bank.as_ref().is_some_and(|old| Arc::ptr_eq(old, bank)) {
            return;
        }
        self.tracks.retain(|kind, _| bank.track(*kind).is_ok());
        for stem in &bank.tracks {
            if self
                .tracks
                .get(&stem.kind)
                .is_none_or(|entry| !Arc::ptr_eq(&entry.audio, &stem.audio))
            {
                self.tracks.insert(
                    stem.kind,
                    Cached {
                        audio: stem.audio.clone(),
                        envelope: Envelope::new(&stem.audio),
                    },
                );
            }
        }
        self.bank = Some(bank.clone());
    }
}

pub(super) fn show(studio: &mut Studio, ui: &mut egui::Ui, compact: bool) {
    let previous_target = studio.target;
    let Some(bank) = studio.bank.clone() else {
        return;
    };
    studio.editor_waveforms.sync(&bank);
    let duration = bank.tracks[0].audio.duration();
    let range = studio.editor.range(duration);
    let header_width = (ui.available_width() * 0.28).clamp(150., 220.);
    let row_height = if compact { 64. } else { 86. };
    let (ruler, response) = ui.allocate_exact_size(
        egui::vec2(ui.available_width(), 28.),
        Sense::click_and_drag(),
    );
    ui.painter().text(
        ruler.left_center(),
        Align2::LEFT_CENTER,
        "ALIGNED STEMS",
        FontId::proportional(11.),
        theme::MUTED,
    );
    let busy = studio.busy();
    let any_solo = studio.mix.tracks.iter().any(|track| track.solo);
    let position = studio.player.seconds();
    let mut changed = false;
    let mut loop_changed = false;
    // Only visible lanes allocate widgets or draw envelopes, even for a large imported bank.
    let lanes = egui::ScrollArea::vertical().id_salt("studio-timeline-lanes").auto_shrink([false, false]).show_rows(ui, row_height, studio.mix.tracks.len(), |ui, rows| {
        for index in rows {
            let track = &mut studio.mix.tracks[index];
            let kind = track.kind;
            let name = bank.display_name(kind);
            let selected = studio.editor.selected == Some(kind);
            let (row, _) = ui.allocate_exact_size(egui::vec2(ui.available_width(), row_height), Sense::hover());
            ui.painter().rect_filled(row.shrink(1.), 3., if selected { egui::Color32::from_rgb(39, 25, 53) } else { theme::SURFACE });
            let header = Rect::from_min_max(row.min + egui::vec2(5., 3.), Pos2::new(row.left() + header_width - 2., row.bottom() - 3.));
            let track_id = ui.make_persistent_id(("timeline-track", kind.name()));
            let mut controls = ui.new_child(egui::UiBuilder::new().scope_id(track_id).max_rect(header));
            controls.set_clip_rect(header.intersect(ui.clip_rect()));
            controls.spacing_mut().item_spacing.y = 2.;
            controls.spacing_mut().button_padding.y = 2.;
            controls.spacing_mut().interact_size.y = 22.;
            let label = controls.add(egui::Button::selectable(selected, egui::RichText::new(name.as_ref()).strong()).truncate());
            if label.on_hover_text(format!("Select {} for time edits and live level controls", name)).clicked() {
                studio.editor.selected = Some(kind);
                studio.target = Target::Stem(kind);
            }
            controls.add_enabled_ui(!busy, |ui| {
                ui.horizontal(|ui| { changed |= mix_controls::switches(track, ui, name.as_ref()); });
            });
            let lane = Rect::from_min_max(Pos2::new(row.left() + header_width + 6., row.top() + 3.), row.max - egui::vec2(1., 3.));
            let excluded = if track.mute { Some("MUTED") } else if any_solo && !track.solo { Some("OUTSIDE SOLO") } else { None };
            paint_lane(ui, lane, studio.editor_waveforms.tracks.get(&kind), LaneView { range, position, selection: selected.then_some(studio.editor.selection).flatten(), excluded, waveform_scale: studio.editor.waveform_scale });
            let response = ui.interact(lane, ui.make_persistent_id(("timeline-audio", kind.name())), Sense::click_and_drag());
            if response.clicked() && let Some(pos) = response.interact_pointer_pos() {
                studio.editor.selected = Some(kind);
                studio.target = Target::Stem(kind);
                studio.player.seek(time_at(pos.x, lane, range));
            }
            if response.drag_started() && let Some(pos) = ui.input(|input| input.pointer.press_origin()) {
                studio.editor.drag_anchor = Some(time_at(pos.x, lane, range));
                studio.editor.selected = Some(kind);
                studio.target = Target::Stem(kind);
            }
            if response.dragged() && let (Some(anchor), Some(pos)) = (studio.editor.drag_anchor, response.interact_pointer_pos()) {
                studio.editor.select_time(anchor, time_at(pos.x, lane, range), bank.tracks[0].audio.sample_rate, bank.tracks[0].audio.frames());
                loop_changed = true;
                studio.editor.inspector_edits = true;
            }
            if response.drag_stopped() {
                studio.editor.drag_anchor = None;
                studio.editor.reveal_edits = true;
            }
            response.on_hover_text(format!("{} waveform. Click to seek; drag to select a time range on this stem. Waveforms show bank audio before live fader gain.", name));
        }
    });
    // Use the scroll area's actual content width: its scrollbar can otherwise
    // shift the ruler's time scale away from the waveforms in compact windows.
    let plot = Rect::from_min_max(
        Pos2::new(lanes.inner_rect.left() + header_width + 6., ruler.top()),
        Pos2::new(lanes.inner_rect.right() - 1., ruler.bottom()),
    );
    ruler_ticks(ui, plot, range);
    if (response.clicked() || response.dragged())
        && let Some(pos) = response.interact_pointer_pos()
        && pos.x >= plot.left()
    {
        studio.player.seek(time_at(pos.x, plot, range));
    }
    if changed {
        studio.monitor_mix_changes();
    }
    if loop_changed {
        studio.update_editor_loop();
    }
    if studio.target != previous_target {
        studio.doctor.invalidate();
    }
}

fn time_at(x: f32, rect: Rect, (start, end): (f64, f64)) -> f64 {
    start + ((x - rect.left()) / rect.width().max(1.)).clamp(0., 1.) as f64 * (end - start)
}
fn time_x(time: f64, rect: Rect, (start, end): (f64, f64)) -> f32 {
    rect.left() + ((time - start) / (end - start)) as f32 * rect.width()
}
fn ruler_ticks(ui: &egui::Ui, rect: Rect, range: (f64, f64)) {
    let divisions = (rect.width() / 100.).floor().max(1.) as usize;
    let painter = ui.painter().with_clip_rect(rect);
    let decimals = if range.1 - range.0 < 0.1 {
        4
    } else if range.1 - range.0 < 1. {
        3
    } else {
        2
    };
    for i in 0..=divisions {
        let time = range.0 + (range.1 - range.0) * i as f64 / divisions as f64;
        let x = time_x(time, rect, range);
        painter.text(
            Pos2::new(x, rect.top() + 2.),
            if i == 0 {
                Align2::LEFT_TOP
            } else if i == divisions {
                Align2::RIGHT_TOP
            } else {
                Align2::CENTER_TOP
            },
            time_label_precise(time, decimals),
            FontId::proportional(11.),
            theme::MUTED,
        );
    }
}
struct LaneView<'a> {
    range: (f64, f64),
    position: f64,
    selection: Option<(f64, f64)>,
    excluded: Option<&'a str>,
    waveform_scale: f32,
}
fn paint_lane(ui: &egui::Ui, rect: Rect, cache: Option<&Cached>, view: LaneView<'_>) {
    let LaneView {
        range,
        position,
        selection,
        excluded,
        waveform_scale,
    } = view;
    let painter = ui.painter().with_clip_rect(rect);
    painter.rect_filled(rect, 2., theme::CANVAS);
    if let Some((start, end)) = selection {
        painter.rect_filled(
            Rect::from_min_max(
                Pos2::new(time_x(start, rect, range), rect.top()),
                Pos2::new(time_x(end, rect, range), rect.bottom()),
            ),
            0.,
            egui::Color32::from_rgba_unmultiplied(159, 77, 222, 65),
        );
    }
    painter.hline(
        rect.x_range(),
        rect.center().y,
        Stroke::new(0.5, theme::BORDER),
    );
    if let Some(cache) = cache {
        let pixels = rect.width().ceil().max(1.) as usize;
        for column in 0..pixels {
            let start = ((range.0 + (range.1 - range.0) * column as f64 / pixels as f64)
                * cache.audio.sample_rate as f64)
                .floor() as usize;
            let end = ((range.0 + (range.1 - range.0) * (column + 1) as f64 / pixels as f64)
                * cache.audio.sample_rate as f64)
                .ceil() as usize;
            let peak = (cache.envelope.peak(&cache.audio, start, end) * waveform_scale).min(1.);
            let half = peak * (rect.height() * 0.46);
            let x = rect.left() + column as f32;
            painter.vline(
                x,
                (rect.center().y - half)..=(rect.center().y + half),
                Stroke::new(
                    1.,
                    if excluded.is_some() {
                        theme::MUTED
                    } else {
                        theme::SLIME
                    },
                ),
            );
        }
    }
    if (range.0..=range.1).contains(&position) {
        painter.vline(
            time_x(position, rect, range),
            rect.y_range(),
            Stroke::new(1.5, theme::BONE),
        );
    }
    if let Some(label) = excluded {
        painter.text(
            rect.right_top() + egui::vec2(-5., 3.),
            Align2::RIGHT_TOP,
            label,
            FontId::proportional(11.),
            theme::MUTED,
        );
    }
}
