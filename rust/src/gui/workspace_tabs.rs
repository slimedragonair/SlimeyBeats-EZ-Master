//! Keep named workspace tabs in one row when platform fonts use more space.
use super::{app::MasterCraftApp, crystal_button, skin_icons::Icon};
use eframe::egui;

const TABS: [(usize, &str, Icon); 4] = [
    (0, "Overview", Icon::Home),
    (1, "Manual EQ", Icon::Sliders),
    (2, "Processing report", Icon::Report),
    (3, "Studio", Icon::Layers),
];

fn button(
    label: &str,
    icon: Option<Icon>,
    selected: bool,
    compact: bool,
) -> crystal_button::CrystalButton {
    let button = crystal_button::secondary(label)
        .selected(selected)
        .min_size(egui::vec2(0., if compact { 32. } else { 36. }));
    if let Some(icon) = icon {
        button.icon(icon)
    } else {
        button
    }
}

fn measured_width(ui: &mut egui::Ui, short_report: bool, icons: bool, compact: bool) -> f32 {
    let mut measure = ui.new_child(
        egui::UiBuilder::new()
            .id_salt(("workspace-tab-measure", short_report, icons))
            .sizing_pass()
            .invisible(),
    );
    measure.disable();
    measure
        .horizontal(|ui| {
            for (tab, label, icon) in TABS {
                let label = if tab == 2 && short_report {
                    "Report"
                } else {
                    label
                };
                ui.add(button(label, icons.then_some(icon), false, compact));
            }
        })
        .response
        .rect
        .width()
}

pub(super) fn show(app: &mut MasterCraftApp, ui: &mut egui::Ui, compact: bool) {
    let available = ui.available_width() - 1. / ui.pixels_per_point();
    let short_report = measured_width(ui, false, true, compact) > available;
    let icons = !short_report || measured_width(ui, true, true, compact) <= available;
    ui.horizontal_wrapped(|ui| {
        for (tab, label, icon) in TABS {
            let label = if tab == 2 && short_report {
                "Report"
            } else {
                label
            };
            if ui
                .add(button(
                    label,
                    icons.then_some(icon),
                    app.tab == tab,
                    compact,
                ))
                .clicked()
            {
                if tab == 3 {
                    app.player.stop();
                } else {
                    app.studio.player.stop();
                }
                app.tab = tab;
            }
        }
    });
}
