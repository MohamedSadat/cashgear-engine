# Assets

A game's files live in its `assets/` folder and are loaded by path, the same on every target: from the disk on desktop, with `fetch` on the web, from the APK on Android.

```rust,ignore
fn setup(server: Res<AssetServer>, mut loading: ResMut<LoadingAssets>, mut commands: Commands) {
    let map: Handle<TiledMap> = server.load("maps/level1.tmx");
    loading.add(&map);
    commands.insert_resource(Level { map });
}
```

`load` returns a `Handle<T>` at once and the file loads in the background; `Assets<T>` holds the loaded values (`assets.get(handle.id())`). An asset stays loaded while a strong handle to it exists, and is unloaded in the next frame after the last one drops. `AssetEvent<T>` messages report when assets are added, modified or removed.

| Type | Files |
| --- | --- |
| `Image` | `.png` |
| `TextureAtlas` | `.atlas.ron` (from `cg_pack atlas`) |
| `TiledMap` | `.tmx` (with `.tsx` tilesets) |
| `SpriteSheet` | Aseprite `.json` exports |
| `AnimationGraph` | `.anim.ron` |
| `Sound` | `.ogg`, `.wav` |
| `Font` | `.ttf`, `.otf`, `.fnt` (BMFont), `.font.ron` |
| `Strings` | `.strings.ron` |
| `PrefabSource` | `.prefab.ron` |
| `LuaScript` | `.lua` |

Loaders that need other files (a map's tilesets and images) load them as dependencies; `is_loaded` and `recursive_load_state` include them.

## Loading screens

Put the assets a state needs in `LoadingAssets`, and `app.add_loading_state(Loading, Next)` moves on when they are all loaded (or failed, each failure logged). `LoadingAssets::progress` drives a loading bar. `--asset-delay 2` holds every load back two seconds, to see the screen on a fast disk.

## Hot reload

In debug builds on desktop, saving a file in `assets/` reloads it while the game runs: images, maps (which rebuild their level), prefabs (for entities spawned afterwards), animation graphs, strings and Lua scripts. A file that fails to load keeps the previous version. `--no-hot-reload` turns it off; `AssetServer::reload(path)` reloads by hand on any target.

## Where `assets/` is

Under `cargo run`, the folder next to the game's `Cargo.toml`. A shipped executable looks next to itself; `CG_ASSET_ROOT` or `--asset-root <path>` overrides both. On the web, trunk copies the folder next to `index.html`.

## Bundles

`cg_pack bundle assets/ game.cgpak` packs a folder into one file, compressing what shrinks. The web release and the Android APK carry the assets that way (`assets.cgpak`), and desktop builds can read one with `--bundle <path>`. A bundle is read before the folder; files missing from it come from the folder.

## Atlases

`cg_pack atlas <folder> <out.atlas.ron>` packs a folder of images into one texture and a RON file of named regions. Commit both; `--check` fails when they are out of date, for CI. At runtime, `AtlasSprite { atlas, region }` draws a region by name.
