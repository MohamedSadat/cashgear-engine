# A tour of the template

The template is a small, complete game: a title screen, one level made in Tiled, an alien that runs and jumps, coins, spikes, a door, a win screen, touch controls, a Lua script, a bot that plays it, and the web, Android and editor builds around it. The excerpts below are the template's own code (its `{{title}}`-style placeholders are filled in when you generate a game).

## The layout

| Path | What |
| --- | --- |
| `src/lib.rs` | The game plugin, its screens (a state enum) and the engine's settings |
| `src/main.rs` | The desktop and web entry point: calls `run` |
| `src/level.rs` | The level's assets, its objects and collecting coins |
| `src/player.rs` | Controls, running and jumping, the alien's animation |
| `src/screens.rs` | The camera, the title, the HUD and the win screen |
| `src/bot.rs`, `src/tests.rs` | The `--demo` bot, and the test that lets it play |
| `assets/` | Art, the map, a prefab, a Lua script, sounds |
| `index.html`, `web/`, `Trunk.toml` | The web page and its loading screen |
| `android/` | The Android library (`cdylib`) and its Gradle project |
| `editor/` | The game with the scene editor built in |

## The game is a plugin

Everything the game adds is in one plugin, so the desktop binary, the Android library, the editor and the tests all build the same game around their own engine settings:

```rust,ignore
{{#include ../../../templates/game/src/lib.rs:run}}
```

```rust,ignore
{{#include ../../../templates/game/src/lib.rs:game}}
```

`Screen` is the game's state: `Loading` until the assets are in, then `Title`, `Playing` and `Won` (pushed over `Playing`, so the level stays behind it). Gameplay systems go in `FixedUpdate`, in the game's own sets, which run only while a level is played.

## Loading

The level's assets load at start-up; `add_loading_state(Screen::Loading, ...)` moves on when everything in `LoadingAssets` is in:

```rust,ignore
{{#include ../../../templates/game/src/level.rs:load}}
```

## The level

The map is a Tiled file. Its tiles collide by the tileset's properties (`solid`, `one_way`, `hazard`) on layers marked `collision`, and its objects spawn as entities with a `TiledObject`: the game looks at their class.

```rust,ignore
{{#include ../../../templates/game/src/level.rs:objects}}
```

Coins and the door are tiles with a `sensor` property, so they get sensor colliders, and touching one sends a `CollisionStarted` message:

```rust,ignore
{{#include ../../../templates/game/src/level.rs:collect}}
```

The `Coin` class also gives coins the `Coin` prefab (`assets/prefabs/coin.prefab.ron`), which adds a Lua script that makes them bob.

## The player

The player is a kinematic body with a collider; its picture is a child entity with an Aseprite sprite sheet and an animation state machine:

```rust,ignore
{{#include ../../../templates/game/src/player.rs:spawn}}
```

Controls are actions bound to keys, gamepad buttons and sticks; the on-screen buttons press the same actions:

```rust,ignore
{{#include ../../../templates/game/src/player.rs:controls}}
```

Each fixed step turns the intent (from the controls or the bot) into velocity, with coyote time and a jump buffer, and feeds the animator its parameters:

```rust,ignore
{{#include ../../../templates/game/src/player.rs:movement}}
```

## The screens

UI is entities too: nodes laid out in the camera's 640x360 virtual pixels. The title is a panel with a text and a button:

```rust,ignore
{{#include ../../../templates/game/src/screens.rs:title}}
```

The HUD shows the coins and, on touch screens only, a d-pad and a jump button:

```rust,ignore
{{#include ../../../templates/game/src/screens.rs:hud}}
```

## The bot and the test

`--demo` hands the controls to a bot that runs right and jumps at walls, gaps and spikes, found with raycasts. At the door the game exits with success. `cargo test` runs the same bot without a window, which is how the template (and CI) knows the game still works after a change. Keep a bot as your game grows: it catches what unit tests miss.

## Making it yours

- **The level:** open `assets/maps/level.tmx` in Tiled or the scene editor. Tiles with classes in `tiles.tsx` (`Coin`, `Exit`) are placed as tile objects; the `PlayerStart` point is where the player starts.
- **The art:** Kenney's Pixel Platformer, CC0. Replace `assets/images/` and the tileset; the player's sheet is an Aseprite JSON export.
- **More levels, enemies, menus:** the sample platformer in the engine's repository (`games/platformer`) has three levels, four kinds of enemies, moving platforms, checkpoints, settings with rebindable controls, saved progress and two languages, built the same way.
