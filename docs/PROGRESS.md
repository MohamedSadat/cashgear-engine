# CashGear Engine — Progress

This file tracks phase status, the decisions made at the start of each phase, known issues and the backlog. The plan itself is in [PLAN.md](PLAN.md).

## Phase status

| Phase | Title | Status | Tag |
| --- | --- | --- | --- |
| 0 | Repository and workspace | Done | `phase-0` |
| 1 | Platform layer and game loop | Done | `phase-1` |
| 2 | Renderer core | Next | |
| 3 | ECS and scenes | Not started | |
| 4 | Assets and tilemaps | Not started | |
| 5 | Collision and physics | Not started | |
| 6 | Animation and effects | Not started | |
| 7 | Audio | Not started | |
| 8 | Text and UI | Not started | |
| 9 | Platformer sample game | Not started | |
| 10 | Web build | Not started | |
| 11 | Android build | Not started | |
| 12 | Lua scripting | Not started | |
| 13 | Scene editor | Not started | |
| 14 | Polish, performance and release | Not started | |

## Phase 0 — Repository and workspace

### Decisions

| Question | Answer |
| --- | --- |
| License | MIT OR Apache-2.0 (dual) |
| Copyright holder | The CashGear Engine contributors |
| Repository visibility | Public: github.com/MohamedSadat/cashgear-engine |
| Toolchain / MSRV | Pinned to Rust 1.99.1 in `rust-toolchain.toml`; `rust-version = "1.99"`. Bump between phases. |

Other choices made during the phase:

- **Layout.** Engine crates live in `crates/`, the sample game in `games/` and desktop tools in `tools/`. The root `Cargo.toml` is a virtual workspace.
- **Examples.** `examples/` will hold one binary crate per example, so `trunk` can build each one for the web. `examples/*` joins the workspace members with the first example in Phase 1.
- **Facade.** `cashgear_engine` re-exports each subsystem under a short module name (`core`, `platform`, `render`, ...). `cg_script` sits behind the `script` feature because `mlua` does not build for wasm.
- **Portability guard.** The root `clippy.toml` disallows `Instant::now`, `SystemTime::now`, `thread::spawn`, `thread::sleep` and direct `std::fs` calls. `cg_platform` opts out per module with `#[allow(clippy::disallowed_methods)]`. `tools/cg_editor` and `tools/cg_pack` have their own `clippy.toml` without the guard.
- **Docs lint.** `missing_docs` is on for the whole workspace. Combined with `-D warnings`, CI enforces rustdoc on public APIs.
- **Wasm check.** `cargo check-wasm`, an alias in `.cargo/config.toml`, checks the workspace for `wasm32-unknown-unknown` without the desktop-only tools.
- **Publishing.** `publish = false` for now; Phase 14 decides about crates.io.

### Done

- Workspace with all crate skeletons, the facade and `prelude`.
- `rust-toolchain.toml`, `rustfmt.toml`, `clippy.toml`, `.gitignore`, `.gitattributes`, license files.
- GitHub Actions: fmt, clippy `-D warnings`, test and the wasm check on Windows and Ubuntu.
- `README.md`, `CHANGELOG.md`, this file; `docs/plan.md` renamed to `docs/PLAN.md`.

## Phase 1 — Platform layer and game loop

### Decisions

| Question | Answer |
| --- | --- |
| Default fixed rate | 60 Hz. At most 5 catch-up steps per frame (the backlog beyond that is dropped). Frame deltas are clamped to 250 ms. |
| FPS cap or vsync | Vsync by default (Fifo from Phase 2; `requestAnimationFrame` on web). The optional `WindowConfig::max_fps` cap works on desktop only. `hello_window` sets it to 60 because there is no surface to vsync against yet. |
| Action map format | Built in code (`ActionMap::new().bind("jump", KeyCode::Space)`). The types derive serde, and a test proves the RON round trip. Loading from a file waits for the Phase 4 asset IO. |
| Resource storage before Phase 3 | `bevy_ecs` 0.20 `World` now. This settles Phase 3's "bevy_ecs vs hecs" question. Systems and schedules are real bevy ones, single-threaded. |

