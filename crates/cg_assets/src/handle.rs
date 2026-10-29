//! [`AssetId`]s, which name an asset, and [`Handle`]s, which also keep it loaded.

use crate::Asset;
use core::any::TypeId;
use core::cmp::Ordering;
use core::fmt;
use core::hash::{Hash, Hasher};
use core::marker::PhantomData;
use std::sync::{Arc, Mutex, MutexGuard};

/// Ids with an index below this are reserved for assets the engine keeps at fixed ids,
/// such as the white texture at [`AssetId::DEFAULT`]. They are never allocated.
pub const RESERVED_IDS: u32 = 16;

/// Names an asset of type `T` in [`Assets<T>`](crate::Assets) without keeping it loaded.
///
/// Ids are small and `Copy`. An index can be reused once its asset is gone, but with a new
/// generation, so an old id finds nothing rather than another asset.
pub struct AssetId<T> {
    index: u32,
    generation: u32,
    marker: PhantomData<fn() -> T>,
}

impl<T> AssetId<T> {
    /// The reserved id that [`Handle::default`] points at. For images it is the 1×1 white
    /// texture; other asset types leave it empty unless they put something there.
    pub const DEFAULT: Self = Self::reserved(0);

    /// A reserved id (see [`RESERVED_IDS`]), for assets an engine crate puts at a fixed id.
    ///
    /// # Panics
    ///
    /// Panics if `index` is not below [`RESERVED_IDS`].
    pub const fn reserved(index: u32) -> Self {
        assert!(
            index < RESERVED_IDS,
            "reserved asset ids are below RESERVED_IDS"
        );
        Self::from_parts(index, 0)
    }

    pub(crate) const fn from_parts(index: u32, generation: u32) -> Self {
        Self {
            index,
            generation,
            marker: PhantomData,
        }
    }

    /// The slot index, for debugging and for tables indexed by asset.
    pub fn index(self) -> u32 {
        self.index
    }

    /// How many times the index has been reused.
    pub fn generation(self) -> u32 {
        self.generation
    }

    /// Whether this is one of the reserved ids.
    pub fn is_reserved(self) -> bool {
        self.index < RESERVED_IDS
    }
}

impl<T: Asset> AssetId<T> {
    /// The id without its type, for code that handles assets of any type.
    pub fn untyped(self) -> UntypedAssetId {
        UntypedAssetId {
            type_id: TypeId::of::<T>(),
            index: self.index,
            generation: self.generation,
        }
    }
}

impl<T> Clone for AssetId<T> {
    fn clone(&self) -> Self {
        *self
    }
}

impl<T> Copy for AssetId<T> {}

impl<T> PartialEq for AssetId<T> {
    fn eq(&self, other: &Self) -> bool {
        (self.index, self.generation) == (other.index, other.generation)
    }
}

impl<T> Eq for AssetId<T> {}

impl<T> Hash for AssetId<T> {
    fn hash<H: Hasher>(&self, state: &mut H) {
        (self.index, self.generation).hash(state);
    }
}

impl<T> PartialOrd for AssetId<T> {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl<T> Ord for AssetId<T> {
    fn cmp(&self, other: &Self) -> Ordering {
        (self.index, self.generation).cmp(&(other.index, other.generation))
    }
}

impl<T> fmt::Debug for AssetId<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let name = core::any::type_name::<T>();
        let short = name.rsplit("::").next().unwrap_or(name);
        write!(f, "AssetId<{short}>({}v{})", self.index, self.generation)
    }
}

impl<T> Default for AssetId<T> {
    fn default() -> Self {
        Self::DEFAULT
    }
}

/// An [`AssetId`] of any asset type.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct UntypedAssetId {
    type_id: TypeId,
    index: u32,
    generation: u32,
}

impl UntypedAssetId {
    /// The typed id, if the asset is a `T`.
    pub fn typed<T: Asset>(self) -> Option<AssetId<T>> {
        (self.type_id == TypeId::of::<T>())
            .then(|| AssetId::from_parts(self.index, self.generation))
    }

    /// The asset type's id.
    pub fn type_id(self) -> TypeId {
        self.type_id
    }

    pub(crate) fn new(type_id: TypeId, index: u32, generation: u32) -> Self {
        Self {
            type_id,
            index,
            generation,
        }
    }

    pub(crate) fn parts(self) -> (u32, u32) {
        (self.index, self.generation)
    }
}

impl<T: Asset> From<AssetId<T>> for UntypedAssetId {
    fn from(id: AssetId<T>) -> Self {
        id.untyped()
    }
}

impl<T: Asset> From<&Handle<T>> for UntypedAssetId {
    fn from(handle: &Handle<T>) -> Self {
        handle.id.untyped()
    }
}

