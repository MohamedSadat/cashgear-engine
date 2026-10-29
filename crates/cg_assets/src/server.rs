//! The [`AssetServer`]: loads files without blocking, tracks load states and
//! dependencies, unloads what nothing uses, and hot reloads changed files.

use crate::handle::{StrongHandle, TypeShared, lock};
use crate::loader::{AnyAsset, ErasedLoader};
use crate::{Asset, AssetError, AssetLoader, Assets, Handle, LoadContext, UntypedAssetId, path};
use bevy_ecs::prelude::*;
use cg_core::Time;
use cg_platform::io::{AssetIo, FileWatcher, IoError};
use cg_platform::tasks::spawn_local;
use core::any::TypeId;
use core::fmt;
use std::collections::{HashMap, HashSet};
use std::sync::{Arc, Mutex, Weak};

/// How far an asset has got.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum LoadState {
    /// The server knows nothing about it: never loaded, unloaded, or made in code with
    /// [`Assets::add`].
    NotLoaded,
    /// Being read or decoded.
    Loading,
    /// In its [`Assets`] collection.
    Loaded,
    /// Loading failed; see [`AssetServer::load_error`].
    Failed,
}

type ApplyFn = fn(&mut World, UntypedAssetId, AnyAsset);

struct TypeRecord {
    shared: Arc<TypeShared>,
    name: &'static str,
    apply: ApplyFn,
}

struct Info {
    path: String,
    weak: Weak<StrongHandle>,
    state: LoadState,
    error: Option<AssetError>,
    dependencies: Vec<UntypedAssetId>,
    /// Other files the loader read; a change to one reloads this asset.
    read_paths: Vec<String>,
    /// A reload is queued or running, so further changes wait for it.
    reloading: bool,
}

impl Info {
    fn alive(&self) -> bool {
        self.weak.strong_count() > 0
    }
}

enum Source {
    File,
    Value(AnyAsset),
}

struct Request {
    id: UntypedAssetId,
    path: String,
    reload: bool,
    source: Source,
}

struct Loaded {
    asset: AnyAsset,
    dependencies: Vec<UntypedAssetId>,
}

struct Completed {
    id: UntypedAssetId,
    path: String,
    reload: bool,
    result: Result<Loaded, AssetError>,
    read_paths: Vec<String>,
    /// When it may be applied, with a simulated delay.
    ready_at: Option<f64>,
}

#[derive(Default)]
struct State {
    paths: HashMap<(TypeId, String), UntypedAssetId>,
    infos: HashMap<UntypedAssetId, Info>,
    queue: Vec<Request>,
}

impl State {
    fn alive(&self, type_id: TypeId, path: &str) -> Option<Arc<StrongHandle>> {
        let id = self.paths.get(&(type_id, path.to_owned()))?;
        self.infos.get(id)?.weak.upgrade()
    }

    fn track(&mut self, record: &TypeRecord, type_id: TypeId, path: String) -> Arc<StrongHandle> {
        let (index, generation) = record.shared.allocate();
        let id = UntypedAssetId::new(type_id, index, generation);
        let strong = Arc::new(StrongHandle::new(id, record.shared.clone()));
        self.paths.insert((type_id, path.clone()), id);
        self.infos.insert(
            id,
            Info {
                path,
                weak: Arc::downgrade(&strong),
                state: LoadState::Loading,
                error: None,
                dependencies: Vec::new(),
                read_paths: Vec::new(),
                reloading: false,
            },
        );
        strong
    }
}

struct Inner {
    io: AssetIo,
    loaders: Mutex<Vec<Arc<dyn ErasedLoader>>>,
    types: Mutex<HashMap<TypeId, TypeRecord>>,
    state: Mutex<State>,
    completed: Mutex<Vec<Completed>>,
    watcher: Mutex<Option<FileWatcher>>,
    delay: Mutex<f64>,
}

