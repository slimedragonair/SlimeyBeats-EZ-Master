//! Reuse safe ZIP admission independently of aligned stem-bank planning.
use crate::failure;
use mastercraft::studio::{stem_archive, stem_import_plan::ImportFile};
use std::io::Cursor;
use wasm_bindgen::prelude::*;

const MAX_BYTES: usize = 128 * 1024 * 1024;

#[wasm_bindgen]
pub struct BrowserArrangementArchive {
    bytes: Vec<u8>,
    files: Vec<ImportFile>,
    skipped: usize,
}
#[wasm_bindgen]
impl BrowserArrangementArchive {
    #[wasm_bindgen(constructor)]
    pub fn new(bytes: Vec<u8>) -> Result<Self, JsValue> {
        if bytes.len() > MAX_BYTES {
            return Err(failure("Arrangement ZIP exceeds 128 MiB"));
        }
        let (files, skipped) =
            stem_archive::inspect_files(Cursor::new(&bytes), MAX_BYTES as u64).map_err(failure)?;
        if files.is_empty() {
            return Err(failure("Choose a ZIP containing WAV or MP3 audio"));
        }
        Ok(Self {
            bytes,
            files,
            skipped,
        })
    }
    pub fn info(&self) -> Result<String, JsValue> {
        serde_json::to_string(&serde_json::json!({"files":self.files,"skipped_files":self.skipped}))
            .map_err(failure)
    }
    pub fn extract_audio(&self, index: usize) -> Result<Vec<u8>, JsValue> {
        let file = self
            .files
            .iter()
            .find(|file| file.index == index)
            .ok_or_else(|| failure("Unknown arrangement ZIP entry"))?;
        let mut output = Vec::new();
        stem_archive::copy_audio(
            Cursor::new(&self.bytes),
            file,
            MAX_BYTES as u64,
            &mut output,
            &|| false,
        )
        .map_err(failure)?;
        Ok(output)
    }
}
