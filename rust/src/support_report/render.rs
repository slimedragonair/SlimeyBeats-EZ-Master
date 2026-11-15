use super::{REVIEW_NOTICE, ReportContext, ReportDraft};
use anyhow::{Context, Result, ensure};
use std::fmt::Write;

pub const MAX_REPORT_BYTES: usize = 64 * 1024;

/// An immutable, reviewed document. No JSON/log attachment is hidden behind the
/// preview; the displayed Markdown is the complete payload written to disk.
#[derive(Clone, Debug)]
pub struct Report {
    markdown: String,
    text_was_changed: bool,
}

impl Report {
    pub fn markdown(&self) -> &str {
        &self.markdown
    }

    pub fn text_was_changed(&self) -> bool {
        self.text_was_changed
    }
}

pub fn build(context: &ReportContext, draft: &ReportDraft) -> Result<Report> {
    let mut markdown = String::with_capacity(4096 + draft.details().len());
    writeln!(markdown, "# SlimeyBeats support report\n")?;
    let delivery_notice = if context.is_mcp() {
        "This report is returned to your AI client and may reach its model provider before you review or save it. What you type into that client may already have reached its provider before filtering. Use only non-sensitive descriptions; the companion does not post issues or upload files."
    } else {
        "This report stays on your device until you choose to share it."
    };
    writeln!(markdown, "{REVIEW_NOTICE} {delivery_notice}\n")?;
    if context.is_mcp() {
        writeln!(
            markdown,
            "This report was prepared through the MCP companion. Diagnostics cover this connection only: linked_engine_build is the compiled library, not an installed desktop version. The installed desktop version is unknown. Tool readiness is not probed by report generation; unchecked values are unknown (null). Desktop recovery files are not inspected or exposed.\n"
        )?;
    }
    writeln!(markdown, "## User description\n")?;
    writeln!(
        markdown,
        "Category: {}\n\nTitle:\n",
        serde_json::to_value(draft.category)?
            .as_str()
            .context("Report category must be an enum label")?
    )?;
    indented(&mut markdown, draft.title())?;
    writeln!(markdown, "\nWhat happened / steps to reproduce:\n")?;
    indented(&mut markdown, draft.details())?;
    writeln!(markdown, "\n## Automatic diagnostics\n")?;
    writeln!(
        markdown,
        "```json\n{}\n```\n",
        serde_json::to_string_pretty(context)?
    )?;
    writeln!(
        markdown,
        "Text changed by the privacy filter: {}. Always review your description before sharing.",
        if draft.text_was_changed() {
            "yes"
        } else {
            "no"
        }
    )?;
    ensure!(
        markdown.len() <= MAX_REPORT_BYTES,
        "Support report exceeds 64 KiB"
    );
    Ok(Report {
        markdown,
        text_was_changed: draft.text_was_changed(),
    })
}

fn indented(output: &mut String, text: &str) -> std::fmt::Result {
    if text.is_empty() {
        return writeln!(output, "    (not provided)");
    }
    for line in text.split('\n') {
        writeln!(output, "    {line}")?;
    }
    Ok(())
}
