//! Transient findings are bound to an immutable input and a Studio revision epoch.
use super::state::{Studio, Target};
use crate::{audio::Audio, gui::plots::Waveform, studio::doctor::IssueReport};
use std::sync::Arc;

#[derive(Clone)]
pub struct Input {
    target: Target,
    audio: Arc<Audio>,
    epoch: u64,
}

pub struct Findings {
    pub input: Input,
    pub report: IssueReport,
    pub waveform: Waveform,
}

pub(super) struct Doctor {
    // This is a transient view within Repair, preserving saved workspace schemas.
    pub visible: bool,
    pub strength: f64,
    pub findings: Option<Findings>,
    pub selected: Option<usize>,
    observed: Option<Input>,
    epoch: u64,
}

impl Default for Doctor {
    fn default() -> Self {
        Self {
            visible: false,
            strength: 0.5,
            findings: None,
            selected: None,
            observed: None,
            epoch: 0,
        }
    }
}

impl Doctor {
    pub fn invalidate(&mut self) {
        self.epoch = self.epoch.wrapping_add(1);
        self.findings = None;
        self.selected = None;
        self.observed = None;
    }
}

impl Input {
    pub fn capture(studio: &Studio) -> anyhow::Result<Self> {
        Ok(Self {
            target: studio.target,
            audio: studio.audio_target()?,
            epoch: studio.doctor.epoch,
        })
    }

    pub fn audio(&self) -> &Arc<Audio> {
        &self.audio
    }

    pub fn matches(&self, studio: &Studio) -> bool {
        self.epoch == studio.doctor.epoch
            && self.target == studio.target
            && studio
                .audio_target()
                .is_ok_and(|audio| Arc::ptr_eq(&audio, &self.audio))
    }
}

impl Studio {
    pub(super) fn sync_doctor_input(&mut self) {
        if self
            .doctor
            .observed
            .as_ref()
            .is_some_and(|input| !input.matches(self))
        {
            self.doctor.invalidate();
        }
        if self.doctor.observed.is_none() {
            self.doctor.observed = Input::capture(self).ok();
        }
    }

    pub(super) fn select_doctor_issue(&mut self, index: usize) {
        self.sync_doctor_input();
        let Some(findings) = &self.doctor.findings else {
            return;
        };
        let Some(issue) = findings.report.issues.get(index) else {
            return;
        };
        let (start, end) = (issue.start_seconds, issue.end_seconds);
        let audio = findings.input.audio();
        self.editor
            .select_time(start, end, audio.sample_rate, audio.frames());
        if let Target::Stem(kind) = self.target {
            self.editor.selected = Some(kind);
        }
        self.doctor.selected = Some(index);
        self.player.seek(start);
        // Selecting a finding updates an existing audition loop without enabling it.
        self.update_editor_loop();
    }
}
