//! Exact and bounded worker-side state preparation; allocation-free seek adoption.
use super::*;

impl ArrangementPlayer {
    pub fn arrangement(&self) -> &Arrangement {
        &self.document
    }
    pub fn assets(&self) -> &AssetLibrary {
        &self.assets
    }
    pub fn render_options(&self) -> &RenderOptions {
        &self.options
    }
    pub fn validation_cache(&self) -> crate::studio::arrangement::AssetValidationCache {
        crate::studio::arrangement::AssetValidationCache {
            checked: self
                .document
                .lanes
                .iter()
                .flat_map(|lane| &lane.clips)
                .map(|clip| (clip.source_id.clone(), self.assets[&clip.source_id].clone()))
                .collect(),
        }
    }
    /// Control-thread bounded warmup; unlike exact seek this intentionally
    /// approximates state older than 1.5 seconds. Geometry/PCM admission is cached.
    pub fn prepare_bounded(
        document: &Arrangement,
        assets: &AssetLibrary,
        options: &RenderOptions,
        cache: &mut crate::studio::arrangement::AssetValidationCache,
        frame: u64,
        warm_frames: u64,
        cancel: &dyn Fn() -> bool,
    ) -> Result<Self> {
        ensure!(!cancel(), "Cancelled");
        options.validate()?;
        cache.validate_used(document, assets, cancel)?;
        ensure!(
            frame <= document.duration_frames(),
            "Arrangement seek is outside the project"
        );
        let warm_frames = warm_frames.min((document.sample_rate as f64 * 1.5).ceil() as u64);
        let start = frame.saturating_sub(warm_frames);
        let plan = Plan::compile_at(document, assets, cancel, false, start, Some(warm_frames))?;
        let mut prepared = Self::new_from_plan(document, assets, options, plan);
        prepared.position = start;
        prepared.input_position = start;
        prepared.event = prepared
            .plan
            .events
            .partition_point(|event| event.frame <= start);
        prepared.active.extend(
            prepared
                .plan
                .clips
                .iter()
                .enumerate()
                .filter(|(_, clip)| clip.meta.start_frame <= start && start < clip.meta.end_frame)
                .map(|(index, _)| index),
        );
        while prepared.position < frame {
            if prepared.position.is_multiple_of(4096) {
                ensure!(!cancel(), "Cancelled");
            }
            prepared.next_frame()?;
        }
        prepared.prime_lookahead(cancel)?;
        ensure!(!cancel(), "Cancelled");
        Ok(prepared)
    }
    pub fn set_replacement_fade_frames(&mut self, frames: usize) -> Result<()> {
        ensure!(
            frames > 0 && frames <= self.sample_rate() as usize,
            "Invalid replacement crossfade duration"
        );
        self.fade_frames = frames;
        Ok(())
    }
    /// Exact state reconstruction is cancellable and must run off the audio callback.
    pub fn seek(&mut self, frame: u64, cancel: &dyn Fn() -> bool) -> Result<()> {
        let prepared = self.prepare_seek(frame, cancel)?;
        *self = prepared;
        Ok(())
    }
    /// Worker/control-thread operation. Used assets are immutable and were
    /// already scanned at admission, so preparation does not rescan source PCM.
    pub fn prepare_seek(&self, frame: u64, cancel: &dyn Fn() -> bool) -> Result<Self> {
        ensure!(
            frame <= self.duration_frames(),
            "Arrangement seek is outside the project"
        );
        ensure!(!cancel(), "Cancelled");
        let plan = Plan::compile(&self.document, &self.assets, cancel, false)?;
        let mut prepared = Self::new_from_plan(&self.document, &self.assets, &self.options, plan);
        while prepared.position() < frame {
            if prepared.position().is_multiple_of(4096) {
                ensure!(!cancel(), "Cancelled");
            }
            prepared.next_frame()?;
        }
        prepared.prime_lookahead(cancel)?;
        ensure!(!cancel(), "Cancelled");
        Ok(prepared)
    }
    /// Constant-work ownership handoff. Dispose of the returned player off-thread.
    pub fn apply_prepared_seek(&mut self, prepared: Self) -> Result<Self> {
        ensure!(
            self.sample_rate() == prepared.sample_rate() && self.channels() == prepared.channels(),
            "Prepared seek layout differs"
        );
        Ok(std::mem::replace(self, prepared))
    }
    pub fn apply_prepared_seek_boxed(&mut self, mut prepared: Box<Self>) -> Result<Box<Self>> {
        ensure!(
            self.sample_rate() == prepared.sample_rate() && self.channels() == prepared.channels(),
            "Prepared seek layout differs"
        );
        std::mem::swap(self, prepared.as_mut());
        Ok(prepared)
    }
    pub fn frame_at(&mut self, frame: u64, cancel: &dyn Fn() -> bool) -> Result<Option<[f32; 2]>> {
        if frame != self.position {
            self.seek(frame, cancel)?;
        }
        self.next_frame()
    }
}
