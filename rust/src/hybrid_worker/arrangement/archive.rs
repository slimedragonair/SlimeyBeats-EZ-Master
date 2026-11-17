//! Shared archive admission and selected contained extraction, never stem alignment.
use super::workspace::Workspace;
use anyhow::{Result, ensure};
use mastercraft::studio::{
    stem_archive,
    stem_import_plan::{self, ImportFile},
};
use serde_json::{Value, json};
use std::{
    collections::BTreeSet,
    fs::File,
    io::{BufWriter, Write},
    path::Path,
};

impl Workspace {
    pub(super) fn inspect_archive(&self, file: &Path) -> Result<Value> {
        self.session()?;
        crate::hybrid_worker::paths::no_links(file)?;
        let file = self.paths.input(file)?;
        let (files, skipped) =
            stem_archive::inspect_files(File::open(file)?, stem_archive::MAX_ENTRY_BYTES)?;
        let mut plan = stem_import_plan::plan_audio(files)?;
        plan.skipped_files += skipped;
        Ok(serde_json::to_value(plan)?)
    }

    pub(super) fn extract_archive(
        &self,
        file: &Path,
        files: &[ImportFile],
        output_id: &str,
        max_total_bytes: u64,
    ) -> Result<Value> {
        self.session()?;
        ensure!(!files.is_empty(), "Choose archive sources");
        let mut used = BTreeSet::new();
        let mut total = 0_u64;
        let mut metadata = 0;
        for file in files {
            metadata = stem_import_plan::charge_metadata(metadata, file.name.len())?;
            ensure!(used.insert(file.index), "Duplicate archive selection");
            ensure!(
                matches!(file.format.as_str(), "wav" | "mp3"),
                "Choose WAV/MP3 sources"
            );
            total = total
                .checked_add(file.bytes)
                .ok_or_else(|| anyhow::anyhow!("Archive byte count overflow"))?;
        }
        ensure!(
            total <= max_total_bytes,
            "Selected sources exceed remaining project storage"
        );
        crate::hybrid_worker::paths::no_links(file)?;
        let file = self.paths.input(file)?;
        let directory = self
            .paths
            .directory(&self.job_directory(output_id)?.join("selected-sources"))?;
        let mut outputs = Vec::new();
        for (index, selected) in files.iter().enumerate() {
            let output = directory.join(format!("asset-{index:05}.{}", selected.format));
            let mut writer = BufWriter::new(File::create_new(&output)?);
            stem_archive::copy_audio(
                File::open(&file)?,
                selected,
                stem_archive::MAX_ENTRY_BYTES,
                &mut writer,
                &|| false,
            )?;
            writer.flush()?;
            outputs.push(output);
        }
        Ok(json!({"files": outputs}))
    }
}
