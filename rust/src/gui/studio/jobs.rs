use super::state::{LoadedSource, Revision};
use crate::studio::stem_mix::StemSet;
use anyhow::Result;
use eframe::egui;
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
    mpsc::{self, Receiver, Sender},
};

pub enum Change {
    Session(Box<crate::studio_native::session::StudioSnapshot>),
    PluginProbe(std::path::PathBuf, crate::studio_native::plugins::Probe),
    Analysis(Box<crate::analysis::Analysis>),
    Source(LoadedSource),
    Preview(Revision),
    Stems(Arc<StemSet>, Revision),
    EngineReady,
    Exported {
        paths: Vec<String>,
        warning: Option<String>,
    },
    Spectrogram(Arc<crate::studio::spectrogram::Spectrogram>),
}
pub enum Event {
    Progress(u64, f32, String),
    Done(u64, Result<Change, String>),
}

pub struct Jobs {
    pub receiver: Receiver<Event>,
    sender: Sender<Event>,
    pub cancel: Arc<AtomicBool>,
    pub active: Option<u64>,
    sequence: u64,
}
impl Default for Jobs {
    fn default() -> Self {
        let (sender, receiver) = mpsc::channel();
        Self {
            sender,
            receiver,
            cancel: Arc::new(AtomicBool::new(false)),
            active: None,
            sequence: 0,
        }
    }
}
impl Jobs {
    pub fn start(
        &mut self,
        ctx: &egui::Context,
        work: impl FnOnce(Arc<AtomicBool>, &dyn Fn(f32, &str)) -> Result<Change> + Send + 'static,
    ) -> Result<()> {
        anyhow::ensure!(self.active.is_none(), "A Studio job is already running");
        self.sequence += 1;
        let id = self.sequence;
        self.active = Some(id);
        self.cancel = Arc::new(AtomicBool::new(false));
        let cancel = self.cancel.clone();
        let sender = self.sender.clone();
        let repaint = ctx.clone();
        let spawned = std::thread::Builder::new()
            .name("slimey-studio".into())
            .spawn(move || {
                let progress = |fraction: f32, message: &str| {
                    let _ = sender.send(Event::Progress(
                        id,
                        fraction.clamp(0., 0.99),
                        message.into(),
                    ));
                    repaint.request_repaint();
                };
                let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                    work(cancel.clone(), &progress)
                }))
                .unwrap_or_else(|_| {
                    Err(anyhow::anyhow!(
                        "Studio worker panicked; original audio was preserved"
                    ))
                })
                .map_err(|e| format!("{e:#}"));
                let durable = matches!(
                    &result,
                    Ok(Change::Exported { .. }) | Ok(Change::EngineReady)
                );
                let result = if cancel.load(Ordering::Relaxed) && !durable {
                    Err("Cancelled; previous preview preserved".into())
                } else {
                    result
                };
                let _ = sender.send(Event::Done(id, result));
                repaint.request_repaint();
            });
        if let Err(error) = spawned {
            self.active = None;
            return Err(error.into());
        }
        Ok(())
    }
}
impl Drop for Jobs {
    fn drop(&mut self) {
        self.cancel.store(true, Ordering::Relaxed);
    }
}
