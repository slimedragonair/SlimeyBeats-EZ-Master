//! Timeline navigation is independent of the audio revision and mix controls.
use crate::studio::stem_mix::{StemKind, StemSet};

const MIN_VIEW_SECONDS: f64 = 0.01;
pub(super) const MIN_LOOP_SECONDS: f64 = 0.05;

pub(super) struct EditorState {
    pub timeline: bool,
    pub selected: Option<StemKind>,
    pub selection: Option<(f64, f64)>,
    pub draft_start: f64,
    pub draft_end: f64,
    pub view_start: f64,
    pub view_end: Option<f64>,
    pub drag_anchor: Option<f64>,
    pub gain_db: f64,
    pub looping: bool,
    pub inspector_edits: bool,
    pub reveal_edits: bool,
    pub waveform_scale: f32,
}

impl Default for EditorState {
    fn default() -> Self {
        Self {
            timeline: true,
            selected: None,
            selection: None,
            draft_start: 0.,
            draft_end: 0.,
            view_start: 0.,
            view_end: None,
            drag_anchor: None,
            gain_db: -3.,
            looping: false,
            inspector_edits: false,
            reveal_edits: false,
            waveform_scale: 4.,
        }
    }
}

impl EditorState {
    pub fn can_loop(&self) -> bool {
        self.selection
            .is_some_and(|(start, end)| end - start >= MIN_LOOP_SECONDS)
    }
    pub fn sync(&mut self, bank: &StemSet) {
        if self.selected.is_none_or(|kind| bank.track(kind).is_err()) {
            self.selected = bank.tracks.first().map(|stem| stem.kind);
            self.selection = None;
            self.fit_song();
        }
    }

    pub fn range(&self, duration: f64) -> (f64, f64) {
        let end = self.view_end.unwrap_or(duration).clamp(0., duration);
        let start = self.view_start.clamp(0., end);
        if end <= start {
            (0., duration)
        } else {
            (start, end)
        }
    }

    pub fn fit_song(&mut self) {
        self.view_start = 0.;
        self.view_end = None;
    }

    pub fn fit_selection(&mut self, duration: f64) {
        if let Some((start, end)) = self.selection {
            let span = (end - start).max(duration.min(MIN_VIEW_SECONDS));
            self.view_start = ((start + end - span) * 0.5).clamp(0., (duration - span).max(0.));
            self.view_end = Some(self.view_start + span);
        }
    }

    pub fn zoom(&mut self, duration: f64, focus: f64, factor: f64) {
        let (start, end) = self.range(duration);
        let span = ((end - start) * factor).clamp(duration.min(MIN_VIEW_SECONDS), duration);
        let fraction = ((focus - start) / (end - start)).clamp(0., 1.);
        self.view_start = (focus - span * fraction).clamp(0., (duration - span).max(0.));
        self.view_end = Some(self.view_start + span);
    }

    pub fn pan(&mut self, duration: f64, direction: f64) {
        let (start, end) = self.range(duration);
        let span = end - start;
        self.view_start = (start + span * 0.75 * direction).clamp(0., (duration - span).max(0.));
        self.view_end = Some(self.view_start + span);
    }

    pub fn select_time(&mut self, a: f64, b: f64, rate: u32, frames: usize) {
        let frame =
            |seconds: f64| (seconds * rate as f64).round().clamp(0., frames as f64) as usize;
        let (a, b) = (frame(a), frame(b));
        self.selection =
            (a != b).then_some((a.min(b) as f64 / rate as f64, a.max(b) as f64 / rate as f64));
        if let Some((start, end)) = self.selection {
            self.draft_start = start;
            self.draft_end = end;
        } else {
            self.draft_start = a as f64 / rate as f64;
            self.draft_end = self.draft_start;
            self.looping = false;
        }
    }
}

pub(super) fn time_label(seconds: f64) -> String {
    time_label_precise(seconds, 2)
}
pub(super) fn time_label_precise(seconds: f64, decimals: usize) -> String {
    let scale = 10_f64.powi(decimals as i32);
    let rounded = (seconds.max(0.) * scale).round() / scale;
    let minutes = rounded as u64 / 60;
    format!(
        "{minutes}:{:0width$.precision$}",
        rounded - minutes as f64 * 60.,
        width = decimals + 3,
        precision = decimals
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn reversed_drags_snap_to_whole_frames_and_zoom_pan_stay_inside_song() {
        let mut editor = EditorState::default();
        editor.select_time(2.00004, 1.00001, 48000, 480000);
        assert_eq!(editor.selection, Some((48000. / 48000., 96002. / 48000.)));
        editor.fit_selection(10.);
        assert_eq!(editor.range(10.), editor.selection.unwrap());
        for _ in 0..30 {
            editor.zoom(10., 2., 0.5);
        }
        let (start, end) = editor.range(10.);
        assert!(end > start && end - start >= 0.009999);
        editor.pan(10., 1e9);
        assert!(editor.range(10.).1 <= 10.);
        editor.pan(10., -1e9);
        assert_eq!(editor.range(10.).0, 0.);
        editor.fit_song();
        assert_eq!(editor.range(10.), (0., 10.));
    }
}
