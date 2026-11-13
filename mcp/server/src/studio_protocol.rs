//! Studio tool schemas/routes are kept separate from the mastering API.
use crate::options::ExportFormat;
use crate::{
    jobs::{Change, Product},
    protocol::Companion,
    studio_operations,
};
use mastercraft::{
    studio::{
        spectral_repair::RepairSelection,
        stem_mix::{MixSettings, StemKind},
        voice_cleanup::VoiceSettings,
    },
    studio_native::inference::SeparationMode,
};
use rmcp::{Json, handler::server::wrapper::Parameters, tool, tool_router};
use schemars::JsonSchema;
use serde::Deserialize;
use serde_json::Value;

#[derive(Clone, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct VoiceRequest {
    /// Loaded source_id, stem audio_id or current studio_preview_id. Discover IDs with get_studio_session.
    pub audio_id: String,
    #[serde(default)]
    pub settings: VoiceSettings,
}
#[derive(Clone, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct RepairRequest {
    pub audio_id: String,
    pub selection: RepairSelection,
}
#[derive(Clone, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct SeparateRequest {
    pub source_id: String,
    #[serde(default)]
    pub mode: SeparationMode,
}
#[derive(Clone, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct StemPatch {
    pub kind: StemKind,
    pub gain_db: Option<f64>,
    pub balance: Option<f64>,
    pub mute: Option<bool>,
    pub solo: Option<bool>,
}
#[derive(Clone, Default, Deserialize, JsonSchema)]
#[serde(default, deny_unknown_fields)]
pub struct MixPatch {
    pub tracks: Vec<StemPatch>,
    pub output_gain_db: Option<f64>,
    pub protect_peaks: Option<bool>,
    pub ceiling_dbtp: Option<f64>,
}
#[derive(Clone, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct MixRequest {
    pub stem_set_id: String,
    #[serde(default)]
    pub options: MixPatch,
}

#[derive(Clone, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct StudioExportRequest {
    pub studio_preview_id: String,
    pub name: String,
    #[serde(default)]
    pub format: ExportFormat,
    #[serde(default)]
    pub sample_rate: u32,
    #[serde(default = "dither_default")]
    pub dither: bool,
}
fn dither_default() -> bool {
    true
}
impl MixPatch {
    fn apply(&self, bank: &crate::studio_state::StemBank) -> anyhow::Result<MixSettings> {
        let mut settings = bank.controls.clone();
        let mut seen = std::collections::BTreeSet::new();
        for patch in &self.tracks {
            anyhow::ensure!(
                seen.insert(patch.kind),
                "A stem patch must not repeat a kind"
            );
            let track = settings
                .tracks
                .iter_mut()
                .find(|t| t.kind == patch.kind)
                .ok_or_else(|| anyhow::anyhow!("This stem is not in the current set"))?;
            if let Some(v) = patch.gain_db {
                track.gain_db = v;
            }
            if let Some(v) = patch.balance {
                track.balance = v;
            }
            if let Some(v) = patch.mute {
                track.mute = v;
            }
            if let Some(v) = patch.solo {
                track.solo = v;
            }
        }
        if let Some(v) = self.output_gain_db {
            settings.output_gain_db = v;
        }
        if let Some(v) = self.protect_peaks {
            settings.protect_peaks = v;
        }
        if let Some(v) = self.ceiling_dbtp {
            settings.ceiling_dbtp = v;
        }
        settings.validate(&bank.stems)?;
        Ok(settings)
    }
}

