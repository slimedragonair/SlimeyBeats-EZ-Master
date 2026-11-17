//! Inspect and revisit applied effects, including source sections hidden by a trim.
use super::arrangement_state::{ArrangementEditor, EqTarget};
use crate::{
    gui::theme,
    studio::arrangement::{Arrangement, Edit, EqScope, FrameRange, ScopedEq},
};
use anyhow::{Context, Result, ensure};
use eframe::egui;

pub(super) fn show(editor: &mut ArrangementEditor, ui: &mut egui::Ui) -> Result<()> {
    let effects = editor
        .document()
        .map(|document| document.eq.clone())
        .unwrap_or_default();
    if effects.is_empty() {
        return Ok(());
    }
    let mut action = None;
    egui::CollapsingHeader::new(format!("Applied EQ · {} effects", effects.len()))
        .id_salt("arrangement-applied-eq").show(ui, |ui| {
            for effect in effects {
                let description = editor.document().map(|document| description(document, &effect)).unwrap_or_default();
                let editable = editor.document().and_then(|document| project_range(document, &effect)).is_some();
                ui.push_id(&effect.id, |ui| {
                    ui.label(description);
                    ui.horizontal_wrapped(|ui| {
                        if ui.add_enabled(editable, egui::Button::new("Edit")).on_disabled_hover_text("This section is outside the current trim or song length. Move or extend its clip to edit the curve; bypass and removal remain available.").clicked() {
                            action = Some((0, effect.clone()));
                        }
                        if ui.button(if effect.bypass { "Enable" } else { "Bypass" }).clicked() { action = Some((1, effect.clone())); }
                        if ui.button("Remove").clicked() { action = Some((2, effect.clone())); }
                    });
                });
            }
        });
    if let Some((action, mut effect)) = action {
        match action {
            0 => select(editor, &effect)?,
            1 => {
                effect.bypass = !effect.bypass;
                editor.apply(Edit::SetEq { effect })?;
            }
            _ => editor.apply(Edit::RemoveEq { eq_id: effect.id })?,
        }
    }
    Ok(())
}

fn description(document: &Arrangement, effect: &ScopedEq) -> String {
    let target = match &effect.scope {
        EqScope::Mix {} => "Song".to_owned(),
        EqScope::Lane { lane_id } => document
            .lane(lane_id)
            .map(|lane| format!("Track: {}", lane.name))
            .unwrap_or_else(|_| lane_id.clone()),
        EqScope::Clip { clip_id } => document
            .clip(clip_id)
            .map(|clip| format!("Clip: {}", clip.name.as_deref().unwrap_or(clip_id)))
            .unwrap_or_else(|_| clip_id.clone()),
    };
    let interval = effect
        .range
        .map(|range| {
            format!(
                " · {:.3}–{:.3} s {}",
                range.start_frame as f64 / document.sample_rate as f64,
                range.end_frame as f64 / document.sample_rate as f64,
                if matches!(effect.scope, EqScope::Clip { .. }) {
                    "source"
                } else {
                    "song"
                }
            )
        })
        .unwrap_or_default();
    format!(
        "{target}{interval}{}",
        if effect.bypass { " · bypassed" } else { "" }
    )
}

pub(super) fn project_range(document: &Arrangement, effect: &ScopedEq) -> Option<FrameRange> {
    mapped_range(document, effect, false)
}

pub(super) fn audible_range(document: &Arrangement, effect: &ScopedEq) -> Option<FrameRange> {
    mapped_range(document, effect, true).filter(|range| range.end_frame > range.start_frame)
}

