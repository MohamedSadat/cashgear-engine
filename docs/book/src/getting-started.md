# Getting started

You need Rust and a few tools, then one command makes a new game.

## Install the tools

1. **Rust**, through [rustup](https://rustup.rs/). The game's `rust-toolchain.toml` pins the version the engine was released with (1.99.1 for 0.1), and rustup installs it with the web and Android targets the first time you build.
2. **cargo-generate**, which makes a project from a template:

   ```sh
   cargo install cargo-generate --locked
   ```

3. **cg_pack**, the engine's asset packer, which the web and Android builds call:

   ```sh
   cargo install --git https://github.com/MohamedSadat/cashgear-engine --tag v0.1.0 cg_pack
   ```

4. For the web, **trunk**: `cargo install trunk --locked`.
5. For Android, **cargo-ndk** (`cargo install cargo-ndk --locked`), the Android SDK with the NDK (29.0.14206865), and JDK 17. See [Android](android.md).

On Linux, the engine needs the development packages for gamepads and sound: `sudo apt install libudev-dev libasound2-dev` on Debian and Ubuntu.

## Make a game

```sh
cargo generate --git https://github.com/MohamedSadat/cashgear-engine templates/game --tag v0.1.0 --name my-game
```

cargo-generate asks for the game's title and its Android application id (such as `com.example.mygame`), and writes the project to `my-game/`. Then:

```sh
cd my-game
cargo run
```

The first build compiles the engine and takes a few minutes; later builds take seconds. A window opens on a title screen; press Enter to play. Arrows or A/D run, Space jumps, F12 opens the debug overlay.

```sh
cargo run -- --demo     # a bot plays the level and quits
cargo test              # the same bot, without a window
```

## In the browser

```sh
trunk serve
```

Open <http://127.0.0.1:8080>. trunk rebuilds and reloads the page when you change the game. For a release that you can upload anywhere:

```sh
trunk build --release --no-default-features --public-url ./
```

`dist/` then holds the whole game. More in [The web](web.md).

## On an Android phone

With the SDK, NDK and JDK installed and a phone connected with USB debugging on:

```sh
cd android
./gradlew assembleDebug
adb install -r app/build/outputs/apk/debug/app-debug.apk
```

(`gradlew.bat` on Windows.) The game starts in landscape, full screen, with touch controls. More in [Android](android.md).

## In the scene editor

```sh
cargo run -p my_game_editor
```

The editor opens the level. Move a coin, press F5 to play, F5 again to stop, Ctrl+S to save it back into the map. More in [The scene editor](editor.md).

## Next

[A tour of the template](template.md) walks through the code you just made.
