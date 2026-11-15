//! Startup offers and a visible, cancellable close-time save.
use super::{app::MasterCraftApp, crystal_button, learning_ui, user_experience::HelpPage};
use eframe::egui;

pub(super) fn startup(app: &mut MasterCraftApp, ctx: &egui::Context) {
    if !app.recovery.offer {
        return;
    }
    let mut review = false;
    let mut later = false;
    let response=learning_ui::dialog(egui::Id::unique("recovery-available"),"Recovery copies available",ctx).show(ctx,|ui|{
        ui.set_width(learning_ui::dialog_width(ctx).min(560.));
        ui.heading("Your earlier work has a recovery copy");
        ui.label("An earlier session ended before it was marked finished. You can review its saved mixes, previews and controls before restoring. Originals and exports are untouched.");
        ui.horizontal_wrapped(|ui|{
            let button=ui.add(crystal_button::primary("Review recovery copies"));if app.recovery.offer_focus{button.request_focus();}review=button.clicked();
            later=ui.add(crystal_button::secondary("Keep in recovery history")).clicked();
        });
        ui.small("Keeping history closes this offer and remembers your choice. Copies remain available in Help → Recovery.");
    });
    app.recovery.offer_focus = false;
    if review {
        app.recovery.offer = false;
        app.recovery.reveal_history = true;
        app.experience.open_help(HelpPage::Recovery, ctx);
    }
    if later || response.should_close() {
        app.recovery.keep_for_later();
    }
}
pub(super) fn blocking(app: &mut MasterCraftApp, ui: &mut egui::Ui, ctx: &egui::Context) -> bool {
    if !app.recovery.closing && !app.recovery.restoring {
        return false;
    }
    app.pause_for_recovery();
    ui.painter()
        .rect_filled(ui.max_rect(), 0., super::theme::CANVAS);
    let response=learning_ui::dialog(egui::Id::unique("recovery-operation"),"Recovery operation",ctx).show(ctx,|ui|{
        ui.set_width(learning_ui::dialog_width(ctx).min(560.));
        ui.heading(if app.recovery.closing_for_clear {"Finishing removal of recovery copies"}else if app.recovery.closing_for_restore {"Finishing backup and recovery before closing"}else if app.recovery.closing {"Finishing your local recovery copy"}else{"Restoring your session"});
        ui.label(&app.recovery.message);
        if app.recovery.busy(){ui.spinner();ui.small("Large first copies can take time. Later snapshots reuse unchanged audio.");}
        if let Some(error)=&app.recovery.error {ui.label(error);}
        ui.horizontal_wrapped(|ui|{
            let cancel=ui.add(crystal_button::secondary(if app.recovery.closing {"Cancel close and keep editing"}else{"Cancel restore"}));
            if app.recovery.block_focus {cancel.request_focus();}
            if cancel.clicked(){
                if app.recovery.closing {app.recovery.closing=false;app.recovery.closing_for_clear=false;app.recovery.closing_for_restore=false;}else{app.recovery.cancel();}
            }
            if app.recovery.closing && ui.add(crystal_button::secondary(if app.recovery.closing_for_clear {"Close without finishing removal"}else{"Close without finishing recovery"})).clicked(){
                app.recovery.cancel();app.recovery.allow_close=true;ctx.send_viewport_cmd(egui::ViewportCommand::Close);
            }
        });
        ui.small("Only completed recovery copies are available after an unexpected shutdown. Save a project or export important work too.");
        ui.small("Running processing jobs stop when the app closes; recovery restores completed previews and pending controls, not running jobs.");
    });
    app.recovery.block_focus = false;
    if response.should_close() {
        if app.recovery.closing {
            app.recovery.closing = false;
            app.recovery.closing_for_clear = false;
            app.recovery.closing_for_restore = false;
        } else {
            app.recovery.cancel();
        }
    }
    ui.ctx()
        .request_repaint_after(super::recovery_state::PROBE_INTERVAL);
    true
}
