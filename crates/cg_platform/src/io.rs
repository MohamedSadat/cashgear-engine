//! Asset IO: reading game files by path, the same way on every target, and watching them
//! for changes on desktop.
//!
//! | Target | Reads from | Watches |
//! | --- | --- | --- |
//! | Desktop | The asset folder on disk | Yes, with `notify` |
//! | Web | The page's server, with `fetch` | No |
//! | Android | The APK's `assets/` folder | No |
//!
//! Paths are relative to the asset root and use `/`, like `maps/level1.tmx`, on every
//! target. Reads are `async`: on desktop the future is ready at once, on the web it
//! completes on a later frame. Run them with [`spawn_local`](crate::tasks::spawn_local).
//!
//! A [`Bundle`] (the whole asset folder in one file, made by `cg_pack bundle`) can stand in
//! for the folder: [`AssetIo`] reads files from it first, and from the folder only when the
//! bundle lacks them. On the web, the page can download the bundle while the game's code
//! loads, and hand it over as `window.cashgearBundle`; on Android, a bundle named
//! [`BUNDLE_FILE`] in the APK's assets is used without being asked for.

// The only engine code that touches the file system and the clock directly.
#![allow(clippy::disallowed_methods)]

mod bundle;

pub use bundle::{BUNDLE_MAGIC, BUNDLE_VERSION, Bundle, BundleError};

/// The usual name of a game's bundle: what `cg_pack trunk-bundle` writes next to a web page
/// and the Android build puts in the APK's assets, where [`AssetIo`] finds it by itself.
pub const BUNDLE_FILE: &str = "assets.cgpak";

use core::fmt;
use core::future::Future;
use std::sync::{Arc, Mutex, PoisonError};

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
/// - Android: the APK's `assets/` folder, read through the asset manager.
///
/// Files are read from a [`Bundle`] first when there is one: one given with
/// [`with_bundle`](Self::with_bundle), on the web the one the page downloaded (a promise
/// of an `ArrayBuffer` in `window.cashgearBundle`, looked for on the first read), on
/// Android [`BUNDLE_FILE`] in the APK's assets (read once per process), and on desktop
/// [`BUNDLE_FILE`] next to the executable when the game is not run by `cargo run` and
/// `CG_ASSET_ROOT` is not set (a shipped game: the executable and its bundle).
///
/// ```no_run
/// # async fn demo() -> Result<(), cg_platform::io::IoError> {
/// let io = cg_platform::io::AssetIo::new(None);
/// let bytes = io.read("maps/level1.tmx").await?;
/// # Ok(()) }
/// ```
#[derive(Clone)]
pub struct AssetIo {
    root: String,
    bundle: Arc<BundleSlot>,
}

impl fmt::Debug for AssetIo {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("AssetIo")
            .field("root", &self.root)
            .field("bundle_path", &self.bundle.path)
            .field("bundle", &self.bundle.known())
            .finish()
    }
}

impl AssetIo {
    /// Asset IO with the given root, or the default root for this target (see above). A
    /// given root is a folder on desktop and a URL prefix on the web.
    pub fn new(root: Option<&str>) -> Self {
        let root = root.map_or_else(default_root, str::to_owned);
        Self {
            root,
            bundle: Arc::new(BundleSlot::new(None, platform_bundle())),
        }
    }

    /// Reads files from the bundle at `path` first: a file on desktop (relative to the
    /// working directory), a URL relative to the page on the web, fetched on the first read
    /// unless the page has handed over a bundle already, a file in the APK's assets on
    /// Android. Without a bundle there, or with a
    /// broken one (logged), files are read from the root as usual.
    pub fn with_bundle(mut self, path: &str) -> Self {
        self.bundle = Arc::new(BundleSlot::new(Some(path.to_owned()), open_bundle(path)));
        self
    }

    /// Reads files from `bundle` first, for tests and tools that have one in memory.
    pub fn with_loaded_bundle(mut self, bundle: Bundle) -> Self {
        let state = BundleState::Ready(Arc::new(bundle));
        self.bundle = Arc::new(BundleSlot::new(None, state));
        self
    }

    /// The asset root: a folder on desktop, a URL prefix on the web.
    pub fn root(&self) -> &str {
        &self.root
    }

