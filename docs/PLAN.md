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
| After 0.1.0 | A GUI engine: Lua projects made, played and exported in the editor, run by a prebuilt player (phases 15–19) |

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
| Scripting | `mlua` (Lua 5.4, sources compiled in) | Chosen in phase 12: desktop and Android; web builds stay Rust-only |
| Scene editor | own `cg_editor` (egui panels, serde-driven inspector) | Chosen in phase 13: edits Tiled object layers in place and RON prefabs; a library games build in for play-in-editor |
| Profiling | `profiling` crate scopes: `puffin` (feature `profile`) or Tracy (`profile-tracy`) | Chosen in phase 14: puffin's frames show in the F12 overlay's Profiler window on every target |
| Benchmarks | `criterion` (dev only) and a counting allocator | Chosen in phase 14: `cg_bench` times frames (reported in CI) and requires allocation-free steady frames |
| Game template and guide | `cargo-generate`, `mdbook` | Chosen in phase 14: `templates/game` and `docs/book`, the guide published on GitHub Pages |
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
| `tools/cg_editor` | Desktop scene editor (a library games build in, and a binary) | 13 |
| `tools/cg_pack` | CLI: atlas packing, asset bundling (a library too from phase 16) | 4 |
| `templates/game` | `cargo generate` template for a new game | 14 |
| `tools/cg_player` | The player: runs a project (scenes, prefabs, Lua, `project.ron`) on every target | 15 |
| `templates/project` | The starter Lua project the editor copies | 15 |
| `tools/cg_export` | Exports a project with a prebuilt player for each target (a library and a CLI) | 16 |
| `benches` | `cg_bench`: frame benchmarks and the allocation check | 14 |
| `docs/` | `PLAN.md`, `PROGRESS.md`, mdBook guide (`docs/book`, phase 14) | 0 |

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
- Panels: scene hierarchy, inspector (component editing through serde, chosen in phase 13 over `bevy_reflect`), asset browser with thumbnails, console.
- Place prefabs, edit properties, parent/child drag-and-drop.
- Undo/redo command stack; save/load scenes (phase 13: Tiled object layers saved in place, prefabs as RON).
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

## After 0.1.0: a GUI engine

The fifteen phases above are done; 0.1.0 is released. Work continues in the same way, one phase per session, each ending with a push, a tag and the next card.

In 0.1.0 a game is Rust code built with Cargo, and the editor places things in its levels. Phases 15–19 make CashGear a GUI engine as well. A game becomes a **project**: a folder of scenes, prefabs, Lua scripts and a project file. A prebuilt **player** runs it, and the **editor** creates, plays and exports it, with no Rust, Cargo or terminal. Rust games stay as they are (the template). The player is itself a Rust game on the same plugins, so both kinds of game share every system.

Much of it exists already:

- The editor's Play runs physics, animation and Lua on a scene without game code.
- UI widgets, sprites, bodies and scripts are all prefab components, so screens and characters can be data.
- A shipped executable reads the `assets.cgpak` beside it, and the web build fetches its bundle.
- Lua reloads while the game runs.

The missing pieces are the player, scene flow and whole-game APIs in Lua, a project manager, exporting, Lua on the web, and editors for tiles, sprites and UI.

**The walkthrough.** These steps are the scope test for every feature in these phases: a feature goes in when a step needs it, and everything else goes to the backlog.

1. Download the editor's zip and run it, with nothing else installed (for Windows and the web; Phase 18 decides what Android needs).
2. Choose New project. A starter game opens: a title screen, a level, a player and coins in Lua, a HUD and a win screen.
3. Edit the level, the player's script and a menu. Play runs the whole game from its title screen.
4. Export a Windows zip, a web zip (also previewed in the browser from the editor) and an Android APK.
5. The exports run on another PC, in Chrome, Firefox and Safari, and on a phone.

**Releases.** 0.2.0 when Phase 16 ends (the first editor that makes whole games, for Windows), then a minor release after each later phase. Each phase's start asks whether it may break the previous release's API.

### Phase 15 — The player: games without Rust

**Goal:** a prebuilt player runs a whole game made of data and Lua, with no Rust of its own.

- **`project.ron` at the asset root.** It holds:
  - the title, app id and version;
  - the window: size, pixel-perfect scale and background;
  - the first scene and the main script;
  - the controls (an `ActionMap` in RON) and the on-screen touch controls;
  - the icon and the default language.

  It is read through serde with defaults, so a minimal file is a few lines, and it has a format version for later changes.
- **`tools/cg_player`, a library and a binary.** It runs `DefaultPlugins` configured from `project.ron`. It reads the asset folder, or the `assets.cgpak` beside the executable, or the folder `--project <folder>` names. The same crate builds for the web (scripts off until Phase 17) and Android (`android_main!`), so later phases only add exporting.
- **Scenes.**
  - `scene.load(path)` in Lua (`SceneManager` in Rust) replaces the current scene with a Tiled map (a level) or a prefab (a screen), with the existing `ScreenFade`.
  - `scene.overlay(path)` adds a prefab on top (a HUD, a pause menu).
  - `scene.reload()` and `scene.current()`.
  - The main script and overlays survive a load.
