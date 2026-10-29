//! Save storage: small named text values, such as settings and save games, that survive
//! restarts.
//!
//! | Target | Stored in |
//! | --- | --- |
//! | Desktop | One file per key in a folder per game: `%APPDATA%\CashGear\<namespace>` on Windows, `~/Library/Application Support/CashGear/<namespace>` on macOS, `$XDG_DATA_HOME/cashgear/<namespace>` (else `~/.local/share/cashgear/<namespace>`) on Linux. `CG_SAVE_DIR` replaces the per-user folder. |
//! | Web | The page's `localStorage`, under `cashgear/<namespace>/<key>` |
//! | Android | Not yet (Phase 11): reads find nothing and writes fail with [`StorageError::Unsupported`] |
//!
//! Values are text, so serialize with RON or JSON. Reads and writes are synchronous and
//! meant for small values (settings, a save slot), not for streaming data.
//!
//! ```
//! use cg_platform::storage::Storage;
//!
//! let storage = Storage::memory();
//! storage.write("settings.ron", "(volume: 0.8)")?;
//! assert_eq!(storage.read("settings.ron")?.as_deref(), Some("(volume: 0.8)"));
//! storage.remove("settings.ron")?;
//! assert_eq!(storage.read("settings.ron")?, None);
//! # Ok::<(), cg_platform::storage::StorageError>(())
//! ```

// Desktop storage is one of the few places in the engine that touches the file system.
#![allow(clippy::disallowed_methods)]

use cg_core::ecs::prelude::*;
use cg_core::{App, Plugin};
use core::fmt;
use std::collections::HashMap;
use std::sync::{Arc, Mutex};

/// An error from reading or writing storage.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StorageError {
    /// The key or namespace has characters other than ASCII letters, digits, `.`, `_` and
    /// `-`, is empty or longer than 64 characters, or starts with a `.`.
    InvalidKey(String),
    /// This target has no storage yet.
    Unsupported(String),
    /// Any other failure: a full disk, a browser with storage turned off, a quota.
    Other {
        /// The key that was read or written.
        key: String,
        /// What went wrong.
        message: String,
    },
}

impl fmt::Display for StorageError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidKey(key) => write!(
                f,
                "invalid storage key `{key}`: use 1 to 64 ASCII letters, digits, `.`, `_` or `-`, not starting with `.`"
            ),
            Self::Unsupported(message) => f.write_str(message),
            Self::Other { key, message } => write!(f, "storage `{key}`: {message}"),
        }
    }
}

impl std::error::Error for StorageError {}

/// Where [`Storage`] keeps its values. The engine provides one per target and
/// [`MemoryStorage`]; implement it to store values elsewhere (a cloud save, say).
///
/// Keys reaching a backend are already checked by [`Storage`].
pub trait StorageBackend: Send + Sync + 'static {
    /// The value stored under `key`, or `None` if there is none.
    fn read(&self, key: &str) -> Result<Option<String>, StorageError>;
    /// Stores `value` under `key`, replacing what was there.
    fn write(&self, key: &str, value: &str) -> Result<(), StorageError>;
    /// Removes `key`. Removing a missing key is not an error.
    fn remove(&self, key: &str) -> Result<(), StorageError>;
    /// Where the values live, for log messages: a folder, `localStorage`, `memory`.
    fn location(&self) -> String;
}

/// The game's save storage: text values by key, kept between runs.
///
/// [`StoragePlugin`] (part of `DefaultPlugins`) inserts it as a resource. It is cheap to
/// clone; clones share the same backend.
#[derive(Resource, Clone)]
pub struct Storage(Arc<dyn StorageBackend>);

impl fmt::Debug for Storage {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_tuple("Storage").field(&self.0.location()).finish()
    }
}

impl Storage {
    /// Storage with the given backend.
    pub fn new(backend: impl StorageBackend) -> Self {
        Self(Arc::new(backend))
    }

    /// This target's storage for the game named `namespace` (see the module docs).
    pub fn platform(namespace: &str) -> Result<Self, StorageError> {
        check_key(namespace)?;
        Ok(Self(Arc::new(PlatformStorage::new(namespace))))
    }

