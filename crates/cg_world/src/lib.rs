//! World layer of CashGear Engine: 2D transforms and the parent/child hierarchy, smooth
//! drawing between fixed steps, game states as a stack, and prefabs.
//!
//! The ECS itself is [`bevy_ecs`], re-exported by `cg_core`. This crate adds:
//!
//! | Item | Does |
//! | --- | --- |
//! | [`Transform2D`], [`GlobalTransform2D`] | Local and world transforms, propagated down `ChildOf`/`Children` |
//! | [`Interpolated`] | Opt-in smoothing of entities that move in fixed steps |
//! | [`State`], [`NextState`], [`OnEnter`] ... | A stack of game states with enter, exit, pause and resume schedules |
//! | [`StateScoped`] | Entities that despawn when their state exits |
//! | [`Prefab`], [`Prefabs`], [`PrefabRegistry`] | Entities described in RON, spawned with `commands.spawn_prefab` |
//! | [`EditMode`], [`PlayStarted`], [`PlayStopped`] | What an editor running the game says: editing or playing |
//!
//! ```
//! use cg_core::prelude::*;
//! use cg_world::prelude::*;
//!
//! #[derive(Clone, PartialEq, Eq, Hash, Debug)]
//! enum GameState { Menu, Playing }
//!
//! #[derive(Component, Clone, serde::Serialize, serde::Deserialize)]
//! struct Velocity(Vec2);
//!
//! fn enter_playing(mut commands: Commands) {
//!     commands
//!         .spawn_prefab("bunny")
//!         .insert(StateScoped(GameState::Playing));
//! }
//!
//! fn fly(time: Res<Time>, mut query: Query<(&mut Transform2D, &Velocity)>) {
//!     for (mut transform, velocity) in &mut query {
//!         transform.translation += velocity.0 * time.fixed_delta_secs();
//!     }
//! }
//!
//! App::new()
//!     .add_plugins(WorldPlugin)
//!     .register_component::<Velocity>("Velocity")
//!     .add_prefab("bunny", r#"(components: { "Velocity": ((60, 0)) })"#)
//!     .init_state(GameState::Menu)
//!     .add_systems(OnEnter(GameState::Playing), enter_playing)
//!     .add_systems(FixedUpdate, fly.run_if(in_state(GameState::Playing)))
//!     .run();
//! ```

mod edit;
mod interpolate;
mod prefab;
mod state;
mod transform;

pub use edit::{EditMode, PlayStarted, PlayStopped, editing, in_editor};
pub use interpolate::{
    Interpolated, InterpolatedTransforms, apply_interpolation_resets, lerp_affine,
    snapshot_interpolated,
};
pub use prefab::{
    Prefab, PrefabCommandsExt, PrefabComponent, PrefabError, PrefabRegistry, Prefabs, insert_prefab,
};
pub use state::{
    NextState, OnEnter, OnExit, OnPause, OnResume, State, StateChange, StateScoped, States,
    in_state, state_in_stack,
};
pub use transform::{
    GlobalTransform2D, Transform2D, TransformPropagation, TransformSystems, propagate_transforms,
};

use bevy_ecs::prelude::*;
use cg_core::{App, FixedPostUpdate, FixedPreUpdate, Plugin, PostUpdate, StateTransition};
use serde::Serialize;
use serde::de::DeserializeOwned;

/// Adds transforms, interpolation and prefabs: the [`PrefabRegistry`] and [`Prefabs`]
/// resources, transform propagation in [`FixedPreUpdate`], [`FixedPostUpdate`] and
/// [`PostUpdate`], and the [`Interpolated`] bookkeeping. Registers `Transform2D`, `Name` and
/// `Interpolated` for prefabs, and the editor's [`PlayStarted`] and [`PlayStopped`]
/// messages.
///
/// `DefaultPlugins` includes it; the renderer adds it if it is missing.
#[derive(Debug, Clone, Copy, Default)]
pub struct WorldPlugin;

