use crate::{analysis::Analysis, dsp::limiter::LimiterInfo};
use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct StageLog {
    pub module: String,
    pub seconds: f64,
    pub detail: Value,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Report {
    pub configuration: Value,
    pub version: String,
    pub file: String,
    pub profile: String,
    pub input: Analysis,
    pub output: Analysis,
    pub target_lufs: f64,
    pub ceiling_dbtp: f64,
    pub limiter: LimiterInfo,
    pub log: Vec<StageLog>,
    pub processing_seconds: f64,
    pub punch_loss_db: f64,
    pub safety_attempts: usize,
    pub notes: Vec<String>,
    pub exports: Vec<Value>,
}
