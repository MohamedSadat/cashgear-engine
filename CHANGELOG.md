# Changelog

All notable changes to CashGear Engine are recorded here: one section per release, and
before the first release one section per phase. The format follows
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and versions follow
[Semantic Versioning](https://semver.org/) (in 0.x, a minor version may break games).

## [Unreleased]

### Added

- The release has the scene editor for Windows: `CashGear-Editor-<version>-windows.zip` holds `CashGear Editor.exe` and the scripting example's assets, so the editor runs without Rust installed. `--asset-root` points it at another folder.

### Changed

- The standalone editor (`cg_editor`) opens the first map in the asset folder's `maps/` when it is given no file, binds the actions `left`, `right`, `down` and `jump` for Play as the template does (`cg_editor::play_controls`), and opens no console window in Windows release builds.

### Fixed

- The scripting example's player and goal scripts run without the example's Rust code (its `game` table), so its level plays in the standalone editor; before, the player's script stopped with an error at Play.
- A game made from the template no longer shows three `error: invalid character ...` lines the first time Cargo resolves the engine: Cargo reads every manifest in a git dependency, and the template's manifests held cargo-generate placeholders. They are `Cargo.toml.liquid` files now, which cargo-generate renders into `Cargo.toml`. (Builds worked before; the lines were only alarming.)

## [0.1.0] — 2026-10-31

The first release of CashGear Engine, a Rust 2D game engine for Windows, Linux, macOS, the web and Android. It has everything its fifteen phases built (the sections below): a fixed-step game loop, `wgpu` sprite batching with pixel-perfect scaling, entities and prefabs, background asset loading with hot reload, Tiled maps, platformer physics, Aseprite animation, tweens and particles, audio, text and UI with right-to-left languages and touch controls, Lua scripting, a scene editor, and web and Android builds. The sample platformer shows it all: play it at <https://mohamedsadat.github.io/cashgear-engine/>, or download it below for Windows and Android.

Start a game with the template and the guide:

```sh
cargo install cargo-generate --locked
cargo generate --git https://github.com/MohamedSadat/cashgear-engine templates/game --tag v0.1.0
```

The guide is at <https://mohamedsadat.github.io/cashgear-engine/book/> and the API docs at <https://mohamedsadat.github.io/cashgear-engine/api/>.

### Phase 14 — Polish, performance and release

#### Added

- A game template, `templates/game`, for `cargo generate`: a small complete game (a title screen, a Tiled level with Kenney's CC0 art, an animated alien that runs and jumps, coins with a Lua script, spikes, a door, a win screen, touch controls, sounds) with its web page, its Android Gradle project, an editor build, a `--demo` bot with a test, and a CI workflow. It depends on the engine by git tag. CI generates a game from it on every push and checks it: formatting, lints, the bot test and the web build.
- The guide, an mdBook in `docs/book`: getting started, a tour of the template, then a chapter for each part of the engine, the web, Android, profiling and releasing, and references for the examples and launch flags. Its code samples are the template's own code. CI publishes it at `/book/` on GitHub Pages, with the API docs (rustdoc, warnings denied) at `/api/`; the platformer stays at the root.
- Profiling: the engine's schedules, systems and renderer steps are marked with the [`profiling`](https://docs.rs/profiling) crate's scopes, which cost nothing without a backend. The `profile` feature records them with puffin; the debug overlay's new Profiler window shows the recent frames, a flame graph of the chosen frame (hover for times, click to zoom) and the scopes ranked by their own time, on desktop, the web and Android. `--profile` records from the start and opens it. The `profile-tracy` feature sends them to Tracy instead. `cg_core::profile` (`set_recording`, `is_recording`) and the re-exported `profiling` crate let games mark their own systems; wgpu and egui scopes show too.
- `cg_bench` (`benches/`): headless scenes (10,000 sprites, a Tiled level with 200 bodies and particles, a menu, 500 scripted bodies, the platformer played by its bot) timed with criterion (`cargo bench -p cg_bench`); CI posts the table in the run's summary. An allocation test counts heap allocations per frame with a counting allocator and requires none in the sprite, level and menu scenes; `CG_ALLOC_TRACE=<frames>` prints where a scene's allocations come from.
- Lua `require`: shared code in other script files, run once and returned to every file that asks (`require("enemies.common")` loads `enemies/common.lua`). Required files load through the asset server, and changing one reloads the files that require it. Circular requires are errors.
- A release workflow (`.github/workflows/release.yml`): a `v*` tag checks that the tag, the workspace version, the template and the changelog agree, builds the platformer for Windows (a zip with the executable and its asset bundle), the web (a zip for itch.io or any host) and Android (an APK and an AAB, signed when the repository has the keystore secrets), and publishes a GitHub Release with this changelog section. `tools/release.py` bumps the version, checks a tag and prints the notes.
- A shipped desktop game reads `assets.cgpak` next to its executable (when not run by `cargo run` and `CG_ASSET_ROOT` is not set), so a release is the executable and one bundle.
- Android builds take their version from `-PversionName=X.Y.Z`, with the version code Google Play needs from it (1.2.3 is 10203).
- Code examples at the top of `cg_core`, `cg_platform` and `cg_editor`'s documentation.

#### Changed

- The engine's frame loop makes no heap allocations in steady frames. The physics keeps its spatial hash buckets, query buffers and overlap lists between steps (`Overlaps` is a sorted list, and changes only when the pairs do); the UI layout keeps its lists; focus navigation keeps its menu list; the batcher sorts with a stable counting sort over its few distinct keys, in kept buffers (release bunnymark at 10,000 sprites: 1.29 ms a frame, against 1.41 ms in Phase 13); nine-slice images and the locale no longer allocate per frame. The level scene went from about 1,170 allocations a frame to none, and its frame time down 9%.
- Lua: engine calls find the lent world in the app data instead of a registry lookup, `self` finds its entity without a string key, entities handed to Lua are cached (one userdata per entity while Lua holds it), `self:contacts()` refills one table kept on `self`, and query results are sized up front. The runtime no longer rebuilds its lists each step when no script, entity or file changed. A frame of 500 scripted bodies went from 4.3 to 2.7 ms.
- The platformer's HUD and log reuse their strings; the platformer and template open no console window in Windows release builds.

#### Fixed

- `platformer --demo --level N` left its bot on the pause menu for ever: started from a level, it had never used the menus, so nothing had the focus and its Accept did nothing. It now moves the focus to Resume (and to Next on the level-complete screen) first.

## Phase 13 — Scene editor

### Added

- `tools/cg_editor`: the scene editor, a library (`EditorPlugin`) and a binary. It draws the scene with the engine's renderer and puts egui panels around it: a menu bar and toolbar, a hierarchy, a palette, an inspector, an asset browser with thumbnails and a console. The view fits between the panels.
- Editing Tiled maps: the editor edits the object layers of a `.tmx`. Place a map tileset's classed tiles, prefabs (as point, rectangle or ellipse objects of that class) and shapes. Select by click, Shift+click or a box. Move, rotate and resize with gizmos, with grid snapping (the map's tile size or a chosen step; Ctrl toggles it while held) and 15° rotation steps. Nudge with the arrows; duplicate and delete. Drag objects between layers in the hierarchy. The inspector edits name, class, layer, position, size, rotation, visibility, tile flips and typed custom properties, including class properties. It overrides a tile's properties and sets components through properties named after them, field by field. A "Spawned as" section lists the components the engine gave the object.
- Saving a map writes only the object layers that changed, and inside them only the changed objects. The rest of the file is copied byte for byte, so Tiled still opens it and a diff shows just the edit; an unchanged map saves identically, and tests check that on all four platformer maps. Saving splices into the file as it is on disk, so tiles saved in Tiled meanwhile are kept.
- Tiled stays the tilemap editor. When the open map is saved in Tiled, the editor follows: it reloads the objects if there are no edits here, and asks (reload or keep mine) if there are. The edits are written into the map asset as they happen, so the view, the spawned prefabs and Play see them before they are saved.
- Editing prefabs (`.prefab.ron`): a component tree whose nodes can be dragged between parents, children added, duplicated and deleted. Each component is edited field by field: numbers, vectors, strings, colours, lists, options, and enums with their variants. A "RON" toggle edits a component as text. The file's header comments are kept when saving, and the result is checked by reading it back before it is written. The prefab is shown live as a preview, particles and all.
- Component editing through serde, not reflection: `cg_editor::value::Value` captures any `Serialize` value with its exact shape and writes it back as RON, which the component's own `Deserialize` reads. Every registered prefab component can be edited with no new derives. An enum's variants are found by asking its `Deserialize`.
- Undo and redo for every change: drags and typing are one step each, selections are restored too, and the Edit menu names the step.
- Play-in-editor: Play (F5) hands the camera back to the game, lets fixed steps run and sends `PlayStarted`. Stop sends `PlayStopped`, waits for the game to leave its level, despawns every entity made since Play and shows the edited scene again; the document is never touched by Play. In `PlayMode::Game` the game starts its level from the edited map. In `PlayMode::Scene`, the standalone editor, the scene itself runs with physics, animation and Lua scripts. The standalone editor loads every prefab file of the asset folder, named after the file in PascalCase.
- While editing, fixed steps are paused, so nothing falls or runs scripts, but effects in `Update` (particles) keep moving.
- The editor asks before discarding unsaved changes (opening another document or closing the window). Errors in game code while playing are logged to the console instead of closing the editor.
- `--demo`: a scripted session that moves an object, undoes and redoes, places a tile, edits a property, saves into a scratch folder and reads it back, plays for two seconds and stops, checking each step. CI runs it for the platformer and for the standalone editor on the scripting example.
- `games/platformer/editor` (`platformer_editor`): the platformer with the editor built in. It opens level 1 (or the map given) and Play runs the level being edited. Plays keep saves in memory.
- `cg_world`: `EditMode`, `PlayStarted`, `PlayStopped` and the `in_editor` and `editing` run conditions, so a game supports play-in-editor without any `cfg`. `Prefab::components` and `Prefabs::names`.
- `cg_core`: `Time::set_fixed_paused` and `fixed_paused`, which pause the fixed steps alone.
- `cg_platform`: `Window::set_view_insets` (edges kept for a tool's panels, counted in `safe_area`); `Window::set_close_guard` with `Lifecycle::CloseRequested`; `LogCapture` and `LogLine`, the log kept in memory for an in-app console.
- `cg_render`: `OffscreenRenderer::render_frame` with `OffscreenFrame`: an offscreen frame with overlays (egui) included, for screenshots of tools.
- `cg_tiled`: `TiledMap::tile_for_gid` and `TiledMap::gid`.
- `cg_ui`: `DebugOverlay::windows` (tools with their own egui panels turn the overlay's windows off) and `Inspector::describe`.
- `platformer`: `Game`, `Options` (with `Options::from_args`) and `engine_plugins` are public; an `Editor` screen; `Game` reacts to the editor's Play and Stop.

### Changed

- The platformer's `run` is built from `engine_plugins` and `Game`, as the editor builds it.
- The overlay's Scripts window follows `DebugOverlay::windows`.
- `cargo check-wasm`, `clippy-wasm`, `check-android` and `clippy-android` leave out `platformer_editor` as well.

## Phase 12 — Lua scripting

### Added

- `cg_script`: Lua 5.4 scripting through mlua 0.12.2 (Lua's C sources compiled in). A `Script` component (also a prefab component, `"Script": "scripts/patrol.lua"`) runs a `.lua` asset (`LuaScript`, `LuaScriptLoader`) on its entity. The file returns a table of hooks, called in `FixedUpdate` (`ScriptSystems::Run`) with the entity's own `self` table: `on_start`, `on_reload`, `on_event(name, data, from)`, `on_collision(other)` and `on_collision_end(other)` (from the physics' `CollisionStarted` and `CollisionEnded`), and `on_update(dt)`; timers come in between.
- The scripts' API: entity methods for the transform, the kinematic body's velocity and contacts, the sprite's flip and colour, the animation clip, the animator's parameters and triggers, tweens, the Tiled object's properties, names, children, components (`has`), prefabs inserted into an entity, despawning and events to another script (`send`); `world.spawn`, `find`, `raycast`, `tile_at` and `overlap_point`; `input` by action; `audio.play`, `stop`, `music` and `stop_music`; `timer.after` and `every` and the same on `self`; `events.emit` (a `ScriptMessage` for the game) and `broadcast`; `state.current`, `stack`, `push`, `pop`, `replace` and `set`; `time`; `log` and `print`.
- `AppScriptExt`: games add their own Lua functions (`add_script_function("game", "set_checkpoint", |world, (x, y): (f32, f32)| ...)`, with arguments and results as `ScriptValue`s or plain Rust types), component names for `entity:has(name)` (`add_script_component`), and the game states scripts may enter (`add_script_states`). `ScriptEvents` sends events from Rust to scripts.
- A sandbox: no `os`, `io`, `package`, `require`, `debug`, `load`, `dofile` or `string.dump`; each file has its own globals; `math.random` starts from a fixed seed; a hook that runs more than 20 million instructions (an endless loop) is stopped (`ScriptPlugin::max_instructions`), and Lua's memory is capped at 64 MiB (`memory_limit`).
- Errors name the script's file and line, also for errors in the engine's functions (`scripts/walker.lua:31: set_velocity: entity 12v0 has no KinematicBody`). A file that does not compile keeps its previous version running; a hook that fails stops that entity's script until the file changes. Both are logged and kept in `ScriptErrors`; `ScriptStatus` lists every script file with its entities and loads.
- Hot reload of scripts on desktop, as for other assets: a saved file is compiled again, and every entity running it keeps its `self` and gets `on_reload`.
- The debug overlay's Scripts window (with the `debug-overlay` feature): each script file with its entities, loads and state, and the latest errors with their tracebacks. The overlay opens by itself at a script's first error in debug builds (`ScriptPlugin::show_errors`).
- `Scripting::runs`: whether scripts run here. On the web `cg_script` builds without Lua (mlua cannot build for `wasm32-unknown-unknown`): the plugin, components and API are there, scripts never run, and games use Rust versions of their scripted behaviours.
- `DefaultPlugins::script` and the `--no-scripts` flag, with the facade's `script` feature; the prelude has `cg_script`'s.
- The platformer's walkers (`assets/scripts/walker.lua`) and checkpoint flags (`assets/scripts/checkpoint.lua`, through `prefabs/checkpoint.prefab.ron`) are Lua scripts on desktop and Android, and stay Rust on the web (and with `--no-scripts`). A test plays the whole game with each and checks that every walker is in the same place at every step of play, with the same checkpoints and results.
- `examples/scripting`: a level whose player, coins, patrolling blocks, crate spawner and goal are Lua scripts on prefabs placed in Tiled, with a HUD fed by script messages, the latest script error on screen, and a `--demo` bot written in Lua. CI runs it, and the platformer's demo with `--no-scripts` too.

### Changed

- `cargo check-android` and `cargo clippy-android` run through cargo-ndk (`cargo ndk -t arm64-v8a`), which hands the NDK's clang to Lua's C sources; they need cargo-ndk and an Android NDK now.
- New dependency: mlua 0.12.2 (with `lua54` and `vendored`: lua-src builds Lua 5.4 from source with the target's C compiler), on every target but wasm32.

## Phase 11 — Android build

### Added

- The platformer on Android: `games/platformer/android` holds the game's Android library (`platformer_android`, a `cdylib` whose one line is `cashgear_engine::android_main!(platformer::run)`) and a Gradle project that builds it with cargo-ndk, bundles the assets with `cg_pack`, and packages a debug APK (arm64 and x86_64), a release APK and a release AAB for Google Play (adding armeabi-v7a). App id `com.cashgear.platformer`, "CashGear Platformer", Android 8.0 (API 26) and newer, targeting API 36, landscape, full screen, with an adaptive launcher icon. Release signing reads `keystore.properties` or `CG_ANDROID_*` environment variables; the README shows how to make a keystore.
- `cashgear_engine::android_main!`: declares a game's Android entry point around the same `run` function its desktop `main` calls.
- `com.cashgear.engine.CashGearActivity` (`crates/cg_platform/android/java`): the engine's `NativeActivity`. It hides the system bars (immersive mode), keeps the screen on, lays the window out under display cutouts and records their safe insets, counts back presses instead of closing the app (`OnBackInvokedCallback` on Android 13 and newer), keeps the intent's `args` extra as launch flags, and ends the process when it is destroyed, so every launch starts fresh.
- `cg_platform::android`: `run` (called by the macro), `app` and `AndroidApp`. On Android, `AssetIo` reads the APK's assets through the asset manager, and uses `assets.cgpak` there as its bundle by itself (`io::BUNDLE_FILE`); `Storage` writes files in the app's private folder; logs and panics go to logcat under the `cashgear` tag; `Args` reads the intent's `args` extra (`adb shell am start ... --es args "--demo"`).
- Android's back button and gesture arrive as `KeyCode::BrowserBack`, which `ui_back` is now bound to by default. The platformer pauses a level on it, goes back in menus, and quits from the title screen.
- `Window::safe_area`, the part of the window no display cutout covers, and `Window::with_safe_insets` for tests. The renderer fits the camera's view inside it (`Camera2D::viewport_in`, `screen_space_in`, `CameraView::viewport_in`), so the bars cover notches; the UI, the fade and screen/world conversions follow.
- `impl Termination for AppExit`: `fn main() -> AppExit { app.run() }` ends the process with the app's exit code.
- `cargo check-android` and `cargo clippy-android` aliases, and the Android targets in `rust-toolchain.toml`.
- CI: an `android` job runs `clippy-android`, builds the debug APK and the release AAB (signed when the repository has the `CG_ANDROID_*` secrets), reports their sizes and uploads them as the `platformer-android` artifact. GitHub Pages now also waits for it.
- The `android-release` Cargo profile (LTO, one codegen unit, no debug info).

### Changed

- The platformer is a library (`platformer::run`) with a small `main.rs`, so the Android library can run it.
- Every example's `main` and the platformer's return the app's `AppExit`, so a demo bot that fails now fails its CI step; before, the process exited with 0 either way.
- The renderer drops the window surface in `LifecycleChanged` the moment the app is suspended, not in the next frame, which never comes on Android before the native window is gone.
- `Window::fullscreen` is always true on Android, and `Window::can_fullscreen` false (games there are always full screen).
- The platformer's rebinding prompt is cancelled by the back button as well as Escape.
- New dependency: jni 0.22.4 (Android only; already in the tree through cpal). winit's `android-native-activity` feature on Android.

## Phase 10 — Web build


### Added

- The platformer's web release: `trunk build --release --no-default-features` in `games/platformer` builds with the new `web-release` profile (opt-level `z`, LTO, one codegen unit) and wasm-opt, without the debug overlay, and packs the assets into one bundle. A loading screen with one progress bar for the code and the assets, an error panel with a Reload button (no WebGPU or WebGL2, a crash, a failed download), a "turn your phone sideways" notice in portrait that also pauses the game, landscape lock in fullscreen on Android, a web manifest and icons so "Add to Home Screen" opens it full screen in landscape, and no page scrolling, zooming or long-press menu over the game.
- CI: a `web` job builds the release, reports the download size per file in the job summary and fails above 8 MB, and uploads the `platformer-web` artifact (a zip with `index.html` at its root, ready for itch.io). On `main`, once every job passes, the build is published to GitHub Pages.
- `cg_platform::io::Bundle`: a whole asset folder in one file (`.cgpak`), with text, maps and fonts deflated and images and sounds stored as they are. `AssetIo` reads files from a bundle first and from the folder when the bundle lacks them: one given with `AssetIo::with_bundle` (`AssetPlugin::bundle`, `--bundle <path>`) or `with_loaded_bundle`, or on the web the one the page downloaded and handed over as `window.cashgearBundle`. Hot reloading is off while a bundle is in use.
- `cg_pack bundle <folder> <out.cgpak>`, and `cg_pack trunk-bundle`, a trunk `post_build` hook that bundles a page's `assets/` in release builds and drops the loose copy.
- `cg_platform::page`: the `cashgear-ready` event once the first frame is drawn and `cashgear-error` (with the message) when the game cannot go on, so a page can follow the game. The renderer and the web panic hook send them; an error exit sends `cashgear-error` too.
- Fullscreen: `Window::fullscreen`, `set_fullscreen` and `can_fullscreen` (desktop, and browsers that allow it; not Safari on iPhone). On desktop F11 and Alt+Enter toggle it (`WindowConfig::fullscreen_keys`). The platformer's Settings has a Fullscreen row where the platform can do it.
- The platformer pauses itself when the player looks away during a level: another window or tab, a minimized window, a click outside the game on a web page, a phone turned upright. Not in the demo.

### Changed

- Every example's page uses the `web-release` profile and wasm-opt for release builds.
- The platformer's debug overlay is its default `overlay` feature, so web releases can leave egui out.
- The platformer's bot finds the Controls button in Settings by its action, whatever rows the platform shows.
- New dependency: miniz_oxide 0.9.1 (deflate for bundles; already in the tree through `tiled`).

### Fixed

- After a hidden browser tab (or a minimized window) came back, the first frame counted all the time away (clamped to a quarter second), so the game lurched forward. The clock now restarts when the window is hidden or shown.

## Phase 9 — Platformer sample game

### Added

- `games/platformer`: the sample game. Kenney's green alien runs, jumps (higher while the button is held, with coyote time and a jump buffer), slides down walls and wall-jumps through three levels made in Tiled: Leafy Woods, Sunset Dunes and Frosty Peaks. Coins, checkpoint flags, springs, moving platforms, an exit door; walkers, spikers (which cannot be stomped), hoppers and bats; spikes and pits. Three hearts per level: a hit costs one with a knock back, a fall costs one and returns to the last checkpoint, and losing the last one is game over. A title screen over a drifting backdrop, level select, settings (volume levels, language, touch controls, controls), a controls screen where each action's two keys and gamepad button can be rebound, pause, level complete, game over and a final screen; a HUD with hearts, coins, time and the level's name; touch controls; English and Arabic. Progress (unlocked levels, best coins and time per level), the controls, the volume levels and the language are saved. `--demo` lets a bot play the whole game from the title screen (an ignored test renders its screens to PNGs); `--level N` starts in a level. Web page on port 8088.
- `cg_physics`: moving platforms. Static colliders moved by game code carry the bodies standing on them, lift bodies they rise into, and push bodies they move into sideways (as far as walls allow). `PhysicsWorld::motion` says how far a static collider moved in the latest step.
- `cg_physics`: `OneWay`, which makes a static collider a one-way platform (landed on from above, passed from below and the sides, dropped through with `drop_through`, hit by rays only from above). A prefab component, and set by a Tiled object's `one_way` property next to `collider`. Debug drawing shows one-way colliders in cyan.
- `cg_platform`: `Binding::just_pressed` (the first key or gamepad button pressed this frame, for "press a key" prompts), `Binding` names for menus (`Display`: `Space`, `K`, `Left`, `Pad A`, `Stick Left`), `Binding::is_key` and `is_gamepad`.
- `cg_platform`: `Target` (`Desktop`, `Web`, `Android`), so games choose by platform without `#[cfg]`; `Target::can_quit`.
- `cg_ui`: `UiFocus::set_paused` and `is_paused`, which stop keyboard and gamepad navigation while a prompt waits for any key.
- New sounds and music, synthesized for the game: a stomp, a spring, a checkpoint chime, a fall, a game-over jingle, and two loops (a desert tune in D Hijaz and a bright one in E Lydian). The meadow and heights tunes and the earlier effects are reused.
- CI: `platformer --demo`.

## Phase 8 — Text and UI

### Added

- `cg_ui`: fonts. `Font` assets from TrueType and OpenType files, BMFont bitmap fonts (`.fnt` with PNG pages) and `.font.ron` files with settings (`pixel`, `line_height`). Pixelify Sans (SIL OFL 1.1) is built in as the default font, sharp at 11 UI pixels and multiples.
- `cg_ui`: `Text` in a font, size and colour, aligned (`Start`, `Center`, `End`, `Left`, `Right`, `Justified`) and wrapped in its node. Outline fonts are shaped by cosmic-text: Arabic joins, mixed left-to-right and right-to-left text is ordered, and characters missing from a font come from another loaded font. Pixel fonts are rasterized at UI pixels without smoothing, exactly on the pixel grid; others at the screen's resolution. Glyphs live in an atlas drawn by the sprite batcher. `TextSystem` measures text for games too.
- `cg_ui`: UI nodes. `Node` with widths and heights in pixels, percent, `Auto` (content) or `Fill`, an anchor and offset, padding, a border, and free, row or column layouts with gaps, alignment and justification; `ZIndex`; `ComputedNode` and `UiStack` as results. The UI is laid out in the camera's virtual pixels and scaled with the game; right-to-left languages mirror it (`LayoutDirection` overrides per node).
- `cg_ui`: widgets. `BackgroundColor`, `BorderColor`, `UiImage` (with nine-slicing), `Button` (sending `Clicked`), `Slider` (dragged or moved by keys, sending `SliderChanged`), `ProgressBar`, with `BarFill` and `SliderThumb` children; `StateColors` and `StateImages` for hovered, pressed and disabled looks; `Interaction`.
- `cg_ui`: focus navigation by keyboard and gamepad. The `ui_up`, `ui_down`, `ui_left`, `ui_right`, `ui_accept` and `ui_back` actions are bound by default (arrows and WASD, Enter, Space, Escape; d-pad, left stick, south and east buttons); the focus moves to the nearest widget in the pressed direction within the topmost menu, repeats when held and wraps round; `UiFocus`, `Focusable { auto_focus }`. Mouse and touch hover, press and click; `UiPointer::over_ui` tells games when a click hit the UI.
- `cg_ui`: on-screen touch controls. `TouchButton` and `TouchDpad` press actions through `VirtualInput` while fingers are on them; `TouchControls` shows them on touch screens, after the first touch, or always (`--touch`) or never (`--no-touch`).
- `cg_ui`: localization. `Strings` tables per language in `.strings.ron` files (with the language's name, direction, and optionally its own font and size), `Locale` with fallback to the default language and `{name}` arguments, `Localized` text that follows the language, and the chosen language saved in `Storage` (`UiPlugin::locale`, `LocaleSettings`).
- `cg_ui`: the egui debug overlay (feature `debug-overlay`, on by default; F12, or `--overlay`): a frame-time graph, render stats, collider and UI outline toggles, the entity list and a component inspector (values of registered `Debug` types, `Transform2D` editable; `InspectExt::register_inspect`). Game systems can draw their own egui windows through `EguiContext`.
- `cg_render`: `ScreenSpace`, the UI's coordinate system in the window; a screen-space sprite queue (`Sprites::push_screen`, `QueuedSprite::screen_rect`) drawn after the world at the window's resolution; `RenderOverlay` passes drawn last (`RenderOverlays`); `VisibilitySystems`; `Image::data_mut`.
- `cg_platform`: `VirtualInput` (actions pressed by on-screen controls, menus and bots), `TextInput` (the text typed each frame), the `InputSystems` sets, and `Window::touch_screen`. `cg_core`: `App::configure_sets`.
- `examples/ui` (web page on port 8087): a pixel-art main menu, settings (volume sliders, touch controls, the overlay), a HUD with a bitmap-font clock, an energy bar and touch controls, and a pause menu, in English and Arabic; `--demo` lets a bot click through every screen, and an ignored test renders each screen to PNGs.
- A reference-image test of a UI screen in `crates/cg_ui/tests/golden/`.
- CI: `ui --demo --overlay`.

### Changed

- `DefaultPlugins` adds `UiPlugin` (field `ui`, flags `--touch`, `--no-touch`, `--overlay`); the prelude includes the UI. The facade's `debug-overlay` feature (default) turns the overlay on.
- `ScreenFade` draws in screen space over everything, the UI and the bars included, so menus fade with the game.
- `ActionState` counts actions held or tapped in `VirtualInput`, also ones without bindings.
- `examples/audio` draws its volume panel with UI nodes, labelled, in screen space, so camera shake no longer moves it.
- The log filter hides egui-wgpu's messages below errors.
- New dependencies: cosmic-text 0.19.0 (with swash; no system fonts), egui 0.37.0, egui-wgpu 0.37.0, unicode-script 0.5.8. Fonts: Pixelify Sans (built in) and Noto Kufi Arabic (in the example), both SIL OFL 1.1.

### Fixed

- The sprite batcher's per-sprite instance builder is inlined again; splitting the batcher for the screen queue had cost release bunnymark 0.1 ms a frame at 10,000 sprites.

## Phase 7 — Audio

### Added

- `cg_audio`: `Sound` assets from OGG Vorbis and WAV files (mono or stereo), decoded by symphonia. Sounds up to `AudioPlugin::stream_longer_than` seconds (10 by default) are decoded when they load; longer ones stay compressed and are streamed while they play, a quarter second per frame on the main thread into a two-second buffer, the same on every target and without threads. Loops are seamless: the decoder trims the encoder's delay and padding. `Sound::from_mono` and `from_stereo` make sounds in code.
- `cg_audio`: the `Audio` resource. `play`, `play_at` and `play_with` start a `Voice` with `PlaySettings` (bus, volume, speed, random pitch jitter in semitones, looping, fade-in, a position or an entity to follow, distance falloff, a per-sound cap); `stop`, `pause`, `resume`, `set_volume`, `set_speed`, `set_position` and `stop_bus` change it, all with fades. `status` says whether sound is waiting, running, paused or off; `stats` counts voices, streams, underruns and sounds cut by the caps.
- `cg_audio`: music with `play_music` (crossfading from the track playing; the same track again keeps playing) and `stop_music`.
- `cg_audio`: buses. Music, effects and interface tracks under a master track in kira's mixer. `AudioSettings` holds a level per bus and the master (applied as loudness: amplitude is the level squared) and a mute switch; it is loaded from `Storage` at start-up and saved whenever it changes (`AudioPlugin::settings_key`).
- `cg_audio`: voice caps. At most 32 effects and interface sounds at once and 4 copies of one sound (`AudioPlugin::max_voices`, `max_instances`, `PlaySettings::max_instances`); past them the oldest copy that does not loop is cut short with a 20 ms fade.
- `cg_audio`: simple 2D positional audio. `Spatial` gives a full-volume radius, a smooth falloff to silence and a pan by horizontal offset; sounds are heard from the `AudioListener` entity, or else the active camera (before shake). `AudioEmitter` loops a sound from an entity while it has the component. Positioned one-shots out of earshot are skipped.
- `cg_audio`: the audio device opens when the game first asks for a sound. On the web that waits for the user's first key press, click or touch; music and looping sounds asked for before then start then. Sound pauses while the app is suspended (Android) or the window hidden (a background tab, a minimized window; `AudioPlugin::pause_when_hidden`). `AudioOutput::Mock` runs without a device for tests; `--no-audio` (`AudioOutput::Off`) runs silent. A device that cannot be opened is logged once and the game runs silent.
- `cg_platform`: `storage::Storage` for small text values kept between runs: one file per key in a per-user folder on desktop (`CG_SAVE_DIR` overrides it), `localStorage` on the web, and a stub on Android until Phase 11. `StorageBackend` lets games store elsewhere; `MemoryStorage` is for tests. `StoragePlugin` inserts it, with a `namespace` per game.
- `cg_platform`: `Window::user_activated`, whether the user has interacted with the page (browsers' rule for starting sound), from `navigator.userActivation` where the browser has it.
- `cg_platform`: the `LifecycleChanged` schedule, run at once when the app is suspended or resumed or the window hidden or shown, since no frame may follow (Android in the background, a hidden browser tab).
- `examples/audio` (web page on port 8086): the alien's level with footsteps from animation events, jumps, landings as loud as the fall, a hit, coins that sound from where they are and rise a tone each in a run, a goal chime, a campfire and a fountain placed in Tiled (`Ambient` objects) that pan as you pass, music that crossfades between two Tiled `MusicZone`s, and a volume panel (1–4, -/=, M) saved between runs. The music and sounds are original, synthesized for the example. `--demo` lets a bot play; a test runs it with the mock device.
- CI: `libasound2-dev` on Ubuntu, and `audio --demo`.

### Changed

- `DefaultPlugins` adds `StoragePlugin` (field `storage`) and `AudioPlugin` (field `audio`, flag `--no-audio`); the prelude includes the audio items, `Storage` and `StoragePlugin`.
- The log filter hides symphonia's messages below warnings, on desktop and (through a small filtering logger in front of `console_log`) on the web.
- New dependencies: kira 0.12.6 (only its cpal backend), symphonia 0.6.1 (OGG, Vorbis, WAV and PCM; MPL-2.0), rtrb 0.4.1.

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