    /// Storage that lives only as long as the app: for tests and tools.
    pub fn memory() -> Self {
        Self::new(MemoryStorage::default())
    }

    /// The value stored under `key`, or `None` if there is none.
    pub fn read(&self, key: &str) -> Result<Option<String>, StorageError> {
        check_key(key)?;
        self.0.read(key)
    }

    /// Stores `value` under `key`, replacing what was there. On desktop the file is written
    /// next to the old one and then moved over it, so a crash never leaves half a file.
    pub fn write(&self, key: &str, value: &str) -> Result<(), StorageError> {
        check_key(key)?;
        self.0.write(key, value)
    }

    /// Removes `key`. Removing a missing key is not an error.
    pub fn remove(&self, key: &str) -> Result<(), StorageError> {
        check_key(key)?;
        self.0.remove(key)
    }

    /// Where the values live: a folder, `localStorage`, `memory`.
    pub fn location(&self) -> String {
        self.0.location()
    }
}

/// Storage in memory, lost when the app exits.
#[derive(Debug, Default)]
pub struct MemoryStorage(Mutex<HashMap<String, String>>);

impl StorageBackend for MemoryStorage {
    fn read(&self, key: &str) -> Result<Option<String>, StorageError> {
        Ok(lock(&self.0).get(key).cloned())
    }

    fn write(&self, key: &str, value: &str) -> Result<(), StorageError> {
        lock(&self.0).insert(key.to_owned(), value.to_owned());
        Ok(())
    }

    fn remove(&self, key: &str) -> Result<(), StorageError> {
        lock(&self.0).remove(key);
        Ok(())
    }

    fn location(&self) -> String {
        "memory".to_owned()
    }
}

fn lock<T>(mutex: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    mutex
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

/// Checks that `key` can be a file name and a `localStorage` key on every target.
fn check_key(key: &str) -> Result<(), StorageError> {
    let valid = !key.is_empty()
        && key.len() <= 64
        && !key.starts_with('.')
        && key
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'.' | b'_' | b'-'));
    if valid {
        Ok(())
    } else {
        Err(StorageError::InvalidKey(key.to_owned()))
    }
}

/// Inserts the [`Storage`] resource for this target.
///
/// `DefaultPlugins` includes it. Set [`namespace`](Self::namespace) to your game's id so
/// its saves never mix with another game's.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct StoragePlugin {
    /// The game's folder name on desktop and key prefix on the web: ASCII letters, digits,
    /// `.`, `_` and `-`. `None` uses the executable's name on desktop and `game` elsewhere.
    pub namespace: Option<String>,
    /// Keep values in memory only, as in tests.
    pub memory: bool,
}

impl Plugin for StoragePlugin {
    fn build(&self, app: &mut App) {
        if app.world().contains_resource::<Storage>() {
            return;
        }
        let storage = if self.memory {
            Storage::memory()
        } else {
            let namespace = self.namespace.clone().unwrap_or_else(default_namespace);
            Storage::platform(&namespace).unwrap_or_else(|err| {
                log::warn!("{err}; keeping saves in memory");
                Storage::memory()
            })
        };
        log::debug!("save storage: {}", storage.location());
        app.insert_resource(storage);
    }
}

/// The executable's name, made into a valid key; `game` if that fails.
#[cfg(not(any(target_arch = "wasm32", target_os = "android")))]
fn default_namespace() -> String {
    let name: String = std::env::current_exe()
        .ok()
        .and_then(|exe| Some(exe.file_stem()?.to_string_lossy().into_owned()))
        .unwrap_or_default()
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '_' || c == '-' {
                c
            } else {
                '_'
            }
        })
        .take(64)
        .collect();
    if name.is_empty() {
        "game".to_owned()
    } else {
        name
    }
}

#[cfg(any(target_arch = "wasm32", target_os = "android"))]
fn default_namespace() -> String {
    "game".to_owned()
}

/// One file per key in the game's save folder.
#[cfg(not(any(target_arch = "wasm32", target_os = "android")))]
struct PlatformStorage {
    dir: Option<std::path::PathBuf>,
}

