use super::*;
use std::io::Cursor;

#[wasm_bindgen]
impl BrowserArrangement {
    pub fn export_wav(
        &self,
        expected: u64,
        format: &str,
        dither: bool,
    ) -> Result<Vec<u8>, JsValue> {
        let work = (|| -> Result<Vec<u8>> {
            self.session.check_revision(expected)?;
            let audio = mastercraft::studio::arrangement::render(
                self.session.document(),
                self.session.assets(),
                self.session.render_options(),
                &|| false,
            )?;
            let settings = mastercraft::config::Settings {
                ceiling_dbtp: self.session.render_options().ceiling_dbtp,
                dither,
                ..mastercraft::config::Settings::default()
            };
            Ok(crate::wav_output::encode_checked(audio, format, &settings)?.0)
        })();
        work.map_err(failure)
    }
    pub fn asset_wav(&self, asset_id: &str) -> Result<Vec<u8>, JsValue> {
        let work = (|| -> Result<Vec<u8>> {
            let audio = self
                .session
                .assets()
                .get(asset_id)
                .ok_or_else(|| anyhow::anyhow!("Arrangement source ID is unavailable"))?;
            let mut output = Cursor::new(Vec::new());
            mastercraft::wav_encoding::write_wav(audio, &mut output, "wav32f", false)?;
            Ok(output.into_inner())
        })();
        work.map_err(failure)
    }
}
