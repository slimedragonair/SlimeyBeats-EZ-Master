//! Local, allowlisted support reports. Nothing here reads sessions, logs or audio,
//! contacts the network, opens a browser, or submits a report.
//!
//! Construct `ReportContext::current()`, set its typed diagnostics, then call
//! `build(&context, &draft)`. Preview and copy `Report::markdown()` verbatim;
//! `Report::write_new()` saves those exact bytes without overwriting a file.
//! User-entered text is best-effort sanitized and still requires user review.

mod context;
mod draft;
mod render;
mod sanitize;
mod sanitize_paths;
mod sanitize_secrets;
mod storage;

pub use context::{
    Activity, Architecture, Counts, OperatingSystem, ReportContext, Tools, Workflow,
};
pub use draft::{IssueCategory, MAX_DETAILS_BYTES, MAX_TITLE_BYTES, ReportDraft};
pub use render::{MAX_REPORT_BYTES, Report, build};

/// Security-sensitive reports belong in the repository's private advisory flow.
/// Opening this URL is an explicit GUI action; this backend never opens it.
pub const PRIVATE_SECURITY_REPORT_URL: &str =
    "https://github.com/slimedragonair/SlimeyBeats-EZ-Master/security/advisories/new";

pub const REVIEW_NOTICE: &str = "Review every line before sharing. Automatic diagnostics contain only app/build identity, OS/architecture, workflow, counts and yes/no/unknown states. Tool readiness is not checked automatically: null means unknown, true means observed ready and false means observed not ready. Your typed text is checked for obvious paths, email addresses and secrets, but automatic redaction cannot guarantee removal of personal or sensitive information. No audio, projects, names, raw logs or error messages are collected automatically.";

#[cfg(test)]
mod privacy_tests;
#[cfg(test)]
mod tests;
