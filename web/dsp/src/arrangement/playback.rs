use super::*;
use anyhow::Context;

impl BrowserArrangement {
    pub(super) fn prepare_checked(
        &mut self,
        expected: u64,
        side: &str,
        frame: u64,
    ) -> Result<String> {
        self.session.check_revision(expected)?;
        ensure!(
            matches!(side, "before" | "after"),
            "Choose before or after arrangement playback"
        );
        let document = if side == "before" {
            self.session.before()
        } else {
            self.session.document()
        };
        let warm = (document.sample_rate as u64 * 3) / 2;
        let prepared = Box::new(ArrangementPlayer::prepare_bounded(
            document,
            self.session.assets(),
            self.session.render_options(),
            &mut self.cache,
            frame,
            warm,
            &|| false,
        )?);
        // Explicit seek/side/structural preparation must expose the requested
        // geometry immediately, even while paused (no next_frame to adopt a
        // queued replacement). Fast live controls never call this path.
        self.player = Some(prepared);
        self.player_side = side.into();
        self.player_revision = Some(expected);
        self.playback_state()
    }
    fn playback_state(&self) -> Result<String> {
        let player = self
            .player
            .as_ref()
            .context("Prepare arrangement playback first")?;
        Ok(json!({"identity":format!("{}:{}:{}", self.session.epoch(), self.player_revision.unwrap_or(self.session.revision()), self.player_side),
            "prepared_revision":self.player_revision,"sample_rate":player.sample_rate(),"channels":player.channels(),
            "frames":player.duration_frames(),"position_frame":player.position(),"side":self.player_side,
            "bounded_warmup_seconds":1.5}).to_string())
    }
    pub(super) fn block_checked(&mut self, frames: usize) -> Result<usize> {
        ensure!(
            (1..=BLOCK_FRAMES).contains(&frames),
            "Arrangement block must contain 1 to 2048 frames"
        );
        let player = self
            .player
            .as_mut()
            .context("Prepare arrangement playback first")?;
        self.block_len = frames * player.channels();
        let written = player.process_block(&mut self.block[..self.block_len])?;
        // This binding runs in a normal worker, so retiring owned engines cannot block the audio callback.
        drop(player.take_retired()?);
        Ok(written)
    }
}

#[wasm_bindgen]
impl BrowserArrangement {
    pub fn prepare(&mut self, expected: u64, side: &str, frame: u64) -> Result<String, JsValue> {
        self.prepare_checked(expected, side, frame).map_err(failure)
    }
    pub fn playback_info(&self) -> Result<String, JsValue> {
        self.playback_state().map_err(failure)
    }
    pub fn render_block(&mut self, frames: u32) -> Result<u32, JsValue> {
        self.block_checked(frames as usize)
            .map(|frames| frames as u32)
            .map_err(failure)
    }
    pub fn block_ptr(&self) -> *const f32 {
        self.block.as_ptr()
    }
    pub fn block_len(&self) -> usize {
        self.block_len
    }
}