/// Loads assets from files in the background and hands out [`Handle`]s to them.
///
/// ```
/// use cg_assets::prelude::*;
/// use cg_core::prelude::*;
///
/// struct Dialogue(String);
/// impl Asset for Dialogue {}
///
/// #[derive(Resource)]
/// struct Intro(Handle<Dialogue>);
///
/// fn setup(mut commands: Commands, server: Res<AssetServer>) {
///     commands.insert_resource(Intro(server.load("dialogue/intro.txt")));
/// }
///
/// fn show(intro: Res<Intro>, dialogues: Res<Assets<Dialogue>>) {
///     if let Some(text) = dialogues.get(&intro.0) {
///         info!("{}", text.0);
///     }
/// }
/// ```
///
/// - [`load`](Self::load) returns at once. The file is read and decoded without
///   blocking: within the frame on desktop, over the next frames on the web. The asset
///   then appears in [`Assets<T>`] in the asset systems of [`First`](cg_core::First).
/// - Loading the same path again while a strong handle to it exists returns that handle.
/// - When the last strong handle is dropped, the asset is unloaded.
/// - Assets can depend on others (a map on its tileset images): see
///   [`is_loaded`](Self::is_loaded) and [`LoadingAssets`](crate::LoadingAssets).
/// - Failures are logged with the path and the reason, and kept for
///   [`load_error`](Self::load_error).
/// - With hot reloading on (desktop debug builds by default), a changed file reloads the
///   assets made from it, and a reload that fails keeps the previous version.
///
/// The server is a cheap, shareable reference: clone it to keep it in a loader or task.
#[derive(Resource, Clone)]
pub struct AssetServer {
    inner: Arc<Inner>,
}

impl fmt::Debug for AssetServer {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("AssetServer")
            .field("root", &self.inner.io.root())
            .field("hot_reload", &self.hot_reload())
            .finish_non_exhaustive()
    }
}

impl AssetServer {
    /// A server reading through `io`, without hot reloading.
    pub fn new(io: AssetIo) -> Self {
        Self {
            inner: Arc::new(Inner {
                io,
                loaders: Mutex::default(),
                types: Mutex::default(),
                state: Mutex::default(),
                completed: Mutex::default(),
                watcher: Mutex::default(),
                delay: Mutex::new(0.0),
            }),
        }
    }

    /// The IO the server reads through.
    pub fn io(&self) -> &AssetIo {
        &self.inner.io
    }

    /// Starts watching the asset folder, so changed files reload. Fails where files cannot
    /// be watched (the web, Android).
    pub fn watch_for_changes(&self) -> Result<(), IoError> {
        let watcher = FileWatcher::new(&self.inner.io)?;
        *lock(&self.inner.watcher) = Some(watcher);
        Ok(())
    }

    /// Whether changed files are reloaded.
    pub fn hot_reload(&self) -> bool {
        lock(&self.inner.watcher).is_some()
    }

    /// Holds every finished load back by `seconds` before it is applied, to try loading
    /// screens on a fast disk. `--asset-delay 0.5` sets it.
    pub fn set_simulated_delay(&self, seconds: f64) {
        *lock(&self.inner.delay) = seconds.max(0.0);
    }

    /// Adds a loader. A loader registered later wins over an earlier one for the same
    /// asset type and extension.
    pub fn register_loader(&self, loader: impl AssetLoader) {
        lock(&self.inner.loaders).push(Arc::new(loader));
    }

    /// Makes `T` loadable, sharing ids with `existing` when its [`Assets`] already exists.
    /// [`AppAssetExt::init_asset`](crate::AppAssetExt::init_asset) calls it.
    pub(crate) fn register_type<T: Asset>(
        &self,
        existing: Option<&Arc<TypeShared>>,
    ) -> Arc<TypeShared> {
        let mut types = lock(&self.inner.types);
        let record = types
            .entry(TypeId::of::<T>())
            .or_insert_with(|| TypeRecord {
                shared: existing.cloned().unwrap_or_default(),
                name: core::any::type_name::<T>(),
                apply: apply::<T>,
            });
        record.shared.clone()
    }

    pub(crate) fn is_registered<T: Asset>(&self) -> bool {
        lock(&self.inner.types).contains_key(&TypeId::of::<T>())
    }

