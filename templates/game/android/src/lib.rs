//! {{title}}'s Android library. The APK's activity loads it and calls `android_main`,
//! which runs the same game as on desktop and the web. Build it with Gradle from this
//! folder (`gradlew assembleDebug`); see `README.md`.

cashgear_engine::android_main!({{crate_name}}::run);
