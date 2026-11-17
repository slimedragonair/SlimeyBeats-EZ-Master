//! Opt-in renderer captures for repeatable visual QA, without desktop input automation.
use eframe::egui;
use image::ImageEncoder;
use std::{
    path::PathBuf,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
        mpsc,
    },
    time::{Duration, Instant},
};

const MIN_READY_TIME: Duration = Duration::from_millis(350);

#[cfg(test)]
mod readiness_tests {
    use super::*;
    #[test]
    fn repeated_layout_passes_cannot_capture_a_dialog_before_it_has_settled() {
        let start = Instant::now();
        let mut gate = CaptureReadiness::default();
        for _ in 0..20 {
            assert!(!gate.update(true, start));
        }
        assert!(gate.update(true, start + Duration::from_millis(400)));
        assert!(!gate.update(false, start + Duration::from_millis(450)));
        assert!(!gate.update(true, start + Duration::from_millis(600)));
        for _ in 0..5 {
            gate.update(true, start + Duration::from_millis(700));
        }
        assert!(!gate.update(true, start + Duration::from_millis(750)));
        assert!(gate.update(true, start + Duration::from_millis(1000)));
    }

    #[test]
    fn doctor_capture_blocks_early_screenshots_and_reports_failure_without_a_png() {
        let ctx = egui::Context::default();
        let root = tempfile::tempdir().unwrap();
        let path = root.path().join("doctor.png");
        let failed = Arc::new(AtomicBool::new(false));
        let mut capture = UiCapture::new(path.clone(), failed.clone());
        capture.require_doctor_scan();
        super::super::studio::doctor_capture::request(&ctx);
        for _ in 0..10 {
            assert!(
                capture
                    .update(&ctx, true, CaptureState::default())
                    .is_none()
            );
        }
        assert!(capture.destination.is_some() && !path.exists());
        let mut studio = super::super::studio::Studio::new(root.path().into());
        studio.error = Some("Synthetic capture load failure".into());
        studio.poll(&ctx);
        assert!(
            capture
                .update(&ctx, true, CaptureState::default())
                .unwrap()
                .is_err()
        );
        assert!(failed.load(Ordering::Relaxed) && !path.exists());
    }

    #[test]
    fn doctor_metadata_records_only_current_completed_scan_and_normal_state_is_unchanged() {
        let normal = serde_json::to_value(RecordedCapture {
            state: CaptureState::default(),
            doctor: None,
        })
        .unwrap();
        assert!(normal.get("doctor_scan_ready").is_none());
        let ctx = egui::Context::default();
        let mut studio = super::super::test_fixture::app(&ctx).studio;
        studio.source = Some(super::super::studio::state::LoadedSource {
            audio: Arc::new(crate::audio::Audio::new(vec![1.; 4800], 48000, 1).unwrap()),
            path: "Synthetic capture test.wav".into(),
            loudness: -20.,
        });
        super::super::studio::doctor_capture::request(&ctx);
        studio.poll(&ctx);
        let deadline = Instant::now() + Duration::from_secs(10);
        while studio.busy() && Instant::now() < deadline {
            studio.poll(&ctx);
            std::thread::sleep(Duration::from_millis(1));
        }
        assert!(!studio.busy());
        let record = RecordedCapture {
            state: CaptureState::default(),
            doctor: super::super::studio::doctor_capture::status(&ctx),
        };
        let json = serde_json::to_value(record).unwrap();
        assert_eq!(json["doctor_scan_ready"], true);
        assert!(json["doctor_issue_count"].as_u64().unwrap() > 0);
        assert!(json.get("phase").is_none());
    }
}

#[derive(Default)]
struct CaptureReadiness {
    passes: usize,
    since: Option<Instant>,
}

impl CaptureReadiness {
    fn update(&mut self, ready: bool, now: Instant) -> bool {
        if !ready {
            self.passes = 0;
            self.since = None;
            return false;
        }
        self.passes += 1;
        let since = *self.since.get_or_insert(now);
        self.passes >= 5 && now.saturating_duration_since(since) >= MIN_READY_TIME
    }
}