    fn type_name(&self, type_id: TypeId) -> &'static str {
        lock(&self.inner.types)
            .get(&type_id)
            .map_or("an unregistered type", |record| record.name)
    }

    /// Loads the asset at `path` (relative to the asset root, `/`-separated) as a `T`.
    /// See the type's docs.
    ///
    /// # Panics
    ///
    /// Panics if `T` was never registered with
    /// [`init_asset`](crate::AppAssetExt::init_asset) (engine plugins register theirs).
    pub fn load<T: Asset>(&self, path: impl Into<String>) -> Handle<T> {
        let path = path::normalize(&path.into());
        let type_id = TypeId::of::<T>();
        let types = lock(&self.inner.types);
        let record = types.get(&type_id).unwrap_or_else(|| unregistered::<T>());
        let mut state = lock(&self.inner.state);
        if let Some(strong) = state.alive(type_id, &path) {
            return Handle::from_strong(strong);
        }
        let strong = state.track(record, type_id, path.clone());
        state.queue.push(Request {
            id: strong.id,
            path,
            reload: false,
            source: Source::File,
        });
        Handle::from_strong(strong)
    }

    /// Stores an asset made in code under a path, as if it had been loaded from there: a
    /// later [`load`](Self::load) of that path (by a prefab, say) gets the same asset while
    /// the returned handle lives. Replaces the asset if the path is already loaded. It
    /// arrives in [`Assets<T>`] in the next asset update, like a load.
    pub fn add<T: Asset>(&self, path: impl Into<String>, value: T) -> Handle<T> {
        let path = path::normalize(&path.into());
        let type_id = TypeId::of::<T>();
        let types = lock(&self.inner.types);
        let record = types.get(&type_id).unwrap_or_else(|| unregistered::<T>());
        let mut state = lock(&self.inner.state);
        let (strong, reload) = match state.alive(type_id, &path) {
            Some(strong) => (strong, true),
            None => (state.track(record, type_id, path.clone()), false),
        };
        state.queue.push(Request {
            id: strong.id,
            path,
            reload,
            source: Source::Value(Box::new(value)),
        });
        Handle::from_strong(strong)
    }

    /// Reloads every live asset made from `path`, or that read it while loading. Returns
    /// how many reloads were started. Hot reloading calls it for each changed file.
    pub fn reload(&self, path: &str) -> usize {
        let path = path::normalize(path);
        let mut state = lock(&self.inner.state);
        let State { infos, queue, .. } = &mut *state;
        let mut started = 0;
        for (id, info) in infos.iter_mut() {
            let affected = info.path == path || info.read_paths.contains(&path);
            if !affected || !info.alive() || info.reloading || info.state == LoadState::Loading {
                continue;
            }
            info.reloading = true;
            queue.push(Request {
                id: *id,
                path: info.path.clone(),
                reload: true,
                source: Source::File,
            });
            started += 1;
        }
        started
    }

    /// How far the asset itself has got, not counting its dependencies.
    pub fn load_state(&self, id: impl Into<UntypedAssetId>) -> LoadState {
        lock(&self.inner.state)
            .infos
            .get(&id.into())
            .map_or(LoadState::NotLoaded, |info| info.state)
    }

    /// How far the asset and everything it depends on have got: `Failed` if any failed,
    /// `Loaded` once all are loaded, else `Loading` (or `NotLoaded` for unknown ids).
    pub fn recursive_load_state(&self, id: impl Into<UntypedAssetId>) -> LoadState {
        let state = lock(&self.inner.state);
        let id = id.into();
        if !state.infos.contains_key(&id) {
            return LoadState::NotLoaded;
        }
        let mut seen = HashSet::new();
        recursive_state(&state.infos, id, &mut seen)
    }

    /// Whether the asset and everything it depends on are loaded.
    pub fn is_loaded(&self, id: impl Into<UntypedAssetId>) -> bool {
        self.recursive_load_state(id) == LoadState::Loaded
    }

    /// Why the asset failed to load, if it did.
    pub fn load_error(&self, id: impl Into<UntypedAssetId>) -> Option<AssetError> {
        lock(&self.inner.state)
            .infos
            .get(&id.into())
            .and_then(|info| info.error.clone())
    }

    /// The path the asset was loaded from.
    pub fn path(&self, id: impl Into<UntypedAssetId>) -> Option<String> {
        lock(&self.inner.state)
            .infos
            .get(&id.into())
            .map(|info| info.path.clone())
    }

    /// The live handle for `path` loaded as a `T`, if there is one.
    pub fn get_handle<T: Asset>(&self, path: &str) -> Option<Handle<T>> {
        let state = lock(&self.inner.state);
        state
            .alive(TypeId::of::<T>(), &path::normalize(path))
            .map(Handle::from_strong)
    }

    /// The number of assets still loading.
    pub fn loading(&self) -> usize {
        lock(&self.inner.state)
            .infos
            .values()
            .filter(|info| info.alive() && info.state == LoadState::Loading)
            .count()
    }

    /// Runs one asset update: picks up changed files, starts queued loads, and stores
    /// finished ones in their [`Assets`]. The asset plugin runs it in
    /// [`First`](cg_core::First).
    pub fn update(&self, world: &mut World) {
        self.poll_watcher();
        // Loaders queue their dependencies while they run; on desktop those run at once,
        // so a whole tree of assets usually loads in one update.
        for _ in 0..64 {
            let requests = core::mem::take(&mut lock(&self.inner.state).queue);
            if requests.is_empty() {
                break;
            }
            for request in requests {
                self.start(request);
            }
        }
        let now = world
            .get_resource::<Time>()
            .map_or(0.0, |time| time.raw_elapsed().as_secs_f64());
        for completed in self.take_ready(now) {
            self.finish(world, completed);
        }
    }

    fn poll_watcher(&self) {
        let changed = match &*lock(&self.inner.watcher) {
            Some(watcher) => watcher.changed(),
            None => return,
        };
        for path in changed {
            let reloads = self.reload(&path);
            if reloads > 0 {
                log::info!("`{path}` changed; reloading {reloads} asset(s)");
            }
        }
    }

    fn find_loader(&self, type_id: TypeId, path: &str) -> Option<Arc<dyn ErasedLoader>> {
        let loaders = lock(&self.inner.loaders);
        path::extensions(path).iter().find_map(|ext| {
            loaders
                .iter()
                .rev()
                .find(|loader| loader.asset_type() == type_id && loader.extensions().contains(ext))
                .cloned()
        })
    }

    fn start(&self, request: Request) {
        let Request {
            id,
            path,
            reload,
            source,
        } = request;
        let done = |result| {
            self.complete(Completed {
                id,
                path: path.clone(),
                reload,
                result,
                read_paths: Vec::new(),
                ready_at: None,
            });
        };
        match source {
            Source::Value(asset) => done(Ok(Loaded {
                asset,
                dependencies: Vec::new(),
            })),
            Source::File => {
                let Some(loader) = self.find_loader(id.type_id(), &path) else {
                    let type_name = self.type_name(id.type_id());
                    done(Err(AssetError::NoLoader {
                        path: path.clone(),
                        type_name,
                    }));
                    return;
                };
                let server = self.clone();
                spawn_local(async move {
                    let (result, read_paths) = server.run(loader.as_ref(), &path).await;
                    server.complete(Completed {
                        id,
                        path,
                        reload,
                        result,
                        read_paths,
                        ready_at: None,
                    });
                });
            }
        }
    }

    async fn run(
        &self,
        loader: &dyn ErasedLoader,
        path: &str,
    ) -> (Result<Loaded, AssetError>, Vec<String>) {
        let bytes = match self.inner.io.read(path).await {
            Ok(bytes) => bytes,
            Err(err) => return (Err(AssetError::from_io(path, err)), Vec::new()),
        };
        let mut ctx = LoadContext::new(self.clone(), path.to_owned(), bytes);
        let result = loader.load(&mut ctx).await;
        let read_paths = core::mem::take(&mut ctx.read_paths);
        let result = match result {
            Ok(asset) => Ok(Loaded {
                asset,
                dependencies: core::mem::take(&mut ctx.dependencies),
            }),
            Err(err) => Err(AssetError::Load {
                path: path.to_owned(),
                message: err.to_string(),
            }),
        };
        (result, read_paths)
    }

    fn complete(&self, completed: Completed) {
        lock(&self.inner.completed).push(completed);
    }

    fn take_ready(&self, now: f64) -> Vec<Completed> {
        let delay = *lock(&self.inner.delay);
        let mut completed = lock(&self.inner.completed);
        if delay <= 0.0 {
            return core::mem::take(&mut *completed);
        }
        let mut ready = Vec::new();
        let mut waiting = Vec::new();
        for mut item in completed.drain(..) {
            let ready_at = *item.ready_at.get_or_insert(now + delay);
            if ready_at <= now {
                ready.push(item);
            } else {
                waiting.push(item);
            }
        }
        *completed = waiting;
        ready
    }

    fn finish(&self, world: &mut World, completed: Completed) {
        let Completed {
            id,
            path,
            reload,
            result,
            read_paths,
            ..
        } = completed;
        let alive = lock(&self.inner.state)
            .infos
            .get(&id)
            .is_some_and(Info::alive);
        if !alive {
            log::debug!("dropped `{path}` before it finished loading");
            return;
        }
        let result = result.map(|loaded| {
            let apply = lock(&self.inner.types)[&id.type_id()].apply;
            apply(world, id, loaded.asset);
            loaded.dependencies
        });
        let mut state = lock(&self.inner.state);
        let Some(info) = state.infos.get_mut(&id) else {
            return;
        };
        info.reloading = false;
        info.read_paths = read_paths;
        match result {
            Ok(dependencies) => {
                info.state = LoadState::Loaded;
                info.error = None;
                info.dependencies = dependencies;
                if reload {
                    log::info!("reloaded `{path}`");
                } else {
                    log::debug!("loaded `{path}`");
                }
            }
            Err(err) if reload && info.state == LoadState::Loaded => {
                log::error!("{err}; keeping the version loaded before");
            }
            Err(err) => {
                log::error!("{err}");
                info.state = LoadState::Failed;
                info.error = Some(err);
            }
        }
    }

    /// Forgets unloaded assets.
    pub(crate) fn forget(&self, ids: impl IntoIterator<Item = UntypedAssetId>) {
        let mut state = lock(&self.inner.state);
        for id in ids {
            let Some(info) = state.infos.remove(&id) else {
                continue;
            };
            let key = (id.type_id(), info.path);
            if state.paths.get(&key) == Some(&id) {
                state.paths.remove(&key);
            }
        }
    }
}

