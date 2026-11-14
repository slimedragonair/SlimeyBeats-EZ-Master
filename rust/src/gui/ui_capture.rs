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

#[derive(serde::Serialize)]
pub(super) struct CaptureState {
    pub error: Option<String>,
    pub preview_ready: bool,
    pub playback_ready: bool,
    pub pending_stem_parts: usize,
    pub selected_stem_parts: usize,
}

pub(super) struct UiCapture {
    destination: Option<PathBuf>,
    state_destination: PathBuf,
    readiness: CaptureReadiness,
    receiver: Option<mpsc::Receiver<Result<(), String>>>,
    failed: Arc<AtomicBool>,
}
impl UiCapture {
    pub fn new(destination: PathBuf, failed: Arc<AtomicBool>) -> Self {
        Self {
            state_destination: destination.with_extension("state.json"),
            destination: Some(destination),
            readiness: CaptureReadiness::default(),
            receiver: None,
            failed,
        }
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
                    serde_json::to_writer_pretty(&mut file, &state)?;
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
