//! Test-only, thread-local accounting of allocations AND releases in native sampling.
use std::{
    alloc::{GlobalAlloc, Layout, System},
    cell::Cell,
};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(super) struct Counts {
    pub allocations: usize,
    pub reallocations: usize,
    pub releases: usize,
}
thread_local! { static COUNTS: Cell<Option<Counts>> = const { Cell::new(None) }; }
struct Allocator;
fn record(event: u8) {
    let _ = COUNTS.try_with(|slot| {
        if let Some(mut counts) = slot.get() {
            match event {
                0 => counts.allocations += 1,
                1 => counts.reallocations += 1,
                _ => counts.releases += 1,
            }
            slot.set(Some(counts));
        }
    });
}
unsafe impl GlobalAlloc for Allocator {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        record(0);
        unsafe { System.alloc(layout) }
    }
    unsafe fn alloc_zeroed(&self, layout: Layout) -> *mut u8 {
        record(0);
        unsafe { System.alloc_zeroed(layout) }
    }
    unsafe fn realloc(&self, ptr: *mut u8, layout: Layout, size: usize) -> *mut u8 {
        record(1);
        unsafe { System.realloc(ptr, layout, size) }
    }
    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        record(2);
        unsafe { System.dealloc(ptr, layout) }
    }
}
#[global_allocator]
static ALLOCATOR: Allocator = Allocator;

pub(super) fn measure<T>(f: impl FnOnce() -> T) -> (T, Counts) {
    struct Reset;
    impl Drop for Reset {
        fn drop(&mut self) {
            let _ = COUNTS.try_with(|slot| slot.set(None));
        }
    }
    COUNTS.with(|slot| {
        assert!(slot.get().is_none());
        slot.set(Some(Counts::default()));
    });
    let _reset = Reset;
    let result = f();
    let counts = COUNTS.with(|slot| slot.replace(None).unwrap());
    (result, counts)
}
