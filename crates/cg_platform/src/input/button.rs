//! [`ButtonInput`]: pressed, just-pressed and just-released state for any button type.

use cg_core::ecs::resource::Resource;
use std::collections::HashSet;
use std::hash::Hash;

/// Pressed, just-pressed and just-released state for a set of buttons: keys
/// (`ButtonInput<KeyCode>`), mouse buttons (`ButtonInput<MouseButton>`) or gamepad buttons.
///
/// "Just" states last for one frame: the runner records presses and releases as they
/// arrive, systems read them during the next frame, and the input plugin clears them at the
/// end of it. A press and release inside one frame shows as both just-pressed and
/// just-released while not pressed, so quick taps are never lost.
///
/// Raw just-states are per frame. In [`FixedUpdate`](cg_core::FixedUpdate), which can run
/// zero or several times per frame, read [`ActionState`](crate::input::ActionState) instead.
#[derive(Resource, Debug, Clone)]
pub struct ButtonInput<T: Copy + Eq + Hash + Send + Sync + 'static> {
    pressed: HashSet<T>,
    just_pressed: HashSet<T>,
    just_released: HashSet<T>,
}

impl<T: Copy + Eq + Hash + Send + Sync + 'static> Default for ButtonInput<T> {
    fn default() -> Self {
        Self {
            pressed: HashSet::new(),
            just_pressed: HashSet::new(),
            just_released: HashSet::new(),
        }
    }
}

impl<T: Copy + Eq + Hash + Send + Sync + 'static> ButtonInput<T> {
    /// Records a press. Repeated presses of a held button are ignored.
    pub fn press(&mut self, button: T) {
        if self.pressed.insert(button) {
            self.just_pressed.insert(button);
        }
    }

    /// Records a release. Releasing a button that is not pressed is ignored.
    pub fn release(&mut self, button: T) {
        if self.pressed.remove(&button) {
            self.just_released.insert(button);
        }
    }

    /// Releases every pressed button, for example when the window loses focus.
    pub fn release_all(&mut self) {
        self.just_released.extend(self.pressed.drain());
    }

    /// Whether the button is held down.
    pub fn pressed(&self, button: T) -> bool {
        self.pressed.contains(&button)
    }

    /// Whether any of the buttons is held down.
    pub fn any_pressed(&self, buttons: impl IntoIterator<Item = T>) -> bool {
        buttons.into_iter().any(|b| self.pressed(b))
    }

    /// Whether the button was pressed this frame.
    pub fn just_pressed(&self, button: T) -> bool {
        self.just_pressed.contains(&button)
    }

    /// Whether any of the buttons was pressed this frame.
    pub fn any_just_pressed(&self, buttons: impl IntoIterator<Item = T>) -> bool {
        buttons.into_iter().any(|b| self.just_pressed(b))
    }

    /// Whether the button was released this frame.
    pub fn just_released(&self, button: T) -> bool {
        self.just_released.contains(&button)
    }

    /// Whether any of the buttons was released this frame.
    pub fn any_just_released(&self, buttons: impl IntoIterator<Item = T>) -> bool {
        buttons.into_iter().any(|b| self.just_released(b))
    }

    /// All buttons held down, in no particular order.
    pub fn get_pressed(&self) -> impl ExactSizeIterator<Item = &T> {
        self.pressed.iter()
    }

    /// All buttons pressed this frame, in no particular order.
    pub fn get_just_pressed(&self) -> impl ExactSizeIterator<Item = &T> {
        self.just_pressed.iter()
    }

    /// All buttons released this frame, in no particular order.
    pub fn get_just_released(&self) -> impl ExactSizeIterator<Item = &T> {
        self.just_released.iter()
    }

    /// Clears the just-pressed and just-released states. The input plugin calls this at the
    /// end of every frame.
    pub fn clear(&mut self) {
        self.just_pressed.clear();
        self.just_released.clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn press_hold_release() {
        let mut input = ButtonInput::default();
        input.press('a');
        assert!(input.pressed('a') && input.just_pressed('a'));
        input.clear();
        input.press('a'); // key repeat
        assert!(input.pressed('a') && !input.just_pressed('a'));
        input.release('a');
        assert!(!input.pressed('a') && input.just_released('a'));
        input.clear();
        assert!(!input.just_released('a'));
        assert_eq!(input.get_pressed().len(), 0);
    }

    #[test]
    fn tap_within_one_frame_is_visible() {
        let mut input = ButtonInput::default();
        input.press(1);
        input.release(1);
        assert!(!input.pressed(1));
        assert!(input.just_pressed(1) && input.just_released(1));
    }

    #[test]
    fn release_all_reports_releases() {
        let mut input = ButtonInput::default();
        input.press(1);
        input.press(2);
        input.clear();
        input.release_all();
        assert!(!input.any_pressed([1, 2]));
        assert!(input.just_released(1) && input.just_released(2));
        input.release(3);
        assert!(!input.just_released(3));
    }
}
