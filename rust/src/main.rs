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
    /// Open a validated local session prepared by the MCP companion.
    #[arg(long, conflicts_with = "files")]
    session: Option<PathBuf>,
    #[arg(long,conflicts_with_all=["files","session","auto_master"])]
    studio_session: Option<PathBuf>,
    files: Vec<PathBuf>,
    #[arg(long)]
    auto_master: bool,
    #[arg(long,default_value="overview",value_parser=["overview","eq","report","stems","voice","repair","plugins"])]
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
    #[arg(long, hide = true, requires = "capture_ui")]
    capture_save_settings: bool,
    #[arg(long, hide=true, num_args=2, requires="capture_ui", value_parser=clap::value_parser!(u32).range(1..=7680))]
    capture_size: Vec<u32>,
    #[arg(long, hide = true, requires = "capture_ui")]
    capture_settings_scroll: Option<u32>,
    #[arg(long, hide = true, requires = "capture_ui")]
    capture_workspace_scroll: Option<u32>,
}
fn main() -> eframe::Result {
    let args = Arguments::parse();
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
    let result = eframe::run_native(
        mastercraft::branding::APP_NAME,
        eframe::NativeOptions {
            viewport: eframe::egui::ViewportBuilder::default()
                .with_inner_size(size)
                .with_min_inner_size([1100., 720.])
                .with_icon(mastercraft::gui::window_icon()),
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
            let mut app = mastercraft::gui::MasterCraftApp::with_initial_settings(
                cc,
                queue_files,
                args.auto_master,
                initial,
            );
            app.tab = if studio_archive {
                3
            } else {
                match args.view.as_str() {
                    "eq" => 1,
                    "report" => 2,
                    "stems" | "voice" | "repair" | "plugins" => 3,
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
                    "voice" => Page::Voice,
                    "repair" => Page::Repair,
                    "plugins" => Page::Plugins,
                    _ => Page::Stems,
                };
                app.studio.two_stems = !args.studio_four_stems;
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
            if let Some(offset) = args.capture_settings_scroll {
                app.capture_settings_at(offset as f32);
            }
            if let Some(offset) = args.capture_workspace_scroll {
                app.capture_workspace_at(offset as f32);
            }
            Ok(Box::new(app))
        }),
    );
    if capture_failed.load(Ordering::Relaxed) {
        std::process::exit(1);
    }
    result
}
