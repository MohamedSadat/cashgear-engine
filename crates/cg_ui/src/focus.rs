//! Keyboard and gamepad navigation: [`UiFocus`], the UI actions and moving the focus to
//! the nearest widget in the pressed direction.

use crate::layout::UiStack;
use crate::node::ComputedNode;
use crate::widgets::{Button, Clicked, Focusable, Slider, SliderChanged};
use bevy_ecs::prelude::*;
use cg_core::Time;
use cg_core::glam::Vec2;
use cg_platform::input::{
    ActionMap, ActionState, AxisDirection, Binding, GamepadAxis, GamepadButton, KeyCode,
};

/// The UI's actions, which [`UiPlugin`](crate::UiPlugin) binds by default (unless the
/// game's [`ActionMap`] already has them): arrows and WASD, the d-pad and left stick
/// move the focus; Enter, Space and the gamepad's south button press; Escape, the east
/// button and Android's back button ([`KeyCode::BrowserBack`]) go back.
pub mod actions {
    /// Moves the focus up.
    pub const UP: &str = "ui_up";
    /// Moves the focus down.
    pub const DOWN: &str = "ui_down";
    /// Moves the focus left, or lowers a focused slider (raises it right to left).
    pub const LEFT: &str = "ui_left";
    /// Moves the focus right, or raises a focused slider (lowers it right to left).
    pub const RIGHT: &str = "ui_right";
    /// Clicks the focused button.
    pub const ACCEPT: &str = "ui_accept";
    /// Back out of a menu. The UI does nothing with it itself; menus read it.
    pub const BACK: &str = "ui_back";
}

/// Adds the default bindings of the UI actions that the map lacks.
pub(crate) fn bind_ui_actions(map: &mut ActionMap) {
    use AxisDirection::{Negative, Positive};
    let defaults: [(&str, Vec<Binding>); 6] = [
        (
            actions::UP,
            vec![
                KeyCode::ArrowUp.into(),
                KeyCode::KeyW.into(),
                GamepadButton::DPadUp.into(),
                Binding::Axis(GamepadAxis::LeftStickY, Positive),
            ],
        ),
        (
            actions::DOWN,
            vec![
                KeyCode::ArrowDown.into(),
                KeyCode::KeyS.into(),
                GamepadButton::DPadDown.into(),
                Binding::Axis(GamepadAxis::LeftStickY, Negative),
            ],
        ),
        (
            actions::LEFT,
            vec![
                KeyCode::ArrowLeft.into(),
                KeyCode::KeyA.into(),
                GamepadButton::DPadLeft.into(),
                Binding::Axis(GamepadAxis::LeftStickX, Negative),
            ],
        ),
        (
            actions::RIGHT,
            vec![
                KeyCode::ArrowRight.into(),
                KeyCode::KeyD.into(),
                GamepadButton::DPadRight.into(),
                Binding::Axis(GamepadAxis::LeftStickX, Positive),
            ],
        ),
        (
            actions::ACCEPT,
            vec![
                KeyCode::Enter.into(),
                KeyCode::NumpadEnter.into(),
                KeyCode::Space.into(),
                GamepadButton::South.into(),
            ],
        ),
        (
            actions::BACK,
            vec![
                KeyCode::Escape.into(),
                KeyCode::BrowserBack.into(),
                GamepadButton::East.into(),
            ],
        ),
    ];
    for (action, bindings) in defaults {
        if map.bindings(action).is_empty() {
            for binding in bindings {
                map.insert(action, binding);
            }
        }
    }
}

/// Binds the UI actions whenever the game replaces its action map.
pub(crate) fn keep_ui_bindings(mut map: ResMut<ActionMap>) {
    let missing = [
        actions::UP,
        actions::DOWN,
        actions::LEFT,
        actions::RIGHT,
        actions::ACCEPT,
        actions::BACK,
    ]
    .iter()
    .any(|a| map.bindings(a).is_empty());
    if map.is_changed() && missing {
        bind_ui_actions(&mut map);
    }
}

/// Which widget has the keyboard and gamepad focus.
///
/// The UI actions move it between [`Focusable`] nodes in the topmost menu (the UI tree
/// drawn on top that has any), and `ui_accept` clicks it. The mouse moves it too, by
/// hovering. A focused widget shows as hovered.
#[derive(Resource, Debug, Clone, Default, PartialEq, Eq)]
pub struct UiFocus {
    pub(crate) focused: Option<Entity>,
    pub(crate) from_keys: bool,
    pub(crate) paused: bool,
}

