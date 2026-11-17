use crate::audio::Audio;

const BUCKET_FRAMES: usize = 64;

/// Absolute peaks across every channel of an immutable audio source.
/// Rebuild this cache whenever the source PCM changes.
pub(super) struct Envelope {
    levels: Vec<Vec<f32>>,
    channel: Option<usize>,
}

impl Envelope {
    pub fn new(audio: &Audio) -> Self {
        Self::build(audio, None)
    }

    pub fn for_channel(audio: &Audio, channel: usize) -> anyhow::Result<Self> {
        anyhow::ensure!(channel < audio.channels, "Waveform channel is unavailable");
        Ok(Self::build(audio, Some(channel)))
    }

    fn build(audio: &Audio, channel: Option<usize>) -> Self {
        let mut level: Vec<f32> = audio
            .samples
            .chunks(BUCKET_FRAMES * audio.channels)
            .map(|samples| sample_peak(samples, audio.channels, channel))
            .collect();
        let mut levels = Vec::new();
        while !level.is_empty() {
            // Unpaired tails stay in lower levels instead of being copied upward.
            // Consequently all levels together hold fewer than twice the base peaks.
            let next = level
                .as_chunks::<2>()
                .0
                .iter()
                .map(|pair| pair[0].max(pair[1]))
                .collect();
            levels.push(level);
            level = next;
        }
        Self { levels, channel }
    }

    /// Returns the maximum absolute sample in the clamped frame range [start, end).
    /// `audio` must contain the same unchanged PCM used to construct this cache.
    pub fn peak(&self, audio: &Audio, start: usize, end: usize) -> f32 {
        let frames = audio.frames();
        let start = start.min(frames);
        let end = end.min(frames);
        if start >= end {
            return 0.0;
        }
        if end - start <= BUCKET_FRAMES {
            return peak_frames(audio, start, end, self.channel);
        }

        let first_bucket = start.div_ceil(BUCKET_FRAMES);
        let last_bucket = end / BUCKET_FRAMES;
        // Only the two partial boundary buckets read PCM: at most 126 frames.
        let mut peak = peak_frames(
            audio,
            start,
            (first_bucket * BUCKET_FRAMES).min(end),
            self.channel,
        );
        peak = peak.max(peak_frames(
            audio,
            (last_bucket * BUCKET_FRAMES).max(start),
            end,
            self.channel,
        ));

        let mut bucket = first_bucket;
        while bucket < last_bucket {
            let remaining = last_bucket - bucket;
            let length_level = (usize::BITS - 1 - remaining.leading_zeros()) as usize;
            let alignment_level = bucket.trailing_zeros() as usize;
            let level = length_level.min(alignment_level);
            peak = peak.max(self.levels[level][bucket >> level]);
            bucket += 1_usize << level;
        }
        peak
    }
}

fn peak_frames(audio: &Audio, start: usize, end: usize, channel: Option<usize>) -> f32 {
    sample_peak(
        &audio.samples[start * audio.channels..end * audio.channels],
        audio.channels,
        channel,
    )
}

fn sample_peak(samples: &[f32], channels: usize, channel: Option<usize>) -> f32 {
    if let Some(channel) = channel {
        samples
            .chunks_exact(channels)
            .fold(0., |peak: f32, frame| peak.max(frame[channel].abs()))
    } else {
        samples
            .iter()
            .fold(0., |peak: f32, sample| peak.max(sample.abs()))
    }
}

#[cfg(test)]
mod tests {
    use super::{BUCKET_FRAMES, Envelope};
    use crate::audio::Audio;

    fn direct_peak(audio: &Audio, start: usize, end: usize) -> f32 {
        let mut peak = 0.0_f32;
        for frame in start.min(audio.frames())..end.min(audio.frames()) {
            for channel in 0..audio.channels {
                peak = peak.max(audio.samples[frame * audio.channels + channel].abs());
            }
        }
        peak
    }

    fn deterministic_audio(frames: usize, channels: usize) -> Audio {
        let mut state = 0x91e1_0da5_u32;
        let samples = (0..frames * channels)
            .map(|_| {
                state = state.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
                ((state >> 16) as i32 - 32_768) as f32 / 16_384.0
            })
            .collect();
        Audio::new(samples, 48_000, channels).unwrap()
    }

    #[test]
    fn stereo_right_only_impulses_respect_first_last_and_bucket_boundaries() {
        let frames = 321;
        for impulse in [0, 1, 62, 63, 64, 65, 127, 128, 255, 256, frames - 1] {
            let mut samples = vec![0.0; frames * 2];
            samples[impulse * 2 + 1] = -1.75;
            let audio = Audio::new(samples, 48_000, 2).unwrap();
            let envelope = Envelope::new(&audio);
            assert_eq!(envelope.peak(&audio, 0, frames), 1.75);
            assert_eq!(envelope.peak(&audio, impulse, impulse + 1), 1.75);
            assert_eq!(envelope.peak(&audio, 0, impulse), 0.0);
            assert_eq!(envelope.peak(&audio, impulse + 1, frames), 0.0);
        }
    }

