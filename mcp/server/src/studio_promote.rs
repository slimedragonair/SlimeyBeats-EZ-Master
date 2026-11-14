use crate::{
    jobs::{Change, Product},
    protocol::Companion,
    state::Input,
};
use anyhow::{Context, ensure};
use mastercraft::{session_document::SessionSource, source_identity::SourceIdentity};
use rmcp::{Json, handler::server::wrapper::Parameters, tool, tool_router};
use schemars::JsonSchema;
use serde::Deserialize;
use serde_json::{Value, json};
use std::sync::Arc;

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct PreviewId {
    pub studio_preview_id: String,
}

#[tool_router(router=studio_promote_router,vis="pub(crate)")]
impl Companion {
    #[tool(
        description = "Promote the current prepared Studio preview into the existing mastering workflow. Returns a NEW source_id; Analyze/Master/Manual EQ/Export then operate on that edited audio. Retains the Studio bank/history, invalidates only the old master and does not export final audio. Poll the job.",
        annotations(destructive_hint = false, open_world_hint = false)
    )]
    fn use_studio_for_mastering(
        &self,
        Parameters(a): Parameters<PreviewId>,
    ) -> Result<Json<Value>, String> {
        let check = a.studio_preview_id.clone();
        self.response(self.engine.start("use_studio_for_mastering",move|s|{ensure!(s.studio.current.as_ref().is_some_and(|p|p.id==check),"Use the current Studio preview ID");Ok(())},move|ctx| {
            let preview=ctx.snapshot.studio.current.as_ref().context("Prepare a Studio preview first")?;
            let file=if let Some(file)=&preview.file {file.clone()}else{crate::artifact_files::preview(&ctx.engine,&preview.audio)?};
            let path=file.as_ref().to_path_buf();let identity=SourceIdentity::read(&path,&||ctx.cancelled())?;
            let id=format!("source-{}",ctx.id);
            let source=Arc::new(Input {id:id.clone(),source:SessionSource {path,identity},audio:preview.audio.clone(),generated:Some(file),studio_preview:Some(preview.clone())});
            Ok(Product {result:json!({"source_id":id,"label":preview.label,"next":"Analyze/master using this new source_id. Your Studio bank/history is retained; final export is still separate."}),change:Change::Promote(source),committed:false})
        }))
    }
}
