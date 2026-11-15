use super::{clip_runs, spectrum, thresholds::*, types::*, validation};
use crate::audio::Audio;
use anyhow::{Result, ensure};
use serde_json::json;

pub(super) struct Findings {
    pub issues: Vec<Issue>,
    pub truncated: bool,
}
impl Findings {
    pub fn push(&mut self, issue: Issue) {
        if self.issues.len() < MAX_ISSUES {
            self.issues.push(issue);
        } else {
            self.truncated = true;
        }
    }
}

/// Measure the supplied PCM without mutating it. The sample walk is linear;
/// spectral work and report storage have fixed caps independent of duration.
pub fn scan(audio: &Audio, cancel: &dyn Fn() -> bool) -> Result<IssueReport> {
    ensure!(!cancel(), "Cancelled");
    validation::layout(audio)?;
    validation::samples(audio, cancel)?;
    let mut findings = Findings {
        issues: Vec::new(),
        truncated: false,
    };
    for channel in 0..audio.channels {
        clip_runs::visit(audio, 0..audio.frames(), channel, cancel, |run| {
            if !run.is_issue() {
                return;
            }
            let repairable = run.repairable(audio, channel);
            findings.push(Issue {
                id: format!("clipping-{channel}-{}-{}", run.start, run.end),
                kind: IssueKind::Clipping,
                start_seconds: run.start as f64 / audio.sample_rate as f64,
                end_seconds: run.end as f64 / audio.sample_rate as f64,
                severity: "warning".into(), title: "Full-scale sample candidate".into(),
                description: "A full-scale plateau or above-unity PCM sample was measured. This is not a true-peak measurement or proof of historical clipping. Review before repair.".into(),
                evidence: json!({"measurement": "absolute_pcm_sample_peak", "channel": channel,
                    "channel_number": channel + 1, "run_frames": run.end - run.start,
                    "minimum_absolute_sample": run.minimum, "peak_absolute_sample": run.peak,
                    "sample_peak_dbfs": 20. * run.peak.log10(),
                    "full_scale_plateau": run.is_plateau(), "above_unity_sample": run.peak > 1.,
                    "repair_eligible": repairable, "true_peak_measured": false}),
                repair: repairable.then_some(RepairAction::Declip),
            });
        })?;
    }
    quiet_regions(audio, cancel, &mut findings)?;
    let spectral_note = spectrum::inspect(audio, cancel, &mut findings)?;
    ensure!(!cancel(), "Cancelled");
    findings.issues.sort_by(|a, b| {
        a.start_seconds
            .total_cmp(&b.start_seconds)
            .then(a.end_seconds.total_cmp(&b.end_seconds))
            .then(a.id.cmp(&b.id))
    });
    let mut notes = vec![
        "Deterministic heuristics for review; no repairs or cuts were applied.".into(),
        "Clipping checks absolute PCM samples, not the track's normalized maximum or reconstructed true peaks.".into(),
        "Quiet regions require all channels below the absolute threshold; possible endings require sustained active audio on both sides.".into(),
        spectral_note,
    ];
    if findings.truncated {
        notes.push(format!(
            "Issue list capped at {MAX_ISSUES}; additional findings were omitted."
        ));
    }
    Ok(IssueReport {
        schema_version: 1,
        duration_seconds: audio.duration(),
        sample_rate: audio.sample_rate,
        channels: audio.channels,
        thresholds: super::thresholds::report(),
        issues: findings.issues,
        truncated: findings.truncated,
        notes,
    })
}

#[derive(Clone, Copy)]
struct QuietGap {
    start: usize,
    end: usize,
    preceded: bool,
}

fn quiet_regions(audio: &Audio, cancel: &dyn Fn() -> bool, findings: &mut Findings) -> Result<()> {
    let block = (audio.sample_rate as usize / 100).max(1);
    let required_active = (RESUME_SECONDS * audio.sample_rate as f64).ceil() as usize;
    let mut quiet: Option<QuietGap> = None;
    let mut pending: Option<QuietGap> = None;
    let mut active_frames = 0;
    let mut resumed_frames = 0;
    for start in (0..audio.frames()).step_by(block) {
        ensure!(!cancel(), "Cancelled");
        let end = (start + block).min(audio.frames());
        let mut peak = 0_f64;
        let mut energy = [0_f64; 2];
        for frame in
            audio.samples[start * audio.channels..end * audio.channels].chunks_exact(audio.channels)
        {
            for (channel, &sample) in frame.iter().enumerate() {
                peak = peak.max((sample as f64).abs());
                energy[channel] += (sample as f64).powi(2);
            }
        }
        let active = energy[..audio.channels]
            .iter()
            .any(|e| *e / (end - start) as f64 >= ACTIVE_RMS.powi(2));
        if peak <= QUIET_PEAK {
            let gap = quiet.get_or_insert(QuietGap {
                start,
                end,
                preceded: active_frames >= required_active,
            });
            gap.end = end;
            active_frames = 0;
            pending = None;
            resumed_frames = 0;
        } else {
            if let Some(gap) = quiet.take() {
                record_silence(audio, gap, findings);
                if gap.preceded
                    && (gap.end - gap.start) as f64 / audio.sample_rate as f64 >= ENDING_GAP_SECONDS
                {
                    pending = Some(gap);
                }
            }
            if active {
                active_frames += end - start;
                if let Some(gap) = pending {
                    resumed_frames += end - start;
                    if resumed_frames >= required_active {
                        findings.push(Issue {
                            id: format!("possible-ending-{}-{}", gap.start, gap.end),
                            kind: IssueKind::PossibleEnding,
                            start_seconds: gap.start as f64 / audio.sample_rate as f64,
                            end_seconds: gap.end as f64 / audio.sample_rate as f64,
                            severity: "info".into(), title: "Internal quiet gap before resumed audio".into(),
                            description: "Sustained active audio occurs before and after this quiet gap. It may be an intentional pause or a false ending; no cut is suggested.".into(),
                            evidence: json!({"quiet_seconds": (gap.end - gap.start) as f64 / audio.sample_rate as f64,
                                "preceding_active_seconds_at_least": RESUME_SECONDS,
                                "resumed_active_seconds_at_least": resumed_frames as f64 / audio.sample_rate as f64,
                                "all_channels_quiet": true}), repair: None,
                        });
                        pending = None;
                    }
                }
            } else {
                active_frames = 0;
                pending = None;
                resumed_frames = 0;
            }
        }
    }
    if let Some(gap) = quiet {
        record_silence(audio, gap, findings);
    }
    Ok(())
}

fn record_silence(audio: &Audio, gap: QuietGap, findings: &mut Findings) {
    let seconds = (gap.end - gap.start) as f64 / audio.sample_rate as f64;
    if seconds < SILENCE_SECONDS {
        return;
    }
    findings.push(Issue {
        id: format!("silence-{}-{}", gap.start, gap.end), kind: IssueKind::Silence,
        start_seconds: gap.start as f64 / audio.sample_rate as f64,
        end_seconds: gap.end as f64 / audio.sample_rate as f64,
        severity: "info".into(), title: "Quiet region across all channels".into(),
        description: "Every channel stays below the absolute quiet threshold. This can be intentional; no trimming or zeroing is performed.".into(),
        evidence: json!({"quiet_seconds": seconds, "all_channels_quiet": true,
            "maximum_allowed_sample_peak_dbfs": -70.,
            "at_track_start": gap.start == 0, "at_track_end": gap.end == audio.frames()}), repair: None,
    });
}
