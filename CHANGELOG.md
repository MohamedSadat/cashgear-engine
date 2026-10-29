# Changelog

All notable changes to CashGear Engine are recorded here, one section per phase.
The format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/).

## Phase 6 — Animation and effects

### Added

- `cg_anim`: `SpriteSheet` assets, frames cut from one image with named `AnimationClip`s (frames with durations, a `Direction` of forward, reverse, ping-pong or ping-pong reverse, an optional repeat count, and `ClipEvent`s on frames). `AsepriteLoader` reads Aseprite sheet exports (`.json`, "Hash" or "Array" layout): frame durations, trimmed frames (pivots stay put), tags as clips with their direction and repeat count, and events from tag user data (`0:footstep, 3:footstep`). `SpriteSheet::from_atlas` and `clip_from_frames` build sheets in code from `cg_pack` atlases.
- `cg_anim`: the `SpriteAnimation` component plays a clip on the entity's `Sprite` in fixed steps (texture, source and pivot; trimmed frames mirror around the pivot when flipped), with speed, pause, `play` and `restart_with`, and sends `AnimationEvent` and `AnimationFinished` messages. A new or switched clip shows at once.
- `cg_anim`: animation state machines. `AnimationGraph` assets (`.anim.ron`, checked on load, or built in code) hold states (a clip, a speed, optionally scaled by a parameter) and transitions (from states or any state, conditions on number, true/false and trigger parameters, after the clip finishes, after a minimum time). The `Animator` component runs one from parameters the game sets (`set`, `set_bool`, `trigger`, `force`) and sends `AnimationStateChanged`.
- `cg_anim`: tweens. `Tween` eases a translation, scale, rotation, sprite colour, sprite flash or camera zoom with any of 31 `Ease` curves, with a start value, delay, repeats, yoyo, a tag and a follow-up tween (`then`). `Tweens` holds an entity's tweens; `commands.entity(e).tween(...)` starts one; `TweenFinished` reports the end. Transform tweens on interpolated entities run in fixed steps, the rest every frame.
- `cg_anim`: CPU particles. `ParticleEmitter` emits at a rate, in a start burst or on demand (`emit`), from a point, rectangle or circle, with lifetime, speed, direction and spread, gravity, drag, size and colour over life, rotation and spin, world or local space, and a per-emitter cap; one-shot emitters despawn when done. Particles are drawn through the sprite batcher and culled per emitter. `ParticleBudget` caps the live total (16,384 by default, `AnimPlugin::particle_budget`).
- `cg_anim`: `CameraFollow` (a dead zone, smoothing, an offset and level bounds; `snap` after a teleport), `CameraShake` (trauma-based, from smooth noise, moving only the drawn view), and the drawn view kept inside the bounds every frame.
- `cg_anim`: `ScreenFade` (fade the screen to a colour and back, on real time) and `StateFade<S>` with `add_state_fade::<S>()`, which changes game state behind a fade.
- `cg_anim`: `AnimPlugin`, `AnimSystems` and a prelude. `SpriteAnimation`, `Animator`, `ParticleEmitter` and `CameraShake` are prefab components.
- `cg_render`: `Sprite::flash`, a colour the sprite is mixed towards by its alpha (in linear light, keeping the sprite's transparency), for hit flashes. Sprite instances carry it as four bytes.
- `cg_tiled`: tile animations made in Tiled's tile animation editor play in tile layers and on tile objects (`TileAnimation`, `TileInfo::animation`, `TileFrame`), in step on the game clock.
- `examples/animation`: Kenney's alien from a trimmed Aseprite sheet with a blink and a landing frame, a state machine in `alien.anim.ron`, footstep dust from a prefab emitter, squash and stretch, a hit flash and blink, spinning and bobbing coins with sparkles and pickup bursts, a waving flag, a camera that follows, shakes and zooms, and a fade into a particle gallery with a 10,000-particle stream. `--demo` lets a bot play it; a test runs the same bot without a window.
- CI: `animation --demo`.

### Changed

- `DefaultPlugins` adds `AnimPlugin` (field `anim`); the prelude includes the animation items and `TileAnimation`.
- `Assets` versions are unique across every collection in the process, so a renderer shared by two worlds (the golden tests, a future editor's play mode) never takes one image for another. This fixed reference-image tests that could fail when run in parallel.
- `examples/tiled_map` plays its enemies' frames with `SpriteAnimation` clips built in code from the character atlas; its `Frames` component is gone.
- New dependency: `serde_json` 1.0.154 (Aseprite sheets).

## Phase 5 — Collision and physics

### Added

- `cg_physics`: the `Collider` component, an axis-aligned rectangle or a circle (`ColliderShape`) with an offset, placed by the entity's transform. A collider on its own is static and blocks bodies; with `Sensor` it only reports overlaps; with `KinematicBody` it is the body's shape.
- `cg_physics`: `KinematicBody`, moved by its velocity in each fixed step with `Gravity` (980 units per second squared down by default, per-body `gravity_scale` and `max_fall_speed`). Moves are swept per axis over the whole distance, in sub-steps of at most 8 units, so nothing tunnels at any speed (tested at 100,000 units per second). Grounded bodies follow slopes up and down, stay on the ground over small drops (`snap_distance`), and can step up low ledges (`step_height`). Setting `drop_through` drops through one-way platforms; a too-low gap under a ceiling stops a climb.
- `cg_physics`: `KinematicContacts` with the ground, ceiling and walls a body touched (entity, tile cell and normal), `hazard`, `just_landed`, `just_left_ground`, `on_slope` and the ground normal.
- `cg_physics`: tile collision. `TileGrid` holds `TileCell`s: solid, one-way, slopes from any left to right height (45° and 22.5° included), each optionally a hazard; `TileGrid::parse` builds one from text. `TileCollider` puts a grid in the world. Slopes carry bodies by their foot, as platformer characters stand.
- `cg_physics`: Tiled support. Every `Tilemap` gets a `TileCollider` from its tile layers with a `collision` property, read from the tiles' `solid`, `one_way`, `slope_left`/`slope_right` and `hazard` properties (slopes flipped horizontally slope the other way); it is rebuilt when the map changes. Objects with a `collider` or `sensor` property get a collider of their shape. `TileGrid::from_tiled_map`.
- `cg_physics`: `CollisionLayers` (32 membership and filter bits) for colliders and tile maps.
- `cg_physics`: overlap reporting. `CollisionStarted` and `CollisionEnded` messages and the `Overlaps` resource cover every interacting pair except two static colliders. Moving bodies are tested against sensors by the box they swept, so a dash still collects the coins it passes. The `PhysicsWorld` snapshot uses a spatial hash.
- `cg_physics`: `SpatialQuery` for raycasts (`RayHit`; solids from any side, one-way platforms and slopes from above), `overlap_point`, `overlap_shape` and `tile_at`, with `QueryFilter` (layers, an excluded entity, sensors, tiles).
- `cg_physics`: `PhysicsDebug` draws tiles, colliders, sensors, bodies and their contacts; F1 toggles it and `--physics-debug` starts with it on.
- `cg_physics`: `PhysicsPlugin` (the physics in `FixedPostUpdate` after transform propagation, in the `PhysicsSystems` sets) and a prelude. `Collider`, `Sensor`, `KinematicBody` and `CollisionLayers` are registered for prefabs and Tiled properties.
- `examples/collision`: a Tiled test course with original slope tiles made from Kenney's grass tile, one-way planks and clouds, spikes, crates, coins, a dash and a slam. `--demo` lets a bot run it with raycasts and hints, exiting with an error if it fails; a test runs the same bot without a window.
- CI: `collision --demo --physics-debug`.

### Changed

- `DefaultPlugins` adds `PhysicsPlugin` (field `physics`, flag `--physics-debug`); the prelude includes the physics items.

## Phase 4 — Assets and tilemaps

### Added

- `cg_platform`: `io::AssetIo`, which reads files by `/`-separated path from the asset folder on desktop, with `fetch` on the web, and (from Phase 11) from the APK on Android, through one `async` API. The folder is `CG_ASSET_ROOT`, else `assets` in the crate run by `cargo run`, else next to the executable; on the web, `assets/` next to the page. `io::FileWatcher` reports changed files on desktop (`notify` 8.2.1).
- `cg_assets`: the `AssetServer` (`load`, `add`, `reload`, load states, errors with the path and reason), reference-counted `Handle<T>`s and copyable `AssetId<T>`s, `Assets<T>` collections with versions and `AssetEvent<T>` messages, and `async` `AssetLoader`s whose `LoadContext` reads further files and loads dependencies. Assets unload when their last strong handle drops.
- `cg_assets`: hot reloading on desktop (on by default in debug builds): a changed file reloads every asset made from it or that read it while loading; a failed reload keeps the previous version.
- `cg_assets`: `LoadingAssets` groups with `progress`, and `add_loading_state(loading, next)`, which moves on once the group has loaded. `--asset-delay <seconds>` holds loads back to try loading screens.
- `cg_assets`: prefab files as assets. `app.load_prefab(name, path)` loads a `.prefab.ron` file into `Prefabs` and reloads it when it changes.
- `cg_assets`: `AssetPlugin`, `AppAssetExt` (`init_asset`, `register_loader`, `load_prefab`, `add_loading_state`) and the `path` helpers.
- `cg_render`: `Image` is an asset with its own `Filter`, loaded from PNG by `ImageLoader`; `AssetId::DEFAULT` is the built-in white texture.
- `cg_render`: texture atlases. `TextureAtlas` assets load from `.atlas.ron` files (`AtlasFile`) with their image as a dependency; `AtlasSprite` draws a named region on an entity and follows changes to the atlas or the region name.
- `cg_render`: `RenderSystems::View` (camera picking) before `RenderSystems::Queue`, so other crates can queue sprites from the `FrameView`; `RenderStats::add_culled`; `QueuedSprite::new`.
- `cg_tiled`: `TiledMap` assets loaded from `.tmx` with `.tsx` tilesets and `.tx` templates through the asset IO (`TmxLoader`, on the `tiled` 0.16 crate). Tile, image, object and group layers, flips, layer offsets, opacity, tint and parallax, custom properties (classes included) on maps, layers, tilesets, tiles and objects, finite and infinite maps.
- `cg_tiled`: the `Tilemap` component. Tile layers are cut into 16×16-tile chunks, built once and culled whole; image layers repeat to fill the view; parallax layers scroll at their own speed. Objects spawn as child entities with a `TiledObject`, sprites for tile objects, the prefab their class names, and components set from properties named after them. Changed maps rebuild in place.
- `tools/cg_pack`: `cg_pack atlas <folder> <out.atlas.ron>` packs a folder of PNGs into an atlas image and RON file, deterministically, with `--padding`, `--extrude`, `--max-size` and `--check`.
- `examples/tiled_map`: a Kenney Pixel Platformer level made in Tiled, with a loading bar, a scrolling camera, parallax hills and clouds, bobbing coin prefabs, enemies animated from a packed atlas and tuned by Tiled class properties, tile properties for Phase 5, and live editing.
- CI: `tiled_map --frames 360 --demo` and `cg_pack atlas ... --check`.

### Changed

- `Sprite::texture` is a `Handle<Image>`, so `Sprite` is `Clone` but no longer `Copy`. The `Textures` store and `TextureId` are gone: images live in `Assets<Image>`, added with `images.add(image)` or loaded with `server.load("x.png")`.
- `QueuedSprite` holds the sprite's fields directly, with the texture as an `AssetId<Image>`, so queueing touches no reference counts. `Sprites::draw` and `draw_on` take `&Sprite`.
- Prefab `Sprite`s name their texture by asset path (`texture: "images/bunny.png"`), loaded when the prefab spawns.
- `OffscreenRenderer::render` takes `&Assets<Image>`.
- `DefaultPlugins` adds `AssetPlugin` (with the asset launch flags) and `TiledPlugin`; the prelude includes the asset and Tiled items.
- `examples/ecs_sprites` loads its prefabs and bunny images from `assets/` through a `Loading` state, and registers the art it draws in code under `generated/...` paths. `examples/bunnymark` keeps its images in `Assets<Image>` and queues `QueuedSprite`s directly.
- New dependencies: `notify` 8.2.1 (desktop), `js-sys` 0.3.106 (web), `tiled` 0.16.0 (without its zstd feature, so it builds for wasm).

## Phase 3 — ECS and scenes

### Added

- `cg_world`: `Transform2D` (translation, rotation, scale) and `GlobalTransform2D`, propagated down bevy's `ChildOf`/`Children` hierarchy with change detection before and after each fixed step and in `PostUpdate` (`TransformSystems::Propagate`).
- `cg_world`: the opt-in `Interpolated` component. Entities moved in `FixedUpdate` are drawn between their last two fixed steps, children included; `Interpolated::reset` skips the blend after a teleport. `InterpolatedTransforms` gives the drawn transform of any entity.
- `cg_world`: a game-state stack over the game's own enum: `init_state`, `State` (current state and stack), `NextState` (`push`, `pop`, `replace`, `set`), the `OnEnter`, `OnExit`, `OnPause` and `OnResume` schedules, the `in_state` and `state_in_stack` run conditions, and `StateScoped` entities that despawn when their state exits.
- `cg_world`: RON prefabs. Components register by name (`register_component` for serde types, `register_prefab_component` for `PrefabComponent`s that resolve names at spawn time); prefabs nest children, report unknown components and bad fields with line and column, and round-trip to RON. `Prefabs` parses on first use; `commands.spawn_prefab(name)` spawns one.
- `cg_world`: `WorldPlugin`, the `AppWorldExt` app methods and a prelude (with `ChildOf`, `Children` and `Name`).
- `cg_core`: the `StateTransition` schedule between `PreUpdate` and the fixed steps; `Rect::overlaps`, `Rect::from_points` and serde for `Rect`.
- `cg_render`: the `Sprite`, `Layer`, `Visibility` (with computed `InheritedVisibility`) and `Camera2D` components. The renderer picks the active camera, culls sprite entities outside its view and queues the rest each frame (`RenderSystems::Queue`, then `Draw`).
- `cg_render`: `CameraView` (a camera's settings plus where it is) with world/screen conversion, and the `MainCamera` system parameter for game code.
- `cg_render`: texture names (`Textures::add_named`, `named`, `name_of`), which prefabs use; `Sprite::solid`; `RenderStats::culled`; serde for `Color` (hex strings or channels), `ScalingMode` and `Camera2D`.
- `cg_platform`: `Window::with_size` for tests and tools.
- `examples/ecs_sprites`: prefabs with nested children, a spinning hierarchy, interpolated movement, blinking visibility, despawning, culling, and a Menu / Playing / Paused state stack with a pause overlay parented to the camera. `--demo` runs every transition without input.
- `examples/bunnymark`: `--ecs` makes every bunny an entity.
- CI: `bunnymark --ecs` and `ecs_sprites --demo` runs.

### Changed

- `Camera2D` is a component on an entity; its transform places the view. The `Camera2D` resource is gone.
- `Sprite` is the ECS component and no longer has `position`, `rotation`, `scale` or `layer`. Immediate-mode drawing takes a transform: `sprites.draw(sprite, transform)` or `sprites.draw_on(layer, sprite, transform)`, queuing `QueuedSprite`s. Sprite transforms are full affine matrices, so skew from scaled parents is drawn correctly.
- `OffscreenRenderer::render` takes a `CameraView` and a `Sprites` queue.
- `DefaultPlugins` adds `WorldPlugin`, and the prelude includes `cg_world`'s.
- New dependency: `erased-serde` 0.4.11 (prefab components are read through it). `ron` is now a normal dependency of `cg_world`.

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