impl UiFocus {
    /// The focused node.
    pub fn get(&self) -> Option<Entity> {
        self.focused
    }

    /// Focuses a node (or nothing), as if moved there with the keys.
    pub fn set(&mut self, entity: Option<Entity>) {
        self.focused = entity;
        self.from_keys = entity.is_some();
    }

    /// Whether the focus last moved by keyboard or gamepad (rather than the mouse).
    pub fn from_keys(&self) -> bool {
        self.from_keys
    }

    /// Stops (or restarts) keyboard and gamepad navigation: while paused, the UI actions
    /// neither move the focus nor click. For prompts that wait for any key, such as
    /// rebinding a control. The pointer still works.
    pub fn set_paused(&mut self, paused: bool) {
        self.paused = paused;
    }

    /// Whether navigation is paused.
    pub fn is_paused(&self) -> bool {
        self.paused
    }

    pub(crate) fn set_from_pointer(&mut self, entity: Entity) {
        self.focused = Some(entity);
        self.from_keys = false;
    }
}

/// Held navigation keys repeat after this long...
const REPEAT_DELAY: f32 = 0.4;
/// ...this often.
const REPEAT_EVERY: f32 = 0.11;

#[derive(Default)]
pub(crate) struct Repeat {
    action: Option<&'static str>,
    held: f32,
    next: f32,
}

type FocusItem = (
    Entity,
    &'static ComputedNode,
    &'static Focusable,
    Option<&'static Button>,
);

/// Moves the focus with the UI actions and clicks or adjusts the focused widget. Runs in
/// `PreUpdate` after the pointer.
#[allow(clippy::too_many_arguments)]
pub(crate) fn navigate(
    action_state: Res<ActionState>,
    time: Res<Time>,
    stack: Res<UiStack>,
    candidates: Query<FocusItem>,
    parents: Query<&ChildOf>,
    mut sliders: Query<&mut Slider>,
    mut focus: ResMut<UiFocus>,
    mut repeat: Local<Repeat>,
    mut clicks: MessageWriter<Clicked>,
    mut changes: MessageWriter<SliderChanged>,
) {
    let root_of = |mut entity: Entity| {
        while let Ok(parent) = parents.get(entity) {
            entity = parent.parent();
        }
        entity
    };
    // The focusable widgets of the topmost menu, in draw order.
    let visible: Vec<_> = stack
        .entities
        .iter()
        .filter_map(|&e| candidates.get(e).ok())
        .filter(|(.., button)| !button.is_some_and(|b| b.disabled))
        .collect();
    let Some(&(top, ..)) = visible.last() else {
        focus.focused = None;
        return;
    };
    let scope = root_of(top);
    let menu: Vec<_> = visible
        .into_iter()
        .filter(|(e, ..)| root_of(*e) == scope)
        .collect();
    let first = || {
        menu.iter()
            .find(|(_, _, f, _)| f.auto_focus)
            .or_else(|| {
                menu.iter().min_by(|a, b| {
                    let (a, b) = (a.1.rect.min, b.1.rect.min);
                    (a.y, a.x)
                        .partial_cmp(&(b.y, b.x))
                        .unwrap_or(core::cmp::Ordering::Equal)
                })
            })
            .map(|(e, ..)| *e)
    };
    if focus
        .focused
        .is_some_and(|f| !menu.iter().any(|(e, ..)| *e == f))
    {
        // The focused widget is gone, hidden or under a newer menu.
        focus.focused = None;
    }
    if focus.focused.is_none() && focus.from_keys {
        // Someone on keys or a gamepad: a menu that opens has its first widget focused,
        // ready for `ui_accept`.
        focus.focused = first();
    }

    // Which direction is pressed, with key repeat on real time.
    let directions = [
        (actions::UP, Vec2::NEG_Y),
        (actions::DOWN, Vec2::Y),
        (actions::LEFT, Vec2::NEG_X),
        (actions::RIGHT, Vec2::X),
    ];
    let dt = time.raw_delta_secs();
    let mut step = None;
    for (action, direction) in directions {
        if action_state.just_pressed(action) {
            *repeat = Repeat {
                action: Some(action),
                held: 0.0,
                next: REPEAT_DELAY,
            };
            step = Some((action, direction));
        }
    }
    if step.is_none()
        && let Some(action) = repeat.action
    {
        if action_state.pressed(action) {
            repeat.held += dt;
            if repeat.held >= repeat.next {
                repeat.next += REPEAT_EVERY;
                step = directions.into_iter().find(|(a, _)| *a == action);
            }
        } else {
            repeat.action = None;
        }
    }

    if focus.paused {
        return;
    }
    if let Some((action, direction)) = step {
        let current = focus
            .focused
            .and_then(|f| menu.iter().find(|(e, ..)| *e == f));
        match current {
            None => {
                focus.focused = first();
                focus.from_keys = focus.focused.is_some();
            }
            Some(&(entity, computed, ..)) => {
                let horizontal = action == actions::LEFT || action == actions::RIGHT;
                if horizontal && let Ok(mut slider) = sliders.get_mut(entity) {
                    let mut steps = if action == actions::RIGHT { 1.0 } else { -1.0 };
                    if computed.rtl {
                        steps = -steps;
                    }
                    let before = slider.value;
                    slider.nudge(steps);
                    if slider.value != before {
                        changes.write(SliderChanged {
                            entity,
                            value: slider.value,
                        });
                    }
                } else {
                    let from = computed.rect.center();
                    let targets = menu.iter().map(|(e, c, ..)| (*e, c.rect.center()));
                    focus.focused = nearest(entity, from, direction, targets).or(Some(entity));
                }
                focus.from_keys = true;
            }
        }
    }

    if action_state.just_pressed(actions::ACCEPT)
        && let Some(entity) = focus.focused
        && candidates
            .get(entity)
            .is_ok_and(|(.., button)| button.is_some())
    {
        clicks.write(Clicked { entity });
        focus.from_keys = true;
    }
}

