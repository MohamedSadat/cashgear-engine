//! On-screen touch controls: [`TouchButton`]s and [`TouchDpad`]s that press actions
//! through [`VirtualInput`], shown on touch devices.

use crate::Node;
use crate::layout::UiStack;
use crate::node::ComputedNode;
use bevy_ecs::prelude::*;
use cg_core::glam::Vec2;
use cg_platform::Window;
use cg_platform::input::{ButtonInput, Gamepads, KeyCode, PointerId, Pointers, VirtualInput};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};

/// When on-screen touch controls show.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
pub enum TouchMode {
    /// On touch screens from the start, and elsewhere after the first touch; hidden again
    /// by a key or gamepad button. The default.
    #[default]
    Auto,
    /// Always, also to the mouse (handy for trying them on desktop: `--touch`).
    Always,
    /// Never.
    Never,
}

/// Whether the on-screen touch controls are showing, and when they should.
///
/// Nodes with [`TouchButton`], [`TouchDpad`] or [`TouchControl`] (and their children) are
/// drawn and work only while the controls show.
#[derive(Resource, Debug, Clone, PartialEq, Eq)]
pub struct TouchControls {
    /// When they show.
    pub mode: TouchMode,
    visible: bool,
    seen_touch: bool,
}

impl Default for TouchControls {
    fn default() -> Self {
        Self::new(TouchMode::Auto)
    }
}

impl TouchControls {
    /// Controls in a mode.
    pub fn new(mode: TouchMode) -> Self {
        Self {
            mode,
            visible: mode == TouchMode::Always,
            seen_touch: false,
        }
    }

    /// Whether the controls show now.
    pub fn visible(&self) -> bool {
        self.visible
    }
}

/// Marks a node (and its children) as part of the touch controls, shown only while they
/// are. [`TouchButton`] and [`TouchDpad`] add it.
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[require(Node)]
pub struct TouchControl;

/// Holds an action down while a finger is on the node. The touch area reaches `margin` UI
/// pixels past the node's edges, since fingers are not precise.
///
/// In prefabs: `"TouchButton": (action: "jump")`.
#[derive(Component, Debug, Clone, PartialEq, Serialize, Deserialize)]
#[require(Node, TouchControl)]
#[serde(default)]
pub struct TouchButton {
    /// The action it presses.
    pub action: String,
    /// Extra touch area around the node, in UI pixels.
    pub margin: f32,
}

impl Default for TouchButton {
    fn default() -> Self {
        Self {
            action: String::new(),
            margin: 6.0,
        }
    }
}

impl TouchButton {
    /// A button for an action.
    pub fn new(action: impl Into<String>) -> Self {
        Self {
            action: action.into(),
            ..Self::default()
        }
    }
}

/// A directional pad: a finger that lands on the node presses the actions for the
/// direction it is from the centre (two at once on the diagonals), and keeps steering it
/// while it stays down, even past the node's edge.
///
/// In prefabs: `"TouchDpad": (left: "left", right: "right", up: "", down: "crouch")`.
/// An empty action name presses nothing.
#[derive(Component, Debug, Clone, PartialEq, Serialize, Deserialize)]
#[require(Node, TouchControl)]
#[serde(default)]
pub struct TouchDpad {
    /// The action pressed up.
    pub up: String,
    /// The action pressed down.
    pub down: String,
    /// The action pressed left.
    pub left: String,
    /// The action pressed right.
    pub right: String,
    /// Near the centre, as a fraction of the pad's half size, nothing is pressed.
    pub dead_zone: f32,
}

impl Default for TouchDpad {
    fn default() -> Self {
        Self {
            up: "up".into(),
            down: "down".into(),
            left: "left".into(),
            right: "right".into(),
            dead_zone: 0.2,
        }
    }
}

impl TouchDpad {
    /// The actions pressed by a point at `offset` from the centre of a pad of `half` size.
    pub fn pressed(&self, offset: Vec2, half: Vec2) -> Vec<&str> {
        let v = offset / half.max(Vec2::ONE);
        let length = v.length();
        if length < self.dead_zone {
            return Vec::new();
        }
        // Eight sectors: an axis counts within 67.5 degrees of it.
        let cos = 67.5f32.to_radians().cos();
        let d = v / length;
        let mut out = Vec::new();
        for (component, action) in [
            (d.x, &self.right),
            (-d.x, &self.left),
            (-d.y, &self.up),
            (d.y, &self.down),
        ] {
            if component > cos && !action.is_empty() {
                out.push(action.as_str());
            }
        }
        out
    }
}

