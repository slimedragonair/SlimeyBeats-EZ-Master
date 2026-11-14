//! Native update and optional-tool page in Help; background work stays outside the renderer.
use super::{app::MasterCraftApp, crystal_button, theme};
use crate::app_update::{self, UpdateProgress};
use eframe::egui;

pub(super) fn show(app: &mut MasterCraftApp, ui: &mut egui::Ui, ctx: &egui::Context) {
    let working_audio = !app.tracks.is_empty() || app.studio.source.is_some();
    let audio_busy = app.busy || app.studio.busy() || app.experience.mcp.busy();
    let state = &mut app.updates;
    ui.strong("Keep SlimeyBeats up to date");
    let current = app_update::current();
    ui.label(format!(
        "Running: {} · build {}",
        current.version, current.build_revision
    ));
    ui.label("Downloads are verified before Setup opens. Your export folders, projects and saved settings are kept.");
    if cfg!(windows) && !super::update_launch::managed_installation() {
        ui.small("Portable/source copy: Setup installs or updates the registered Windows copy. To stay portable, get the portable ZIP from the release page; this copy is not replaced automatically.");
    }
    if ui.checkbox(&mut state.preferences.automatic, "Check for updates automatically").on_hover_text("Check GitHub at most once a day. No audio or project data is sent, and updates never install automatically.").changed() {
        state.persist(false);
    }
    if ui
        .add_enabled(
            !state.busy(),
            crystal_button::secondary("Check for updates"),
        )
        .clicked()
    {
        state.check(ctx);
    }
    let status = ui.label(&state.message);
    if state.reveal_progress {
        status.scroll_to_me(Some(egui::Align::Center));
        state.reveal_progress = false;
    }
    if let Some(progress) = &state.progress {
        match progress {
            UpdateProgress::Downloading {
                received_bytes,
                total_bytes,
            } => {
                ui.add(
                    egui::ProgressBar::new(
                        (*received_bytes as f64 / (*total_bytes).max(1) as f64) as f32,
                    )
                    .text(format!(
                        "{} / {} MiB",
                        received_bytes / 1048576,
                        total_bytes / 1048576
                    )),
                );
            }
            UpdateProgress::CheckingRelease => {
                ui.spinner();
                ui.small("Reading the official release…");
            }
            UpdateProgress::ReadingManifest => {
                ui.spinner();
                ui.small("Checking update details…");
            }
            UpdateProgress::Verifying => {
                ui.spinner();
                ui.small("Verifying the downloaded installer…");
            }
            UpdateProgress::Ready => {}
        }
    }
    if let Some(fraction) = state.tool_fraction {
        ui.add(egui::ProgressBar::new(fraction.clamp(0., 1.)));
    }
    if state.busy() && !state.opening_setup && ui.add(crystal_button::secondary("Cancel")).clicked()
    {
        state.cancel();
    }
    if let Some(candidate) = &state.available {
        ui.separator();
        ui.label(
            egui::RichText::new(format!(
                "Available: {} · build {}",
                candidate.version(),
                candidate.build_revision()
            ))
            .color(theme::AMETHYST),
        );
        if state.downloaded.is_none()
            && ui
                .add_enabled(!state.busy(), crystal_button::primary("Download update"))
                .clicked()
        {
            state.download(ctx);
        }
        ui.collapsing("What's new in this build", |ui| {
            if let Some(candidate) = &state.available {
                ui.label(candidate.notes());
            }
        });
    }
    if state.downloaded.is_some() {
        if state.confirm_install {
            let reminder = ui.group(|ui| {
                ui.strong("Save your audio session first");
                ui.label("Setup will open and this app will close. Export any masters and use Save project for Studio work before continuing. Close other SlimeyBeats windows and AI connections before Setup copies files.");
                ui.horizontal_wrapped(|ui| {
                    let back = ui.add(crystal_button::secondary("Go back and save"));
                    if state.reveal_confirmation { back.request_focus(); }
                    if back.clicked() { state.confirm_install = false; }
                    if ui.add_enabled(!state.busy() && !audio_busy, crystal_button::primary("Work saved · open Setup")).clicked() { state.launch(ctx); }
                });
            });
            if state.reveal_confirmation { reminder.response.scroll_to_me(Some(egui::Align::BOTTOM)); state.reveal_confirmation = false; }
        } else if ui.add_enabled(!state.busy() && !audio_busy, crystal_button::primary("Install update…")).on_hover_text("Open the verified installer, then close this app. Setup retains the installed folder and can launch the new build when finished.").clicked() {
            if working_audio { state.confirm_install = true; state.reveal_confirmation = true; } else { state.launch(ctx); }
        }
        if audio_busy {
            ui.small("Finish or cancel current audio work before installing.");
        }
    }
    if let Some(error) = &state.error {
        ui.label(egui::RichText::new(error).color(theme::BONE));
    }
    if let Some(error) = &state.save_error {
        ui.label(egui::RichText::new(error).color(theme::BONE));
        if ui
            .add(crystal_button::secondary("Save update preferences"))
            .clicked()
        {
            state.persist(true);
        }
    }
    ui.hyperlink_to(
        "Open the latest release page",
        app_update::MANUAL_RELEASE_URL,
    );
    ui.separator();
    ui.strong("Optional audio tools");
    ui.label("FFmpeg is prepared automatically when you open compressed audio, export MP3/FLAC or change sample rate. Native WAV needs no download.");
    if ui
        .add_enabled(
            !state.busy() && !audio_busy,
            crystal_button::secondary("Set up audio codecs now"),
        )
        .clicked()
    {
        state.prepare_codecs(ctx);
    }
    ui.label("Stem separation prepares its private engine and model on first use. Existing stems, voice cleanup and spectral repair work without that model.");
    ui.small("Optional setup needs internet and disk space; progress and Cancel stay available. Audio processing stays on your computer.");
}
