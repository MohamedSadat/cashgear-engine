//! Prefabs as assets: `.prefab.ron` files loaded into [`Prefabs`] by name, and reloaded
//! when they change.

use crate::{Asset, AssetLoader, Assets, BoxError, BoxedFuture, Handle, LoadContext};
use bevy_ecs::prelude::*;
use cg_world::Prefabs;
use std::collections::BTreeMap;

/// The RON text of a prefab file. Loading one by itself does nothing; give it a name with
/// [`AppAssetExt::load_prefab`](crate::AppAssetExt::load_prefab) (or
/// [`PrefabFiles::add`]) so that `commands.spawn_prefab(name)` finds it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PrefabSource(pub String);

impl Asset for PrefabSource {}

/// Reads `.prefab.ron` files as [`PrefabSource`]s.
#[derive(Debug, Default)]
pub struct PrefabLoader;

impl AssetLoader for PrefabLoader {
    type Asset = PrefabSource;

    fn extensions(&self) -> &[&str] {
        &["prefab.ron"]
    }

    fn load<'a>(
        &'a self,
        ctx: &'a mut LoadContext,
    ) -> BoxedFuture<'a, Result<PrefabSource, BoxError>> {
        Box::pin(async move { Ok(PrefabSource(String::from_utf8(ctx.take_bytes())?)) })
    }
}

/// Prefab files by the name prefabs are spawned with. Each loaded or changed file is
/// handed to [`Prefabs`], which parses it the next time it is spawned; entities already
/// spawned from the old version stay as they are.
#[derive(Resource, Debug, Default)]
pub struct PrefabFiles {
    files: BTreeMap<String, (Handle<PrefabSource>, Option<u64>)>,
}

impl PrefabFiles {
    /// Names a loaded prefab file.
    pub fn add(&mut self, name: impl Into<String>, handle: Handle<PrefabSource>) {
        self.files.insert(name.into(), (handle, None));
    }

    /// The handle of the file named `name`.
    pub fn get(&self, name: &str) -> Option<&Handle<PrefabSource>> {
        self.files.get(name).map(|(handle, _)| handle)
    }
}

/// Copies new and changed prefab files into [`Prefabs`].
pub(crate) fn sync_prefab_files(
    mut files: ResMut<PrefabFiles>,
    sources: Res<Assets<PrefabSource>>,
    mut prefabs: ResMut<Prefabs>,
) {
    for (name, (handle, synced)) in &mut files.files {
        let Some(version) = sources.version(&*handle) else {
            continue;
        };
        if *synced == Some(version) {
            continue;
        }
        if synced.is_some() {
            log::info!("prefab `{name}` changed; new spawns use it");
        }
        *synced = Some(version);
        prefabs.add_ron(name.clone(), sources.get(&*handle).map_or("", |s| &s.0));
    }
}
