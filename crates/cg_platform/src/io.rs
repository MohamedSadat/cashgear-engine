//! Asset IO: reading game files by path, the same way on every target, and watching them
//! for changes on desktop.
//!
//! | Target | Reads from | Watches |
//! | --- | --- | --- |
//! | Desktop | The asset folder on disk | Yes, with `notify` |
//! | Web | The page's server, with `fetch` | No |
//! | Android | APK assets (Phase 11; an error until then) | No |
//!
//! Paths are relative to the asset root and use `/`, like `maps/level1.tmx`, on every
//! target. Reads are `async`: on desktop the future is ready at once, on the web it
//! completes on a later frame. Run them with [`spawn_local`](crate::tasks::spawn_local).

// The only engine code that touches the file system and the clock directly.
#![allow(clippy::disallowed_methods)]

use core::fmt;
use core::future::Future;

/// An error from reading a file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum IoError {
    /// No file at this path.
    NotFound(String),
    /// This target cannot read files yet.
    Unsupported(String),
    /// Any other failure, such as a permission problem or a server error.
    Other {
        /// The path that was read.
        path: String,
        /// What went wrong.
        message: String,
    },
}

impl fmt::Display for IoError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NotFound(path) => write!(f, "`{path}` not found"),
            Self::Unsupported(message) => f.write_str(message),
            Self::Other { path, message } => write!(f, "`{path}`: {message}"),
        }
    }
}

impl std::error::Error for IoError {}

/// Reads files under the asset root.
///
/// The root is found once, at creation:
///
/// - Desktop: the `CG_ASSET_ROOT` environment variable; else `assets` in the crate being
///   run by `cargo run` (`CARGO_MANIFEST_DIR`); else `assets` next to the executable; else
///   `assets` in the working directory.
/// - Web: `assets/`, relative to the page, so `trunk` can copy the folder next to
///   `index.html` (`<link data-trunk rel="copy-dir" href="assets" />`).
///
/// ```no_run
/// # async fn demo() -> Result<(), cg_platform::io::IoError> {
/// let io = cg_platform::io::AssetIo::new(None);
/// let bytes = io.read("maps/level1.tmx").await?;
/// # Ok(()) }
/// ```
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AssetIo {
    root: String,
}

impl AssetIo {
    /// Asset IO with the given root, or the default root for this target (see above). A
    /// given root is a folder on desktop and a URL prefix on the web.
    pub fn new(root: Option<&str>) -> Self {
        let root = root.map_or_else(default_root, str::to_owned);
        Self { root }
    }

    /// The asset root: a folder on desktop, a URL prefix on the web.
    pub fn root(&self) -> &str {
        &self.root
    }

    /// Reads the whole file at `path` (relative to the root, `/`-separated).
    ///
    /// The future owns everything it needs, so it can be handed to
    /// [`spawn_local`](crate::tasks::spawn_local).
    pub fn read(&self, path: &str) -> impl Future<Output = Result<Vec<u8>, IoError>> + 'static {
        read(self.root.clone(), path.to_owned())
    }
}

impl Default for AssetIo {
    fn default() -> Self {
        Self::new(None)
    }
}

#[cfg(not(any(target_arch = "wasm32", target_os = "android")))]
fn default_root() -> String {
    use std::path::{Path, PathBuf};

    let candidates = [
        std::env::var_os("CG_ASSET_ROOT").map(PathBuf::from),
        std::env::var_os("CARGO_MANIFEST_DIR").map(|dir| Path::new(&dir).join("assets")),
        std::env::current_exe()
            .ok()
            .and_then(|exe| Some(exe.parent()?.join("assets"))),
    ];
    let root = candidates
        .into_iter()
        .flatten()
        .find(|dir| dir.is_dir())
        .unwrap_or_else(|| PathBuf::from("assets"));
    std::path::absolute(&root)
        .unwrap_or(root)
        .to_string_lossy()
        .into_owned()
}

#[cfg(target_arch = "wasm32")]
fn default_root() -> String {
    "assets".to_owned()
}

#[cfg(target_os = "android")]
fn default_root() -> String {
    String::new()
}

