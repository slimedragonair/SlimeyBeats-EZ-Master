use super::*;
use std::sync::mpsc;
use std::time::Duration;

fn completed(receiver: &std::sync::mpsc::Receiver<Event>) -> Arc<TrackResult> {
    let mut ready = None;
    loop {
        match receiver
            .recv_timeout(Duration::from_secs(15))
            .expect("Worker must finish")
        {
            Event::Ready(_, result) => ready = Some(result),
            Event::Failed(_, error) => panic!("Worker failed: {error}"),
            Event::Finished => return ready.expect("Worker must return a result"),
            Event::Progress(..) => {}
            Event::Suggestion(..) => panic!("Unexpected suggestion event"),
            Event::EqRendered(_, _, result) => return result,
            Event::EqRenderFailed(_, _, error) => panic!("EQ render failed: {error}"),
            Event::SuggestionUnavailable(reason) => {
                panic!("Unexpected suggestion failure: {reason}")
            }
        }
    }
}

#[test]
fn mastering_and_eq_do_not_export_until_explicit_export() {
    let dir = tempfile::tempdir().unwrap();
    let input = dir.path().join("source.wav");
    let mut writer = hound::WavWriter::create(
        &input,
        hound::WavSpec {
            channels: 2,
            sample_rate: 48000,
            bits_per_sample: 32,
            sample_format: hound::SampleFormat::Float,
        },
    )
    .unwrap();
    for i in 0..48000 {
        let value = 0.1 * (i as f32 * std::f32::consts::TAU * 1000. / 48000.).sin();
        writer.write_sample(value).unwrap();
        writer.write_sample(value).unwrap();
    }
    writer.finalize().unwrap();
    let mut settings = Settings {
        out_dir: dir.path().join("exports"),
        safety: false,
        ..Default::default()
    };
    let (tx, rx) = mpsc::channel();
    let ctx = egui::Context::default();
    let cancel = Arc::new(AtomicBool::new(false));
    batch(
        vec![super::BatchInput {
            index: 0,
            path: input.clone(),
            audio: None,
        }],
        settings.clone(),
        false,
        tx.clone(),
        cancel.clone(),
        ctx.clone(),
        crate::paths::app_root(),
    );
    let master = completed(&rx);
    assert!(master.after.is_some());
    assert!(master.raw.is_some());
    assert!(master.report.as_ref().unwrap().exports.is_empty());
    assert!(
        !settings.out_dir.exists(),
        "Mastering must not create the export directory"
    );
    settings.manual_eq[4].gain = 2.;
    super::super::eq_jobs::render_revision(0, 1, master, settings.clone(), tx.clone(), ctx.clone());
    let tweaked = completed(&rx);
    assert!(tweaked.report.as_ref().unwrap().exports.is_empty());
    assert!(!settings.out_dir.exists());
    super::super::eq_jobs::export_selected(0, input, tweaked, settings.clone(), tx, cancel, ctx);
    let saved = completed(&rx);
    let report = saved.report.as_ref().unwrap();
    assert_eq!(report.exports.len(), 1);
    assert!(Path::new(report.exports[0]["path"].as_str().unwrap()).is_file());
    assert_eq!(
        std::fs::read_dir(&settings.out_dir).unwrap().count(),
        2,
        "Export writes audio and report together"
    );
}

#[test]
fn suggestion_worker_carries_all_proposal_notes_and_measures_the_immutable_pre_eq_master() {
    let profile = crate::profiles::resolve("neutral", Path::new(".")).unwrap();
    let tone = |hz: f64| {
        Arc::new(
            crate::audio::Audio::new(
                (0..96000)
                    .map(|i| (0.1 * (std::f64::consts::TAU * hz * i as f64 / 48000.).sin()) as f32)
                    .collect(),
                48000,
                1,
            )
            .unwrap(),
        )
    };
    let before = tone(1000.);
    let raw = tone(8000.);
    let analysis = crate::analysis::analyze(&before, &profile).unwrap();
    let measured = crate::analysis::analyze(&raw, &profile).unwrap();
    let expected = crate::dsp::eq_suggestion::proposal(&measured, &profile).unwrap();
    let mut settings = Settings::default();
    settings.manual_eq[2].gain = 2.75;
    let pending_eq = settings.manual_eq.clone();
    let result = Arc::new(TrackResult {
        before,
        raw: Some(raw.clone()),
        raw_analysis: None,
        eq_parameters: None,
        after: None,
        analysis,
        report: None,
    });
    let (tx, rx) = mpsc::channel();
    suggest_eq(
        0,
        result,
        settings.clone(),
        tx,
        Arc::new(AtomicBool::new(false)),
        egui::Context::default(),
    );
    match rx.recv_timeout(Duration::from_secs(15)).unwrap() {
        Event::Suggestion(0, proposal) => {
            assert!(Arc::ptr_eq(&proposal.source, &raw));
            assert_eq!(proposal.profile, settings.profile);
            assert_eq!(proposal.bands, expected.bands);
            assert!(proposal.note.contains("automatic master"));
            assert!(!expected.notes.is_empty());
            for note in expected.notes {
                assert!(proposal.note.contains(&note), "Missing note: {note}");
            }
        }
        _ => panic!("Worker must produce an unapplied proposal"),
    }
    assert!(matches!(
        rx.recv_timeout(Duration::from_secs(15)).unwrap(),
        Event::Finished
    ));
    assert_eq!(settings.manual_eq, pending_eq);
}
