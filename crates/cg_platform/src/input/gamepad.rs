//! Gamepads through `gilrs`: native APIs on desktop, the Gamepad API on the web.

use super::ButtonInput;
use cg_core::ecs::prelude::*;
use serde::{Deserialize, Serialize};

/// A gamepad button, named by position (Xbox A is [`South`](Self::South), PlayStation
/// cross is [`South`](Self::South) too).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum GamepadButton {
    /// Bottom face button (Xbox A, PlayStation cross, Nintendo B).
    South,
    /// Right face button (Xbox B, PlayStation circle, Nintendo A).
    East,
    /// Top face button (Xbox Y, PlayStation triangle, Nintendo X).
    North,
    /// Left face button (Xbox X, PlayStation square, Nintendo Y).
    West,
    /// Left shoulder button (LB, L1).
    LeftBumper,
    /// Right shoulder button (RB, R1).
    RightBumper,
    /// Left trigger (LT, L2), pressed past the platform's threshold.
    LeftTrigger,
    /// Right trigger (RT, R2), pressed past the platform's threshold.
    RightTrigger,
    /// Select, Back, View or Share.
    Select,
    /// Start, Menu or Options.
    Start,
    /// The vendor button (Xbox, PS, Home).
    Mode,
    /// Pressing the left stick in.
    LeftStick,
    /// Pressing the right stick in.
    RightStick,
    /// D-pad up.
    DPadUp,
    /// D-pad down.
    DPadDown,
    /// D-pad left.
    DPadLeft,
    /// D-pad right.
    DPadRight,
}

/// A gamepad axis. Sticks range from `-1.0` to `1.0` with up and right positive; triggers
/// range from `0.0` to `1.0`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum GamepadAxis {
    /// Left stick, horizontal.
    LeftStickX,
    /// Left stick, vertical (up is positive).
    LeftStickY,
    /// Right stick, horizontal.
    RightStickX,
    /// Right stick, vertical (up is positive).
    RightStickY,
    /// Left trigger pressure.
    LeftTrigger,
    /// Right trigger pressure.
    RightTrigger,
}

impl GamepadAxis {
    fn index(self) -> usize {
        self as usize
    }
}

/// Identifies a connected gamepad. Ids are not reused while the app runs.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct GamepadId(pub usize);

/// One connected gamepad.
#[derive(Debug, Clone)]
pub struct Gamepad {
    name: String,
    buttons: ButtonInput<GamepadButton>,
    axes: [f32; 6],
}

impl Gamepad {
    /// The name the platform reports, such as "Xbox Wireless Controller".
    pub fn name(&self) -> &str {
        &self.name
    }

    /// Button state.
    pub fn buttons(&self) -> &ButtonInput<GamepadButton> {
        &self.buttons
    }

    /// Whether the button is held down.
    pub fn pressed(&self, button: GamepadButton) -> bool {
        self.buttons.pressed(button)
    }

    /// Whether the button was pressed this frame.
    pub fn just_pressed(&self, button: GamepadButton) -> bool {
        self.buttons.just_pressed(button)
    }

    /// Whether the button was released this frame.
    pub fn just_released(&self, button: GamepadButton) -> bool {
        self.buttons.just_released(button)
    }

    /// The axis value, without a deadzone applied.
    pub fn axis(&self, axis: GamepadAxis) -> f32 {
        self.axes[axis.index()]
    }
}

/// All connected gamepads, updated each frame in [`PreUpdate`](cg_core::PreUpdate).
///
/// The `any_*` methods treat all gamepads as one, which is what single-player games and the
/// action map want.
#[derive(Resource, Debug, Clone, Default)]
pub struct Gamepads {
    pads: Vec<(GamepadId, Gamepad)>,
}

impl Gamepads {
    /// Connected gamepads in connection order.
    pub fn iter(&self) -> impl ExactSizeIterator<Item = (GamepadId, &Gamepad)> {
        self.pads.iter().map(|(id, pad)| (*id, pad))
    }

    /// The gamepad with this id, if connected.
    pub fn get(&self, id: GamepadId) -> Option<&Gamepad> {
        self.pads.iter().find(|(i, _)| *i == id).map(|(_, pad)| pad)
    }

    /// Number of connected gamepads.
    pub fn len(&self) -> usize {
        self.pads.len()
    }

    /// Whether no gamepad is connected.
    pub fn is_empty(&self) -> bool {
        self.pads.is_empty()
    }

