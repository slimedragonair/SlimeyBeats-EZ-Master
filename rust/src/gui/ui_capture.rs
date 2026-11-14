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
    time::Duration,
};

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
    settled_frames: usize,
    receiver: Option<mpsc::Receiver<Result<(), String>>>,
    failed: Arc<AtomicBool>,
}
impl UiCapture {
    pub fn new(destination: PathBuf, failed: Arc<AtomicBool>) -> Self {
        Self {
            state_destination: destination.with_extension("state.json"),
            destination: Some(destination),
            settled_frames: 0,
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
        self.settled_frames = if ready { self.settled_frames + 1 } else { 0 };
        ctx.request_repaint_after(Duration::from_millis(40));
        if self.settled_frames < 5 {
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
