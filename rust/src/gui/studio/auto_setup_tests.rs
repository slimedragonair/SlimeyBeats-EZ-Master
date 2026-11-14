//! Setup orchestration and native controls, without model downloads or inference.
use super::{Studio, auto_setup};
use crate::{
    gui::{
        studio::{
            interaction_tests::{click, fixture, frame, text},
            state::Page,
        },
        theme,
    },
    studio_native::runtime::{MODEL_SHA256, RUNTIME_API},
};
use eframe::egui;
use std::{
    cell::RefCell,
    collections::HashMap,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
        mpsc,
    },
    time::{Duration, Instant},
};

#[test]
fn missing_engine_installs_then_separates_with_one_cancel_token_and_mapped_progress() {
    let cancel = Arc::new(AtomicBool::new(false));
    let calls = RefCell::new(Vec::new());
    let updates = RefCell::new(Vec::new());
    let result = auto_setup::run(
        true,
        cancel.clone(),
        &|fraction, message| updates.borrow_mut().push((fraction, message.to_owned())),
        |progress, token| {
            assert!(Arc::ptr_eq(&cancel, &token));
            calls.borrow_mut().push("install");
            progress(0.5, "download");
            progress(1., "verified");
            Ok(())
        },
        |progress, token| {
            assert!(Arc::ptr_eq(&cancel, &token));
            calls.borrow_mut().push("separate");
            for fraction in [0., 0.5, 0.2, f32::NAN, 1., 2.] {
                progress(fraction, "inference");
            }
            Ok("usable stems")
        },
    )
    .unwrap();
    assert_eq!(result, "usable stems");
    assert_eq!(*calls.borrow(), ["install", "separate"]);
    let updates = updates.borrow();
    assert!(
        updates
            .iter()
            .all(|(f, _)| f.is_finite() && (0. ..1.).contains(f))
    );
    assert!(updates.windows(2).all(|w| w[0].0 <= w[1].0));
    let installed = updates.iter().find(|(_, m)| m == "verified").unwrap().0;
    let first_inference = updates.iter().find(|(_, m)| m == "inference").unwrap().0;
    assert!(installed > 0. && installed < 1.);
    assert_eq!(installed, first_inference);
}

#[test]
fn ready_engine_separates_without_calling_installer() {
    let updates = RefCell::new(Vec::new());
    let result = auto_setup::run(
        false,
        Arc::new(AtomicBool::new(false)),
        &|fraction, _| updates.borrow_mut().push(fraction),
        |_, _| panic!("Ready engines must not trigger network setup"),
        |progress, _| {
            progress(0., "starting");
            progress(1., "aligned");
            Ok(42)
        },
    )
    .unwrap();
    assert_eq!(result, 42);
    assert_eq!(updates.borrow()[0], 0.);
    assert!(updates.borrow().iter().all(|fraction| *fraction < 1.));
}

#[test]
fn failed_setup_never_calls_separation_and_keeps_the_actionable_error() {
    let result = auto_setup::run(
        true,
        Arc::new(AtomicBool::new(false)),
        &|_, _| {},
        |_, _| anyhow::bail!("No disk space"),
        |_, _| -> anyhow::Result<()> { panic!("Failed setup must stop this job") },
    );
    let error = format!("{:#}", result.unwrap_err());
    assert!(error.contains("No disk space") && error.contains("Setup and separate"));
}

#[test]
fn cancellation_before_setup_never_calls_either_stage() {
    let result = auto_setup::run(
        true,
        Arc::new(AtomicBool::new(true)),
        &|_, _| panic!("Already cancelled"),
        |_, _| panic!("Already cancelled"),
        |_, _| -> anyhow::Result<()> { panic!("Already cancelled") },
    );
    assert!(result.unwrap_err().to_string().contains("Cancelled"));
}

#[test]
fn completed_setup_after_cancel_stays_cached_but_only_a_new_request_separates() {
    let cached = AtomicBool::new(false);
    let cancelled = auto_setup::run(
        true,
        Arc::new(AtomicBool::new(false)),
        &|_, _| {},
        |_, token| {
            cached.store(true, Ordering::Relaxed);
            token.store(true, Ordering::Relaxed);
            Ok(())
        },
        |_, _| -> anyhow::Result<()> { panic!("A cached setup cannot revive a cancelled job") },
    );
    assert!(cancelled.unwrap_err().to_string().contains("Cancelled"));
    assert!(cached.load(Ordering::Relaxed));
    let retry = auto_setup::run(
        !cached.load(Ordering::Relaxed),
        Arc::new(AtomicBool::new(false)),
        &|_, _| {},
        |_, _| panic!("Reuse the completed setup"),
        |_, _| Ok("explicit retry"),
    )
    .unwrap();
    assert_eq!(retry, "explicit retry");
}

