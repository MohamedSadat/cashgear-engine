//! [`Assets<T>`]: the loaded assets of one type, and [`AssetEvent`]s about them.

use crate::handle::{StrongHandle, TypeShared};
use crate::{Asset, AssetId, Handle};
use bevy_ecs::message::Message;
use bevy_ecs::resource::Resource;
use core::fmt;
use std::sync::Arc;

/// Something that happened to an asset, sent as a message at the end of the asset
/// systems in [`First`](cg_core::First).
pub enum AssetEvent<T: Asset> {
    /// The asset was stored for the first time.
    Added {
        /// Which asset.
        id: AssetId<T>,
    },
    /// The asset was replaced (a hot reload, say) or borrowed mutably.
    Modified {
        /// Which asset.
        id: AssetId<T>,
    },
    /// The asset was removed, or unloaded because its last strong handle was dropped.
    Removed {
        /// Which asset.
        id: AssetId<T>,
    },
}

impl<T: Asset> AssetEvent<T> {
    /// The asset the event is about.
    pub fn id(&self) -> AssetId<T> {
        match *self {
            Self::Added { id } | Self::Modified { id } | Self::Removed { id } => id,
        }
    }
}

impl<T: Asset> Message for AssetEvent<T> {}

impl<T: Asset> Clone for AssetEvent<T> {
    fn clone(&self) -> Self {
        *self
    }
}

impl<T: Asset> Copy for AssetEvent<T> {}

impl<T: Asset> PartialEq for AssetEvent<T> {
    fn eq(&self, other: &Self) -> bool {
        core::mem::discriminant(self) == core::mem::discriminant(other) && self.id() == other.id()
    }
}

impl<T: Asset> Eq for AssetEvent<T> {}

impl<T: Asset> fmt::Debug for AssetEvent<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let kind = match self {
            Self::Added { .. } => "Added",
            Self::Modified { .. } => "Modified",
            Self::Removed { .. } => "Removed",
        };
        write!(f, "{kind}({:?})", self.id())
    }
}

struct Slot<T> {
    generation: u32,
    value: Option<T>,
    version: u64,
}

/// The loaded assets of type `T`, by [`AssetId`].
///
/// Assets loaded by the [`AssetServer`](crate::AssetServer) land here once they are ready;
/// assets made in code join with [`add`](Self::add). Look them up with a handle or an id:
///
/// ```
/// use cg_assets::{Asset, Assets};
///
/// struct Dialogue(String);
/// impl Asset for Dialogue {}
///
/// let mut dialogues = Assets::<Dialogue>::default();
/// let hello = dialogues.add(Dialogue("Hello!".into()));
/// assert_eq!(dialogues.get(&hello).unwrap().0, "Hello!");
/// ```
///
/// Each asset has a [`version`](Self::version) that changes whenever it is replaced or
/// borrowed mutably, so systems can notice changes by comparing versions instead of
/// reading [`AssetEvent`]s.
#[derive(Resource)]
pub struct Assets<T: Asset> {
    slots: Vec<Slot<T>>,
    shared: Arc<TypeShared>,
    revision: u64,
    events: Vec<AssetEvent<T>>,
    len: usize,
}

impl<T: Asset> Default for Assets<T> {
    fn default() -> Self {
        Self::with_shared(Arc::default())
    }
}

impl<T: Asset> fmt::Debug for Assets<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Assets")
            .field("type", &core::any::type_name::<T>())
            .field("len", &self.len)
            .finish()
    }
}

impl<T: Asset> Assets<T> {
    pub(crate) fn with_shared(shared: Arc<TypeShared>) -> Self {
        Self {
            slots: Vec::new(),
            shared,
            revision: 0,
            events: Vec::new(),
            len: 0,
        }
    }

    pub(crate) fn shared(&self) -> &Arc<TypeShared> {
        &self.shared
    }

