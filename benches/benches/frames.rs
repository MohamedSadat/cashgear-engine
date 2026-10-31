//! One frame of each scene, timed with criterion: `cargo bench -p cg_bench`. Each scene
//! warms up first (its assets load, its caches fill), so the times are of steady frames.
//! CI runs them on shared machines and reports the times without failing on them.

use cashgear_engine::core::App;
use cg_bench::{frame, warm_up};
use criterion::{Criterion, criterion_group, criterion_main};

/// A scene's name and how to build it.
type Scene = (&'static str, fn() -> App);

fn frames(c: &mut Criterion) {
    let scenes: [Scene; 6] = [
        ("sprites/10000", || cg_bench::sprites(10_000)),
        ("sprites/1000", || cg_bench::sprites(1_000)),
        ("level/200 bodies", || cg_bench::level(200)),
        ("menu/40 buttons", || cg_bench::menu(40)),
        ("scripts/500", || cg_bench::scripts(500)),
        ("platformer/level 1", cg_bench::platformer),
    ];
    let mut group = c.benchmark_group("frame");
    for (name, scene) in scenes {
        let mut app = scene();
        warm_up(&mut app);
        group.bench_function(name, |b| b.iter(|| frame(&mut app)));
    }
    group.finish();
}

criterion_group!(benches, frames);
criterion_main!(benches);
