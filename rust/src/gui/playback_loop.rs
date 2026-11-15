//! Frame bounds for native auditioning. The UI publishes seconds through Player;
//! ABSource reads a coherent snapshot only between complete interleaved frames.
//! Loop resets use the same delay-aligned DSP priming as seeks, without a gain fade.
use anyhow::{Result, ensure};
use std::sync::{
    Mutex,
    atomic::{AtomicU64, Ordering},
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct FrameRange {
    pub start: usize,
    pub end: usize,
}
impl FrameRange {
    pub fn from_seconds(start: f64, end: f64, rate: u32, frames: usize) -> Result<Self> {
        ensure!(
            rate > 0 && frames > 0,
            "Load audio before setting an audition loop"
        );
        ensure!(
            start.is_finite() && end.is_finite(),
            "Loop endpoints must be finite seconds"
        );
        ensure!(
            start >= 0. && start < end && end <= frames as f64 / rate as f64,
            "Loop must satisfy 0 <= start < end <= the current audio duration"
        );
        let range = Self {
            start: (start * rate as f64).round() as usize,
            end: (end * rate as f64).round() as usize,
        };
        ensure!(
            range.start < range.end && range.end <= frames,
            "Loop must contain at least one complete sample frame"
        );
        Ok(range)
    }
}

/// Writers serialize on the control thread. The audio reader makes one bounded
/// attempt, retaining its last complete snapshot if publication is in progress.
/// SeqCst gives the version and both atomic endpoints one total order: a reader
/// accepts either complete publication, never an old start with a new end.
#[derive(Default)]
pub(super) struct LoopControl {
    writer: Mutex<()>,
    version: AtomicU64,
    start: AtomicU64,
    end: AtomicU64,
}
impl LoopControl {
    pub fn set(&self, range: Option<FrameRange>) {
        let _writer = self.writer.lock().unwrap_or_else(|e| e.into_inner());
        self.version.fetch_add(1, Ordering::SeqCst);
        self.start
            .store(range.map_or(0, |r| r.start as u64), Ordering::SeqCst);
        self.end
            .store(range.map_or(0, |r| r.end as u64), Ordering::SeqCst);
        self.version.fetch_add(1, Ordering::SeqCst);
    }
    fn read_range(&self) -> Option<FrameRange> {
        let start = self.start.load(Ordering::SeqCst) as usize;
        let end = self.end.load(Ordering::SeqCst) as usize;
        (start < end).then_some(FrameRange { start, end })
    }
    pub fn try_range(&self) -> Option<Option<FrameRange>> {
        let version = self.version.load(Ordering::SeqCst);
        if !version.is_multiple_of(2) {
            return None;
        }
        let range = self.read_range();
        (version == self.version.load(Ordering::SeqCst)).then_some(range)
    }
    pub fn range(&self) -> Option<FrameRange> {
        let _writer = self.writer.lock().unwrap_or_else(|e| e.into_inner());
        self.read_range()
    }
}

#[cfg(test)]
#[path = "playback_loop_tests.rs"]
mod tests;
