#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]
use clap::Parser;
use std::path::PathBuf;

#[derive(Parser)]
#[command(name=mastercraft::branding::APP_NAME,version, about = "SlimeyBeats-EZ-Master desktop mastering workstation")]
struct Arguments {
    /// Open a validated local session prepared by the MCP companion.
    #[arg(long, conflicts_with = "files")]
    session: Option<PathBuf>,
    files: Vec<PathBuf>,
    #[arg(long)]
    auto_master: bool,
    #[arg(long,default_value="overview",value_parser=["overview","eq","report"])]
    view: String,
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
    eframe::run_native(
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
            let mut app = mastercraft::gui::MasterCraftApp::with_initial_settings(
                cc,
                files,
                args.auto_master,
                initial,
            );
            app.tab = match args.view.as_str() {
                "eq" => 1,
                "report" => 2,
                _ => 0,
            };
            if args.capture_save_settings {
                app.save_settings();
            }
            if let Some(destination) = args.capture_ui {
                app.capture_ui_once(destination);
            }
            if let Some(offset) = args.capture_settings_scroll {
                app.capture_settings_at(offset as f32);
            }
            if let Some(offset) = args.capture_workspace_scroll {
                app.capture_workspace_at(offset as f32);
            }
            Ok(Box::new(app))
        }),
    )
}
