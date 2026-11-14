//! Recovery of application-owned helpers; third-party plugins are never downloaded here.
mod cache;
mod install;
use anyhow::{Context, Result};
use std::{
    path::PathBuf,
    sync::{Arc, atomic::AtomicBool},
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ToolKind {
    PluginHost,
    McpBridge,
}
impl ToolKind {
    pub fn key(self) -> &'static str {
        match self {
            Self::PluginHost => "vst3_host",
            Self::McpBridge => "mcp_bridge",
        }
    }
    pub fn executable(self) -> &'static str {
        match self {
            Self::PluginHost => "slimeybeats-vst3-host.exe",
            Self::McpBridge => "slimeybeats-mcp.exe",
        }
    }
    pub fn display_name(self) -> &'static str {
        match self {
            Self::PluginHost => "VST3 helper",
            Self::McpBridge => "AI companion",
        }
    }
    pub fn protocol(self) -> u32 {
        1
    }
}
pub fn locate(kind: ToolKind) -> Option<PathBuf> {
    if !cfg!(all(windows, target_arch = "x86_64")) {
        return None;
    }
    cache::ready(&root().ok()?, kind).ok().flatten()
}
pub fn ensure(
    kind: ToolKind,
    cancel: Arc<AtomicBool>,
    progress: &dyn Fn(f32, &str),
) -> Result<PathBuf> {
    install::ensure(&root()?, kind, cancel, progress)
}
fn root() -> Result<PathBuf> {
    Ok(crate::paths::cache_root()
        .context("No user-local helper cache is available")?
        .join("optional-tools/bundled"))
}

#[cfg(test)]
mod tests;