    /// Stores an asset made in code and returns a strong handle to it. The asset is
    /// unloaded once the last strong handle is dropped.
    pub fn add(&mut self, value: T) -> Handle<T> {
        let (index, generation) = self.shared.allocate();
        let id = AssetId::from_parts(index, generation);
        self.store(id, value);
        Handle::from_strong(Arc::new(StrongHandle::new(
            id.untyped(),
            self.shared.clone(),
        )))
    }

    /// Stores `value` under `id`, replacing any asset there. `id` must be a reserved id
    /// or one whose handles are still around; returns `false`, storing nothing, otherwise.
    pub fn insert(&mut self, id: impl Into<AssetId<T>>, value: T) -> bool {
        let id = id.into();
        if !self.shared.is_live(id.index(), id.generation()) {
            return false;
        }
        self.store(id, value);
        true
    }

    fn store(&mut self, id: AssetId<T>, value: T) {
        let index = id.index() as usize;
        if self.slots.len() <= index {
            self.slots.resize_with(index + 1, || Slot {
                generation: 0,
                value: None,
                version: 0,
            });
        }
        self.revision += 1;
        let revision = self.revision;
        let slot = &mut self.slots[index];
        let replaced = slot.generation == id.generation() && slot.value.is_some();
        if slot.value.is_some() && !replaced {
            // An asset of an older generation was never cleared; it is gone now.
            self.events.push(AssetEvent::Removed {
                id: AssetId::from_parts(id.index(), slot.generation),
            });
            self.len -= 1;
        }
        slot.generation = id.generation();
        slot.value = Some(value);
        slot.version = revision;
        if replaced {
            self.events.push(AssetEvent::Modified { id });
        } else {
            self.len += 1;
            self.events.push(AssetEvent::Added { id });
        }
    }

    fn slot(&self, id: AssetId<T>) -> Option<&Slot<T>> {
        self.slots
            .get(id.index() as usize)
            .filter(|slot| slot.generation == id.generation() && slot.value.is_some())
    }

    /// The asset, if it is loaded.
    pub fn get(&self, id: impl Into<AssetId<T>>) -> Option<&T> {
        self.slot(id.into())?.value.as_ref()
    }

    /// The asset, mutably. Counts as a change: its version moves on and a
    /// [`AssetEvent::Modified`] is sent.
    pub fn get_mut(&mut self, id: impl Into<AssetId<T>>) -> Option<&mut T> {
        let id = id.into();
        self.slot(id)?;
        self.revision += 1;
        self.events.push(AssetEvent::Modified { id });
        let slot = &mut self.slots[id.index() as usize];
        slot.version = self.revision;
        slot.value.as_mut()
    }

    /// Whether the asset is loaded.
    pub fn contains(&self, id: impl Into<AssetId<T>>) -> bool {
        self.slot(id.into()).is_some()
    }

    /// Removes the asset and returns it. Handles to it stay valid but find nothing.
    pub fn remove(&mut self, id: impl Into<AssetId<T>>) -> Option<T> {
        let id = id.into();
        self.slot(id)?;
        self.revision += 1;
        self.len -= 1;
        self.events.push(AssetEvent::Removed { id });
        self.slots[id.index() as usize].value.take()
    }

    /// A number that changes whenever this asset changes, or `None` if it is not loaded.
    pub fn version(&self, id: impl Into<AssetId<T>>) -> Option<u64> {
        self.slot(id.into()).map(|slot| slot.version)
    }

    /// A number that changes whenever any asset of this type is added, changed or removed.
    pub fn revision(&self) -> u64 {
        self.revision
    }

    /// The number of loaded assets.
    pub fn len(&self) -> usize {
        self.len
    }

    /// Whether no asset is loaded.
    pub fn is_empty(&self) -> bool {
        self.len == 0
    }

    /// Every loaded asset with its id.
    pub fn iter(&self) -> impl Iterator<Item = (AssetId<T>, &T)> {
        self.slots.iter().enumerate().filter_map(|(index, slot)| {
            let value = slot.value.as_ref()?;
            Some((AssetId::from_parts(index as u32, slot.generation), value))
        })
    }

