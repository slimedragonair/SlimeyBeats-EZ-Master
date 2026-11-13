use anyhow::{Result, ensure};

pub const MAX_SECONDS: f64 = 15. * 60.;
pub const MAX_DECODED_BYTES: usize = 384 * 1024 * 1024;
pub const MAX_FILE_BYTES: usize = 384 * 1024 * 1024;

pub fn validate(samples: usize, rate: u32, channels: u32) -> Result<()> {
    ensure!(
        rate > 0 && (1..=2).contains(&channels),
        "Use mono or stereo audio"
    );
    ensure!(
        samples <= MAX_DECODED_BYTES / size_of::<f32>(),
        "Decoded audio exceeds the 384 MiB browser limit"
    );
    ensure!(
        samples as f64 / channels as f64 / rate as f64 <= MAX_SECONDS,
        "Browser preview supports up to 15 minutes per track"
    );
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fifteen_minutes_of_48khz_mono_and_stereo_fit() {
        for channels in [1, 2] {
            assert!(validate(900 * 48000 * channels as usize, 48000, channels).is_ok());
            assert!(
                validate(
                    900 * 48000 * channels as usize + channels as usize,
                    48000,
                    channels
                )
                .is_err()
            );
        }
    }

    #[test]
    fn decoded_memory_is_independent_of_duration() {
        assert!(validate(MAX_DECODED_BYTES / 4 + 2, 192000, 2).is_err());
        assert!(validate(48000, 0, 1).is_err());
        assert!(validate(48000, 48000, 3).is_err());
    }
}