- **The main script.** `scripts/main.lua` runs on a persistent entity for the whole game. `events.emit` from any script reaches its `on_event`, so the rules (score, lives, level order) live in Lua instead of Rust.
- **Lua for whole games,** each a thin layer over what the engine has:
  - **UI:** `on_click`, `on_focus` and `on_change` hooks on widgets that have scripts; `self:set_text`, `self:set_visible` and `self:set_value`; gamepad focus for menus.
  - **`camera`:** `follow(entity, opts)` with a dead zone and bounds from the map, `shake`, `set_position` and `zoom`.
  - **`save`:** `get`, `set` and `remove` of plain Lua values through `Storage`.
  - **`settings`:** volumes, fullscreen and language (`Locale`).
  - **`app`:** `quit`, `pause` and `resume` (fixed steps), `time_scale`, `platform()` and `is_touch()`. It is named `app` so it doesn't clash with games' own `game` tables.
- **`templates/project`,** the starter project the editor will copy: a title screen prefab, a level, a Lua player, coins, a HUD, a win screen, `project.ron` and `main.lua`, with a Lua bot for its test.
- **The scripting example becomes a project.** Its Rust parts (`game.demo`, the HUD, the win screen, the camera) move into `project.ron`, `main.lua` and prefabs. Its `main.rs` shrinks to a call into `cg_player`, and its bot test still reaches the goal.
- **The editor's Play learns projects.** With a `project.ron`, Play uses the project's controls and main script instead of `play_controls`.

**Ask:**

- the project file's name and format (RON like everything else, or TOML);
- whether screens are prefabs, as proposed, or a new scene file that can hold a map and its overlays;
- how far Lua UI goes in v1: hooks and setters only, or creating widgets from Lua.

**Done when:**

- `cg_player --project templates/project` plays from the title to the level, to the win screen and back to the title on desktop;
- the starter project's bot test passes headless in CI;
- the scripting example has no game logic in Rust;
- the player builds for wasm and Android.

### Phase 16 — The editor as the hub

**Goal:** a project is created, edited, played and exported for Windows from the editor alone.

- **Start screen:** New project (from `templates/project`, built into the editor), Open (a folder with a `project.ron`) and Recent. The editor's own settings are kept with `Storage`.
- **Project settings window:** everything in `project.ron`. It includes a controls editor (press a key or button to bind it) and a preview of the touch layout.
- **Two kinds of Play,** both with the main script:
  - F6 plays the game from its first scene;
  - F5 plays the open scene, as now.

  Script errors in the console link to their file and line.
- **New assets from the editor:**
  - a script, from a template of hooks;
  - a prefab;
  - a screen (a UI prefab);
  - a map, copied from a starter map with its tileset.

  Rename and delete check references (prefab names, and script paths in prefabs and maps).
- **Scripts:** Edit opens the `.lua` file in the system's editor (VS Code when present), and hot reload applies the change.
- **`cg_pack` becomes a library** (bundles and atlases) as well as the CLI, so the editor packs bundles itself.
- **`tools/cg_export`,** a library and a CLI, exports a project for a target from a prebuilt player:
  - Windows: the player renamed to the game's title and given its icon, `assets.cgpak`, the licences, and a zip;
  - Linux.

  The CLI lets CI export the starter project and play the result with its bot.
- **The editor's download** carries the prebuilt players (`players/windows/`, `players/linux/`), which the release workflow builds.

**Ask:**

- whether the download holds a player for every target (a bigger zip) or fetches them on the first export, as Godot does with its export templates;
- how the exe gets the game's icon (a resource-editing crate, or the player's icon in v1);
- whether macOS comes now or later (it needs signing to open without warnings).

**Done when:**

- on a Windows PC with only the editor's download, a new project can be made, edited, played and exported, and the export runs on another PC;
- CI exports the starter project and plays it with its bot;
- v0.2.0 is released.

### Phase 17 — Scripts on the web, web export

**Goal:** Lua projects run in browsers, and the editor exports and previews web builds.

- **A time-boxed spike comes first.** It is measured against two things the engine already has:
  - the platformer's bot gives the same result in a browser as on desktop;
  - `cg_bench`'s 500 scripted bodies stay within twice the desktop time.

  The candidates, in order:
  1. Lua 5.4 compiled with clang into the engine's own `wasm32-unknown-unknown` module. It needs a small C runtime shim (allocation from Rust, plus the string and math functions Lua calls). The hard part is Lua's error handling (`setjmp`/`longjmp`, or C++ exceptions), which needs WebAssembly exception handling. The `mlua` bindings stay if mlua can be built this way; otherwise a thin binding layer of our own.
  2. Lua in its own wasm module, called through JavaScript. The bindings are reached through a flat call table: slower per call, but nothing changes in the engine's build.
  3. A pure-Rust script language (Rhai, or a maintained pure-Rust Lua if one exists by then). Every binding and script is rewritten, and it runs the same on every target.
  4. The fallback: projects export to the web only without scripts.
