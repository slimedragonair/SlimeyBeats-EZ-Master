//! Review a whole stem selection once, rather than choosing every part separately.
use super::state::Studio;
use crate::{
    gui::{crystal_button, theme},
    studio::{
        stem_import_plan::{self, PartSelection, PreferredFormat},
        stem_mix::StemKind,
    },
    studio_native::stem_bundle::PreparedImport,
};
use eframe::egui;
use std::collections::BTreeSet;

const OMITTED_NAME_CHARS: usize = 24;

fn short_part_name(name: &str) -> String {
    if name.chars().count() <= OMITTED_NAME_CHARS {
        name.to_owned()
    } else {
        format!(
            "{}…",
            name.chars()
                .take(OMITTED_NAME_CHARS - 1)
                .collect::<String>()
        )
    }
}

struct Choice {
    included: bool,
    kind: StemKind,
}
pub struct Review {
    prepared: PreparedImport,
    choices: Vec<Choice>,
    preferred: PreferredFormat,
}
impl Review {
    pub fn part_count(&self) -> usize {
        self.prepared.plan.parts.len()
    }
    pub fn selected_count(&self) -> usize {
        self.choices.iter().filter(|choice| choice.included).count()
    }
    pub fn new(prepared: PreparedImport) -> Self {
        let choices = prepared
            .plan
            .parts
            .iter()
            .map(|part| Choice {
                included: true,
                kind: part.kind,
            })
            .collect();
        Self {
            prepared,
            choices,
            preferred: PreferredFormat::default(),
        }
    }
    fn selected(&self) -> Vec<PartSelection> {
        self.choices
            .iter()
            .enumerate()
            .filter(|(_, choice)| choice.included)
            .map(|(part_index, choice)| PartSelection {
                part_index,
                kind: Some(choice.kind),
            })
            .collect()
    }
}

pub fn show(studio: &mut Studio, ui: &mut egui::Ui, ctx: &egui::Context) {
    theme::eyebrow(ui, "IMPORT A STEM SET");
    ui.label("Combine aligned parts. No fixed track count.");
    ui.horizontal_wrapped(|ui| {
        if ui.add(crystal_button::secondary("Choose stem files…")).on_hover_text("Select multiple aligned files. Matching WAV/MP3 names count as one part; review the format before import.").clicked()
            && let Some(paths) = rfd::FileDialog::new().add_filter("Audio stems", crate::audio::EXTENSIONS).pick_files() { studio.prepare_stem_files(paths, ctx); }
        if ui.add(crystal_button::secondary("Choose Suno / stem ZIP…")).on_hover_text("Read a ZIP locally. Only selected WAV/MP3 files are unpacked into private temporary files. No upload or model download.").clicked()
            && let Some(path) = rfd::FileDialog::new().add_filter("Stem ZIP", &["zip"]).pick_file() { studio.prepare_stem_archive(path, ctx); }
    });
    if let Some(review) = &studio.bulk_import {
        ui.small(format!(
            "Review {} / {} selected parts in the workspace.",
            review.selected_count(),
            review.part_count()
        ));
    }
    actions(studio, ui, ctx);
}

