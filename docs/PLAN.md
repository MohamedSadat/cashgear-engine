# CashGear Engine — Implementation Plan

Oct 24, 2026 · @Sadat

## Overview

CashGear Engine is a Rust 2D game engine for desktop, web and Android, proven by a sample platformer, built in 15 phases by a code agent. Rough total: 2–3 months, with the scene editor as the largest single phase.

| Decision | Choice |
| --- | --- |
| Name | CashGear Engine (crates prefixed `cg_`) |
| Language | Rust stable, edition 2024 |
| Platforms | Windows, Linux, macOS, Web (WASM), Android — all designed in from phase 1 |
| Proof game | 2D platformer sample (`games/platformer`) |
| Gameplay code | Rust first; Lua scripting added late |
| Tools | Tiled for levels, Aseprite for sprites, own scene editor late |
| Branching | Every phase committed and pushed to `main` when it ends |

### Tech stack

| Concern | Crate / tool | Notes |
| --- | --- | --- |
| Window, input, lifecycle | `winit` | Desktop, web canvas, Android via `android-activity` |
| Rendering | `wgpu` | Vulkan/DX12/Metal; WebGPU with WebGL2 fallback; GLES/Vulkan on Android |
| Math | `glam` | Vec2, Mat3/Affine2, SIMD |
| ECS | `bevy_ecs` (standalone) | Systems, resources, events, schedules; confirmed in phase 3 |
| Images | `image` | PNG decode |
| Tilemaps | `tiled` | TMX/TSX via a custom reader for web and APK |
| Collision | own AABB + tile sweep; `rapier2d` optional | Platformer-grade kinematics first |
| Audio | `kira` | Buses, streaming music, tweens |
| Text | `glyphon` / `cosmic-text` | Shaping, including Arabic if wanted |
| Gamepad | `gilrs` | Desktop; web via Gamepad API |
| Debug UI | `egui` + `egui-wgpu` + `egui-winit` | Overlay and later the editor |
| Data files | `serde` + `ron` | Prefabs, scenes, settings |
| Scripting | `mlua` (Lua 5.4 / Luau) | Web strategy decided in phase 12 |
| Profiling | `puffin` (+ Tracy optional) |  |
| Web tooling | `trunk`, `wasm-bindgen`, `wasm-opt` |  |
| Android tooling | `cargo-ndk` + Gradle (Android Gradle plugin) | Chosen in phase 11: `cargo-apk` is deprecated and `xbuild` unmaintained |
| Logging / errors | `log`, `env_logger`, `console_log`, `thiserror`, `anyhow` |  |

## Agent workflow rules

Every phase runs in its own fresh session, starts with questions, and ends with a push to `main` plus a card for the next phase.

### At the start of every phase

1. Read `docs/PLAN.md` (this plan), `docs/PROGRESS.md` and `CHANGELOG.md`.
2. Run `cargo build`, `cargo test` and the wasm check to confirm `main` is green before touching anything.
3. Ask 2–5 questions about the phase's open decisions (each phase below lists suggested ones). Give a recommended option for each. Wait for answers before writing code.
4. Write a short phase task list and work through it.

### While working

- Keep the portability rules in the Architecture section; every PR-sized step must still build for `wasm32-unknown-unknown`.
- Add or update an example under `examples/` that shows the new feature running.
- Prefer existing crates over hand-written layers; ask before adding a heavy dependency.
- For visual features, describe what should appear and ask the user to confirm with a screenshot when the result can't be tested automatically.

### At the end of every phase