#[test]
fn late_inference_cancellation_cannot_publish_a_success() {
    let result = auto_setup::run(
        false,
        Arc::new(AtomicBool::new(false)),
        &|_, _| {},
        |_, _| panic!("No setup needed"),
        |_, token| {
            token.store(true, Ordering::Relaxed);
            Ok("late result")
        },
    );
    assert!(result.unwrap_err().to_string().contains("Cancelled"));
}

#[test]
fn cancellation_at_the_setup_or_separation_progress_boundary_stops_the_next_stage() {
    for stop_at_setup in [false, true] {
        let cancel = Arc::new(AtomicBool::new(false));
        let installed = AtomicBool::new(false);
        let result = auto_setup::run(
            true,
            cancel.clone(),
            &|_, message| {
                if stop_at_setup || message == "Separating the source locally" {
                    cancel.store(true, Ordering::Relaxed);
                }
            },
            |_, _| {
                installed.store(true, Ordering::Relaxed);
                Ok(())
            },
            |_, _| -> anyhow::Result<()> { panic!("Cancelled at the stage boundary") },
        );
        assert!(result.unwrap_err().to_string().contains("Cancelled"));
        assert_eq!(installed.load(Ordering::Relaxed), !stop_at_setup);
    }
}

fn context() -> egui::Context {
    let ctx = egui::Context::default();
    theme::apply(&ctx);
    ctx.enable_accesskit();
    ctx.all_styles_mut(|style| style.animation_time = 0.);
    ctx
}

fn settled(ctx: &egui::Context, studio: &mut Studio) -> egui::FullOutput {
    let mut output = frame(ctx, studio, false, egui::vec2(1480., 1800.), 332., vec![]).0;
    for _ in 0..5 {
        output = frame(ctx, studio, false, egui::vec2(1480., 1800.), 332., vec![]).0;
    }
    output
}

fn open_engine(ctx: &egui::Context, studio: &mut Studio) -> egui::FullOutput {
    let output = settled(ctx, studio);
    let header = if studio.bank.is_some() {
        "Optional local stem engine"
    } else {
        "Optional: separate a full mix locally"
    };
    click(ctx, studio, &output, header);
    settled(ctx, studio)
}

fn activate_exact(
    ctx: &egui::Context,
    studio: &mut Studio,
    output: &egui::FullOutput,
    label: &str,
) {
    // The explanation also names the action; activate the actual button text.
    let shape = output.shapes.iter().find(|shape| matches!(&shape.shape, egui::Shape::Text(text) if text.galley.text() == label && shape.clip_rect.contains_rect(text.visual_bounding_rect()))).unwrap_or_else(|| panic!("Missing visible control: {label}"));
    let pos = shape.shape.visual_bounding_rect().center();
    for pressed in [true, false] {
        frame(
            ctx,
            studio,
            false,
            egui::vec2(1480., 1800.),
            332.,
            vec![
                egui::Event::PointerMoved(pos),
                egui::Event::PointerButton {
                    pos,
                    button: egui::PointerButton::Primary,
                    pressed,
                    modifiers: egui::Modifiers::NONE,
                },
            ],
        );
    }
}

fn button_enabled(output: &egui::FullOutput, label: &str) -> bool {
    let tree = output.platform_output.accesskit_update.as_ref().unwrap();
    let nodes: HashMap<_, _> = tree.nodes.iter().map(|(id, node)| (*id, node)).collect();
    fn name(
        id: egui::accesskit::NodeId,
        nodes: &HashMap<egui::accesskit::NodeId, &egui::accesskit::Node>,
    ) -> String {
        let node = nodes[&id];
        node.label().map(str::to_owned).unwrap_or_else(|| {
            node.labelled_by()
                .iter()
                .map(|id| name(*id, nodes))
                .collect::<Vec<_>>()
                .join(" ")
        })
    }
    let (_, node) = tree
        .nodes
        .iter()
        .find(|(id, node)| {
            node.role() == egui::accesskit::Role::Button && name(*id, &nodes) == label
        })
        .unwrap_or_else(|| panic!("Missing native button: {label}"));
    !node.is_disabled()
}