#[cfg(not(any(target_arch = "wasm32", target_os = "android")))]
impl PlatformStorage {
    fn new(namespace: &str) -> Self {
        Self {
            dir: save_root().map(|root| root.join(namespace)),
        }
    }

    fn path(&self, key: &str) -> Result<std::path::PathBuf, StorageError> {
        self.dir.as_ref().map(|dir| dir.join(key)).ok_or_else(|| {
            StorageError::Unsupported("no folder for saves: set CG_SAVE_DIR or HOME".to_owned())
        })
    }
}

#[cfg(not(any(target_arch = "wasm32", target_os = "android")))]
fn other(key: &str, err: std::io::Error) -> StorageError {
    StorageError::Other {
        key: key.to_owned(),
        message: err.to_string(),
    }
}

#[cfg(not(any(target_arch = "wasm32", target_os = "android")))]
impl StorageBackend for PlatformStorage {
    fn read(&self, key: &str) -> Result<Option<String>, StorageError> {
        match std::fs::read_to_string(self.path(key)?) {
            Ok(value) => Ok(Some(value)),
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => Ok(None),
            Err(err) => Err(other(key, err)),
        }
    }

    fn write(&self, key: &str, value: &str) -> Result<(), StorageError> {
        let path = self.path(key)?;
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir).map_err(|err| other(key, err))?;
        }
        // Write a temporary file and move it over the old one, which replaces it in one
        // step, so a crash mid-write keeps the previous value.
        let temporary = path.with_file_name(format!("{key}.tmp"));
        std::fs::write(&temporary, value).map_err(|err| other(key, err))?;
        std::fs::rename(&temporary, &path).map_err(|err| other(key, err))
    }

    fn remove(&self, key: &str) -> Result<(), StorageError> {
        match std::fs::remove_file(self.path(key)?) {
            Ok(()) => Ok(()),
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(err) => Err(other(key, err)),
        }
    }

    fn location(&self) -> String {
        self.dir.as_ref().map_or_else(
            || "nowhere (no save folder)".to_owned(),
            |dir| dir.display().to_string(),
        )
    }
}

/// The per-user folder that holds every game's save folder.
#[cfg(not(any(target_arch = "wasm32", target_os = "android")))]
fn save_root() -> Option<std::path::PathBuf> {
    use std::path::PathBuf;

    let env = |name: &str| {
        std::env::var_os(name)
            .filter(|value| !value.is_empty())
            .map(PathBuf::from)
    };
    if let Some(dir) = env("CG_SAVE_DIR") {
        return Some(dir);
    }
    if cfg!(target_os = "windows") {
        env("APPDATA").map(|dir| dir.join("CashGear"))
    } else if cfg!(target_os = "macos") {
        env("HOME").map(|home| home.join("Library/Application Support/CashGear"))
    } else {
        env("XDG_DATA_HOME")
            .or_else(|| env("HOME").map(|home| home.join(".local/share")))
            .map(|dir| dir.join("cashgear"))
    }
}

/// The page's `localStorage`, with keys under `cashgear/<namespace>/`.
#[cfg(target_arch = "wasm32")]
struct PlatformStorage {
    prefix: String,
}

#[cfg(target_arch = "wasm32")]
impl PlatformStorage {
    fn new(namespace: &str) -> Self {
        Self {
            prefix: format!("cashgear/{namespace}/"),
        }
    }

    fn local_storage(key: &str) -> Result<web_sys::Storage, StorageError> {
        web_sys::window()
            .and_then(|window| window.local_storage().ok().flatten())
            .ok_or_else(|| StorageError::Other {
                key: key.to_owned(),
                message: "localStorage is not available (turned off, or a private window)"
                    .to_owned(),
            })
    }
}

#[cfg(target_arch = "wasm32")]
fn js_error(key: &str, err: wasm_bindgen::JsValue) -> StorageError {
    StorageError::Other {
        key: key.to_owned(),
        message: format!("{err:?}"),
    }
}

#[cfg(target_arch = "wasm32")]
impl StorageBackend for PlatformStorage {
    fn read(&self, key: &str) -> Result<Option<String>, StorageError> {
        Self::local_storage(key)?
            .get_item(&format!("{}{key}", self.prefix))
            .map_err(|err| js_error(key, err))
    }

