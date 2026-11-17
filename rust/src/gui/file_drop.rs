//! Route native file drops to the visible editing workflow.
use super::{app::MasterCraftApp, studio::state::Page};
use eframe::egui;
use std::path::PathBuf;

impl MasterCraftApp {
    pub(super) fn route_file_drop(&mut self, files: Vec<PathBuf>, ctx: &egui::Context) {
        if files.is_empty() {
            return;
        }
        let archives = files
            .iter()
            .filter(|path| {
                path.extension()
                    .is_some_and(|ext| ext.eq_ignore_ascii_case("zip"))
            })
            .count();
        if self.studio.page == Page::Tracks && (self.tab == 3 || archives > 0) {
            self.activate_workspace(3);
            self.studio.prepare_track_files(files, ctx);
        } else if archives > 0 {
            self.activate_workspace(3);
            if files.len() == 1 {
                self.studio.prepare_stem_archive(files[0].clone(), ctx);
            } else {
                self.studio.error = Some(
                    "Drop one stem ZIP at a time, or choose multiple loose audio stems.".into(),
                );
            }
        } else if self.tab == 3 && files.len() > 1 {
            self.studio.prepare_stem_files(files, ctx);
        } else if self.tab == 3 {
            self.studio.load(files[0].clone(), ctx);
        } else {
            self.add_paths(files);
        }
    }
}

#[cfg(test)]
#[path = "file_drop_tests.rs"]
mod tests;