    /// The bundle files are read from, once known: on desktop and Android from the start, on
    /// the web after the first read.
    pub fn bundle(&self) -> Option<Arc<Bundle>> {
        self.bundle.known().flatten()
    }

    /// Reads the whole file at `path` (relative to the root, `/`-separated).
    ///
    /// The future owns everything it needs, so it can be handed to
    /// [`spawn_local`](crate::tasks::spawn_local).
    pub fn read(&self, path: &str) -> impl Future<Output = Result<Vec<u8>, IoError>> + 'static {
        let io = self.clone();
        let path = path.to_owned();
        async move {
            if let Some(bundle) = io.find_bundle().await
                && let Some(result) = bundle.read(&path)
            {
                return result.map_err(|message| IoError::Other { path, message });
            }
            read(io.root, path).await
        }
    }

    async fn find_bundle(&self) -> Option<Arc<Bundle>> {
        if let Some(known) = self.bundle.known() {
            return known;
        }
        #[cfg(target_arch = "wasm32")]
        web::look_for_bundle(&self.bundle).await;
        self.bundle.known().flatten()
    }
}

impl Default for AssetIo {
    fn default() -> Self {
        Self::new(None)
    }
}

/// Where an [`AssetIo`]'s bundle comes from and whether it has been found.
struct BundleSlot {
    /// The bundle the game asked for: a file on desktop, a URL on the web.
    path: Option<String>,
    state: Mutex<BundleState>,
}

enum BundleState {
    /// Not looked for yet (the web looks on the first read).
    Unchecked,
    /// There is none: files are read one by one.
    Missing,
    Ready(Arc<Bundle>),
}

impl BundleSlot {
    fn new(path: Option<String>, state: BundleState) -> Self {
        Self {
            path,
            state: Mutex::new(state),
        }
    }

    /// The bundle, `Some(None)` if there is none, `None` if not looked for yet.
    fn known(&self) -> Option<Option<Arc<Bundle>>> {
        match &*self.state.lock().unwrap_or_else(PoisonError::into_inner) {
            BundleState::Unchecked => None,
            BundleState::Missing => Some(None),
            BundleState::Ready(bundle) => Some(Some(bundle.clone())),
        }
    }

    /// Records what was found, unless another read got there first.
    #[cfg_attr(not(target_arch = "wasm32"), expect(dead_code))]
    fn settle(&self, found: impl FnOnce() -> Option<Bundle>) {
        let mut state = self.state.lock().unwrap_or_else(PoisonError::into_inner);
        if matches!(*state, BundleState::Unchecked) {
            *state = found().map_or(BundleState::Missing, |b| BundleState::Ready(Arc::new(b)));
        }
    }
}

/// The bundle a new [`AssetIo`] starts with: the web looks for the page's on the first
/// read, Android uses the APK's, and a shipped desktop game the [`BUNDLE_FILE`] next to its
/// executable.
fn platform_bundle() -> BundleState {
    if cfg!(target_arch = "wasm32") {
        return BundleState::Unchecked;
    }
    #[cfg(target_os = "android")]
    if let Some(bundle) = crate::android::apk_bundle() {
        return BundleState::Ready(bundle);
    }
    #[cfg(not(any(target_arch = "wasm32", target_os = "android")))]
    if let Some(path) = shipped_bundle() {
        return open_bundle(&path.to_string_lossy());
    }
    BundleState::Missing
}

/// [`BUNDLE_FILE`] next to the executable, for a game shipped as the executable and its
/// bundle (a release zip). Not when run by `cargo run` or with `CG_ASSET_ROOT` set: then
/// the asset folder is meant.
#[cfg(not(any(target_arch = "wasm32", target_os = "android")))]
fn shipped_bundle() -> Option<std::path::PathBuf> {
    if std::env::var_os("CG_ASSET_ROOT").is_some()
        || std::env::var_os("CARGO_MANIFEST_DIR").is_some()
    {
        return None;
    }
    let path = std::env::current_exe().ok()?.parent()?.join(BUNDLE_FILE);
    path.is_file().then_some(path)
}