/// Which pad each finger is steering.
#[derive(Default)]
pub(crate) struct TouchState {
    pads: HashMap<PointerId, Entity>,
    held: HashSet<String>,
}

/// Shows or hides the controls from the kind of input in use, then presses the actions
/// under the fingers. Runs in `PreUpdate` before the actions are evaluated.
#[allow(clippy::too_many_arguments)]
#[profiling::function]
pub(crate) fn update_touch_controls(
    mut controls: ResMut<TouchControls>,
    window: Res<Window>,
    pointers: Res<Pointers>,
    keys: Res<ButtonInput<KeyCode>>,
    gamepads: Res<Gamepads>,
    stack: Res<UiStack>,
    buttons: Query<(&ComputedNode, &TouchButton)>,
    pads: Query<(&ComputedNode, &TouchDpad)>,
    mut input: ResMut<VirtualInput>,
    mut state: Local<TouchState>,
) {
    let touched = pointers.iter().any(|p| p.is_touch() && p.just_pressed());
    let visible = match controls.mode {
        TouchMode::Always => true,
        TouchMode::Never => false,
        TouchMode::Auto => {
            if touched || (window.touch_screen() && !controls.seen_touch) {
                controls.seen_touch |= touched;
                true
            } else if keys.get_just_pressed().len() > 0
                || gamepads
                    .iter()
                    .any(|(_, pad)| pad.buttons().get_just_pressed().len() > 0)
            {
                false
            } else {
                controls.visible
            }
        }
    };
    if controls.visible != visible {
        controls.visible = visible;
    }

    let mut pressed: HashSet<String> = HashSet::new();
    if visible {
        let space = stack.space;
        let fingers: Vec<_> = pointers
            .iter()
            .filter(|p| p.pressed() || p.just_pressed())
            .filter(|p| p.is_touch() || controls.mode == TouchMode::Always)
            .collect();
        state
            .pads
            .retain(|id, _| fingers.iter().any(|p| p.id() == *id));
        for finger in &fingers {
            let point = space.from_screen(finger.position(), &window);
            for (computed, button) in &buttons {
                let area = computed.rect;
                let area = cg_core::Rect::from_corners(
                    area.min - Vec2::splat(button.margin),
                    area.max + Vec2::splat(button.margin),
                );
                if computed.is_visible() && area.contains(point) && !button.action.is_empty() {
                    pressed.insert(button.action.clone());
                }
            }
            // A finger steers the topmost pad it lands on for as long as it stays down.
            if finger.just_pressed()
                && let Some(pad) = stack.entities.iter().rev().copied().find(|&e| {
                    pads.get(e)
                        .is_ok_and(|(c, _)| c.is_visible() && c.rect.contains(point))
                })
            {
                state.pads.insert(finger.id(), pad);
            }
            if let Some(&pad) = state.pads.get(&finger.id())
                && let Ok((computed, dpad)) = pads.get(pad)
            {
                let rect = computed.rect;
                for action in dpad.pressed(point - rect.center(), rect.size() * 0.5) {
                    pressed.insert(action.to_owned());
                }
            }
        }
    } else {
        state.pads.clear();
    }

    for action in state.held.difference(&pressed) {
        input.release(action);
    }
    for action in &pressed {
        input.press(action);
    }
    state.held = pressed;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_dpad_presses_by_direction_with_diagonals() {
        let pad = TouchDpad::default();
        let half = Vec2::splat(20.0);
        assert!(pad.pressed(Vec2::new(1.0, 1.0), half).is_empty());
        assert_eq!(pad.pressed(Vec2::new(15.0, 0.0), half), ["right"]);
        assert_eq!(pad.pressed(Vec2::new(0.0, -15.0), half), ["up"]);
        assert_eq!(pad.pressed(Vec2::new(-12.0, 12.0), half), ["left", "down"]);
        // Far outside the pad still steers.
        assert_eq!(pad.pressed(Vec2::new(-300.0, 20.0), half), ["left"]);
    }
}
