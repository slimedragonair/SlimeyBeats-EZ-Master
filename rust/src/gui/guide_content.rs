use anyhow::{Result, ensure};
use serde::Deserialize;
use std::{collections::BTreeSet, sync::OnceLock};

#[derive(Deserialize)]
pub(super) struct Guide {
    pub version: u32,
    pub welcome: Welcome,
    pub steps: Vec<TourStep>,
    pub glossary: Vec<GlossaryEntry>,
    pub mcp: McpGuide,
}

#[derive(Deserialize)]
pub(super) struct Welcome {
    pub title: String,
    pub body: String,
    pub privacy_note: String,
}

#[derive(Deserialize)]
pub(super) struct TourStep {
    pub id: String,
    pub title: String,
    pub body: String,
    pub detail: String,
    pub desktop_target: String,
}

#[derive(Deserialize)]
pub(super) struct GlossaryEntry {
    pub term: String,
    pub description: String,
}

#[derive(Deserialize)]
pub(super) struct McpGuide {
    pub title: String,
    pub body: String,
    pub steps: Vec<String>,
    pub privacy_note: String,
}

impl Guide {
    fn parse(text: &str) -> Result<Self> {
        let value: Self = serde_json::from_str(text)?;
        ensure!(value.version == 1, "Unsupported learning guide version");
        ensure!(!value.steps.is_empty(), "The learning guide needs a tour");
        let mut ids = BTreeSet::new();
        for step in &value.steps {
            ensure!(
                !step.id.is_empty() && ids.insert(&step.id),
                "Duplicate or empty tutorial step"
            );
            ensure!(
                !step.title.is_empty() && !step.body.is_empty() && !step.detail.is_empty(),
                "Incomplete tutorial step"
            );
            ensure!(
                ["overview", "eq", "stems"].contains(&step.desktop_target.as_str()),
                "Unknown tutorial destination"
            );
        }
        Ok(value)
    }
}

pub(super) fn guide() -> &'static Guide {
    static GUIDE: OnceLock<Guide> = OnceLock::new();
    GUIDE.get_or_init(|| {
        Guide::parse(include_str!("../../assets/learning-guide.json"))
            .expect("Bundled learning guide must be valid")
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn bundled_content_has_clear_destinations_and_no_duplicate_steps() {
        let value = guide();
        assert_eq!(value.version, 1);
        assert!(value.steps.len() >= 6 && value.glossary.len() >= 10);
        assert!(!value.welcome.title.is_empty() && !value.mcp.steps.is_empty());
        let mut broken: serde_json::Value =
            serde_json::from_str(include_str!("../../assets/learning-guide.json")).unwrap();
        broken["steps"][1]["id"] = broken["steps"][0]["id"].clone();
        assert!(Guide::parse(&broken.to_string()).is_err());
    }
}
