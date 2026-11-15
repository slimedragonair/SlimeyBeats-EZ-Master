use super::sanitize;
use anyhow::{Result, ensure};
use serde::{Deserialize, Serialize};

pub const MAX_TITLE_BYTES: usize = 512;
pub const MAX_DETAILS_BYTES: usize = 16 * 1024;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "schemas", derive(schemars::JsonSchema))]
#[serde(rename_all = "snake_case")]
pub enum IssueCategory {
    Crash,
    FreezeOrSlow,
    AudioPlayback,
    ImportOrExport,
    MasteringOrEq,
    Stems,
    OptionalToolSetup,
    Updates,
    Recovery,
    Accessibility,
    #[default]
    Other,
}

/// Only sanitized text is retained. Do not fill these fields from logs/errors or
/// project metadata; they are for text the user deliberately types and reviews.
#[derive(Clone, Debug)]
pub struct ReportDraft {
    pub category: IssueCategory,
    title: String,
    details: String,
    text_was_changed: bool,
}

impl ReportDraft {
    pub fn new(category: IssueCategory, title: &str, details: &str) -> Result<Self> {
        ensure!(
            title.len() <= MAX_TITLE_BYTES,
            "Report title exceeds 512 bytes"
        );
        ensure!(
            details.len() <= MAX_DETAILS_BYTES,
            "Report description exceeds 16 KiB"
        );
        let title = sanitize::text(title, false);
        let details = sanitize::text(details, true);
        Ok(Self {
            category,
            text_was_changed: title.changed || details.changed,
            title: title.text,
            details: details.text,
        })
    }

    pub fn title(&self) -> &str {
        &self.title
    }

    pub fn details(&self) -> &str {
        &self.details
    }

    pub fn text_was_changed(&self) -> bool {
        self.text_was_changed
    }
}
