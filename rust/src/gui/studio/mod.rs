pub mod controls;
#[cfg(test)]
mod interaction_tests;
mod jobs;
mod operations;
#[cfg(test)]
mod pending_mix_tests;
mod plugin_controls;
mod projects;
mod spectral_view;
pub mod state;
mod stem_bulk_import;
mod stem_import;
mod stem_import_operations;
mod transport;
pub mod workspace;
pub use state::Studio;

mod mix_controls;
mod preview_actions;
mod repair_controls;
mod tool_fields;
mod tool_inputs;
mod voice_controls;

#[cfg(test)]
mod layout_tests;