    #[test]
    fn wide_ranges_exclude_louder_samples_in_partial_boundary_buckets() {
        let mut samples = vec![0.0; 400 * 2];
        samples[64 * 2 + 1] = -9.0;
        samples[65 * 2] = -0.75;
        samples[255 * 2 + 1] = 1.25;
        samples[270 * 2 + 1] = -8.0;
        let audio = Audio::new(samples, 48_000, 2).unwrap();
        let envelope = Envelope::new(&audio);
        assert_eq!(envelope.peak(&audio, 65, 270), 1.25);
        assert_eq!(envelope.peak(&audio, 65, 255), 0.75);
        assert_eq!(envelope.peak(&audio, 66, 255), 0.0);
        assert_eq!(envelope.peak(&audio, 255, 270), 1.25);
    }

    #[test]
    fn deterministic_windows_match_direct_pcm_for_mono_and_stereo() {
        for channels in [1, 2] {
            for frames in [
                1, 2, 63, 64, 65, 127, 128, 129, 191, 192, 193, 255, 256, 257, 511, 512, 513,
                1_023, 1_024, 1_025, 4_099, 16_385,
            ] {
                let audio = deterministic_audio(frames, channels);
                let envelope = Envelope::new(&audio);
                for start in [
                    0,
                    1,
                    31,
                    62,
                    63,
                    64,
                    65,
                    126,
                    127,
                    128,
                    129,
                    frames / 2,
                    frames - 1,
                    frames,
                    frames + 1,
                ] {
                    for width in [
                        0,
                        1,
                        2,
                        31,
                        63,
                        64,
                        65,
                        127,
                        128,
                        129,
                        255,
                        256,
                        257,
                        1_024,
                        frames,
                        usize::MAX,
                    ] {
                        let end = start.saturating_add(width);
                        assert_eq!(
                            envelope.peak(&audio, start, end),
                            direct_peak(&audio, start, end),
                            "channels={channels}, frames={frames}, range={start}..{end}"
                        );
                    }
                }

                let mut state = 0x46b7_31c9_usize;
                for _ in 0..128 {
                    state = state.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
                    let start = state % (frames + 129);
                    state = state.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
                    let end = state % (frames + 129);
                    assert_eq!(
                        envelope.peak(&audio, start, end),
                        direct_peak(&audio, start, end),
                        "channels={channels}, frames={frames}, range={start}..{end}"
                    );
                }
            }
        }
    }

    #[test]
    fn silence_empty_reversed_and_out_of_bounds_ranges_return_zero() {
        for channels in [1, 2] {
            let audio = Audio::new(vec![0.0; 193 * channels], 48_000, channels).unwrap();
            let envelope = Envelope::new(&audio);
            for (start, end) in [
                (0, 193),
                (1, 192),
                (0, 0),
                (64, 64),
                (193, 193),
                (129, 65),
                (193, usize::MAX),
                (usize::MAX, 0),
                (usize::MAX, usize::MAX),
                (0, usize::MAX),
            ] {
                assert_eq!(envelope.peak(&audio, start, end), 0.0);
            }
        }

        // Audio::new rejects empty PCM, but an empty cache remains well defined.
        let audio = Audio {
            samples: Vec::new(),
            sample_rate: 48_000,
            channels: 2,
        };
        let envelope = Envelope::new(&audio);
        assert!(envelope.levels.is_empty());
        assert_eq!(envelope.peak(&audio, 0, usize::MAX), 0.0);
    }

    #[test]
    fn separate_channel_caches_match_pcm_without_cross_channel_leakage() {
        let mut audio = deterministic_audio(4099, 2);
        for frame in audio.samples.as_chunks_mut::<2>().0 {
            frame[1] *= 0.01;
        }
        for channel in 0..2 {
            let envelope = Envelope::for_channel(&audio, channel).unwrap();
            for start in [0, 1, 63, 64, 127, 255, 1023, 4000] {
                for width in [1, 3, 64, 128, 1024, 4099] {
                    let end = (start + width).min(audio.frames());
                    let expected = audio.samples[start * 2..end * 2]
                        .as_chunks::<2>()
                        .0
                        .iter()
                        .map(|frame| frame[channel].abs())
                        .fold(0_f32, f32::max);
                    assert_eq!(envelope.peak(&audio, start, end), expected);
                }
            }
        }
        assert!(Envelope::for_channel(&audio, 2).is_err());
    }

    #[test]
    fn levels_aggregate_complete_pairs_with_bounded_bucket_storage() {
        for frames in [1, 63, 64, 65, 127, 128, 129, 319, 320, 321, 4_099, 65_537] {
            let audio = deterministic_audio(frames, 2);
            let envelope = Envelope::new(&audio);
            let base_count = frames.div_ceil(BUCKET_FRAMES);
            assert_eq!(envelope.levels[0].len(), base_count);
            let stored_peaks: usize = envelope.levels.iter().map(Vec::len).sum();
            assert!(stored_peaks < 2 * base_count);
            for (bucket, &peak) in envelope.levels[0].iter().enumerate() {
                assert_eq!(
                    peak,
                    direct_peak(&audio, bucket * BUCKET_FRAMES, (bucket + 1) * BUCKET_FRAMES)
                );
            }
            for pair in envelope.levels.windows(2) {
                assert_eq!(pair[1].len(), pair[0].len() / 2);
                for (bucket, &peak) in pair[1].iter().enumerate() {
                    assert_eq!(peak, pair[0][bucket * 2].max(pair[0][bucket * 2 + 1]));
                }
            }
            assert_eq!(envelope.levels.last().unwrap().len(), 1);
        }
    }
}
