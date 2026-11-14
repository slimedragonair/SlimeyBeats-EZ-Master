use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Default, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(super) enum TutorialStatus {
    #[default]
    Unseen,
    Started,
    Completed,
    // Unknown future statuses suppress the automatic offer when opening an older build.
    #[serde(other)]
    Declined,
}

#[derive(Clone, Default, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub(super) struct InterfacePreferences {
    pub advanced: bool,
    pub tutorial_status: TutorialStatus,
    pub tutorial_step: usize,
}

impl InterfacePreferences {
    pub fn should_offer(&self) -> bool {
        matches!(
            self.tutorial_status,
            TutorialStatus::Unseen | TutorialStatus::Started
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn old_and_future_documents_do_not_repeat_a_finished_or_declined_tour() {
        let defaults: InterfacePreferences = serde_json::from_str("{}").unwrap();
        assert!(!defaults.advanced && defaults.should_offer());
        for status in ["completed", "declined", "completed_in_a_newer_build"] {
            let value: InterfacePreferences =
                serde_json::from_value(serde_json::json!({"tutorial_status":status})).unwrap();
            assert!(!value.should_offer(), "{status}");
        }
    }
}
