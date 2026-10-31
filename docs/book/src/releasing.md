# Releasing

## Your game

A game made from the template ships as:

- **Windows, Linux, macOS:** `cargo build --release`, then the executable with `assets.cgpak` (from `cg_pack bundle assets/ assets.cgpak`) beside it; a shipped game reads the bundle next to itself. Windows release builds open no console window.
- **The web:** `trunk build --release --no-default-features --public-url ./`, and `dist/` (see [The web](web.md)).
- **Android:** `./gradlew bundleRelease -PversionName=1.0.0` from `android/`, signed with your upload key (see [Android](android.md)).

The template's `.github/workflows/ci.yml` checks formatting, lints, runs the bot test and checks the web build on every push.

## Upgrading the engine

The game's `Cargo.toml` names the engine's release tag:

```toml
cashgear_engine = { git = "https://github.com/MohamedSadat/cashgear-engine", tag = "v0.1.0", ... }
```

Change the tag (in both lines), read that release's changelog for what changed, and build. 0.x minor versions may break games; patch versions do not.

## The engine's own releases

The engine is one version for all its crates, following semantic versioning from 0.1.0. A release is a tag:

1. `python tools/release.py bump 0.2.0` sets the workspace version, the template's engine tag and the guide's install commands, and names CHANGELOG.md's `[Unreleased]` section.
2. Write that section of `CHANGELOG.md`; it becomes the release notes.
3. Commit, then tag and push: `git tag v0.2.0 && git push origin v0.2.0`.

The tag starts `.github/workflows/release.yml`. It checks that the tag, the workspace version, the template and the changelog agree (`tools/release.py check`), builds the sample platformer for Windows (a zip), the web (a zip for itch.io or any host) and Android (an APK and an AAB, signed when the repository has the `CG_ANDROID_*` secrets), builds the scene editor for Windows (a zip with the scripting example's assets), and publishes a GitHub Release with the notes. Running the workflow by hand builds everything without publishing.

The engine is not on crates.io; games depend on it through git and a tag.
