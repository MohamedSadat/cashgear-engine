//! [`TextInput`]: the characters typed this frame.

use cg_core::ecs::prelude::*;

/// The text typed this frame, in order, as the keyboard layout and the system produce it
/// (shifted letters, accents, other scripts), for text fields and the debug overlay. Keys
/// that type nothing (arrows, Enter, Backspace) are left out: read those from
/// [`ButtonInput<KeyCode>`](super::ButtonInput). Cleared at the end of each frame.
///
/// ```
/// use cg_core::prelude::*;
/// use cg_platform::input::TextInput;
///
/// fn type_name(text: Res<TextInput>, mut name: Local<String>) {
///     name.push_str(text.typed());
/// }
/// ```
#[derive(Resource, Debug, Clone, Default, PartialEq, Eq)]
pub struct TextInput {
    typed: String,
}

impl TextInput {
    /// The text typed this frame.
    pub fn typed(&self) -> &str {
        &self.typed
    }

    /// Adds typed text, dropping control characters. The runner calls it for key presses.
    pub fn push(&mut self, text: &str) {
        self.typed.extend(text.chars().filter(|c| !c.is_control()));
    }

    /// Forgets this frame's text. The input plugin calls it at the end of each frame.
    pub fn clear(&mut self) {
        self.typed.clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn control_characters_are_dropped() {
        let mut text = TextInput::default();
        text.push("a\u{8}");
        text.push("\r");
        text.push("é٣");
        assert_eq!(text.typed(), "aé٣");
        text.clear();
        assert_eq!(text.typed(), "");
    }
}
