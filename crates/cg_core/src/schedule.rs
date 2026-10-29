//! Schedule labels for the stages of a frame.
//!
//! [`App::update`](crate::App::update) runs one frame in this order:
//!
//! 1. [`Startup`], on the first frame only.
//! 2. [`First`]: engine bookkeeping (message buffers).
//! 3. [`PreUpdate`]: input is turned into action state.
//! 4. [`StateTransition`]: requested game-state changes are applied.
//! 5. Zero or more fixed steps, each running [`FixedPreUpdate`], [`FixedUpdate`] and
//!    [`FixedPostUpdate`]. Gameplay belongs in [`FixedUpdate`].
//! 6. [`Update`]: per-frame logic that does not need a fixed rate.
//! 7. [`PostUpdate`]: engine work that reacts to this frame's changes, such as transform
//!    propagation.
//! 8. [`Render`]: the renderer draws the frame.
//! 9. [`Last`]: end-of-frame cleanup, such as clearing just-pressed input.

use bevy_ecs::schedule::ScheduleLabel;

/// Runs once, at the start of the first frame, after the window exists.
#[derive(ScheduleLabel, Debug, Default, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Startup;

/// Runs first in every frame. Reserved for engine bookkeeping.
#[derive(ScheduleLabel, Debug, Default, Clone, Copy, PartialEq, Eq, Hash)]
pub struct First;

/// Runs before the fixed steps. The input plugin updates action state here.
#[derive(ScheduleLabel, Debug, Default, Clone, Copy, PartialEq, Eq, Hash)]
pub struct PreUpdate;

/// Runs after [`PreUpdate`], before the fixed steps. Game-state changes (`cg_world`'s
/// state stack) are applied here, so a frame's gameplay sees the new state.
#[derive(ScheduleLabel, Debug, Default, Clone, Copy, PartialEq, Eq, Hash)]
pub struct StateTransition;

/// Runs before [`FixedUpdate`] in every fixed step.
#[derive(ScheduleLabel, Debug, Default, Clone, Copy, PartialEq, Eq, Hash)]
pub struct FixedPreUpdate;

/// Gameplay: runs at the fixed rate (60 Hz by default), zero or more times per frame.
/// Use [`Time::fixed_delta`](crate::Time::fixed_delta) for its maths.
#[derive(ScheduleLabel, Debug, Default, Clone, Copy, PartialEq, Eq, Hash)]
pub struct FixedUpdate;

/// Runs after [`FixedUpdate`] in every fixed step.
#[derive(ScheduleLabel, Debug, Default, Clone, Copy, PartialEq, Eq, Hash)]
pub struct FixedPostUpdate;

/// Runs once per frame after the fixed steps, with
/// [`Time::alpha`](crate::Time::alpha) set for interpolation.
#[derive(ScheduleLabel, Debug, Default, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Update;

/// Runs after [`Update`]. Reserved for engine work such as transform propagation.
#[derive(ScheduleLabel, Debug, Default, Clone, Copy, PartialEq, Eq, Hash)]
pub struct PostUpdate;

/// Runs after [`PostUpdate`]. The renderer draws what was queued this frame here.
#[derive(ScheduleLabel, Debug, Default, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Render;

/// Runs last in every frame. Reserved for engine cleanup.
#[derive(ScheduleLabel, Debug, Default, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Last;
