# Android

Games run as native Android apps (Android 8.0 and newer), built by Gradle with the Android Gradle plugin; cargo-ndk compiles the Rust library. The template's `android/` folder has the library (`android/src/lib.rs`, one line) and the Gradle project.

## Setting up

1. Install **JDK 17** and the **Android SDK** (Android Studio's SDK manager, or the command-line tools) with platform 36, build tools 36 and **NDK 29.0.14206865**. Set `ANDROID_HOME` to the SDK (or write `sdk.dir` in `android/local.properties`).
2. `cargo install cargo-ndk --locked`, and `cg_pack` (see [Getting started](getting-started.md)).
3. `rustup` installs the Android targets from `rust-toolchain.toml` on the first build.

## Building

From `android/`:

```sh
./gradlew assembleDebug -PcgAbis=arm64-v8a      # a debug APK for a phone, quickly
./gradlew assembleDebug                         # arm64 and x86_64 (emulators)
./gradlew bundleRelease -PversionName=1.0.0     # the release AAB for Google Play
```

The Gradle build runs cargo-ndk for each ABI (release builds add 32-bit `armeabi-v7a`), packs `assets/` into `assets.cgpak` stored uncompressed in the APK, and adds the engine's Java activity, `CashGearActivity`. `versionName` sets the version, and the version code Google Play needs (`1.2.3` is 10203).

Install and watch the log:

```sh
adb install -r app/build/outputs/apk/debug/app-debug.apk
adb logcat -s cashgear
adb shell am start -n <app id>/com.cashgear.engine.CashGearActivity --es args "--demo"
```

## What the activity does

It runs full screen (immersive) in landscape, keeps the screen on, lays the game out under display cutouts (the view keeps clear of them), turns the back button and gesture into `ui_back`, and ends the process when Android destroys it, so each launch starts fresh. When the app goes to the background the game pauses its sound and drops its window surface; it makes a new one on return. Saves go to the app's private files folder.

## Signing

Google Play takes signed AABs. Make an upload key once:

```sh
keytool -genkeypair -v -keystore upload.jks -keyalg RSA -keysize 2048 -validity 10000 -alias upload
```

and name it in `android/keystore.properties` (kept out of git):

```properties
storeFile=upload.jks
storePassword=...
keyAlias=upload
keyPassword=...
```

Or set `CG_ANDROID_KEYSTORE`, `CG_ANDROID_KEYSTORE_PASSWORD`, `CG_ANDROID_KEY_ALIAS` and `CG_ANDROID_KEY_PASSWORD`, as CI does from secrets. Without them, release builds are unsigned.

## Limits

No gamepads yet, no soft keyboard, and the debug overlay is left out (it is driven by keys). Panic backtraces in logcat show addresses: the Android Gradle plugin strips the libraries it packages.
