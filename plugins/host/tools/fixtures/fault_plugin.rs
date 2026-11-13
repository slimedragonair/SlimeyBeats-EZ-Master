//! Test-only native module: controlled exit, invalid factory, or a stalled probe.
//! Never distributed as an audio effect.
use std::{ffi::c_void, time::Duration};

#[unsafe(no_mangle)]
pub extern "system" fn GetPluginFactory() -> *mut c_void {
    match std::env::var("SLIMEYBEATS_VST_TEST_FAULT").as_deref() {
        Ok("exit") => std::process::exit(42),
        Ok("stall") => loop {
            std::thread::sleep(Duration::from_secs(1));
        },
        _ => std::ptr::null_mut(),
    }
}
