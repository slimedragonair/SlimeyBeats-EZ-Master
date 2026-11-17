use super::*;
use crate::gui::{
    arrangement_controls::{CONTROL_BUNDLE_CAPACITY, ControlBundle, ControlSlot},
    native_allocation_probe::{Counts, measure},
};

fn sample_without_heap_work(source: &mut ABSource, frames: usize) {
    let (_, counts) = measure(|| advance(source, frames));
    assert_eq!(
        counts,
        Counts::default(),
        "Native callback allocated or released heap storage"
    );
}

#[test]
fn native_coalesced_ids_and_rapid_eq_transitions_do_not_allocate_or_free_on_callback() {
    let (mut document, assets, options, before) = data(48_000, 2);
    let (player, mut source) = load(&document, &assets, &options, before);
    advance(&mut source, 256);
    for gain in [-1., -3., -5.] {
        player
            .set_arrangement_lane_controls("voice_lane", gain, 0.1, false, false)
            .unwrap();
    }
    player.set_arrangement_clip_gain("voice_clip", -2.).unwrap();
    document.eq[0].bands[2].on = true;
    document.eq[0].bands[2].freq = 1000.;
    for gain in [-2., -4., -6.] {
        document.eq[0].bands[2].gain = gain;
        player.set_arrangement_eq(&document.eq[0]).unwrap();
    }
    let deadline = Instant::now() + Duration::from_secs(3);
    loop {
        sample_without_heap_work(&mut source, 256);
        let active = source.arrangement.as_ref().unwrap().processor.arrangement();
        if active.lanes[0].gain_db == -5.
            && active.lanes[0].clips[0].gain_db == -2.
            && active.eq[0].bands[2].gain == -6.
        {
            break;
        }
        assert!(Instant::now() < deadline);
        std::thread::sleep(Duration::from_millis(1));
    }
    // Each update arrives before the 20 ms fade finishes; pending parameters
    // and old document/control metadata must remain free of callback disposal.
    for gain in [-8., -10., -12.] {
        document.eq[0].bands[2].gain = gain;
        player.set_arrangement_eq(&document.eq[0]).unwrap();
        sample_without_heap_work(&mut source, 128);
    }
    sample_without_heap_work(&mut source, 2400);
    assert!(!player.controls.failure.load(Ordering::Relaxed));
}

#[test]
fn bounded_control_bundles_replenish_and_retirement_backpressure_preserves_ownership() {
    let (mut document, assets, options, before) = data(8000, 1);
    // More controls than one bundle, without imposing an arrangement lane cap.
    for index in 0..CONTROL_BUNDLE_CAPACITY + 3 {
        let mut lane = document.lanes[0].clone();
        lane.id = format!("extra_lane_{index}");
        lane.clips[0].id = format!("extra_clip_{index}");
        document.lanes.push(lane);
    }
    let (mut player, mut source) = load(&document, &assets, &options, before);
    let shared = source.arrangement.as_ref().unwrap().updates.0.clone();
    shared.hold_retirement.store(true, Ordering::Release);
    shared.wake.notify_one();
    let deadline = Instant::now() + Duration::from_secs(3);
    while !shared.retirement_held.load(Ordering::Acquire) {
        assert!(Instant::now() < deadline);
        std::thread::sleep(Duration::from_millis(1));
    }
    // Hold the worker off the disposal receiver and fill its fixed retirement queue.
    let mut filled = 0;
    loop {
        let bundle = Box::new(ControlBundle {
            epoch: 0,
            cursor: 0,
            slots: vec![],
        });
        match shared.retired.try_send(Retired::Controls(bundle)) {
            Ok(()) => filled += 1,
            Err(_) => break,
        }
    }
    assert_eq!(filled, 8);
    for lane in &mut document.lanes {
        lane.gain_db = -3.;
    }
    player
        .set_arrangement(&document, &assets, &options)
        .unwrap();
    // The callback can apply the prepared bundle, then must retain its complete
    // container when retirement is full instead of dropping it or blocking.
    sample_without_heap_work(&mut source, 64 * (CONTROL_BUNDLE_CAPACITY + 2));
    assert!(source.arrangement.as_ref().unwrap().deferred.is_some());
    shared.hold_retirement.store(false, Ordering::Release);
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        sample_without_heap_work(&mut source, 128);
        let active = source.arrangement.as_ref().unwrap().processor.arrangement();
        if !player.arrangement_preparing() && active.lanes.iter().all(|lane| lane.gain_db == -3.) {
            break;
        }
        assert!(Instant::now() < deadline);
        std::thread::sleep(Duration::from_millis(1));
    }
    assert!(!player.controls.failure.load(Ordering::Relaxed));
}

#[test]
fn stale_bundles_and_unknown_targets_are_retired_without_callback_release() {
    let (document, assets, options, before) = data(8000, 1);
    let (player, mut source) = load(&document, &assets, &options, before);
    let path = source.arrangement.as_mut().unwrap();
    let shared = path.updates.0.clone();
    player.set_arrangement_clip_gain("voice_clip", -4.).unwrap();
    // Taking an offered bundle here is setup outside the measured callback.
    path.controls = shared.state.lock().unwrap().control_ready.take();
    assert!(path.controls.is_some());
    shared.epoch.fetch_add(1, Ordering::AcqRel);
    sample_without_heap_work(&mut source, 128);
    assert!(source.arrangement.as_ref().unwrap().controls.is_none());
    let slots = vec![
        (
            "lane:absent",
            FastUpdate::Lane {
                id: "absent".into(),
                gain: -3.,
                pan: 0.,
                mute: false,
                solo: false,
            },
        ),
        (
            "clip:absent",
            FastUpdate::Clip {
                id: "absent".into(),
                gain: -3.,
            },
        ),
        (
            "eq:absent",
            FastUpdate::Eq(
                crate::studio::arrangement::PreparedEqUpdate::new(
                    &ScopedEq::new("absent", EqScope::Mix {}),
                    8000,
                )
                .unwrap(),
            ),
        ),
    ]
    .into_iter()
    .map(|(key, update)| ControlSlot {
        key: key.into(),
        update,
        retired_eq: None,
    })
    .collect();
    source.arrangement.as_mut().unwrap().controls = Some(Box::new(ControlBundle {
        epoch: shared.epoch.load(Ordering::Acquire),
        cursor: 0,
        slots,
    }));
    sample_without_heap_work(&mut source, 256);
    assert_eq!(
        source
            .arrangement
            .as_ref()
            .unwrap()
            .processor
            .arrangement()
            .lanes[0]
            .clips[0]
            .gain_db,
        0.
    );
    assert!(!player.controls.failure.load(Ordering::Relaxed));
}
