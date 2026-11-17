//! Private persistent-worker protocol. Public requests are projected by Python.
use crate::hybrid_worker::request::Limits;
use mastercraft::studio::{
    arrangement::{Edit, FrameRange, ScopedEq},
    arrangement_session::EqTarget,
};
use serde::Deserialize;
use std::path::PathBuf;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Envelope {
    pub id: u64,
    pub request: Command,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Asset {
    pub id: String,
    pub name: String,
    pub file: PathBuf,
}
#[derive(Deserialize)]
#[serde(tag = "operation", rename_all = "snake_case", deny_unknown_fields)]
pub enum Command {
    Create {
        sample_rate: u32,
        channels: usize,
        epoch: String,
        limits: Limits,
    },
    Read,
    InspectArchive {
        file: PathBuf,
    },
    ExtractArchive {
        file: PathBuf,
        files: Vec<mastercraft::studio::stem_import_plan::ImportFile>,
        output_id: String,
        max_total_bytes: u64,
    },
    AddAssets {
        expected_revision: u64,
        assets: Vec<Asset>,
        edits: Vec<Edit>,
    },
    AddFiles {
        expected_revision: u64,
        assets: Vec<Asset>,
    },
    Edit {
        expected_revision: u64,
        edits: Vec<Edit>,
    },
    History {
        expected_revision: u64,
        redo: bool,
    },
    Waveform {
        expected_revision: u64,
        asset_id: String,
        range: FrameRange,
        bins: usize,
    },
    EqResponse {
        expected_revision: u64,
        effect: ScopedEq,
    },
    SuggestEq {
        expected_revision: u64,
        target: EqTarget,
        exclude_eq_id: Option<String>,
        profile: String,
    },
    Prepare {
        expected_revision: u64,
        side: String,
        frame: u64,
    },
    PlaybackInfo,
    Block {
        frames: usize,
    },
    Export {
        expected_revision: u64,
        output_id: String,
        format: String,
        sample_rate: u32,
        dither: bool,
    },
    ProjectManifest {
        expected_revision: u64,
    },
    ForkState {
        expected_revision: u64,
        output_id: String,
        max_metadata_bytes: u64,
    },
    RestoreFork {
        file: PathBuf,
    },
    SaveProject {
        expected_revision: u64,
        output_id: String,
    },
    OpenProject {
        expected_revision: u64,
        file: PathBuf,
        epoch: String,
    },
}