fn ready_receipt(studio: &Studio) {
    std::fs::create_dir_all(studio.runtime.python().parent().unwrap()).unwrap();
    std::fs::create_dir_all(studio.runtime.models()).unwrap();
    std::fs::write(
        studio.runtime.python(),
        b"readiness fixture only; never executed",
    )
    .unwrap();
    std::fs::write(studio.runtime.models().join("model.index"), b"fixture").unwrap();
    std::fs::write(
        studio.runtime.receipt(),
        serde_json::to_vec(
            &serde_json::json!({"runtime_api": RUNTIME_API, "model_sha256": MODEL_SHA256}),
        )
        .unwrap(),
    )
    .unwrap();
    assert!(studio.runtime.is_ready());
}

#[test]
fn native_separate_button_uses_setup_label_and_routes_only_with_source_and_without_busy_job() {
    for ready in [false, true] {
        for has_source in [false, true] {
            for busy in [false, true] {
                let root = tempfile::tempdir().unwrap();
                let ctx = context();
                let mut studio = fixture(0);
                studio.runtime.base = root.path().join("engine");
                // Clicking must exercise the real handler without ever starting a download.
                studio.cache = None;
                if ready {
                    ready_receipt(&studio);
                }
                if !has_source {
                    studio.source = None;
                }
                open_engine(&ctx, &mut studio);
                assert!(!studio.busy(), "Disclosure must not start setup");
                studio.error = None;
                studio.jobs.active = busy.then_some(500);
                let output = settled(&ctx, &mut studio);
                let label = if ready {
                    "Separate source"
                } else {
                    "Setup and separate"
                };
                assert_eq!(button_enabled(&output, label), has_source && !busy);
                if !ready {
                    for explanation in [
                        "One-time setup",
                        "internet, disk space",
                        "several minutes",
                        "automatically",
                        "Cancel",
                    ] {
                        assert!(
                            text(&output, explanation).is_some(),
                            "Hidden explanation: {explanation}"
                        );
                    }
                }
                activate_exact(&ctx, &mut studio, &output, label);
                if has_source && !busy {
                    assert!(
                        studio
                            .error
                            .as_deref()
                            .unwrap()
                            .contains("Private Studio cache")
                    );
                } else {
                    assert!(studio.error.is_none(), "Disabled control routed an action");
                }
                assert_eq!(studio.jobs.active, busy.then_some(500));
            }
        }
    }
}

#[test]
fn engine_maintenance_is_optional_for_both_missing_and_ready_engines() {
    for ready in [false, true] {
        let root = tempfile::tempdir().unwrap();
        let ctx = context();
        let mut studio = fixture(0);
        studio.runtime.base = root.path().join("engine");
        if ready {
            ready_receipt(&studio);
        }
        let output = open_engine(&ctx, &mut studio);
        assert!(text(&output, "Setup stem engine").is_none());
        assert!(text(&output, "Verify installed model").is_none());
        assert!(text(&output, "Repair stem engine").is_none());
        click(&ctx, &mut studio, &output, "Engine maintenance");
        let output = settled(&ctx, &mut studio);
        assert!(button_enabled(
            &output,
            if ready {
                "Verify installed model"
            } else {
                "Setup stem engine"
            }
        ));
        assert!(button_enabled(&output, "Repair stem engine"));
        assert!(
            !studio.busy(),
            "Opening maintenance must never set up an engine"
        );
    }
}

fn finish(ctx: &egui::Context, studio: &mut Studio) {
    let deadline = Instant::now() + Duration::from_secs(5);
    while studio.busy() && Instant::now() < deadline {
        studio.poll(ctx);
        std::thread::sleep(Duration::from_millis(2));
    }
    assert!(!studio.busy(), "Studio job failed to finish");
}

#[test]
fn setup_failure_preserves_existing_audio_bank_and_pending_settings() {
    let root = tempfile::tempdir().unwrap();
    let ctx = context();
    let mut studio = fixture(2);
    studio.runtime.base = root.path().join("engine");
    std::fs::create_dir_all(&studio.runtime.base).unwrap();
    let unrelated = studio.runtime.base.join("keep.txt");
    std::fs::write(&unrelated, b"Preserve existing files").unwrap();
    studio.mix.tracks[0].gain_db = -7.;
    studio.mix_dirty = true;
    studio.voice.deess = 0.37;
    studio.two_stems = false;
    let source = studio.source.as_ref().unwrap().audio.clone();
    let preview = studio.current.as_ref().unwrap().audio.clone();
    let bank = studio.bank.clone().unwrap();
    let mix = serde_json::to_value(&studio.mix).unwrap();
    let voice = serde_json::to_value(&studio.voice).unwrap();
    studio.separate(&ctx);
    assert!(
        studio.busy(),
        "Missing engines must enter the same background job"
    );
    finish(&ctx, &mut studio);
    assert!(
        studio
            .error
            .as_deref()
            .unwrap()
            .contains("existing unrelated files were preserved")
    );
    assert!(Arc::ptr_eq(&source, &studio.source.as_ref().unwrap().audio));
    assert!(Arc::ptr_eq(
        &preview,
        &studio.current.as_ref().unwrap().audio
    ));
    assert!(Arc::ptr_eq(&bank, studio.bank.as_ref().unwrap()));
    assert_eq!(serde_json::to_value(&studio.mix).unwrap(), mix);
    assert_eq!(serde_json::to_value(&studio.voice).unwrap(), voice);
    assert!(studio.mix_dirty && !studio.two_stems);
    assert_eq!(
        std::fs::read(unrelated).unwrap(),
        b"Preserve existing files"
    );
}

