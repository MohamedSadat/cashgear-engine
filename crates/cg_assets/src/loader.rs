//! [`AssetLoader`]s turn file bytes into assets; [`LoadContext`] lets them read more files
//! and load other assets they depend on.

use crate::{Asset, AssetServer, Handle, UntypedAssetId, path};
use cg_platform::io::IoError;
use core::any::{Any, TypeId};
use core::fmt;
use core::future::Future;
use core::pin::Pin;

/// A boxed future that need not be `Send`: loaders run on the main thread (and on the
/// browser's event loop on the web).
pub type BoxedFuture<'a, T> = Pin<Box<dyn Future<Output = T> + 'a>>;

/// Any error a loader returns. Strings and every `std::error::Error` convert into it with
/// `?` or `.into()`.
pub type BoxError = Box<dyn std::error::Error + Send + Sync>;

/// An error from loading an asset.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AssetError {
    /// No file at this path.
    NotFound {
        /// The asset path.
        path: String,
    },
    /// The file could not be read.
    Io {
        /// The asset path.
        path: String,
        /// What went wrong.
        message: String,
    },
    /// No loader for this asset type handles the file's extension.
    NoLoader {
        /// The asset path.
        path: String,
        /// The asset type that was asked for.
        type_name: &'static str,
    },
    /// The loader failed: the file is malformed, or something it needs failed to load.
    Load {
        /// The asset path.
        path: String,
        /// What went wrong.
        message: String,
    },
}

impl AssetError {
    /// The asset path the error is about.
    pub fn path(&self) -> &str {
        match self {
            Self::NotFound { path }
            | Self::Io { path, .. }
            | Self::NoLoader { path, .. }
            | Self::Load { path, .. } => path,
        }
    }

    pub(crate) fn from_io(path: &str, err: IoError) -> Self {
        match err {
            IoError::NotFound(_) => Self::NotFound {
                path: path.to_owned(),
            },
            IoError::Unsupported(message) | IoError::Other { message, .. } => Self::Io {
                path: path.to_owned(),
                message,
            },
        }
    }
}

impl fmt::Display for AssetError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NotFound { path } => write!(f, "`{path}` not found"),
            Self::Io { path, message } => write!(f, "could not read `{path}`: {message}"),
            Self::NoLoader { path, type_name } => {
                write!(f, "no loader for `{path}` as a `{type_name}`")
            }
            Self::Load { path, message } => write!(f, "could not load `{path}`: {message}"),
        }
    }
}

impl std::error::Error for AssetError {}

/// Turns a file into an asset. Register loaders with
/// [`AppAssetExt::register_loader`](crate::AppAssetExt::register_loader); the server picks
/// one by the asset type asked for and the file's extension.
///
/// Loading is `async` so a loader can read further files (a map's tilesets, say) through
/// the [`LoadContext`], which on the web means waiting for the network. A loader that only
/// needs its own bytes returns a future that is ready at once:
///
/// ```
/// use cg_assets::{Asset, AssetLoader, BoxError, BoxedFuture, LoadContext};
///
/// struct Dialogue(String);
/// impl Asset for Dialogue {}
///
/// struct DialogueLoader;
///
/// impl AssetLoader for DialogueLoader {
///     type Asset = Dialogue;
///
///     fn extensions(&self) -> &[&str] {
///         &["txt"]
///     }
///
///     fn load<'a>(&'a self, ctx: &'a mut LoadContext) -> BoxedFuture<'a, Result<Dialogue, BoxError>> {
///         Box::pin(async move { Ok(Dialogue(String::from_utf8(ctx.bytes().to_vec())?)) })
///     }
/// }
/// ```
pub trait AssetLoader: Send + Sync + 'static {
    /// The asset the loader makes.
    type Asset: Asset;

    /// The extensions it handles, lower-case without the leading dot. Compound extensions
    /// such as `prefab.ron` win over shorter ones such as `ron`.
    fn extensions(&self) -> &[&str];

    /// Makes the asset from the file in `ctx`.
    fn load<'a>(
        &'a self,
        ctx: &'a mut LoadContext,
    ) -> BoxedFuture<'a, Result<Self::Asset, BoxError>>;
}

