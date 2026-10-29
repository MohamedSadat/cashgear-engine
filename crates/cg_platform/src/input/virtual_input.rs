//! [`VirtualInput`]: actions pressed by code, such as on-screen touch controls or a demo bot.

use cg_core::ecs::prelude::*;
use std::collections::HashSet;

/// Actions pressed without a physical input: on-screen touch controls, a menu's focus
/// navigation, a scripted demo. [`ActionState`](super::ActionState) counts an action as
/// active while any of its bindings is, or while it is held or tapped here, so game code
/// reads touch buttons and keys the same way.
///
/// An action pressed here does not need to be in the [`ActionMap`](super::ActionMap).
///
/// ```
/// use cg_core::prelude::*;
/// use cg_platform::input::VirtualInput;
///
/// fn touch_jump(mut input: ResMut<VirtualInput>) {
///     input.press("jump");   // held until released
///     input.release("jump");
///     input.tap("pause");    // active for one frame
/// }
/// ```
#[derive(Resource, Debug, Clone, Default)]
pub struct VirtualInput {
    held: HashSet<String>,
    taps: HashSet<String>,
}

impl VirtualInput {
    /// Holds an action down until [`release`](Self::release).
    pub fn press(&mut self, action: &str) {
        if !self.held.contains(action) {
            self.held.insert(action.to_owned());
        }
    }

    /// Lets go of an action held with [`press`](Self::press).
    pub fn release(&mut self, action: &str) {
        self.held.remove(action);
    }

    /// Presses or releases an action.
    pub fn set(&mut self, action: &str, held: bool) {
        if held {
            self.press(action);
        } else {
            self.release(action);
        }
    }

    /// Makes an action active for the next input update only: it is just pressed in one
    /// frame and just released in the next, like a key tapped within a frame.
    pub fn tap(&mut self, action: &str) {
        self.taps.insert(action.to_owned());
    }

    /// Whether an action is held here.
    pub fn held(&self, action: &str) -> bool {
        self.held.contains(action)
    }

    /// Releases every held action.
    pub fn release_all(&mut self) {
        self.held.clear();
    }

    /// Whether the action counts as active in this input update.
    pub(crate) fn active(&self, action: &str) -> bool {
        self.held.contains(action) || self.taps.contains(action)
    }

    /// The actions active in this input update.
    pub(crate) fn active_names(&self) -> impl Iterator<Item = &str> {
        self.held.iter().chain(&self.taps).map(String::as_str)
    }

    /// Ends this input update: taps are used up.
    pub(crate) fn end_update(&mut self) {
        self.taps.clear();
    }
}
