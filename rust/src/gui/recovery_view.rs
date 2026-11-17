//! Recovery settings and inspection of inactive sessions, separate from projects/exports.
use super::{app::MasterCraftApp, crystal_button, theme};
use eframe::egui;
use std::time::{SystemTime, UNIX_EPOCH};

pub(super) fn age(milliseconds: u64) -> String {
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis()
        .min(u64::MAX as u128) as u64;
    let seconds = now.saturating_sub(milliseconds) / 1000;
    match seconds {
        0..=59 => "less than a minute ago".into(),
        60..=3599 => format!("{} min ago", seconds / 60),
        3600..=86399 => format!("{} h ago", seconds / 3600),
        _ => format!("{} days ago", seconds / 86400),
    }
}
pub(super) fn show(app: &mut MasterCraftApp, ui: &mut egui::Ui, ctx: &egui::Context) {
    ui.strong("Autosave and recovery");
    ui.label("Local copies protect loaded audio, previews and pending controls. Restoring is separate from exporting and never starts playback.");
    ui.label(&app.recovery.message);
    if let Some(error) = &app.recovery.error {
        ui.colored_label(theme::BONE, error);
    }
    if let Some(error) = &app.recovery.preference_error {
        ui.colored_label(theme::BONE, error);
    }
    if !app.recovery.listing.checkpoints.is_empty() {
        history(app, ui, ctx);
        ui.separator();
        ui.collapsing("Autosave settings and this window", |ui| {
            settings(app, ui, ctx)
        });
    } else {
        settings(app, ui, ctx);
        history(app, ui, ctx);
    }
}

fn settings(app: &mut MasterCraftApp, ui: &mut egui::Ui, ctx: &egui::Context) {
    let working = app.recovery.busy();
    let state = &mut app.recovery;
    if ui.checkbox(&mut state.preferences.enabled,"Autosave locally in this window").on_hover_text("Remember this choice for future launches. Changes settle briefly before saving in the background; in-progress processing jobs are not resumed.").changed() {
        if !state.preferences.enabled && state.busy(){state.cancel();}state.persist(false);
    }
    if ui
        .checkbox(
            &mut state.preferences.offer_on_startup,
            "Offer interrupted sessions when starting an empty window",
        )
        .changed()
    {
        state.persist(false);
    }
    ui.small("Keeps three recovery points per session, within an 8 GiB shared storage budget. Unchanged audio is reused. Storage errors stop recovery saves, not editing. Turning autosave off does not delete existing copies.");
    if let Some(path) = &state.root {
        ui.collapsing("Local recovery folder", |ui| {
            ui.add(egui::Label::new(path.display().to_string()).wrap());
        });
    }
    if let Some(saved) = &state.last_saved {
        ui.label(format!(
            "Current session: saved {} · point {}",
            age(saved.captured_unix_ms),
            saved.checkpoint.generation()
        ));
    }
    if let Some(error) = &state.preference_error {
        ui.colored_label(theme::BONE, error);
        if ui
            .add(crystal_button::secondary("Save recovery preferences"))
            .clicked()
        {
            state.persist(true);
        }
    }
    ui.horizontal_wrapped(|ui| {
        if ui
            .add_enabled(
                !working,
                crystal_button::secondary("Refresh recovery history"),
            )
            .clicked()
        {
            state.refresh(ctx);
        }
        if working
            && ui
                .add(crystal_button::secondary("Cancel recovery operation"))
                .clicked()
        {
            state.cancel();
        }
    });
    if ui
        .add_enabled(
            !working && app.has_recovery_work(),
            crystal_button::primary("Save recovery copy now"),
        )
        .clicked()
    {
        app.save_recovery_now(ctx);
    }
    if ui
        .add_enabled(
            !app.recovery.restoring,
            crystal_button::secondary("Stop autosave and clear this window's copies…"),
        )
        .clicked()
    {
        app.recovery.confirm_clear_current = true;
    }
    if app.recovery.confirm_clear_current {
        ui.label("Turn autosave off and delete this window's recovery points? Your open audio remains editable. Earlier, inactive sessions are managed below.");
        ui.horizontal_wrapped(|ui| {
            if ui
                .add(crystal_button::secondary("Keep this window's copies"))
                .clicked()
            {
                app.recovery.confirm_clear_current = false;
            }
            if ui
                .add(crystal_button::secondary("Stop autosave and clear copies"))
                .clicked()
            {
                app.recovery.request_clear_current();
            }
        });
    }
}

