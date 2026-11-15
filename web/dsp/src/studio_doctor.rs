//! Read-only issue reports and explicitly requested conservative preview edits.
use super::BrowserStudio;
use crate::failure;
use anyhow::Result;
use mastercraft::studio::doctor::{self, RepairRequest};
use wasm_bindgen::prelude::*;

impl BrowserStudio {
    fn doctor_checked(&mut self, target: &str, json: &str) -> Result<()> {
        let input = self.audio(target)?;
        let request: RepairRequest = serde_json::from_str(json)?;
        let result = doctor::repair(&input, &request, &|| false)?;
        // Finish validation and rendering before creating any history revision.
        self.push(result.audio);
        self.preview_is_mix = false;
        self.before = Some(input);
        self.notes = result.notes;
        Ok(())
    }
}

#[wasm_bindgen]
impl BrowserStudio {
    pub fn scan_issues(&self, target: &str) -> Result<String, JsValue> {
        let input = self.audio(target).map_err(failure)?;
        let report = doctor::scan(&input, &|| false).map_err(failure)?;
        serde_json::to_string(&report).map_err(failure)
    }

    /// Like voice/repair, create a preview; replacing a bank stem stays explicit.
    pub fn doctor(&mut self, target: &str, json: &str) -> Result<String, JsValue> {
        self.doctor_checked(target, json).map_err(failure)?;
        self.info()
    }
}

#[cfg(test)]
#[path = "studio_doctor_tests.rs"]
mod tests;