pub fn review(studio: &mut Studio, ui: &mut egui::Ui, ctx: &egui::Context, compact: bool) {
    if let Some(review) = &mut studio.bulk_import {
        ui.separator();
        if compact {
            egui::CollapsingHeader::new("Pairing & format help")
                .id_salt("stem-import-help")
                .show(ui, |ui| {
                    ui.small("Alternate WAV/MP3 files count once. A missing preferred format falls back to the available file. Leave the full mix out when importing its individual stems.");
                });
        } else {
            ui.small("Alternate WAV/MP3 files count once. A missing preferred format falls back to the available file.");
        }
        ui.horizontal_wrapped(|ui| {
            for (format, label) in [
                (PreferredFormat::Wav, "Prefer WAV"),
                (PreferredFormat::Mp3, "Prefer MP3"),
            ] {
                if ui
                    .add(crystal_button::secondary(label).selected(review.preferred == format))
                    .clicked()
                {
                    review.preferred = format;
                }
            }
        });
        let selected = review.selected_count();
        ui.label(format!(
            "{} distinct parts · {selected} selected",
            review.part_count()
        ));
        ui.horizontal(|ui| {
            if ui.small_button("Select all").clicked() {
                for choice in &mut review.choices {
                    choice.included = true;
                }
            }
            if ui.small_button("Select none").clicked() {
                for choice in &mut review.choices {
                    choice.included = false;
                }
            }
            ui.small("Available memory applies.");
        });
        let mut used: BTreeSet<_> = review
            .choices
            .iter()
            .filter(|row| row.included)
            .map(|row| row.kind)
            .collect();
        if compact {
            compact_choices(review, ui, &mut used);
        } else {
            let columns = if ui.available_width() >= 700. {
                3
            } else if ui.available_width() >= 440. {
                2
            } else {
                1
            };
            let width = ((ui.available_width() - (columns - 1) as f32 * 12.) / columns as f32
                - 22.)
                .max(150.);
            egui::Grid::new("stem-import-cards").num_columns(columns).max_col_width(width + 22.).spacing([12., 12.]).show(ui, |ui| {
            for (index, (part, choice)) in review.prepared.plan.parts.iter().zip(&mut review.choices).enumerate() {
                egui::Frame::group(ui.style()).inner_margin(10).show(ui, |ui| {
                    ui.with_layout(egui::Layout::top_down(egui::Align::Min), |ui| {
                        ui.set_width(width);
                        ui.style_mut().wrap_mode = Some(egui::TextWrapMode::Wrap);
                        ui.push_id(("bulk-part", index), |ui| {
                            let previous = choice.included;
                            ui.checkbox(&mut choice.included, &part.name);
                            if !previous && choice.included { choice.kind = stem_import_plan::available_kind(choice.kind, &used); used.insert(choice.kind); }
                            ui.add_enabled_ui(choice.included, |ui| {
                                kind_picker(choice, ui, width.min(150.), &used);
                            });
                            let version = part.preferred(&review.preferred);
                            let filename = version.name.rsplit('/').next().unwrap_or(&version.name);
                            ui.small(format!("{} · {}", version.format.to_uppercase(), filename)).on_hover_text(format!("Using {}. Its alternate encoding will not be mixed again.", version.name));
                        });
                    });
                });
                if (index + 1) % columns == 0 { ui.end_row(); }
            }
        });
        }
        if review.prepared.plan.skipped_files > 0 {
            ui.small(format!(
                "{} non-audio/metadata files skipped",
                review.prepared.plan.skipped_files
            ));
        }
    }
    actions(studio, ui, ctx);
}

fn kind_picker(choice: &mut Choice, ui: &mut egui::Ui, width: f32, used: &BTreeSet<StemKind>) {
    egui::ComboBox::from_id_salt("bulk-kind").width(width).selected_text(choice.kind.name()).show_ui(ui, |ui| {
        if !StemKind::ALL.contains(&choice.kind) { let kind = choice.kind; ui.selectable_value(&mut choice.kind, kind, kind.name()); }
        for kind in StemKind::ALL { ui.selectable_value(&mut choice.kind, kind, kind.name()); }
        let kind = stem_import_plan::available_kind(StemKind::Part1, used);
        ui.selectable_value(&mut choice.kind, kind, format!("{} (unused)", kind.name()));
    }).response.on_hover_text("A unique identity for this part, also used in exported filenames. Generic part slots can hold any instrument.");
}

