//! Local model/plugin adapters. Shared DSP stays in `studio` for browser reuse.
pub mod audio_import;
mod download;
pub mod inference;
mod install;
pub mod plugins;
pub mod runtime;
pub mod session;
pub mod stem_bundle;
pub mod stem_files;
