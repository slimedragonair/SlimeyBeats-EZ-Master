use super::{
    spectral_view,
    state::{Page, Studio},
    transport,
};
use crate::gui::{crystal_button, theme};
use eframe::egui::{self, RichText};

/// Returns true when the user explicitly sends the prepared mix to mastering.
pub fn show(studio: &mut Studio, ui: &mut egui::Ui, ctx: &egui::Context) -> bool {
    let mut promote = false;
    let compact = ctx.viewport_rect().height() < 800.;
    ui.horizontal_wrapped(|ui| {
        if studio.busy() {
            ui.spinner();
            ui.add(egui::ProgressBar::new(studio.progress).desired_width(140.));
            if ui.add(crystal_button::secondary("Cancel")).clicked() {
                studio
                    .jobs
                    .cancel
                    .store(true, std::sync::atomic::Ordering::Relaxed);
            }
        }
        ui.label(
            egui::RichText::new(&studio.status)
                .color(theme::MUTED)
                .size(13.),
        );
    });
    ui.horizontal_wrapped(|ui| {
        for (page, name) in [
            (Page::Stems, if compact { "Stems" } else { "Stem Studio" }),
            (Page::Voice, if compact { "Voice" } else { "Voice cleanup" }),
            (
                Page::Repair,
                if compact { "Repair" } else { "Spectral repair" },
            ),
            (Page::Plugins, "Plugins"),
        ] {
            if ui
                .add(crystal_button::secondary(name).selected(studio.page == page))
                .clicked()
            {
                studio.page = page;
            }
        }
    });
    if let Some(error) = studio.error.clone() {
        egui::Frame::new()
            .fill(egui::Color32::from_rgb(62, 29, 44))
            .inner_margin(10)
            .show(ui, |ui| {
                ui.label(error);
                if ui.small_button("Dismiss").clicked() {
                    studio.error = None;
                }
            });
    }
    ui.horizontal_wrapped(|ui| {
        if ui.add_enabled(!studio.busy()&&!studio.undo.is_empty(),crystal_button::secondary("Undo")).clicked(){studio.history(false);}
        if ui.add_enabled(!studio.busy()&&!studio.redo.is_empty(),crystal_button::secondary("Redo")).clicked(){studio.history(true);}
        if ui.add_enabled(!studio.busy()&&studio.current.is_some()&&!studio.mix_dirty,crystal_button::secondary("Use for mastering")).on_hover_text("Add a private working copy to the mastering queue. This prepares another stage; it does not export a final delivery file.").clicked(){promote=true;}
    });
    let reserve = if compact { 170. } else { 195. };
    if let Some(analysis) = &studio.analysis {
        ui.label(format!(
            "Selected input · {:.1} LUFS · {:.2} dBTP · {} Hz",
            analysis.metrics.integrated_lufs, analysis.metrics.true_peak_dbtp, analysis.sample_rate
        ));
    }
    egui::ScrollArea::vertical().id_salt("studio-workspace").max_height((ui.available_height()-reserve).max(100.)).show(ui,|ui| {
        match studio.page {
            Page::Stems=>{
                ui.label(theme::display("Separate. Rebalance. Refine.",24.));
                ui.label("Prepare vocal/instrumental or four stems, compare the mix, and save when you are ready.");
                if studio.bank.is_none(){ui.add_space(12.);ui.label(RichText::new("01  Load a music source\n02  Separate locally\n03  Balance parts, compare, then export").color(theme::AMETHYST));}
                for (kind,waveform) in &studio.stem_waveforms {
                    ui.add_space(6.);theme::eyebrow(ui,kind.name());
                    waveform.show_labeled(ui,0.,studio.source.as_ref().map_or(1.,|s|s.audio.duration()),("STEM",""),76.);
                }
            },
            Page::Voice=>{
                ui.label(theme::display("A cleaner voice, on your terms.",24.));
                ui.label("Work on the original voice, a vocal stem or the current preview. Compare every change; previewing and exporting are separate.");
                ui.add_space(10.);ui.label("Rumble removal · learned noise reduction · de-essing · optional quiet-gap gate");
                ui.small("Change settings in the right panel, then Preview voice cleanup to apply them.");
            },
            Page::Repair=>{
                if !compact {ui.label(theme::display("Refine a precise moment.",24.));}
                spectral_view::show(studio,ui,if compact {150.}else{260.});
            },
            Page::Plugins=>{ui.label(theme::display("Your effects. Your workflow.",24.));ui.label("Choose a trusted VST3 effect, adjust its generic controls, and render a preview. The helper compensates reported latency and retains the source duration.");ui.small("Compatible mono/stereo float32 audio effects are supported. Native vendor editors and MIDI/instrument sequencing are outside this host's scope.");},
        }
        if let Some(preview)=&studio.current {
            ui.separator();ui.label(RichText::new(format!("Current preview · {}",preview.label)).color(theme::SLIME));
            for note in &preview.notes {ui.small(note);}
        }
    });
    ui.separator();
    if let Some(current) = &studio.current {
        if let Some(seconds) = studio.waveform.show_labeled(
            ui,
            studio.player.seconds(),
            current.audio.duration(),
            ("BEFORE", "PREVIEW"),
            if compact { 76. } else { 100. },
        ) {
            studio.player.seek(seconds);
        }
        ui.separator();
    }
    transport::show(studio, ui);
    promote
}