    /// Whether the button is held on any gamepad.
    pub fn any_pressed(&self, button: GamepadButton) -> bool {
        self.pads.iter().any(|(_, pad)| pad.pressed(button))
    }

    /// Whether the button was pressed this frame on any gamepad.
    pub fn any_just_pressed(&self, button: GamepadButton) -> bool {
        self.pads.iter().any(|(_, pad)| pad.just_pressed(button))
    }

    /// The axis value with the largest magnitude across all gamepads.
    pub fn axis(&self, axis: GamepadAxis) -> f32 {
        self.pads
            .iter()
            .map(|(_, pad)| pad.axis(axis))
            .fold(0.0, |best, v| if v.abs() > best.abs() { v } else { best })
    }

    fn pad_mut(&mut self, id: GamepadId) -> Option<&mut Gamepad> {
        self.pads
            .iter_mut()
            .find(|(i, _)| *i == id)
            .map(|(_, pad)| pad)
    }

    /// Adds a gamepad. Connecting an id that is already connected does nothing.
    pub fn connect(&mut self, id: GamepadId, name: impl Into<String>) {
        if self.get(id).is_none() {
            self.pads.push((
                id,
                Gamepad {
                    name: name.into(),
                    buttons: ButtonInput::default(),
                    axes: [0.0; 6],
                },
            ));
        }
    }

    /// Removes a gamepad.
    pub fn disconnect(&mut self, id: GamepadId) {
        self.pads.retain(|(i, _)| *i != id);
    }

    /// Records a button press on a connected gamepad.
    pub fn press(&mut self, id: GamepadId, button: GamepadButton) {
        if let Some(pad) = self.pad_mut(id) {
            pad.buttons.press(button);
        }
    }

    /// Records a button release on a connected gamepad.
    pub fn release(&mut self, id: GamepadId, button: GamepadButton) {
        if let Some(pad) = self.pad_mut(id) {
            pad.buttons.release(button);
        }
    }

    /// Sets an axis value on a connected gamepad, clamped to the axis range.
    pub fn set_axis(&mut self, id: GamepadId, axis: GamepadAxis, value: f32) {
        let min = match axis {
            GamepadAxis::LeftTrigger | GamepadAxis::RightTrigger => 0.0,
            _ => -1.0,
        };
        if let Some(pad) = self.pad_mut(id) {
            pad.axes[axis.index()] = if value.is_finite() {
                value.clamp(min, 1.0)
            } else {
                0.0
            };
        }
    }

    /// Clears per-frame button state. The input plugin calls this at the end of every frame.
    pub fn clear(&mut self) {
        for (_, pad) in &mut self.pads {
            pad.buttons.clear();
        }
    }
}

/// The `gilrs` context. It is not `Send` on every platform, so it lives in the world as a
/// non-send resource.
pub(crate) struct GilrsContext(gilrs::Gilrs);

impl GilrsContext {
    /// Opens the platform gamepad API and returns the gamepads already connected.
    pub(crate) fn new() -> Option<(Self, Vec<(GamepadId, String)>)> {
        let gilrs = match gilrs::Gilrs::new() {
            Ok(gilrs) => gilrs,
            Err(gilrs::Error::NotImplemented(_)) => {
                log::warn!("gamepads are not supported on this platform");
                return None;
            }
            Err(err) => {
                log::warn!("gamepads disabled: {err}");
                return None;
            }
        };
        let connected = gilrs
            .gamepads()
            .map(|(id, pad)| (GamepadId(id.into()), pad.name().to_owned()))
            .collect();
        Some((Self(gilrs), connected))
    }
}

/// Drains `gilrs` events into [`Gamepads`].
pub(crate) fn poll_gamepads(
    context: Option<NonSendMut<GilrsContext>>,
    mut gamepads: ResMut<Gamepads>,
) {
    let Some(mut context) = context else {
        return;
    };
    let gilrs = &mut context.0;
    while let Some(gilrs::Event { id, event, .. }) = gilrs.next_event() {
        let pad = GamepadId(id.into());
        match event {
            gilrs::EventType::Connected => {
                let name = gilrs.gamepad(id).name().to_owned();
                log::info!("gamepad {} connected: {name}", pad.0);
                gamepads.connect(pad, name);
            }
            gilrs::EventType::Disconnected => {
                log::info!("gamepad {} disconnected", pad.0);
                gamepads.disconnect(pad);
            }
            gilrs::EventType::ButtonPressed(button, _) => {
                if let Some(button) = map_button(button) {
                    gamepads.press(pad, button);
                }
            }
            gilrs::EventType::ButtonReleased(button, _) => {
                if let Some(button) = map_button(button) {
                    gamepads.release(pad, button);
                }
            }
            gilrs::EventType::ButtonChanged(button, value, _) => match button {
                gilrs::Button::LeftTrigger2 => {
                    gamepads.set_axis(pad, GamepadAxis::LeftTrigger, value);
                }
                gilrs::Button::RightTrigger2 => {
                    gamepads.set_axis(pad, GamepadAxis::RightTrigger, value);
                }
                _ => {}
            },
            gilrs::EventType::AxisChanged(axis, value, _) => {
                if let Some(axis) = map_axis(axis) {
                    gamepads.set_axis(pad, axis, value);
                }
            }
            _ => {}
        }
    }
    gilrs.inc();
}

