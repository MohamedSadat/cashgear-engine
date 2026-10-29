# CashGear Engine — Progress

This file tracks phase status, the decisions made at the start of each phase, known issues and the backlog. The plan itself is in [PLAN.md](PLAN.md).

## Phase status

| Phase | Title | Status | Tag |
| --- | --- | --- | --- |
| 0 | Repository and workspace | Done | `phase-0` |
| 1 | Platform layer and game loop | Done | `phase-1` |
| 2 | Renderer core | Done | `phase-2` |
| 3 | ECS and scenes | Next | |
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

## Phase 2 — Renderer core

### Decisions

| Question | Answer |
| --- | --- |
| Default virtual resolution | 640×360. It scales by a whole number to 720p (2×), 1080p (3×), 1440p (4×) and 4K (6×). |
| Pixel art or smooth art first | Pixel art, true grid. Nearest sampling is the default, and `ScalingMode::PixelPerfect` (the default mode) draws the scene into a 640×360 image that is scaled up by the largest whole number that fits, with bars. Sprites, rotations and camera moves all land on the virtual pixel grid. Smooth art uses `Filter::Linear` and `Letterbox`, `Stretch` or `WindowSize`. |
| Colour space | sRGB with linear blending. Surfaces and textures are sRGB; `Color` values are sRGB (as Aseprite shows them) and are converted to linear light in the shaders. 50% white over black is sRGB 188, and a test pins that. |
| Reference-image tests | Added now. `OffscreenRenderer` renders through the same painter as the window. `tests/golden` compares six scenes with PNGs (channel tolerance 4, at most 0.1% of pixels different) and adds exact checks for snapping and sRGB. |

Other choices made during the phase:

- **Versions.** wgpu 30.0.1, image 0.25.10 (PNG only), bytemuck 1.25.2, pollster 1.0.1 (desktop), wasm-bindgen-futures 0.4.79 (web). wgpu 30 differs from older guides: `queue.present(frame)`, `get_current_texture()` returns a `CurrentSurfaceTexture` enum, pipeline layouts take `Option<&BindGroupLayout>` and `immediate_size`, and `InstanceDescriptor` takes the display handle. glam 0.34 has no `Mat4::orthographic_rh`; the camera builds its scale matrix directly.
- **Drawing API before the ECS.** Systems queue draws into resources (`Sprites`, `DebugShapes`); the renderer draws them in the new `Render` schedule and empties the queues. `Camera2D` is a resource. Phase 3 turns these into components that feed the same queues.
- **Textures.** `Textures` keeps CPU copies (`Image`) and uploads new or changed ones once the GPU exists, so `Startup` can add textures on the web before the device is ready, and a lost device can re-upload everything. Ids are never reused. Phase 4's asset server replaces this store.
- **Batching.** Sprites are stable-sorted by (layer, texture) each frame (the sort is skipped when the queue is already in order). Each run of one texture is one instanced draw of six vertices per instance. Batches bind a slice of the instance buffer instead of using a first instance, which WebGL2 lacks. Instances are 56 bytes (two axes, translation, UV rect, sRGB tint). The tint is converted to linear in the vertex shader.
- **Pixel snapping.** `Camera2D::pixel_snap` (on by default) shifts each sprite in the vertex shader so its origin corner lands on a pixel of the image it is drawn into. Whole-texel sprites then map texels to pixels exactly at any sub-pixel position (tested).
- **Debug shapes** are tessellated into triangles on the CPU, with line widths in logical pixels. They are drawn after the pixel-perfect upscale at window resolution, so collider outlines stay thin over pixel art.
- **Uniforms.** All per-frame view data sits in one uniform buffer, bound with dynamic offsets. Three separate uniform bind groups rendered wrongly on Direct3D 12's WARP adapter (every pass read the last-created buffer), which the golden tests caught. The single-buffer layout works on every backend tried.
- **GPU setup.** `cg_platform::tasks::spawn_local` runs the async setup: it blocks on desktop (pollster) and is spawned on the web (`wasm_bindgen_futures`). The renderer checks for the result each frame and draws nothing until then. On the web, `new_instance_with_webgpu_detection` picks WebGPU when the browser can really create an adapter, else WebGL2. A setup failure is logged and exits the app with an error.
- **Surface format.** An sRGB format from the surface's capabilities. Browsers only offer non-sRGB canvas formats, so there the surface is configured as `Bgra8Unorm` with a `Bgra8UnormSrgb` view format.
- **Window handle and canvas.** `cg_platform` now keeps the window in an `Arc` and exposes it as the `WindowHandle` resource. On the web the runner sets the canvas's `width`/`height` to the physical size on creation and on each resize; WebGPU's configure does this itself, WebGL2 does not.
- **Frame pacing.** Vsync (`AutoVsync`) paces the desktop loop. While the window is occluded or has no area, nothing is presented, so the runner caps frames at 60 per second. Redraws are requested from `about_to_wait`, so leaving the capped mode always resumes.
- **Launch flags everywhere.** `cg_platform::Args` reads the command line on desktop and `?key=value&flag` from the page URL on the web. `DefaultPlugins` maps `--frames`, `--no-vsync` and `--backend <list>` (so `?backend=gl` forces WebGL2). On desktop `WGPU_BACKEND` and friends still override the renderer settings.
- **Dev profile.** `[profile.dev.package."*"] opt-level = 2`. Debug bunnymark went from about 70 to about 120 fps uncapped at 10,000 sprites.
- **Logging.** The default desktop filter adds `wgpu_hal=error`, which hides the Vulkan loader's harmless registry warnings. `RUST_LOG` overrides it.
- **Bunny art.** The four 18×24 bunnies in `examples/bunnymark/assets` are original pixel art made for this example.

