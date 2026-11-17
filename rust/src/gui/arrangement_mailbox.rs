//! Latest document/control requests and bounded prepared-engine ownership handoff.
use super::{arrangement_controls::ControlBundle, player::Controls};
use crate::studio::arrangement::{
    Arrangement, ArrangementPlayer, AssetLibrary, PreparedEqUpdate, RenderOptions, ScopedEq,
};
use anyhow::{Context, Result, ensure};
use std::{
    collections::BTreeMap,
    sync::{
        Arc, Condvar, Mutex,
        atomic::{AtomicBool, AtomicU64, Ordering},
        mpsc::{SyncSender, TrySendError},
    },
};

// Compiled coefficients stay inline in an off-thread allocated bundle. Boxing
// this variant alone would require retiring another allocation on every update.
#[allow(clippy::large_enum_variant)]
pub(super) enum FastUpdate {
    Lane {
        id: String,
        gain: f64,
        pan: f64,
        mute: bool,
        solo: bool,
    },
    Clip {
        id: String,
        gain: f64,
    },
    Eq(PreparedEqUpdate),
    Output(f64),
}
pub(super) enum Retired {
    Player(Box<ArrangementPlayer>),
    Controls(Box<ControlBundle>),
}
pub(super) struct Prepared {
    pub epoch: u64,
    pub seek: bool,
    pub player: Box<ArrangementPlayer>,
}
pub(super) struct State {
    pub document: Arrangement,
    pub assets: AssetLibrary,
    pub options: RenderOptions,
    pub request: bool,
    pub ready: Option<Prepared>,
    pub fast: BTreeMap<String, FastUpdate>,
    pub control_ready: Option<Box<ControlBundle>>,
    pub error: Option<String>,
}
pub(super) struct Shared {
    pub writer: Mutex<()>,
    pub state: Mutex<State>,
    pub wake: Condvar,
    pub epoch: AtomicU64,
    pub seek: AtomicU64,
    pub closed: AtomicBool,
    pub preparing: AtomicBool,
    pub duration: AtomicU64,
    pub controls: Arc<Controls>,
    pub retired: SyncSender<Retired>,
    pub rate: u32,
    pub channels: usize,
    #[cfg(test)]
    pub hold_preparation: AtomicBool,
    #[cfg(test)]
    pub preparation_entered: AtomicBool,
    #[cfg(test)]
    pub hold_retirement: AtomicBool,
    #[cfg(test)]
    pub retirement_held: AtomicBool,
}
#[derive(Clone)]
pub(super) struct ArrangementMailbox(pub Arc<Shared>);
impl ArrangementMailbox {
    pub fn close(&self) {
        self.0.closed.store(true, Ordering::Release);
        self.0.epoch.fetch_add(1, Ordering::AcqRel);
        self.0.wake.notify_one();
    }
    pub fn preparing(&self) -> bool {
        self.0.preparing.load(Ordering::Acquire)
    }
    pub fn error(&self) -> Option<String> {
        self.0
            .state
            .lock()
            .map(|state| state.error.clone())
            .unwrap_or_else(|_| Some("Arrangement preparation lock failed".into()))
    }
    pub fn set(
        &self,
        document: &Arrangement,
        assets: &AssetLibrary,
        options: &RenderOptions,
    ) -> Result<()> {
        let _writer = self
            .0
            .writer
            .lock()
            .map_err(|_| anyhow::anyhow!("Arrangement control lock failed"))?;
        self.set_inner(document, assets, options)
    }
    fn set_inner(
        &self,
        document: &Arrangement,
        assets: &AssetLibrary,
        options: &RenderOptions,
    ) -> Result<()> {
        ensure!(
            !self.0.closed.load(Ordering::Acquire),
            "Arrangement player is closed"
        );
        document.validate(assets)?;
        options.validate()?;
        ensure!(
            document.sample_rate == self.0.rate && document.channels == self.0.channels,
            "Load a new arrangement player when changing project layout"
        );
        let mut state = self
            .0
            .state
            .lock()
            .map_err(|_| anyhow::anyhow!("Arrangement preparation lock failed"))?;
        let geometry = !same_geometry(&state.document, document)
            || !same_asset_catalog(&state.assets, assets)
            || state.options.protect_peaks != options.protect_peaks
            || state.options.ceiling_dbtp != options.ceiling_dbtp;
        let mut fast = vec![];
        if !geometry {
            for (old, lane) in state.document.lanes.iter().zip(&document.lanes) {
                if (old.gain_db, old.pan, old.mute, old.solo)
                    != (lane.gain_db, lane.pan, lane.mute, lane.solo)
                {
                    fast.push((
                        format!("lane:{}", lane.id),
                        FastUpdate::Lane {
                            id: lane.id.clone(),
                            gain: lane.gain_db,
                            pan: lane.pan,
                            mute: lane.mute,
                            solo: lane.solo,
                        },
                    ));
                }
                for (old, clip) in old.clips.iter().zip(&lane.clips) {
                    if old.gain_db != clip.gain_db {
                        fast.push((
                            format!("clip:{}", clip.id),
                            FastUpdate::Clip {
                                id: clip.id.clone(),
                                gain: clip.gain_db,
                            },
                        ));
                    }
                }
            }
            for (old, effect) in state.document.eq.iter().zip(&document.eq) {
                if old != effect {
                    fast.push((
                        format!("eq:{}", effect.id),
                        FastUpdate::Eq(PreparedEqUpdate::new(effect, self.0.rate)?),
                    ));
                }
            }
            if state.options.output_gain_db != options.output_gain_db {
                fast.push(("output".into(), FastUpdate::Output(options.output_gain_db)));
            }
        }
        state.document = document.clone();
        state.assets = assets.clone();
        state.options = options.clone();
        state.error = None;
        state.reclaim_controls();
        for (key, update) in fast {
            state.fast.insert(key, update);
        }
        self.0
            .duration
            .store(document.duration_frames(), Ordering::Release);
        if geometry || self.preparing() {
            state.request = true;
            if geometry {
                state.fast.clear();
                state.control_ready = None;
            }
            self.0.preparing.store(true, Ordering::Release);
            self.0.epoch.fetch_add(1, Ordering::AcqRel);
            self.0.wake.notify_one();
        }
        state.prepare_controls(self.0.epoch.load(Ordering::Acquire));
        self.0.wake.notify_one();
        Ok(())
    }
    pub fn update_lane(&self, id: &str, gain: f64, pan: f64, mute: bool, solo: bool) -> Result<()> {
        let _writer = self
            .0
            .writer
            .lock()
            .map_err(|_| anyhow::anyhow!("Arrangement control lock failed"))?;
        let (mut doc, assets, opts) = self.snapshot()?;
        let lane = doc
            .lanes
            .iter_mut()
            .find(|lane| lane.id == id)
            .context("Arrangement lane is not present")?;
        lane.gain_db = gain;
        lane.pan = pan;
        lane.mute = mute;
        lane.solo = solo;
        self.set_inner(&doc, &assets, &opts)
    }
    pub fn update_clip(&self, id: &str, gain: f64) -> Result<()> {
        let _writer = self
            .0
            .writer
            .lock()
            .map_err(|_| anyhow::anyhow!("Arrangement control lock failed"))?;
        let (mut doc, assets, opts) = self.snapshot()?;
        let clip = doc
            .lanes
            .iter_mut()
            .flat_map(|lane| &mut lane.clips)
            .find(|clip| clip.id == id)
            .context("Arrangement clip is not present")?;
        clip.gain_db = gain;
        self.set_inner(&doc, &assets, &opts)
    }
    pub fn update_eq(&self, effect: &ScopedEq) -> Result<()> {
        let _writer = self
            .0
            .writer
            .lock()
            .map_err(|_| anyhow::anyhow!("Arrangement control lock failed"))?;
        let (mut doc, assets, opts) = self.snapshot()?;
        let old = doc
            .eq
            .iter_mut()
            .find(|old| old.id == effect.id)
            .context("Add arrangement EQ through a document edit first")?;
        *old = effect.clone();
        self.set_inner(&doc, &assets, &opts)
    }
    fn snapshot(&self) -> Result<(Arrangement, AssetLibrary, RenderOptions)> {
        let state = self
            .0
            .state
            .lock()
            .map_err(|_| anyhow::anyhow!("Arrangement preparation lock failed"))?;
        Ok((
            state.document.clone(),
            state.assets.clone(),
            state.options.clone(),
        ))
    }
    pub fn request_seek(&self, frame: u64) -> u64 {
        let epoch = self.0.epoch.fetch_add(1, Ordering::AcqRel) + 1;
        self.0.seek.store(frame, Ordering::Release);
        self.0.preparing.store(true, Ordering::Release);
        self.0.wake.notify_one();
        epoch
    }
    pub fn retire(&self, item: Retired) -> Option<Retired> {
        match self.0.retired.try_send(item) {
            Ok(()) => {
                self.0.wake.notify_one();
                None
            }
            Err(TrySendError::Full(item) | TrySendError::Disconnected(item)) => Some(item),
        }
    }
}
fn same_asset_catalog(old: &AssetLibrary, new: &AssetLibrary) -> bool {
    old.len() == new.len()
        && old
            .iter()
            .all(|(id, source)| new.get(id).is_some_and(|other| Arc::ptr_eq(source, other)))
}
fn same_geometry(a: &Arrangement, b: &Arrangement) -> bool {
    a.sample_rate == b.sample_rate
        && a.channels == b.channels
        && a.lanes.len() == b.lanes.len()
        && a.eq.len() == b.eq.len()
        && a.lanes.iter().zip(&b.lanes).all(|(a, b)| {
            a.id == b.id
                && a.clips.len() == b.clips.len()
                && a.clips.iter().zip(&b.clips).all(|(a, b)| {
                    (
                        a.id.as_str(),
                        a.source_id.as_str(),
                        a.start_frame,
                        a.source_start_frame,
                        a.end_frame,
                        a.fade_in_frames,
                        a.fade_out_frames,
                    ) == (
                        b.id.as_str(),
                        b.source_id.as_str(),
                        b.start_frame,
                        b.source_start_frame,
                        b.end_frame,
                        b.fade_in_frames,
                        b.fade_out_frames,
                    )
                })
        })
        && a.eq
            .iter()
            .zip(&b.eq)
            .all(|(a, b)| a.id == b.id && a.scope == b.scope && a.range == b.range)
}
