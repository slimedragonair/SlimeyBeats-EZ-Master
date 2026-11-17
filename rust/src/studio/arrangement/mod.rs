//! Non-destructive clip arrangements over immutable audio assets.
mod budget;
mod edits;
mod eq_processor;
mod gain;
mod history;
mod metadata;
mod plan;
mod playback;
mod render;
mod scoped_eq;
mod sources;
mod target;

pub use budget::MAX_METADATA_BYTES;
pub(crate) use budget::check as check_metadata;
pub(crate) use budget::serialized_bytes;
pub use edits::{Edit, apply_edit, apply_edits};
pub use eq_processor::PreparedEqUpdate;
pub use history::{ArrangementHistory, DEFAULT_HISTORY_METADATA_BYTES, HistorySnapshot};
pub(crate) use metadata::identifier as validate_identifier;
pub use metadata::{Arrangement, Clip, FrameRange, Lane, MAX_FRAME};
pub use playback::ArrangementPlayer;
pub use render::{RenderOptions, render};
pub use scoped_eq::{EqScope, ScopedEq};
pub use sources::{AssetLibrary, AssetValidationCache};
pub use target::{TargetAudio, TargetBasis, render_eq_target};