1. `cargo fmt --all`, `cargo clippy --workspace --all-targets -- -D warnings`, `cargo test --workspace`.
2. `cargo check --target wasm32-unknown-unknown` (and the Android target from phase 11 on).
3. Update `docs/PROGRESS.md` (what's done, decisions made, known issues) and `CHANGELOG.md`.
4. Commit with `Phase N: <title>` and push to `main`. Tag `phase-N`.
5. Finish with the next-phase card below, filled in, so it can be pasted into a fresh session.

### Next-phase card template

```markdown
### Next: Phase N — <title>
Goal: <one line>
Depends on: Phase N-1 (pushed to main, tag phase-N-1)
Key decisions to ask about: <2-3 items>
Prompt for the fresh session:
> Read docs/PLAN.md and docs/PROGRESS.md. Start Phase N of CashGear Engine.
> Ask me your questions first, then implement, then push to main and give me the next-phase card.
```

## Architecture

The engine is one Cargo workspace of small crates behind a single facade crate, with all platform-specific code confined to `cg_platform`.

| Crate / folder | Responsibility | Phase |
| --- | --- | --- |
| `cg_core` | App builder, plugin trait, fixed-timestep loop, `Time`, events, logging, re-exports `glam` | 1 |
| `cg_platform` | `winit` glue, input state, file/asset IO per target, storage, lifecycle | 1 |
| `cg_render` | `wgpu` context, surface handling, sprite batcher, camera, textures, debug shapes | 2 |
| `cg_world` | `bevy_ecs` integration, transforms, hierarchy, schedules, game states, prefabs | 3 |
| `cg_assets` | Handles, async loaders, atlases, hot reload | 4 |
| `cg_tiled` | Tiled map loading, chunked tile rendering, object-to-entity spawning | 4 |
| `cg_physics` | AABB/tile collision, layers, sensors, raycasts; optional `rapier2d` feature | 5 |
| `cg_anim` | Sprite animation clips, state machines, tweens, particles | 6 |
| `cg_audio` | `kira` wrapper, buses, positional pan | 7 |
| `cg_ui` | Text, runtime game UI, touch controls, egui debug overlay | 8 |
| `cg_script` | Lua runtime and bindings | 12 |
| `cashgear_engine` | Facade crate, `prelude`, `DefaultPlugins` | 1 |
| `examples/` | One runnable example per feature | every phase |
| `games/platformer` | Sample game proving the engine | 9 |
| `tools/cg_editor` | Desktop scene editor (binary) | 13 |
| `tools/cg_pack` | CLI: atlas packing, asset bundling | 4 |
| `docs/` | `PLAN.md`, `PROGRESS.md`, mdBook guide | 0 |

### Portability rules (apply from phase 1)

1. Only `cg_platform` (and the web/Android entry points) may use `#[cfg(target_os/arch)]`.
2. All file access goes through one async asset IO API: filesystem on desktop, `fetch` on web, APK assets on Android.
3. No `std::thread`, `std::fs` or blocking IO in engine crates. Background work goes through a small task abstraction that runs inline on web.
4. Mouse and touch are one "pointer" input; keyboard, gamepad and on-screen controls map to named actions.
5. The renderer handles surface lost/recreated and app suspend/resume.
6. Game logic runs in the fixed-timestep schedule; rendering interpolates.
7. Saves go through a `Storage` trait: file on desktop, `localStorage` on web, app data on Android.
8. CI checks `wasm32-unknown-unknown` from phase 0 and `aarch64-linux-android` from phase 11.

## Phases 0–5: core engine

These phases produce a playable core: a window on all targets, fast sprite rendering, ECS, assets with Tiled maps, and collision.

### Phase 0 — Repository and workspace

**Goal:** an empty, green workspace with CI and docs.

- Cargo workspace with the crate skeletons above, `cashgear_engine` facade and `prelude`.
- `rust-toolchain.toml`, `rustfmt.toml`, `clippy.toml`, `.gitignore`, license file.
- GitHub Actions: fmt, clippy `-D warnings`, test, `cargo check --target wasm32-unknown-unknown` on Windows and Ubuntu.
- `docs/PLAN.md` (this plan), `docs/PROGRESS.md`, `CHANGELOG.md`, `README.md`.

**Ask:** license (MIT/Apache-2.0 dual, proprietary, other); public or private repo; minimum supported Rust version.

**Done when:** CI is green on `main`.

### Phase 1 — Platform layer and game loop

**Goal:** a window that runs the same loop on desktop and web.

- `App` builder with `Plugin` trait; `winit` event loop with `ApplicationHandler`.
- Fixed-timestep loop (accumulator, default 60 Hz, max catch-up steps) with an interpolation alpha for rendering.
- `Time` resource: delta, fixed delta, elapsed, frame count, time scale.
- Input: keyboard, mouse, unified pointer/touch, `gilrs` gamepad; pressed/just-pressed/released states; action mapping (`"jump"` → Space, gamepad A, touch button).
- Lifecycle events: resumed, suspended, focus lost, resize, scale-factor change.
- Logging to console on desktop and browser.
- Web entry point with `trunk` (`index.html`, canvas fills page).

**Ask:** default fixed rate (60 Hz?); should the loop cap FPS or rely on vsync; action map format (code or RON file).

**Done when:** `examples/hello_window` opens, logs input, and runs in the browser via `trunk serve`.

### Phase 2 — Renderer core

**Goal:** thousands of sprites at 60 FPS on every target.

- `wgpu` instance, adapter, device, surface; WebGPU with WebGL2 fallback; surface-lost recovery.
- Texture upload, samplers (nearest for pixel art, linear option), mipmaps off by default.
- Sprite batcher: instanced quads, sort by layer then texture, one draw per texture batch; tint, flip, rotation, pivot, source rect.
- 2D orthographic camera: position, zoom, rotation; world-to-screen and screen-to-world.
- Virtual resolution modes: stretch, letterbox, integer pixel-perfect scaling.
- Debug shape renderer: lines, rects, circles.
- Render stats: draw calls, sprites, frame time.

**Ask:** default virtual resolution (e.g. 320×180 or 640×360 for pixel art); pixel-art first or smooth art first; colour space (sRGB) confirmation.

**Done when:** `examples/bunnymark` draws 10,000+ moving sprites at 60 FPS on desktop and runs on web.

### Phase 3 — ECS and scenes

**Goal:** game code is written as components and systems.

- Integrate `bevy_ecs` standalone; `World`, resources, events, commands.
- Schedules: `Startup`, `PreUpdate` (input), `FixedUpdate` (gameplay), `Update`, `PostUpdate` (transforms), `Render`.
- `Transform2D`, `GlobalTransform2D`, parent/child hierarchy and propagation.
- `Sprite`, `Camera2D`, `Visibility`, `Layer` components feeding the batcher; frustum culling.
- Game state stack (`Menu`, `Playing`, `Paused`) with enter/exit systems.
- Prefabs: RON files describing entities and components.

**Ask:** confirm `bevy_ecs` vs `hecs` (recommended: `bevy_ecs` for schedules and events); prefab format details; whether to add `bevy_reflect` now (helps the editor later).

**Done when:** `examples/ecs_sprites` spawns, moves and despawns entities from a prefab, and switches states.

### Phase 4 — Assets and tilemaps

**Goal:** load everything asynchronously on all targets and draw Tiled levels.

- Asset server: typed `Handle<T>`, reference counting, load states, dependency loading, error reporting.
- Platform IO backends: filesystem, `fetch`, APK assets (stub until phase 11).
- Hot reload on desktop (`notify`) for textures, prefabs and maps.
- Texture atlases: `tools/cg_pack` CLI packs a folder into atlas PNG + RON; runtime atlas lookups by name.
- `cg_tiled`: TMX/TSX loading, tile layers drawn as cached chunk meshes, parallax and image layers, object layers spawning entities by type, custom properties to components.
- Loading screen state that waits for asset groups.

**Ask:** Tiled TMX or JSON export; asset folder layout; whether atlases are packed at build time or runtime.

**Done when:** `examples/tiled_map` loads a Kenney tileset level with a scrolling camera on desktop and web; editing the map on desktop updates it live.

### Phase 5 — Collision and physics

**Goal:** platformer-quality movement and collision.

- Colliders: AABB and circle; collision layers and masks; sensors/triggers with enter/exit events.
- Tile collision from Tiled: solid, one-way platforms, slopes (45° and 22.5°), hazard tiles.
- Kinematic body with swept movement per axis, ground/wall/ceiling contact flags.
- Spatial hash broadphase for entity-entity checks; raycasts and shape queries.
- Debug draw of colliders and contacts (toggle key).
- Optional `rapier2d` feature for dynamic rigid bodies.

**Ask:** slopes needed in v1; units (pixels or metres); include `rapier2d` now or later.

**Done when:** `examples/collision` has a box character running, jumping on one-way platforms and slopes without tunnelling at high speed; collision unit tests pass.

## Phases 6–11: features, sample game and platforms

These phases complete the runtime, prove it with the platformer, and ship it to web and Android.

### Phase 6 — Animation and effects

**Goal:** characters and scenes feel alive.

- Sprite animation clips from Aseprite JSON exports (frames, durations, tags, loop modes).
- Animation state machine component with transitions driven by parameters (speed, grounded).
- Animation events (e.g. footstep on frame 3).
- Tweens with easing functions for position, scale, rotation, colour.
- CPU particle emitters (burst and continuous, gravity, colour/size over life), batched draw.
- Camera: smooth follow with dead zone, level bounds, shake, zoom tween; fade transitions between states.

**Ask:** Aseprite JSON as the standard format; particle budget; any shader effects wanted (flash on hit, outline).

**Done when:** `examples/animation` shows an animated character with a state machine, dust particles and camera shake.

### Phase 7 — Audio

**Goal:** sound effects and music on all targets.

- `kira` manager in `cg_audio`; `Handle<Sound>` through the asset server (OGG, WAV).
- Buses: master, music, sfx, UI; volume and mute settings saved through `Storage`.
- Streaming music with crossfade; one-shot and looping SFX; pitch variation.
- Simple 2D positional audio: pan and distance attenuation from the camera.
- Web: unlock audio on the first user input; Android: pause and resume with the app.

**Ask:** audio formats to support; max simultaneous sounds; whether settings menu is needed now or in phase 9.

**Done when:** `examples/audio` plays music and positional SFX on desktop and web.

### Phase 8 — Text and UI

**Goal:** readable text, menus, HUD and a debug overlay.

- Text rendering with `glyphon`/`cosmic-text`: TTF/OTF fonts, sizes, colour, alignment, wrapping, bitmap-font option for pixel art.
- Runtime UI: panel, label, image, button, slider, progress bar, anchors and percentage layout, UI scale with screen size.
- Focus navigation by keyboard and gamepad; pointer and touch hit testing.
- On-screen touch controls (virtual d-pad, buttons) mapped to input actions; shown only on touch devices.
- egui debug overlay: FPS graph, entity list and component inspector, collider toggle, render stats.
- Localization: string tables in RON keyed by ID.

**Ask:** Arabic and right-to-left text in v1; UI style (pixel or smooth); which languages to ship.

**Done when:** `examples/ui` shows a main menu navigable by keyboard, gamepad and touch, plus the debug overlay.

### Phase 9 — Platformer sample game

**Goal:** a short, complete platformer that exercises every system.

- Player controller: run, acceleration, variable-height jump, coyote time, jump buffer, wall slide (optional).
- Enemies: patroller and jumper; stomp to defeat; damage and knockback; hazards (spikes, pits).
- Collectibles (coins), checkpoints, level exit, 3 levels built in Tiled with Kenney assets.
- Main menu, pause menu, settings (volume, controls), HUD (lives, coins), game over and win screens.
- Save progress through `Storage`.
- Fix engine gaps found while building the game; log them in `PROGRESS.md`.

**Ask:** art pack (Kenney Pixel Platformer or own art); number of levels; extra mechanics (double jump, dash).

**Done when:** the game is playable start to finish on desktop with keyboard and gamepad.

### Phase 10 — Web build

**Goal:** the platformer runs well in browsers and is ready to publish.

- `trunk` release profile, `wasm-opt -Oz`, LTO; asset bundling and compression.
- Preload screen with progress bar; canvas resize and device-pixel-ratio handling; fullscreen toggle.
- WebGPU with WebGL2 fallback verified; tab-hidden pause.
- `localStorage` saves verified; touch controls on mobile browsers.
- GitHub Actions job builds the web bundle; zip ready for itch.io or GitHub Pages.

**Ask:** hosting target (itch.io, GitHub Pages, own server); target download size; mobile-browser support.

**Done when:** the platformer runs at 60 FPS in Chrome, Edge and Firefox, and in a mobile browser.

### Phase 11 — Android build

**Goal:** the platformer runs as a native Android app.

- `android-activity` through `winit`; build with `cargo-apk` or `xbuild`; `aarch64` and `armv7` targets.
- APK asset reader backend for the asset server.
- Lifecycle: drop surface on suspend, recreate on resume; pause audio and game.
- Touch controls, back button, orientation lock (landscape), safe-area insets, immersive mode.
- App storage backend for saves; app icon and name.
- Debug and release signing; AAB output for Play Store; CI job for the Android target.

**Ask:** build tool (`cargo-apk` or `xbuild`); minimum Android API level; app ID (e.g. `com.cashgear.platformer`).

**Done when:** the platformer installs on a real device, survives backgrounding and resumes correctly.

## Phases 12–14: scripting, editor and release

These phases add tools for faster content work and finish the engine as a reusable product.

### Phase 12 — Lua scripting

**Goal:** gameplay behaviours can be written and hot-reloaded without recompiling.

- `mlua` runtime in `cg_script` (Lua 5.4 or Luau), sandboxed (no `os`/`io`).
- `Script` component pointing at a `.lua` asset; lifecycle hooks `on_start`, `on_update(dt)`, `on_collision`, `on_event`.
- Bindings: entity, transform, input actions, spawn/despawn prefabs, audio, timers, state changes, logging.
- Hot reload of scripts on desktop; clear errors shown in the debug overlay.
- Web strategy: `mlua` does not build for `wasm32-unknown-unknown`, so pick one: a pure-Rust Lua (`piccolo`, experimental), compiling scripts to Rust for web releases, or keeping web builds Rust-only.
- Port one enemy and the checkpoint logic of the platformer to Lua as proof.

**Ask:** Lua 5.4 or Luau; the web strategy above; how much of the API to expose in v1.

**Done when:** the Lua enemy behaves the same as the Rust version, and editing its script on desktop updates it live.

### Phase 13 — Scene editor

**Goal:** a desktop editor for placing and tuning entities without code.

- `tools/cg_editor` binary: engine renderer in the viewport, egui panels around it.
- Viewport: pan, zoom, grid, snapping, selection, move/rotate/scale gizmos.
- Panels: scene hierarchy, inspector (component editing via `bevy_reflect`), asset browser with thumbnails, console.
- Place prefabs, edit properties, parent/child drag-and-drop.
- Undo/redo command stack; save/load scenes as RON.
- Play-in-editor: run the scene, stop, and restore the edited state.
- Tiled stays the tilemap editor; the editor shows maps and reloads them on change.

**Ask:** Windows-only editor or all desktops; scope of v1 (placing entities only, or also animation/particle editing); whether to edit Tiled object layers directly.

**Done when:** a platformer level's entities can be placed, tuned, saved and played from the editor, with undo working.

### Phase 14 — Polish, performance and release

**Goal:** a documented, profiled engine that others can start a game with.

- `puffin` profiling scopes in all hot paths; optional Tracy.
- Performance pass: batching, culling, allocation-free frame loop; benchmarks in CI.
- Docs: rustdoc for public APIs, mdBook guide (getting started, ECS, assets, Tiled, audio, UI, web, Android, Lua, editor).
- `cargo generate` template for a new game; examples index.
- Release workflow: version bump, changelog, signed builds of the platformer for Windows, web and Android; optional crates.io publishing.

**Ask:** publish to crates.io or keep private; release targets for the sample game; versioning scheme (0.1.0 start).

**Done when:** a new game can be created from the template and run on desktop, web and Android by following the guide.

## Quality and risks

A phase is done only when it runs on every target it touches, is tested, documented and pushed; the main risks are rendering bugs the agent can't see and scope creep.

### Definition of done (every phase)

- [ ] Phase questions asked and answers recorded in `PROGRESS.md`
- [ ] New example runs on desktop; web build still runs
- [ ] fmt, clippy `-D warnings`, tests and wasm check pass in CI
- [ ] Public APIs have rustdoc comments
- [ ] `PROGRESS.md` and `CHANGELOG.md` updated
- [ ] Pushed to `main`, tagged `phase-N`, next-phase card given

### Testing strategy

- **Unit tests:** math, timestep, input mapping, collision sweeps, tile collision, asset handle lifetimes, animation timing, RON round-trips.
- **Headless render tests:** render examples to an offscreen texture and compare against reference PNGs (small tolerance) so the agent can detect visual regressions.
- **Examples as tests:** every example has a `--frames N` flag that runs N frames and exits; CI runs them headless where possible.
- **Manual checks:** a short checklist per phase for the user (screenshot or short clip) when the result is visual or device-specific.

### Risks

| Risk | Mitigation |
| --- | --- |
| Agent can't see rendering bugs | Headless reference-image tests; ask the user for screenshots at phase end |
| Scope creep toward a general engine | Every feature must serve the platformer or a listed example; extras go to a backlog in `PROGRESS.md` |
| Web limits (no threads, async-only IO, audio unlock) | Portability rules from phase 1; wasm check in CI every phase |
| Android lifecycle and surface loss | Surface-recreation path built in phase 2, tested on device in phase 11 |
| `mlua` not available on web | Decide the web scripting strategy at the start of phase 12 |
| Crate API churn (`wgpu`, `winit`, `bevy_ecs`) | Pin versions in the workspace; upgrade only between phases |
| Editor grows too large | Fix v1 editor scope in phase 13 questions; Tiled remains the map editor |
| Arabic/RTL text complexity | Use `cosmic-text` shaping; decide in phase 8 whether RTL is v1 |
