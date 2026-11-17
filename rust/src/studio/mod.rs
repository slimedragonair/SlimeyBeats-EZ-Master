//! Shared audio editing primitives; no model runtimes, filesystem or GUI dependencies.
pub mod arrangement;
pub mod arrangement_project;
pub mod arrangement_session;
pub mod doctor;
pub mod realtime_mix;
pub mod region_edit;
pub mod spectral_repair;
pub mod spectrogram;
pub mod stem_archive;
pub mod stem_import_plan;
pub mod stem_mix;
mod stft;
pub mod voice_cleanup;
mod voice_noise;