fn map_button(button: gilrs::Button) -> Option<GamepadButton> {
    use gilrs::Button as B;
    Some(match button {
        B::South => GamepadButton::South,
        B::East => GamepadButton::East,
        B::North => GamepadButton::North,
        B::West => GamepadButton::West,
        B::LeftTrigger => GamepadButton::LeftBumper,
        B::RightTrigger => GamepadButton::RightBumper,
        B::LeftTrigger2 => GamepadButton::LeftTrigger,
        B::RightTrigger2 => GamepadButton::RightTrigger,
        B::Select => GamepadButton::Select,
        B::Start => GamepadButton::Start,
        B::Mode => GamepadButton::Mode,
        B::LeftThumb => GamepadButton::LeftStick,
        B::RightThumb => GamepadButton::RightStick,
        B::DPadUp => GamepadButton::DPadUp,
        B::DPadDown => GamepadButton::DPadDown,
        B::DPadLeft => GamepadButton::DPadLeft,
        B::DPadRight => GamepadButton::DPadRight,
        B::C | B::Z | B::Unknown => return None,
    })
}

fn map_axis(axis: gilrs::Axis) -> Option<GamepadAxis> {
    use gilrs::Axis as A;
    Some(match axis {
        A::LeftStickX => GamepadAxis::LeftStickX,
        A::LeftStickY => GamepadAxis::LeftStickY,
        A::RightStickX => GamepadAxis::RightStickX,
        A::RightStickY => GamepadAxis::RightStickY,
        A::LeftZ => GamepadAxis::LeftTrigger,
        A::RightZ => GamepadAxis::RightTrigger,
        A::DPadX | A::DPadY | A::Unknown => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn buttons_and_axes_merge_across_pads() {
        let mut pads = Gamepads::default();
        let (a, b) = (GamepadId(0), GamepadId(1));
        pads.connect(a, "A");
        pads.connect(b, "B");
        pads.press(b, GamepadButton::South);
        assert!(pads.any_pressed(GamepadButton::South));
        assert!(pads.any_just_pressed(GamepadButton::South));
        pads.set_axis(a, GamepadAxis::LeftStickX, 0.3);
        pads.set_axis(b, GamepadAxis::LeftStickX, -0.8);
        assert_eq!(pads.axis(GamepadAxis::LeftStickX), -0.8);
        pads.clear();
        assert!(!pads.any_just_pressed(GamepadButton::South));
        pads.disconnect(b);
        assert!(!pads.any_pressed(GamepadButton::South));
        assert_eq!(pads.axis(GamepadAxis::LeftStickX), 0.3);
    }

    #[test]
    fn axes_are_clamped() {
        let mut pads = Gamepads::default();
        let id = GamepadId(3);
        pads.connect(id, "pad");
        pads.set_axis(id, GamepadAxis::LeftTrigger, -0.5);
        pads.set_axis(id, GamepadAxis::RightStickY, 2.0);
        pads.set_axis(id, GamepadAxis::RightStickX, f32::NAN);
        let pad = pads.get(id).unwrap();
        assert_eq!(pad.axis(GamepadAxis::LeftTrigger), 0.0);
        assert_eq!(pad.axis(GamepadAxis::RightStickY), 1.0);
        assert_eq!(pad.axis(GamepadAxis::RightStickX), 0.0);
    }

    #[test]
    fn events_for_unknown_pads_are_ignored() {
        let mut pads = Gamepads::default();
        pads.press(GamepadId(9), GamepadButton::Start);
        pads.set_axis(GamepadId(9), GamepadAxis::LeftStickX, 1.0);
        assert!(pads.is_empty());
    }
}
