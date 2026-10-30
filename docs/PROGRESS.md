# CashGear Engine — Progress

This file tracks phase status, the decisions made at the start of each phase, known issues and the backlog. The plan itself is in [PLAN.md](PLAN.md).

## Phase status

| Phase | Title | Status | Tag |
| --- | --- | --- | --- |
| 0 | Repository and workspace | Done | `phase-0` |
| 1 | Platform layer and game loop | Done | `phase-1` |
| 2 | Renderer core | Done | `phase-2` |
| 3 | ECS and scenes | Done | `phase-3` |
| 4 | Assets and tilemaps | Done | `phase-4` |
| 5 | Collision and physics | Done | `phase-5` |
| 6 | Animation and effects | Done | `phase-6` |
| 7 | Audio | Done | `phase-7` |
| 8 | Text and UI | Done | `phase-8` |
| 9 | Platformer sample game | Done | `phase-9` |
| 10 | Web build | Done | `phase-10` |
| 11 | Android build | In progress (device test left) | |
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

## Phase 3 — ECS and scenes

### Decisions

| Question | Answer |
| --- | --- |
| `bevy_ecs` or `hecs` | `bevy_ecs` 0.20, settled in Phase 1. The hierarchy is bevy's own `ChildOf`/`Children`. |
| Camera in the ECS | `Camera2D` is a component on an entity whose transform places it. The renderer draws through one active camera (lowest entity index if several are active, with a warning). Multiple cameras and split screen go to the backlog. |
| Prefab format and `bevy_reflect` | A serde registry, no reflection. Components register under a short name; prefabs list them by name with nested children, and missing fields take their defaults. Textures are named until Phase 4's asset paths. `bevy_reflect` waits for the editor (Phase 13), where it would drive the inspector. |
| Game states | An engine-written stack over the game's own enum: `push`, `pop`, `replace`, `set`; `OnEnter`, `OnExit`, `OnPause`, `OnResume`; `in_state` (top only) and `state_in_stack` run conditions; `StateScoped` entities. |
| Interpolation | Opt-in `Interpolated` component. Entities without it draw where they are. |

Other choices made during the phase:

- **Transforms.** `Transform2D` holds translation, rotation (radians) and scale; `GlobalTransform2D` is an `Affine2`, so skew from non-uniformly scaled parents is exact. Propagation runs before and after each fixed step and in `PostUpdate`. All three runs share one "last propagated" tick (`TransformPropagation`), so each change is computed once. Gameplay in `FixedUpdate` sees globals as of the start of the step. Parents without a `Transform2D` stop propagation.
- **Interpolation details.** The snapshot is taken in `FixedPreUpdate` after a propagation, so entities spawned in `Startup` or `OnEnter` do not blend in from the origin. Children of an interpolated entity are drawn relative to its blended transform, so a hat on an interpolated player moves smoothly without its own `Interpolated`. Turns under about 3 degrees per step blend as matrices (cheaper; at most 0.03% shrink), larger turns are decomposed.
- **State transitions.** A new `StateTransition` schedule in `cg_core` runs after `PreUpdate`, before the fixed steps. The initial state's `OnEnter` runs in the first frame's transition, after `Startup`. During an `OnExit`, the exiting state is still current. Changes requested from enter schedules apply in the same frame (at most 64 a frame).
- **Prefab mechanics.** Plain components (`Clone + Serialize + Deserialize`) register with `register_component`. Components that resolve names at spawn time, such as `Sprite` and its texture name, implement `PrefabComponent` and register with `register_prefab_component`. Parsing goes through `erased-serde` (0.4.11, a small dtolnay crate) so each component is read straight from the RON text with its own serde impl. That keeps enums and newtypes exact, which `ron::Value` would lose. Errors carry line and column; a component that fails at spawn time is skipped and logged, and the rest of the prefab still spawns. `Prefabs` stores RON text and parses it on first use, so registration order does not matter. Prefabs serialize back to RON (round-trip tested) for the editor later.
- **Sprites.** `Sprite` is now the component (texture, size, source, pivot, colour, flips). `Layer` is a separate component; `Visibility` (Inherited, Visible, Hidden) computes `InheritedVisibility` down the hierarchy. The immediate-mode queue stays: `Sprites::draw(sprite, transform)` and `draw_on(layer, ...)` push `QueuedSprite`s with an `Affine2`.
- **Queueing and culling.** In `Render`, `RenderSystems::Queue` picks the camera, builds the `FrameView`, and queues each visible sprite entity whose world bounding box overlaps the camera's visible rectangle. `RenderSystems::Draw` then draws. `RenderStats::culled` counts what was skipped.
- **`CameraView` and `MainCamera`.** `CameraView` is a camera's settings plus position and rotation; it carries the world/screen maths and is what `OffscreenRenderer` takes. `MainCamera` is a system parameter giving game code `screen_to_world`, `world_to_screen` and `visible_rect` for the active camera.
- **Draw order.** Still layer, then texture within a layer. No z or y-sort key yet; the platformer can use layers. See known issues.
- **Dev profile.** The workspace's own crates now build with `opt-level = 1` in dev (dependencies stay at 2). bevy's generic query code is compiled into our crates, and at opt-level 0 the 10,000-entity bunnymark ran at 9 fps; at 1 it runs at about 390 fps uncapped. A full rebuild of the workspace crates takes about 20 s.
- **Example art.** `ecs_sprites` reuses two of bunnymark's original bunnies; the hat, star, ground and cloud are drawn in code.

### Done

- `cg_world`: `Transform2D`, `GlobalTransform2D`, propagation, `Interpolated` and `InterpolatedTransforms`, the state stack, `StateScoped`, prefabs (`PrefabRegistry`, `Prefab`, `Prefabs`, `PrefabComponent`, `spawn_prefab`), `WorldPlugin`, `AppWorldExt` and a prelude.
- `cg_core`: the `StateTransition` schedule; `Rect::overlaps`, `Rect::from_points`, serde for `Rect`.
- `cg_render`: `Sprite`, `Layer`, `Visibility`/`InheritedVisibility` and `Camera2D` components; `CameraView`, `MainCamera`, `FrameView`, `RenderSystems`; culling; texture names; serde for `Color`, `ScalingMode` and `Camera2D`.
- `examples/ecs_sprites` (with a trunk page and `--demo`), `bunnymark --ecs`, and `hello_window` on the camera entity.
- 153 tests (54 new), including a golden test that an ECS hierarchy renders the same pixels as hand-placed sprites. The six existing reference PNGs are unchanged.
- CI: `bunnymark --frames 120 --ecs` and `ecs_sprites --frames 480 --demo`.

### Verified

- All tests, including the golden images, pass on Vulkan (GTX 1650) and on WARP (software Direct3D 12).
- Desktop (Windows, Vulkan): `ecs_sprites --demo` runs menu, playing, paused, playing and menu without warnings at 60 fps. The log shows bunnies spawned from the prefab, caught and despawned with their hats (entity counts go back down), and 45 to 60 sprites culled outside the view.
- Desktop, bunnymark at 10,000 sprites, uncapped (`--no-vsync`), Vulkan on a GTX 1650:

  | Build | Immediate mode | Entities (`--ecs`) |
  | --- | --- | --- |
  | Release | about 750 fps (1.33 ms) | about 520 fps (1.92 ms) |
  | Dev | about 600 fps (1.67 ms) | about 390 fps (2.6 ms) |

  So the ECS path (propagation, interpolation, visibility, culling and queueing) costs about 0.6 ms per frame for 10,000 moving entities in release. Both modes hold 60 fps with vsync.
