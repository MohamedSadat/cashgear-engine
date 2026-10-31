# Profiling and performance

## The profiler

The engine marks its schedules, its systems and the renderer's steps with scopes from the [`profiling`](https://docs.rs/profiling) crate. They cost nothing until you build with a backend:

```sh
cargo run --features cashgear_engine/profile -- --profile
```

`profile` records with [puffin](https://github.com/EmbarkStudios/puffin). `--profile` starts recording at once and opens the debug overlay's **Profiler** window (also under F12, "Profiler"): the recent frames as bars (click one to keep it), a flame graph of the chosen frame (hover for times, click a scope to zoom in), and a table of scopes by their own time. It works on desktop, in browsers and on phones. wgpu and egui mark their own work with the same crate, so their scopes appear too.

For [Tracy](https://github.com/wolfpld/tracy) on desktop, build with `cashgear_engine/profile-tracy` instead and connect the Tracy profiler while the game runs. The two backends cannot be on at once.

Mark your own systems the same way:

```rust,ignore
use cashgear_engine::profiling;

#[profiling::function]
fn think(/* ... */) {
    profiling::scope!("pathfinding");
    // ...
}
```

## What frames cost

`cargo bench -p cg_bench` times one frame of several scenes without a window; CI posts the table in each run's summary. On a desktop with a GTX 1650 (release build):

| Scene | Frame |
| --- | ---: |
| 10,000 moving sprites (entities, interpolated, three quarters culled) | 1.27 ms |
| A Tiled level with 200 walking bodies and three particle emitters | 0.55 ms |
| A menu of 40 buttons with text | 0.05 ms |
| 500 Lua-scripted bodies | 2.7 ms |
| The sample platformer, played by its bot | 0.09 ms |

These are CPU times for the engine's work; drawing on the GPU comes on top and overlaps with the next frame. The bunnymark example draws 10,000 sprites in about 1.3 ms a frame (some 770 frames a second, uncapped) in a release build on the same machine.

## No allocations in steady frames

The engine's frame loop keeps its memory from frame to frame: sprite queues and batches, the physics' spatial hash and overlap lists, the UI layout's lists. A test (`cargo test -p cg_bench --test allocations`) counts heap allocations per frame with a counting allocator and requires none for sprite, level and menu scenes. Lua scripts allocate what they create (tables from queries), which Lua collects.

To find what allocates in your game, run that test's approach on your own scene: `CG_ALLOC_TRACE=60` prints where the allocations of 60 frames come from.

## Tips

- Gameplay in `FixedUpdate`, effects in `Update`: fixed steps can run twice in a slow frame.
- Many sprites of one texture batch into one draw call; atlases help. Use `Layer` rather than many textures interleaved.
- Change text only when its value changes: assigning a `Text` reshapes it.
- Reuse `String`s and `Vec`s in `Local`s for per-frame work, as the template's HUD does.
- Profile release builds (`--release`); debug builds of the engine are optimised, but your own code is not.
