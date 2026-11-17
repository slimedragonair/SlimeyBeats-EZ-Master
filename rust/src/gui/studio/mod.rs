mod arrangement_actions;
mod arrangement_clip_controls;
mod arrangement_eq_controls;
mod arrangement_eq_curve;
mod arrangement_eq_effects;
mod arrangement_eq_targets;
mod arrangement_events;
mod arrangement_fields;
mod arrangement_gestures;
mod arrangement_inspector;
#[cfg(test)]
mod arrangement_interaction_tests;
mod arrangement_lane_controls;
mod arrangement_loading;
mod arrangement_runtime;
mod arrangement_state;
#[cfg(test)]
mod arrangement_state_tests;
mod arrangement_timeline;
mod arrangement_toolbar;
mod arrangement_transport;
mod arrangement_waveforms;
mod arrangement_workspace;
pub mod controls;
pub(super) mod doctor_capture;
mod doctor_controls;
#[cfg(test)]
mod doctor_interaction_tests;
mod doctor_jobs;
mod doctor_state;
#[cfg(test)]
mod doctor_tests;
mod doctor_time;
mod doctor_waveform;
mod editor_audio;
mod editor_controls;
mod editor_envelope;
mod editor_state;
mod editor_timeline;
mod editor_toolbar;
#[cfg(test)]
mod interaction_tests;
mod jobs;
mod operations;
#[cfg(test)]
mod pending_mix_tests;
mod plugin_controls;
mod projects;
mod recovery;
pub(in crate::gui) use recovery::PreparedArrangement;
mod spectral_view;
pub mod state;
mod stem_bulk_import;
mod stem_import;
mod stem_import_operations;
mod transport;
pub mod workspace;
pub use state::Studio;

mod live_mix;
mod mix_controls;
mod preview_actions;
mod repair_controls;
mod tool_fields;
mod tool_inputs;
mod voice_controls;

#[cfg(test)]
mod layout_tests;

#[cfg(test)]
mod editor_tests;
