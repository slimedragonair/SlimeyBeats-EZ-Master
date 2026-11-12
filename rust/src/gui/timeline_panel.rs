use super::{app::MasterCraftApp, theme};
use eframe::egui::{self, RichText};

pub fn show(app: &mut MasterCraftApp, ui: &mut egui::Ui) {
    let Some((_, result)) = app.current() else {
        return;
    };
    let eq = app.tab == 1;
    ui.scope(|ui| {
        ui.spacing_mut().interact_size.y = 20.;
        ui.horizontal_wrapped(|ui| {
            theme::eyebrow(
                ui,
                if eq {
                    "EQ PLAYBACK / CLICK TO SEEK"
                } else {
                    "PLAYBACK / CLICK TO SEEK"
                },
            );
            let detail = if eq && app.eq_dirty {
                "Live EQ · export measurement preparing".into()
            } else if let Some(report) = &result.report {
                format!(
                    "Processed {:.2}s · {} export{}",
                    report.processing_seconds,
                    report.exports.len(),
                    if report.exports.len() == 1 { "" } else { "s" }
                )
            } else {
                "Input analysis".into()
            };
            ui.label(RichText::new(detail).size(13.).color(theme::MUTED));
        });
    });
    let height = if ui.ctx().viewport_rect().height() < 800. {
        96.
    } else {
        120.
    };
    let seconds = if eq {
        app.eq_waveform.show_labeled(
            ui,
            app.player.seconds(),
            result.before.duration(),
            ("PRE EQ", "LATEST MEASURED EQ"),
            height,
        )
    } else {
        app.waveform.show_labeled(
            ui,
            app.player.seconds(),
            result.before.duration(),
            ("ORIGINAL", "MASTER"),
            height,
        )
    };
    if let Some(seconds) = seconds {
        app.player.seek(seconds);
    }
}
