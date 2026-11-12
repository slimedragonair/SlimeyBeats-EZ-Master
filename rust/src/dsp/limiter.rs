use crate::{
    audio::Audio,
    dsp::true_peak,
    meters::{self, db, lin},
};
use anyhow::{Result, ensure};
use serde::{Deserialize, Serialize};
use std::collections::VecDeque;

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct LimiterInfo {
    pub iterations: usize,
    pub reachable: bool,
    pub pre_gain_db: f64,
    pub max_gr_db: f64,
    pub absolute_max_gr_db: f64,
    pub trim_db: f64,
    pub out_lufs: f64,
    pub out_tp_dbtp: f64,
}
#[derive(Clone, Debug)]
pub struct LimiterConfig {
    pub target: f64,
    pub ceiling: f64,
    pub max_gr: f64,
    pub max_makeup: f64,
    pub lookahead_ms: f64,
    pub release_ms: f64,
    pub slow_ms: f64,
    pub enabled: bool,
}

pub fn gain_curve(envelope: &[f32], sr: u32, pre_gain: f64, cfg: &LimiterConfig) -> Vec<f32> {
    let n = envelope.len();
    if n == 0 {
        return Vec::new();
    }
    let length = ((cfg.lookahead_ms * 0.001 * sr as f64).round() as usize).max(3) | 1;
    let ceiling = lin(cfg.ceiling - 0.2);
    let required: Vec<f32> = envelope
        .iter()
        .map(|&e| (ceiling / (e as f64 * pre_gain).max(1e-20)).min(1.) as f32)
        .collect();
    let mut h = vec![1_f32; n];
    let mut queue: VecDeque<usize> = VecDeque::new();
    for i in 0..n + length - 1 {
        let value = required[i.min(n - 1)];
        while queue
            .back()
            .is_some_and(|&k| required[k.min(n - 1)] >= value)
        {
            queue.pop_back();
        }
        queue.push_back(i);
        let start = (i + 1).saturating_sub(length);
        while queue.front().is_some_and(|&k| k < start) {
            queue.pop_front();
        }
        if i + 1 >= length {
            h[start] = required[queue[0].min(n - 1)];
        }
    }
    let block = 16.min(length);
    let mut release = Vec::with_capacity(n.div_ceil(block));
    let a = (-(block as f64) / (cfg.release_ms.max(1.) * 0.001 * sr as f64)).exp();
    let slow_a = (-(block as f64) / (cfg.slow_ms.max(1.) * 0.001 * sr as f64)).exp();
    let slow_att = (-(block as f64) / (0.080 * sr as f64)).exp();
    let (mut fast, mut slow) = (h[0] as f64, 1_f64);
    for chunk in h.chunks(block) {
        let target = chunk.iter().copied().fold(1_f32, f32::min) as f64;
        fast = if target < fast {
            target
        } else {
            (a * fast + (1. - a) * target).min(target)
        };
        let coeff = if target < slow { slow_att } else { slow_a };
        slow = coeff * slow + (1. - coeff) * target;
        release.push(if cfg.slow_ms > 0. {
            fast.min(slow)
        } else {
            fast
        });
    }
    let mut sum = release[0] * length as f64;
    let mut gains = vec![0_f32; n];
    for (t, g) in gains.iter_mut().enumerate() {
        let old = if t < length {
            release[0]
        } else {
            release[(t - length) / block]
        };
        sum += release[t / block] - old;
        *g = (sum / length as f64).clamp(0., required[t] as f64) as f32;
    }
    gains
}
fn reduction(gains: &[f32]) -> (f64, f64) {
    let mut values = gains.to_vec();
    let index = ((values.len() as f64 * 0.0002) as usize).min(values.len() - 1);
    let percentile = *values.select_nth_unstable_by(index, f32::total_cmp).1;
    let absolute = gains.iter().copied().fold(1_f32, f32::min);
    (-db(percentile as f64), -db(absolute as f64))
}
fn apply(input: &Audio, pre_gain: f64, gains: &[f32]) -> Audio {
    let mut out = input.clone();
    for (frame, gain) in out.samples.chunks_exact_mut(input.channels).zip(gains) {
        let g = (*gain as f64 * pre_gain) as f32;
        for sample in frame {
            *sample *= g;
        }
    }
    out
}
pub fn guard(audio: &mut Audio, ceiling: f64) -> Result<f64> {
    let measured = meters::measure(audio)?;
    let trim = (ceiling - 0.08 - measured.true_peak_dbtp).min(0.);
    if trim < 0. {
        audio.gain(trim);
    }
    Ok(trim)
}

pub fn limit(
    input: &Audio,
    cfg: &LimiterConfig,
    cancel: &dyn Fn() -> bool,
) -> Result<(Audio, LimiterInfo)> {
    let loudness = meters::integrated(input)?;
    let mut pre_db = (cfg.target - loudness).clamp(-40., cfg.max_makeup);
    let mut info = LimiterInfo {
        reachable: true,
        ..Default::default()
    };
    if !cfg.enabled {
        let mut out = input.clone();
        out.gain(pre_db);
        info.trim_db = guard(&mut out, cfg.ceiling)?;
        info.pre_gain_db = pre_db;
        let m = meters::measure(&out)?;
        info.out_lufs = m.integrated_lufs;
        info.out_tp_dbtp = m.true_peak_dbtp;
        info.reachable = (info.out_lufs - cfg.target).abs() <= 0.2;
        return Ok((out, info));
    }
    let envelope = true_peak::envelope(input);
    let mut output = input.clone();
    for iteration in 0..6 {
        ensure!(!cancel(), "Cancelled");
        let gain = gain_curve(&envelope, input.sample_rate, lin(pre_db), cfg);
        let (gr, abs) = reduction(&gain);
        info.iterations = iteration + 1;
        info.max_gr_db = gr;
        info.absolute_max_gr_db = abs;
        if gr > cfg.max_gr + 1e-6 {
            let (mut low, mut high) = (pre_db - 40., pre_db);
            for _ in 0..12 {
                ensure!(!cancel(), "Cancelled");
                let mid = (low + high) * 0.5;
                let g = gain_curve(&envelope, input.sample_rate, lin(mid), cfg);
                if reduction(&g).0 > cfg.max_gr {
                    high = mid;
                } else {
                    low = mid;
                }
            }
            pre_db = low;
            let g = gain_curve(&envelope, input.sample_rate, lin(pre_db), cfg);
            (info.max_gr_db, info.absolute_max_gr_db) = reduction(&g);
            output = apply(input, lin(pre_db), &g);
            info.reachable = false;
            break;
        }
        output = apply(input, lin(pre_db), &gain);
        let error = cfg.target - meters::integrated(&output)?;
        if error.abs() < 0.1 {
            break;
        }
        let next = (pre_db + error * if error > 0. { 1.1 } else { 1. }).clamp(-40., cfg.max_makeup);
        if (next - pre_db).abs() < 0.001 {
            info.reachable = false;
            break;
        }
        if iteration < 5 {
            pre_db = next;
        }
    }
    info.pre_gain_db = pre_db;
    info.trim_db = guard(&mut output, cfg.ceiling)?;
    let m = meters::measure(&output)?;
    info.out_lufs = m.integrated_lufs;
    info.out_tp_dbtp = m.true_peak_dbtp;
    info.reachable &= (info.out_lufs - cfg.target).abs() <= 0.25;
    Ok((output, info))
}
