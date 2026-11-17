//! Per-source, per-channel peak caches. Navigation and playhead paint never rebuild PCM.
use super::editor_envelope::Envelope;
use crate::audio::Audio;
use anyhow::Result;
use eframe::egui::{Color32, Painter, Pos2, Rect, Stroke};
use std::{collections::BTreeMap, sync::Arc};

pub(super) struct PreparedWaveform {
    pub audio: Arc<Audio>,
    channels: Vec<Envelope>,
}
impl PreparedWaveform {
    pub fn new(audio: Arc<Audio>) -> Result<Self> {
        let channels = (0..audio.channels)
            .map(|channel| Envelope::for_channel(&audio, channel))
            .collect::<Result<_>>()?;
        Ok(Self { audio, channels })
    }
}

#[derive(Default)]
pub(super) struct Waveforms {
    sources: BTreeMap<String, PreparedWaveform>,
}
impl Waveforms {
    pub fn insert(&mut self, id: String, waveform: PreparedWaveform) {
        self.sources.insert(id, waveform);
    }
    pub fn clear(&mut self) {
        self.sources.clear();
    }
    pub fn paint(
        &self,
        id: &str,
        painter: &Painter,
        rect: Rect,
        source_start: u64,
        source_end: u64,
        color: Color32,
    ) {
        let Some(source) = self.sources.get(id) else {
            return;
        };
        if rect.width() < 1. || rect.height() < 1. || source_start >= source_end {
            return;
        }
        let pixels = rect.width().ceil().clamp(1., 2048.) as usize;
        for (channel, envelope) in source.channels.iter().enumerate() {
            let height = rect.height() / source.channels.len() as f32;
            let middle = rect.top() + height * (channel as f32 + 0.5);
            painter.line_segment(
                [
                    Pos2::new(rect.left(), middle),
                    Pos2::new(rect.right(), middle),
                ],
                Stroke::new(0.5, color.gamma_multiply(0.35)),
            );
            for pixel in 0..pixels {
                let first = source_start
                    + ((source_end - source_start) as u128 * pixel as u128 / pixels as u128) as u64;
                let end = source_start
                    + ((source_end - source_start) as u128 * (pixel + 1) as u128 / pixels as u128)
                        as u64;
                let peak = envelope
                    .peak(&source.audio, first as usize, end.max(first + 1) as usize)
                    .min(1.);
                let x = rect.left() + rect.width() * (pixel as f32 + 0.5) / pixels as f32;
                let amplitude = peak * (height * 0.5 - 2.).max(0.);
                painter.line_segment(
                    [
                        Pos2::new(x, middle - amplitude),
                        Pos2::new(x, middle + amplitude),
                    ],
                    Stroke::new(1., color),
                );
            }
        }
    }
}
