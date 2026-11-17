//! Explicit canonical project save/load; validation completes before adoption.
use super::*;
use mastercraft::studio::arrangement_project::{describe_source, encode_source, load_zip};

#[wasm_bindgen]
impl BrowserArrangement {
    pub fn project_manifest(&self, expected: u64) -> Result<String, JsValue> {
        let work = (|| -> Result<String> {
            self.session.check_revision(expected)?;
            let snapshot = self.session.project_snapshot()?;
            let sources = snapshot
                .assets
                .iter()
                .map(|(id, audio)| {
                    let bytes = encode_source(audio)?;
                    describe_source(id.clone(), snapshot.asset_names[id].clone(), audio, &bytes)
                })
                .collect::<Result<Vec<_>>>()?;
            Ok(String::from_utf8(snapshot.manifest(sources)?.to_json()?)?)
        })();
        work.map_err(failure)
    }
    pub fn open_project_zip(
        &mut self,
        expected: u64,
        bytes: Vec<u8>,
        epoch: &str,
    ) -> Result<String, JsValue> {
        let work = (|| -> Result<String> {
            self.session.check_revision(expected)?;
            let snapshot = load_zip(&bytes, 256 * 1024 * 1024, MAX_SOURCE_BYTES)?;
            self.session
                .restore_project(expected, snapshot, epoch.into())?;
            self.player = None;
            self.player_revision = None;
            self.player_side = "after".into();
            self.cache = AssetValidationCache::new();
            self.staged.clear();
            self.staged_revision = None;
            self.snapshot()
        })();
        work.map_err(failure)
    }
}
