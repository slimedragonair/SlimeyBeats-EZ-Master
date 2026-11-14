use super::{
    app::MasterCraftApp, crystal_button, guide_content::guide, learning_ui, theme,
    user_experience::HelpPage,
};
use eframe::egui::{self, Id, RichText};

pub(super) fn show(app: &mut MasterCraftApp, ctx: &egui::Context) {
    let Some(page) = app.experience.help_page else {
        return;
    };
    let mut close = false;
    let response = learning_ui::dialog(Id::unique("help-and-learning"), "Help & learning", ctx)
        .show(ctx, |ui| {
            ui.set_width(learning_ui::dialog_width(ctx));
            ui.label(theme::display("Help & learning", 26.));
            ui.add_space(8.);
            ui.horizontal_wrapped(|ui| {
                for (value, label) in [
                    (HelpPage::Start, "Quick start"),
                    (HelpPage::Terms, "Terms"),
                    (HelpPage::Studio, "Studio"),
                    (HelpPage::Assistant, "AI setup"),
                    (HelpPage::Troubleshooting, "Troubleshooting"),
                    (HelpPage::About, "About"),
                ] {
                    let response = ui.add(crystal_button::secondary(label).selected(page == value));
                    if app.experience.request_dialog_focus && value == page {
                        response.request_focus();
                    }
                    if response.clicked() {
                        app.experience.help_page = Some(value);
                    }
                }
            });
            ui.separator();
            egui::ScrollArea::vertical()
                .id_salt(("help-content", page))
                .auto_shrink([false, false])
                .max_height((ctx.content_rect().height() - 270.).clamp(180., 540.))
                .show(ui, |ui| {
                    ui.set_width(ui.available_width());
                    match page {
                        HelpPage::Start => quick_start(app, ui),
                        HelpPage::Terms => terms(app, ui),
                        HelpPage::Studio => studio(ui),
                        HelpPage::Assistant => assistant(app, ui, ctx),
                        HelpPage::Troubleshooting => troubleshooting(ui),
                        HelpPage::About => about(ui),
                    }
                    if let Some(error) = app.experience.save_error.clone() {
                        ui.separator();
                        ui.label(RichText::new(error).color(theme::BONE));
                        if ui
                            .add(crystal_button::secondary(
                                "Save current interface preferences",
                            ))
                            .clicked()
                        {
                            app.experience.persist(true, true);
                        }
                    }
                });
            ui.separator();
            if ui.add(crystal_button::secondary("Close help")).clicked() {
                close = true;
            }
        });
    app.experience.request_dialog_focus = false;
    if close || response.should_close() {
        app.experience.close_help(ctx);
    }
}

fn quick_start(app: &mut MasterCraftApp, ui: &mut egui::Ui) {
    ui.label(
        RichText::new("A clear path to a finished file")
            .strong()
            .size(19.),
    );
    ui.label("Add a mix → Analyze → Master → compare and adjust → Export.");
    ui.add_space(7.);
    ui.label("Master creates a preview. Export saves a file. You can listen, change EQ or re-master as often as you need before exporting.");
    ui.add_space(8.);
    if ui
        .add(crystal_button::primary("Replay the quick tour"))
        .clicked()
    {
        app.experience.replay_tour();
    }
    ui.add_space(10.);
    learning_ui::mode_selector(&mut app.experience, ui);
    ui.label("Basic keeps the common choices in view. Advanced reveals precise targets and less frequent controls. Switching does not reset your sound settings.");
    ui.separator();
    ui.strong("Keyboard shortcuts");
    ui.label("F1 opens Help. Space plays or pauses outside focused controls. Ctrl+O adds audio to the mastering queue. Escape closes a help or tutorial dialog.");
    ui.separator();
    ui.label("Export folders and preferences are remembered. Use Save project to keep a desktop Studio working session; loaded audio and the mastering queue are not automatically recovered.");
}

fn terms(app: &mut MasterCraftApp, ui: &mut egui::Ui) {
    ui.label("Look up a term without leaving your track.");
    let label = ui.label("Find a term");
    ui.add(
        egui::TextEdit::singleline(&mut app.experience.search)
            .hint_text("Search EQ, LUFS, stems…")
            .desired_width(ui.available_width()),
    )
    .labelled_by(label.id);
    let search = app.experience.search.trim().to_lowercase();
    let mut count = 0;
    for term in &guide().glossary {
        if search.is_empty()
            || term.term.to_lowercase().contains(&search)
            || term.description.to_lowercase().contains(&search)
        {
            ui.add_space(8.);
            ui.strong(&term.term);
            ui.label(&term.description);
            count += 1;
        }
    }
    if count == 0 {
        ui.label("No matches. Try EQ, loudness or stem.");
    }
}

fn studio(ui: &mut egui::Ui) {
    for (title, body) in [
        (
            "Start with the material you have",
            "Choose several aligned stem files or a Suno ZIP to work on individual parts. You do not need a separation model for imported stems. WAV/MP3 alternatives are reviewed as one part.",
        ),
        (
            "Preview before saving",
            "Fader changes are staged until Update mix preview. Prepare that mix before cleanup, repair or a processed export. Raw stem export keeps the original aligned parts.",
        ),
        (
            "Work on one part",
            "Choose the vocal or another stem, prepare a cleanup or repair preview, then Apply preview to selected stem to rebuild the full mix. A/B lets you compare before and after.",
        ),
        (
            "Keep your working session",
            "Save project preserves source copies, stems and the current prepared preview. Reopen session.studio.json to continue. Export saves a listening file; it is a separate action.",
        ),
        (
            "Explore optional desktop tools",
            "Model separation and compatible VST3 effects require explicit setup. Model quality and plugin compatibility vary. No model downloads or plugin processing happen merely by opening Help.",
        ),
    ] {
        ui.add_space(8.);
        ui.strong(title);
        ui.label(body);
    }
}

