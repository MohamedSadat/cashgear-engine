# CashGear Engine

CashGear Engine is a Rust 2D game engine for Windows, Linux, macOS, the web (WASM) and Android, built with `winit`, `wgpu` and `bevy_ecs`. A sample platformer (`games/platformer`) proves it. The engine is built in 15 phases; see [docs/PLAN.md](docs/PLAN.md) for the plan and [docs/PROGRESS.md](docs/PROGRESS.md) for where it stands.

> **Status:** Phase 9. `cg_core` (app, plugins, fixed-timestep loop, `Time`, schedules), `cg_platform` (window, input, actions, lifecycle, logging, asset IO, save storage), `cg_render` (`wgpu` sprites with hit flashes, camera, virtual resolutions, debug shapes, culling, image and atlas assets), `cg_world` (transforms and hierarchy, interpolation, a game-state stack, RON prefabs), `cg_assets` (handles, background loading, loading states, hot reload), `cg_tiled` (Tiled maps with parallax, chunked tiles, tile animations and object prefabs), `cg_physics` (kinematic bodies with slopes, one-way platforms and moving platforms, tile and entity colliders, sensors, layers, raycasts) and `cg_anim` (Aseprite sprite sheets, animation state machines and events, tweens, CPU particles, camera follow, shake and fades) `cg_audio` (OGG and WAV sounds, streamed music with crossfades, buses with saved volume settings, positional sound) and `cg_ui` (shaped text in TTF, OTF and bitmap fonts with Arabic and right-to-left layouts, nodes with anchors, rows and columns, buttons, sliders and progress bars, keyboard, gamepad, mouse and touch navigation, on-screen touch controls, localization, and an egui debug overlay) work on desktop and web, and the platformer sample game (`games/platformer`) plays through three levels on both.

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
| `crates/cg_anim` | Sprite animation, state machines, tweens, particles, camera effects | 6 |
| `crates/cg_audio` | `kira` mixer, sounds and streamed music, buses, positional pan | 7 |
| `crates/cg_ui` | Text, game UI, touch controls, `egui` debug overlay | 8 |
| `crates/cg_script` | Lua runtime and bindings (facade feature `script`) | 12 |
| `games/platformer` | Sample game | 9 |
| `tools/cg_pack` | CLI: atlas packing, asset bundling | 4 |
| `tools/cg_editor` | Desktop scene editor | 13 |
| `examples/` | One runnable example per feature | from 1 |

## The sample game

`games/platformer` is a short platformer in three levels made in Tiled with Kenney's Pixel Platformer art: run, jump, wall-jump and stomp past walkers, spikers, hoppers and bats, collect coins, raise checkpoint flags, ride moving platforms and springs, and reach the door. Three hearts per level, a title screen, level select, settings with rebindable controls, pause, level-complete and game-over screens, touch controls, English and Arabic, and saved progress.

```sh
cargo run -p platformer                    # keyboard (arrows or A/D, Space or Up to jump, Down + jump drops, Esc pauses) or gamepad
cargo run -p platformer -- --demo          # a bot plays the whole game and quits (CI runs this)
cargo run -p platformer -- --level 2       # straight into a level
cd games/platformer && trunk serve --port 8088   # web: open http://127.0.0.1:8088
```

`cargo test -p platformer -- --ignored capture_screens` renders the bot's run to PNGs in `target/platformer-captures/`.

## Examples

Each example is its own binary crate under `examples/`, with an `index.html` for the web build.

| Example | Shows |
| --- | --- |
| `hello_window` | The window, the game loop, input logging and an interpolated player box |
| `bunnymark` | Thousands of bouncing sprites, scaling modes, camera controls and debug shapes; `--ecs` makes each bunny an entity |
| `ecs_sprites` | Prefabs and images loaded as assets, a transform hierarchy, interpolation, visibility, culling and a Loading / Menu / Playing / Paused state stack |
| `tiled_map` | A Tiled level with Kenney art: a loading bar, parallax, chunked tiles, object prefabs, an atlas packed by `cg_pack`, and live editing |
| `collision` | A box runs a Tiled test course: 45° and 22.5° slopes, one-way platforms, spikes, crates, coin sensors, a dash and a slam that pass through nothing; F1 draws the colliders, `--demo` lets a bot run it |
| `animation` | Kenney's alien from an Aseprite sheet, run by a state machine: footstep dust, squash and stretch, hit flashes, spinning coins, a waving Tiled flag, a camera that follows, shakes and zooms, and a fade into a gallery of 10,000 particles; `--demo` lets a bot play |
| `audio` | The alien's level with sound: footsteps, jumps and landings, coins rising in pitch, a campfire and a fountain that pan as you pass, music that crossfades between two areas, and a volume panel saved between runs; `--demo` lets a bot play |
| `ui` | A main menu, settings, a HUD and a pause menu in pixel-art UI, navigable by keyboard, gamepad, mouse and touch, in English and Arabic (right to left), with on-screen touch controls and the F12 debug overlay; `--demo` lets a bot click through |