- **One API everywhere.** Whatever wins runs the same API on every target. `cg_script` on the web stops being a stub, and the platformer's Rust copies of its scripted walker and checkpoint go.
- **Web export in `cg_export`:** the prebuilt web player (`index.html`, the wasm, the loader and the icons) with the project's bundle, title and icons, zipped for itch.io or any static host.
- **Preview from the editor:** the editor serves the export on a local port and opens the default browser at it.
- **Browser field testing:** Firefox and Safari (desktop and iPhone) with a gamepad, and WebGL2 frame times with many sprites.

**Ask:**

- which candidate to try first, and how long the spike may take;
- how much slower than desktop the web may be;
- the oldest browsers to support (candidate 1 needs WebAssembly exception handling).

**Done when:**

- the starter project and the scripting example, exported from the editor, play with their scripts in Chrome, Firefox and Safari;
- their bots give the same results as on desktop;
- the platformer no longer needs Rust copies of its scripts.

### Phase 18 — Android export

**Goal:** the editor exports an Android APK and AAB that install and play on a phone.

- **The prebuilt Android player:** `cg_player` built for arm64-v8a, armeabi-v7a and x86_64 with the engine's activity. The release workflow builds it.
- **The export path,** decided at the start:
  - **(a) Patch a prebuilt APK in Rust.** Replace `assets/assets.cgpak`, the app name and id in the binary manifest, and the icons, then align and sign (APK signature v2) with a debug key or the user's key. No SDK is needed, but there is no AAB. Check first which Rust crates for building and signing APKs exist.
  - **(b) A Gradle project generated by the editor** around the prebuilt libraries, with no Rust or NDK. It is built with the Android SDK and a JDK the editor finds or helps install, and gives AABs for the Play Store.
- **Keys:** a debug key made on the first export, and a release key created in the editor, kept out of the project, using Phase 11's `CG_ANDROID_*` settings.
- **Install and run on a connected phone** through adb, when the SDK is present.
- **The open Phase 11 device checks,** through an exported project: lifecycle, audio with headphones and calls, back, display cutouts, touch and gamepads. Also the frame rate and battery on a mid-range phone.

**Ask:**

- path (a), path (b), or (a) for quick APKs and (b) for store builds;
- which phones are available;
- Play Store extras now (debug symbols, the icon's foreground layer, the target SDK policy).

**Done when:**

- a project exported from the editor installs on a phone and plays;
- the device checklist passes;
- a signed AAB comes out of the editor.

### Phase 19 — Built-in content editors

**Goal:** a whole game is made in the editor, without Tiled or Aseprite.

- **Tile painting:**
  - brush, rectangle, fill, eraser and picker on the tile layers of `.tmx` files, written back as Tiled writes them so Tiled still opens them (the editor writes object layers only today);
  - new maps (size and tile size);
  - tilesets from a PNG (grid and margins), with per-tile collision (solid, one-way, slope, hazard) and classes;
  - adding, renaming and ordering layers.
- **Sprites:**
  - sheets from a PNG grid or a folder of frames;
  - clips (frames, durations, looping), previewed;
  - an `.anim.ron` graph edited as a list of states and transitions.
- **UI screens:** a UI prefab laid out at the project's resolution and on phone shapes, with widgets dragged in, styles in the inspector, and text with its translations.
- **Previews:** particles and tweens previewed in the inspector.

**Ask:**

- which editors come first;
- auto-tiling (Tiled's terrain sets or rules of our own);
- graph editors now, or lists first.

**Done when:** the starter project's second level, its sprites and a new options screen are made in the editor alone, and Tiled still opens the maps.

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
| `mlua` not available on web | Decided in phase 12: web builds keep Rust versions of scripted behaviours, and a test checks both play the same |
| Crate API churn (`wgpu`, `winit`, `bevy_ecs`) | Pin versions in the workspace; upgrade only between phases |
| A GUI engine's scope (phases 15–19) | The walkthrough is the scope test: a feature goes in when a step of it needs one; the rest goes to the backlog |
| No Lua on the web (phase 17) | A time-boxed spike measured against the bot test and `cg_bench`; the fallback exports projects without scripts to the web |
| Android export needing Java tools (phase 18) | Decided at the phase's start: patch a prebuilt APK in Rust (no SDK), or drive Gradle (the SDK and a JDK) |
| Rust games and Lua projects drifting apart | The player is a Rust game on the same plugins; the template's game and the starter project both run their bots in CI |
| Editor grows too large | Fix v1 editor scope in phase 13 questions; Tiled remains the map editor |
| Arabic/RTL text complexity | Use `cosmic-text` shaping; decide in phase 8 whether RTL is v1 |
