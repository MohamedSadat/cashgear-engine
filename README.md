# CashGear Engine

CashGear Engine is a Rust 2D game engine for Windows, Linux, macOS, the web (WASM) and Android, built with `winit`, `wgpu` and `bevy_ecs`. A sample platformer (`games/platformer`) proves it. The engine is built in 15 phases; see [docs/PLAN.md](docs/PLAN.md) for the plan and [docs/PROGRESS.md](docs/PROGRESS.md) for where it stands.

> **Status:** Phase 0. The workspace, CI and docs are in place, and every crate is still an empty skeleton.

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

## Development

The toolchain is pinned in `rust-toolchain.toml` (Rust 1.99.1, with the `wasm32-unknown-unknown` target). `rustup` installs it on first use.

```sh
cargo build --workspace
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
cargo fmt --all
cargo check-wasm   # alias: cargo check for wasm32-unknown-unknown, desktop tools excluded
```

CI runs all of these on Windows and Ubuntu for every push to `main` and every pull request.

### Portability rules

Engine and game crates must build for every target. Only `cg_platform` may use `#[cfg(target_os/arch)]`, and the root `clippy.toml` rejects calls that break on the web: `std::time::Instant::now`, `std::thread::spawn` and direct `std::fs` access. The full rules are in [docs/PLAN.md](docs/PLAN.md#portability-rules-apply-from-phase-1).

## License

Licensed under either of

- Apache License, Version 2.0 ([LICENSE-APACHE](LICENSE-APACHE))
- MIT license ([LICENSE-MIT](LICENSE-MIT))

at your option.

Unless you explicitly state otherwise, any contribution intentionally submitted for inclusion in the work by you, as defined in the Apache-2.0 license, shall be dual licensed as above, without any additional terms or conditions.
