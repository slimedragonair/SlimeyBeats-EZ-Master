use super::{jobs::Change, state::Studio};
use crate::{
    studio::{
        stem_import_plan::{PartSelection, PreferredFormat},
        stem_mix::{self, StemSet},
    },
    studio_native::{session::StudioSnapshot, stem_bundle::PreparedImport},
};
use anyhow::Result;
use eframe::egui;
use std::{
    path::PathBuf,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    time::Duration,
};

fn session(bank: StemSet, cancel: &AtomicBool) -> Result<Change> {
    let mix = bank.defaults();
    let mut raw = mix.clone();
    raw.protect_peaks = false;
    let source = Arc::new(stem_mix::render(&bank, &raw, &|| {
        cancel.load(Ordering::Relaxed)
    })?);
    let preview = Arc::new(stem_mix::render(&bank, &mix, &|| {
        cancel.load(Ordering::Relaxed)
    })?);
    Ok(Change::Session(Box::new(StudioSnapshot {
        source_name: "Imported stem mix.wav".into(), source: source.clone(), stems: Some(bank), mix, before: source, preview,
        label: "Imported stem mix".into(), notes: vec!["Aligned stems retained at their original rate, channels and length. Alternate file formats were not mixed twice.".into()], view: "stems".into(),
    })))
}
impl Studio {
    pub fn import_aligned_stems(
        &mut self,
        files: Vec<(stem_mix::StemKind, PathBuf)>,
        ctx: &egui::Context,
    ) {
        self.begin(
            ctx,
            "Importing aligned local stems",
            move |cancel, progress| {
                session(
                    crate::studio_native::stem_files::import(&files, cancel.clone(), progress)?,
                    &cancel,
                )
            },
        );
    }
    pub fn prepare_stem_files(&mut self, paths: Vec<PathBuf>, ctx: &egui::Context) {
        self.begin(ctx, "Grouping selected stem files", move |_, _| {
            Ok(Change::StemImport(PreparedImport::files(paths)?))
        });
    }
    pub fn prepare_stem_archive(&mut self, path: PathBuf, ctx: &egui::Context) {
        self.begin(ctx, "Reading stem ZIP contents locally", move |_, _| {
            Ok(Change::StemImport(PreparedImport::archive(path)?))
        });
    }
    pub fn import_stem_bundle(
        &mut self,
        prepared: PreparedImport,
        selected: Vec<PartSelection>,
        preferred: PreferredFormat,
        ctx: &egui::Context,
    ) {
        self.begin(
            ctx,
            "Importing selected stem parts",
            move |cancel, progress| {
                session(
                    prepared.import(
                        &selected,
                        &preferred,
                        512 * 1024 * 1024,
                        Duration::from_secs(300),
                        cancel.clone(),
                        progress,
                    )?,
                    &cancel,
                )
            },
        );
    }
}
