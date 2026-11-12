use crate::audio::Audio;
use std::sync::atomic::{AtomicU32, AtomicUsize, Ordering};

const CAPACITY: usize = 8192;
pub struct PlaybackTap {
    samples: Vec<[AtomicU32; 2]>,
    written: AtomicUsize,
}
impl Default for PlaybackTap {
    fn default() -> Self {
        Self {
            samples: (0..CAPACITY)
                .map(|_| [AtomicU32::new(0), AtomicU32::new(0)])
                .collect(),
            written: AtomicUsize::new(0),
        }
    }
}
impl PlaybackTap {
    pub fn clear(&self) {
        self.written.store(0, Ordering::Release);
    }
    pub fn push(&self, frame: [f64; 2]) {
        let i = self.written.load(Ordering::Relaxed);
        for (ch, value) in frame.iter().enumerate() {
            self.samples[i % CAPACITY][ch].store((*value as f32).to_bits(), Ordering::Relaxed);
        }
        self.written.store(i + 1, Ordering::Release);
    }
    pub fn snapshot(&self, sample_rate: u32, channels: usize) -> Option<Audio> {
        let end = self.written.load(Ordering::Acquire);
        let count = end.min(4096);
        if count == 0 {
            return None;
        }
        let mut samples = Vec::with_capacity(count * channels);
        for frame in end - count..end {
            for ch in 0..channels {
                samples.push(f32::from_bits(
                    self.samples[frame % CAPACITY][ch].load(Ordering::Relaxed),
                ));
            }
        }
        Some(Audio {
            samples,
            sample_rate,
            channels,
        })
    }
}
