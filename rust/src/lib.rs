pub mod analysis;
pub mod audio;
pub mod branding;
mod clock;
pub mod config;
#[cfg(feature = "gui")]
pub mod desktop_integration;
pub mod dsp;
pub mod engine;
pub mod export;
#[cfg(feature = "gui")]
pub mod gui;
pub mod meters;
#[cfg(not(target_arch = "wasm32"))]
pub mod native_process;
pub mod paths;
pub mod preview;
#[cfg(not(target_arch = "wasm32"))]
mod process_group;
pub mod profiles;
pub mod report;
#[cfg(not(target_arch = "wasm32"))]
pub mod session_document;
#[cfg(not(target_arch = "wasm32"))]
pub mod source_identity;
pub mod studio;
#[cfg(not(target_arch = "wasm32"))]
pub mod studio_native;

pub mod wav_encoding;
