//! Two bounded header rows keep track selection, EQ, mute and solo visible.
use super::arrangement_state::Inspector;
use super::arrangement_state::{ArrangementEditor, EqTarget};
use crate::studio::arrangement::Edit;
use anyhow::{Context, Result};
use eframe::egui;

pub(super) fn show(editor: &mut ArrangementEditor, ui: &mut egui::Ui, lane_id: &str) -> Result<()> {
    let lane = editor
        .document()
        .context("Import audio first")?
        .lane(lane_id)?;
    let (name, mute, solo) = (lane.name.clone(), lane.mute, lane.solo);
    let selected = editor.selected_lane.as_deref() == Some(lane_id);
    ui.spacing_mut().item_spacing.x = 4.;
    ui.spacing_mut().button_padding.x = 4.;
    ui.horizontal(|ui| -> Result<()> {
        let name_width = (ui.available_width() - 37.).max(24.);
        if ui
            .add_sized(
                egui::vec2(name_width, 24.),
                egui::Button::selectable(selected, egui::RichText::new(&name).strong()).truncate(),
            )
            .on_hover_text(&name)
            .clicked()
        {
            editor.select_lane(lane_id)?;
            editor.inspector = Inspector::Track;
        }
        if ui
            .small_button("EQ")
            .on_hover_text("EQ this track, or only its selected time range.")
            .clicked()
        {
            editor.select_lane(lane_id)?;
            editor.eq_target = EqTarget::Lane;
            editor.inspector = Inspector::Eq;
            editor.refresh_eq();
        }
        Ok(())
    })
    .inner?;
    ui.horizontal(|ui| -> Result<()> {
        if ui
            .selectable_label(mute, "Mute")
            .on_hover_text("Silence this track, including when it is soloed.")
            .clicked()
        {
            editor.apply(Edit::SetLane {
                lane_id: lane_id.into(),
                name: None,
                gain_db: None,
                pan: None,
                mute: Some(!mute),
                solo: None,
            })?;
        }
        if ui
            .selectable_label(solo, "Solo")
            .on_hover_text("Listen only to soloed tracks. Muted tracks stay silent.")
            .clicked()
        {
            editor.apply(Edit::SetLane {
                lane_id: lane_id.into(),
                name: None,
                gain_db: None,
                pan: None,
                mute: None,
                solo: Some(!solo),
            })?;
        }
        Ok(())
    })
    .inner?;
    Ok(())
}
