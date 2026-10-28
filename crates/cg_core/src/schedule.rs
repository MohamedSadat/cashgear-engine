//! Schedule labels for the stages of a frame.
//!
//! [`App::update`](crate::App::update) runs one frame in this order:
//!
//! 1. [`Startup`], on the first frame only.
//! 2. [`First`]: engine bookkeeping (message buffers).
//! 3. [`PreUpdate`]: input is turned into action state.
//! 4. Zero or more fixed steps, each running [`FixedPreUpdate`], [`FixedUpdate`] and
//!    [`FixedPostUpdate`]. Gameplay belongs in [`FixedUpdate`].
//! 5. [`Update`]: per-frame logic that does not need a fixed rate.
//! 6. [`PostUpdate`]: engine work that reacts to this frame's changes.
//! 7. [`Last`]: end-of-frame cleanup, such as clearing just-pressed input.

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

/// Runs last in every frame. Reserved for engine cleanup.
#[derive(ScheduleLabel, Debug, Default, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Last;
