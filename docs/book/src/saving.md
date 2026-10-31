# Saving

`Storage` keeps small text values by key: settings, progress, save slots. It writes files in the user's data folder on desktop (`%APPDATA%\CashGear\<game>` on Windows), `localStorage` in browsers, and the app's private folder on Android. Give your game its own namespace:

```rust,ignore
DefaultPlugins {
    storage: StoragePlugin { namespace: Some("my-game".into()), ..default() },
    ..default()
}
```

```rust,ignore
fn save(storage: Res<Storage>, progress: Res<Progress>) {
    let text = ron::ser::to_string(&*progress).unwrap();
    if let Err(err) = storage.write("progress.ron", &text) {
        warn!("could not save: {err}");
    }
}

fn load(storage: Res<Storage>) -> Option<Progress> {
    storage.read("progress.ron").ok().flatten().and_then(|text| ron::from_str(&text).ok())
}
```

Keys are 1 to 64 letters, digits, `.`, `_` or `-`. Writes are atomic on desktop (a crash keeps the previous value). Storage is synchronous, made for small values; the engine saves audio settings, the language and the template's progress this way.

`StoragePlugin { memory: true, .. }` keeps everything in memory instead: for tests, bots and play from the editor, so they never touch the player's saves.
