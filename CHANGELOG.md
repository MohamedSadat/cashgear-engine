# Changelog

All notable changes to CashGear Engine are recorded here, one section per phase.
The format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/).

## Phase 2 — Renderer core

### Added

- `cg_render`: `RenderPlugin` on `wgpu` 30, with Vulkan, Direct3D 12, Metal and GL on desktop, and WebGPU with a WebGL2 fallback on the web. GPU setup runs async (a few frames on the web); vsync is on by default.
- `cg_render`: surface handling. Resizes and outdated surfaces reconfigure; a lost surface is recreated; the surface is dropped on suspend and recreated on resume; a lost device is set up again.
- `cg_render`: `Textures` with `TextureId` handles (and the always-present `TextureId::WHITE`), `Image` (RGBA8, PNG decode and encode), and per-texture `Filter::Nearest` (default) or `Filter::Linear`. Mipmaps are off.
- `cg_render`: `Sprites`, a per-frame queue of `Sprite`s (texture, position, rotation, scale, size, source rect, pivot, tint, flips, layer). Instanced quads are sorted by layer then texture, with one draw call per batch.
- `cg_render`: the `Camera2D` resource (position, zoom, rotation, clear and bar colours, pixel snapping) with `world_to_screen`, `screen_to_world` and `visible_rect`.
- `cg_render`: `ScalingMode::{WindowSize, Stretch, Letterbox, PixelPerfect}`. The default is pixel-perfect 640×360: the scene is drawn at the virtual resolution, then scaled up by a whole number.
- `cg_render`: `DebugShapes` (lines, rectangle and circle outlines and fills) drawn at window resolution with widths in logical pixels.
- `cg_render`: `RenderStats` (draw calls, sprites, batches, shapes, average frame time and fps, adapter and backend).
- `cg_render`: `OffscreenRenderer`, which renders to an `Image` through the window's code path, and reference-image tests in `tests/golden` that run on WARP and lavapipe in CI.
- `cg_core`: the `Render` schedule between `PostUpdate` and `Last`, and the `Rect` type.
- `cg_platform`: the `WindowHandle` resource, `tasks::spawn_local` for async work without threads, and `Args`, which reads launch flags from the command line or the page's query string.
- `cashgear_engine`: `DefaultPlugins` includes `RenderPlugin` and applies `--no-vsync` and `--backend`; the prelude covers rendering.
- `examples/bunnymark`: 10,000+ bouncing, interpolated sprites in four textures. The pointer adds bunnies, keys switch scaling modes and move the camera, and debug shapes toggle.
- CI: Mesa lavapipe on Ubuntu, `CG_REQUIRE_GPU=1` for the tests, a `bunnymark --frames 120` run, and uploads of failed reference images.

### Changed

- `examples/hello_window` draws its player box over a striped ground, and no longer caps the frame rate: vsync paces it.
- The desktop runner caps frames at 60 per second while the window is hidden or has no area, and requests redraws from `about_to_wait`.
- On the web the runner sizes the canvas's drawing buffer to the window's physical size, which WebGL2 needs.
- `--frames N` is read through `Args`, so it also works on the web as `?frames=N`.
- Dependencies build with `opt-level = 2` in the dev profile.
- The default log filter shows only errors from `wgpu_hal`.

## Phase 1 — Platform layer and game loop

### Added

- `cg_core`: the `App` builder over a `bevy_ecs` 0.20 `World` (plugins, resources, messages, systems, custom runners) and the `Plugin` trait.
- `cg_core`: frame schedules `Startup`, `First`, `PreUpdate`, `FixedPreUpdate`, `FixedUpdate`, `FixedPostUpdate`, `Update`, `PostUpdate` and `Last`.
- `cg_core`: a fixed-timestep loop (60 Hz default, at most 5 catch-up steps, 250 ms delta clamp) with an interpolation alpha.
- `cg_core`: the `Time` resource (delta, raw delta, fixed delta, elapsed, frame and step counts, time scale, alpha).
- `cg_core`: the `AppExit` message, `FrameLimitPlugin` (`--frames N`), the `default()` helper, and a prelude re-exporting the ECS, `glam` and `log`.
- `cg_platform`: `WindowPlugin`, a `winit` 0.30 runner for desktop (`run_app`) and web (`spawn_app`), with an optional desktop `max_fps` cap.
- `cg_platform`: the `Window` resource and `Lifecycle` messages (resumed, suspended, focus, occlusion, resize, scale factor).
- `cg_platform`: `InputPlugin` with `ButtonInput<KeyCode>`, `ButtonInput<MouseButton>`, unified mouse/touch `Pointers`, and `gilrs` `Gamepads` on desktop and web.
- `cg_platform`: an action map (`ActionMap`, `Binding`, `ScreenRegion`, serde/RON-ready). Its `ActionState` reports edges correctly inside fixed steps.
- `cg_platform`: `LogPlugin`, logging to `env_logger` on desktop and to the browser console (with a panic hook) on web.
- `cashgear_engine`: `DefaultPlugins` and a prelude for game code.
- `examples/hello_window`: opens a window, runs the loop, moves a pretend player with actions and logs input. It runs in the browser with `trunk serve`.
- `cargo clippy-wasm`: lints the wasm build with warnings denied.
- CI: Linux packages for `gilrs` and Xvfb, the wasm lint, and a `hello_window --frames 60` smoke run on Windows and Ubuntu.

### Changed

- `examples/*` joined the workspace members, and external dependencies are pinned in `[workspace.dependencies]`.
- The `clippy.toml` time guard now points at the `Time` resource. `web_time::Instant::now()` is `std::time::Instant::now()` on desktop, so the guard flags it too.

## Phase 0 — Repository and workspace

### Added

- A Cargo workspace (edition 2024, resolver 3) with skeleton crates: the `cashgear_engine` facade with a `prelude`, eleven `cg_*` engine crates, the `games/platformer` stub, and the `tools/cg_editor` and `tools/cg_pack` stubs.
- Lua scripting (`cg_script`) behind the facade's `script` feature, off by default.
- `rust-toolchain.toml` pinned to Rust 1.99.1 with `rustfmt`, `clippy` and the `wasm32-unknown-unknown` target.
- `rustfmt.toml`, `.gitattributes` (LF line endings) and `.gitignore`.
- A `clippy.toml` portability guard that rejects `Instant::now`, threads and direct `std::fs` calls in engine and game crates.
- A `cargo check-wasm` alias for the wasm check.
- A workspace-wide `missing_docs` lint.
- GitHub Actions CI: fmt, then clippy `-D warnings`, tests and the wasm check on Windows and Ubuntu.
- A dual MIT/Apache-2.0 license, `README.md`, `docs/PROGRESS.md` and this changelog.

### Changed

- Renamed `docs/plan.md` to `docs/PLAN.md` to match the workflow references.