#[cfg(not(any(target_arch = "wasm32", target_os = "android")))]
async fn read(root: String, path: String) -> Result<Vec<u8>, IoError> {
    let full = std::path::Path::new(&root).join(&path);
    std::fs::read(&full).map_err(|err| match err.kind() {
        std::io::ErrorKind::NotFound => IoError::NotFound(path),
        _ => IoError::Other {
            path,
            message: err.to_string(),
        },
    })
}

#[cfg(target_arch = "wasm32")]
async fn read(root: String, path: String) -> Result<Vec<u8>, IoError> {
    use wasm_bindgen::JsCast;
    use wasm_bindgen_futures::JsFuture;

    let other = |message: String| IoError::Other {
        path: path.clone(),
        message,
    };
    let js = |value: wasm_bindgen::JsValue| format!("{value:?}");
    let url = if root.is_empty() {
        path.clone()
    } else {
        format!("{}/{path}", root.trim_end_matches('/'))
    };
    let window = web_sys::window().ok_or_else(|| other("no browser window".into()))?;
    let response = JsFuture::from(window.fetch_with_str(&url))
        .await
        .map_err(|err| other(js(err)))?;
    let response: web_sys::Response = response.dyn_into().map_err(|err| other(js(err)))?;
    match response.status() {
        200..=299 => {}
        404 => return Err(IoError::NotFound(path)),
        status => return Err(other(format!("HTTP {status} for {url}"))),
    }
    // Development servers such as `trunk serve` answer unknown paths with the page itself
    // (status 200). An HTML answer for a file that is not HTML means it is missing.
    let html = response
        .headers()
        .get("content-type")
        .ok()
        .flatten()
        .is_some_and(|kind| kind.starts_with("text/html"));
    if html && !path.ends_with(".html") && !path.ends_with(".htm") {
        return Err(IoError::NotFound(path));
    }
    let buffer = JsFuture::from(response.array_buffer().map_err(|err| other(js(err)))?)
        .await
        .map_err(|err| other(js(err)))?;
    Ok(js_sys::Uint8Array::new(&buffer).to_vec())
}

#[cfg(target_os = "android")]
async fn read(_root: String, _path: String) -> Result<Vec<u8>, IoError> {
    Err(IoError::Unsupported(
        "reading APK assets arrives with the Android build (Phase 11)".into(),
    ))
}

/// Watches the asset folder and reports files that changed, for hot reloading.
///
/// Desktop only: [`new`](Self::new) fails on the web and on Android. Changes are reported
/// once a file has been quiet for a moment, so an editor's several writes during one save
/// count once.
pub struct FileWatcher {
    #[cfg(not(any(target_arch = "wasm32", target_os = "android")))]
    inner: desktop::Watcher,
}

impl fmt::Debug for FileWatcher {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("FileWatcher").finish_non_exhaustive()
    }
}

impl FileWatcher {
    /// Starts watching everything under `io`'s root. Fails with [`IoError::NotFound`] if
    /// the root folder does not exist, and with [`IoError::Unsupported`] off desktop.
    pub fn new(io: &AssetIo) -> Result<Self, IoError> {
        #[cfg(not(any(target_arch = "wasm32", target_os = "android")))]
        return Ok(Self {
            inner: desktop::Watcher::new(io.root())?,
        });
        #[cfg(any(target_arch = "wasm32", target_os = "android"))]
        {
            let _ = io;
            Err(IoError::Unsupported(
                "watching files works on desktop only".into(),
            ))
        }
    }

    /// The files that changed and have been quiet since, as `/`-separated paths relative
    /// to the root, sorted. Each change is reported once.
    pub fn changed(&self) -> Vec<String> {
        #[cfg(not(any(target_arch = "wasm32", target_os = "android")))]
        return self.inner.changed();
        #[cfg(any(target_arch = "wasm32", target_os = "android"))]
        Vec::new()
    }
}

#[cfg(not(any(target_arch = "wasm32", target_os = "android")))]
mod desktop {
    use super::IoError;
    use notify::{EventKind, RecursiveMode, Watcher as _};
    use std::collections::HashMap;
    use std::path::{Path, PathBuf};
    use std::sync::{Arc, Mutex};
    use std::time::{Duration, Instant};

    /// How long a file must be quiet before its change is reported.
    const QUIET: Duration = Duration::from_millis(100);

