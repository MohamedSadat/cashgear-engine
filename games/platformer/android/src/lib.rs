//! The platformer's Android library. The APK's activity loads it and calls
//! `android_main`, which runs [`platformer::run`], the same game as on desktop and the web.
//!
//! Build it with Gradle from this folder (`gradlew assembleDebug`), which runs cargo-ndk;
//! see `README.md`.

cashgear_engine::android_main!(platformer::run);