fn mapped_range(document: &Arrangement, effect: &ScopedEq, intersect: bool) -> Option<FrameRange> {
    match &effect.scope {
        EqScope::Clip { clip_id } => {
            let clip = document.clip(clip_id).ok()?;
            let Some(mut range) = effect.range else {
                return Some(FrameRange {
                    start_frame: clip.start_frame,
                    end_frame: clip.end_frame,
                });
            };
            if intersect {
                range.start_frame = range.start_frame.max(clip.source_start_frame);
                range.end_frame = range.end_frame.min(clip.source_end_frame().ok()?);
                if range.end_frame <= range.start_frame {
                    return None;
                }
            }
            if range.start_frame < clip.source_start_frame
                || range.end_frame > clip.source_end_frame().ok()?
            {
                return None;
            }
            Some(FrameRange {
                start_frame: clip.start_frame + range.start_frame - clip.source_start_frame,
                end_frame: clip.start_frame + range.end_frame - clip.source_start_frame,
            })
        }
        _ => {
            let mut range = effect.range.unwrap_or(FrameRange {
                start_frame: 0,
                end_frame: document.duration_frames(),
            });
            if intersect {
                range.end_frame = range.end_frame.min(document.duration_frames());
            }
            (range.end_frame <= document.duration_frames()).then_some(range)
        }
    }
}

pub(super) fn select(editor: &mut ArrangementEditor, effect: &ScopedEq) -> Result<()> {
    let document = editor.document().context("Import audio first")?;
    let range = project_range(document, effect)
        .context("This EQ section is outside the visible source trim")?;
    ensure!(
        range.end_frame > range.start_frame,
        "This effect has no audible section yet"
    );
    let ranged = effect.range.is_some();
    match &effect.scope {
        EqScope::Mix {} => {
            editor.eq_target = if ranged {
                EqTarget::MixRange
            } else {
                EqTarget::Mix
            }
        }
        EqScope::Lane { lane_id } => {
            editor.select_lane(lane_id)?;
            editor.eq_target = if ranged {
                EqTarget::LaneRange
            } else {
                EqTarget::Lane
            };
        }
        EqScope::Clip { clip_id } => {
            editor.select_clip(clip_id)?;
            editor.eq_target = if ranged {
                EqTarget::ClipRange
            } else {
                EqTarget::Clip
            };
        }
    }
    editor.selection = Some(range);
    editor.eq_id = Some(effect.id.clone());
    editor.refresh_eq();
    editor.eq_inspector = true;
    Ok(())
}

pub(super) fn markers(
    editor: &ArrangementEditor,
    ui: &egui::Ui,
    row: egui::Rect,
    plot: egui::Rect,
    lane_id: &str,
    view_start: f64,
    span: f64,
) {
    let Some(document) = editor.document() else {
        return;
    };
    for effect in &document.eq {
        let relevant = match &effect.scope {
            EqScope::Mix {} => true,
            EqScope::Lane { lane_id: target } => target == lane_id,
            EqScope::Clip { clip_id } => document
                .lane(lane_id)
                .is_ok_and(|lane| lane.clips.iter().any(|clip| &clip.id == clip_id)),
        };
        if !relevant {
            continue;
        }
        let Some(range) = audible_range(document, effect) else {
            continue;
        };
        let left = plot.left()
            + ((range.start_frame as f64 / document.sample_rate as f64 - view_start) / span) as f32
                * plot.width();
        let right = plot.left()
            + ((range.end_frame as f64 / document.sample_rate as f64 - view_start) / span) as f32
                * plot.width();
        let offset = match effect.scope {
            EqScope::Mix {} => 13.,
            EqScope::Lane { .. } => 9.,
            EqScope::Clip { .. } => 5.,
        };
        let band = egui::Rect::from_min_max(
            egui::pos2(left.max(plot.left()), row.bottom() - offset),
            egui::pos2(right.min(plot.right()), row.bottom() - offset + 3.),
        );
        if !band.is_positive() {
            continue;
        }
        let color = if effect.bypass {
            theme::MUTED
        } else {
            match effect.scope {
                EqScope::Mix {} => theme::AMETHYST,
                EqScope::Lane { .. } => theme::SLIME,
                EqScope::Clip { .. } => theme::BONE,
            }
        };
        ui.painter().rect_filled(band, 1., color);
        ui.interact(
            band,
            ui.make_persistent_id(("arrangement-eq-marker", lane_id, &effect.id)),
            egui::Sense::hover(),
        )
        .on_hover_text(description(document, effect));
    }
}