```sh
cargo run -p hello_window                  # desktop
cargo run -p hello_window -- --frames 120  # run 120 frames, then exit (every example)
cargo run -p bunnymark --release -- --bunnies 50000 --no-vsync
cargo run -p ecs_sprites                   # add -- --demo to cycle the states by itself
cargo run -p tiled_map                     # edit examples/tiled_map/assets/maps/level.tmx in Tiled while it runs
cargo run -p collision                     # add -- --demo for the bot, --physics-debug to draw colliders
cargo run -p animation                     # Z zooms, P opens the particle gallery; -- --demo for the bot
cargo run -p audio                         # 1-4 pick a bus, -/= change it, M mutes; -- --demo for the bot
cargo run -p ui                            # F12: debug overlay; -- --touch shows the touch controls, --demo for the bot

cargo install trunk --locked               # once
cd examples/hello_window && trunk serve    # web: open http://127.0.0.1:8080
cd examples/bunnymark && trunk serve --port 8081   # flags go in the URL: ?bunnies=20000&backend=gl
cd examples/ecs_sprites && trunk serve --port 8082
cd examples/tiled_map && trunk serve --port 8083
cd examples/collision && trunk serve --port 8084
cd examples/animation && trunk serve --port 8085
cd examples/audio && trunk serve --port 8086       # sound starts after a key press or click
cd examples/ui && trunk serve --port 8087          # on a phone the touch controls show by themselves
```

Every example takes `--frames N`, `--no-vsync` and `--backend <vulkan|dx12|metal|gl|webgpu>`; on the web they go in the page URL as `?frames=N&no-vsync&backend=gl`. The asset flags are `--hot-reload` / `--no-hot-reload` (on by default in debug desktop builds), `--asset-root <path>` and `--asset-delay <seconds>` (slows loading down to try loading screens). `--physics-debug` starts with colliders drawn (F1 toggles them), `--no-audio` turns the sound off, `--touch` / `--no-touch` show or hide the on-screen touch controls, and `--overlay` opens the debug overlay (F12 toggles it).

### Assets

A game's files live in its own `assets/` folder and are loaded by path: `server.load("maps/level1.tmx")`. Under `cargo run` that folder is found through `CARGO_MANIFEST_DIR`; a shipped executable looks for `assets` next to itself, and `CG_ASSET_ROOT` overrides both. On the web, `trunk` copies the folder next to `index.html` (`<link data-trunk rel="copy-dir" href="assets" />`) and the engine fetches from there.

Sprite animations come from Aseprite: export a sprite sheet with its JSON data ("Hash" or "Array", with tags) next to the image and load the `.json` as a `SpriteSheet`; each tag becomes a clip, and a tag's user data can carry events (`0:footstep, 3:footstep`). Animation state machines are `.anim.ron` files. Both hot-reload like any asset.

Sounds are OGG Vorbis or WAV files, mono or stereo, loaded as `Sound` assets. Files up to 10 seconds are decoded when they load; longer ones (music) stay compressed and are streamed while they play.

Texture atlases are packed ahead of time and committed:

```sh
cargo run -p cg_pack -- atlas examples/tiled_map/art/characters examples/tiled_map/assets/atlases/characters.atlas.ron
```

`--check` fails when the committed files are out of date (CI runs it).

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

`cg_render`'s and `cg_ui`'s reference-image tests render scenes offscreen and compare them with the PNGs in `crates/cg_render/tests/golden/` and `crates/cg_ui/tests/golden/`. Without a GPU they use a software adapter (WARP on Windows, Mesa lavapipe on Linux) or skip with a note; `CG_REQUIRE_GPU=1` makes a missing adapter a failure, as in CI. After an intended visual change, regenerate and review the references:

```sh
CG_UPDATE_GOLDEN=1 cargo test -p cg_render --test golden
CG_UPDATE_GOLDEN=1 cargo test -p cg_ui --test golden
```

`cargo test -p ui -- --ignored capture_screens` renders each screen of the `ui` example's bot run to PNGs in `target/ui-captures/`, for looking at the UI without a window.

### Portability rules

Engine and game crates must build for every target. Only `cg_platform` may use `#[cfg(target_os/arch)]`, and the root `clippy.toml` rejects calls that break on the web: `std::time::Instant::now`, `std::thread::spawn` and direct `std::fs` access. Game and engine code read time from the `Time` resource. The full rules are in [docs/PLAN.md](docs/PLAN.md#portability-rules-apply-from-phase-1).

## License

Licensed under either of

- Apache License, Version 2.0 ([LICENSE-APACHE](LICENSE-APACHE))
- MIT license ([LICENSE-MIT](LICENSE-MIT))

at your option.

Unless you explicitly state otherwise, any contribution intentionally submitted for inclusion in the work by you, as defined in the Apache-2.0 license, shall be dual licensed as above, without any additional terms or conditions.