#[derive(Default, serde::Serialize)]
pub(super) struct CaptureState {
    pub recovery_saved: bool,
    pub recovery_enabled: bool,
    pub recovery_error: Option<String>,
    pub recovery_points: usize,
    pub recovery_restored: bool,
    pub support_preview_ready: bool,
    pub update_build_revision: u64,
    pub update_check_enabled: bool,
    pub update_error: Option<String>,
    pub error: Option<String>,
    pub preview_ready: bool,
    pub playback_ready: bool,
    pub pending_stem_parts: usize,
    pub selected_stem_parts: usize,
    pub arrangement_tracks: usize,
    pub arrangement_clips: usize,
    pub arrangement_effects: usize,
    pub arrangement_eq_open: bool,
}

pub(super) struct UiCapture {
    doctor_required: bool,
    destination: Option<PathBuf>,
    state_destination: PathBuf,
    readiness: CaptureReadiness,
    receiver: Option<mpsc::Receiver<Result<(), String>>>,
    failed: Arc<AtomicBool>,
}
impl UiCapture {
    pub fn new(destination: PathBuf, failed: Arc<AtomicBool>) -> Self {
        Self {
            doctor_required: false,
            state_destination: destination.with_extension("state.json"),
            destination: Some(destination),
            readiness: CaptureReadiness::default(),
            receiver: None,
            failed,
        }
    }
    pub fn require_doctor_scan(&mut self) {
        self.doctor_required = true;
    }
    pub fn update(
        &mut self,
        ctx: &egui::Context,
        ready: bool,
        state: CaptureState,
    ) -> Option<Result<(), String>> {
        if let Some(receiver) = &self.receiver {
            let result = match receiver.try_recv() {
                Ok(result) => Some(result),
                Err(mpsc::TryRecvError::Disconnected) => {
                    Some(Err("Renderer capture callback disconnected".into()))
                }
                Err(mpsc::TryRecvError::Empty) => None,
            };
            if matches!(&result, Some(Err(_))) {
                self.failed.store(true, Ordering::Relaxed);
            }
            return result;
        }
        ctx.request_repaint_after(Duration::from_millis(40));
        let doctor = self
            .doctor_required
            .then(|| super::studio::doctor_capture::status(ctx).unwrap_or_default());
        if let Some(error) = doctor.as_ref().and_then(|status| status.error.clone()) {
            self.failed.store(true, Ordering::Relaxed);
            return Some(Err(error));
        }
        let ready = ready
            && doctor
                .as_ref()
                .is_none_or(|status| status.doctor_scan_ready);
        // A layout pass is not a presented frame. Multipass sizing can otherwise
        // capture a learning dialog during its fade-in, before its text is opaque.
        if !self.readiness.update(ready, Instant::now()) {
            return None;
        }
        let destination = self.destination.take()?;
        let (sender, receiver) = mpsc::channel();
        self.receiver = Some(receiver);
        let repaint = ctx.clone();
        let state_destination = self.state_destination.clone();
        ctx.request_screenshot(move |image| {
            let result = save_png(&destination, &image)
                .and_then(|()| {
                    use std::io::Write;
                    let mut file = std::fs::OpenOptions::new()
                        .create_new(true)
                        .write(true)
                        .open(state_destination)?;
                    serde_json::to_writer_pretty(&mut file, &RecordedCapture { state, doctor })?;
                    file.write_all(b"\n")?;
                    Ok(())
                })
                .map_err(|error| format!("UI capture {}: {error:#}", destination.display()));
            let _ = sender.send(result);
            repaint.request_repaint();
        });
        None
    }
}

#[derive(serde::Serialize)]
struct RecordedCapture {
    #[serde(flatten)]
    state: CaptureState,
    #[serde(flatten)]
    doctor: Option<super::studio::doctor_capture::Status>,
}

fn save_png(destination: &std::path::Path, image: &egui::ColorImage) -> anyhow::Result<()> {
    let file = std::fs::OpenOptions::new()
        .create_new(true)
        .write(true)
        .open(destination)?;
    let bytes: Vec<_> = image
        .pixels
        .iter()
        .flat_map(|pixel| pixel.to_array())
        .collect();
    image::codecs::png::PngEncoder::new(file).write_image(
        &bytes,
        image.width() as u32,
        image.height() as u32,
        image::ExtendedColorType::Rgba8,
    )?;
    Ok(())
}