### Done

- `cg_render`: `RenderPlugin`, window surface lifecycle (resize, outdated, lost, suspend/resume, device lost), `Textures`/`TextureId`/`Image`/`Filter`, `Sprites`/`Sprite`, `Camera2D` with `ScalingMode` and world/screen conversion, `DebugShapes`, `RenderStats`, `OffscreenRenderer`, `Color`, `RenderError`.
- `cg_core`: the `Render` schedule and `Rect`. `cg_platform`: `WindowHandle`, `tasks::spawn_local`, `Args`, canvas sizing and the idle frame cap.
- `examples/bunnymark` (with a trunk page); `hello_window` now draws.
- 99 tests: 53 new, including 7 GPU tests with 6 reference PNGs.
- CI: lavapipe on Ubuntu, `CG_REQUIRE_GPU=1`, `bunnymark --frames 120`, and failed renders uploaded as artifacts.

### Verified

- Reference images match on Vulkan, Direct3D 12 and GL (GTX 1650) and on WARP (software Direct3D 12). All four use the same PNGs.
- Desktop (Windows, GTX 1650), bunnymark at 10,000 sprites: 60 fps with vsync on Vulkan, Direct3D 12 and GL in a dev build. Uncapped dev build: about 120 fps. Uncapped release build: 795 fps at 10,000, 226 fps at 50,000 and 104 fps at 100,000 sprites. Each frame is 4 batches and 5 draw calls.
- Desktop window, checked by screenshot: the four bunny textures, flips and leaning; all four scaling modes; debug shapes; and a maximized window that drops to a whole 2× scale with bars.
- Web (`trunk serve`, Chromium in the app's browser pane), debug wasm build: WebGPU holds 10,000 bunnies at 60 fps, and so does WebGL2 (`?backend=gl`, ANGLE on D3D11); 20,000 on WebGL2 ran at 30. WebGL2 writes the clear colour as #1e1f24 against the expected #1d1f24, so sRGB output is right. Resizing the pane re-fits the view (below 1× in a narrow pane). A hidden tab stops drawing frames.
- Minimised on desktop, the loop drops to the 60 fps idle cap with nothing drawn (about 0 CPU seconds over 3 s). After restore it draws again at 5 draw calls a frame.
- `hello_window` runs at 60 fps on vsync alone, on desktop and on the web.

### Manual checks for the user

- Desktop: `cargo run -p bunnymark --release`. Hold the mouse to pour bunnies in, press M to cycle scaling modes, use the wheel and Q/E to zoom and turn, Space for debug shapes. The log line shows fps and draw calls.
- If you have a high-DPI display or a second GPU, check bunnymark there too (`--backend dx12` / `vulkan` / `gl`).
- Web: `cd examples/bunnymark && trunk serve --port 8081`, then open <http://127.0.0.1:8081> and <http://127.0.0.1:8081/?backend=gl> in Chrome, Edge and Firefox.

## Known issues

- On the web, `AppExit` stops the loop but leaves the page; there is nowhere to exit to.
- Within one layer, sprites are grouped by texture, so overlapping sprites with different textures draw in texture order, not queue order. Use layers when the order matters. Phase 3's ECS sorting can add a y-sort or z key.
- `Textures` keeps every image in CPU memory as well as on the GPU, so a lost device can re-upload. Phase 4's asset server should decide whether to drop CPU copies.
- Browsers hide the adapter name behind WebGPU, so `RenderStats::adapter()` reads "an unnamed GPU" there.
- With 20,000 bunnies in a debug wasm build, WebGL2 drops to 30 fps. A release web build is Phase 10's job.

## Notes for later phases

- **Phase 3:** `Sprite`, `Camera2D` and the `ScalingMode` settings become components or resources that feed `Sprites`. Keep `Sprites::draw` for immediate-mode drawing. Frustum culling can use `Camera2D::visible_rect`. Add a stable sort key (`z` or y-sort) if layers alone are too coarse.
- **Phase 3:** keep `App::add_systems` and the schedule labels (`Render` now exists). Add game states, and multi-threaded execution on desktop if profiling calls for it. Decide on `bevy_reflect` (the editor needs it).
- **Phase 4:** textures should come from the asset server as `Handle<Image>`; `Textures` and `TextureId` can become the GPU side of it. `Image::from_png` is the decode step to reuse. `trunk` only watches the example folder, so touch the example after editing engine crates while `trunk serve` runs.
- **Phase 6:** particles can reuse the sprite batcher (white texture, tint). Flash-on-hit and outline effects need a second sprite pipeline or a shader parameter in the instance data.
- **Phase 8:** egui will need its own pass after the debug shapes. `Painter::render` is the place to add it.
- **Phase 10:** measure a release wasm build (`trunk build --release`, `wasm-opt`). The debug wasm is about 10 MB.
- **Phase 8:** on-screen buttons can press actions through a new `Binding` variant or by driving `ScreenRegion` touches.
- **Phase 7:** `kira` (via `cpal`) needs `libasound2-dev` on Ubuntu CI.
- **Phase 11:** add `aarch64-linux-android` to `rust-toolchain.toml` targets and a CI job.
- **Phase 12:** `cg_script` will not build for wasm; keep it out of `check-wasm` or behind the `script` feature.

## Backlog

Features that came up but don't serve the platformer or a listed example go here.

- (empty)
