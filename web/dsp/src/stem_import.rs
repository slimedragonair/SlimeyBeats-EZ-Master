use crate::failure;
use mastercraft::studio::{
    stem_archive,
    stem_import_plan::{self, ImportFile, ImportPlan},
};
use serde::Deserialize;
use std::io::Cursor;
use wasm_bindgen::prelude::*;

const MAX_BROWSER_ARCHIVE_BYTES: usize = 128 * 1024 * 1024;
#[derive(Deserialize)]
struct FileName {
    name: String,
    size: u64,
}

#[wasm_bindgen]
pub fn plan_stem_files(files: &str) -> Result<String, JsValue> {
    let files: Vec<FileName> = serde_json::from_str(files).map_err(failure)?;
    let files = files
        .into_iter()
        .enumerate()
        .map(|(index, file)| ImportFile {
            index,
            format: stem_import_plan::file_format(&file.name).unwrap_or_default(),
            name: file.name,
            bytes: file.size,
            crc32: None,
        })
        .collect();
    let plan = stem_import_plan::plan(files).map_err(failure)?;
    serde_json::to_string(&plan).map_err(failure)
}

#[wasm_bindgen]
pub struct BrowserStemArchive {
    bytes: Vec<u8>,
    plan: ImportPlan,
}
#[wasm_bindgen]
impl BrowserStemArchive {
    #[wasm_bindgen(constructor)]
    pub fn new(bytes: Vec<u8>) -> Result<BrowserStemArchive, JsValue> {
        if bytes.len() > MAX_BROWSER_ARCHIVE_BYTES {
            return Err(failure(
                "Browser stem ZIP exceeds 128 MiB. Use the desktop app for larger archives.",
            ));
        }
        let plan = stem_archive::inspect(Cursor::new(&bytes), MAX_BROWSER_ARCHIVE_BYTES as u64)
            .map_err(failure)?;
        Ok(Self { bytes, plan })
    }
    pub fn info(&self) -> Result<String, JsValue> {
        serde_json::to_string(&self.plan).map_err(failure)
    }
    pub fn extract_audio(&self, index: usize) -> Result<Vec<u8>, JsValue> {
        let file = self
            .plan
            .parts
            .iter()
            .flat_map(|part| &part.versions)
            .find(|file| file.index == index)
            .ok_or_else(|| failure("Unknown ZIP audio entry"))?;
        let mut result = Vec::with_capacity((file.bytes as usize).min(1024 * 1024));
        stem_archive::copy_audio(
            Cursor::new(&self.bytes),
            file,
            MAX_BROWSER_ARCHIVE_BYTES as u64,
            &mut result,
            &|| false,
        )
        .map_err(failure)?;
        Ok(result)
    }
}
