use super::{
    input,
    output::{self, Asset, AudioFile, Response, Role},
    reference,
    request::{Operation, Request},
    settings,
};
use anyhow::{Context, Result};
use mastercraft::{
    analysis,
    audio::Audio,
    config::Settings,
    dsp::manual_eq,
    engine, export,
    studio::{doctor, region_edit, spectral_repair, spectrogram, voice_cleanup},
};
use serde_json::json;
use std::path::Path;

pub fn single(
    request: &Request,
    source: &Path,
    out_dir: &Path,
    settings: &Settings,
) -> Result<Response> {
    let audio = input::load(source, &request.limits, request.limits.max_pcm_bytes)?;
    let profile = settings::profile(settings)?;
    let mut result = Response::new(request.operation, &request.limits);
    result.audio = Some(AudioFile::new(source, &audio));
    super::progress(0.12, "Processing audio");
    match request.operation {
        Operation::Import => {
            publish(
                &audio,
                "original",
                Role::Original,
                out_dir,
                request,
                &mut result,
            )?;
            result.analysis = Some(analysis::analyze(&audio, &profile)?);
        }
        Operation::Analyze => {
            result.describe(&audio, source, &request.limits);
            result.analysis = Some(analysis::analyze(&audio, &profile)?);
        }
        Operation::Suggest => {
            let measured = analysis::analyze(&audio, &profile)?;
            reference::validate(&measured, &request.limits)?;
            let bands = mastercraft::dsp::eq_suggestion::suggest(&measured, &profile)?;
            result.suggestion = Some(output::Suggestion { bands });
            result.analysis = Some(measured);
        }
        Operation::Master => {
            if let Some(reference) = &request.reference_analysis {
                reference::validate(reference, &request.limits)?;
            }
            let prepared = settings::before_eq(settings);
            let mastered = engine::master(
                &audio,
                &prepared,
                &profile,
                request.reference_analysis.as_ref(),
                "source",
                &|p, _| super::progress(0.12 + p * 0.76, "Mastering audio"),
                &|| false,
            )?;
            publish(
                &mastered.audio,
                "master",
                Role::Master,
                out_dir,
                request,
                &mut result,
            )?;
            result.analysis = Some(mastered.report.output.clone());
            result.report = Some(mastered.report);
        }
        Operation::Render => {
            let rendered = manual_eq::render(&audio, settings, &|| false)?;
            publish(
                &rendered.audio,
                "preview",
                Role::Preview,
                out_dir,
                request,
                &mut result,
            )?;
            result.analysis = Some(analysis::analyze(&rendered.audio, &profile)?);
            super::progress(0.78, "Encoding audio");
            let exports =
                export::export(&rendered.audio, Path::new("master.wav"), settings, &|| {
                    false
                })?;
            for export in &exports {
                let path = Path::new(export["path"].as_str().context("Export missing path")?);
                // Measure dimensions from the final encode, including codec padding/resampling.
                let mut encoded_limits = request.limits.clone();
                encoded_limits.max_duration_seconds += 1.;
                let decoded = input::load(path, &encoded_limits, encoded_limits.max_pcm_bytes)?;
                result.assets.push(Asset {
                    role: Role::Master,
                    name: "master".into(),
                    format: export["format"]
                        .as_str()
                        .context("Export missing format")?
                        .into(),
                    channel_peaks: output::waveform(&decoded, 1).channel_peaks,
                    audio: AudioFile::new(path, &decoded),
                    kind: None,
                });
            }
            result.tool_metadata = Some(
                json!({"eq_hold_gain_db":rendered.hold_gain_db,"eq_trim_db":rendered.trim_db,"eq_converged":rendered.converged,"exports":exports}),
            );
        }
        Operation::Issues => result.issues = Some(doctor::scan(&audio, &|| false)?),
        Operation::Repair => {
            let repaired = doctor::repair(
                &audio,
                request.repair.as_ref().context("Missing repair")?,
                &|| false,
            )?;
            publish(
                &repaired.audio,
                "repaired",
                Role::Original,
                out_dir,
                request,
                &mut result,
            )?;
            result.analysis = Some(analysis::analyze(&repaired.audio, &profile)?);
            result.issues = Some(doctor::scan(&repaired.audio, &|| false)?);
            result.tool_metadata = Some(
                json!({"notes":repaired.notes,"changed_samples":repaired.changed_samples,"peak_trim_db":repaired.peak_trim_db}),
            );
        }
        Operation::VoiceCleanup => {
            let cleaned = voice_cleanup::render(
                &audio,
                request.voice.as_ref().context("Missing voice")?,
                &|| false,
            )?;
            publish(
                &cleaned.audio,
                "voice",
                Role::Original,
                out_dir,
                request,
                &mut result,
            )?;
            result.analysis = Some(analysis::analyze(&cleaned.audio, &profile)?);
            result.tool_metadata = Some(
                json!({"notes":cleaned.notes,"peak_trim_db":cleaned.peak_trim_db,"deess_max_cut_db":cleaned.deess_max_cut_db}),
            );
        }
        Operation::SpectralRepair => {
            let repaired = spectral_repair::render(
                &audio,
                request
                    .spectral
                    .as_ref()
                    .context("Missing spectral selection")?,
                &|| false,
            )?;
            publish(
                &repaired,
                "spectral",
                Role::Original,
                out_dir,
                request,
                &mut result,
            )?;
            result.analysis = Some(analysis::analyze(&repaired, &profile)?);
        }
        Operation::RegionEdit => {
            let edited = region_edit::render(
                &audio,
                request.region.as_ref().context("Missing region")?,
                &|| false,
            )?;
            publish(
                &edited,
                "region",
                Role::Original,
                out_dir,
                request,
                &mut result,
            )?;
            result.analysis = Some(analysis::analyze(&edited, &profile)?);
        }
        Operation::Spectrogram => {
            let defaults = super::request::SpectrogramSettings::default();
            let display = request.spectrogram.as_ref().unwrap_or(&defaults);
            result.spectrogram = Some(spectrogram::build(
                &audio,
                display.columns,
                display.rows,
                &|| false,
            )?);
        }
        _ => anyhow::bail!("Invalid single-input operation"),
    }
    Ok(result)
}

pub fn publish(
    audio: &Audio,
    name: &str,
    role: Role,
    out_dir: &Path,
    request: &Request,
    response: &mut Response,
) -> Result<()> {
    let asset = output::float(audio, out_dir, name, role, &request.limits)?;
    response.describe(audio, &asset.audio.path, &request.limits);
    response.assets.push(asset);
    Ok(())
}