fn unregistered<T>() -> ! {
    panic!(
        "asset type `{}` is not registered; add its plugin or call `app.init_asset::<T>()`",
        core::any::type_name::<T>()
    )
}

fn recursive_state(
    infos: &HashMap<UntypedAssetId, Info>,
    id: UntypedAssetId,
    seen: &mut HashSet<UntypedAssetId>,
) -> LoadState {
    if !seen.insert(id) {
        return LoadState::Loaded;
    }
    // Dependencies the server no longer knows were dropped by their asset: nothing to wait for.
    let Some(info) = infos.get(&id) else {
        return LoadState::Loaded;
    };
    if info.state != LoadState::Loaded {
        return info.state;
    }
    let mut result = LoadState::Loaded;
    for &dependency in &info.dependencies {
        match recursive_state(infos, dependency, seen) {
            LoadState::Failed => return LoadState::Failed,
            LoadState::Loaded => {}
            _ => result = LoadState::Loading,
        }
    }
    result
}

fn apply<T: Asset>(world: &mut World, id: UntypedAssetId, asset: AnyAsset) {
    let (Some(id), Ok(asset)) = (id.typed::<T>(), asset.downcast::<T>()) else {
        log::error!("an asset arrived with the wrong type; this is an engine bug");
        return;
    };
    match world.get_resource_mut::<Assets<T>>() {
        Some(mut assets) => {
            if !assets.insert(id, *asset) {
                log::error!(
                    "`Assets<{}>` does not share ids with the asset server; register it with `init_asset`",
                    core::any::type_name::<T>()
                );
            }
        }
        None => log::error!(
            "no `Assets<{}>` resource; register the type with `init_asset`",
            core::any::type_name::<T>()
        ),
    }
}