fn compact_choices(review: &mut Review, ui: &mut egui::Ui, used: &mut BTreeSet<StemKind>) {
    egui::Grid::new("compact-stem-import")
        .num_columns(2)
        .spacing([8., 4.])
        .show(ui, |ui| {
            for (index, (part, choice)) in review
                .prepared
                .plan
                .parts
                .iter()
                .zip(&mut review.choices)
                .enumerate()
            {
                let version = part.preferred(&review.preferred);
                ui.push_id(("compact-part", index), |ui| {
                    let previous = choice.included;
                    ui.checkbox(
                        &mut choice.included,
                        format!("{:02}  {}", index + 1, short_part_name(&part.name)),
                    )
                    .on_hover_text(format!(
                        "{}\nUsing {} · {}",
                        part.name,
                        version.format.to_uppercase(),
                        version.name
                    ));
                    if !previous && choice.included {
                        choice.kind = stem_import_plan::available_kind(choice.kind, used);
                        used.insert(choice.kind);
                    }
                });
                ui.push_id(("compact-kind", index), |ui| {
                    ui.add_enabled_ui(choice.included, |ui| kind_picker(choice, ui, 112., used));
                });
                ui.end_row();
            }
        });
}

fn actions(studio: &mut Studio, ui: &mut egui::Ui, ctx: &egui::Context) {
    let mut discard = false;
    let mut import = None;
    if let Some(review) = &studio.bulk_import {
        let selected = review.selected();
        let omitted: Vec<_> = review
            .prepared
            .plan
            .parts
            .iter()
            .zip(&review.choices)
            .filter(|(_, choice)| !choice.included)
            .map(|(part, _)| part.name.as_str())
            .collect();
        if !omitted.is_empty() {
            ui.small(format!(
                "{} found · {} selected · {} not selected",
                review.part_count(),
                selected.len(),
                omitted.len()
            ));
            let names = omitted
                .iter()
                .take(3)
                .map(|name| short_part_name(name))
                .collect::<Vec<_>>()
                .join(", ");
            ui.small(format!(
                "Left out: {names}{}",
                if omitted.len() > 3 { " …" } else { "" }
            ))
            .on_hover_text(if omitted.len() > 3 {
                "Expand the full list below to inspect every omitted part.".to_owned()
            } else {
                omitted.join("\n")
            });
        }
        let valid = stem_import_plan::select(&review.prepared.plan, &selected, &review.preferred);
        if let Err(error) = &valid {
            ui.colored_label(theme::BONE, error.to_string());
        }
        if ui
            .add_enabled(
                valid.is_ok(),
                crystal_button::primary("Import selected stems"),
            )
            .clicked()
        {
            import = Some((review.prepared.clone(), selected, review.preferred.clone()));
        }
        discard = ui
            .small_button("Discard file selection")
            .on_hover_text("Clear this pending selection. Loaded Studio audio is unchanged.")
            .clicked();
        if !omitted.is_empty() {
            egui::CollapsingHeader::new(format!("Show all {} not selected", omitted.len()))
                .id_salt(("stem-import-omissions", ui.scope_id()))
                .show(ui, |ui| {
                    egui::ScrollArea::vertical()
                        .max_height(110.)
                        .show(ui, |ui| {
                            for name in &omitted {
                                ui.small(*name);
                            }
                        });
                });
        }
    }
    if discard {
        studio.bulk_import = None;
    }
    if let Some((prepared, selected, preferred)) = import {
        studio.import_stem_bundle(prepared, selected, preferred, ctx);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{studio::stem_import_plan::ImportFile, studio_native::stem_bundle::ImportSource};

    #[test]
    fn large_review_selects_every_part_without_reusing_ids() {
        let files = (0..80)
            .map(|index| ImportFile {
                index,
                name: format!("Instrument {index:03}.wav"),
                format: "wav".into(),
                bytes: 100,
                crc32: None,
            })
            .collect();
        let review = Review::new(PreparedImport {
            source: ImportSource::Files(Vec::new()),
            plan: stem_import_plan::plan(files).unwrap(),
        });
        assert_eq!(review.part_count(), 80);
        assert_eq!(review.selected_count(), 80);
        let selection =
            stem_import_plan::select(&review.prepared.plan, &review.selected(), &review.preferred)
                .unwrap();
        assert_eq!(selection.len(), 80);
        assert_eq!(
            selection
                .iter()
                .map(|(kind, _)| *kind)
                .collect::<BTreeSet<_>>()
                .len(),
            80
        );
    }
}
