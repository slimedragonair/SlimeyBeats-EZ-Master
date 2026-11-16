//! Adopt server PCM into the same immutable raw-master bank as local mastering.
//! Measurements and configuration are local; only bounded numeric processing
//! metadata survives the server report. Recovery can replay that projection.
use super::{Session, master_settings};
use crate::{failure, import_limits, profile};
use anyhow::{Context, Result, ensure};
use mastercraft::{
    analysis, audio::Audio, branding::ENGINE_VERSION, dsp::limiter::LimiterInfo, profiles,
    report::Report,
};
use serde_json::Value;
use wasm_bindgen::prelude::*;

// Match the hybrid report's byte budget and bound untrusted display metadata.
const MAX_REPORT_BYTES: usize = 2 * 1024 * 1024;
const MAX_PROCESSING_SECONDS: f64 = 24. * 60. * 60.;
const MAX_DB: f64 = 120.;
const MAX_ATTEMPTS: u64 = 100;
const SERVER_NOTE: &str = "Server-origin master adopted for local analysis, live EQ and export. Processing metadata is supplied by the server.";

fn validate_layout(input: &Audio, sample_count: usize) -> Result<()> {
    import_limits::validate(sample_count, input.sample_rate, input.channels as u32)?;
    ensure!(
        sample_count == input.samples.len(),
        "Server master must have exactly the input sample count"
    );
    Ok(())
}

fn number(value: &Value, key: &str, default: f64, low: f64, high: f64) -> Result<f64> {
    let Some(value) = value.get(key) else {
        return Ok(default);
    };
    let number = value
        .as_f64()
        .filter(|number| number.is_finite())
        .with_context(|| format!("Server report {key} must be a finite number"))?;
    Ok(number.clamp(low, high))
}

fn attempts(value: &Value, key: &str) -> Result<usize> {
    let Some(value) = value.get(key) else {
        return Ok(0);
    };
    Ok(value
        .as_u64()
        .with_context(|| format!("Server report {key} must be a nonnegative integer"))?
        .min(MAX_ATTEMPTS) as usize)
}

fn limiter_metadata(value: Option<&Value>, output: &analysis::Analysis) -> Result<LimiterInfo> {
    let empty = serde_json::json!({});
    let value = value.unwrap_or(&empty);
    ensure!(value.is_object(), "Server report limiter must be an object");
    let reachable = value
        .get("reachable")
        .map(|value| {
            value
                .as_bool()
                .context("Server report reachable must be a boolean")
        })
        .transpose()?
        .unwrap_or(false);
    Ok(LimiterInfo {
        iterations: attempts(value, "iterations")?,
        reachable,
        pre_gain_db: number(value, "pre_gain_db", 0., -MAX_DB, MAX_DB)?,
        max_gr_db: number(value, "max_gr_db", 0., 0., MAX_DB)?,
        absolute_max_gr_db: number(value, "absolute_max_gr_db", 0., 0., MAX_DB)?,
        trim_db: number(value, "trim_db", 0., -MAX_DB, 0.)?,
        out_lufs: number(
            value,
            "out_lufs",
            output.metrics.integrated_lufs,
            -MAX_DB,
            MAX_DB,
        )?,
        out_tp_dbtp: number(
            value,
            "out_tp_dbtp",
            output.metrics.true_peak_dbtp,
            -MAX_DB,
            MAX_DB,
        )?,
    })
}

impl Session {
    fn adopt_master_checked(
        &mut self,
        samples: Vec<f32>,
        json: &str,
        serverreport: &str,
    ) -> Result<String> {
        validate_layout(&self.input, samples.len())?;
        let raw = Audio::new(samples, self.input.sample_rate, self.input.channels)?;
        let settings = master_settings(json)?;
        let profile = profile(&settings.profile)?;
        ensure!(
            serverreport.len() <= MAX_REPORT_BYTES,
            "Server report exceeds its byte limit"
        );
        let metadata: Value =
            serde_json::from_str(serverreport).context("Invalid server report JSON")?;
        ensure!(metadata.is_object(), "Server report must be an object");

        let input = analysis::analyze(&self.input, &profile)?;
        let output = analysis::analyze(&raw, &profile)?;
        let requested = settings
            .target_lufs
            .unwrap_or_else(|| profiles::number(&profile, &["lufs", &settings.mode], -14.));
        let target = if settings.match_loudness {
            self.reference
                .as_ref()
                .map(|reference| reference.metrics.integrated_lufs)
                .unwrap_or(requested)
        } else {
            requested
        }
        .min(input.metrics.integrated_lufs + settings.max_makeup_db);
        let report = Report {
            configuration: serde_json::to_value(&settings)?,
            version: ENGINE_VERSION.into(),
            file: "browser-track".into(),
            profile: profile["display_name"].as_str().unwrap_or("Custom").into(),
            target_lufs: number(&metadata, "target_lufs", target, -MAX_DB, 0.)?,
            ceiling_dbtp: number(&metadata, "ceiling_dbtp", settings.ceiling_dbtp, -12., 0.)?,
            limiter: limiter_metadata(metadata.get("limiter"), &output)?,
            input,
            output,
            log: vec![],
            processing_seconds: number(
                &metadata,
                "processing_seconds",
                0.,
                0.,
                MAX_PROCESSING_SECONDS,
            )?,
            punch_loss_db: number(&metadata, "punch_loss_db", 0., 0., MAX_DB)?,
            safety_attempts: attempts(&metadata, "safety_attempts")?,
            notes: vec![SERVER_NOTE.into()],
            exports: vec![],
        };
        // Prepare every fallible result before replacing the previous master.
        let result = serde_json::to_string(&serde_json::json!({
            "report": report,
            "raw_analysis": report.output,
        }))?;
        self.raw = Some(raw);
        self.report = Some(report);
        self.raw_reference_stale = false;
        Ok(result)
    }
}

#[wasm_bindgen]
impl Session {
    /// Accept interleaved PCM with the immutable input's exact layout and length.
    /// `serverreport` may be the hybrid report or its saved numeric projection.
    pub fn adopt_master(
        &mut self,
        samples: Vec<f32>,
        json: &str,
        serverreport: &str,
    ) -> Result<String, JsValue> {
        self.adopt_master_checked(samples, json, serverreport)
            .map_err(failure)
    }
}

#[cfg(test)]
#[path = "external_master_tests.rs"]
mod tests;
