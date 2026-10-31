//! Counts the heap allocations of a frame once each scene has warmed up. The engine's own
//! frame loop should make none: queues, batches and caches keep their memory from frame to
//! frame. A failure prints how many there were and which scene made them.
//!
//! Only the test's own thread is counted, so tests running beside it do not disturb it.

use cg_bench::{frames, warm_up};
use std::alloc::{GlobalAlloc, Layout, System};
use std::cell::Cell;

struct Counting;

thread_local! {
    static ALLOCATIONS: Cell<u64> = const { Cell::new(0) };
    /// With `CG_ALLOC_TRACE` set: whether to record where allocations come from, and the
    /// guard that keeps recording from recording itself.
    static TRACING: Cell<bool> = const { Cell::new(false) };
    static IN_TRACE: Cell<bool> = const { Cell::new(false) };
    static TRACES: std::cell::RefCell<Vec<std::backtrace::Backtrace>> =
        const { std::cell::RefCell::new(Vec::new()) };
}

fn count() {
    ALLOCATIONS.with(|n| n.set(n.get() + 1));
    if TRACING.with(Cell::get) && !IN_TRACE.with(Cell::get) {
        IN_TRACE.with(|t| t.set(true));
        let trace = std::backtrace::Backtrace::force_capture();
        TRACES.with(|t| t.borrow_mut().push(trace));
        IN_TRACE.with(|t| t.set(false));
    }
}

/// Prints the engine functions that allocated most often while tracing.
fn report_traces(scene: &str) {
    let traces = TRACES.with(|t| core::mem::take(&mut *t.borrow_mut()));
    let mut places = std::collections::HashMap::<String, usize>::new();
    for trace in &traces {
        let text = trace.to_string();
        // The first frames in the engine's (or the game's) own code.
        let ours: Vec<&str> = text
            .lines()
            .map(str::trim)
            .filter(|l| {
                (l.contains("cg_") || l.contains("platformer::") || l.contains("cashgear"))
                    && !l.starts_with("at ")
                    && !l.contains("allocations::")
            })
            .take(3)
            .collect();
        *places.entry(ours.join(" <- ")).or_default() += 1;
    }
    let mut places: Vec<_> = places.into_iter().collect();
    places.sort_by_key(|(_, n)| core::cmp::Reverse(*n));
    println!("--- {scene}: {} allocations traced", traces.len());
    for (place, n) in places.iter().take(25) {
        println!("{n:6} {place}");
    }
}

// SAFETY: every call is passed on to the system allocator unchanged; the counter is a
// const-initialized thread local, which never allocates.
unsafe impl GlobalAlloc for Counting {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        count();
        // SAFETY: the caller's contract is the same as `System`'s.
        unsafe { System.alloc(layout) }
    }

    unsafe fn alloc_zeroed(&self, layout: Layout) -> *mut u8 {
        count();
        // SAFETY: as above.
        unsafe { System.alloc_zeroed(layout) }
    }

    unsafe fn realloc(&self, ptr: *mut u8, layout: Layout, new_size: usize) -> *mut u8 {
        count();
        // SAFETY: as above.
        unsafe { System.realloc(ptr, layout, new_size) }
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        // SAFETY: as above.
        unsafe { System.dealloc(ptr, layout) }
    }
}

#[global_allocator]
static GLOBAL: Counting = Counting;

/// Frames counted after the warm-up.
const COUNTED: u64 = 120;

/// The allocations made by `COUNTED` frames of a warmed-up scene, per frame, after `warm`
/// frames more than [`warm_up`]'s. With
/// `CG_ALLOC_TRACE=<frames>` set, that many more frames (2 by default) are traced and the
/// places that allocated printed (run with `--nocapture`).
fn per_frame(scene: &str, mut app: cashgear_engine::core::App, warm: usize) -> f64 {
    warm_up(&mut app);
    frames(&mut app, warm);
    let before = ALLOCATIONS.with(Cell::get);
    frames(&mut app, COUNTED as usize);
    let after = ALLOCATIONS.with(Cell::get);
    if let Ok(traced) = std::env::var("CG_ALLOC_TRACE") {
        TRACING.with(|t| t.set(true));
        frames(&mut app, traced.parse().unwrap_or(2));
        TRACING.with(|t| t.set(false));
        report_traces(scene);
    }
    (after - before) as f64 / COUNTED as f64
}

#[test]
fn sprite_frames_do_not_allocate() {
    let n = per_frame("10,000 sprites", cg_bench::sprites(10_000), 0);
    assert_eq!(n, 0.0, "{n} allocations a frame with 10,000 sprites");
}

#[test]
fn level_frames_do_not_allocate() {
    // Bodies walking into hash cells nobody touched yet grow the hash; ten seconds in, they
    // have been everywhere they go.
    let n = per_frame("the level", cg_bench::level(200), 600);
    assert_eq!(n, 0.0, "{n} allocations a frame in the level");
}

#[test]
fn menu_frames_do_not_allocate() {
    let n = per_frame("the menu", cg_bench::menu(40), 0);
    assert_eq!(n, 0.0, "{n} allocations a frame in the menu");
}

/// Lua lends the world to scripts through `mlua`'s scope, which costs a few allocations a
/// run, and what scripts make (tables from queries) is Lua's to collect. Counted, not
/// required to be zero.
#[test]
fn script_frames() {
    let n = per_frame("500 scripted bodies", cg_bench::scripts(500), 0);
    println!("{n} allocations a frame with 500 scripted bodies");
    assert!(n < 20.0, "{n} allocations a frame with 500 scripted bodies");
}

#[test]
fn platformer_frames() {
    // Past the level's start (its fade, the name card and the bot's pause), into play.
    // The Lua walkers' raycasts make tables, and the HUD's clock text changes.
    let n = per_frame("the platformer", cg_bench::platformer(), 240);
    println!("{n} allocations a frame in the platformer");
    assert!(n < 100.0, "{n} allocations a frame in the platformer");
}
