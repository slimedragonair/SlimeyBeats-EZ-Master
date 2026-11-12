//! Opt-in real-music EQ verification. Inputs and reports remain outside the source tree.
use anyhow::{Context, Result, ensure};
use mastercraft::{
    analysis,
    audio::{self, Audio},
    config::Settings,
    dsp::{biquad, eq_loudness::EqLoudnessModel, eq_suggestion, manual_eq},
    export, meters, profiles,
    report::Report,
};
use serde::Deserialize;
use serde_json::json;
use std::path::{Path, PathBuf};

#[derive(Deserialize)]
struct Fixture {
    id: String,
    report: PathBuf,
}

fn main() -> Result<()> {
    let mut args = std::env::args_os().skip(1);
    let manifest = PathBuf::from(args.next().context("Expected fixture manifest JSON")?);
    let destination = PathBuf::from(args.next().context("Expected new output JSON")?);
    ensure!(args.next().is_none(), "Expected exactly two paths");
    let fixtures: Vec<Fixture> = serde_json::from_slice(&std::fs::read(manifest)?)?;
    ensure!(!fixtures.is_empty(), "Empty corpus");
    let profile = profiles::resolve("neutral", Path::new("."))?;
    let mut results = Vec::new();
    for fixture in fixtures {
        let report: Report = serde_json::from_slice(&std::fs::read(&fixture.report)?)?;
        let bands = eq_suggestion::suggest(&report.output, &profile)?;
        ensure!(
            serde_json::to_vec(&bands)?
                == serde_json::to_vec(&eq_suggestion::suggest(&report.output, &profile)?)?,
            "{}: nondeterministic suggestion",
            fixture.id
        );
        ensure!(bands.len() == 7, "{}: wrong band count", fixture.id);
        for band in &bands {
            let upper = if band.freq <= 120. || band.freq >= 8000. {
                1.
            } else {
                2.5
            };
            ensure!(
                band.gain >= -2.5 && band.gain <= upper,
                "{}: unbounded suggestion",
                fixture.id
            );
        }
        let mastered = report.exports[0]["path"]
            .as_str()
            .context("No saved master")?;
        let audio = audio::load(Path::new(mastered))?;
        let frames = audio.frames().min(audio.sample_rate as usize * 12);
        let first = (audio.frames() - frames) / 2;
        let clip = Audio::new(
            audio.samples[first * audio.channels..(first + frames) * audio.channels].to_vec(),
            audio.sample_rate,
            audio.channels,
        )?;
        let before = meters::measure(&clip)?;
        let spectrum = analysis::spectrum(&clip);
        let settings = Settings {
            manual_eq: bands.clone(),
            ..Default::default()
        };
        let model = EqLoudnessModel::new(&spectrum, clip.sample_rate, before.integrated_lufs);
        let estimate = model.delta_db(&settings);
        let mut unheld = clip.clone();
        for band in settings.manual_eq.iter().filter(|band| band.on) {
            biquad::from_band(band, clip.sample_rate).apply(&mut unheld);
        }
        let measured_delta = meters::integrated(&unheld)? - before.integrated_lufs;
        let rendered = manual_eq::render(&clip, &settings, &|| false)?;
        let after = meters::measure(&rendered.audio)?;
        ensure!(
            rendered.audio.frames() == clip.frames(),
            "{}: duration changed",
            fixture.id
        );
        ensure!(
            rendered
                .audio
                .samples
                .iter()
                .all(|sample| sample.is_finite()),
            "{}: nonfinite samples",
            fixture.id
        );
        ensure!(
            after.true_peak_dbtp <= settings.ceiling_dbtp + 0.01,
            "{}: peak ceiling exceeded",
            fixture.id
        );
        results.push(json!({
            "id": fixture.id, "bands": bands, "clip_seconds": clip.duration(),
            "deterministic": true, "duration_preserved": true, "finite": true,
            "fast_delta_db": estimate, "measured_unheld_delta_db": measured_delta,
            "estimate_error_db": estimate - measured_delta,
            "hold_converged": rendered.converged, "hold_gain_db": rendered.hold_gain_db,
            "hold_lufs_error": after.integrated_lufs - before.integrated_lufs,
            "peak_trim_db": rendered.trim_db, "output_true_peak_dbtp": after.true_peak_dbtp,
        }));
        println!("Checked {}", fixture.id);
    }
    export::write_json(&destination, &results)?;
    Ok(())
}
