use mastercraft::{audio::Audio, studio::arrangement::*};
use std::{
    alloc::{GlobalAlloc, Layout, System},
    cell::Cell,
    sync::Arc,
};

thread_local! { static ALLOCATIONS: Cell<Option<usize>> = const { Cell::new(None) }; }
struct CountingAllocator;
fn record() {
    let _ = ALLOCATIONS.try_with(|count| {
        if let Some(value) = count.get() {
            count.set(Some(value + 1));
        }
    });
}
// Instrument only this test thread; other tests/worker setup remain uncounted.
unsafe impl GlobalAlloc for CountingAllocator {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        record();
        unsafe { System.alloc(layout) }
    }
    unsafe fn alloc_zeroed(&self, layout: Layout) -> *mut u8 {
        record();
        unsafe { System.alloc_zeroed(layout) }
    }
    unsafe fn realloc(&self, pointer: *mut u8, layout: Layout, size: usize) -> *mut u8 {
        record();
        unsafe { System.realloc(pointer, layout, size) }
    }
    unsafe fn dealloc(&self, pointer: *mut u8, layout: Layout) {
        unsafe { System.dealloc(pointer, layout) }
    }
}
#[global_allocator]
static ALLOCATOR: CountingAllocator = CountingAllocator;

fn begin() {
    ALLOCATIONS.with(|count| count.set(Some(0)));
}
fn end() -> usize {
    ALLOCATIONS.with(|count| count.replace(None).unwrap())
}

#[test]
fn prepared_handoffs_events_gain_ramps_and_pending_eq_do_not_allocate_in_sample_processing() {
    let assets = AssetLibrary::from([(
        "source".into(),
        Arc::new(Audio::new(vec![0.2; 10_000], 8000, 1).unwrap()),
    )]);
    let mut document = Arrangement::new(8000, 1);
    let mut lane = Lane::new("lane", "Part");
    for index in 0..100 {
        lane.clips.push(Clip {
            id: format!("clip_{index}"),
            name: None,
            source_id: "source".into(),
            start_frame: index * 50,
            source_start_frame: 0,
            end_frame: index * 50 + 100,
            gain_db: 0.,
            fade_in_frames: 0,
            fade_out_frames: 0,
        });
    }
    document.lanes.push(lane);
    let mut effect = ScopedEq::new("eq", EqScope::Mix {});
    for band in &mut effect.bands {
        band.on = false;
    }
    document.eq.push(effect.clone());
    let options = RenderOptions {
        protect_peaks: false,
        ..Default::default()
    };
    let mut player = ArrangementPlayer::new(&document, &assets, &options).unwrap();
    for gain in [-3., -6., -9.] {
        effect.output_gain_db = gain;
        player.set_eq(&effect).unwrap();
    }
    let mut block = [0.; 1000];
    begin();
    player
        .set_lane_controls("lane", -1., 0., false, false)
        .unwrap();
    player.set_clip_gain("clip_0", -1.).unwrap();
    player.set_output_gain_db(-1.).unwrap();
    player.process_block(&mut block).unwrap();
    let processing = end();
    assert_eq!(processing, 0, "Sample processing allocated");
    effect.output_gain_db = -12.;
    let prepared_eq = PreparedEqUpdate::new(&effect, 8000).unwrap();
    begin();
    let retired_eq = player.apply_prepared_eq(prepared_eq).unwrap();
    player.process_block(&mut block).unwrap();
    let eq_processing = end();
    assert_eq!(eq_processing, 0, "Prepared EQ adoption allocated");
    drop(retired_eq);

    let prepared = player.prepare_seek(2000, &|| false).unwrap();
    begin();
    let retired_seek = player.apply_prepared_seek(prepared).unwrap();
    let adopting_seek = end();
    assert_eq!(adopting_seek, 0, "Prepared seek adoption allocated");
    drop(retired_seek);

    let mut next = ArrangementPlayer::new(&document, &assets, &options).unwrap();
    next.seek(player.position(), &|| false).unwrap();
    let boxed = Box::new(next);
    let mut retired = Vec::with_capacity(4);
    begin();
    if let Some(old) = player.replace_prepared_boxed(boxed).unwrap() {
        retired.push(old);
    }
    for _ in 0..400 {
        player.next_frame().unwrap();
        if let Some(old) = player.take_retired().unwrap() {
            retired.push(old);
        }
    }
    let replacement = end();
    assert_eq!(replacement, 0, "Prepared replacement processing allocated");
    assert_eq!(
        retired.len(),
        1,
        "Completed old engine must be handed out for off-thread disposal"
    );
}