    fn write(&self, key: &str, value: &str) -> Result<(), StorageError> {
        Self::local_storage(key)?
            .set_item(&format!("{}{key}", self.prefix), value)
            .map_err(|err| js_error(key, err))
    }

    fn remove(&self, key: &str) -> Result<(), StorageError> {
        Self::local_storage(key)?
            .remove_item(&format!("{}{key}", self.prefix))
            .map_err(|err| js_error(key, err))
    }

    fn location(&self) -> String {
        format!("localStorage ({})", self.prefix)
    }
}

/// No storage on Android until Phase 11.
#[cfg(target_os = "android")]
struct PlatformStorage;

#[cfg(target_os = "android")]
impl PlatformStorage {
    fn new(_namespace: &str) -> Self {
        Self
    }
}

#[cfg(target_os = "android")]
impl StorageBackend for PlatformStorage {
    fn read(&self, _key: &str) -> Result<Option<String>, StorageError> {
        Ok(None)
    }

    fn write(&self, _key: &str, _value: &str) -> Result<(), StorageError> {
        Err(StorageError::Unsupported(
            "save storage on Android arrives in Phase 11".to_owned(),
        ))
    }

    fn remove(&self, _key: &str) -> Result<(), StorageError> {
        Ok(())
    }

    fn location(&self) -> String {
        "nowhere (Android storage arrives in Phase 11)".to_owned()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keys_are_checked() {
        for key in ["settings.ron", "slot-1", "a_b.C9", &"k".repeat(64)] {
            assert_eq!(check_key(key), Ok(()), "{key}");
        }
        for key in [
            "",
            ".hidden",
            "a/b",
            "a\\b",
            "..",
            "x y",
            "é",
            &"k".repeat(65),
        ] {
            assert!(
                matches!(check_key(key), Err(StorageError::InvalidKey(_))),
                "{key}"
            );
        }
        let storage = Storage::memory();
        assert!(storage.write("../escape", "x").is_err());
        assert!(Storage::platform("bad/name").is_err());
    }

    #[test]
    fn memory_storage_round_trips() {
        let storage = Storage::memory();
        assert_eq!(storage.read("a"), Ok(None));
        storage.write("a", "one").unwrap();
        storage.write("a", "two").unwrap();
        assert_eq!(storage.read("a"), Ok(Some("two".to_owned())));
        let clone = storage.clone();
        assert_eq!(clone.read("a"), Ok(Some("two".to_owned())));
        storage.remove("a").unwrap();
        storage.remove("a").unwrap();
        assert_eq!(clone.read("a"), Ok(None));
        assert_eq!(storage.location(), "memory");
    }

    #[cfg(not(any(target_arch = "wasm32", target_os = "android")))]
    #[test]
    fn files_round_trip_and_replace() {
        let root = std::env::temp_dir().join(format!(
            "cg_storage_test_{}_{:?}",
            std::process::id(),
            std::thread::current().id()
        ));
        let _ = std::fs::remove_dir_all(&root);
        let storage = Storage::new(PlatformStorage {
            dir: Some(root.join("game")),
        });
        assert_eq!(storage.read("save.ron"), Ok(None));
        storage.write("save.ron", "(level: 1)").unwrap();
        storage.write("save.ron", "(level: 2)").unwrap();
        assert_eq!(storage.read("save.ron"), Ok(Some("(level: 2)".to_owned())));
        assert!(!root.join("game/save.ron.tmp").exists());
        storage.remove("save.ron").unwrap();
        storage.remove("save.ron").unwrap();
        assert_eq!(storage.read("save.ron"), Ok(None));
        std::fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn plugin_inserts_storage_once() {
        let mut app = App::new();
        app.insert_resource(Storage::memory());
        app.world()
            .resource::<Storage>()
            .write("kept", "yes")
            .unwrap();
        app.add_plugins(StoragePlugin::default());
        assert_eq!(
            app.world().resource::<Storage>().read("kept"),
            Ok(Some("yes".to_owned()))
        );
    }
}