#[tool_router(router=studio_router,vis="pub(crate)")]
impl Companion {
    #[tool(
        description = "Explicitly save the current Studio voice/repair/stem-mix preview as a new WAV/FLAC/MP3 plus report. Requires current studio_preview_id and filename stem; no path or extension. Only on the user's export request. Existing files are preserved. Compressed output/resampling needs FFmpeg. Poll the job.",
        annotations(destructive_hint = false, open_world_hint = false)
    )]
    fn export_studio_audio(
        &self,
        Parameters(a): Parameters<StudioExportRequest>,
    ) -> Result<Json<Value>, String> {
        let id = a.studio_preview_id.clone();
        let name = a.name.clone();
        self.response(self.engine.start(
            "export_studio_audio",
            move |s| {
                crate::file_policy::export_stem(&name)?;
                anyhow::ensure!(
                    s.studio.current.as_ref().is_some_and(|p| p.id == id),
                    "Use the current studio_preview_id from get_studio_session"
                );
                Ok(())
            },
            move |ctx| {
                crate::studio_export::audio(
                    ctx,
                    a.studio_preview_id,
                    a.name,
                    a.format,
                    a.sample_rate,
                    a.dither,
                )
            },
        ))
    }
    #[tool(
        description = "Inspect whether the optional local CPU stem engine is installed. Setup downloads dependencies/model; processing uses local audio only.",
        annotations(read_only_hint = true, open_world_hint = false)
    )]
    fn get_stem_engine_status(&self) -> Json<Value> {
        Json(self.engine.stem_runtime.status())
    }

    #[tool(
        description = "Download/install the isolated local stem engine and verified model, then run an inference test. Use when the user requests/agrees to engine setup; needs internet and substantial disk space. Returns a job ID; poll it. Does not upload audio or modify global Python.",
        annotations(destructive_hint = false, open_world_hint = true)
    )]
    fn install_stem_engine(&self) -> Result<Json<Value>, String> {
        self.response(self.engine.start(
            "install_stem_engine",
            |_| Ok(()),
            |ctx| {
                let result = ctx.engine.stem_runtime.install(
                    &|fraction, message| ctx.progress(fraction, message),
                    ctx.cancel.clone(),
                )?;
                Ok(Product {
                    result,
                    change: Change::None,
                    committed: true,
                })
            },
        ))
    }

    #[tool(
        description = "Discover available original/stem/edited audio IDs, current stem controls, studio preview and undo/redo availability. Studio edits are separate from the mastering preview.",
        annotations(read_only_hint = true, open_world_hint = false)
    )]
    fn get_studio_session(&self) -> Result<Json<Value>, String> {
        self.response((|| {
            let store = self.engine.lock()?;
            store.session.studio.summary(&self.engine, &store.session)
        })())
    }

    #[tool(
        description = "Separate the loaded source into vocal/instrumental or four stems (vocals, drums, bass, other), using the installed local model. Poll the job. Creates a stem workspace and temporary neutral mix, not final exports. Model estimates can contain bleed/artifacts.",
        annotations(destructive_hint = false, open_world_hint = false)
    )]
    fn separate_stems(
        &self,
        Parameters(a): Parameters<SeparateRequest>,
    ) -> Result<Json<Value>, String> {
        let check = a.clone();
        self.response(self.engine.start(
            "separate_stems",
            move |s| s.source(&check.source_id).map(|_| ()),
            move |ctx| studio_operations::separate(ctx, a.source_id, a.mode),
        ))
    }

    #[tool(
        description = "Patch gain/mute/solo/stereo balance for current stems and render a temporary mix from immutable stem audio. Omitted settings are retained. Gain -60..+12 dB, balance -1..1 (mono requires 0); mute wins over solo. Poll for studio_preview_id; no final export.",
        annotations(destructive_hint = false, open_world_hint = false)
    )]
    fn mix_stems(&self, Parameters(a): Parameters<MixRequest>) -> Result<Json<Value>, String> {
        let result = (|| {
            let settings = {
                let store = self.engine.lock()?;
                let bank = store.session.studio_bank(&a.stem_set_id)?;
                a.options.apply(&bank)?
            };
            self.engine.start(
                "mix_stems",
                |s| s.studio_bank(&a.stem_set_id).map(|_| ()),
                {
                    let id = a.stem_set_id.clone();
                    move |ctx| studio_operations::mix(ctx, id, settings)
                },
            )
        })();
        self.response(result)
    }

    #[tool(
        description = "Prepare voice cleanup from a loaded source, vocal stem or current studio preview: rumble removal, conservative stationary-noise reduction, de-essing and optional gate. It does not isolate speech from music. Omitted settings use documented defaults. Poll; source is preserved and export remains separate.",
        annotations(destructive_hint = false, open_world_hint = false)
    )]
    fn clean_voice(&self, Parameters(a): Parameters<VoiceRequest>) -> Result<Json<Value>, String> {
        let check = a.clone();
        self.response(self.engine.start(
            "clean_voice",
            move |s| {
                let target = s.studio_audio(&check.audio_id)?;
                check.settings.validate(&target.audio)
            },
            move |ctx| studio_operations::voice(ctx, a.audio_id, a.settings),
        ))
    }

    #[tool(
        description = "Prepare a localized spectral attenuation/heal on an audio ID. Select start/end seconds and low/high Hz within Nyquist; reduction -60..0 dB. Samples outside selected time remain unchanged. Heal estimates surrounding magnitudes and needs unselected context. Poll, audition and undo if needed; no final export.",
        annotations(destructive_hint = false, open_world_hint = false)
    )]
    fn repair_audio(
        &self,
        Parameters(a): Parameters<RepairRequest>,
    ) -> Result<Json<Value>, String> {
        let check = a.clone();
        self.response(self.engine.start(
            "repair_audio",
            move |s| {
                let target = s.studio_audio(&check.audio_id)?;
                check.selection.validate(&target.audio)
            },
            move |ctx| studio_operations::repair(ctx, a.audio_id, a.selection),
        ))
    }

    #[tool(
        description = "Undo the last retained Studio edit, including the first voice/repair edit. Restores its audio and stem controls. Poll the job; inspect get_studio_session for the restored ID. Does not delete final exports.",
        annotations(destructive_hint = false, open_world_hint = false)
    )]
    fn undo_studio_edit(&self) -> Result<Json<Value>, String> {
        self.response(self.engine.start(
            "undo_studio_edit",
            |s| {
                anyhow::ensure!(
                    !s.studio.undo.is_empty(),
                    "No Studio undo step is available"
                );
                Ok(())
            },
            |ctx| studio_operations::history(ctx, false),
        ))
    }

    #[tool(
        description = "Redo a retained Studio edit after undo. A newly rendered edit clears redo. Poll, then use the returned/current Studio ID.",
        annotations(destructive_hint = false, open_world_hint = false)
    )]
    fn redo_studio_edit(&self) -> Result<Json<Value>, String> {
        self.response(self.engine.start(
            "redo_studio_edit",
            |s| {
                anyhow::ensure!(
                    !s.studio.redo.is_empty(),
                    "No Studio redo step is available"
                );
                Ok(())
            },
            |ctx| studio_operations::history(ctx, true),
        ))
    }
}