fn assistant(app: &mut MasterCraftApp, ui: &mut egui::Ui, ctx: &egui::Context) {
    let guide = &guide().mcp;
    ui.label(RichText::new(&guide.title).strong().size(19.));
    ui.label(&guide.body);
    ui.label("Choose which audio folder it may read and where outputs go. Your AI client's settings stay unchanged.");
    ui.separator();
    let Some(binary) = super::mcp_setup::companion(&app.root) else {
        ui.label("The MCP companion was not found in this build. Use the complete Studio Preview package, or build the companion from source.");
        ui.hyperlink_to(
            "Read the MCP setup guide",
            "https://github.com/slimedragonair/SlimeyBeats-EZ-Master/blob/main/docs/MCP.md",
        );
        mcp_details(ui);
        return;
    };
    let setup = &mut app.experience.mcp;
    ui.add_enabled_ui(!setup.busy(), |ui| {
        for (input, label) in [
            (true, "1. Choose audio folder…"),
            (false, "2. Choose export folder…"),
        ] {
            if ui.add(crystal_button::secondary(label)).clicked()
                && let Some(path) = rfd::FileDialog::new()
                    .set_title(if input {
                        "Choose audio the assistant may read"
                    } else {
                        "Choose a folder for new outputs"
                    })
                    .pick_folder()
            {
                setup.invalidate();
                if input {
                    setup.input = Some(path);
                } else {
                    setup.output = Some(path);
                }
            }
            if let Some(path) = if input { &setup.input } else { &setup.output } {
                ui.add(egui::Label::new(path.to_string_lossy()).truncate())
                    .on_hover_text(path.display().to_string());
            }
        }
        if ui
            .add_enabled(
                setup.input.is_some() && setup.output.is_some(),
                crystal_button::primary("3. Generate connection example"),
            )
            .clicked()
        {
            setup.generate(&binary, ctx);
        }
    });
    if setup.busy() {
        ui.horizontal(|ui| {
            ui.spinner();
            ui.label("Preparing your connection example…");
        });
    }
    if let Some(error) = &setup.error {
        ui.label(RichText::new(error).color(theme::BONE));
    }
    if let Some(config) = &setup.config {
        ui.add_space(8.);
        if ui
            .add(crystal_button::primary(if setup.copied {
                "Copied — paste into your AI client"
            } else {
                "Copy connection JSON"
            }))
            .clicked()
        {
            ctx.copy_text(config.clone());
            setup.copied = true;
        }
        ui.label("Use your client's MCP setup instructions. Clients that use another schema need the same command and arguments.");
        ui.collapsing("View connection JSON", |ui| {
            ui.add(
                egui::Label::new(RichText::new(config).monospace())
                    .selectable(true)
                    .wrap(),
            );
        });
    }
    mcp_details(ui);
}

fn mcp_details(ui: &mut egui::Ui) {
    let guide = &guide().mcp;
    ui.collapsing("Setup & privacy details", |ui| {
        for (index, text) in guide.steps.iter().enumerate() { ui.label(format!("{}. {text}", index + 1)); }
        ui.label(RichText::new(&guide.privacy_note).color(theme::MUTED));
        ui.label("Generating a connection example does not start a server. Your AI client starts the local companion when you connect it.");
    });
}

fn troubleshooting(ui: &mut egui::Ui) {
    for (title, body) in [
        (
            "I cannot hear anything",
            "Load audio, choose A or B, check the listening volume and your system's output device, then press Play. Mastering and exports can work even when the system has no usable playback device.",
        ),
        (
            "Export is unavailable",
            "Prepare a master or Studio preview first. Apply pending fader changes. When mastering direction changes, re-master before exporting the new direction. Read any error message shown by the app.",
        ),
        (
            "An audio file will not open",
            "Native WAV works without extra codecs. Other desktop input formats, MP3/FLAC exports and resampling need FFmpeg and ffprobe on PATH. Restart the app after installing them.",
        ),
        (
            "My stems do not line up",
            "Parts must have matching sample rate, channel count and exact frame count. Studio does not automatically align, time-stretch or resample imported stems. Check the export settings in the source tool.",
        ),
        (
            "The master changes too much",
            "Try a gentler loudness direction, reduce correction intensity, or use a more suitable profile. Compare at matched loudness and judge the sound rather than only the measurements.",
        ),
        (
            "I want to learn a control",
            "Hover its label or control for a short explanation, or search Terms here. Switch to Advanced when you need the detailed controls; your current settings are preserved.",
        ),
    ] {
        ui.add_space(8.);
        ui.strong(title);
        ui.label(body);
    }
}

fn about(ui: &mut egui::Ui) {
    ui.strong("SlimeyBeats-EZ-Master · SlimeDragon Audio");
    ui.label(format!("Desktop version {}", env!("CARGO_PKG_VERSION")));
    ui.label("Audio processing happens on this device. The desktop app does not need an AI connection. Optional model setup and external help links need network access when you choose them.");
    ui.label("The software uses the MIT license. The software license does not claim ownership of the music you make or edit. Keep the rights to any source material you use.");
    ui.hyperlink_to(
        "Read the user guide",
        "https://github.com/slimedragonair/SlimeyBeats-EZ-Master/blob/main/docs/USER_GUIDE.md",
    );
}
