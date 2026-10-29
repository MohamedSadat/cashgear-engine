# CashGear Engine

CashGear Engine is a Rust 2D game engine for Windows, Linux, macOS, the web (WASM) and Android, built with `winit`, `wgpu` and `bevy_ecs`. A sample platformer (`games/platformer`) proves it. The engine is built in 15 phases; see [docs/PLAN.md](docs/PLAN.md) for the plan and [docs/PROGRESS.md](docs/PROGRESS.md) for where it stands.

> **Status:** Phase 3. `cg_core` (app, plugins, fixed-timestep loop, `Time`, schedules), `cg_platform` (window, input, actions, lifecycle, logging), `cg_render` (`wgpu` sprites, camera, virtual resolutions, debug shapes, culling) and `cg_world` (transforms and hierarchy, interpolation, a game-state stack, RON prefabs) work on desktop and web. Assets and Tiled maps arrive in Phase 4.

## Workspace layout

| Path | Responsibility | Phase |
| --- | --- | --- |
| `crates/cashgear_engine` | Facade crate: re-exports every subsystem, `prelude` | 0 |
| `crates/cg_core` | App builder, plugins, fixed-timestep loop, `Time`, events, logging | 1 |
| `crates/cg_platform` | `winit` glue, input, per-target asset IO, storage, lifecycle | 1 |
| `crates/cg_render` | `wgpu` context, sprite batcher, camera, textures, debug shapes | 2 |
| `crates/cg_world` | `bevy_ecs` integration, transforms, schedules, game states, prefabs | 3 |
| `crates/cg_assets` | Handles, async loaders, atlases, hot reload | 4 |
| `crates/cg_tiled` | Tiled maps, chunked tile rendering, object spawning | 4 |
| `crates/cg_physics` | AABB/tile collision, layers, sensors, raycasts | 5 |
| `crates/cg_anim` | Sprite animation, state machines, tweens, particles | 6 |
| `crates/cg_audio` | `kira` wrapper, buses, positional pan | 7 |
| `crates/cg_ui` | Text, game UI, touch controls, `egui` debug overlay | 8 |
| `crates/cg_script` | Lua runtime and bindings (facade feature `script`) | 12 |
| `games/platformer` | Sample game | 9 |
| `tools/cg_pack` | CLI: atlas packing, asset bundling | 4 |
| `tools/cg_editor` | Desktop scene editor | 13 |
| `examples/` | One runnable example per feature | from 1 |

## Examples

Each example is its own binary crate under `examples/`, with an `index.html` for the web build.

| Example | Shows |
| --- | --- |
| `hello_window` | The window, the game loop, input logging and an interpolated player box |
| `bunnymark` | Thousands of bouncing sprites, scaling modes, camera controls and debug shapes; `--ecs` makes each bunny an entity |
| `ecs_sprites` | Prefabs, a transform hierarchy, interpolation, visibility, culling and a Menu / Playing / Paused state stack |

```sh
cargo run -p hello_window                  # desktop
cargo run -p hello_window -- --frames 120  # run 120 frames, then exit (every example)
cargo run -p bunnymark --release -- --bunnies 50000 --no-vsync
cargo run -p ecs_sprites                   # add -- --demo to cycle the states by itself

cargo install trunk --locked               # once
cd examples/hello_window && trunk serve    # web: open http://127.0.0.1:8080
cd examples/bunnymark && trunk serve --port 8081   # flags go in the URL: ?bunnies=20000&backend=gl
cd examples/ecs_sprites && trunk serve --port 8082
```

Every example takes `--frames N`, `--no-vsync` and `--backend <vulkan|dx12|metal|gl|webgpu>`; on the web they go in the page URL as `?frames=N&no-vsync&backend=gl`.

## Development

The toolchain is pinned in `rust-toolchain.toml` (Rust 1.99.1, with the `wasm32-unknown-unknown` target). `rustup` installs it on first use.

```sh
cargo build --workspace
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
cargo fmt --all
cargo check-wasm   # alias: cargo check for wasm32-unknown-unknown, desktop tools excluded
cargo clippy-wasm  # alias: the same, linted with warnings denied (CI runs this)
```

CI runs all of these on Windows and Ubuntu for every push to `main` and every pull request.

`cg_render`'s reference-image tests render scenes offscreen and compare them with the PNGs in `crates/cg_render/tests/golden/`. Without a GPU they use a software adapter (WARP on Windows, Mesa lavapipe on Linux) or skip with a note; `CG_REQUIRE_GPU=1` makes a missing adapter a failure, as in CI. After an intended visual change, regenerate and review the references:

```sh
CG_UPDATE_GOLDEN=1 cargo test -p cg_render --test golden
```

### Portability rules

Engine and game crates must build for every target. Only `cg_platform` may use `#[cfg(target_os/arch)]`, and the root `clippy.toml` rejects calls that break on the web: `std::time::Instant::now`, `std::thread::spawn` and direct `std::fs` access. Game and engine code read time from the `Time` resource. The full rules are in [docs/PLAN.md](docs/PLAN.md#portability-rules-apply-from-phase-1).

## License

Licensed under either of

- Apache License, Version 2.0 ([LICENSE-APACHE](LICENSE-APACHE))
- MIT license ([LICENSE-MIT](LICENSE-MIT))

at your option.

Unless you explicitly state otherwise, any contribution intentionally submitted for inclusion in the work by you, as defined in the Apache-2.0 license, shall be dual licensed as above, without any additional terms or conditions.
