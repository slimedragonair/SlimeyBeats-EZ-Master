//! Apply shared time-region DSP as an immutable stem-bank revision.
use super::BrowserStudio;
use crate::failure;
use anyhow::{Context, Result};
use mastercraft::studio::{
    region_edit::{self, RegionEdit},
    stem_mix::{self, StemKind},
};
use std::sync::Arc;
use wasm_bindgen::prelude::*;

impl BrowserStudio {
    fn edit_region_checked(&mut self, kind: &str, json: &str) -> Result<()> {
        let kind: StemKind = kind.parse()?;
        let edit: RegionEdit = serde_json::from_str(json)?;
        let bank = self.bank.as_ref().context("Import aligned stems first")?;
        let edited = region_edit::render(&bank.track(kind)?.audio, &edit, &|| false)?;
        let bank = bank.replace(kind, Arc::new(edited))?;
        let mixed = stem_mix::render(&bank, &self.mix, &|| false)?;

        // Retain the old bank in history only after both renders succeed.
        self.push(mixed);
        self.preview_is_mix = true;
        self.before = self.source.clone();
        self.bank = Some(bank);
        self.notes.clear();
        Ok(())
    }
}

#[wasm_bindgen]
impl BrowserStudio {
    pub fn edit_region(&mut self, kind: &str, json: &str) -> Result<String, JsValue> {
        self.edit_region_checked(kind, json).map_err(failure)?;
        self.info()
    }
}

#[cfg(test)]
#[path = "studio_region_edit_tests.rs"]
mod tests;