#[test]
fn combined_job_snapshots_audio_mode_and_cache_then_finishes_a_usable_preview() {
    use crate::{
        audio::Audio,
        studio::stem_mix::{self, Stem, StemKind},
        studio_native::inference::SeparationMode,
    };
    let root = tempfile::tempdir().unwrap();
    let ctx = context();
    let mut studio = fixture(2);
    studio.runtime.base = root.path().join("fake-engine");
    studio.two_stems = false;
    let original = studio.source.as_ref().unwrap().audio.clone();
    let snapshot = original.clone();
    let previous_preview = studio.current.as_ref().unwrap().audio.clone();
    let previous_bank = studio.bank.clone().unwrap();
    let cache = studio.cache.as_ref().unwrap().path().to_owned();
    let (setup_started, setup_entered) = mpsc::channel();
    let (finish_setup, setup_release) = mpsc::channel();
    let (separation_started, separation_entered) = mpsc::channel();
    let (finish_separation, separation_release) = mpsc::channel();
    studio.separate_with(
        &ctx,
        move |progress, _| {
            setup_started.send(()).unwrap();
            setup_release.recv_timeout(Duration::from_secs(5)).unwrap();
            progress(1., "Fake setup verified");
            Ok(())
        },
        move |input, mode, work, progress, _| {
            assert!(std::ptr::eq(input, snapshot.as_ref()));
            assert!(matches!(mode, SeparationMode::FourStems));
            assert_eq!(work, cache);
            progress(1., "Fake inference aligned");
            separation_started.send(()).unwrap();
            separation_release
                .recv_timeout(Duration::from_secs(5))
                .unwrap();
            let estimates = [StemKind::Vocals, StemKind::Drums, StemKind::Bass]
                .into_iter()
                .map(|kind| Stem {
                    kind,
                    audio: Arc::new(
                        Audio::new(
                            input.samples.iter().map(|sample| sample * 0.2).collect(),
                            input.sample_rate,
                            input.channels,
                        )
                        .unwrap(),
                    ),
                })
                .collect();
            stem_mix::consistent(input, estimates, false)
        },
    );
    setup_entered.recv_timeout(Duration::from_secs(5)).unwrap();
    let job = studio.jobs.active;
    // Simulate a later selection; the queued request already owns its input and mode.
    let later = Arc::new(Audio::new(vec![0.08; 9600], 48000, 2).unwrap());
    studio.source.as_mut().unwrap().audio = later.clone();
    studio.two_stems = true;
    finish_setup.send(()).unwrap();
    separation_entered
        .recv_timeout(Duration::from_secs(5))
        .unwrap();
    studio.poll(&ctx);
    assert_eq!(studio.jobs.active, job);
    assert!(studio.progress > 0.4 && studio.progress < 1.);
    assert!(Arc::ptr_eq(
        &previous_preview,
        &studio.current.as_ref().unwrap().audio
    ));
    assert!(Arc::ptr_eq(&previous_bank, studio.bank.as_ref().unwrap()));
    finish_separation.send(()).unwrap();
    finish(&ctx, &mut studio);
    assert!(studio.error.is_none(), "{:?}", studio.error);
    assert_eq!(studio.progress, 1.);
    let preview = studio.current.as_ref().unwrap();
    assert_eq!(preview.label, "Neutral stem mix");
    assert!(Arc::ptr_eq(&original, &preview.before));
    assert_eq!(studio.bank.as_ref().unwrap().tracks.len(), 4);
    for (actual, expected) in preview.audio.samples.iter().zip(&original.samples) {
        assert!((actual - expected).abs() < 1e-6);
    }
    assert!(Arc::ptr_eq(&later, &studio.source.as_ref().unwrap().audio));
    assert!(studio.two_stems);
}