/// A loaded asset of some type.
pub(crate) type AnyAsset = Box<dyn Any + Send + Sync>;

/// An [`AssetLoader`] with its asset type erased.
pub(crate) trait ErasedLoader: Send + Sync {
    fn asset_type(&self) -> TypeId;
    fn extensions(&self) -> Vec<String>;
    fn load<'a>(&'a self, ctx: &'a mut LoadContext) -> BoxedFuture<'a, Result<AnyAsset, BoxError>>;
}

impl<L: AssetLoader> ErasedLoader for L {
    fn asset_type(&self) -> TypeId {
        TypeId::of::<L::Asset>()
    }

    fn extensions(&self) -> Vec<String> {
        AssetLoader::extensions(self)
            .iter()
            .map(|ext| ext.trim_start_matches('.').to_ascii_lowercase())
            .collect()
    }

    fn load<'a>(&'a self, ctx: &'a mut LoadContext) -> BoxedFuture<'a, Result<AnyAsset, BoxError>> {
        Box::pin(async move {
            let asset = AssetLoader::load(self, ctx).await?;
            Ok(Box::new(asset) as AnyAsset)
        })
    }
}

/// What a loader gets: the file's path and bytes, and ways to reach other files.
pub struct LoadContext {
    server: AssetServer,
    path: String,
    bytes: Vec<u8>,
    pub(crate) dependencies: Vec<UntypedAssetId>,
    pub(crate) read_paths: Vec<String>,
}

impl LoadContext {
    pub(crate) fn new(server: AssetServer, path: String, bytes: Vec<u8>) -> Self {
        Self {
            server,
            path,
            bytes,
            dependencies: Vec::new(),
            read_paths: Vec::new(),
        }
    }

    /// The path of the file being loaded, relative to the asset root.
    pub fn path(&self) -> &str {
        &self.path
    }

    /// The file's bytes.
    pub fn bytes(&self) -> &[u8] {
        &self.bytes
    }

    /// The file's bytes, taken out of the context (it is left empty).
    pub fn take_bytes(&mut self) -> Vec<u8> {
        core::mem::take(&mut self.bytes)
    }

    /// `relative`, resolved against the folder of the file being loaded. See
    /// [`path::resolve`].
    pub fn resolve(&self, relative: &str) -> String {
        path::resolve(&self.path, relative)
    }

    /// Starts loading another asset this one depends on, by its path from the asset root,
    /// and returns its handle; keep it in the asset. The asset counts as
    /// [loaded with its dependencies](crate::AssetServer::is_loaded) only once this one is
    /// too.
    pub fn load<A: Asset>(&mut self, path: impl Into<String>) -> Handle<A> {
        let handle = self.server.load::<A>(path);
        self.dependencies.push(handle.id().untyped());
        handle
    }

    /// Reads another file, by its path from the asset root, as part of this asset (a map's
    /// tileset, say). When hot reloading is on, a change to that file reloads this asset.
    pub async fn read(&mut self, path: &str) -> Result<Vec<u8>, AssetError> {
        let path = path::normalize(path);
        if !self.read_paths.contains(&path) {
            self.read_paths.push(path.clone());
        }
        self.server
            .io()
            .read(&path)
            .await
            .map_err(|err| AssetError::from_io(&path, err))
    }

    /// The asset server, for anything else.
    pub fn server(&self) -> &AssetServer {
        &self.server
    }
}

impl fmt::Debug for LoadContext {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("LoadContext")
            .field("path", &self.path)
            .field("bytes", &self.bytes.len())
            .finish_non_exhaustive()
    }
}
