# The platformer on Android

This folder is the platformer's Android app:

- `src/lib.rs` and `Cargo.toml`: the Rust library the APK loads (`libplatformer_android.so`). It is one line, `cashgear_engine::android_main!(platformer::run)`, so the game is the same code as on desktop and the web.
- A Gradle project (`settings.gradle.kts`, `app/`) that builds that library with [cargo-ndk](https://github.com/bbqsrc/cargo-ndk), packs `../assets` into one bundle (`assets.cgpak`) with `cg_pack`, and packages both with the engine's activity, `com.cashgear.engine.CashGearActivity` (in `crates/cg_platform/android/java`).

The app id is `com.cashgear.platformer`. It runs on Android 8.0 (API 26) and newer, in landscape, full screen.

## What you need

- Rust with the Android targets (`rust-toolchain.toml` lists them; rustup installs them on first use).
- `cargo install cargo-ndk`
- JDK 17 or newer (`JAVA_HOME`).
- The Android SDK (`ANDROID_HOME`, or `sdk.dir` in `local.properties`) with `platforms;android-36`, `build-tools;36.0.0` and `ndk;29.0.14206865`. Android Studio installs these, or the command-line tools' `sdkmanager`.

## Build and run

From this folder:

```
./gradlew assembleDebug
adb install -r app/build/outputs/apk/debug/app-debug.apk
adb shell am start -n com.cashgear.platformer/com.cashgear.engine.CashGearActivity
```

On Windows use `gradlew.bat`. The debug APK holds arm64 (phones) and x86_64 (emulators) libraries; `-PcgAbis=arm64-v8a` builds only one. Android Studio can open this folder too.

- Logs: `adb logcat -s cashgear` (the engine's log, panics included with a backtrace).
- Launch flags go in the intent's `args` extra, as on the desktop command line:
  `adb shell am start -n com.cashgear.platformer/com.cashgear.engine.CashGearActivity --es args "--demo"` lets the bot play; `--level 2`, `--backend gl` and the engine's other flags work too.
- Saves live in the app's private folder (`files/cashgear-platformer/`); uninstalling removes them.

## Release builds

```
./gradlew bundleRelease     # app/build/outputs/bundle/release/app-release.aab, for Google Play
./gradlew assembleRelease   # app/build/outputs/apk/release/, for installing by hand
```

Release builds use the `android-release` Cargo profile (LTO, no debug info) and add armeabi-v7a for older 32-bit phones.

### Signing

Debug builds are signed with the SDK's debug key. Release builds are signed only when a keystore is configured; otherwise they come out unsigned.

1. Make an upload keystore once, and keep it and its passwords safe (Google Play needs the same key for every update):

   ```
   keytool -genkeypair -v -keystore upload.jks -alias upload -keyalg RSA -keysize 4096 -validity 10000
   ```

2. Next to `settings.gradle.kts`, write `keystore.properties` (git ignores it, and `*.jks`):

   ```
   storeFile=upload.jks
   storePassword=...
   keyAlias=upload
   keyPassword=...
   ```

The `CG_ANDROID_KEYSTORE`, `CG_ANDROID_KEYSTORE_PASSWORD`, `CG_ANDROID_KEY_ALIAS` and `CG_ANDROID_KEY_PASSWORD` environment variables override the file. CI signs the AAB when the repository has the secrets `CG_ANDROID_KEYSTORE_BASE64` (the keystore, base64-encoded), `CG_ANDROID_KEYSTORE_PASSWORD`, `CG_ANDROID_KEY_ALIAS` and `CG_ANDROID_KEY_PASSWORD`.

## On the device

- The back button (or gesture) pauses a level, goes back in menus, and on the title screen closes the game.
- Leaving the app (home, another app, the screen turning off) pauses the level and the sound; coming back resumes where you were.
- The view keeps clear of notches and punch-hole cameras; the bars cover them.
