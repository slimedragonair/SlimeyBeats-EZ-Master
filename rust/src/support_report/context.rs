use crate::app_update::{self, BuildIdentity};
use anyhow::{Result, ensure};
use serde::Serialize;

/// All automatic diagnostics are finite enums, numbers, optional booleans or build
/// constants. There is deliberately no arbitrary string/error/log/path field.
#[derive(Clone, Debug, Serialize)]
pub struct ReportContext {
    #[serde(flatten)]
    identity: ReportingIdentity,
    operating_system: OperatingSystem,
    architecture: Architecture,
    pub workflow: Workflow,
    pub counts: Counts,
    pub tools: Tools,
    pub activity: Activity,
}

impl ReportContext {
    pub fn current() -> Self {
        Self {
            identity: ReportingIdentity::Desktop {
                application: app_update::current(),
            },
            operating_system: OperatingSystem::current(),
            architecture: Architecture::current(),
            workflow: Workflow::Other,
            counts: Counts::default(),
            tools: Tools::default(),
            activity: Activity::default(),
        }
    }

    /// The compiled companion is the reporter, not an observation of an
    /// installed GUI. Accept only its trusted compile-time package version.
    pub fn for_mcp(companion_version: &'static str) -> Result<Self> {
        ensure!(
            companion_version.len() <= 128 && semver::Version::parse(companion_version).is_ok(),
            "Companion version must be a bounded compile-time semantic version"
        );
        let mut context = Self::current();
        context.identity = ReportingIdentity::Mcp {
            component: McpComponent::McpCompanion,
            companion_version,
            linked_engine_build: app_update::current(),
            installed_desktop_version: None,
            diagnostic_scope: McpScope::ThisConnectionOnly,
            tool_readiness_probed: false,
        };
        context.workflow = Workflow::McpSetup;
        Ok(context)
    }

    /// Compiled engine identity. In a companion report this is the linked
    /// library build, never evidence of the installed desktop's version.
    pub fn application(&self) -> &BuildIdentity {
        match &self.identity {
            ReportingIdentity::Desktop { application } => application,
            ReportingIdentity::Mcp {
                linked_engine_build,
                ..
            } => linked_engine_build,
        }
    }

    pub fn operating_system(&self) -> OperatingSystem {
        self.operating_system
    }

    pub fn architecture(&self) -> Architecture {
        self.architecture
    }

    pub fn is_mcp(&self) -> bool {
        matches!(self.identity, ReportingIdentity::Mcp { .. })
    }
}

#[derive(Clone, Debug, Serialize)]
#[serde(untagged)]
enum ReportingIdentity {
    Desktop {
        application: BuildIdentity,
    },
    Mcp {
        component: McpComponent,
        companion_version: &'static str,
        linked_engine_build: BuildIdentity,
        installed_desktop_version: Option<()>,
        diagnostic_scope: McpScope,
        tool_readiness_probed: bool,
    },
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "snake_case")]
enum McpComponent {
    McpCompanion,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "snake_case")]
enum McpScope {
    ThisConnectionOnly,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum OperatingSystem {
    Windows,
    Linux,
    MacOs,
    Android,
    Ios,
    Other,
}

impl OperatingSystem {
    pub fn current() -> Self {
        match std::env::consts::OS {
            "windows" => Self::Windows,
            "linux" => Self::Linux,
            "macos" => Self::MacOs,
            "android" => Self::Android,
            "ios" => Self::Ios,
            _ => Self::Other,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Architecture {
    X86,
    X86_64,
    Arm,
    Aarch64,
    Other,
}

impl Architecture {
    pub fn current() -> Self {
        match std::env::consts::ARCH {
            "x86" => Self::X86,
            "x86_64" => Self::X86_64,
            "arm" => Self::Arm,
            "aarch64" => Self::Aarch64,
            _ => Self::Other,
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Workflow {
    Overview,
    ManualEq,
    ProcessingReport,
    StemStudio,
    VoiceCleanup,
    SpectralRepair,
    PluginProcessing,
    UpdatesAndTools,
    McpSetup,
    Recovery,
    #[default]
    Other,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize)]
pub struct Counts {
    pub tracks: u32,
    pub stems: u32,
    pub eq_bands: u32,
}

/// Already-known observations only: None means unknown/not checked, Some(true)
/// means observed ready, Some(false) means observed not ready. Constructing a
/// report never probes or downloads a tool to fill these fields.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize)]
pub struct Tools {
    pub audio_codecs_ready: Option<bool>,
    pub stem_engine_ready: Option<bool>,
    pub vst3_host_ready: Option<bool>,
    pub mcp_bridge_ready: Option<bool>,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize)]
pub struct Activity {
    pub session_loaded: bool,
    pub preview_available: bool,
    pub processing_active: bool,
    pub recovery_available: bool,
}
