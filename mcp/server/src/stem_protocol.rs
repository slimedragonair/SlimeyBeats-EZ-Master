use crate::{
    jobs::{Change, Product},
    protocol::Companion,
    studio_state::{EditedAudio, StemBank, StudioState},
};
use anyhow::{Context, Result, ensure};
use mastercraft::studio::stem_mix::{self, Stem, StemKind, StemSet};
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
        description = "Import 2–4 aligned user stems from authorized audio libraries without any neural model. Assign each unique kind explicitly. Rates, channels and frame counts must match; no hidden trimming/resampling. Replaces only the Studio workspace, with a temporary neutral mix. Poll the job; final export is separate.",
        annotations(destructive_hint = false, open_world_hint = false)
    )]
    fn import_stems(
        &self,
        Parameters(a): Parameters<ImportRequest>,
    ) -> Result<Json<Value>, String> {
        self.response((|| {
            ensure!(
                (2..=4).contains(&a.files.len()),
                "Import two to four aligned stems"
            );
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
                    let mut tracks = Vec::new();
                    for (kind, path) in paths {
                        ctx.progress(0.05, "Decoding aligned local stems");
                        let (audio, _) = crate::audio_input::load(
                            &path,
                            ctx.engine.max_decoded_bytes,
                            ctx.engine.decode_timeout,
                            ctx.cancel.clone(),
                        )?;
                        tracks.push(Stem {
                            kind,
                            audio: Arc::new(audio),
                        });
                    }
                    let stems = StemSet::new(tracks)?;
                    let controls = stems.defaults();
                    let mut raw = controls.clone();
                    raw.protect_peaks = false;
                    let original_mix =
                        Arc::new(stem_mix::render(&stems, &raw, &|| ctx.cancelled())?);
                    let audio = stem_mix::render(&stems, &controls, &|| ctx.cancelled())?;
                    let bank = Arc::new(StemBank {
                        id: format!("stems-{}", ctx.id),
                        stems,
                        controls,
                        model: json!({"origin":"imported aligned stems"}),
                        original_mix,
                    });
                    let file = crate::artifact_files::preview(&ctx.engine, &audio)?;
                    let mut studio = StudioState {
                        bank: Some(bank.clone()),
                        ..Default::default()
                    };
                    studio.current = Some(Arc::new(EditedAudio {
                        id: format!("studio-{}", ctx.id),
                        label: "Imported stem mix".into(),
                        audio: Arc::new(audio),
                        before: bank.original_mix.clone(),
                        file: Some(file),
                        steps: vec![json!({"operation":"import_stems"})],
                        notes: vec![],
                        bank_snapshot: Some(bank),
                    }));
                    let result = studio.summary(&ctx.engine, &ctx.snapshot)?;
                    ctx.check()?;
                    Ok(Product {
                        result,
                        change: Change::Studio(studio),
                        committed: false,
                    })
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
            studio.bank=Some(Arc::new(StemBank {id:format!("stems-{}",ctx.id),stems,controls:bank.controls.clone(),model:bank.model.clone(),original_mix:bank.original_mix.clone()}));
            let target=crate::studio_state::AudioTarget {audio:bank.original_mix.clone(),label:"Mix with edited stem".into(),steps:preview.steps.clone()};
            crate::studio_operations::finish(&ctx,target,audio,json!({"operation":"replace_stem","kind":a.kind}),vec!["The prepared stem edit was applied to the mix; other stem audio was retained.".into()],studio)
        }))
    }
}
