//! Imported banks have their own layout; neural extraction keeps its model modes.
use super::state::Studio;
use crate::{
    gui::crystal_button,
    studio::{
        stem_import_plan::available_kind,
        stem_mix::{self, StemKind},
    },
};
use eframe::egui;
use std::{collections::BTreeSet, path::PathBuf};

#[derive(Clone, Copy, PartialEq, Eq)]
enum Layout {
    Two,
    Four,
    Multitrack,
}
struct Input {
    kind: StemKind,
    path: Option<PathBuf>,
}
pub struct StemImport {
    layout: Layout,
    rows: Vec<Input>,
}
impl Default for StemImport {
    fn default() -> Self {
        Self {
            layout: Layout::Two,
            rows: [StemKind::Vocals, StemKind::Instrumental]
                .into_iter()
                .map(|kind| Input { kind, path: None })
                .collect(),
        }
    }
}
impl StemImport {
    fn set_layout(&mut self, layout: Layout) {
        let kinds = match layout {
            Layout::Two => &[StemKind::Vocals, StemKind::Instrumental][..],
            Layout::Four => &[
                StemKind::Vocals,
                StemKind::Drums,
                StemKind::Bass,
                StemKind::Other,
            ][..],
            Layout::Multitrack => &StemKind::MULTITRACK[..],
        };
        self.rows = kinds
            .iter()
            .map(|&kind| Input {
                kind,
                path: self
                    .rows
                    .iter()
                    .find(|row| row.kind == kind)
                    .and_then(|row| row.path.clone()),
            })
            .collect();
        self.layout = layout;
    }
}

pub fn show(studio: &mut Studio, ui: &mut egui::Ui, ctx: &egui::Context) {
    ui.collapsing("Assign files individually", |ui| {
        ui.small("No model needed. Match sample rate, channels and length. Available memory applies.");
        let import = &mut studio.stem_import;
        let mut layout = import.layout;
        egui::ComboBox::from_id_salt("stem-import-layout").selected_text(match layout {
            Layout::Two => "Vocal + instrumental", Layout::Four => "Four stems", Layout::Multitrack => "Custom stem set",
        }).show_ui(ui, |ui| {
            ui.selectable_value(&mut layout, Layout::Two, "Vocal + instrumental");
            ui.selectable_value(&mut layout, Layout::Four, "Four stems");
            ui.selectable_value(&mut layout, Layout::Multitrack, "Custom stem set");
        });
        if layout != import.layout { import.set_layout(layout); }
        if layout == Layout::Multitrack {
            ui.small("Choose at least two files. Add more slots for extra instruments or tracks; unused slots can stay empty.");
            if ui.small_button("+ Add stem slot").clicked() {
                let used = import.rows.iter().map(|row| row.kind).collect();
                import.rows.push(Input { kind: available_kind(StemKind::Part1, &used), path: None });
            }
        }
        let used: BTreeSet<_> = import.rows.iter().map(|row| row.kind).collect();
        let mut remove = None;
        for (index, row) in import.rows.iter_mut().enumerate() {
            ui.push_id(index, |ui| {
                ui.horizontal_wrapped(|ui| {
                    egui::ComboBox::from_id_salt("kind").width(110.).selected_text(row.kind.name()).show_ui(ui, |ui| {
                        if !StemKind::ALL.contains(&row.kind) { let kind = row.kind; ui.selectable_value(&mut row.kind, kind, kind.name()); }
                        for kind in StemKind::ALL { ui.selectable_value(&mut row.kind, kind, kind.name()); }
                        let kind = available_kind(StemKind::Part1, &used);
                        ui.selectable_value(&mut row.kind, kind, format!("{} (unused)", kind.name()));
                    }).response.on_hover_text("A unique identity for this part, also used in exported filenames. Generic part slots can hold any instrument.");
                    if ui.small_button("Choose…").clicked() && let Some(path) = rfd::FileDialog::new().add_filter("Audio", crate::audio::EXTENSIONS).pick_file() {
                        row.path = Some(path);
                    }
                    if row.path.is_some() && ui.small_button("Clear").on_hover_text("Leave this import slot empty; the loaded bank is unchanged.").clicked() { row.path = None; }
                    if layout == Layout::Multitrack && ui.small_button("Remove slot").clicked() { remove = Some(index); }
                });
                if let Some(path) = &row.path { ui.small(path.file_name().unwrap_or_default().to_string_lossy()).on_hover_text(path.to_string_lossy()); }
            });
        }
        if let Some(index) = remove { import.rows.remove(index); }
        let files: Vec<_> = import.rows.iter().filter_map(|row| row.path.clone().map(|path| (row.kind, path))).collect();
        let valid = stem_mix::validate_kinds(files.iter().map(|(kind, _)| *kind));
        let complete = layout == Layout::Multitrack || files.len() == import.rows.len();
        ui.small(format!("{} parts selected", files.len()));
        if files.len() >= 2 && let Err(error) = &valid { ui.colored_label(crate::gui::theme::BONE, error.to_string()); }
        if ui.add_enabled(valid.is_ok() && complete, crystal_button::secondary("Import aligned stems")).clicked() {
            studio.import_aligned_stems(files, ctx);
        }
    });
}
