use super::*;
use mastercraft::studio::{
    arrangement::{FrameRange, ScopedEq},
    arrangement_session::EqTarget,
};

#[wasm_bindgen]
impl BrowserArrangement {
    pub fn waveform(
        &self,
        asset_id: &str,
        start: u64,
        end: u64,
        bins: u32,
    ) -> Result<String, JsValue> {
        let work = (|| -> Result<String> {
            let value = self.session.waveform(
                asset_id,
                FrameRange {
                    start_frame: start,
                    end_frame: end,
                },
                bins as usize,
            )?;
            Ok(serde_json::to_string(&value)?)
        })();
        work.map_err(failure)
    }
    pub fn eq_response(&self, json: &str) -> Result<String, JsValue> {
        let work = (|| -> Result<String> {
            ensure!(
                json.len() <= MAX_METADATA_BYTES,
                "EQ metadata exceeds 16 MiB"
            );
            let effect: ScopedEq = serde_json::from_str(json)?;
            Ok(serde_json::to_string(&self.session.eq_response(&effect)?)?)
        })();
        work.map_err(failure)
    }
    pub fn suggest_eq(
        &self,
        expected: u64,
        target: &str,
        profile_name: &str,
        exclude_eq_id: &str,
    ) -> Result<String, JsValue> {
        let work = (|| -> Result<String> {
            self.session.check_revision(expected)?;
            ensure!(
                target.len() <= MAX_METADATA_BYTES,
                "EQ target exceeds 16 MiB"
            );
            let target: EqTarget = serde_json::from_str(target)?;
            let proposal = self.session.suggest_eq_replacing(
                &target,
                (!exclude_eq_id.is_empty()).then_some(exclude_eq_id),
                &crate::profile(profile_name)?,
                &|| false,
            )?;
            Ok(serde_json::to_string(&proposal)?)
        })();
        work.map_err(failure)
    }
}
