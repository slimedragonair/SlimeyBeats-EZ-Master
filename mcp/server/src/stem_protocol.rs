use crate::{
    jobs::{Change, Product},
    protocol::Companion,
    studio_state::StemBank,
};
use anyhow::{Context, Result, ensure};
use mastercraft::studio::stem_mix::{self, StemKind};
use rmcp::{Json, handler::server::wrapper::Parameters, tool, tool_router};
use schemars::JsonSchema;
use serde::Deserialize;
use serde_json::{Value, json};
use std::sync::Arc;

#[derive(Clone, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct StemFile {
    pub kind: StemKind,
    pub library: usize,
    pub path: String,
}
#[derive(Clone, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ImportRequest {
    pub files: Vec<StemFile>,
}
#[derive(Clone, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct BankId {
    pub stem_set_id: String,
}
#[derive(Clone, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ApplyStem {
    pub stem_set_id: String,
    pub studio_preview_id: String,
    pub kind: StemKind,
}

#[tool_router(router=stem_router,vis="pub(crate)")]
impl Companion {
    #[tool(
        description = "Import at least two aligned user stems from authorized audio libraries without a neural model. No fixed stem-count cap; configured per-input decoded-audio limits (default 512 MiB), a separate native bank budget (4 GiB), timeout and available-memory limits apply. Assign unique kinds: vocals, instrumental, drums, bass, other, guitar, piano, synth, strings, percussion, or canonical part_N (positive decimal integer, e.g. part_10, part_24, part_64) for extra/custom parts. Do not reuse IDs. Rates, channels and frames must match; no hidden trimming/resampling. Replaces only Studio, creating a temporary mix. Poll; export is separate. Automatic model extraction remains two/four stems.",
        annotations(destructive_hint = false, open_world_hint = false)
    )]
    fn import_stems(
        &self,
        Parameters(a): Parameters<ImportRequest>,
    ) -> Result<Json<Value>, String> {
        self.response((|| {
            stem_mix::validate_kinds(a.files.iter().map(|file| file.kind))?;
            let paths = a
                .files
                .iter()
                .map(|file| {
                    Ok((
                        file.kind,
                        self.engine.policy.audio(file.library, &file.path)?,
                    ))
                })
                .collect::<Result<Vec<_>>>()?;
            self.engine.start(
                "import_stems",
                |_| Ok(()),
                move |ctx| {
                    let stems = mastercraft::studio_native::stem_files::import_with_limits(
                        &paths,
                        ctx.engine.max_decoded_bytes,
                        ctx.engine.decode_timeout,
                        ctx.cancel.clone(),
                        &|fraction, message| ctx.progress(fraction, message),
                    )?;
                    crate::studio_import::prepared(&ctx, stems, json!({"operation":"import_stems"}))
                },
            )
        })())
    }

    #[tool(
        description = "Explicitly export the current immutable stem bank as aligned 32-bit float WAVs in a new unique folder. Relative levels and source frames are retained; fader/solo settings affect only the mix preview. Float stems can exceed 0 dBFS without integer clipping. Only use after the user's stem-export request. Poll for folder/manifest.",
        annotations(destructive_hint = false, open_world_hint = false)
    )]
    fn export_stems(&self, Parameters(a): Parameters<BankId>) -> Result<Json<Value>, String> {
        let check = a.stem_set_id.clone();
        self.response(self.engine.start(
            "export_stems",
            move |s| s.studio_bank(&check).map(|_| ()),
            move |ctx| {
                let bank = ctx.snapshot.studio_bank(&a.stem_set_id)?;
                ctx.engine.policy.check_output()?;
                let mut result = mastercraft::studio_native::stem_files::export(
                    &bank.stems,
                    &ctx.engine.policy.output,
                    &ctx.cancel,
                )?;
                let path = std::path::PathBuf::from(
                    result["folder"]
                        .as_str()
                        .context("Stem exporter returned no folder")?,
                );
                result["folder"] = ctx.engine.policy.relative_output(&path)?.into();
                Ok(Product {
                    result,
                    change: Change::None,
                    committed: true,
                })
            },
        ))
    }

    #[tool(
        description = "Apply a prepared direct stem edit (voice cleanup/repair/plugin) back to that stem, then render the whole mix. Requires current preview and stem-set IDs. The preview must come directly from this stem; apply each edit before chaining another. Other stems remain unchanged; undo restores the prior bank. Poll for new IDs.",
        annotations(destructive_hint = false, open_world_hint = false)
    )]
    fn apply_preview_to_stem(
        &self,
        Parameters(a): Parameters<ApplyStem>,
    ) -> Result<Json<Value>, String> {
        let check = a.clone();
        self.response(self.engine.start("apply_preview_to_stem",move|s|{s.studio_bank(&check.stem_set_id)?;ensure!(s.studio.current.as_ref().is_some_and(|p|p.id==check.studio_preview_id),"Use the current Studio preview");Ok(())},move|ctx| {
            let bank=ctx.snapshot.studio_bank(&a.stem_set_id)?;let preview=ctx.snapshot.studio.current.as_ref().context("Prepare a stem edit first")?;
            let input_id=preview.steps.last().and_then(|s|s["input_audio_id"].as_str()).context("Preview has no direct stem edit")?;
            ensure!(input_id==crate::studio_state::stem_id(&bank.id,a.kind),"Preview must be rendered directly from the selected stem; apply each edit before chaining");
            let stems=bank.stems.replace(a.kind,preview.audio.clone())?;
            let audio=stem_mix::render(&stems,&bank.controls,&||ctx.cancelled())?;
            let mut studio=ctx.snapshot.studio.clone();
            studio.bank=Some(Arc::new(StemBank {id:format!("stems-{}",ctx.id),source_name:bank.source_name.clone(),stems,controls:bank.controls.clone(),model:bank.model.clone(),original_mix:bank.original_mix.clone()}));
            let mut target=crate::studio_state::AudioTarget::bank_mix(studio.bank.as_ref().expect("replacement bank"),"Mix with edited stem");
            target.steps=preview.steps.clone();
            let bank_id = studio.bank.as_ref().expect("replacement bank").id.clone();
            let mut product = crate::studio_operations::finish(&ctx,target,audio,json!({"operation":"replace_stem","kind":a.kind}),vec!["The prepared stem edit was applied to the mix; other stem audio was retained.".into()],studio)?;
            product.result["stem_set_id"] = bank_id.into();
            Ok(product)
        }))
    }
}
