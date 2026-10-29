//! Animation and effects for CashGear Engine: sprite animation from Aseprite exports,
//! animation state machines with events, tweens, CPU particles, and camera follow, shake
//! and screen fades.
//!
//! | Item | Does |
//! | --- | --- |
//! | [`SpriteSheet`], [`AsepriteLoader`] | Frames and clips from an Aseprite `.json` export, or built in code from an atlas |
//! | [`SpriteAnimation`] | Plays a clip on the entity's sprite; sends [`AnimationEvent`]s and [`AnimationFinished`] |
//! | [`AnimationGraph`], [`Animator`] | A state machine (`.anim.ron`) that picks clips from parameters such as `speed` and `grounded` |
//! | [`Tween`], [`Tweens`], [`Ease`] | Eases position, scale, rotation, colour, flash or zoom; [`TweenCommandsExt::tween`] starts one |
//! | [`ParticleEmitter`], [`ParticleSettings`] | Bursts and streams of particles with gravity, drag, and size and colour over life |
//! | [`CameraFollow`], [`CameraShake`] | A camera that follows with a dead zone inside level bounds, and trauma-based shake |
//! | [`ScreenFade`], [`StateFade`] | Fading the screen, and changing game state behind a fade |
//!
//! ```
//! use cg_anim::prelude::*;
//! use cg_assets::AssetServer;
//! use cg_core::prelude::*;
//! use cg_world::prelude::*;
//!
//! fn spawn_hero(mut commands: Commands, server: Res<AssetServer>) {
//!     commands.spawn((
//!         SpriteAnimation::new(server.load("images/hero.json"), "idle").with_pivot(FEET),
//!         Animator::new(server.load("anims/hero.anim.ron")),
//!         Transform2D::from_xy(40.0, 16.0),
//!     ));
//! }
//!
//! fn footsteps(mut events: MessageReader<AnimationEvent>) {
//!     for event in events.read() {
//!         if event.name == "footstep" {
//!             info!("step");
//!         }
//!     }
//! }
//!
//! App::new()
//!     .add_plugins(WorldPlugin)
//!     .add_plugins(AnimPlugin::default())
//!     .add_systems(Startup, spawn_hero)
//!     .add_systems(FixedUpdate, footsteps)
//!     .run();
//! ```
//!
//! What runs where:
//!
//! - **Fixed steps** ([`FixedPostUpdate`], after the physics): transform tweens on
//!   [`Interpolated`](cg_world::Interpolated) entities, animators, clips and their events
//!   ([`AnimSystems::Animate`]), then camera following ([`AnimSystems::Camera`]). Gameplay
//!   in `FixedUpdate` sets animator parameters and reads events from the step before.
//! - **Every frame** ([`Update`], [`AnimSystems::Effects`]): the other tweens, particles,
//!   shake decay and fades.
//! - **Drawing** ([`Render`]): shake and camera bounds adjust the frame's view before
//!   anything is queued; particles and the fade are queued with the sprites.

mod aseprite;
mod camera;
mod ease;
mod fade;
mod machine;
mod particles;
mod player;
mod rng;
mod sheet;
mod tween;

pub use aseprite::{AsepriteExport, AsepriteLoader};
pub use camera::{CameraFollow, CameraShake};
pub use ease::Ease;
pub use fade::{AppFadeExt, ScreenFade, StateFade};
pub use machine::{
    AnimationGraph, AnimationGraphLoader, AnimationState, AnimationStateChanged, Animator,
    AnimatorData, Condition, Test, Transition,
};
pub use particles::{
    EmitShape, Particle, ParticleBudget, ParticleEmitter, ParticleEmitterData, ParticleSettings,
};
pub use player::{AnimationEvent, AnimationFinished, FEET, SpriteAnimation, SpriteAnimationData};
pub use sheet::{AnimationClip, ClipEvent, ClipFrame, Direction, SheetFrame, SpriteSheet};
pub use tween::{
    Tween, TweenCommandsExt, TweenData, TweenFinished, TweenProperty, TweenValue, Tweens,
};