- Web (`trunk serve`, the app's browser pane, debug wasm), checked by screenshot: the menu logo with its orbiting star ring, the level with ground, clouds, the hatted player and falling hatted bunnies, and the pause overlay centred on the camera. `?demo` runs every transition at 60 fps on WebGPU and on WebGL2 (`?backend=gl`, ANGLE on Direct3D 11).
- Not checked by me: the desktop window by screenshot (screen access was declined this session), and keyboard and gamepad play. See the manual checks.

### Manual checks for the user

- Desktop: `cargo run -p ecs_sprites`. In the menu the stars should orbit the big bunny. Press Enter to play: run with A/D or the arrows, jump with Space, and catch falling bunnies. Click to drop bunnies at the pointer, and press V to hide the player and its hat together. Escape pauses: the level should freeze under a dim overlay with a pause sign. Escape resumes; Q from the pause goes back to the menu, and Q in the menu quits.
- On a monitor faster than 60 Hz, the player, bunnies and camera should move smoothly (they are interpolated).
- `cargo run -p bunnymark --release -- --ecs --no-vsync` against the same command without `--ecs`, to compare the ECS cost on your machine.
- Web: `cd examples/ecs_sprites && trunk serve --port 8082`, then open <http://127.0.0.1:8082> (and `?demo`, `?backend=gl`).

## Phase 4 — Assets and tilemaps

### Decisions

| Question | Answer |
| --- | --- |
| Tiled TMX or JSON | TMX and TSX, read by the `tiled` crate (0.16, without its zstd feature, which is C code and does not build for wasm). The crate reads files synchronously, so the loader serves it from memory: it parses, and whenever the parser asks for a file it does not have yet (a tileset, a template), it reads that file through the async asset IO and parses again. |
| Asset folder layout | Each game or example has its own `assets/` folder; assets are named by `/`-separated paths inside it (`maps/level.tmx`). Desktop finds the folder through `CG_ASSET_ROOT`, then `assets` in `CARGO_MANIFEST_DIR` (set by `cargo run`), then next to the executable, then in the working directory. On the web, `trunk` copies `assets/` next to `index.html` and the engine fetches from there. |
| Atlases at build time or runtime | Build time. `cg_pack atlas <folder> <out.atlas.ron>` writes the image and a RON file of named regions, and the outputs are committed. Packing is deterministic; `--check` fails when they are stale, and CI runs it. The runtime only loads atlases and looks regions up by name. |
| Example art | Kenney's Pixel Platformer (CC0), downloaded by the user since kenney.nl has a bot check. Only the packed sheets, a hills strip composed from its background tiles, and character frames for the atlas are in the repository, with Kenney's license as `examples/tiled_map/assets/KENNEY-LICENSE.txt`. |

Other choices made during the phase:

- **Handles.** `Handle<T>` is a reference-counted strong handle or a weak one; `AssetId<T>` is an index and generation, `Copy` and eight bytes. Indices are reused with a new generation, so a stale id finds nothing. When the last strong handle drops, the asset is unloaded in the next asset update. The first 16 indices are reserved for assets at fixed ids; `AssetId::DEFAULT` is the white texture.
- **Sprites hold handles.** `Sprite::texture` is a `Handle<Image>`, so a sprite keeps its image loaded and `Sprite` is no longer `Copy`. `QueuedSprite` holds the texture as an `AssetId` and the sprite's fields directly, so immediate-mode queues (bunnymark's 10,000+ bunnies, tile chunks) never touch reference counts.
- **One asset update per frame.** `AssetServer::update` runs in `First`: it picks up changed files, starts queued loads, and stores finished ones in their `Assets<T>`. A load started any time appears in the next update, on every target. On desktop, files are read and decoded inside that update (a whole dependency tree usually loads in one frame); on the web, the fetches finish over later frames. Desktop loads block the frame while they run; see known issues.
- **Async loaders.** `AssetLoader::load` returns a boxed (non-`Send`) future, so a loader can read more files through the `LoadContext` (the map loader reads tilesets) and load dependencies (`ctx.load`). Files read that way are recorded, so changing a tileset reloads the maps that use it.
- **Load states.** `load_state` covers the asset alone; `recursive_load_state` and `is_loaded` include its dependencies (`Failed` if any failed). `LoadingAssets` holds strong handles to a group and reports `progress`; `add_loading_state(loading, next)` moves on when the group is done, even with failures (they are logged one by one, then summarised).
- **Hot reload.** On by default in debug desktop builds (`--hot-reload` / `--no-hot-reload`). `notify` reports changes from its own thread inside `cg_platform`; a file counts as changed once it has been quiet for 100 ms, so an editor's several writes during one save reload once. A reload replaces the asset in place (same id, new version, `AssetEvent::Modified`); a failed reload logs the error and keeps the previous version. `AssetServer::reload(path)` does the same by hand on every target (the example's R key).
- **CPU copies of images stay.** `Assets<Image>` keeps every image in memory after upload, so a lost GPU can re-upload. The renderer syncs uploads by version, like `Textures` did. Revisit in Phase 10/11 if memory matters.
- **Images carry their filter.** `Image::filter` replaces the per-texture filter of `Textures`; change it on a loaded image through `Assets<Image>::get_mut`. There are no loader settings yet.
- **Code-made assets by path.** `AssetServer::add(path, value)` stores an asset as if loaded from `path`, so prefabs can name art drawn in code (`ecs_sprites` registers `generated/hat` and friends).
- **Prefabs as assets.** `app.load_prefab(name, path)` loads a `.prefab.ron`, adds it to `LoadingAssets` and copies it into `Prefabs` whenever it loads or changes; new spawns use the new version. Prefab `Sprite`s and `AtlasSprite`s name assets by path and load them through the server when spawned.
- **Atlas sprites.** `AtlasSprite { atlas, region }` points the entity's `Sprite` at a region once the atlas loads, and again whenever the atlas or the region name changes, so animation by region name works (the example's `Frames`).
- **Map coordinates.** A map's bottom-left corner sits at its entity's origin, one Tiled pixel per world unit, y up: Tiled's `(x, y)` is `(x, height_px - y)`. Rows are counted from the top as in Tiled. Tall tiles hang from the bottom-left of their cell, as Tiled draws them.
- **Tile chunks.** Each visible tile layer is cut into 16×16-tile chunks of `QueuedSprite`s, built once per map version and culled whole against the view; visible chunks are copied into the sprite queue each frame with the map's transform and parallax applied. That is the "cached chunk" the plan asked for, at the CPU level: the 100×20-tile example queues about 320 tiles a frame. Tiled's flips (including diagonal ones) are folded into each tile's transform, so the batcher needs nothing new.
- **Parallax.** A layer with factor `p` is shifted by `(1 - p) × (camera - parallax origin)`, measured in the map entity's space, as Tiled does. Image layers repeat to fill the view (at most 4096 copies a frame).
- **Objects.** Each object becomes a child of the map entity with a `TiledObject` (id, name, class, layer, shape in world units, properties), a `Transform2D` (the centre of rectangles, ellipses and tile objects; Tiled's anchor otherwise; Tiled's clockwise degrees become counter-clockwise radians), a `Name` and a `Layer` (`base + layer index`). Tile objects get a `Sprite` of their tile. A tile object without a class takes its tile's class and properties.
- **Objects and prefabs.** The class (or a `prefab` string property) names a prefab. If `Prefabs` has it, it is inserted into the object's entity, and the object's position and rotation are put back (the prefab's scale stays). Then each property named after a registered prefab component replaces that component: a class-typed property gives its members as fields, a string is read as RON (or as a string), and other values fill a one-field component such as `Layer(7)`.
- **Rebuilds.** When a `Tilemap`'s map asset changes, its chunks are rebuilt and its objects despawned and spawned again, so objects lose any state they had. Objects are children of the map entity, so despawning the map despawns them.
- **System order.** `cg_render` now picks the camera in `RenderSystems::View` before `RenderSystems::Queue`, so `cg_tiled` (and later crates) queue sprites from the `FrameView`. `RenderStats::culled` is reset in `View` and added to by each queueing system.
- **Missing files on the web.** `trunk serve` answers unknown paths with the page itself (status 200), so an HTML answer for a file that is not HTML counts as not found, and errors read the same on every target.
- **Versions.** notify 8.2.1 (9.0 is still a release candidate), tiled 0.16.0, js-sys 0.3.106 (web, for the fetched bytes).

### Done

- `cg_platform`: `io::AssetIo` (filesystem, `fetch`, an Android stub until Phase 11), `io::FileWatcher`, `io::IoError`.
- `cg_assets`: `AssetServer`, `Handle`, `AssetId`, `UntypedHandle`, `UntypedAssetId`, `Assets`, `AssetEvent`, `AssetLoader`, `LoadContext`, `AssetError`, `LoadState`, `LoadingAssets`, `LoadProgress`, `PrefabSource`, `PrefabLoader`, `PrefabFiles`, `AssetPlugin`, `AppAssetExt`, `AssetSystems` and the `path` helpers.
- `cg_render`: `Image` as an asset with `ImageLoader` and `Filter`; `TextureAtlas`, `AtlasFile`, `AtlasRegion`, `AtlasLoader`, `AtlasSprite`; `RenderSystems::View`; `RenderStats::add_culled`. `Textures` and `TextureId` are gone.
- `cg_tiled`: `TiledMap` and its parts (`Tileset`, `TileInfo`, `MapLayer`, `LayerKind`, `TileLayer`, `LayerTile`, `ImageLayer`, `MapObject`, `ObjectShape`, `Properties`, `PropertyValue`), `TmxLoader`, `Tilemap`, `TilemapState`, `TiledObject`, `TiledPlugin`.
- `tools/cg_pack`: `cg_pack atlas` with `--padding`, `--extrude`, `--max-size` and `--check`.
- `cashgear_engine`: `DefaultPlugins` adds `AssetPlugin` (with `--hot-reload`, `--no-hot-reload`, `--asset-root`, `--asset-delay`) and `TiledPlugin`.
- `examples/tiled_map` (new, with a trunk page) and `ecs_sprites` (now loading from `assets/` through a `Loading` state); `bunnymark` and `hello_window` ported.
- 201 tests (48 more than Phase 3), including integration tests over real asset folders; the reference PNGs are unchanged.
- CI: `tiled_map --frames 360 --demo` and the atlas `--check`.

### Verified

- All tests, including the golden images, pass on Vulkan (GTX 1650) and on WARP (software Direct3D 12).
- Desktop (Windows, Vulkan), `tiled_map --demo`: the level (619 tiles in 29 chunks, 53 objects) loads in the first frame and holds 60 fps with vsync, about 320 sprites in 4 draw calls with 340 to 380 tiles culled. The demo's reload rebuilds the level.
- Hot reload on desktop, with the example running: saving the map file (a changed title) rebuilt the level within a frame of the 100 ms quiet time; saving the tileset (a changed tile property) reloaded the map through it; saving a broken map logged `could not load maps/level.tmx: Document ended before root element <map> was found; keeping the version loaded before` and the level stayed; restoring the files rebuilt it again.
- Web (`trunk serve`, the app's browser pane, debug wasm), checked by screenshot: the level with parallax hills and clouds, coin arcs, animated slimes, aliens and bats, and decorations, on WebGPU and on WebGL2 (`?backend=gl`), at 60 fps. Every asset arrives over `fetch`. `?asset-delay=3` shows the loading bar, then the level. Pressing R re-fetches and rebuilds the map. A missing prefab reports `not found`.
- Not checked by me: the desktop window by screenshot (I checked the desktop through logs, and the picture on the web, which runs the same code), editing in the Tiled application itself (I edited the files from a script), keyboard and gamepad scrolling. See the manual checks.

### Manual checks for the user

- Desktop: `cargo run -p tiled_map`. The level scrolls by itself; arrows or A/D take over, Space toggles auto-scroll. The hills should scroll slowest, the clouds a little faster, the ground with the camera. Coins bob, slimes, aliens and the pink alien walk back and forth and face where they walk, bats flap.
- Live editing: with the example running, open `examples/tiled_map/assets/maps/level.tmx` in Tiled (1.10 or later), move a platform or add tiles and save. The level should rebuild in place; the terminal logs `rebuilt maps/level.tmx`. Then change a `Patrol` property on an enemy (its speed) and save.
- `cargo run -p tiled_map -- --asset-delay 1` shows the loading bar on desktop.
- `cargo run -p ecs_sprites`: the same game as in Phase 3, now loading its prefabs and images from `assets/`. Editing `examples/ecs_sprites/assets/prefabs/bunny.prefab.ron` (the hat colour, say) while it runs changes the bunnies spawned after the save.
- Web: `cd examples/tiled_map && trunk serve --port 8083`, then open <http://127.0.0.1:8083> (and `?backend=gl`, `?asset-delay=2`).

## Phase 5 — Collision and physics

### Decisions

| Question | Answer |
| --- | --- |
| Slopes in v1 | Yes: 45° and 22.5°, and in fact any surface from a left height to a right height. Kenney's Pixel Platformer has no slope tiles, so the example's `slopes.png` is made from Kenney's grass and dirt tiles, sheared column by column (five tiles: a 45° slope and the cell under it, the two halves of a 22.5° slope and the cell under the lower half). Slopes going down are the same tiles flipped horizontally in Tiled. |
| Units | Pixels: one world unit is one Tiled pixel, as the renderer and `cg_tiled` already use. Speeds are pixels per second; the default gravity is 980 down. |
| `rapier2d` | Later, in the backlog. The platformer needs no dynamic bodies. |
| Character controller | The engine gives a `KinematicBody` with contacts; the example writes its own short controller, and Phase 9 builds the platformer's (coyote time, jump buffer) in the game. |

Chosen without asking (said at the start of the phase):

- **Which layers collide.** Tile layers with a `collision` property set to `true`. Otherwise the parallax cloud layer in `tiled_map`, which uses the `one_way` cloud tiles, would collide.
- **The example's level** is its own test course, not `tiled_map`'s level.
- **Debug drawing** toggles with F1 (`PhysicsPlugin::debug_key`); `--physics-debug` starts with it on.

Other choices made during the phase:

- **Schedule.** The physics runs in `FixedPostUpdate` after `TransformSystems::Propagate`, in three chained sets: `Prepare` (snapshot every collider into `PhysicsWorld` and rebuild the spatial hash), `Move` (kinematic bodies) and `Detect` (overlap messages). Game code sets velocities in `FixedUpdate`; contacts and messages are ready in the next `FixedUpdate`. Reading `GlobalTransform2D` after propagation means bodies and colliders moved in `FixedUpdate` are seen where they are now. A body's `Transform2D` moves by the same amount in its parent's space, and its `GlobalTransform2D` translation is updated too, so `Detect` and later systems see the new position.
- **Movement.** Gravity, then a move along x, then along y (or onto the floor), each swept over the whole distance against tiles and static colliders. Long moves are cut into sub-steps of at most 8 units (at most 32 per step), so ground following sees every part of a slope; sweeps keep even the longest sub-step from passing anything. Hitting something zeroes the velocity in that direction. A floor that would lift the body into a ceiling turns into a wall.
- **Slopes carry the foot.** A body stands on a slope with the middle of its bottom edge on the surface, as platformer characters do; its corners hang over or into the hill. While the foot is over a slope, only the foot's column decides the floor, so a body walks down a slope instead of hanging off the plateau at its top. Slopes block only from above (bodies pass them from below and from the sides, like one-way platforms). Near a slope, a grounded body may run its lower corner into terrain as high as the slope rises under half its width plus the move, which lets it reach the plateau at the top of a slope. Bodies wider than twice a slope's run climb narrow slopes by their corner instead.
- **Bodies are boxes.** Kinematic bodies move as the bounding box of their collider, and treat every collider they hit as its bounding box. Rectangles stay axis-aligned: an entity's rotation moves a collider's offset but does not turn it.
- **Static colliders** are entities with a `Collider` and no `Sensor` or `KinematicBody`. They block bodies but are not moved by the physics; a crate moved by code does not push bodies.
- **Overlaps.** Every pair of interacting colliders that share area is reported, except two static ones; touching is not overlapping, so a body resting on a crate does not overlap it. A body that moved is tested against sensors and other bodies by the box covering its whole move, so the dash in the example collects every coin it passes. Pairs are ordered by entity index: bevy's own `Entity` ordering (and `to_bits`) runs backwards over the index, so `Pair` orders by `index_u32` and generation.
- **Tiled.** Tile properties `solid`, `one_way`, `slope_left` and `slope_right` (pixels from the tile's bottom) and `hazard`. Where collision layers overlap, the topmost layer's shape wins and a hazard on any of them counts. Flipped-vertically or diagonal slope tiles collide as solid blocks, with a warning. Layer offsets and parallax are ignored for collision, with a warning. Objects with a `collider` or `sensor` property get a collider of their shape (round ellipses become circles, other shapes their bounding box; points have no area). `Collider`, `Sensor`, `KinematicBody` and `CollisionLayers` are prefab components, so prefabs and Tiled properties can set them too. The grid is rebuilt when the map changes (tested through `AssetServer::reload`).
- **Hazards** cover the whole cell (overlap, or touching a solid hazard). Tiled's per-tile collision shapes are not read; see the backlog.
- **Queries.** `SpatialQuery` reads the `PhysicsWorld` snapshot and only `TileCollider`s, so it can sit next to queries that move entities. Rays hit solids from any side and one-way platforms and slopes only from above, the way they block bodies.
- **API.** `KinematicBody` requires `Interpolated` (bodies move in fixed steps). `drop_through` is a public field: a private field would stop games from writing `KinematicBody { max_fall_speed: 420.0, ..default() }`.
- **Touching.** Distances up to 0.01 units count as touching (`EPSILON`).
- **Example bot.** The bot fills the same per-step `Intent` the controls do, inside `FixedUpdate`, so its run depends only on the number of fixed steps, never on the frame rate. A test runs it without a window; CI runs `collision --demo --physics-debug` with no frame limit, since the demo exits by itself (with an error if the bot restarts or runs out of time).

### Done

- `cg_physics`: `Collider`, `ColliderShape`, `Sensor`, `WorldShape`; `KinematicBody`, `KinematicContacts`, `Contact`, `Gravity`; `TileGrid`, `TileCell`, `TileShape`, `TileCollider` and `TileGrid::from_tiled_map`; `CollisionLayers`; `CollisionStarted`, `CollisionEnded`, `Overlaps`; `PhysicsWorld`, `SpatialQuery`, `QueryFilter`, `RayHit`, `TileHit`; `PhysicsDebug`; `PhysicsPlugin`, `PhysicsSystems` and a prelude.
- `cashgear_engine`: `DefaultPlugins` adds `PhysicsPlugin` (field `physics`, flag `--physics-debug`); the prelude includes the physics.
- `examples/collision` (new, with a trunk page on port 8084): the test course, the slope tiles, the box controller, coins, hints and the bot.
- 247 tests (46 more than Phase 4): sweeps, floors, slopes both ways at several speeds, tunnelling at 100,000 units per second, one-way platforms, ledges and step height, low ceilings, crates, hazards, layers, rays, the spatial hash, overlap pairs and swept sensors, Tiled properties, objects and the rebuild, prefabs, child bodies, and the bot finishing the course. The reference PNGs are unchanged.
- CI: `collision --demo --physics-debug`.

### Verified

- All tests pass; fmt, clippy (desktop and wasm) and rustdoc with warnings denied are clean.
- Desktop (Windows, GTX 1650), `collision --demo`, on Vulkan and on Direct3D 12: 60 fps; the course is complete in 16.9 s of game time with 39 of 39 coins and no restarts; the dash stopped at x 1650.0 (the pillar's face at 1656 minus half the box); the slam landed on a one-way cloud at 3,000 units per second. The other four examples still run without warnings.
- Web (`trunk serve`, the app's browser pane, debug wasm), checked by screenshot: the course with the slope tiles, the parallax hills, planks and clouds, and the debug drawing (tile edges, slope surfaces, crossed spikes, amber sensors and hints, the box). `?demo` runs the course to the goal flag and stops.
- Not checked by me: the desktop window by screenshot, playing with keyboard, gamepad or touch, and WARP (`WGPU_ADAPTER_NAME` did not pick it for the windowed run; the demo is frame-rate independent anyway). See the manual checks.

### Manual checks for the user

- Desktop: `cargo run -p collision`. Run up and down both hills: the box should stay on the slopes without bumps at the top or bottom. Jump over the spikes (touching them sends you back to the start), up the planks, and along the high plateau. Down + jump drops through planks and clouds; S or Down in the air slams. Shift dashes: into the pillar it should stop dead, never pass through. Climb the crates and jump the pit to the flag.
- Press F1 to see the colliders: blue tile edges and slope lines, cyan one-way platforms, red crossed spikes, green crates, amber coins and hints, the magenta box with its contacts in yellow. With it on, a white ray follows the mouse and stops at what it hits.
- `cargo run -p collision -- --demo` watches the bot run the course.
- Live editing: with the example running, open `examples/collision/assets/maps/course.tmx` in Tiled, change the ground and save; the log says `rebuilt collision for maps/course.tmx`.
- Web: `cd examples/collision && trunk serve --port 8084`, then open <http://127.0.0.1:8084> (and `?physics-debug`, `?demo`).

## Phase 6 — Animation and effects

### Decisions

| Question | Answer |
| --- | --- |
| Clip format | Aseprite's JSON sheet export is the standard: both the "Hash" and "Array" layouts, trimmed or not, tags as clips with their direction and repeat count, and events in a tag's user data (`0:footstep, 1:footstep`). Clips can also be built in code from the regions of a `cg_pack` atlas, which replaced `tiled_map`'s `Frames` stand-in. |
| Character art | Kenney's Pixel Platformer green alien, the character Phase 9 will most likely use. It has two poses, so the sheet adds a blink and a squashed landing frame edited from them, and tweens do the rest: squash and stretch, breathing, the hit flash and blink. Aseprite is not installed here, so a script wrote the sheet in Aseprite 1.3's export format (trimmed, Hash layout). |
| Particle budget | At most 16,384 live particles across all emitters (`AnimPlugin::particle_budget`), plus a cap per emitter (1,024 by default). Particles beyond the budget are not emitted, and the first time that happens is logged. The target was 10,000 live at 60 fps in a debug build on desktop and web. |
| Shader effects | A hit flash now (`Sprite::flash`); outlines went to the backlog, because they sample neighbouring texels and Kenney's sheets have no padding. |

Chosen without asking (said at the end of the questions):

- **State machine files.** `AnimationGraph` is a serde struct, loaded from `.anim.ron` files (hot-reloaded) or built in code.
- **Tiled tile animations.** Included, since Phase 5's notes put them in this phase: the example's goal flag waves.

Other choices made during the phase:

- **Schedule.** Animation runs in `FixedPostUpdate` after `PhysicsSystems::Detect`, in `AnimSystems::Animate`: tweens of interpolated transforms, then animators, then clips and their events. Camera following runs next, in `AnimSystems::Camera`. Frames and events therefore depend only on the number of fixed steps, never the frame rate. Game code sets animator parameters in `FixedUpdate` and reads events there in the next step, as with physics contacts. Particles, the other tweens, shake decay and fades run every frame in `Update` (`AnimSystems::Effects`). `cg_anim` depends on `cg_physics` only for that ordering.
- **Clips.** A ping-pong play goes there and back without repeating the ends (0 1 2 1), and a finished ping-pong clip holds the frame it started on. A tag without a repeat count (or 0) loops for ever. Events fire each time their frame starts showing, including frames passed over in one long step. A sheet without tags gets one `default` clip. Tag user data that is not `frame:name` is ignored with a warning. A new or switched clip shows its first frame in `PostUpdate` of the same frame, so even a paused game shows it.
- **Pivots.** `SpriteAnimation::pivot` is a fraction of the untrimmed frame. The player sets `Sprite::pivot` for every frame so trimmed frames stay put. With `flip_x`, the frame mirrors around that point, so a character turns round on the spot. A plain `Sprite` mirrors inside its quad instead. The player sets the texture, source and pivot and leaves size, colour and flips to the game.
- **Animators.** Transitions are tried in file order, up to eight a step, so `land` → `idle` → `run` can happen in one step. An empty `from` means any other state. Triggers last one step and are used up by the transition they fire. A state without a `clip` plays the clip named after it; `speed_param` scales its speed by the parameter's absolute value. `force` jumps to a state (respawns). A reloaded graph keeps the current state if it still exists.
- **Tweens.** Properties are an enum (translation, scale, rotation, colour, flash, zoom), not generic lenses, so there are no trait objects and tweens stay easy to describe. Transform tweens on `Interpolated` entities run in fixed steps, so interpolation smooths them. All other tweens run every frame, on scaled time either way. `Tweens::start` replaces a running tween of the same property; `add` runs alongside. A tween's time past its end is not carried into its `then` follow-up, an error of at most one frame. Unchanged values are not written, so change detection stays quiet.
- **Particles.** The settings use degrees because they are mostly written by hand in prefabs. Particles emitted during a frame spread along the emitter's path and are aged by the part of the frame since they left, so a moving emitter leaves an even trail. Each emitter has its own PCG-32 stream (`with_seed` makes one repeatable). Particles go to the sprite queue as one `QueuedSprite` each, culled per emitter by bounds that use the real texel size.
- **Camera.** `CameraFollow` moves the camera's transform in fixed steps; give the camera `Interpolated`. Shake and the bounds act on `FrameView` in `Render`, between `RenderSystems::View` and `Queue`, so `MainCamera` and pointer picking ignore the shake. A zoom tween, which changes the view between fixed steps, never shows past the bounds; the example's watch caught exactly that before the view was clamped too. Shake is trauma squared times smooth value noise.
- **Fades.** `ScreenFade` draws a sprite on layer `i32::MAX` over the visible rectangle (debug shapes stay on top) and runs on real time. `StateFade<S>` queues `StateChange`s and hands each to `NextState` once the screen is covered. It fades back in a frame later, after `OnEnter` has run.
- **Flash.** `Sprite::flash` is a `Color` whose alpha is the amount. It travels as `Unorm8x4`, so an instance is now 60 bytes (was 56). The shader mixes towards it in linear light and keeps the alpha. A transparent flash leaves the output exactly as it was, so the six reference PNGs did not change; a new GPU test pins the mix. The batcher skips the colour conversion for sprites without a flash: converting every sprite's flash cost 0.45 ms a frame at 10,000 sprites, found by running bunnymark against a Phase 5 build. With the skip, release bunnymark is back at about 1.37 ms (730 fps) at 10,000 sprites, as before.
- **Asset versions.** `Assets` versions now come from one counter for the whole process. The new GPU test exposed a test-only bug: every test's first image had the same id and version, so the shared offscreen renderer sometimes drew another test's texture. The fix also covers an editor's play mode later, which may share one renderer between worlds.
- **Tile animations.** Tile layers swap the source rectangle of animated tiles as chunks are queued. Tile objects of animated tiles get a `TileAnimation` that sets their sprite in `PostUpdate`. Both run on `Time::elapsed`, all copies in step as in Tiled.
- **Example art.** A scratch script made `images/alien.png` and `images/alien.json` from Kenney's characters: seven frames, trimmed, packed in a row with one texel of padding. Another made `maps/playground.tmx` from `collision`'s Kenney tiles and slope tiles. Neither script is in the repository.
- **Versions.** serde_json 1.0.154.

### Done

- `cg_anim`: `SpriteSheet`, `SheetFrame`, `AnimationClip`, `ClipFrame`, `ClipEvent`, `Direction`; `AsepriteLoader` and `AsepriteExport`; `SpriteAnimation` (with `FEET`), `AnimationEvent`, `AnimationFinished`; `AnimationGraph`, `AnimationState`, `Transition`, `Condition`, `Test`, `Animator`, `AnimationStateChanged`, `AnimationGraphLoader`; `Tween`, `TweenValue`, `TweenProperty`, `TweenData`, `Tweens`, `TweenFinished`, `TweenCommandsExt`, `Ease`; `ParticleEmitter`, `ParticleSettings`, `EmitShape`, `Particle`, `ParticleBudget`; `CameraFollow`, `CameraShake`; `ScreenFade`, `StateFade`, `AppFadeExt`; `AnimPlugin`, `AnimSystems` and a prelude.
- `cg_render`: `Sprite::flash` (and in `SpriteData` and `QueuedSprite`).
- `cg_tiled`: `TileAnimation`, `TileFrame`, `TileInfo::animation`.
- `cg_assets`: versions unique across collections.
- `cashgear_engine`: `DefaultPlugins` adds `AnimPlugin` (field `anim`); the prelude includes `cg_anim`'s.
- `examples/animation` (new, with a trunk page on port 8085); `tiled_map`'s enemies ported to clips.
- 308 tests (61 more than Phase 5): easing curves, the random stream and noise, clip directions, pivots of trimmed and flipped frames, Aseprite exports in both layouts with tags, repeats, events and errors, playback and events in an app, animator transitions with triggers, chains and forcing, tween clocks, chains and messages, particle rates, bursts, the budget and one-shot despawns, follow with dead zone and bounds, shake, state fades, tile animations in layers and objects, the flash on the GPU, and the example's bot playing the whole demo. The reference PNGs are unchanged.
- CI: `animation --demo`.

### Verified

- All tests pass, the reference images too, on Vulkan (GTX 1650) and on WARP. fmt, clippy (desktop and wasm) and rustdoc with warnings denied are clean.
- Desktop (Windows, GTX 1650, Vulkan), `animation --demo`: 60 fps with vsync. The bot reaches the goal in about 10.5 s of game time with 14 of 19 coins, 58 footsteps, 10 landings (the cliff at the 480 cap) and one hit. All six states were seen, peak trauma was 0.6, and the camera never showed outside the level. The gallery held about 10,900 particles at 60 fps in 6 draw calls. Uncapped (`--no-vsync`) with those particles: 402 fps in a dev build, 521 in release. The other five examples still run as CI runs them, without warnings. Release bunnymark at 10,000 sprites: about 730 fps (1.37 ms), level with a Phase 5 build run back to back.
- Web (`trunk serve`, the app's browser pane, debug wasm, WebGPU), checked by screenshot at an exact 2×:
  - the level fading in, with the alien, spinning coins and the parallax hills;
  - the F1 drawing of colliders, the dead zone and the coin sensors;
  - the bot at the goal, and the fade on the way to the gallery;
  - the gallery (snow, fountain, fire and the stream) holding about 10,860 particles in 8 draw calls.
- Not checked by me:
  - The web frame rate. The pane was hidden, so the page ran as a throttled background tab at 4–5 fps.
  - WebGL2.
  - The desktop window by screenshot.
  - Play by keyboard, gamepad or touch.
  - The flag waving and the hit flash by eye. Tests cover both.

### Manual checks for the user

- Desktop: `cargo run -p animation`.
  - Run with A/D or the arrows and jump with Space. The alien should blink and breathe while idle, kick up dust at every step, stretch on take-off and squash on landing (more after the cliff).
  - Touch the spikes: a white flash, a blink, a knock back and a shake.
  - Coins spin, bob and sparkle, and pop into a burst when taken. The flag at the goal waves.
  - Z zooms in and out smoothly, never showing past the level's edges. P fades into the particle gallery and back. F1 shows the colliders, the camera's dead zone (pink) and the bounds (green).
- `cargo run -p animation -- --demo` watches the bot. Add `--no-vsync` to see the frame rate in the gallery.
- Live editing: with the example running, change a transition in `examples/animation/assets/anims/alien.anim.ron` (for example the run speed) and save; the alien changes at once. Tag durations in `images/alien.json` reload too.
- `cargo run -p tiled_map`: the enemies walk and flap as before, now through clips.
- Web: `cd examples/animation && trunk serve --port 8085`, then open <http://127.0.0.1:8085> (and `?demo`, `?backend=gl`). With the tab in front, the gallery (P) should hold 60 fps.

## Phase 7 — Audio

### Decisions

| Question | Answer |
| --- | --- |
| Audio formats | OGG Vorbis and WAV (16-bit or other PCM), mono or stereo. Only symphonia's `ogg`, `vorbis`, `wav` and `pcm` features are compiled, which keeps the wasm build small. |
| Most sounds at once | 32 effect and interface voices, and 4 copies of one sound (`AudioPlugin::max_voices`, `max_instances`, and `PlaySettings::max_instances` per play). Past a cap the oldest copy that does not loop is cut short with a 20 ms fade; if every candidate loops, the new sound is skipped. Music is not counted. |
| Settings menu | Not yet: keys now, a menu in Phase 9 with Phase 8's UI. The example draws the levels as bars (there is no text until Phase 8). The settings are saved through the new `Storage` and come back on the next run. |
| Example sounds | Original, synthesized for the example by a scratch Python script (not in the repository) and encoded to OGG Vorbis with ffmpeg's libvorbis. |

Chosen without asking (said with the questions):

- **Streaming without threads.** kira's streaming sounds decode on a thread of their own and are not built for wasm at all. Music is streamed by our own decoder instead, which the main thread tops up every frame, the same on every target. `cg_audio` has no `cfg` and spawns no thread (portability rules 1 and 3).
- **Storage** arrives in this phase, in `cg_platform` (portability rule 7).
- **Positional audio** is the engine's own maths (pan and distance falloff from the camera), sent to kira as volume and panning; kira's 3D spatial tracks are more than a 2D game needs.
- **Web unlock and the background:** the device opens on the first key press, click or touch; music and looping sounds asked for earlier start then, other sounds are skipped. Sound pauses while the app is suspended (Android) or the page hidden.
- **The example** is the Phase 6 playground with sound, not a new level.

Other choices made during the phase:

- **What kira does.** kira (0.12.6, `default-features = false`, only its `cpal` backend) provides the device, the mixer and the bus tracks. Files are decoded by symphonia directly, not by kira's sound types. One kira `Sound` implementation (`VoiceSound`) plays both decoded frames and streams, resampling from the file's rate to the device's with kira's 4-point Hermite `interpolate_frame`. On desktop kira's cpal backend follows a change of the default output device by itself.
- **Control without queues.** Each voice shares a few atomics with the audio callback: volume, panning and speed targets with a ramp time, and a play, pause or stop request with a fade. The callback reads them once per block and moves towards them frame by frame. Nothing is queued, so nothing can overflow when frames stop (a hidden tab). Fades follow an equal-power curve, so a crossfade keeps the loudness even.
- **Streams.** A streamed voice decodes half a second before it starts, then at most a quarter second per frame into a two-second ring buffer (`rtrb`), which keeps up even at 4 frames a second. An empty buffer plays silence and counts an underrun (`AudioStats::underruns`). A looping stream opens its file again at the end. symphonia's decoder trims the encoder's delay and padding, so loops are sample-exact; a test compares two and a half loops of a stream with the decoded file. Sounds up to 10 seconds (`AudioPlugin::stream_longer_than`) are decoded at load, so they start at once and voices share them.
- **Requests once a frame.** `Audio` is a plain resource. Calls queue commands, applied in `PostUpdate` after transform propagation (`AudioSystems::Update`), so positions are those of this frame. `Voice` ids are never reused; voices the engine starts itself (music, emitters) use ids with the top bit set.
- **The device opens when first needed:** when the game asks for its first sound, so games and examples without sound never open one. If it cannot be opened (no sound card, as on CI runners), that is logged once and the game runs silent; `--no-audio` does the same on purpose. `AudioOutput::Mock` runs kira's mock backend for tests.
- **Sounds still loading** start when they load. A one-shot that waited more than half a second is skipped, since a late coin sound is worse than none; music and loops wait. Unknown and failed sounds are logged and skipped.
- **Music.** A crossfade fades the old voice out and the new one in over the same time, both on the music bus. Asking for the track already playing changes only its volume, so re-entering an area keeps the music going. A track that ends (not looping) clears `Audio::music`.
- **Buses.** A master sub-track (so everything can pause at once) with music, effects and interface tracks under it. `AudioSettings` levels are loudness: amplitude is the level squared, so 0.5 is -12 dB and slider steps feel even. Settings are saved in `Last` when they change, under `audio-settings.ron` by default; loading them is not a change, so nothing is written at start-up.
- **Positional defaults.** `Spatial { near: 96, far: 640, pan_width: 320, max_pan: 0.8 }` suits the 640×360 view: full volume within 96 units, a `(1 - t)²` falloff to silence at 640, and 0.8 of a full pan at the edge of the view. kira's panning keeps power even (a full pan is +3 dB on that side). Positions are updated every frame with a 50 ms ramp. Sounds are heard from the first `AudioListener`, else the active camera's transform (before shake, which moves only the drawn view), else the origin.
- **The background.** The new `cg_platform::LifecycleChanged` schedule runs at once when the app is suspended or resumed or the window hidden or shown, because no frame may follow (Android in the background; a browser stops frames in a hidden tab). `cg_audio` pauses and resumes the master track there, with a 50 ms fade. `AudioPlugin::pause_when_hidden` (on by default) covers a background tab and a minimized desktop window.
- **User activation on the web.** `Window::user_activated` follows `navigator.userActivation.hasBeenActive` where the browser has it (Chrome, Edge, Firefox 120+, Safari 16.4+), and the browser's own rule elsewhere: a key press other than Escape, a mouse press or the end of a touch. Script-made events do not count where the API exists, since they would not let the page start sound. The device opens in the frame after the gesture; Chromium allows that, because activation is sticky.
- **Storage.** `Storage` is a cheap-to-clone resource over a `StorageBackend`: text values by key, synchronous, for small things (settings, save slots). Keys and namespaces are 1 to 64 ASCII letters, digits, `.`, `_` or `-` (not starting with `.`), so they are valid file names and `localStorage` keys everywhere. Desktop writes `<key>.tmp` and moves it over the old file, so a crash keeps the previous value. The default namespace is the executable's name on desktop and `game` elsewhere; games set their own.
- **Logging.** symphonia logs two lines each time a stream opens or loops; the default filter now hides its messages below warnings. On the web a small logger in front of `console_log` applies the same filter.
- **Versions.** kira 0.12.6 (cpal 0.18.2 with it), symphonia 0.6.1 (MPL-2.0; used unmodified, which the license allows in an MIT/Apache project), rtrb 0.4.1.
- **Example assets.** `maps/level.tmx` is the Phase 6 playground with four objects added: two `Ambient` points and two `MusicZone` sensor rectangles. The art is copied from `examples/animation`. The scratch synth script made two chiptune loops (a 30 s meadow tune at 128 BPM and a 42 s calm one at 92 BPM; square, triangle and noise voices, mixed circularly so they loop without a seam) and nine effects. Together they are about 1 MB of OGG.

### Done

- `cg_audio`: `Sound`, `SoundLoader`, `DecodeError`; `Audio`, `Voice`, `PlaySettings`, `MusicSettings`, `AudioStatus`, `AudioStats`; `Bus`, `AudioSettings`; `Spatial`, `AudioListener`, `AudioEmitter`; `AudioOutput`, `AudioPlugin`, `AudioSystems` and a prelude.
- `cg_platform`: `storage` (`Storage`, `StorageBackend`, `MemoryStorage`, `StorageError`, `StoragePlugin`), `Window::user_activated`, the `LifecycleChanged` schedule, and the quieter log filter on desktop and web.
- `cashgear_engine`: `DefaultPlugins` adds `StoragePlugin` (field `storage`) and `AudioPlugin` (field `audio`, flag `--no-audio`); the prelude includes them.
- `examples/audio` (new, with a trunk page on port 8086).
- 345 tests (37 more than Phase 6): WAV decoding in mono and stereo, bad files, gapless OGG Vorbis, a stream looping sample-exactly, resampling, speed, fades, pause and stop in the callback, panning, one-shots finishing, both voice caps, positional pan and falloff from the camera and from a listener, emitters following, restarting and stopping, the music crossfade and repeat, stopping a bus, waiting for user activation, pausing while suspended, settings loaded from and saved to storage, files loaded through the asset server while a play waits, streams through the app, the silent output, pitch jitter, storage keys and the file and memory backends, and the example's bot with the mock device. The reference PNGs are unchanged.
- CI: `libasound2-dev` on Ubuntu, and `audio --demo`.

### Verified

- All tests pass; fmt, clippy (desktop and wasm) and rustdoc with warnings denied are clean.
- Desktop (Windows, GTX 1650, Vulkan, the default output device), `audio --demo`: the device opened with the first sound; the meadow tune crossfaded to the heights tune at the zone; both ambient loops played; the bot took 14 of 19 coins (best run 6) with 58 footsteps, 10 landings and a hit; 0 underruns at 60 fps; exit success. `hello_window`, `bunnymark`, `animation --demo` and `collision --demo` still run without warnings and never open an audio device. Release bunnymark at 10,000 sprites: about 730 fps (1.37 ms), level with Phase 6.
- Web (`trunk serve`, the app's browser pane, debug wasm, WebGPU):
  - before any input the status is `Waiting`, the speaker light blinks yellow and the music request is held;
  - a real click opened Web Audio: music (streamed) and both ambient loops, 0 underruns, 60 fps, light green;
  - the volume panel's keys changed the music level; the settings were saved in `localStorage` (`cashgear/cashgear-audio-example/audio-settings.ron`), a reload brought the level back, and M muted (master icon red);
  - `?demo` after a click ran as on desktop: both zones, 0 underruns, no warnings or errors in the console, and no symphonia lines.
- Not checked by me:
  - **Listening.** I cannot hear. I checked the sound by numbers instead: peaks and loudness, loop seams, spectrograms of the music, decoded samples compared exactly, and underrun counters. The mix, the loudness of each effect and the music itself need your ears.
  - Pausing in a hidden browser tab and on a minimized window: tested through the `LifecycleChanged` schedule, not seen happen.
  - Firefox and Safari (their unlock rules), WebGL2, Android, and keyboard or gamepad play on desktop.
  - The browser pane's own key presses did not reach the game (they seem to carry no physical key code); script-made `KeyboardEvent`s with a `code` did.

### Manual checks for the user

- Desktop: `cargo run -p audio`, and listen:
  - the meadow tune starts at once; footsteps while running, each a little different; a blip on each jump; a thud on landing, louder after the cliff; a crunch on the spikes; coin chimes that rise a tone each in a quick run; a chime at the flag;
  - the campfire near the start crackles and moves from the right ear to the left as you pass it, fading with distance; the fountain by the goal splashes;
  - past the hill, at the planks, the music crossfades to the calm tune over 1.5 s; walking back crossfades back;
  - the music loops (30 s and 42 s) and the campfire (4 s) should have no click or gap at the seam.
- The volume panel at the top left: 1 to 4 pick master, music, effects or interface (white marker); - and = change it with a click on the interface bus; M mutes (master icon red). Quit and run again: the levels come back. The file is `%APPDATA%\CashGear\cashgear-audio-example\audio-settings.ron`.
- Minimize the window: the sound pauses; restore it: it resumes.
- `cargo run -p audio -- --demo` lets the bot play with sound. `--no-audio` runs silent (grey light).
- Web: `cd examples/audio && trunk serve --port 8086`, open <http://127.0.0.1:8086>. The light blinks yellow until you click or press a key; then the music starts. Switch to another tab: the sound stops; come back: it resumes. Try Firefox and Safari if you have them.

## Phase 8 — Text and UI

### Decisions

| Question | Answer |
| --- | --- |
| Arabic and right-to-left text in v1 | Yes, in full. cosmic-text shapes Arabic and orders mixed-direction text, and when the language is right to left the layouts mirror: anchors, offsets, padding, rows, sliders and `Start`/`End` text alignment. A node can opt out with `LayoutDirection` (the example's touch controls do). |
| UI style | Pixel. The UI is laid out in the camera's virtual pixels (640×360 by default) and scaled by the same whole number as the game, so nine-slice pixel art and pixel fonts stay sharp. The engine also works with smooth UI (`ScalingMode::WindowSize`: one UI pixel per logical pixel). |
| Languages in the example | English and Arabic. |
| Fonts and UI art | Pixelify Sans (79 KB) and Noto Kufi Arabic (434 KB), both SIL OFL 1.1, downloaded from github.com/google/fonts with their license files. The panels, buttons, slider parts, touch controls and the HUD's bitmap digits are original pixel art drawn by a scratch script (not in the repository). |

Chosen without asking (said with the questions):

- **Text** goes through cosmic-text (shaping with harfrust, bidirectional text, line breaking) and swash, rasterized into the engine's own glyph atlas and drawn by the sprite batcher, not glyphon's renderer: text needs no second pipeline and sorts with the rest of the UI.
- **Bitmap fonts** are BMFont's text format (`.fnt` with PNG pages).
- **UI as entities**: `Node` plus widget components, laid out through `ChildOf`, registered for prefabs; our own layout (anchors, pixel and percent sizes, padding, rows and columns), no flexbox.
- **Drawing**: a screen-space sprite pass at the window's resolution after the world (after the pixel-perfect upscale), then debug shapes, then egui. `ScreenFade` moved to the top of that pass, so menus fade with the game.
- **Input**: UI actions with default bindings, focus that moves to the nearest widget in the pressed direction, pointer and touch hit testing. Touch controls press actions through `VirtualInput` (instead of a new `Binding`), show after the first touch or with `--touch`, and hide on key or gamepad input.
- **egui overlay** fed from the engine's own input (no egui-winit), drawn last, behind the `debug-overlay` feature (on by default), F12 toggles it. The inspector shows every component by name and the values of registered `Debug` types; `Transform2D` is editable. Reflection waits for Phase 13.
- **Localization**: one `strings/<lang>.strings.ron` per language, `{name}` arguments, the default language as fallback, hot reloading, the chosen language saved in `Storage`.

Other choices made during the phase:

- **Screen space.** `ScreenSpace` (in `cg_render`) is the UI's coordinate system: UI pixels from the top-left of the camera's view, y down, with the physical scale and origin. It follows the scaling mode but not zoom: `PixelPerfect` and `Letterbox` use the virtual resolution, `Stretch` keeps UI pixels square (the short side decides), `WindowSize` uses logical pixels. `Sprites::push_screen` queues sprites there; `QueuedSprite::screen_rect` maps a rectangle the right way up. Screen sprites are sorted by layer only, so the UI draws in queue order, and they bind their own uniform slot (`View::Screen`) with pixel snapping on.
- **Overlays.** `cg_render::RenderOverlays` holds `RenderOverlay` passes that record into the frame's encoder after everything else; `OverlayFrame::device_generation` tells them when the device was recreated. egui uses it; the editor can too.
- **Layout.** Runs every frame in `PostUpdate` after visibility: localized text is applied, bars are sized, then each root (a `Node` without a `Node` parent) is laid out against the screen. `Val::Auto` sizes come from text, an image's size or the children; `Val::Fill` shares a row's leftover space. Rectangles are snapped to physical pixels, so nine-slice pieces meet without seams. Hidden nodes keep their space. Right-to-left mirroring happens while placing children (it is the parent's direction that decides where a child goes). The result is `ComputedNode` (rectangle, content box, z-index layer, depth-first order, direction, visibility) and `UiStack`, every visible node in draw order, which hit testing walks backwards.
- **Text layout.** `TextLayout` caches a text's natural size and up to eight wrapped sizes, and its glyphs with the width, scale, font revision and direction they were made for. Text is centred vertically in its node's content box. `Start` and `End` follow the node's direction (as in CSS), not the paragraph's.
- **Rasterizing.** Pixel fonts (`FontSettings::pixel`) are rasterized at UI pixels, with hinting off, glyphs on whole UI pixels and coverage thresholded at half, so they are exactly sharp at any whole scale (a test checks every 2×2 block). Other fonts are rasterized at the screen's scale with smooth edges and drawn one texel per physical pixel. The atlas has up to four 1024×1024 pages; when they fill, it starts again and all text is laid out once more.
- **Fonts.** The default `Handle<Font>` is Pixelify Sans, built into `cg_ui` (sharp at 11 UI pixels and multiples; `DEFAULT_FONT_SIZE`). cosmic-text's fallback lists name system fonts the engine never loads, so `cg_ui` gives it empty ones: a character the text's font lacks is found in any loaded font (Arabic in an English font's text, Latin in the Arabic font's). `.font.ron` files name a font file with settings (`pixel`, `line_height`). Noto Kufi's own spacing is 1.9 times the size; the example sets 1.4.
- **Bitmap fonts** lay out left to right with kerning and word wrap, at `size / font.size()` times, and are drawn straight from their pages.
- **Widgets.** `Button` (with `Clicked`), `Slider` (with `SliderChanged`; dragging, keys, steps), `ProgressBar`, with `BarFill` and `SliderThumb` children positioned from the value; `StateColors` and `StateImages` show hovered, pressed and disabled states (focus shows as hovered). `UiImage` draws an image or a nine-slice (corners at one texel per UI pixel, shrinking evenly when the node is smaller).
- **Pointers.** In `PreUpdate` after the actions: the topmost interactive node under each pointer gets it; a press captures it until release, and a release over the same node clicks. The mouse hovering a widget moves the focus there. `UiPointer::over_ui` tells games whether a click landed on the UI. While the egui overlay uses the pointer, the game UI ignores it.
- **Focus.** Candidates are the focusable nodes of the topmost UI tree that has any (a pause menu over the HUD, a dialog over a menu). Directions pick the closest widget along the direction, weighting sideways distance double, and wrap round to the far end of the same line. Held directions repeat after 0.4 s every 0.11 s, on real time, so paused games navigate. When a menu opens and the player is on keys or a gamepad, its first widget (or the one marked `Focusable { auto_focus }`) gets the focus at once; the example's bot found the case where Enter did nothing without it. `ui_back` does nothing by itself; menus read it.
- **Default UI bindings** are added for actions the map lacks, at build time and whenever the game replaces its `ActionMap`.
- **Touch controls.** `TouchButton`s hold their action while a finger is within 6 UI pixels of them; a `TouchDpad` presses the actions within 67.5 degrees of the finger's direction from its centre (two on diagonals) and keeps following that finger past its edge. `TouchMode::Always` also takes the mouse, for trying them on desktop. `Window::touch_screen` (new) is `matchMedia("(pointer: coarse)")` on the web and true on Android.
- **Input additions in `cg_platform`.** `VirtualInput` (held and tapped actions that `ActionState` counts like bindings, with or without an entry in the map), `TextInput` (the text typed each frame, for egui), `InputSystems` sets, and `App::configure_sets` in `cg_core`.
- **egui.** The overlay builds its windows in an exclusive system in `Render`, so the inspector can read and edit any component, then hands the triangles and texture changes to its `RenderOverlay` through a shared buffer. egui's texture changes must be cleared after use (epaint 0.37 panics on dropping them otherwise; found in the browser). Game systems can draw their own windows through `EguiContext` while the overlay is open. With the overlay on, bevy's `debug` feature names component types for the inspector; it costs nothing measurable.
- **Performance.** The batcher's world and screen queues share one generic `append`, which stopped `instance()` from being inlined: release bunnymark lost 0.1 ms a frame at 10,000 sprites until it was marked `#[inline]` (found by running a Phase 7 build back to back). An empty UI costs nothing measurable.
- **Versions.** cosmic-text 0.19.0 (`std` and `swash` only: no system fonts), egui 0.37.0 (with its default fonts), egui-wgpu 0.37.0, unicode-script 0.5.8 (the script type cosmic-text's fallback trait names).

### Done

- `cg_ui`: `Font`, `FontKind`, `OutlineFont`, `BitmapFont`, `BitmapGlyph`, `FontSettings`, `FontFile`, `FontLoader`, `FontError`, `DEFAULT_FONT_SIZE`; `Text`, `TextStyle`, `TextAlign`, `TextData`, `TextLayout`, `PositionedGlyph`, `TextSystem`; `Node`, `Val`, `Anchor`, `Edges`, `Layout`, `Align`, `Justify`, `LayoutDirection`, `ZIndex`, `ComputedNode`, `UiStack`; `BackgroundColor`, `BorderColor`, `UiImage`, `UiImageData`, `Button`, `Clicked`, `Interaction`, `Focusable`, `StateColors`, `StateImages`, `Slider`, `SliderChanged`, `ProgressBar`, `BarFill`, `SliderThumb`; `UiFocus`, `actions`, `UiPointer`; `TouchControls`, `TouchMode`, `TouchControl`, `TouchButton`, `TouchDpad`; `Strings`, `StringsFile`, `StringsLoader`, `Locale`, `LocaleSettings`, `Localized`; `DebugOverlay`, `EguiContext`, `Inspector`, `InspectExt` and `egui` (feature `debug-overlay`); `UiPlugin`, `UiSystems` and a prelude. The embedded Pixelify Sans with its license in `crates/cg_ui/fonts/`.
- `cg_render`: `ScreenSpace` (with `Camera2D::screen_space` and `MainCamera::screen_space`), the screen-space queue (`Sprites::push_screen`, `screen_queued`, `QueuedSprite::screen_rect`) and its pass, `RenderOverlay`, `RenderOverlays`, `OverlayFrame`, `VisibilitySystems`, `Image::data_mut`.
- `cg_platform`: `VirtualInput`, `TextInput`, `InputSystems`, `Window::touch_screen` (and `with_touch_screen`), quieter egui-wgpu logs. `cg_core`: `App::configure_sets`.
- `cg_anim`: `ScreenFade` draws in screen space, over the UI and the bars.
- `cashgear_engine`: `DefaultPlugins` adds `UiPlugin` (field `ui`, flags `--touch`, `--no-touch`, `--overlay`) and registers the animation, audio and Tiled components with the inspector; the `debug-overlay` feature (default); the prelude includes the UI.
- `examples/ui` (new, with a trunk page on port 8087): main menu, settings, HUD with touch controls, pause menu, English and Arabic, `--demo` bot, and an ignored test that renders every screen to PNGs. `examples/audio`'s volume panel is now UI nodes in screen space.
- 394 tests (49 more than Phase 7; one more, the screen capture, is ignored), including a reference image of a UI screen (panel, border, pixel text, nine-slice, bar, a right-to-left row) and exact checks that pixel text sits on the pixel grid; layout of anchors, rows, columns, fill and padding; mirroring; text sizes, wrapping, crisp and smooth glyphs, Arabic shaping and fallback, bitmap fonts with kerning; clicks, states, disabled buttons; focus moves, wrapping, repeats, menus on top; sliders by drag and keys, right to left too; touch buttons and the d-pad; localization with fallback, arguments, fonts and direction; the screen-space queue order; the overlay; and the example's bot through every screen.
- CI: `ui --demo --overlay`.

### Verified

- All tests pass, the reference images too (the new UI one on Vulkan with a GTX 1650 and on WARP). fmt, clippy (desktop and wasm) and rustdoc with warnings denied are clean.
- Desktop (Windows, GTX 1650, Vulkan), `ui --demo`: menu, Arabic and back, settings, play, pause and quit at 60 fps with vsync, 10 to 24 draw calls, exit success; with `--overlay` as well. The other examples still run as CI runs them, without warnings.
- Offscreen captures of every screen of the bot's run (`cargo test -p ui -- --ignored capture_screens`), looked at: the English and Arabic menus (Arabic joined, right to left, with Latin words in place, the whole menu mirrored), settings over the dimmed menu, the HUD with the bitmap-font clock, the energy bar and the touch d-pad, jump and pause buttons, and the pause menu. Magnified, the pixel font is exact 2×2 blocks and the Arabic is smooth.
- Web (`trunk serve`, the app's browser pane, debug wasm): on WebGPU the menu, keyboard navigation by script-made key events, the Play screen with its HUD, and the debug overlay with its frame graph and render stats (1280×720 at scale 1, the UI 640×360 at 2); on WebGL2 (`?backend=gl`, ANGLE on Direct3D 11) the loading bar, the menu and the overlay, in the same 13 draw calls. The overlay crashed the page at first (the egui texture panic above); fixed and checked again. The pane was hidden, so frame rates there mean nothing.
- Release bunnymark at 10,000 sprites: about 1.41 to 1.43 ms a frame, level with a Phase 7 build run back to back.
- Not checked by me:
  - A real touch screen (phone or tablet), a gamepad, and mouse play on desktop.
  - The desktop window by screenshot, Firefox and Safari.
  - The Arabic wording, which a native reader should check.
  - The click sounds, by ear.

### Manual checks for the user

- Desktop: `cargo run -p ui`.
  - Move with arrows, WASD, a gamepad's d-pad or stick: the focused button turns light blue with a gold frame; Enter, Space or the south button chooses. Hover and click with the mouse.
  - Language switches to Arabic: every screen mirrors (the menu text, the settings rows, the HUD in the top right) and the text is Noto Kufi; switch back. Restart: the language is remembered.
  - Settings: the sliders change the volume and the menu-click level (left and right while focused, or drag); "Touch controls" cycles Auto, On, Off; "Debug overlay" opens it. Escape goes back.
  - Play: run and jump; the clock counts and running drains the energy bar. Escape or P pauses; Resume, Settings or Main menu.
  - F12: the overlay. Open "Entities", click one (the alien) and drag its position in the inspector; tick "UI outlines" and "Colliders".
- `cargo run -p ui -- --touch` shows the touch controls on desktop (the mouse can press them); `--demo` lets the bot click through.
- On a phone: `cd examples/ui && trunk serve --port 8087 --address 0.0.0.0`, then open the page from the phone on the same network. The d-pad, jump and pause buttons should show by themselves and work with two fingers at once; the menus work by tapping.
- Please read the Arabic strings in `examples/ui/assets/strings/ar.strings.ron` and correct any wording.

## Phase 9 — Platformer sample game

### Decisions

| Question | Answer |
| --- | --- |
| Art and levels | Kenney's Pixel Platformer (CC0), three levels: Leafy Woods (grass, a hill of slopes, planks, a spring, one moving platform), Sunset Dunes (sand, two moving platforms that meet over a pit, a wall-jump shaft, a spring up a wall) and Frosty Peaks (snow, an elevator, clouds, two platforms over spikes, a second shaft up to the summit). Each takes a skilled player under a minute; the bot finishes them in 26, 36 and 38 seconds. |
| Extra mechanics | Wall slide and wall jump; springs and moving platforms. Moving platforms needed engine work in `cg_physics` (below). No double jump or dash. |
| Lives and saving | Three hearts per level, shown in the HUD. A hit (enemy, spikes) costs a heart and knocks the player back; a fall costs a heart and returns to the last checkpoint; the last heart ends the level, and "Try again" restarts it with full hearts. Saved: unlocked levels and the best coins and time per level (`progress.ron`), with a level-select screen. |
| Controls in Settings | Rebinding: each action (left, right, jump, down, pause) has two key slots and a gamepad slot. Choosing a slot waits for a key (or a button for the pad slot); Escape cancels; a key bound elsewhere moves to its new action. Saved as `controls.ron`; Reset restores the defaults. The left stick always moves, and the menus keep their own fixed keys. |

Chosen without asking (said with the questions):

- **Levels by script.** A scratch Python script (not in the repository) wrote the three maps, the title backdrop and the tilesets from a small layout language; they are ordinary TMX files, to be edited in Tiled from now on.
- **Sound.** The meadow and heights tunes and the Phase 7 effects are reused; a scratch synthesizer (not in the repository) made a stomp, a spring, a checkpoint chime, a fall, a game-over jingle and two loops: Sunset Dunes in D Hijaz at 112 BPM (34 s) and Frosty Peaks in E Lydian at 96 BPM (40 s), both mixed circularly so they loop without a seam.
- **Languages.** English and Arabic, with the Phase 8 fonts and UI art.
- **The demo.** `--demo` lets a bot play the whole game from the title screen, as the earlier examples' bots do; CI runs it. It keeps saves in memory, so it never touches the player's.

Other choices made during the phase:

- **Moving platforms in the engine.** `PhysicsWorld` remembers where each static collider was in the previous step, so every static collider moved by game code has a motion. A body standing on one is carried by it first (vertically, then horizontally, swept so walls stop it); a collider that rose into a body from below counts as its floor if its top was within reach before it moved; a solid collider that moved into a body from the side pushes it out (swept, so a wall stops the push and the body stays inside: there is no crushing). No component marks platforms: any static collider moved in `FixedUpdate` behaves this way.
- **`OneWay` colliders.** One-way entity colliders are skipped by the horizontal and upward sweeps and the side pushes, and count as floors only while not dropping through, like one-way tiles. Rays hit them only from above. The game's platforms are one-way wooden beams, so they never crush or push the player.
- **Rebinding in the engine.** `Binding::just_pressed` captures the next key or button; `Display` names bindings for menus; `UiFocus::set_paused` stops the menus' own keys while a prompt waits. The game unpauses the focus two frames after a prompt closes, so the key that closed it (Enter, Escape) does not also click or go back.
- **`Target`** in `cg_platform` replaces a `cfg!` in the game: the title hides Quit on the web, where there is nowhere to quit to.
- **Player.** A 12×18 body: 125 units per second with 1,100 (ground) and 760 (air) units per second squared of acceleration, a 340 jump (about three tiles) cut to half when the button is let go, 0.1 s of coyote time, a 0.12 s jump buffer, a 70 wall slide, a wall jump of (165, 320) with 0.16 s before the controls steer again and 0.08 s of wall coyote time, a 480 fall cap, a 4-unit step height (so the lip of a platform not quite level with the ground is walked up, not into), and springs at 560. Down plus jump drops through planks.
- **Enemies** are tile objects of the character tileset with a class: `Walker` (turns at walls, ledges, spikes and the end of an optional `range`), `Spiker` (the same, but stomping it hurts), `Hopper` (hops every 1.3 s, towards a player within 180 units, keeping to `range` of home and off ledges) and `Bat` (flies back and forth on the level's clock, no gravity). A stomp needs the player falling with its feet within 8 units of the enemy's top; the bounce is 250, or 360 with jump held. Touches come from `Overlaps`, so standing in an enemy after invincibility ends still hurts.
- **Level objects** are tile objects with classes in `tiles.tsx`: `Coin` (animated in Tiled), `Checkpoint` (grey until reached; its sensor reaches down over the pole, since the flag tile sits a tile above the ground), `Spring`, `Exit` (the door); `Platform` rectangles with `dx`, `dy`, `speed` and `phase`, eased at the ends and moved on the level's fixed-step clock, so every run sees them in the same places; `PlayerStart`; `BotHint` rectangles.
- **Screens** are a state stack: the title (with its own scrolling backdrop map) under the level select, settings and controls; the level under the pause, level-complete, game-over and final screens. Overlays on a level set the time scale to 0. Starting, restarting and leaving levels go through `StateFade`; `set(Playing)` exits the old level (its `StateScoped` entities) and spawns a fresh one.
- **The bot.** Runs right, jumps by reflex at walls, gaps, spikes and enemies, and follows hints in the maps: `jump`, `board` (wait at an edge for a platform level with the feet), `walljump` (alternate kicks between the shaft's walls) and `right`. Standing on a platform it rides: walks to the platform's leading edge and steps off when ground or another platform is level ahead. Its rays use the world layer only, because `QueryFilter::exclude` holds one entity and excluding the platform would let the player itself be hit. The run is the same on every target and frame rate: the windowed desktop run, the headless test and the browser all finish level 1 in 26.3 s with 15 coins and one hit.
- **Versions.** No new external dependencies; the game uses `ron` and `serde` from the workspace for its save files.

### Done

- `games/platformer`: modules `level`, `player`, `enemies`, `things`, `progress`, `controls`, `menus`, `hud`, `bot`; three levels, a title backdrop, tilesets, strings in English and Arabic, art, sounds and music; a trunk page on port 8088; `--demo` and `--level N`.
- `cg_physics`: moving static colliders carry, lift and push bodies; `PhysicsWorld::motion`; `OneWay` (prefab component, Tiled `one_way` property, cyan debug drawing).
- `cg_platform`: `Binding::just_pressed`, `Display for Binding`, `Binding::is_key`, `Binding::is_gamepad`; `Target` with `can_quit`.
- `cg_ui`: `UiFocus::set_paused`, `UiFocus::is_paused`.
- 411 tests (17 more than Phase 8; two ignored captures): platforms carrying, lifting and pushing, one-way colliders and rays, binding names and capture, paused focus; the game's progress and controls, platform paths, every level's objects loading, rebinding with Escape cancelling, game over and "Try again", and the bot playing the whole game without a window (about 2 seconds). The reference PNGs are unchanged.
- CI: `platformer --demo`.

### Verified

- All tests pass; fmt, clippy (desktop and wasm) and rustdoc with warnings denied are clean.
- Desktop (Windows, GTX 1650, Vulkan, the default audio device), `platformer --demo`: title, settings, controls, the three levels (26.3 s with 15 of 45 coins and one hit; 35.8 s with 15 of 59; 38.0 s with 19 of 44), pause and resume, all seven checkpoints, the final screen and Quit, at 60 fps with vsync and 8 to 29 draw calls; exit success.
- Offscreen captures of the bot's run (`cargo test -p platformer -- --ignored capture_screens`), looked at: the title over its backdrop, settings, the controls grid, each level with the HUD and the level's name fading, the pause menu, level complete with "New best!", the final screen.
- Web (`trunk serve`, the app's browser pane, debug wasm, WebGPU), `?demo`: every asset fetched, 60 fps, and the same run as on desktop, checked by the log and by screenshot in each level.
- Not checked by me:
  - Playing by hand with a keyboard, a gamepad or touch, and rebinding a real gamepad button.
  - Listening: I checked the new sounds by loudness numbers only (the fall and game-over sounds were turned down after that).
  - Whether the levels are fun and fair for a person, and how hard they are.
  - The Arabic wording; WebGL2; Firefox and Safari; a phone.

### Manual checks for the user

- Desktop: `cargo run -p platformer`. Play the three levels:
  - Run and jump; a tap jumps low, a held button high. Jump just after running off a ledge (coyote time) and press jump just before landing (buffer).
  - Push into a wall while falling: the alien slides slowly; jump to kick off. Climb the shafts in Sunset Dunes and Frosty Peaks this way.
  - Stomp the blue walkers, the yellow hoppers and the bats; the red spikers hurt from above too. Springs launch you; planks and clouds can be jumped through from below and dropped through with Down and jump.
  - Ride the wooden beams: they carry you, and you can walk and jump on them.
  - Lose hearts to spikes and enemies (the knock back and the flash), fall into a pit (back to the last grey flag you raised), and lose all three (game over, Try again).
- Menus: Levels shows each level's best; Settings has the volume sliders, the language (the whole game in Arabic, mirrored), touch controls and Controls. In Controls choose a slot, press a key (or a button in the gamepad column); Escape cancels; Reset. Quit and run again: progress, controls, volumes and language come back. Delete `%APPDATA%\CashGear\cashgear-platformer\` to start fresh.
- `cargo run -p platformer -- --demo` watches the bot play; `-- --level 3` starts in Frosty Peaks.
- Web: `cd games/platformer && trunk serve --port 8088`, open <http://127.0.0.1:8088>; on a phone the touch controls show by themselves.
- Please read the Arabic strings in `games/platformer/assets/strings/ar.strings.ron` and correct any wording.

## Phase 10 — Web build

### Decisions

| Question | Answer |
| --- | --- |
| Hosting | GitHub Pages and itch.io. CI publishes the platformer to <https://mohamedsadat.github.io/cashgear-engine/> on every push to `main` once all jobs pass, and every run uploads the `platformer-web` artifact, whose zip has `index.html` at its root, ready to upload to itch.io by hand. One build serves both: it uses relative URLs (`--public-url ./`). |
| How assets reach the browser | One bundle file, `assets.cgpak`, made by `cg_pack` in release builds. The page starts downloading it next to the code, with one progress bar for both, and hands it to the engine. Debug builds (`trunk serve`) and desktop keep loose files and hot reloading. |
| Download size | Under 8 MB as transferred (code and text gzipped, the bundle and images as they are). CI fails the web job above it. Measured below. |
| Mobile browsers | Android Chrome and iOS Safari: the touch controls (Phase 8), a "turn your phone sideways" notice in portrait, a Fullscreen setting where the browser allows one (not on iPhone), landscape lock in fullscreen on Android, and a manifest so "Add to Home Screen" opens the game full screen in landscape. Checking on a real phone is the user's (see the manual checks). |

Chosen without asking (said with the questions):

- **Looking away pauses.** During a level, the pause menu opens when the game loses focus or is hidden: another window or tab, a minimized window, a click outside the game's frame on itch.io, a phone turned upright. Never during the demo.
- **Fullscreen** in the engine: F11 and Alt+Enter on desktop, and a Settings row in the platformer.
- **No debug overlay** in the web release (egui stays in debug builds and on desktop).
- **Errors in the page**: no WebGPU or WebGL2, a panic or a failed download shows a panel with the message and a Reload button instead of a frozen canvas.

Other choices made during the phase:

- **Release profile.** `[profile.web-release]` inherits `release` with opt-level `z`, LTO and one codegen unit. Each page picks it with `data-cargo-profile-release="web-release"`, so `trunk build --release` makes small code while desktop release builds keep full speed. trunk runs wasm-opt in every release build unless a page says `data-wasm-opt="0"`; the pages ask for level `z`. Leaving the overlay out needs `--no-default-features` on the command line (trunk has no per-profile features); the platformer's `overlay` feature is on by default for desktop and `trunk serve`.
- **The bundle format** (`cg_platform::io::Bundle`, version 1): a 16-byte header (magic `CGPAK\0`, version, the bundle's total size, the entry count), an index of paths with a method, offset, stored and unpacked length, then the data. Entries are deflated (miniz_oxide, level 9) when that saves at least 5%: maps, text and fonts shrink, PNG and OGG are stored. The same files always give the same bytes. The platformer's 42 files go from 2.54 MB to 2.19 MB (the Arabic font from 434 KB to about 214 KB, the maps to a tenth). The bundle stays in memory for the run, so reloads and late loads read from it.
- **Reading through a bundle.** `AssetIo` reads from its bundle first and falls back to the folder for files the bundle lacks (and logs which bundle it uses). On the web it waits, on its first read, for the page's `window.cashgearBundle` promise (an `ArrayBuffer`, or `null` for none); reads started before it arrives wait together for one promise, and the first to wake unpacks the index. `AssetPlugin::bundle` (`--bundle <path>`) names a bundle to read on desktop or fetch on the web when the page has not handed one over. Hot reloading is off while a bundle is in use, since reloads would read the bundle again.
- **Building the bundle.** `games/platformer/Trunk.toml` runs `cargo run -p cg_pack -- trunk-bundle` as a `post_build` hook. In release builds it packs the page's `assets/` into `assets.cgpak` in trunk's staging folder and deletes the loose copy that `copy-dir` made; debug builds keep the loose files.
- **The loading screen** (`games/platformer/web/loader.mjs`) is trunk's `data-initializer`. trunk's loader reports the code's progress against its real size; the bundle's own header carries its size, so the bar is exact even when the host gzips the download. The page also preloads the bundle from `<head>` (`<link rel="preload" as="fetch">`), and the loader's `fetch` reuses that request. The bar shows a percentage, since it counts unpacked bytes. The screen stays until the engine's first frame (`cashgear-ready`) and the bundle have both arrived, then fades. If the page's scripts never run, an inline fallback says so after 20 seconds.
- **Page events.** `cg_platform::page::ready` and `fatal` dispatch `cashgear-ready` and `cashgear-error` `CustomEvent`s on `window` (nothing off the web). The renderer sends `ready` after its first presented frame and `fatal` with its error; the web panic hook sends the panic message; an error exit sends a generic one. The page shows the first error it gets.
- **Fullscreen.** `Window::set_fullscreen` asks the runner to switch after the frame; `Window::fullscreen` is read back from winit at the start of every frame, so Escape leaving a browser's fullscreen shows up too. Browsers allow fullscreen only within a few seconds of a key press, click or touch (transient activation), which a request one frame after the click is well within. `can_fullscreen` is `document.fullscreenEnabled` on the web (false on iPhone and in frames without `allowfullscreen`), true on desktop, false on Android until Phase 11. The F11 and Alt+Enter keys are handled in the runner and not passed to the game, so Alt+Enter does not also press the focused button; on the web F11 stays the browser's.
- **Hidden tabs.** The runner restarts the frame clock when the window is hidden or shown, so a tab coming back does not run a quarter second of catch-up steps at once. The game's own pause is `menus::pause_when_away`, from `Lifecycle::Focused(false)` and `Occluded(true)`.
- **Phones in portrait.** The notice is CSS (`orientation: portrait` and `pointer: coarse`); the loader also blurs the canvas when it shows, which pauses a level through the focus rule above. In fullscreen on a touch screen the loader asks for `screen.orientation.lock("landscape")` (Android allows it; others refuse quietly).
- **CI.** The `web` job installs trunk 0.22.1 with `taiki-e/install-action`, builds, writes a size table to the job summary and uploads the artifacts; the `pages` job (`actions/deploy-pages`) runs on `main` after `fmt`, `check` and `web` pass. GitHub Pages serves `.wasm` gzipped (checked on another Pages site), so the budget counts the code at its gzip size.
- **Icons.** `web/icons` holds the manifest, Apple and favicon icons, drawn from Kenney's alien and grass on a 32-pixel grid (36 for Apple's 180-pixel icon) and scaled up by whole numbers by a scratch script (not in the repository).
- **The examples' pages** use the same profile and wasm-opt for their release builds; they keep loose assets.
- **Versions.** miniz_oxide 0.9.1 (in the tree already through `tiled`'s flate2). Actions: `taiki-e/install-action@v2`, `actions/upload-pages-artifact@v5`, `actions/deploy-pages@v5`.

### Done

- `cg_platform`: `io::Bundle`, `BundleError`, `BUNDLE_MAGIC`, `BUNDLE_VERSION`; `AssetIo::with_bundle`, `with_loaded_bundle`, `bundle`; `page` (`ready`, `fatal`, `READY_EVENT`, `ERROR_EVENT`); `Window::fullscreen`, `set_fullscreen`, `can_fullscreen`; `WindowConfig::fullscreen_keys`; the clock restart on hiding; the web panic hook's error event.
- `cg_assets`: `AssetPlugin::bundle` and `--bundle`. `cg_render`: the page events. `tools/cg_pack`: `bundle` and `trunk-bundle`.
- `games/platformer`: the web page (loading screen, error panel, rotate notice, manifest, icons), `Trunk.toml`, the `overlay` feature, the Fullscreen setting, pausing when away, and the bot's `DownTo` step.
- The `web-release` profile; every example page uses it.
- 419 tests (8 more than Phase 9, one of them `Bundle`'s doc example): bundles round-tripping, compressing only what shrinks, the same bytes every time, bad and damaged bundles, reads through a bundle with the folder filling in (in memory, from a file, missing or broken), the asset server loading a file and its dependencies through a bundle, `cg_pack bundle` leaving hidden files out, and the platformer pausing when the player looks away but not in the demo. The reference PNGs are unchanged.
- CI: the `web` and `pages` jobs.

### Verified

- All tests pass; fmt, clippy (desktop and wasm) and rustdoc with warnings denied are clean.
- Download size, from CI's web job (with wasm-opt):

  | File | Bytes | Transferred |
  | --- | ---: | ---: |
  | `platformer_bg.wasm` | 6,162,146 | 2,382,926 (gzipped) |
  | `platformer.js` | 162,176 | 24,626 (gzipped) |
  | `assets.cgpak` | 2,193,465 | 2,193,465 |
  | `index.html`, loader, manifest, icons | 32,167 | 23,053 |
  | **Total** | | **4,624,093** (budget 8,000,000) |

  wasm-opt takes the code from 11.35 MB to 6.16 MB (2.94 MB to 2.38 MB gzipped). For comparison, the debug wasm is 28.3 MB, and a release build with the overlay, before wasm-bindgen and wasm-opt, was 16.7 MB (4.4 MB gzipped).
- The release build here, served the way GitHub Pages serves it (under `/cashgear-engine/`, code gzipped) by a scratch Node server, built without wasm-opt (trunk could not download it here; see the known issues).
- The app's browser pane (Chromium, visible), checked by screenshot and script: the loading screen with its bar (on a server throttled to 600 KB/s), then the title; only the code and one `assets.cgpak` request (the preload reused), no loose files; `reading assets from the bundle` in the log; 60 fps through the bot's run on WebGPU and on WebGL2 (`?backend=gl`); the Settings screen with "Fullscreen: Off"; a changed music level saved in `localStorage` (`cashgear/cashgear-platformer/audio-settings.ron`) and back after a reload; the pause menu opening when the canvas lost focus mid-level; the error panel for `?backend=vulkan` ("cannot draw to the window"); in a phone-sized view, the rotate notice in portrait and the touch controls in a landscape level.
- Headless Chrome 156 and Edge 156 on this machine's GPU, driven by a scratch DevTools script: Chrome at 60 fps on WebGPU through the bot's run; Edge at 60 fps on WebGPU for a full minute (level 1 complete, into level 2) and at 60 fps on WebGL2 (ANGLE on Direct3D 11, the GTX 1650) while the page stayed visible (headless Edge marks a page hidden after about 20 seconds unless told not to, and the game then stops drawing, as it should); in Chrome, choosing Fullscreen in Settings with real key presses put the canvas in fullscreen and the row read "Fullscreen: On".
- Desktop: `platformer --demo` still finishes the whole game and exits with success, with the same times and coins as in Phase 9.
- GitHub Pages: CI deployed the wasm-opt build to <https://mohamedsadat.github.io/cashgear-engine/>. Pages serves the code gzipped (2.41 MB) and gzips the bundle too (2.13 MB); in the app's browser pane, `?demo` loads and plays at 60 fps.
- Not checked by me:
  - Firefox and Safari (not installed here), and a real phone: touch, the rotate notice, fullscreen and landscape lock on Android, "Add to Home Screen" on both.
  - Fullscreen in the app's browser pane, which never grants it; F11 and Alt+Enter on desktop (no screen access).
  - itch.io: uploading the artifact is yours.
  - The audio under load in browsers (cpal's Web Audio output on the main thread): I cannot hear.

### Manual checks for the user

- Open <https://mohamedsadat.github.io/cashgear-engine/> in Chrome, Edge and Firefox: the loading bar, then the title at 60 fps. Play a little; switch to another tab and back: the pause menu should be open and the music paused. Settings: Fullscreen On (Escape leaves it), change a volume and reload: it is kept.
- On a phone (Android Chrome and iPhone Safari): open the same address. Upright, the "turn your phone sideways" notice shows; sideways, the game with the d-pad, jump and pause buttons; two fingers at once should work. On Android, Settings, Fullscreen should fill the screen and stay landscape. On both, "Add to Home Screen" (Share, Add to Home Screen on iPhone) and open it from the icon: full screen, landscape.
- itch.io: download the `platformer-web` artifact from the CI run (Actions, the run, Artifacts), upload the zip as an HTML game, tick "Mobile friendly" and "Fullscreen button", and set the viewport to 1280×720. In the embedded frame, clicking outside the game should pause it.
- Desktop: `cargo run -p platformer`, then F11 and Alt+Enter go fullscreen and back; Settings, Fullscreen too. Alt-tab away during a level: the pause menu opens.
- A release build locally: `cd games/platformer && trunk build --release --no-default-features`, then serve `dist/` with any static server. If trunk times out downloading wasm-opt, install binaryen so `wasm-opt` is on the PATH; trunk uses that one.

## Phase 11 — Android build

### Decisions

| Question | Answer |
| --- | --- |
| Build tool | cargo-ndk and a Gradle project (Android Gradle plugin 9.5) in `games/platformer/android`. `cargo-apk` is deprecated and `xbuild` unmaintained; Gradle is what Google Play and Android Studio expect, and it makes the AAB. |
| Lowest Android version | Android 8.0 (API 26), where cpal's AAudio output starts. Targets Android 16 (API 36). |
| App id and name | `com.cashgear.platformer`, "CashGear Platformer". |
| Where it is tested | The user's phone over USB, and an Android 16 emulator (`cashgear_api36`, x86_64), which needs the emulator's hypervisor driver on this machine. |

Chosen without asking (said with the questions):

- **Release signing.** The keystore is yours and never in the repository. Gradle reads it from `keystore.properties` (ignored by git) or the `CG_ANDROID_*` environment variables; without either, release builds come out unsigned. CI signs the AAB only when the repository has the secrets. Debug builds use the SDK's debug key.
- **The APK carries the assets as one bundle**, `assets.cgpak`, as on the web.
- **Back** is `ui_back`: it pauses a level, goes back in menus, and quits from the title screen.
- **Landscape and always full screen** (immersive). The Fullscreen setting is hidden on Android.
- **Gamepads on Android** go to the backlog.

Other choices made during the phase:

- **NativeActivity, not GameActivity.** winit supports both; GameActivity needs Google's games library (an AAR with AndroidX) at a version matching `android-activity`'s, while `NativeActivity` is part of Android. The engine's subclass, `com.cashgear.engine.CashGearActivity`, adds what it lacks: immersive mode (hidden again whenever the window regains focus), the screen kept on, the window laid out under display cutouts, back presses counted instead of closing the app, the intent's `args` extra, and ending the process in `onDestroy`.
- **Java to Rust by polling.** The activity keeps three fields that `cg_platform::android` reads through JNI (jni 0.22): `backPresses` (a counter, so no press between two frames is lost), `safeInsets` (16 bits a side in one `long`) and `launchArgs`. The runner reads the first two once a frame. There are no native methods to register and no calls between threads.
- **Back arrives in three ways and becomes one key.** The three-button bar's back key reaches the native input queue, and the runner maps Android's `KEYCODE_BACK` (which winit passes as an unidentified key) to `KeyCode::BrowserBack`. On Android 13 and newer the back gesture calls the activity's `OnBackInvokedCallback` (the manifest opts in with `enableOnBackInvokedCallback`); on older versions the gesture sends the key. The runner taps `BrowserBack` (pressed and released in one frame) for each counted press. winit reports every key as handled, so Android never closes the activity on its own.
- **The surface.** Android destroys the native window as soon as the app is suspended, before another frame runs, so the renderer drops its surface in `LifecycleChanged` (`follow_suspend`) instead of at the next frame, and makes a new one on resume.
- **One run per process.** winit can make one event loop per process, so a second activity in the same process could not run the game. `onDestroy` ends the process; every launch starts fresh, and the game's state lives in `Storage`.
- **Display cutouts.** The window covers the whole screen (`LAYOUT_IN_DISPLAY_CUTOUT_MODE_ALWAYS` from Android 11, `SHORT_EDGES` on 9 and 10). The cutout's safe insets become `Window::safe_area`, and the camera's view, the UI's screen space and the fade fit inside it; the rest is the letterbox colour. The hidden system bars' insets are not used.
- **Assets.** Gradle stores `assets.cgpak` uncompressed in the APK (`noCompress`), so the asset manager hands it over as it is. The engine reads it whole into memory, as on the web, and `AssetIo` uses it as its bundle by itself; loose files in the APK's `assets/` are read through the asset manager.
- **Saves** go to the app's private files folder (`internal_data_path`), under the game's namespace.
- **Logs** go to logcat through `__android_log_write` (no logging crate), tag `cashgear`, one entry per line and at most 4000 bytes each. Panics are logged with a backtrace before the default hook runs.
- **Gradle.** The Variant API registers two tasks per variant whose folders become the APK's native libraries and assets: `cargoNdk<Variant>` (cargo decides what to rebuild, so it always runs; it empties its folder first, so the libraries of ABIs no longer asked for leave the APK) and `bundleAssets<Variant>` (`cg_pack bundle`). The engine's Java folder is added to the app's sources. AGP 9 adds the Kotlin standard library to every app; `kotlin.stdlib.default.dependency=false` in `gradle.properties` leaves it out, which took `classes.dex` from 2 MB to 119 KB. (Excluding it from every configuration instead also took it from Android lint, which runs on Kotlin, and broke the release build's lint step in CI.) The Gradle 9.8.2 wrapper is in the repository.
- **ABIs and profiles.** Debug APKs hold arm64-v8a (phones) and x86_64 (emulators), built with the `dev` profile; release builds add armeabi-v7a and use `[profile.android-release]` (LTO, one codegen unit, no debug info). `-PcgAbis=arm64-v8a` builds fewer. The Android Gradle plugin strips the symbols of every library it packages, debug ones too (the 509 MB debug library becomes 19 MB in the APK).
- **16 KB pages.** NDK r28 and newer link with 16 KB alignment by default, which Google Play requires for apps targeting Android 15 and newer; the libraries' load segments are aligned to `0x4000`.
- **`AppExit` as the exit code.** The platformer became a library so the Android library can call `platformer::run`; its `main.rs` and every example's `main` now return the `AppExit`, through a new `impl Termination for AppExit`. Before, `main` dropped it and the process exited with 0 even when a demo bot failed, so CI could not see a failing demo.
- **The launcher icon** is an adaptive icon whose background layer is the web favicon's art, scaled up by whole numbers by a scratch script (not in the repository).
- **Versions.** jni 0.22.4 (Android only, in the tree already through cpal), winit's `android-native-activity` feature, cargo-ndk 4.1.2, Gradle 9.8.2, Android Gradle plugin 9.5.0, NDK 29.0.14206865, JDK 17. Actions: `actions/setup-java@v6`.

### Done

- `cg_platform`: the `android` module (`run`, `app`, `AndroidApp`); on Android, assets read from the APK, `assets.cgpak` there as the bundle (`io::BUNDLE_FILE`), `Storage` in the app's files folder, logcat logging and the panic hook, `Args` from the intent's `args` extra, the back button as `KeyCode::BrowserBack`, and the safe insets; `Window::safe_area` and `with_safe_insets`; `Window::fullscreen` always true and `can_fullscreen` false on Android.
- `crates/cg_platform/android/java`: `com.cashgear.engine.CashGearActivity`.
- `cashgear_engine::android_main!`. `cg_core`: `impl Termination for AppExit`.
- `cg_render`: `Camera2D::viewport_in`, `screen_space_in`, `CameraView::viewport_in`; the view, picking and `MainCamera::screen_space` keep to the safe area; the surface follows `LifecycleChanged`. `cg_anim`: the fade keeps to the safe area. `cg_ui`: `ui_back` is bound to `BrowserBack` too.
- `games/platformer`: a library with a small `main.rs`; back pauses a level and quits from the title screen; back cancels a rebinding prompt. `games/platformer/android`: the `platformer_android` library, the Gradle project, the manifest, the launcher icon and a README.
- The `android-release` profile, the Android targets in `rust-toolchain.toml`, and the `check-android` and `clippy-android` aliases.
- 421 tests (2 more than Phase 10): the view keeping to the safe area, and `android_main!`'s doc example.
- CI: the `android` job (clippy for Android, the debug APK and the release AAB, their sizes, the `platformer-android` artifact). The Pages deploy waits for it too.

### Verified

- All tests pass; fmt, clippy (desktop, wasm and Android, aarch64) and rustdoc with warnings denied are clean.
- All ten CI demos exit with success now that `main` returns the exit code.
- The Gradle builds here, on Windows:

  | Output | Size | Inside |
  | --- | ---: | --- |
  | Debug APK | 43.8 MB | arm64-v8a and x86_64 libraries (`dev` profile), `assets.cgpak` stored, `classes.dex` 119 KB |
  | Release APK (unsigned) | 43.0 MB | arm64-v8a 14.1 MB, armeabi-v7a 10.1 MB, x86_64 15.9 MB |
  | Release AAB | 41.4 MB | the same libraries, and their symbol metadata for Play |

  The libraries' load segments are aligned to 16 KB. A release build from clean takes about 20 minutes here (three ABIs with LTO).
- Not checked yet: anything on a device. The phone was not connected during the phase, and the emulator needs the hypervisor driver, which needs an administrator. The phone test (install, play, home and back, the back button, saves after a restart, the safe area, the frame rate, the sound) is the step left before the `phase-11` tag.

### Manual checks for the user

- Build and install from `games/platformer/android` (see its README): `gradlew.bat assembleDebug`, then `adb install -r app/build/outputs/apk/debug/app-debug.apk`; or install the debug APK from the CI run's `platformer-android` artifact. The launcher shows "CashGear Platformer".
- On the phone: it opens in landscape with no status or navigation bar, and the view clears the camera cutout. Play a level with the touch controls. Press Home, wait, and come back: the pause menu is open and the music is paused; resume and play on. Turn the screen off and on: the same. Back pauses a level, goes back in menus, and closes the game from the title screen.
- Change a setting (a volume, the language), close the game from the title and open it again: the setting is kept.
- With music playing: plug or unplug headphones, and take a phone call; the music should go quiet for the call and come back after.
- `adb logcat -s cashgear` shows the engine's log, and `adb shell am start -n com.cashgear.platformer/com.cashgear.engine.CashGearActivity --es args "--demo"` lets the bot play the whole game.
- For Google Play: make an upload keystore with `keytool` (the README has the command), write `keystore.properties`, and run `gradlew.bat bundleRelease`. For CI to sign, add the secrets `CG_ANDROID_KEYSTORE_BASE64`, `CG_ANDROID_KEYSTORE_PASSWORD`, `CG_ANDROID_KEY_ALIAS` and `CG_ANDROID_KEY_PASSWORD` to the repository. Uploading to the Play Console is yours.
- For the emulator on this machine: run `D:\Android\sdk\extras\google\Android_Emulator_Hypervisor_Driver\silent_install.bat` as an administrator once, then `emulator -avd cashgear_api36`.

## Known issues

- On the web, `AppExit` stops the loop but leaves the page; there is nowhere to exit to.
- Within one layer, sprites are grouped by texture, so overlapping sprites with different textures draw in texture order, not queue or spawn order. Use `Layer` when the order matters. A z or y-sort key can be added if the platformer needs one.
- Moving an `Interpolated` entity in `Update` instead of `FixedUpdate` draws it blended between its last fixed-step position and the new one. Move interpolated entities in fixed steps, or call `Interpolated::reset`.
- A child whose parent has no `Transform2D` keeps a stale `GlobalTransform2D`. Give every parent in a hierarchy a transform.
- `MainCamera` uses the camera's un-interpolated transform, so pointer picking can be off by up to one fixed step of camera motion.
- `Assets<Image>` keeps every image in CPU memory as well as on the GPU, so a lost device can re-upload.
- On desktop, files are read and decoded on the main thread inside the asset update, so a large asset makes that frame long. A worker thread for desktop loads can come with the Phase 14 performance pass.
- An example's executable run directly (not through `cargo run`) finds its assets only if they are next to it or `CG_ASSET_ROOT` points at them.
- Hot-reloading a map respawns its objects, so they lose their state; hot-reloading a prefab changes only entities spawned afterwards.
- `cg_tiled` supports orthogonal maps with single-image tilesets. Image-collection tilesets draw nothing (with a warning); isometric and hexagonal maps, and text objects (spawned as plain rectangles) are not supported yet.
- Tilemaps are drawn at their entity's `GlobalTransform2D` without interpolation, so a map moved in fixed steps would step on fast displays. Maps normally stay put.
- Tile chunks are cached as queued sprites on the CPU and copied into the sprite queue each frame, not kept as GPU buffers. At the example's size that is about 320 tiles a frame; revisit if a level gets much larger.
- Browsers hide the adapter name behind WebGPU, so `RenderStats::adapter()` reads "an unnamed GPU" there.
- With 20,000 bunnies in a debug wasm build, WebGL2 drops to 30 fps. Bunnymark's page now builds releases with the web profile, but that has not been measured yet (Phase 14).
- Kinematic bodies are axis-aligned boxes and collide with every collider as its bounding box; rotated rectangles stay axis-aligned.
- Slopes carry a body by its foot, so its corners hang over or into the hill; slopes block only from above, so their high side should meet a solid tile or another slope. Ceiling slopes (tiles flipped vertically) collide as solid blocks. A body wider than twice a slope's run climbs it by its corner.
- Hazard tiles hurt over their whole cell, so a body jumping just over Kenney's low spikes can be hit by the empty top of the cell.
- Bodies do not block each other. Static colliders moved by code carry, lift and push bodies, but nothing crushes: a body pushed into a wall, or pressed into a ceiling by a rising platform, ends up inside the collider. Keep moving solid colliders away from walls, or use `OneWay` ones.
- A body carried by a platform leaves it with only its own velocity; the platform's speed is not added to jumps.
- `PhysicsWorld::motion` compares a collider with where it was in the previous step, so a static collider teleported by code also carries what stands on it, by the whole jump.
- `QueryFilter::exclude` holds one entity; to leave out two (the asker and the platform it stands on), filter by layer.
- Overlaps are checked once per step. A fast body is tested by its swept box, which on a diagonal move also covers corners it never crossed.
- Queries (`SpatialQuery`) see colliders where they were at the end of the latest physics step, not changes made since.
- Hot-reloading a map rebuilds its collision and respawns its objects; in the `collision` example that restarts the run.

- Clips, animators and fixed-step tweens run after the physics in `FixedPostUpdate`, so parameters set in `FixedUpdate` take effect in the same step, but events reach `FixedUpdate` one step later.
- The engine's animation systems run in every game state. A game paused through its state stack (not `Time::set_time_scale(0.0)`) keeps animating sprites, moving particles and decaying shake underneath, as the example's gallery shows.
- A tween's time past its end is not carried into its `then` follow-up (at most one frame of drift per link). Rotations tween numerically, not the short way round.
- Flipping an animated sprite mirrors it around `SpriteAnimation::pivot`, unlike a plain sprite, which mirrors inside its quad.
- A tile object of an animated Tiled tile that also gets a `SpriteAnimation` (from a prefab, say) has two things setting its sprite; remove its `TileAnimation`.
- Particles are simulated on the CPU and queued as one sprite each. With about 11,000 alive a dev build still ran at 400 fps on the test machine, but a much larger budget would want GPU instancing of its own.
- `CameraFollow` assumes the camera has no parent. Shake and bounds apply to the first active camera with them.
- The screen fade covers the camera's view; with letterbox or pixel-perfect scaling, the bars stay black whatever the fade colour.
- `Camera2D::zoom` is not interpolated, so a zoom changed in fixed steps would step on fast displays; zoom tweens therefore run every frame.
- The Aseprite loader reads one image per sheet and ignores slices, layers and cel user data.

- Streams are decoded on the main thread, two seconds ahead. A frame that takes longer than that (a very large asset loading on desktop) runs a stream dry for a moment of silence, counted in `AudioStats::underruns`.
- With `pause_when_hidden` (the default) all sound pauses while the window is hidden, which on desktop includes a minimized window and, on some systems, one fully covered by others.
- On the web, one-shot sounds asked for before the user's first input are skipped; music and looping sounds wait for it.
- Positioned sounds pan by horizontal offset and fade by 2D distance from the camera's position before shake; there is no Doppler or occlusion, and positions update once a frame.
- `Audio::is_playing` knows only voices started with `play`; music and emitters are the engine's (use `Audio::music`). An emitter skipped by the voice caps is not retried until its component changes.
- `AudioEmitter` is not a prefab component yet; the example turns Tiled objects into emitters in game code.
- Whether a sound is streamed depends only on its length (`stream_longer_than`); there are no per-file loader settings.
- `Storage` is synchronous and meant for small values.

- UI nodes are not clipped: children and text that overflow a node draw outside it, and there are no scrolling panels.
- Hidden nodes keep their space in the layout; there is no "display: none". Despawn or move a node to take it out.
- The layout runs over every node each frame (text is cached). Menus and HUDs cost nothing measurable; a UI of thousands of nodes would want dirty tracking.
- Images do not mirror in right-to-left layouts; an arrow icon points the same way in Arabic.
- Bitmap fonts have no shaping and run left to right only, so Arabic needs an outline font. `TextAlign::Justified` works for outline fonts only.
- Pixel fonts are sharp only at multiples of their pixel grid (11 UI pixels for the built-in Pixelify Sans); other sizes threshold unevenly.
- Smooth fonts are rasterized at the screen's scale, so resizing the window rasterizes their glyphs again. The glyph atlas holds up to four 1024×1024 pages (16 MB in memory and on the GPU when full); when they fill, it starts again and every text is laid out once more.
- There is no text in world space (a `Text2d`); text is UI only.
- A first touch shows the touch controls but does not press one; the next touch does.
- The egui overlay gets the engine's input: no IME, clipboard or cursor shapes. Keys typed into its fields still reach the game. egui draws into the sRGB frame, so its blending differs a little from egui's own look.
- Focus moves by geometry only (plus `auto_focus`); there is no explicit tab order.
- The locale starts in the saved language or the default one; it does not read the system's language. Strings have `{name}` arguments but no plural rules.

- The platformer's controls screen rebinds keys and gamepad buttons, not gamepad axes or the menus' own keys; Escape cannot be bound from the prompt (it cancels), so a pause slot cleared of Escape gets it back only through Reset.
- The platformer's bot passes its levels by hints placed in the maps and by deterministic timing; a level edited in Tiled may need its hints moved. A run that loses a level fails the demo at once.
- Platformer enemies are kinematic bodies with simple rules; hoppers and walkers stay within `range` only when the map gives one, and an enemy that falls out of the level is removed.
- The platformer loads all three maps and every sound before the title screen; music streams. Fine at this size, but a larger game would load levels on demand.

- trunk gives each tool download 30 seconds; on a slow connection the wasm-opt download (binaryen) times out and the release build fails. Install binaryen so `wasm-opt` is on the PATH, or build a copy of the page with `data-wasm-opt="0"`.
- `trunk build --release` without `--no-default-features` builds the platformer with the debug overlay (about a megabyte more). CI passes the flag.
- The bundle has a fixed name, `assets.cgpak`. Right after a deploy, a browser or a host's cache (GitHub Pages keeps files for 10 minutes) can pair new code with the old bundle, or the reverse. A content hash in the name would fix it.
- The whole bundle downloads before the title screen, music included (1.8 MB of its 2.2 MB), and stays in memory for the run next to the decoded assets.
- Web fullscreen works only shortly after a key press, click or touch; a game that calls `Window::set_fullscreen` from a timer is refused, and `Window::fullscreen` turns false again. Safari on iPhone has no fullscreen for pages (`can_fullscreen` is false); "Add to Home Screen" is the way there. Landscape lock works only in fullscreen and only where the browser allows it (Android).
- The rotate notice and the page's loading and error texts are in English (the notice also in Arabic); the page does not know the game's language.
- On desktop, the platformer pauses whenever its window loses focus during a level, even for a moment.

- Android has no gamepads yet: winit's Android backend and gilrs do not read them. Touch controls only.
- The game runs once per process: the activity ends the process when Android destroys it (the game quit from the title, the task swiped away), so nothing in memory survives that; the platformer keeps its saves in `Storage`. A launch while the game runs (`singleTask`) brings it forward, and a new intent's `args` extra is ignored.
- Only display cutouts are kept clear; rounded screen corners can clip the corners of the HUD a little.
- Panic backtraces in logcat show addresses, not function names, since the Android Gradle plugin strips the libraries it packages. The panic's message and place are there; `ndk-stack` with the unstripped libraries in `app/build/generated/jniLibs` names the frames.
- A release build of the three ABIs takes about 20 minutes from clean, here and in CI.
- Without a keystore, release builds (and CI's AAB without the secrets) are unsigned; Google Play takes only signed ones.
- There is no soft keyboard (nothing in the engine takes typed text yet).

## Notes for later phases

- **Any phase:** `trunk` only watches the example folder, so touch the example after editing engine crates while `trunk serve` runs.
- **Phase 9:** build the player controller on `KinematicBody` (coyote time from `just_left_ground`, jump buffer, variable jump as in `examples/collision`). Enemies can be kinematic bodies on their own `CollisionLayers`; stomps come from `CollisionStarted` plus comparing velocities or positions. Moving platforms need carrying (backlog). Tag tiles in the platformer's tileset with the same `solid`/`one_way`/`slope_left`/`slope_right`/`hazard` properties and copy `slopes.png`/`slopes.tsx` from `examples/collision`.
- **Phase 12:** an `on_collision` Lua hook can be fed from `CollisionStarted`/`CollisionEnded`.
- **Phase 13:** the editor can draw colliders through `PhysicsDebug` and pick entities with `SpatialQuery::overlap_point`.
- **Phase 13:** add `bevy_reflect` derives for the inspector; keep serde for prefab files. `Prefab::to_ron` already writes prefabs, but capturing a live entity back into a prefab still needs a `to_data` direction on `PrefabComponent`.
- **Phase 8:** egui will need its own pass after the debug shapes. `Painter::render` is the place to add it.
- **Phase 10:** measure a release wasm build (`trunk build --release`, `wasm-opt`). The debug wasm is about 10 MB.
- **Phase 8:** on-screen buttons can press actions through a new `Binding` variant or by driving `ScreenRegion` touches.
- **Phase 9:** the Kenney Pixel Platformer sheets and license are in `examples/tiled_map/assets`; the scratch generator that built the level is not in the repository, so new levels are made in Tiled.
- **Phase 11:** add `aarch64-linux-android` to `rust-toolchain.toml` targets and a CI job. Fill in the Android branch of `cg_platform::io` (APK assets through `android-activity`'s asset manager); file watching stays desktop-only.
- **Phase 12:** `cg_script` will not build for wasm; keep it out of `check-wasm` or behind the `script` feature.

- **Phase 8:** tweens know only transforms, sprites and camera zoom; UI widgets will need their own tween properties (or a generic lens). `ScreenFade` is a sprite on layer `i32::MAX`, so UI drawn in its own pass after the sprites shows over the fade; decide whether menus fade with the game.
- **Phase 9:** reuse `examples/animation`'s controller, `alien.anim.ron`, dust prefab and hit flash for the player, `StateFade` for menu and level changes, and Tiled tile animations for coins, water and flags in the platformer's tileset. Enemies can each have an `Animator`.
- **Phase 10:** measure the particle gallery in a release wasm build with the tab in front, on WebGPU and WebGL2.
- **Phase 13:** an animation preview (clips of a sheet, a graph's states) would suit the editor; clip events and particle settings are plain data already.

- **Phase 8:** interface sounds belong on `Bus::Ui`. A settings screen edits `AudioSettings` (levels 0 to 1, `step_level`, `muted`); it saves itself. Move the audio example's volume panel into the UI layer.
- **Phase 9:** reuse `examples/audio`: footsteps from animation events, landing volume from the fall, coin runs, `Ambient` and `MusicZone` objects in Tiled, and `play_music` per level (the same track keeps playing across a restart). Build the settings menu on `AudioSettings`. Set `StoragePlugin::namespace` to the game's id; save games go through `Storage` too.
- **Phase 10:** measure what kira, cpal and symphonia add to a release wasm build. cpal's Web Audio backend runs its callback on the browser's main thread; check for crackles under load, and consider cpal's AudioWorklet support (it needs shared memory, so COOP/COEP headers) if there are.
- **Phase 11:** test cpal's Android output (AAudio) and pausing through `LifecycleChanged` on a device, with headphones and a phone call. Fill in `Storage` for Android (the app's internal data folder).
- **Phase 12:** Lua bindings for `Audio` (play, music, settings).
- **Phase 14:** decode streams on a worker thread on desktop, with the asset loads.

- **Phase 9:** build the menus from `examples/ui`: the `panel`, `button` and `slider_row` helpers, `StateImages` for button frames, `Localized` text with `strings/*.strings.ron`, a settings screen on `AudioSettings` and `TouchControls::mode`, a HUD with a bitmap font, and `TouchDpad`/`TouchButton` controls in a left-to-right root. Pausing pushes a state and sets the time scale to 0; menus still navigate on real time. Check `UiPointer::over_ui` before using clicks in the world.
- **Phase 10:** measure what cosmic-text, egui and the embedded font add to a release wasm build, and consider building web releases without the `debug-overlay` feature. Try the touch controls in mobile browsers (`Window::touch_screen` comes from `(pointer: coarse)`).
- **Phase 11:** map the Android back button to `ui_back`; keep UI anchors inside safe-area insets (a margin on `ScreenSpace` or the root nodes).
- **Phase 12:** Lua bindings for UI text, buttons (`Clicked`) and the locale.
- **Phase 13:** the editor can draw with egui through `RenderOverlays` and the same `EguiContext` input feeding; the overlay's `Inspector` registry is the stand-in until `bevy_reflect`.
- **Phase 14:** profile the layout and text shaping with many nodes; text in world space if a game needs it.

- **Phase 10:** the platformer is the web build's target (`games/platformer`, port 8088). Measure its release wasm and assets (about 2.7 MB of assets now, 1.8 MB of it music). Touch controls show on touch screens; check the d-pad's `down` and the pause button on a phone. The title hides Quit on the web through `Target::can_quit`.
- **Phase 11:** map the Android back button to `ui_back` (the platformer's menus and pause already read it); `Target::Android` exists; the platformer's saves (`progress.ron`, `controls.ron`, audio settings, language) need the Android `Storage`.
- **Phase 12:** the plan ports one enemy and the checkpoint logic to Lua: the platformer's `enemies::think` (walker branch) and `things::touch_things` (checkpoint branch) are the Rust versions to match, and `bot` checks the result.
- **Phase 13:** the editor can place the platformer's objects: tile objects with classes (`Coin`, `Checkpoint`, `Spring`, `Exit`, the enemies) and `Platform` rectangles with `dx`, `dy`, `speed`, `phase`. The level script is gone; Tiled and the editor are the tools now.
- **Phase 14:** a release of the sample game; a `cargo generate` template can start from `games/platformer`'s layout (modules by subject, a `Game` plugin the tests build without a window, a bot for CI).

- **Phase 11:** an APK can carry the assets as one bundle too: read `assets.cgpak` through the APK asset manager into `AssetIo::with_loaded_bundle`, and `cg_pack bundle` can make it in the Android build. `Window::can_fullscreen` is false on Android until immersive mode; set it there. `page` events are web only. The platformer already pauses on `Lifecycle::Focused(false)` and `Occluded(true)`; check what Android sends on backgrounding.
- **Phase 12:** keep `cg_script` out of the web build (the `script` feature); the web release's size budget is 8 MB.
- **Phase 14:** a content-hashed bundle name (trunk hashes the code but copies the bundle as is), a service worker for offline play, and per-level bundles (or music outside the bundle, streamed) if the download grows. Measure the web build's frame time with many sprites (bunnymark's release page now uses the web profile). The CI size table is the place to watch.

- **Phase 12:** check that `mlua` with vendored Lua builds for Android through cargo-ndk (it compiles C with the NDK's clang); Luau is C++ and needs the NDK's C++ library linked. Keep `cg_script` behind the `script` feature so `clippy-android` and the APK build stay green until it does.
- **Phase 13:** the editor is desktop only; `check-android` and `clippy-android` leave out `cg_editor` and `cg_pack`.
- **Phase 14:** for a Play release: the version code from the tag, a signed AAB from CI, the libraries' debug symbols for Play's crash reports (`debugSymbolLevel`), a foreground layer for the launcher icon, and the frame rate and battery on a mid-range phone. The `cargo generate` template can include `games/platformer/android`.

## Backlog

Features that came up but don't serve the platformer or a listed example go here.

- Several cameras drawn at once (split screen, minimap), each with its own viewport and order.
- A z or y-sort key within a layer.
- Multi-threaded system execution on desktop, if profiling ever calls for it. At 10,000 entities the ECS work is about 0.6 ms a frame single-threaded.
- Packing atlases at runtime (Phase 4 packs them with `cg_pack`), and per-load loader settings (an image's filter, say).
- Image-collection tilesets, and isometric or hexagonal maps in `cg_tiled`.
- `rapier2d` behind a feature, for dynamic rigid bodies (tumbling crates, ragdolls).
- Crushing (a body squeezed between a moving collider and a wall), and platform velocity carried into jumps.
- Partial-cell tile shapes from Tiled's tile collision editor (low spikes, half blocks), and ceiling slopes.
- Rotated (oriented) boxes and polygon colliders.
- Outlines as a sprite effect (sampling neighbouring texels; sheets need padding).
- Generic tween lenses for any component; interpolated camera zoom.
- GPU particles, or instanced particle buffers, if the CPU particles ever become the bottleneck.
- Aseprite slices (per-frame pivots and hitboxes) and multi-layer exports.
- Camera look-ahead in the direction of travel.
- Audio effects through kira's track effects: reverb, a low-pass for pause menus or underwater, ducking music under dialogue.
- Doppler, occlusion, and listener orientation; beat-synced events with kira's clocks.
- Seeking and loop regions in streams; audio sprites (several sounds in one file); per-file loader settings (force streaming).
- MP3 and FLAC (symphonia features), if a game needs them.
- `AudioEmitter` as a prefab component, for emitters set up in prefabs and Tiled properties.
- UI: clipping and scroll panels, text fields (typing with `TextInput`), checkboxes and dropdowns, tween properties for nodes (offset, opacity), rich text spans, plural rules, reading the system language, `Text2d` in world space, mirroring images in right-to-left layouts.
- A `cg_pack font` command that bakes a TTF into a BMFont, for hand-editing glyph art.
- `QueryFilter` excluding several entities.
- Platformer extras: more levels, a double jump or dash, collectible gems, enemies that throw things, a level timer leaderboard.
- Web: uploading to itch.io from CI with butler (needs an API key secret), a service worker for offline play, content-hashed bundle names, the page following the game's language, and cpal's AudioWorklet output (needs cross-origin isolation headers, which GitHub Pages cannot set).
- Android: gamepads, a soft keyboard for text fields, GameActivity, Android TV and ChromeOS, keeping clear of rounded screen corners, and Play Asset Delivery if the bundle grows.
