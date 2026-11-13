use crate::audio::Audio;
use anyhow::Result;

/// Identical deterministic WAV quantization for native files and browser downloads.
pub fn write_wav<W: std::io::Write + std::io::Seek>(
    audio: &Audio,
    destination: W,
    format: &str,
    dither: bool,
) -> Result<()> {
    // Hound emits samples individually. Buffer here so every native caller,
    // including temporary previews, avoids millions of tiny filesystem writes.
    let destination = std::io::BufWriter::with_capacity(64 * 1024, destination);
    let float = format == "wav32f";
    let bits = if float {
        32
    } else if format.ends_with("16") {
        16
    } else {
        24
    };
    let spec = hound::WavSpec {
        channels: audio.channels as u16,
        sample_rate: audio.sample_rate,
        bits_per_sample: bits,
        sample_format: if float {
            hound::SampleFormat::Float
        } else {
            hound::SampleFormat::Int
        },
    };
    let mut writer = hound::WavWriter::new(destination, spec)?;
    let scale = 2_f64.powi(bits as i32 - 1);
    let mut random = 0x6d61737465726372_u64;
    let mut uniform = || {
        random ^= random << 13;
        random ^= random >> 7;
        random ^= random << 17;
        (random >> 11) as f64 / (1_u64 << 53) as f64
    };
    for &sample in &audio.samples {
        if float {
            writer.write_sample(sample)?;
        } else {
            let noise = if dither { uniform() - uniform() } else { 0. };
            let quantized = (sample as f64 * scale + noise)
                .round()
                .clamp(-scale, scale - 1.) as i32;
            writer.write_sample(quantized)?;
        }
    }
    writer.finalize()?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;

    #[test]
    fn file_like_destinations_use_bounded_writes_and_keep_float_samples_exact() {
        use std::{
            cell::Cell,
            io::{Seek, SeekFrom, Write},
            rc::Rc,
        };
        struct Counted {
            data: Cursor<Vec<u8>>,
            calls: Rc<Cell<usize>>,
        }
        impl Write for Counted {
            fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
                self.calls.set(self.calls.get() + 1);
                self.data.write(bytes)
            }
            fn flush(&mut self) -> std::io::Result<()> {
                self.data.flush()
            }
        }
        impl Seek for Counted {
            fn seek(&mut self, p: SeekFrom) -> std::io::Result<u64> {
                self.data.seek(p)
            }
        }
        let calls = Rc::new(Cell::new(0));
        let mut file = Counted {
            data: Cursor::new(Vec::new()),
            calls: calls.clone(),
        };
        let audio = Audio::new(
            (0..100000)
                .map(|i| (i % 1000) as f32 / 1000. - 0.5)
                .collect(),
            48000,
            2,
        )
        .unwrap();
        write_wav(&audio, &mut file, "wav32f", false).unwrap();
        assert!(
            calls.get() < 100,
            "WAV encoding made {} destination writes",
            calls.get()
        );
        file.data.set_position(0);
        let decoded = hound::WavReader::new(file.data)
            .unwrap()
            .samples::<f32>()
            .collect::<Result<Vec<_>, _>>()
            .unwrap();
        assert_eq!(decoded, audio.samples);
    }

    #[test]
    fn memory_wav_has_the_requested_layout_and_deterministic_dither() {
        let audio = Audio::new(vec![-0.99, 0.99, 0., 0.00001], 48000, 2).unwrap();
        for (format, bits) in [("wav16", 16), ("wav24", 24), ("wav32f", 32)] {
            let mut first = Cursor::new(Vec::new());
            let mut second = Cursor::new(Vec::new());
            write_wav(&audio, &mut first, format, true).unwrap();
            write_wav(&audio, &mut second, format, true).unwrap();
            assert_eq!(first.get_ref(), second.get_ref());
            first.set_position(0);
            let reader = hound::WavReader::new(first).unwrap();
            assert_eq!(reader.duration(), 2);
            assert_eq!(reader.spec().channels, 2);
            assert_eq!(reader.spec().sample_rate, 48000);
            assert_eq!(reader.spec().bits_per_sample, bits);
        }
    }
}
