//! Keep support-report preview and reviewed-byte saving on their original guards.
use crate::{
    protocol::Companion,
    support_report_protocol::{PrepareSupportReport, SaveSupportReport},
};
use rmcp::{Json, handler::server::wrapper::Parameters, tool, tool_router};
use schemars::JsonSchema;
use serde::Deserialize;
use serde_json::Value;

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub(crate) struct SupportReportRequest {
    pub operation: SupportReportOperation,
}

#[derive(Deserialize, JsonSchema)]
#[serde(tag = "action", rename_all = "snake_case", deny_unknown_fields)]
pub(crate) enum SupportReportOperation {
    Preview(PrepareSupportReport),
    Save(SaveSupportReport),
}

#[tool_router(router=compact_support_router,vis="pub(crate)")]
impl Companion {
    #[tool(
        description = "Two-step local support report. operation.action=preview accepts optional non-sensitive category/title/details and returns the complete immutable preview_markdown plus review_id; it reads no audio/projects/logs/private recovery cache and creates no file. Tool input/output may reach the AI provider before review; never autofill raw errors, paths, names or secrets. Show the ENTIRE preview to the owner. Only on explicit owner review and save approval use operation.action=save with the latest review_id, a fresh .md filename and owner_review_confirmed:true (default false). A new preview invalidates earlier IDs; saves preserve exact reviewed bytes, refuse overwrite and never upload/post/open a browser. Consent is a caller assertion, never infer it. These operations return immediately; no job polling.",
        annotations(
            read_only_hint = false,
            destructive_hint = false,
            open_world_hint = false
        )
    )]
    pub(crate) fn support_report(
        &self,
        Parameters(request): Parameters<SupportReportRequest>,
    ) -> Result<Json<Value>, String> {
        match request.operation {
            SupportReportOperation::Preview(value) => {
                self.prepare_support_report(Parameters(value))
            }
            SupportReportOperation::Save(value) => self.save_support_report(Parameters(value)),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::region_edit_test_support::Workspace;
    use serde_json::json;

    #[test]
    fn support_schema_is_an_object_and_rejects_untyped_or_extra_inputs() {
        let schema = serde_json::to_value(schemars::schema_for!(SupportReportRequest)).unwrap();
        assert_eq!(schema["type"], "object");
        assert_eq!(schema["additionalProperties"], false);
        assert_eq!(schema["required"], json!(["operation"]));
        for invalid in [
            json!({"operation":{"action":"preview","logs":"private"}}),
            json!({"operation":{"action":"save","filename":"report.md"}}),
            json!({"operation":{"action":"save","review_id":"r","filename":"r.md","owner_review_confirmed":"true"}}),
            json!({"operation":{"action":"save","review_id":"r","filename":"r.md","url":"https://invalid.test"}}),
        ] {
            assert!(serde_json::from_value::<SupportReportRequest>(invalid).is_err());
        }
    }

    #[test]
    fn support_dispatch_keeps_consent_latest_review_and_exact_saved_bytes() {
        let workspace = Workspace::new();
        let companion = Companion::new(workspace.engine.clone());
        let before = workspace.engine.session().unwrap();
        let request = |operation| {
            Parameters(
                serde_json::from_value::<SupportReportRequest>(json!({"operation":operation}))
                    .unwrap(),
            )
        };
        let first = companion
            .support_report(request(
                json!({"action":"preview","title":"Playback stops"}),
            ))
            .unwrap()
            .0;
        let current = companion
            .support_report(request(
                json!({"action":"preview","title":"Reviewed reproduction"}),
            ))
            .unwrap()
            .0;
        let output = workspace.engine.policy.output.join("approved.md");
        let unconfirmed = companion
            .support_report(request(
                json!({"action":"save","review_id":current["review_id"],"filename":"approved.md"}),
            ))
            .err()
            .expect("saving must reject missing owner review");
        assert!(unconfirmed.contains("owner_review_confirmed defaults false"));
        let stale = companion.support_report(request(json!({"action":"save","review_id":first["review_id"],"filename":"approved.md","owner_review_confirmed":true}))).err().expect("saving must reject a stale review");
        assert!(stale.contains("stale or belongs to another connection"));
        assert!(!output.exists());
        companion.support_report(request(json!({"action":"save","review_id":current["review_id"],"filename":"approved.md","owner_review_confirmed":true}))).unwrap();
        assert_eq!(
            std::fs::read_to_string(&output).unwrap(),
            current["preview_markdown"].as_str().unwrap()
        );
        assert!(companion.support_report(request(json!({"action":"save","review_id":current["review_id"],"filename":"approved.md","owner_review_confirmed":true}))).is_err());
        assert_eq!(workspace.engine.session().unwrap(), before);
        let store = workspace.engine.lock().unwrap();
        assert_eq!(store.sequence, 0);
        assert!(store.jobs.is_empty() && store.active.is_none());
    }
}