use bevy_ecs::schedule::{IntoScheduleConfigs, SystemSet};
use cg_assets::AppAssetExt;
use cg_core::{App, FixedPostUpdate, Plugin, PostUpdate, Render, Update};
use cg_physics::PhysicsSystems;
use cg_render::{RenderPlugin, RenderSystems};
use cg_world::AppWorldExt;

/// System sets of the animation systems.
#[derive(SystemSet, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum AnimSystems {
    /// In [`FixedPostUpdate`] after the physics: fixed-step tweens, animators, then clips.
    Animate,
    /// In [`FixedPostUpdate`] after [`Animate`](Self::Animate): cameras follow.
    Camera,
    /// In [`Update`]: frame tweens, particles, shake decay and fades.
    Effects,
}

/// Adds animation and effects: the [`SpriteSheet`] asset with the [`AsepriteLoader`]
/// (`.json`), the [`AnimationGraph`] asset with its loader (`.anim.ron`), the messages,
/// the [`ParticleBudget`] and [`ScreenFade`] resources, the systems, and the prefab
/// components `SpriteAnimation`, `Animator`, `ParticleEmitter` and `CameraShake`. Adds
/// [`RenderPlugin`] if it is missing.
///
/// `DefaultPlugins` includes it.
#[derive(Debug, Clone, PartialEq)]
pub struct AnimPlugin {
    /// The most particles alive at once across all emitters.
    pub particle_budget: u32,
}

impl Default for AnimPlugin {
    fn default() -> Self {
        Self {
            particle_budget: ParticleBudget::default().max,
        }
    }
}

impl Plugin for AnimPlugin {
    fn build(&self, app: &mut App) {
        if !app.is_plugin_added::<RenderPlugin>() {
            app.add_plugins(RenderPlugin::default());
        }
        app.init_asset::<SpriteSheet>()
            .register_loader(AsepriteLoader)
            .init_asset::<AnimationGraph>()
            .register_loader(AnimationGraphLoader)
            .add_message::<AnimationEvent>()
            .add_message::<AnimationFinished>()
            .add_message::<AnimationStateChanged>()
            .add_message::<TweenFinished>()
            .insert_resource(ParticleBudget::new(self.particle_budget))
            .init_resource::<ScreenFade>()
            .register_prefab_component::<SpriteAnimation>("SpriteAnimation")
            .register_prefab_component::<Animator>("Animator")
            .register_prefab_component::<ParticleEmitter>("ParticleEmitter")
            .register_component::<CameraShake>("CameraShake")
            .add_systems(
                FixedPostUpdate,
                (
                    (
                        tween::advance_fixed_tweens,
                        machine::run_animators,
                        player::play_animations,
                    )
                        .chain()
                        .in_set(AnimSystems::Animate)
                        .after(PhysicsSystems::Detect),
                    camera::follow_targets
                        .in_set(AnimSystems::Camera)
                        .after(AnimSystems::Animate),
                ),
            )
            .add_systems(
                Update,
                (
                    tween::advance_frame_tweens,
                    particles::update_particles,
                    camera::decay_shake,
                    fade::advance_fade,
                )
                    .chain()
                    .in_set(AnimSystems::Effects),
            )
            .add_systems(PostUpdate, player::show_changed_animations)
            .add_systems(
                Render,
                (
                    camera::adjust_view
                        .after(RenderSystems::View)
                        .before(RenderSystems::Queue),
                    (particles::queue_particles, fade::queue_fade)
                        .in_set(RenderSystems::Queue)
                        .after(RenderSystems::View),
                ),
            );
    }
}

/// Common imports: the plugin, sheets and clips, the animation components and messages,
/// tweens, particles, camera effects and fades.
pub mod prelude {
    pub use crate::{
        AnimPlugin, AnimSystems, AnimationClip, AnimationEvent, AnimationFinished, AnimationGraph,
        AnimationState, AnimationStateChanged, Animator, AppFadeExt, CameraFollow, CameraShake,
        Direction, Ease, EmitShape, FEET, ParticleBudget, ParticleEmitter, ParticleSettings,
        ScreenFade, SpriteAnimation, SpriteSheet, StateFade, Test, Transition, Tween,
        TweenCommandsExt, TweenFinished, TweenProperty, Tweens,
    };
}

#[cfg(test)]
mod tests;
