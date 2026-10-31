# The scene editor

The scene editor places and tunes a level's entities without code. It edits the object layers of Tiled maps in place, and prefab files; Tiled stays the editor for tiles. It runs on desktop.

The editor is a library a game builds in, so **Play** runs the game's own code on the level being edited. The template's `editor/` crate is that build:

```sh
cargo run -p my_game_editor
cargo run -p my_game_editor -- maps/level2.tmx
```

Or on its own, for any asset folder, where Play runs only the engine (physics, animation, Lua):

```sh
cargo run -p cg_editor -- --asset-root path/to/assets maps/level.tmx
```

## What it shows

The level in the middle, drawn by the engine; around it a hierarchy of the map's layers and objects (and the prefab's tree), a palette of the tileset's classed tiles, prefabs and shapes, an inspector, an asset browser with thumbnails, and a console with the engine's log.

## Editing

| Key | Does |
| --- | --- |
| Click, Shift+click, drag a box | Select |
| W, E, R | Move, rotate, resize tools |
| Drag | Move, snapped to the grid (Ctrl: free) |
| Arrows (Shift: one pixel) | Nudge |
| Middle or right drag, Space+drag | Pan |
| Wheel | Zoom |
| F | Frame the selection |
| Delete, Ctrl+D | Delete, duplicate |
| Ctrl+Z, Ctrl+Y | Undo, redo |
| Ctrl+S | Save |
| F5 | Play, and Stop |

The inspector edits an object's name, class, position, size, rotation and custom properties, and every registered component it gets, field by field (numbers, vectors, colours, lists, enums), through each component's own serde code. "Spawned as" lists what the engine made of the object.

## Saving

Saving writes only the changed objects back into the `.tmx`; everything else stays byte for byte as Tiled wrote it, so Tiled still opens the map and a diff shows just your edit. Saving the map in Tiled while the editor is open updates the editor too (it asks if you have unsaved edits).

## Play

Play hands the camera to the game and lets fixed steps run; the game starts the level from the edited map (`PlayStarted` tells it which). Stop despawns everything made during play and brings back the edited scene as it was. To support it in a game:

- Add `EditorPlugin` after the engine's plugins and before the game's.
- When `EditMode` exists at build time, start on an idle state instead of the title.
- On `PlayStarted`, start the level; on `PlayStopped`, go back to the idle state.

The template's `level.rs` (`play_in_editor`) and `lib.rs` do exactly this.
