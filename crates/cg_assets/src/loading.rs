//! Waiting for groups of assets: [`LoadingAssets`] and loading-screen states.

use crate::{AssetServer, LoadState, UntypedHandle};
use bevy_ecs::prelude::*;
use cg_world::{NextState, States};

/// The assets a loading screen waits for. Add handles to it (in the loading state's
/// [`OnEnter`](cg_world::OnEnter), say) and read [`progress`](Self::progress) to draw a
/// bar. With [`add_loading_state`](crate::AppAssetExt::add_loading_state), the game moves
/// on by itself once everything has loaded.
///
/// It holds strong handles, so the assets stay loaded until [`clear`](Self::clear) is
/// called, even if the game drops its own handles.
///
/// ```
/// use cg_assets::prelude::*;
/// use cg_core::prelude::*;
///
/// struct Level(String);
/// impl Asset for Level {}
///
/// fn start_loading(server: Res<AssetServer>, mut loading: ResMut<LoadingAssets>) {
///     let level: Handle<Level> = server.load("levels/one.txt");
///     loading.add(&level);
/// }
///
/// fn draw_bar(server: Res<AssetServer>, loading: Res<LoadingAssets>) {
///     let progress = loading.progress(&server);
///     info!("{:.0}%", progress.fraction() * 100.0);
/// }
/// ```
#[derive(Resource, Debug, Default, Clone)]
pub struct LoadingAssets {
    handles: Vec<UntypedHandle>,
}

impl LoadingAssets {
    /// Adds an asset to wait for, with everything it depends on.
    pub fn add(&mut self, handle: impl Into<UntypedHandle>) {
        let handle = handle.into();
        if !self.handles.contains(&handle) {
            self.handles.push(handle);
        }
    }

    /// The assets in the group.
    pub fn handles(&self) -> &[UntypedHandle] {
        &self.handles
    }

    /// Empties the group, dropping its handles.
    pub fn clear(&mut self) {
        self.handles.clear();
    }

    /// How many assets in the group have loaded (with their dependencies) or failed.
    /// Assets the server does not know, such as ones made in code, count as loaded.
    pub fn progress(&self, server: &AssetServer) -> LoadProgress {
        let mut progress = LoadProgress {
            total: self.handles.len(),
            ..LoadProgress::default()
        };
        for handle in &self.handles {
            match server.recursive_load_state(handle) {
                LoadState::Loaded | LoadState::NotLoaded => progress.loaded += 1,
                LoadState::Failed => progress.failed += 1,
                LoadState::Loading => {}
            }
        }
        progress
    }
}

/// How far a group of assets has got. See [`LoadingAssets::progress`].
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct LoadProgress {
    /// Assets loaded with their dependencies.
    pub loaded: usize,
    /// Assets that, or whose dependencies, failed to load.
    pub failed: usize,
    /// Assets in the group.
    pub total: usize,
}

impl LoadProgress {
    /// Whether nothing is still loading.
    pub fn done(&self) -> bool {
        self.loaded + self.failed >= self.total
    }

    /// The share finished (loaded or failed), from 0 to 1. An empty group is finished.
    pub fn fraction(&self) -> f32 {
        if self.total == 0 {
            1.0
        } else {
            (self.loaded + self.failed) as f32 / self.total as f32
        }
    }
}

/// Moves from the loading state to `next` once [`LoadingAssets`] is done. Failures were
/// already logged one by one; the game moves on anyway, and missing textures draw nothing.
///
/// It runs in `Update` while the loading state is current. The change applies in the next
/// frame's state transition, before `Update` could run it again.
pub(crate) fn finish_loading<S: States>(
    next: S,
) -> impl FnMut(Res<AssetServer>, Res<LoadingAssets>, ResMut<NextState<S>>) {
    move |server, loading, mut next_state| {
        let progress = loading.progress(&server);
        if !progress.done() {
            return;
        }
        if progress.failed > 0 {
            log::warn!(
                "{} of {} assets failed to load; carrying on without them",
                progress.failed,
                progress.total
            );
        } else {
            log::info!("loaded {} assets", progress.total);
        }
        next_state.replace(next.clone());
    }
}