Other choices made during the phase:

- **Schedules.** `Startup` (first frame, after the window exists), `First`, `PreUpdate`, then 0..n × (`FixedPreUpdate`, `FixedUpdate`, `FixedPostUpdate`), `Update`, `PostUpdate`, `Last`. Phase 3 adds `Render` and state transitions.
- **Single-threaded executor.** `bevy_ecs` uses `default-features = false` with `std` and `async_executor`. There is no `multi_threaded` and no `bevy_reflect` (Phase 3 decides about reflection).
- **Messages and fixed steps.** Message buffers swap only after a fixed step has run, as in bevy, so `FixedUpdate` readers never miss a message. While `time_scale` is 0 they swap every frame instead, so paused games do not pile up messages.
- **Input in `FixedUpdate`.** `ActionState::just_pressed`/`just_released` report "since the previous fixed step" inside fixed steps and "this frame" elsewhere. A press is seen exactly once by gameplay, whatever the frame rate. The raw `ButtonInput` just-states are per frame; game code should use actions.
- **Taps.** A press and release inside one frame shows as just-pressed and just-released. An action is active for that frame, so taps are never lost.
- **Pointers.** `Pointers` holds the mouse and every touch as `Pointer`s with logical and normalized positions. Touch actions bind to normalized `ScreenRegion`s, and only touch pointers trigger them. On-screen buttons in Phase 8 can build on that.
- **Input types.** `KeyCode` (physical keys) and `MouseButton` are re-exported from `winit`, with winit's `serde` feature. Gamepad buttons and axes are engine enums mapped from `gilrs`.
- **Gamepads.** `gilrs` runs on desktop and web (Gamepad API). Its context is a non-send resource polled in `PreUpdate`. `InputPlugin { gamepads: false }` skips it for tests and tools.
- **Clock.** Only the `cg_platform` runner reads the clock. On desktop `web_time::Instant` *is* `std::time::Instant`, so the clippy guard flags `web_time::Instant::now()` too. Engine code reads the `Time` resource instead, and the guard's message now says so.
- **Logging.** `LogPlugin` sends records to `env_logger` (honours `RUST_LOG`) on desktop and to `console_log` plus `console_error_panic_hook` on web.
- **bevy derives in game crates.** bevy's `Resource`/`Component`/`Message` derives expand to `bevy_ecs::...` paths. The prelude re-exports `bevy_ecs` (doc-hidden), so the derives work in crates that depend only on `cashgear_engine` and import its prelude. Engine crates depend on `bevy_ecs` directly.
- **Web canvas.** `WindowConfig::canvas_id` uses an existing `<canvas>`, falling back to appending one. The page's CSS sizes it. The runner measures it at creation so `Startup` sees the real size, and focuses it so keys work without a click.
- **Scroll units.** Lines, with 100 logical pixels counted as one line (a browser wheel notch).
- **`--frames N`.** `FrameLimitPlugin` (part of `DefaultPlugins`) exits after N frames. CI runs `hello_window --frames 60` on Windows and under Xvfb on Ubuntu.
- **Stricter wasm lint.** A new `cargo clippy-wasm` alias runs clippy for wasm32 with `-D warnings`, and CI runs it in place of `check-wasm`. `check-wasm` stays for quick local checks.
- **Versions.** External dependencies are pinned in `[workspace.dependencies]`: bevy_ecs 0.20.1, winit 0.30.13 (0.31 is still in beta), gilrs 0.11.2, glam 0.34.1, web-time 1.1.0, log 0.4.34, env_logger 0.11.11, console_log 1.1.0, console_error_panic_hook 0.1.7, serde 1.0.229, ron 0.12.3 (dev only), wasm-bindgen 0.2.129, web-sys 0.3.106.

### Done