/// The best target from `from` in `direction`: close along it and near its line. With
/// nothing that way, the focus wraps to the farthest target the other way.
pub(crate) fn nearest(
    current: Entity,
    from: Vec2,
    direction: Vec2,
    targets: impl Iterator<Item = (Entity, Vec2)>,
) -> Option<Entity> {
    let side = direction.perp();
    let mut best: Option<(f32, Entity)> = None;
    let mut wrap: Option<(f32, Entity)> = None;
    for (entity, at) in targets {
        if entity == current {
            continue;
        }
        let delta = at - from;
        let along = delta.dot(direction);
        let across = delta.dot(side).abs();
        if along > 0.5 {
            let score = along + across * 2.0;
            if best.is_none_or(|(s, _)| score < s) {
                best = Some((score, entity));
            }
        } else if along < -0.5 && across < 0.5 + (-along) * 0.25 {
            // Roughly in line the other way: a candidate to wrap round to.
            let score = along - across * 2.0;
            if wrap.is_none_or(|(s, _)| score < s) {
                wrap = Some((score, entity));
            }
        }
    }
    best.or(wrap).map(|(_, e)| e)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_nearest_target_is_along_the_direction_and_wraps() {
        let e = |i| Entity::from_raw_u32(i).unwrap();
        // A column of three buttons and one off to the right.
        let targets = [
            (e(1), Vec2::new(100.0, 50.0)),
            (e(2), Vec2::new(100.0, 80.0)),
            (e(3), Vec2::new(100.0, 110.0)),
            (e(4), Vec2::new(300.0, 80.0)),
        ];
        let go = |from: u32, dir: Vec2| {
            let at = targets[from as usize - 1].1;
            nearest(e(from), at, dir, targets.iter().copied())
        };
        assert_eq!(go(1, Vec2::Y), Some(e(2)));
        assert_eq!(go(2, Vec2::Y), Some(e(3)));
        // Past the bottom: wrap to the top.
        assert_eq!(go(3, Vec2::Y), Some(e(1)));
        assert_eq!(go(2, Vec2::X), Some(e(4)));
        // Nothing to the left of the column: wrap round to the far right, roughly in line.
        assert_eq!(go(1, Vec2::NEG_X), Some(e(4)));
        // Nothing in line above the top button but itself: no wrap target but the column.
        assert_eq!(go(1, Vec2::NEG_Y), Some(e(3)));
    }
}
