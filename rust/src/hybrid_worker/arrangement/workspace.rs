use super::protocol::Command;
use crate::hybrid_worker::{input, paths::ProjectPaths, request::Limits};
use anyhow::{Context, Result, ensure};
use mastercraft::studio::{
    arrangement::{ArrangementPlayer, AssetValidationCache},
    arrangement_session::{ArrangementSession, OwnedAsset, SessionLimits, edit_label},
};
use serde_json::{Value, json};
use std::{path::Path, sync::Arc};

pub struct Workspace {
    pub(super) paths: ProjectPaths,
    pub(super) session: Option<ArrangementSession>,
    pub(super) limits: Limits,
    pub(super) player: Option<ArrangementPlayer>,
    pub(super) prepared_revision: Option<u64>,
    side: String,
    pub(super) cache: AssetValidationCache,
    block: Vec<f32>,
    pub(super) sources: std::collections::BTreeMap<String, super::fork::SourceFile>,
}
impl Workspace {
    pub fn new(paths: ProjectPaths) -> Self {
        Self {
            paths,
            session: None,
            limits: Limits::default(),
            player: None,
            prepared_revision: None,
            side: "after".into(),
            cache: AssetValidationCache::new(),
            block: vec![0.; 4096],
            sources: Default::default(),
        }
    }
    pub(super) fn session(&self) -> Result<&ArrangementSession> {
        self.session.as_ref().context("Create an arrangement first")
    }
    pub(super) fn selected(&self, revision: u64) -> Result<&ArrangementSession> {
        let session = self.session()?;
        session.check_revision(revision)?;
        Ok(session)
    }
    pub(super) fn snapshot(&self) -> Result<Value> {
        let session = self.session()?;
        let mut value = serde_json::to_value(session.read())?;
        value["preview"] = json!({"identity":format!("{}:{}:after",session.epoch(),session.revision()),
            "prepared_revision":self.prepared_revision,"pending":self.prepared_revision != Some(session.revision()),
            "frames":session.document().duration_frames(),"sample_rate":session.document().sample_rate,"channels":session.document().channels});
        value["defaults"] = json!({"bands":mastercraft::config::default_eq()});
        Ok(value)
    }
    fn playback_info(&self) -> Result<Value> {
        let player = self
            .player
            .as_ref()
            .context("Prepare arrangement playback first")?;
        Ok(
            json!({"prepared_revision":self.prepared_revision,"side":self.side,"position_frame":player.position(),
            "frames":player.duration_frames(),"sample_rate":player.sample_rate(),"channels":player.channels()}),
        )
    }
    pub fn execute(&mut self, command: Command) -> Result<Value> {
        match command {
            Command::InspectArchive { file } => self.inspect_archive(&file),
            Command::ExtractArchive {
                file,
                files,
                output_id,
                max_total_bytes,
            } => self.extract_archive(&file, &files, &output_id, max_total_bytes),
            Command::Create {
                sample_rate,
                channels,
                epoch,
                limits,
            } => {
                ensure!(self.session.is_none(), "Arrangement is already initialized");
                limits.validate()?;
                let session = ArrangementSession::new(
                    sample_rate,
                    channels,
                    epoch,
                    SessionLimits {
                        max_decoded_bytes: limits.max_bank_pcm_bytes as u64,
                        ..Default::default()
                    },
                )?;
                self.session = Some(session);
                self.limits = limits;
                self.snapshot()
            }
            Command::Read => self.snapshot(),
            Command::AddAssets {
                expected_revision,
                assets,
                edits,
            } => {
                self.selected(expected_revision)?;
                let mut owned = Vec::new();
                let mut registered = Vec::new();
                let existing =
                    self.session()?
                        .assets()
                        .values()
                        .try_fold(0_usize, |total, audio| {
                            total
                                .checked_add(audio.samples.len() * 4)
                                .context("Source pool size overflow")
                        })?;
                let mut remaining = self
                    .limits
                    .max_bank_pcm_bytes
                    .checked_sub(existing)
                    .context("Source pool exceeds its decoded budget")?;
                for asset in assets {
                    crate::hybrid_worker::paths::no_links(&asset.file)?;
                    let file = self.paths.input(&asset.file)?;
                    ensure!(
                        file.extension()
                            .is_some_and(|ext| ext.eq_ignore_ascii_case("wav")),
                        "Register normalized private WAV assets only"
                    );
                    let (audio, identity) = input::load_identified(&file, &self.limits, remaining)?;
                    registered.push((
                        asset.id.clone(),
                        super::fork::SourceFile::new(
                            asset.id.clone(),
                            asset.name.clone(),
                            file,
                            identity,
                            &audio,
                        ),
                    ));
                    remaining = remaining
                        .checked_sub(audio.samples.len() * 4)
                        .context("Source pool exceeds its decoded budget")?;
                    owned.push(OwnedAsset {
                        id: asset.id,
                        name: asset.name,
                        audio: Arc::new(audio),
                    });
                }
                self.session.as_mut().unwrap().add_assets(
                    expected_revision,
                    owned,
                    &edits,
                    "Add tracks",
                )?;
                self.sources.extend(registered);
                self.prepared_revision = None;
                self.snapshot()
            }
            Command::AddFiles {
                expected_revision,
                assets,
            } => {
                self.selected(expected_revision)?;
                let existing =
                    self.session()?
                        .assets()
                        .values()
                        .try_fold(0_usize, |total, audio| {
                            total
                                .checked_add(audio.samples.len() * 4)
                                .context("Source pool size overflow")
                        })?;
                let mut remaining = self
                    .limits
                    .max_bank_pcm_bytes
                    .checked_sub(existing)
                    .context("Source pool exceeds its decoded budget")?;
                let mut owned = Vec::new();
                let mut edits = Vec::new();
                let mut registered = Vec::new();
                for asset in assets {
                    crate::hybrid_worker::paths::no_links(&asset.file)?;
                    let file = self.paths.input(&asset.file)?;
                    let (audio, identity) = input::load_identified(&file, &self.limits, remaining)?;
                    registered.push((
                        asset.id.clone(),
                        super::fork::SourceFile::new(
                            asset.id.clone(),
                            asset.name.clone(),
                            file,
                            identity,
                            &audio,
                        ),
                    ));
                    remaining = remaining
                        .checked_sub(audio.samples.len() * 4)
                        .context("Source pool exceeds its decoded budget")?;
                    let mut lane = mastercraft::studio::arrangement::Lane::new(
                        format!("{}-lane", asset.id),
                        asset.name.clone(),
                    );
                    lane.clips.push(mastercraft::studio::arrangement::Clip {
                        id: format!("{}-clip", asset.id),
                        name: None,
                        source_id: asset.id.clone(),
                        start_frame: 0,
                        source_start_frame: 0,
                        end_frame: audio.frames() as u64,
                        gain_db: 0.,
                        fade_in_frames: 0,
                        fade_out_frames: 0,
                    });
                    edits.push(mastercraft::studio::arrangement::Edit::AddLane { lane });
                    owned.push(OwnedAsset {
                        id: asset.id,
                        name: asset.name,
                        audio: Arc::new(audio),
                    });
                }
                self.session.as_mut().unwrap().add_assets(
                    expected_revision,
                    owned,
                    &edits,
                    "Add tracks",
                )?;
                self.sources.extend(registered);
                self.prepared_revision = None;
                self.snapshot()
            }
            Command::Edit {
                expected_revision,
                edits,
            } => {
                let session = self.selected(expected_revision)?;
                let fast = self.side == "after"
                    && self.prepared_revision == Some(expected_revision)
                    && edits.iter().all(|edit| match edit {
                        mastercraft::studio::arrangement::Edit::SetLane { .. }
                        | mastercraft::studio::arrangement::Edit::SetClipGain { .. }
                        | mastercraft::studio::arrangement::Edit::SetClipName { .. } => true,
                        mastercraft::studio::arrangement::Edit::SetEq { effect } => {
                            session.document().eq.iter().any(|old| {
                                old.id == effect.id
                                    && old.scope == effect.scope
                                    && old.range == effect.range
                            })
                        }
                        _ => false,
                    });
                self.session.as_mut().unwrap().edit(
                    expected_revision,
                    &edits,
                    &edit_label(&edits),
                )?;
                if fast {
                    let session = self.session.as_ref().unwrap();
                    let player = self.player.as_mut().unwrap();
                    for edit in edits {
                        use mastercraft::studio::arrangement::Edit;
                        match edit {
                            Edit::SetLane { lane_id, .. } => {
                                let lane = session.document().lane(&lane_id)?;
                                player.set_lane_controls(
                                    &lane_id,
                                    lane.gain_db,
                                    lane.pan,
                                    lane.mute,
                                    lane.solo,
                                )?;
                            }
                            Edit::SetClipGain { clip_id, gain_db } => {
                                player.set_clip_gain(&clip_id, gain_db)?
                            }
                            Edit::SetEq { effect } => player.set_eq(&effect)?,
                            _ => {}
                        }
                    }
                    self.prepared_revision = Some(session.revision());
                } else {
                    self.prepared_revision = None;
                }
                self.snapshot()
            }
            Command::History {
                expected_revision,
                redo,
            } => {
                self.selected(expected_revision)?;
                let session = self.session.as_mut().unwrap();
                if redo {
                    session.redo(expected_revision)?;
                } else {
                    session.undo(expected_revision)?;
                }
                self.prepared_revision = None;
                self.snapshot()
            }
            Command::Waveform {
                expected_revision,
                asset_id,
                range,
                bins,
            } => Ok(serde_json::to_value(
                self.selected(expected_revision)?
                    .waveform(&asset_id, range, bins)?,
            )?),
            Command::EqResponse {
                expected_revision,
                effect,
            } => Ok(serde_json::to_value(
                self.selected(expected_revision)?.eq_response(&effect)?,
            )?),
            Command::SuggestEq {
                expected_revision,
                target,
                exclude_eq_id,
                profile,
            } => {
                let profiles = mastercraft::profiles::builtin();
                ensure!(
                    profile
                        .split('+')
                        .all(|part| profiles.contains_key(part.split(':').next().unwrap_or(""))),
                    "Choose a bundled profile"
                );
                let profile = mastercraft::profiles::resolve(&profile, Path::new(""))?;
                Ok(serde_json::to_value(
                    self.selected(expected_revision)?.suggest_eq_replacing(
                        &target,
                        exclude_eq_id.as_deref(),
                        &profile,
                        &|| false,
                    )?,
                )?)
            }
            Command::Prepare {
                expected_revision,
                side,
                frame,
            } => {
                ensure!(
                    matches!(side.as_str(), "before" | "after"),
                    "Choose before or after"
                );
                self.selected(expected_revision)?;
                let session = self.session.as_ref().unwrap();
                let doc = if side == "before" {
                    session.before()
                } else {
                    session.document()
                };
                let player = ArrangementPlayer::prepare_bounded(
                    doc,
                    session.assets(),
                    session.render_options(),
                    &mut self.cache,
                    frame.min(doc.duration_frames()),
                    doc.sample_rate as u64 * 3 / 2,
                    &|| false,
                )?;
                self.player = Some(player);
                self.prepared_revision = Some(expected_revision);
                self.side = side;
                self.playback_info()
            }
            Command::PlaybackInfo => self.playback_info(),
            Command::Block { frames } => {
                ensure!(
                    (1..=2048).contains(&frames),
                    "Playback block requires1..2048frames"
                );
                let player = self
                    .player
                    .as_mut()
                    .context("Prepare arrangement playback first")?;
                let channels = player.channels();
                let count = player.process_block(&mut self.block[..frames * channels])?;
                drop(player.take_retired()?);
                Ok(
                    json!({"frames":count,"channels":channels,"position_frame":player.position(),"samples":&self.block[..count*channels]}),
                )
            }
            Command::Export {
                expected_revision,
                output_id,
                format,
                sample_rate,
                dither,
            } => {
                ensure!(
                    output_id.len() == 32 && output_id.bytes().all(|b| b.is_ascii_hexdigit()),
                    "Use an opaque output ID"
                );
                ensure!(
                    ["wav16", "wav24", "wav32f", "flac16", "flac24", "mp3"]
                        .contains(&format.as_str()),
                    "Unsupported native output format"
                );
                let out = self
                    .paths
                    .directory(&self.paths.root.join("arrangement-output").join(&output_id))?;
                let session = self.selected(expected_revision)?;
                let mut options = session.render_options().clone();
                options.max_output_bytes = options
                    .max_output_bytes
                    .min(self.limits.max_bank_pcm_bytes as u64);
                let audio = mastercraft::studio::arrangement::render(
                    session.document(),
                    session.assets(),
                    &options,
                    &|| false,
                )?;
                let settings = mastercraft::config::Settings {
                    out_dir: out,
                    formats: vec![format],
                    output_sr: sample_rate,
                    dither,
                    ceiling_dbtp: options.ceiling_dbtp,
                    suffix: String::new(),
                    ..Default::default()
                };
                settings.validate()?;
                let files = mastercraft::export::export(
                    &audio,
                    Path::new("arrangement.wav"),
                    &settings,
                    &|| false,
                )?;
                Ok(json!({"exports":files,"revision":expected_revision}))
            }
            Command::ForkState {
                expected_revision,
                output_id,
                max_metadata_bytes,
            } => self.fork_state(expected_revision, &output_id, max_metadata_bytes),
            Command::RestoreFork { file } => self.restore_fork(&file),
            Command::SaveProject {
                expected_revision,
                output_id,
            } => self.save_project(expected_revision, &output_id),
            Command::ProjectManifest { expected_revision } => {
                let snapshot = self.selected(expected_revision)?.project_snapshot()?;
                let sources = snapshot
                    .assets
                    .iter()
                    .map(|(id, audio)| {
                        let bytes = mastercraft::studio::arrangement_project::encode_source(audio)?;
                        mastercraft::studio::arrangement_project::describe_source(
                            id.clone(),
                            snapshot.asset_names[id].clone(),
                            audio,
                            &bytes,
                        )
                    })
                    .collect::<Result<Vec<_>>>()?;
                Ok(serde_json::to_value(snapshot.manifest(sources)?)?)
            }
            Command::OpenProject {
                expected_revision,
                file,
                epoch,
            } => {
                self.selected(expected_revision)?;
                let file = self.paths.input(&file)?;
                let snapshot = if file
                    .extension()
                    .is_some_and(|ext| ext.eq_ignore_ascii_case("zip"))
                {
                    ensure!(
                        file.metadata()?.len() <= self.limits.max_bank_pcm_bytes as u64,
                        "Project ZIP exceeds its encoded budget"
                    );
                    let bytes = std::fs::read(file)?;
                    mastercraft::studio::arrangement_project::load_zip(
                        &bytes,
                        self.limits.max_bank_pcm_bytes as u64,
                        self.limits.max_bank_pcm_bytes as u64,
                    )?
                } else {
                    mastercraft::studio_native::arrangement_project::load(
                        &file,
                        self.limits.max_bank_pcm_bytes as u64,
                        &|| false,
                    )?
                };
                self.session.as_mut().unwrap().restore_project(
                    expected_revision,
                    snapshot,
                    epoch,
                )?;
                self.player = None;
                self.sources.clear();
                self.prepared_revision = None;
                self.cache = AssetValidationCache::new();
                self.side = "after".into();
                self.snapshot()
            }
        }
    }
}