fn history(app: &mut MasterCraftApp, ui: &mut egui::Ui, ctx: &egui::Context) {
    let mut reveal_confirmation = false;
    ui.separator();
    let history = ui.strong("Earlier, inactive sessions");
    if app.recovery.reveal_history {
        history.scroll_to_me(Some(egui::Align::TOP));
        app.recovery.reveal_history = false;
    }
    ui.small("Active windows are excluded. Select a point, inspect its details, then choose Restore. Existing work is copied first. This does not export files or start playback.");
    let state = &mut app.recovery;
    if state.listing.checkpoints.is_empty() {
        ui.label("No earlier recovery copies are available.");
    }
    egui::ScrollArea::vertical()
        .id_salt("recovery-point-selector")
        .max_height(180.)
        .show(ui, |ui| {
            ui.add_enabled_ui(!state.busy(), |ui| {
                for (index, point) in state.listing.checkpoints.iter().enumerate() {
                    let label = format!(
                        "{} · {} · point {} · {:.1} MiB decoded",
                        index + 1,
                        age(point.captured_unix_ms),
                        point.checkpoint.generation(),
                        point.decoded_audio_bytes as f64 / 1048576.
                    );
                    if ui
                        .radio_value(&mut state.selected, Some(point.checkpoint.clone()), label)
                        .changed()
                    {
                        state.inspected = None;
                        state.confirm_restore = false;
                        state.confirm_delete = false;
                    }
                }
            });
        });
    for warning in &state.listing.warnings {
        ui.colored_label(theme::BONE, warning);
    }
    if state.selected.is_some() {
        if ui
            .add_enabled(
                !state.busy(),
                crystal_button::secondary("Inspect selected recovery copy"),
            )
            .clicked()
        {
            state.inspect(ctx);
        }
        ui.horizontal_wrapped(|ui| {
            if ui
                .add_enabled(
                    !state.busy() && state.inspected.is_some(),
                    crystal_button::primary("Restore selected session…"),
                )
                .clicked()
            {
                state.confirm_restore = true;
                reveal_confirmation = true;
            }
            if ui
                .add_enabled(
                    !state.busy(),
                    crystal_button::secondary("Delete this recovery session…"),
                )
                .clicked()
            {
                state.confirm_delete = true;
            }
        });
        if let Some(payload) = &state.inspected {
            let tracks = payload.get("tracks").and_then(|v| v.as_array());
            let count = tracks.map_or(0, |t| t.len());
            let loaded = tracks.map_or(0, |items| {
                items
                    .iter()
                    .filter(|t| t.get("result").is_some_and(|v| !v.is_null()))
                    .count()
            });
            let stems = payload
                .pointer("/studio/bank/stems")
                .and_then(|v| v.as_array())
                .map_or(0, |s| s.len());
            ui.label(format!("{count} queued mix(es) · {stems} Studio stems"));
            ui.small(format!("Audio saved for {loaded} of {count} queued mixes. Items not loaded yet still need their original files."));
            if let Some(tracks) = tracks {
                for track in tracks.iter().take(8) {
                    if let Some(path) = track.get("path").and_then(|v| v.as_str()) {
                        let name = std::path::Path::new(path)
                            .file_name()
                            .unwrap_or_default()
                            .to_string_lossy();
                        ui.label(name);
                    }
                }
            }
            ui.small("Names are shown only here, locally. Recovery details are never attached to support reports. Reference tracks stay linked; reselect them if moved. Timeline committed edits and retained undo/redo are restored; other tool undo histories and running jobs are not.");
        }
        if state.confirm_delete {
            ui.label("Delete all recovery points for this inactive session? Original audio, projects and exports are kept.");
            ui.horizontal_wrapped(|ui| {
                if ui
                    .add(crystal_button::secondary("Keep recovery copies"))
                    .clicked()
                {
                    state.confirm_delete = false;
                }
                if ui
                    .add_enabled(
                        !state.busy(),
                        crystal_button::secondary("Delete recovery copies"),
                    )
                    .clicked()
                {
                    state.delete_selected(ctx);
                    state.confirm_delete = false;
                }
            });
        }
    }
    if app.recovery.confirm_restore {
        let confirmation=ui.group(|ui|{
            ui.strong("Restore this session?");ui.label("A recovery copy of your current work is saved first. Playback will stop. Audio and project exports remain a separate action.");
            ui.horizontal_wrapped(|ui|{
                let keep=ui.add(crystal_button::secondary("Keep editing this session"));if reveal_confirmation{keep.request_focus();}
                if keep.clicked(){app.recovery.confirm_restore=false;}
                if ui.add_enabled(!app.busy && !app.studio.busy() && !app.recovery.busy(),crystal_button::primary("Back up current work and restore")).clicked()
                    && let Some(key)=app.recovery.selected.clone() {app.request_recovery_restore(key,ctx);}
            });
        });
        if reveal_confirmation {
            confirmation
                .response
                .scroll_to_me(Some(egui::Align::BOTTOM));
        }
    }
}