    /// Every loaded asset with its id and [`version`](Self::version).
    pub fn iter_versions(&self) -> impl Iterator<Item = (AssetId<T>, &T, u64)> {
        self.slots.iter().enumerate().filter_map(|(index, slot)| {
            let value = slot.value.as_ref()?;
            Some((
                AssetId::from_parts(index as u32, slot.generation),
                value,
                slot.version,
            ))
        })
    }

    /// Unloads the assets whose last strong handle was dropped and frees their ids.
    /// Returns the ids that were freed.
    pub(crate) fn unload_dropped(&mut self) -> Vec<AssetId<T>> {
        let mut freed = Vec::new();
        for (index, generation) in self.shared.take_dropped() {
            let id = AssetId::from_parts(index, generation);
            if !self.shared.release(index, generation) {
                continue;
            }
            if self.slot(id).is_some() {
                self.revision += 1;
                self.len -= 1;
                self.events.push(AssetEvent::Removed { id });
                self.slots[index as usize].value = None;
            }
            freed.push(id);
        }
        freed
    }

    pub(crate) fn take_events(&mut self) -> Vec<AssetEvent<T>> {
        core::mem::take(&mut self.events)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Debug, PartialEq)]
    struct Text(&'static str);
    impl Asset for Text {}

    #[test]
    fn added_assets_live_until_their_last_strong_handle_drops() {
        let mut texts = Assets::<Text>::default();
        let hello = texts.add(Text("hello"));
        let id = hello.id();
        assert_eq!(texts.get(&hello), Some(&Text("hello")));
        assert_eq!(texts.len(), 1);
        let copy = hello.clone();
        drop(hello);
        assert!(texts.unload_dropped().is_empty());
        assert!(texts.contains(id));
        drop(copy);
        assert_eq!(texts.unload_dropped(), [id]);
        assert!(!texts.contains(id));
        assert!(texts.is_empty());
        assert_eq!(
            texts.take_events(),
            [AssetEvent::Added { id }, AssetEvent::Removed { id }]
        );
    }

    #[test]
    fn reused_indices_do_not_find_old_assets() {
        let mut texts = Assets::<Text>::default();
        let first = texts.add(Text("first"));
        let old = first.id();
        drop(first);
        texts.unload_dropped();
        let second = texts.add(Text("second"));
        assert_eq!(second.id().index(), old.index());
        assert_ne!(second.id(), old);
        assert_eq!(texts.get(old), None);
        assert!(
            !texts.insert(old, Text("stale")),
            "dead ids cannot be filled"
        );
        assert_eq!(texts.get(&second), Some(&Text("second")));
    }

    #[test]
    fn changes_move_versions_and_send_events() {
        let mut texts = Assets::<Text>::default();
        let handle = texts.add(Text("a"));
        let before = texts.version(&handle).unwrap();
        texts.get_mut(&handle).unwrap().0 = "b";
        let after = texts.version(&handle).unwrap();
        assert!(after > before);
        assert!(texts.insert(&handle, Text("c")));
        assert!(texts.version(&handle).unwrap() > after);
        let id = handle.id();
        assert_eq!(
            texts.take_events(),
            [
                AssetEvent::Added { id },
                AssetEvent::Modified { id },
                AssetEvent::Modified { id }
            ]
        );
        assert_eq!(texts.remove(&handle), Some(Text("c")));
        assert_eq!(texts.version(&handle), None);
    }

    #[test]
    fn reserved_ids_can_always_be_filled() {
        let mut texts = Assets::<Text>::default();
        assert!(texts.insert(AssetId::DEFAULT, Text("default")));
        assert_eq!(texts.get(Handle::default().id()), Some(&Text("default")));
        assert_eq!(texts.iter().count(), 1);
    }
}
