use super::{Lane, eq_processor::CONTROL_FADE_SECONDS};
use crate::{
    dsp::eq_chain::Frame,
    studio::stem_mix::{StemControl, StemKind, channel_levels},
};

pub(super) fn lane_levels(lane: &Lane, any_solo: bool) -> Frame {
    // channel_levels does not inspect kind; reuse the existing routing/pan law.
    channel_levels(
        &StemControl {
            kind: StemKind::Other,
            gain_db: lane.gain_db,
            balance: lane.pan,
            mute: lane.mute,
            solo: lane.solo,
        },
        0.,
        any_solo,
    )
}
pub(super) struct GainRamp {
    current: Frame,
    target: Frame,
    step: Frame,
    remaining: usize,
    length: usize,
}
impl GainRamp {
    pub fn new(value: Frame, rate: u32) -> Self {
        Self {
            current: value,
            target: value,
            step: [0.; 2],
            remaining: 0,
            length: (CONTROL_FADE_SECONDS * rate as f64).round().max(1.) as usize,
        }
    }
    pub fn set(&mut self, target: Frame) {
        if self.target == target {
            return;
        }
        self.target = target;
        self.remaining = self.length;
        for (channel, value) in target.iter().enumerate() {
            self.step[channel] = (*value - self.current[channel]) / self.length as f64;
        }
    }
    pub fn next(&mut self) -> Frame {
        if self.remaining > 0 {
            self.remaining -= 1;
            for channel in 0..2 {
                self.current[channel] = if self.remaining == 0 {
                    self.target[channel]
                } else {
                    self.current[channel] + self.step[channel]
                };
            }
        }
        self.current
    }
}
