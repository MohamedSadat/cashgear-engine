//! Core of CashGear Engine: the [`App`] builder, the [`Plugin`] trait, the fixed-timestep
//! game loop, the [`Time`] resource, messages and the [`Rect`] type. Re-exports
//! [`bevy_ecs`] as [`ecs`], [`glam`] for maths and [`log`] for logging.
//!
//! This crate has no platform code. The window, the input and the runner that drives the
//! frames come from `cg_platform`; logging output is set up there too.

mod app;
mod exit;
mod plugin;
mod rect;
pub mod schedule;
mod time;

pub use app::App;
pub use exit::{AppExit, FrameLimit, FrameLimitPlugin};
pub use plugin::Plugin;
pub use rect::Rect;
pub use schedule::{
    First, FixedPostUpdate, FixedPreUpdate, FixedUpdate, Last, PostUpdate, PreUpdate, Render,
    Startup, Update,
};
pub use time::{DEFAULT_FIXED_HZ, DEFAULT_MAX_DELTA, DEFAULT_MAX_FIXED_STEPS, Time};

pub use bevy_ecs as ecs;
pub use glam;
pub use log;

/// Shorthand for [`Default::default`], handy in struct literals:
/// `WindowConfig { title: "Game".into(), ..default() }`.
pub fn default<T: Default>() -> T {
    T::default()
}

/// Common imports: the app, schedules, time, the ECS prelude, `glam` vectors, [`Rect`] and
/// the logging macros.
pub mod prelude {
    pub use crate::schedule::{
        First, FixedPostUpdate, FixedPreUpdate, FixedUpdate, Last, PostUpdate, PreUpdate, Render,
        Startup, Update,
    };
    pub use crate::{App, AppExit, Plugin, Rect, Time, default};
    pub use bevy_ecs::prelude::*;
    // bevy's derive macros (`Resource`, `Component`, `Message`) expand to `bevy_ecs::...`
    // paths. This makes them resolve in game crates that depend only on the engine.
    #[doc(hidden)]
    pub use bevy_ecs;
    pub use glam::{
        Affine2, IVec2, Mat2, Mat3, UVec2, Vec2, Vec3, Vec4, ivec2, uvec2, vec2, vec3, vec4,
    };
    pub use log::{debug, error, info, trace, warn};
}
