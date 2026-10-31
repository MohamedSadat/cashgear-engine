# {{title}}

A game made with [CashGear Engine](https://github.com/MohamedSadat/cashgear-engine): an alien runs and jumps through a level made in Tiled, takes coins and reaches the door. It runs on Windows, Linux and macOS, in the browser, and on Android, from the same code.

The [CashGear guide](https://mohamedsadat.github.io/cashgear-engine/book/) explains the engine; this file is how to run and ship this game.

## Run it

```sh
cargo run                      # desktop: arrows or A/D to run, Space to jump, F12 for the debug overlay
cargo run -- --demo            # a bot plays the level and quits (the test does the same)
cargo test                     # the bot plays without a window
cargo run -p {{crate_name}}_editor   # the scene editor, with the game built in
```

While the game runs on desktop, saving a file in `assets/` reloads it: the level (`maps/level.tmx`, in [Tiled](https://www.mapeditor.org/) or the editor), the coins' Lua script, the player's animation states.

## Where things are

| Path | What |
| --- | --- |
| `src/lib.rs` | The game plugin and its screens; start here |
| `src/level.rs` | The Tiled map and its objects: the start, coins, the door |
| `src/player.rs` | Running and jumping, the controls, the alien's animation |
| `src/screens.rs` | The camera, the title, the HUD with touch controls, the win screen |
| `src/bot.rs`, `src/tests.rs` | The `--demo` bot and the test that runs it |
| `assets/` | Art (Kenney's Pixel Platformer, CC0), the map, sounds, the Lua script |
| `index.html`, `web/`, `Trunk.toml` | The web page |
| `android/` | The Android library and its Gradle project |
| `editor/` | The game with the scene editor built in |

## Tools

```sh
cargo install trunk --locked                                   # the web build
cargo install cargo-ndk --locked                               # the Android build
cargo install --git https://github.com/MohamedSadat/cashgear-engine --tag v0.1.0 cg_pack   # packs assets into one bundle
```

`rust-toolchain.toml` pins the Rust version the engine was released with; `rustup` installs it, with the web and Android targets, on the first build.

## The web

```sh
trunk serve                                                    # debug build: http://127.0.0.1:8080
trunk build --release --no-default-features --public-url ./    # release: dist/ is the whole game
```

`dist/` works on any static host: GitHub Pages, itch.io (zip it with `index.html` at the root) or your own server. The release packs `assets/` into one `assets.cgpak` and leaves out the debug overlay. Lua does not run in browsers, so on the web the coins do not bob; see the guide's Lua chapter for games that need their scripted behaviour there too.

## Android

Install the Android SDK with the NDK (Android Studio's SDK manager, or the command-line tools) and JDK 17, then from `android/`:

```sh
./gradlew assembleDebug                                        # android/app/build/outputs/apk/debug/app-debug.apk
adb install -r app/build/outputs/apk/debug/app-debug.apk
./gradlew bundleRelease -PversionName=1.0.0                    # the AAB for Google Play
```

On Windows use `gradlew.bat`. Gradle finds the SDK through `ANDROID_HOME` (or `local.properties`). The app id is `{{app_id}}`; it runs in landscape, full screen, from Android 8.0. Release builds are signed when `android/keystore.properties` (or the `CG_ANDROID_*` environment variables) names your upload keystore:

```sh
keytool -genkeypair -v -keystore upload.jks -keyalg RSA -keysize 2048 -validity 10000 -alias upload
```

```properties
storeFile=upload.jks
storePassword=...
keyAlias=upload
keyPassword=...
```

Keep the keystore and its passwords out of git (`.gitignore` already does).

## Licenses

The art in `assets/` is Kenney's Pixel Platformer, CC0 (`assets/KENNEY-LICENSE.txt`). The engine is MIT or Apache-2.0. Your game's code is yours.
