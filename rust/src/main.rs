#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]
use clap::Parser;
use std::{
    path::PathBuf,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
};

#[derive(Parser)]
#[command(name=mastercraft::branding::APP_NAME,version, about = "SlimeyBeats-EZ-Master desktop mastering workstation")]
struct Arguments {
    /// Print the update identity without starting the GUI or opening any audio.
    #[arg(long, hide = true, conflicts_with_all = ["files", "session", "studio_session", "auto_master", "capture_ui"])]
    build_info: bool,
    /// Open a validated local session prepared by the MCP companion.
    #[arg(long, conflicts_with = "files")]
    session: Option<PathBuf>,
    #[arg(long,conflicts_with_all=["files","session","auto_master"])]
    studio_session: Option<PathBuf>,
    files: Vec<PathBuf>,
    #[arg(long)]
    auto_master: bool,
    #[arg(long,default_value="overview",value_parser=["overview","eq","report","tracks","stems","voice","repair","plugins"])]
    view: String,
    #[arg(long)]
    studio_data: Option<PathBuf>,
    #[arg(long,hide=true,value_parser=["separate","voice","spectrogram","repair","plugin"])]
    studio_action: Option<String>,
    #[arg(long, hide = true, requires = "studio_action")]
    studio_plugin: Option<PathBuf>,
    #[arg(long, hide = true)]
    studio_four_stems: bool,
    #[arg(long, hide = true)]
    capture_ui: Option<PathBuf>,
    /// Load and scan synthetic diagnostic audio before capturing the Doctor page.
    #[arg(long, hide = true, requires_all = ["capture_ui", "recovery_data", "files"],
        conflicts_with_all = ["session", "studio_session", "auto_master", "studio_action", "capture_learning", "capture_restore_recovery", "capture_wait_recovery", "capture_stay_open", "capture_save_settings"])]
    capture_doctor: bool,
    /// Diagnostic runs isolate persistent recovery from normal app data.
    #[arg(long, hide = true, requires = "capture_ui")]
    recovery_data: Option<PathBuf>,
    #[arg(long,hide=true,requires_all=["capture_ui","recovery_data"])]
    capture_wait_recovery: bool,
    #[arg(long,hide=true,requires_all=["capture_ui","recovery_data"],conflicts_with_all=["files","session","studio_session","auto_master"])]
    capture_restore_recovery: bool,
    #[arg(long,hide=true,requires_all=["capture_ui","recovery_data"])]
    capture_stay_open: bool,
    #[arg(long, hide = true, requires = "capture_ui")]
    capture_save_settings: bool,
    #[arg(long, hide=true, num_args=2, requires="capture_ui", value_parser=clap::value_parser!(u32).range(1..=7680))]
    capture_size: Vec<u32>,
    #[arg(long, hide = true, requires = "capture_ui")]
    capture_settings_scroll: Option<u32>,
    #[arg(long, hide = true, requires = "capture_ui")]
    capture_workspace_scroll: Option<u32>,
    #[arg(long, hide = true, requires = "capture_ui", value_parser = ["welcome", "tour", "help", "terms", "mcp", "updates", "report", "report-preview", "recovery"])]
    capture_learning: Option<String>,
}
fn main() -> eframe::Result {
    let args = Arguments::parse();
    if args.build_info {
        println!(
            "{}",
            serde_json::to_string(&mastercraft::app_update::current())
                .expect("valid build identity")
        );
        return Ok(());
    }
    let document = if let Some(path) = &args.session {
        match mastercraft::session_document::SessionDocument::load(path) {
            Ok(document) => Some(document),
            Err(error) => {
                rfd::MessageDialog::new()
                    .set_title("Session could not be opened")
                    .set_description(format!("{error:#}"))
                    .set_level(rfd::MessageLevel::Error)
                    .show();
                return Ok(());
            }
        }
    } else {
        None
    };
    let (files, initial) = match document {
        Some(document) => (vec![document.source.path], Some(document.settings)),
        None => (args.files, None),
    };
    let size = if args.capture_size.len() == 2 {
        [args.capture_size[0] as f32, args.capture_size[1] as f32]
    } else {
        [1480., 940.]
    };
    let studio_source = files.first().cloned();
    let studio_inputs = files.clone();
    let studio_archive = files.len() == 1
        && files[0]
            .extension()
            .is_some_and(|ext| ext.eq_ignore_ascii_case("zip"));
    let capture_failed = Arc::new(AtomicBool::new(false));
    let capture_result = capture_failed.clone();
    let mut viewport = eframe::egui::ViewportBuilder::default()
        .with_inner_size(size)
        .with_min_inner_size([1100., 720.])
        .with_icon(mastercraft::gui::window_icon());
    if args.capture_ui.is_some() {
        viewport = viewport.with_active(false).with_resizable(false);
    }
    let result = eframe::run_native(
        mastercraft::branding::APP_NAME,
        eframe::NativeOptions {
            viewport,
            renderer: eframe::Renderer::Glow,
            ..Default::default()
        },
        Box::new(move |cc| {
            let queue_files = if studio_archive || (args.view == "stems" && studio_inputs.len() > 1)
            {
                vec![]
            } else {
                files
            };
            let mut app = if args.capture_ui.is_some()
                && let Some(root) = args.recovery_data.as_ref()
            {
                mastercraft::gui::MasterCraftApp::with_private_capture_root(
                    cc,
                    queue_files,
                    args.auto_master,
                    initial,
                    root.clone(),
                )
            } else {
                mastercraft::gui::MasterCraftApp::with_initial_settings(
                    cc,
                    queue_files,
                    args.auto_master,
                    initial,
                )
            };
            app.tab = if studio_archive || args.capture_doctor {
                3
            } else {
                match args.view.as_str() {
                    "eq" => 1,
                    "report" => 2,
                    "tracks" | "stems" | "voice" | "repair" | "plugins" => 3,
                    _ => 0,
                }
            };
            if let Some(base) = args.studio_data {
                app.studio.runtime = mastercraft::studio_native::runtime::RuntimePaths { base };
            }
            if let Some(project) = args.studio_session {
                app.tab = 3;
                app.studio.load_project(project, &cc.egui_ctx);
            }
            if app.tab == 3 {
                use mastercraft::gui::studio::state::{AutoAction, Page};
                app.studio.page = match args.view.as_str() {
                    "tracks" => Page::Tracks,
                    "voice" => Page::Voice,
                    "repair" => Page::Repair,
                    "plugins" => Page::Plugins,
                    _ => Page::Stems,
                };
                if args.studio_four_stems {
                    app.studio.two_stems = false;
                }
                app.studio.plugin_path = args.studio_plugin;
                app.studio.auto_action = args.studio_action.map(|action| match action.as_str() {
                    "voice" => AutoAction::Voice,
                    "spectrogram" => AutoAction::Spectrogram,
                    "repair" => AutoAction::Repair,
                    "plugin" => AutoAction::Plugin,
                    _ => AutoAction::Separate,
                });
                if let Some(path) = studio_source {
                    if studio_archive {
                        app.studio.auto_action = None;
                        app.studio.prepare_stem_archive(path, &cc.egui_ctx);
                    } else if studio_inputs.len() > 1 && app.studio.page == Page::Stems {
                        app.studio.auto_action = None;
                        app.studio.prepare_stem_files(studio_inputs, &cc.egui_ctx);
                    } else {
                        app.studio.load(path, &cc.egui_ctx);
                    }
                }
            }
            if args.capture_save_settings {
                app.save_settings();
            }
            if let Some(destination) = args.capture_ui {
                app.capture_ui_once(destination, capture_result);
            }
            if args.capture_doctor {
                app.configure_doctor_capture(&cc.egui_ctx)?;
            }
            if let Some(root) = args.recovery_data {
                app.configure_recovery_capture(
                    root.join("recovery"),
                    args.capture_wait_recovery,
                    args.capture_restore_recovery,
                    args.capture_stay_open,
                );
            }
            if let Some(offset) = args.capture_settings_scroll {
                app.capture_settings_at(offset as f32);
            }
            if let Some(offset) = args.capture_workspace_scroll {
                app.capture_workspace_at(offset as f32);
            }
            if let Some(view) = args.capture_learning {
                app.capture_learning_at(&view);
            }
            Ok(Box::new(app))
        }),
    );
    if capture_failed.load(Ordering::Relaxed) {
        std::process::exit(1);
    }
    result
}

#[cfg(test)]
mod capture_argument_tests {
    use super::*;

    #[test]
    fn doctor_capture_requires_a_destination_private_data_and_audio() {
        for arguments in [
            vec!["app", "--capture-doctor"],
            vec![
                "app",
                "--capture-doctor",
                "--capture-ui",
                "shot.png",
                "input.wav",
            ],
            vec![
                "app",
                "--capture-doctor",
                "--capture-ui",
                "shot.png",
                "--recovery-data",
                "private",
            ],
        ] {
            assert!(Arguments::try_parse_from(arguments).is_err());
        }
        let arguments = [
            "app",
            "input.wav",
            "--capture-doctor",
            "--capture-ui",
            "shot.png",
            "--recovery-data",
            "private",
        ];
        assert!(Arguments::try_parse_from(arguments).unwrap().capture_doctor);
        assert!(Arguments::try_parse_from(arguments.into_iter().chain(["--auto-master"])).is_err());
    }
}