impl From<&UntypedHandle> for UntypedAssetId {
    fn from(handle: &UntypedHandle) -> Self {
        handle.id
    }
}

/// Allocates ids for one asset type and collects the ids whose last strong handle was
/// dropped. Shared by that type's [`Assets`](crate::Assets) and the asset server.
#[derive(Default)]
pub(crate) struct TypeShared {
    allocator: Mutex<Allocator>,
    dropped: Mutex<Vec<(u32, u32)>>,
}

#[derive(Default)]
struct Allocator {
    /// The live generation of each allocated index, minus [`RESERVED_IDS`].
    generations: Vec<u32>,
    live: Vec<bool>,
    free: Vec<u32>,
}

pub(crate) fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    // A panic while holding one of these locks leaves plain data behind; carry on.
    mutex
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

impl TypeShared {
    /// A fresh `(index, generation)`.
    pub(crate) fn allocate(&self) -> (u32, u32) {
        let mut alloc = lock(&self.allocator);
        if let Some(slot) = alloc.free.pop() {
            alloc.live[slot as usize] = true;
            return (slot + RESERVED_IDS, alloc.generations[slot as usize]);
        }
        let slot = alloc.generations.len() as u32;
        alloc.generations.push(0);
        alloc.live.push(true);
        (slot + RESERVED_IDS, 0)
    }

    /// Whether `(index, generation)` is allocated and current. Reserved ids always are.
    pub(crate) fn is_live(&self, index: u32, generation: u32) -> bool {
        if index < RESERVED_IDS {
            return generation == 0;
        }
        let alloc = lock(&self.allocator);
        let slot = (index - RESERVED_IDS) as usize;
        alloc.live.get(slot) == Some(&true) && alloc.generations[slot] == generation
    }

    /// Frees an index for reuse with the next generation. Returns `false`, doing nothing,
    /// if it was not live with this generation.
    pub(crate) fn release(&self, index: u32, generation: u32) -> bool {
        if index < RESERVED_IDS {
            return false;
        }
        let mut alloc = lock(&self.allocator);
        let slot = (index - RESERVED_IDS) as usize;
        if alloc.live.get(slot) != Some(&true) || alloc.generations[slot] != generation {
            return false;
        }
        alloc.live[slot] = false;
        alloc.generations[slot] = generation.wrapping_add(1);
        alloc.free.push(slot as u32);
        true
    }

    pub(crate) fn take_dropped(&self) -> Vec<(u32, u32)> {
        core::mem::take(&mut *lock(&self.dropped))
    }
}

/// The shared part of a strong handle. Dropping the last one reports its id for unloading.
pub(crate) struct StrongHandle {
    pub(crate) id: UntypedAssetId,
    shared: Arc<TypeShared>,
}

impl StrongHandle {
    pub(crate) fn new(id: UntypedAssetId, shared: Arc<TypeShared>) -> Self {
        Self { id, shared }
    }
}

impl Drop for StrongHandle {
    fn drop(&mut self) {
        lock(&self.shared.dropped).push(self.id.parts());
    }
}

/// A reference to an asset of type `T`. A strong handle keeps the asset loaded: once the
/// last strong handle to it is dropped, it is unloaded. Weak handles (from
/// [`Handle::weak`], [`clone_weak`](Self::clone_weak) or [`Handle::default`]) do not.
///
/// Get strong handles from [`AssetServer::load`](crate::AssetServer::load) or
/// [`Assets::add`](crate::Assets::add), and keep them in components or resources for as long
/// as the asset is needed. Cloning a handle is cheap: it counts a reference.
///
/// Handles compare, hash and sort by their [`AssetId`].
pub struct Handle<T> {
    id: AssetId<T>,
    strong: Option<Arc<StrongHandle>>,
}

impl<T> Handle<T> {
    /// A weak handle to `id`: it finds the asset while something else keeps it loaded.
    pub const fn weak(id: AssetId<T>) -> Self {
        Self { id, strong: None }
    }

    pub(crate) fn from_strong(strong: Arc<StrongHandle>) -> Self {
        let (index, generation) = strong.id.parts();
        Self {
            id: AssetId::from_parts(index, generation),
            strong: Some(strong),
        }
    }

    /// The asset's id.
    pub fn id(&self) -> AssetId<T> {
        self.id
    }

    /// Whether this handle keeps the asset loaded.
    pub fn is_strong(&self) -> bool {
        self.strong.is_some()
    }

    /// A weak copy of this handle.
    pub fn clone_weak(&self) -> Self {
        Self::weak(self.id)
    }
}

