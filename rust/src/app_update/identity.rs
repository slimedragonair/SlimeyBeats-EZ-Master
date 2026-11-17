//! Release identity independent of a version bump or a build timestamp.
use serde::{Deserialize, Serialize};

pub const VERSION: &str = env!("CARGO_PKG_VERSION");
pub const BUILD_REVISION: u64 = 2026111701;
pub const CHANNEL: &str = "studio-preview";

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BuildIdentity {
    pub version: String,
    pub build_revision: u64,
    pub channel: String,
}

pub fn current() -> BuildIdentity {
    BuildIdentity {
        version: VERSION.into(),
        build_revision: BUILD_REVISION,
        channel: CHANNEL.into(),
    }
}
