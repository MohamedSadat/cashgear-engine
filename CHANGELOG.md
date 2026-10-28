# Changelog

All notable changes to CashGear Engine are recorded here, one section per phase.
The format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/).

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