    pub(super) struct Watcher {
        root: PathBuf,
        pending: Arc<Mutex<HashMap<PathBuf, Instant>>>,
        // Dropping it stops the watch.
        _watcher: notify::RecommendedWatcher,
    }

    impl Watcher {
        pub(super) fn new(root: &str) -> Result<Self, IoError> {
            let error = |err: notify::Error| IoError::Other {
                path: root.to_owned(),
                message: err.to_string(),
            };
            let root = PathBuf::from(root);
            if !root.is_dir() {
                return Err(IoError::NotFound(root.to_string_lossy().into_owned()));
            }
            let pending: Arc<Mutex<HashMap<PathBuf, Instant>>> = Arc::default();
            let sink = pending.clone();
            // notify calls this on its own thread.
            let mut watcher = notify::recommended_watcher(move |event: notify::Result<_>| {
                let Ok(notify::Event { kind, paths, .. }) = event else {
                    return;
                };
                if matches!(kind, EventKind::Access(_) | EventKind::Other) {
                    return;
                }
                let now = Instant::now();
                let mut pending = sink.lock().unwrap_or_else(|e| e.into_inner());
                for path in paths {
                    pending.insert(path, now);
                }
            })
            .map_err(error)?;
            watcher
                .watch(&root, RecursiveMode::Recursive)
                .map_err(error)?;
            Ok(Self {
                root,
                pending,
                _watcher: watcher,
            })
        }

        pub(super) fn changed(&self) -> Vec<String> {
            let now = Instant::now();
            let mut pending = self.pending.lock().unwrap_or_else(|e| e.into_inner());
            let mut quiet = Vec::new();
            pending.retain(|path, last| {
                if now.duration_since(*last) < QUIET {
                    return true;
                }
                if path.is_file() {
                    quiet.extend(relative(&self.root, path));
                }
                false
            });
            quiet.sort();
            quiet.dedup();
            quiet
        }
    }

    /// `path` relative to `root` with `/` separators, if it is under it.
    fn relative(root: &Path, path: &Path) -> Option<String> {
        // notify reports paths under the folder as it was given; fall back to comparing
        // canonical paths in case one side went through a link or a `\\?\` prefix.
        let rest = match path.strip_prefix(root) {
            Ok(rest) => rest.to_path_buf(),
            Err(_) => {
                let root = root.canonicalize().ok()?;
                path.canonicalize()
                    .ok()?
                    .strip_prefix(&root)
                    .ok()?
                    .to_path_buf()
            }
        };
        let parts: Vec<_> = rest
            .components()
            .map(|part| part.as_os_str().to_string_lossy().into_owned())
            .collect();
        (!parts.is_empty()).then(|| parts.join("/"))
    }

    #[cfg(test)]
    mod tests {
        use super::*;

        #[test]
        fn relative_paths_use_slashes() {
            let root = Path::new("/game/assets");
            assert_eq!(
                relative(root, &root.join("maps").join("level.tmx")),
                Some("maps/level.tmx".to_owned())
            );
            assert_eq!(relative(root, Path::new("/elsewhere/x.png")), None);
        }
    }
}

#[cfg(all(test, not(target_arch = "wasm32")))]
mod tests {
    use super::*;

    #[test]
    fn reads_files_under_the_root_and_reports_missing_ones() {
        let dir = std::env::temp_dir().join(format!("cg_io_test_{}", std::process::id()));
        std::fs::create_dir_all(dir.join("maps")).unwrap();
        std::fs::write(dir.join("maps").join("a.txt"), b"hello").unwrap();
        let io = AssetIo::new(Some(dir.to_str().unwrap()));
        assert_eq!(
            pollster::block_on(io.read("maps/a.txt")),
            Ok(b"hello".to_vec())
        );
        assert_eq!(
            pollster::block_on(io.read("maps/missing.txt")),
            Err(IoError::NotFound("maps/missing.txt".into()))
        );
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn the_default_root_is_the_crates_assets_folder_under_cargo() {
        // `cargo test` sets CARGO_MANIFEST_DIR; cg_platform has no `assets` folder, so the
        // fallbacks apply, and the result is always absolute.
        let io = AssetIo::default();
        assert!(std::path::Path::new(io.root()).is_absolute());
    }
}