impl Plugin for WorldPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<PrefabRegistry>()
            .init_resource::<Prefabs>()
            .init_resource::<TransformPropagation>()
            .add_message::<PlayStarted>()
            .add_message::<PlayStopped>()
            .register_component::<Transform2D>("Transform2D")
            .register_prefab_component::<Name>("Name")
            .register_prefab_component::<Interpolated>("Interpolated")
            // Entities spawned since the last propagation (in `Startup`, say) get their
            // global transform before the snapshot, so they do not blend in from the origin.
            .add_systems(
                FixedPreUpdate,
                (
                    propagate_transforms.in_set(TransformSystems::Propagate),
                    snapshot_interpolated.after(TransformSystems::Propagate),
                ),
            )
            .add_systems(
                FixedPostUpdate,
                (
                    propagate_transforms.in_set(TransformSystems::Propagate),
                    apply_interpolation_resets.after(TransformSystems::Propagate),
                ),
            )
            .add_systems(
                PostUpdate,
                (
                    propagate_transforms.in_set(TransformSystems::Propagate),
                    apply_interpolation_resets.after(TransformSystems::Propagate),
                ),
            );
    }
}

impl PrefabComponent for Name {
    type Data = String;

    fn from_data(name: &String, _: &World) -> Result<Self, String> {
        Ok(Name::new(name.clone()))
    }
}

impl PrefabComponent for Interpolated {
    type Data = ();

    fn from_data(_: &(), _: &World) -> Result<Self, String> {
        Ok(Interpolated::default())
    }
}

/// World-layer additions to [`App`]: prefab components and prefabs, and game states.
pub trait AppWorldExt {
    /// Lets prefabs use the component `T` under `name`, read as it is with serde. Give the
    /// type `#[serde(default)]` so prefabs can leave fields out.
    fn register_component<T: Component + Clone + Serialize + DeserializeOwned>(
        &mut self,
        name: &str,
    ) -> &mut Self;

    /// Lets prefabs use the [`PrefabComponent`] `T` under `name`.
    fn register_prefab_component<T: PrefabComponent>(&mut self, name: &str) -> &mut Self;

    /// Adds a prefab as RON text, to be parsed the first time it is spawned. See
    /// [`Prefabs`].
    fn add_prefab(&mut self, name: &str, source: &str) -> &mut Self;

    /// Adds the [`State`] stack for `S`, starting with `initial`, and the [`NextState`]
    /// resource for changing it. [`OnEnter`] of the initial state runs in the first frame's
    /// [`StateTransition`] schedule, after `Startup`.
    ///
    /// Calling it again for the same type does nothing but warn.
    fn init_state<S: States>(&mut self, initial: S) -> &mut Self;
}

impl AppWorldExt for App {
    fn register_component<T: Component + Clone + Serialize + DeserializeOwned>(
        &mut self,
        name: &str,
    ) -> &mut Self {
        self.world_mut()
            .get_resource_or_init::<PrefabRegistry>()
            .register::<T>(name);
        self
    }

    fn register_prefab_component<T: PrefabComponent>(&mut self, name: &str) -> &mut Self {
        self.world_mut()
            .get_resource_or_init::<PrefabRegistry>()
            .register_with::<T>(name);
        self
    }

    fn add_prefab(&mut self, name: &str, source: &str) -> &mut Self {
        self.world_mut()
            .get_resource_or_init::<Prefabs>()
            .add_ron(name, source);
        self
    }

    fn init_state<S: States>(&mut self, initial: S) -> &mut Self {
        if self.world().contains_resource::<State<S>>() {
            log::warn!(
                "state `{}` was initialised twice; keeping the first",
                core::any::type_name::<S>()
            );
            return self;
        }
        state::init(self.world_mut(), initial);
        self.add_systems(StateTransition, state::apply_state_changes::<S>)
    }
}

/// Common imports: the plugin, transforms, interpolation, states, prefabs and what an editor
/// running the game says, plus the `ChildOf`/`Children` hierarchy and `Name` from the ECS.
pub mod prelude {
    pub use crate::{
        AppWorldExt, EditMode, GlobalTransform2D, Interpolated, NextState, OnEnter, OnExit,
        OnPause, OnResume, PlayStarted, PlayStopped, Prefab, PrefabCommandsExt, PrefabComponent,
        Prefabs, State, StateScoped, Transform2D, WorldPlugin, editing, in_editor, in_state,
        state_in_stack,
    };
    pub use bevy_ecs::hierarchy::{ChildOf, Children};
    pub use bevy_ecs::name::Name;
}
