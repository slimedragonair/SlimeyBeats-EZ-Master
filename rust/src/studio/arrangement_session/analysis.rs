use super::{ArrangementSession, EqTarget};
use crate::{
    analysis,
    dsp::eq_suggestion::{self, EqSuggestion},
    studio::arrangement::{RenderOptions, render_eq_target},
};
use anyhow::Result;
use serde_json::Value;

impl ArrangementSession {
    pub fn suggest_eq(
        &self,
        target: &EqTarget,
        profile: &Value,
        cancel: &dyn Fn() -> bool,
    ) -> Result<EqSuggestion> {
        self.suggest_eq_replacing(target, None, profile, cancel)
    }
    pub fn suggest_eq_replacing(
        &self,
        target: &EqTarget,
        exclude_eq_id: Option<&str>,
        profile: &Value,
        cancel: &dyn Fn() -> bool,
    ) -> Result<EqSuggestion> {
        let options = RenderOptions {
            protect_peaks: false,
            max_output_bytes: self.limits.max_decoded_bytes,
            ..self.options.clone()
        };
        let selected = render_eq_target(
            self.document(),
            &self.assets,
            &target.scope,
            target.range,
            exclude_eq_id,
            &options,
            cancel,
        )?;
        let measured = analysis::analyze(&selected.audio, profile)?;
        let mut proposal = eq_suggestion::proposal(&measured, profile)?;
        proposal.notes.push(format!("Measured {} frames {}–{} of the selected scope; only the replaced EQ is excluded. The proposal remains unapplied.",
            selected.basis.coordinate_domain, selected.basis.range.start_frame, selected.basis.range.end_frame));
        Ok(proposal)
    }
}
