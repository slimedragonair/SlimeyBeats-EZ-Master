//! Deterministic issue measurements and explicitly selected, length-preserving
//! repairs. Findings are review prompts, not proof of damage. No automatic cuts,
//! normalization, model inference, or changes outside the selected frame range.
mod clip_runs;
mod declip;
mod low_mids;
mod repair;
mod scan;
mod spectrum;
mod thresholds;
mod types;
mod validation;

pub use repair::repair;
pub use scan::scan;
pub use types::{Issue, IssueKind, IssueReport, RepairAction, RepairRequest, RepairResult};