impl<T: Asset> Handle<T> {
    /// The handle without its type. It still keeps the asset loaded if it is strong.
    pub fn untyped(&self) -> UntypedHandle {
        UntypedHandle {
            id: self.id.untyped(),
            strong: self.strong.clone(),
        }
    }
}

impl<T> Clone for Handle<T> {
    fn clone(&self) -> Self {
        Self {
            id: self.id,
            strong: self.strong.clone(),
        }
    }
}

impl<T> Default for Handle<T> {
    /// A weak handle to [`AssetId::DEFAULT`].
    fn default() -> Self {
        Self::weak(AssetId::DEFAULT)
    }
}

impl<T> PartialEq for Handle<T> {
    fn eq(&self, other: &Self) -> bool {
        self.id == other.id
    }
}

impl<T> Eq for Handle<T> {}

impl<T> Hash for Handle<T> {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.id.hash(state);
    }
}

impl<T> PartialOrd for Handle<T> {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl<T> Ord for Handle<T> {
    fn cmp(&self, other: &Self) -> Ordering {
        self.id.cmp(&other.id)
    }
}

impl<T> fmt::Debug for Handle<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let kind = if self.is_strong() { "Strong" } else { "Weak" };
        write!(f, "{kind}({:?})", self.id)
    }
}

impl<T> From<&Handle<T>> for AssetId<T> {
    fn from(handle: &Handle<T>) -> Self {
        handle.id
    }
}

impl<T> From<Handle<T>> for AssetId<T> {
    fn from(handle: Handle<T>) -> Self {
        handle.id
    }
}

/// A [`Handle`] of any asset type, for lists of assets to wait for.
#[derive(Clone)]
pub struct UntypedHandle {
    id: UntypedAssetId,
    strong: Option<Arc<StrongHandle>>,
}

impl UntypedHandle {
    /// The asset's id.
    pub fn id(&self) -> UntypedAssetId {
        self.id
    }

    /// The typed handle, if the asset is a `T`.
    pub fn typed<T: Asset>(&self) -> Option<Handle<T>> {
        Some(Handle {
            id: self.id.typed()?,
            strong: self.strong.clone(),
        })
    }
}

impl PartialEq for UntypedHandle {
    fn eq(&self, other: &Self) -> bool {
        self.id == other.id
    }
}

impl Eq for UntypedHandle {}

impl fmt::Debug for UntypedHandle {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let kind = if self.strong.is_some() {
            "Strong"
        } else {
            "Weak"
        };
        write!(f, "{kind}({:?})", self.id)
    }
}

impl<T: Asset> From<Handle<T>> for UntypedHandle {
    fn from(handle: Handle<T>) -> Self {
        Self {
            id: handle.id.untyped(),
            strong: handle.strong,
        }
    }
}

impl<T: Asset> From<&Handle<T>> for UntypedHandle {
    fn from(handle: &Handle<T>) -> Self {
        handle.untyped()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct Text;
    impl Asset for Text {}

    #[test]
    fn indices_are_reused_with_a_new_generation() {
        let shared = TypeShared::default();
        let a = shared.allocate();
        assert_eq!(a, (RESERVED_IDS, 0));
        assert!(shared.is_live(a.0, a.1));
        assert!(shared.release(a.0, a.1));
        assert!(!shared.release(a.0, a.1), "a second release does nothing");
        assert!(!shared.is_live(a.0, a.1));
        let b = shared.allocate();
        assert_eq!(b, (RESERVED_IDS, 1));
        assert!(!shared.release(0, 0), "reserved ids are never released");
    }

    #[test]
    fn dropping_the_last_strong_handle_reports_the_id() {
        let shared = Arc::new(TypeShared::default());
        let (index, generation) = shared.allocate();
        let id = UntypedAssetId::new(TypeId::of::<Text>(), index, generation);
        let handle: Handle<Text> =
            Handle::from_strong(Arc::new(StrongHandle::new(id, shared.clone())));
        let copy = handle.clone();
        let weak = handle.clone_weak();
        assert!(copy.is_strong() && !weak.is_strong());
        assert_eq!(copy, weak);
        drop(handle);
        assert!(shared.take_dropped().is_empty());
        drop(copy);
        assert_eq!(shared.take_dropped(), [(index, generation)]);
        drop(weak);
        assert!(shared.take_dropped().is_empty());
    }

    #[test]
    fn untyped_ids_remember_their_type() {
        let id: AssetId<Text> = AssetId::reserved(3);
        let untyped = id.untyped();
        assert_eq!(untyped.typed::<Text>(), Some(id));
        struct Other;
        impl Asset for Other {}
        assert_eq!(untyped.typed::<Other>(), None);
        assert_eq!(format!("{id:?}"), "AssetId<Text>(3v0)");
    }
}