/// Logs what a bundle holds, or why it is not used, and keeps it if it is good.
pub(crate) fn accept_bundle(path: &str, parsed: Result<Bundle, BundleError>) -> Option<Bundle> {
    match parsed {
        Ok(bundle) => {
            log::info!(
                "reading assets from the bundle `{path}` ({} files, {:.1} MB)",
                bundle.len(),
                bundle.size() as f64 / 1e6
            );
            Some(bundle)
        }
        Err(err) => {
            log::warn!("not using the asset bundle `{path}`: {err}");
            None
        }
    }
}

#[cfg(not(any(target_arch = "wasm32", target_os = "android")))]
fn open_bundle(path: &str) -> BundleState {
    match std::fs::read(path) {
        Ok(bytes) => accept_bundle(path, Bundle::parse(bytes))
            .map_or(BundleState::Missing, |b| BundleState::Ready(Arc::new(b))),
        Err(err) => {
            log::warn!("no asset bundle at `{path}` ({err}); reading files one by one");
            BundleState::Missing
        }
    }
}

/// On the web the bundle is fetched on the first read.
#[cfg(target_arch = "wasm32")]
fn open_bundle(_path: &str) -> BundleState {
    BundleState::Unchecked
}

#[cfg(target_os = "android")]
fn open_bundle(path: &str) -> BundleState {
    match crate::android::read_asset(path) {
        Ok(bytes) => accept_bundle(path, Bundle::parse(bytes))
            .map_or(BundleState::Missing, |b| BundleState::Ready(Arc::new(b))),
        Err(err) => {
            log::warn!("no asset bundle at `{path}` in the APK ({err}); reading files one by one");
            BundleState::Missing
        }
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
    let url = if root.is_empty() {
        path.clone()
    } else {
        format!("{}/{path}", root.trim_end_matches('/'))
    };
    let buffer = web::fetch(&url, &path).await?;
    Ok(js_sys::Uint8Array::new(&buffer).to_vec())
}

#[cfg(target_arch = "wasm32")]
mod web {
    use super::{Bundle, BundleSlot, IoError, accept_bundle};
    use core::cell::RefCell;
    use js_sys::{ArrayBuffer, Promise, Reflect, Uint8Array};
    use wasm_bindgen::{JsCast, JsValue};
    use wasm_bindgen_futures::{JsFuture, future_to_promise};

    /// The name of the page's bundle promise on `window`.
    const PAGE_BUNDLE: &str = "cashgearBundle";

    thread_local! {
        /// The bundle's bytes as one promise that every waiting read shares, once asked for:
        /// `None` inside when there is no bundle to wait for.
        static BUNDLE: RefCell<Option<Option<(String, Promise)>>> = const { RefCell::new(None) };
    }

    /// Fetches `url` (asset `path`) as an `ArrayBuffer`.
    pub(super) async fn fetch(url: &str, path: &str) -> Result<ArrayBuffer, IoError> {
        let other = |message: String| IoError::Other {
            path: path.to_owned(),
            message,
        };
        let js = |value: JsValue| format!("{value:?}");
        let window = web_sys::window().ok_or_else(|| other("no browser window".into()))?;
        let response = JsFuture::from(window.fetch_with_str(url))
            .await
            .map_err(|err| other(js(err)))?;
        let response: web_sys::Response = response.dyn_into().map_err(|err| other(js(err)))?;
        match response.status() {
            200..=299 => {}
            404 => return Err(IoError::NotFound(path.to_owned())),
            status => return Err(other(format!("HTTP {status} for {url}"))),
        }
        // Development servers such as `trunk serve` answer unknown paths with the page
        // itself (status 200). An HTML answer for a file that is not HTML means it is
        // missing.
        let html = response
            .headers()
            .get("content-type")
            .ok()
            .flatten()
            .is_some_and(|kind| kind.starts_with("text/html"));
        if html && !path.ends_with(".html") && !path.ends_with(".htm") {
            return Err(IoError::NotFound(path.to_owned()));
        }
        let buffer = JsFuture::from(response.array_buffer().map_err(|err| other(js(err)))?)
            .await
            .map_err(|err| other(js(err)))?;
        buffer.dyn_into().map_err(|err| other(js(err)))
    }

    /// Waits for the page's bundle, or fetches the slot's, and records what was found.
    pub(super) async fn look_for_bundle(slot: &BundleSlot) {
        let Some((name, promise)) = bundle_promise(slot.path.as_deref()) else {
            slot.settle(|| None);
            return;
        };
        let value = JsFuture::from(promise).await.unwrap_or(JsValue::NULL);
        // Every read waiting here wakes up; the first one unpacks the index.
        slot.settle(|| {
            let buffer = value.dyn_into::<ArrayBuffer>().ok()?;
            let parsed = Bundle::parse(Uint8Array::new(&buffer).to_vec());
            accept_bundle(&name, parsed)
        });
    }

    /// The page's bundle promise if it has one, else a fetch of `path`, else `None`; the
    /// same promise on every call.
    fn bundle_promise(path: Option<&str>) -> Option<(String, Promise)> {
        BUNDLE.with(|cell| {
            cell.borrow_mut()
                .get_or_insert_with(|| page_bundle().or_else(|| fetched_bundle(path?)))
                .clone()
        })
    }

    fn page_bundle() -> Option<(String, Promise)> {
        let window = web_sys::window()?;
        let value = Reflect::get(&window, &JsValue::from_str(PAGE_BUNDLE)).ok()?;
        if value.is_null() || value.is_undefined() {
            return None;
        }
        Some((format!("window.{PAGE_BUNDLE}"), Promise::resolve(&value)))
    }

    fn fetched_bundle(path: &str) -> Option<(String, Promise)> {
        let url = path.to_owned();
        let promise = future_to_promise(async move {
            match fetch(&url, &url).await {
                Ok(buffer) => Ok(buffer.into()),
                Err(IoError::NotFound(_)) => {
                    log::warn!("no asset bundle at `{url}`; reading files one by one");
                    Ok(JsValue::NULL)
                }
                Err(err) => {
                    log::warn!("cannot fetch the asset bundle: {err}; reading files one by one");
                    Ok(JsValue::NULL)
                }
            }
        });
        Some((path.to_owned(), promise))
    }
}

/// The root is a folder inside the APK's `assets/` (empty for `assets/` itself).
#[cfg(target_os = "android")]
async fn read(root: String, path: String) -> Result<Vec<u8>, IoError> {
    let full = if root.is_empty() {
        path.clone()
    } else {
        format!("{}/{path}", root.trim_end_matches('/'))
    };
    crate::android::read_asset(&full).map_err(|err| match err {
        IoError::NotFound(_) => IoError::NotFound(path),
        other => other,
    })
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
    fn bundled_files_come_first_and_the_folder_fills_in() {
        let dir = std::env::temp_dir().join(format!("cg_io_bundle_{}", std::process::id()));
        std::fs::create_dir_all(dir.join("maps")).unwrap();
        std::fs::write(dir.join("maps").join("a.txt"), b"loose").unwrap();
        std::fs::write(dir.join("b.txt"), b"only loose").unwrap();
        let bytes = Bundle::write(vec![("maps/a.txt".into(), b"bundled".to_vec())]);
        let root = dir.to_str().unwrap();
        let read = |io: &AssetIo, path: &str| pollster::block_on(io.read(path));

        let io = AssetIo::new(Some(root)).with_loaded_bundle(Bundle::parse(bytes.clone()).unwrap());
        assert_eq!(read(&io, "maps/a.txt"), Ok(b"bundled".to_vec()));
        assert_eq!(read(&io, "b.txt"), Ok(b"only loose".to_vec()));
        assert_eq!(read(&io, "c.txt"), Err(IoError::NotFound("c.txt".into())));

        // From a file, as `--bundle` gives it.
        let file = dir.join("game.cgpak");
        std::fs::write(&file, &bytes).unwrap();
        let io = AssetIo::new(Some(root)).with_bundle(file.to_str().unwrap());
        assert_eq!(io.bundle().map(|b| b.len()), Some(1));
        assert_eq!(read(&io, "maps/a.txt"), Ok(b"bundled".to_vec()));

        // A missing or broken bundle is skipped, and the folder serves everything.
        for path in [dir.join("none.cgpak"), dir.join("b.txt")] {
            let io = AssetIo::new(Some(root)).with_bundle(path.to_str().unwrap());
            assert!(io.bundle().is_none());
            assert_eq!(read(&io, "maps/a.txt"), Ok(b"loose".to_vec()));
        }
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