#[test]
fn native_cancel_stops_the_combined_job_after_setup_without_touching_previous_preview() {
    let root = tempfile::tempdir().unwrap();
    let ctx = context();
    let mut studio = fixture(2);
    studio.runtime.base = root.path().join("fake-engine");
    let preview = studio.current.clone().unwrap();
    let bank = studio.bank.clone().unwrap();
    let original_audio = preview.audio.clone();
    let original_bank = bank.clone();
    let original_source = studio.source.as_ref().unwrap().audio.clone();
    studio.mix.tracks[0].gain_db = -9.;
    studio.mix_dirty = true;
    let mix = serde_json::to_value(&studio.mix).unwrap();
    let cached = Arc::new(AtomicBool::new(false));
    let installed = cached.clone();
    let separated = Arc::new(AtomicBool::new(false));
    let continued = separated.clone();
    let (started, in_setup) = mpsc::channel();
    let (release, resume) = mpsc::channel();
    studio.separate_with(
        &ctx,
        move |_, _| {
            started.send(()).unwrap();
            resume.recv_timeout(Duration::from_secs(5)).unwrap();
            installed.store(true, Ordering::Relaxed);
            Ok(())
        },
        move |_, _, _, _, _| {
            continued.store(true, Ordering::Relaxed);
            anyhow::bail!("A cancelled job reached separation")
        },
    );
    in_setup.recv_timeout(Duration::from_secs(5)).unwrap();
    let job = studio.jobs.active;
    // A concurrent programmatic request must not reset progress/status or start setup.
    let status = studio.status.clone();
    studio.separate(&ctx);
    assert_eq!(studio.jobs.active, job);
    assert_eq!(studio.status, status);
    let output = settled(&ctx, &mut studio);
    assert!(button_enabled(&output, "Cancel"));
    click(&ctx, &mut studio, &output, "Cancel");
    assert!(studio.jobs.cancel.load(Ordering::Relaxed));
    assert_eq!(
        studio.jobs.active, job,
        "Setup and inference must share job identity"
    );
    release.send(()).unwrap();
    finish(&ctx, &mut studio);
    assert!(cached.load(Ordering::Relaxed));
    assert!(!separated.load(Ordering::Relaxed));
    assert!(studio.error.as_deref().unwrap().contains("Cancelled"));
    assert_eq!(serde_json::to_value(&studio.mix).unwrap(), mix);
    assert!(studio.mix_dirty);
    assert!(Arc::ptr_eq(
        &original_audio,
        &studio.current.as_ref().unwrap().audio
    ));
    assert!(Arc::ptr_eq(&original_bank, studio.bank.as_ref().unwrap()));
    assert!(Arc::ptr_eq(
        &original_source,
        &studio.source.as_ref().unwrap().audio
    ));
    assert_eq!(studio.current.as_ref().unwrap().label, "Synthetic preview");
}

#[test]
fn opening_studio_and_using_local_editors_never_creates_a_stem_runtime() {
    let root = tempfile::tempdir().unwrap();
    let ctx = context();
    let mut studio = fixture(0);
    studio.runtime.base = root.path().join("must-stay-absent");
    studio.repair.start_seconds = 0.01;
    studio.repair.end_seconds = 0.08;
    for page in [Page::Stems, Page::Voice, Page::Repair] {
        studio.page = page;
        settled(&ctx, &mut studio);
        assert!(!studio.busy());
        assert!(!studio.runtime.base.exists());
    }
    for action in [Studio::clean_voice, Studio::repair_audio] {
        action(&mut studio, &ctx);
        finish(&ctx, &mut studio);
        assert!(studio.error.is_none(), "{:?}", studio.error);
        assert!(!studio.runtime.base.exists());
    }
}

#[test]
fn importing_existing_stems_finishes_without_creating_a_stem_runtime() {
    use crate::studio::stem_mix::StemKind;
    let root = tempfile::tempdir().unwrap();
    let ctx = context();
    let mut studio = fixture(0);
    studio.runtime.base = root.path().join("must-stay-absent");
    let input = studio.source.as_ref().unwrap().audio.clone();
    let files = [StemKind::Vocals, StemKind::Instrumental]
        .into_iter()
        .map(|kind| {
            let path = root.path().join(format!("{}.wav", kind.name()));
            let file = std::fs::File::create(&path).unwrap();
            crate::wav_encoding::write_wav(&input, &file, "wav32f", false).unwrap();
            (kind, path)
        })
        .collect();
    studio.import_aligned_stems(files, &ctx);
    finish(&ctx, &mut studio);
    assert!(studio.error.is_none(), "{:?}", studio.error);
    assert_eq!(studio.bank.as_ref().unwrap().tracks.len(), 2);
    assert_eq!(studio.current.as_ref().unwrap().label, "Imported stem mix");
    assert!(!studio.runtime.base.exists());
}
