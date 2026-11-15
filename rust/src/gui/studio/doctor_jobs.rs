//! Background scan and explicit repair preparation; acceptance rechecks the input.
use super::{
    doctor_state::{Findings, Input},
    jobs::Change,
    state::{Revision, Studio},
};
use crate::{
    gui::plots::Waveform,
    studio::doctor::{self, RepairRequest},
};
use eframe::egui;
use std::sync::{Arc, atomic::Ordering};

pub enum Result {
    Scan(Findings),
    Preview(Input, Revision),
    Unchanged(Input, Vec<String>),
}

impl Studio {
    pub(super) fn scan_doctor(&mut self, ctx: &egui::Context) {
        if self.busy() {
            self.error = Some("Wait for the current Studio job or cancel it first.".into());
            return;
        }
        self.sync_doctor_input();
        let input = match Input::capture(self) {
            Ok(input) => input,
            Err(error) => {
                self.error = Some(error.to_string());
                return;
            }
        };
        self.doctor.findings = None;
        self.doctor.selected = None;
        self.begin(
            ctx,
            "Scanning selected audio · original preserved",
            move |cancel, progress| {
                let cancelled = || cancel.load(Ordering::Relaxed);
                let report = doctor::scan(input.audio(), &cancelled)?;
                progress(0.95, "Preparing issue markers");
                anyhow::ensure!(!cancelled(), "Cancelled");
                let waveform = Waveform::new(input.audio(), None);
                Ok(Change::Doctor(Box::new(Result::Scan(Findings {
                    input,
                    report,
                    waveform,
                }))))
            },
        );
    }

    pub(super) fn preview_doctor_repair(&mut self, ctx: &egui::Context) {
        self.sync_doctor_input();
        let result = (|| -> anyhow::Result<_> {
            anyhow::ensure!(
                !self.busy(),
                "Wait for the current Studio job or cancel it first."
            );
            self.editing_target()?;
            let findings = self
                .doctor
                .findings
                .as_ref()
                .ok_or_else(|| anyhow::anyhow!("Scan the selected audio first."))?;
            anyhow::ensure!(findings.input.matches(self), "Input changed; scan again.");
            let issue = self
                .doctor
                .selected
                .and_then(|index| findings.report.issues.get(index))
                .ok_or_else(|| anyhow::anyhow!("Select a finding first."))?;
            let action = issue.repair.as_ref().ok_or_else(|| anyhow::anyhow!("This finding is inspection only; no automatic cut or reconstruction is available."))?;
            anyhow::ensure!(
                self.doctor.strength.is_finite(),
                "Repair strength must be finite."
            );
            let request = RepairRequest {
                action: *action,
                start_seconds: issue.start_seconds,
                end_seconds: issue.end_seconds,
                strength: self.doctor.strength.clamp(0., 1.),
            };
            Ok((findings.input.clone(), request))
        })();
        let (input, request) = match result {
            Ok(value) => value,
            Err(error) => {
                self.error = Some(error.to_string());
                return;
            }
        };
        let bank = self.bank.clone();
        let mix = self.mix.clone();
        self.begin(ctx, "Preparing an explicit Stem Doctor repair preview", move |cancel, progress| {
            let cancelled = || cancel.load(Ordering::Relaxed);
            let result = doctor::repair(input.audio(), &request, &cancelled)?;
            anyhow::ensure!(!cancelled(), "Cancelled");
            if result.changed_samples == 0 {
                return Ok(Change::Doctor(Box::new(Result::Unchanged(input, result.notes))));
            }
            progress(0.85, "Measuring the repair preview for A/B listening");
            let before_lufs = crate::meters::integrated(input.audio())?;
            let after_lufs = crate::meters::integrated(&result.audio)?;
            anyhow::ensure!(!cancelled(), "Cancelled");
            let mut notes = result.notes;
            notes.push(format!("Stem Doctor · {:.3}–{:.3} s · strength {:.2} · {} changed samples · peak trim {:.2} dB. Compare A/B before applying or exporting.", request.start_seconds, request.end_seconds, request.strength, result.changed_samples, result.peak_trim_db));
            let revision = Revision {
                before: input.audio().clone(),
                audio: Arc::new(result.audio),
                label: "Stem Doctor repair".into(),
                notes,
                before_lufs,
                after_lufs,
                bank,
                mix,
            };
            Ok(Change::Doctor(Box::new(Result::Preview(input, revision))))
        });
    }

    pub(super) fn accept_doctor_result(&mut self, result: Result) {
        if self.jobs.cancel.load(Ordering::Relaxed) {
            self.status = "Stem Doctor cancelled · previous audio preserved".into();
            return;
        }
        let input = match &result {
            Result::Scan(findings) => &findings.input,
            Result::Preview(input, _) | Result::Unchanged(input, _) => input,
        };
        if !input.matches(self) {
            self.status =
                "Input changed · outdated Stem Doctor result discarded; scan again".into();
            return;
        }
        if matches!(&result, Result::Preview(_, _)) && self.mix_dirty {
            self.status = "Mix changed · repair preview discarded; update mix preview first".into();
            return;
        }
        match result {
            Result::Scan(findings) => {
                self.status = format!(
                    "Scan complete · {} finding(s) · audio unchanged",
                    findings.report.issues.len()
                );
                self.doctor.findings = Some(findings);
                self.doctor.selected = None;
            }
            Result::Preview(_, revision) => {
                self.accept(revision, true);
                self.status =
                    "Stem Doctor preview ready · compare A/B; apply to the stem explicitly".into();
            }
            Result::Unchanged(_, notes) => {
                self.status = format!(
                    "No repair changes · previous preview preserved. {}",
                    notes.join(" ")
                );
            }
        }
    }
}