- `cg_core`: `App` builder (plugins, resources, messages, systems, runner), `Plugin` trait, frame schedules, fixed-timestep loop with interpolation alpha, `Time` (delta, raw delta, fixed delta, elapsed, frame and step counts, time scale, alpha), `AppExit`, `FrameLimitPlugin`, prelude re-exporting the ECS, `glam` and `log`.
- `cg_platform`: `WindowPlugin` with the `winit` 0.30 `ApplicationHandler` runner (desktop `run_app`, web `spawn_app`), `Window` resource, `Lifecycle` messages (resumed, suspended, focus, occlusion, resize, scale factor), `InputPlugin` (`ButtonInput`, `Pointers`, `Gamepads`, `ActionMap`, `ActionState`), `LogPlugin`.
- `cashgear_engine`: `DefaultPlugins` (log, `--frames`, input, window) and a prelude covering both crates.
- `examples/hello_window`: a pretend player moved by actions in `FixedUpdate`, with input, lifecycle and a once-a-second status line logged. It has a trunk `index.html` with a full-page canvas.
- 40 unit tests (timestep maths, schedule order, messages, exit, button and pointer state, action edges in fixed steps, axis deadzones, touch regions, RON round trip) and 6 doc tests.
- CI: Linux packages (`libudev-dev`, Xvfb), `clippy-wasm`, and the `--frames` example run.

### Verified

- Desktop (Windows): the window opens, runs at 60 fps with 60 fixed steps/s, logs lifecycle events and exits after `--frames`.
- Web (`trunk serve`, Chromium): runs at 60 fps with 60 fixed steps/s. Keyboard works without a click; mouse, wheel and synthetic touch were logged; touch regions trigger `jump`/`left`; the fixed-step jump fires once.

### Manual checks for the user

- Desktop: run `cargo run -p hello_window`, press keys, click and scroll, and check that the terminal logs them.
- A gamepad on desktop and in the browser: buttons and the left stick should log and drive `left`/`right`/`jump`.

## Known issues

- Without a renderer, an uncapped desktop loop (`max_fps: None`) runs flat out. Phase 2's vsync fixes it.
- On the web, `AppExit` stops the loop but leaves the page; there is nowhere to exit to.

## Notes for later phases

- **Phase 2:** if anything pulls `getrandom` 0.3+ into the wasm build (Phase 1 does not), wasm needs its `wasm_js` backend: the `wasm_js` feature plus `--cfg getrandom_backend="wasm_js"` in the wasm target rustflags.
- **Phase 2:** consider `[profile.dev.package."*"] opt-level = 2` so the dev-profile bunnymark stays near 60 FPS.
- **Phase 2:** the runner owns the `winit::window::Window`. The renderer needs it (as an `Arc`) for the surface; expose it from `cg_platform` then. On web the canvas drawing buffer is still the default 300×150; size it to `Window::physical_size()` when configuring the surface.
- **Phase 2:** with vsync in place, drop `max_fps: Some(60.0)` from `hello_window`. Render in a `Render` schedule after `PostUpdate`, interpolating with `Time::alpha()`.
- **Phase 2:** `trunk serve` must run inside the example's folder, because the root is a virtual workspace. Install it with `cargo install trunk --locked`.
- **Phase 3:** keep `App::add_systems` and the schedule labels; add `Render`, game states, and multi-threaded execution on desktop if profiling calls for it. Decide on `bevy_reflect` (the editor needs it).
- **Phase 8:** on-screen buttons can press actions through a new `Binding` variant or by driving `ScreenRegion` touches.
- **Phase 7:** `kira` (via `cpal`) needs `libasound2-dev` on Ubuntu CI.
- **Phase 11:** add `aarch64-linux-android` to `rust-toolchain.toml` targets and a CI job.
- **Phase 12:** `cg_script` will not build for wasm; keep it out of `check-wasm` or behind the `script` feature.

## Backlog

Features that came up but don't serve the platformer or a listed example go here.

- (empty)
