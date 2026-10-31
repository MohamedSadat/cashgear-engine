//! Named actions: [`ActionMap`] binds keys, mouse buttons, gamepad buttons and axes and
//! touch regions to names; [`ActionState`] says which actions are active.

use super::{ButtonInput, GamepadAxis, GamepadButton, Gamepads, Pointers, VirtualInput};
use cg_core::ecs::prelude::*;
use cg_core::glam::Vec2;
use core::fmt;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashMap};
use winit::event::MouseButton;
use winit::keyboard::KeyCode;

/// One input that can trigger an action.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub enum Binding {
    /// A keyboard key, by physical position (`KeyW` is the same key on QWERTY and AZERTY).
    Key(KeyCode),
    /// A mouse button.
    Mouse(MouseButton),
    /// A button on any gamepad.
    Gamepad(GamepadButton),
    /// A gamepad axis pushed in one direction past [`ActionMap::press_threshold`].
    Axis(GamepadAxis, AxisDirection),
    /// A finger touching a region of the window. Only touch pointers count; the mouse
    /// does not.
    Touch(ScreenRegion),
}

impl Binding {
    /// The first key or gamepad button pressed in this frame, for "press a key" prompts
    /// that rebind an action. Keys win over buttons; with several keys pressed in one
    /// frame, any one of them.
    ///
    /// ```
    /// use cg_platform::input::{Binding, ButtonInput, Gamepads, KeyCode};
    ///
    /// let mut keys = ButtonInput::default();
    /// keys.press(KeyCode::KeyK);
    /// let pressed = Binding::just_pressed(&keys, &Gamepads::default());
    /// assert_eq!(pressed, Some(Binding::Key(KeyCode::KeyK)));
    /// assert_eq!(pressed.unwrap().to_string(), "K");
    /// ```
    pub fn just_pressed(keys: &ButtonInput<KeyCode>, gamepads: &Gamepads) -> Option<Self> {
        if let Some(key) = keys.get_just_pressed().next() {
            return Some(Self::Key(*key));
        }
        gamepads
            .iter()
            .find_map(|(_, pad)| pad.buttons().get_just_pressed().next().copied())
            .map(Self::Gamepad)
    }

    /// Whether this is a keyboard key.
    pub fn is_key(&self) -> bool {
        matches!(self, Self::Key(_))
    }

    /// Whether this is a gamepad button or axis.
    pub fn is_gamepad(&self) -> bool {
        matches!(self, Self::Gamepad(_) | Self::Axis(..))
    }
}

/// A short name for menus and prompts: `Space`, `K`, `Left`, `Pad A`, `Stick Left`.
/// Gamepad buttons are named as on an Xbox pad.
impl fmt::Display for Binding {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match *self {
            Self::Key(key) => f.write_str(&key_name(key)),
            Self::Mouse(button) => match button {
                MouseButton::Left => f.write_str("Mouse Left"),
                MouseButton::Right => f.write_str("Mouse Right"),
                MouseButton::Middle => f.write_str("Mouse Middle"),
                MouseButton::Back => f.write_str("Mouse Back"),
                MouseButton::Forward => f.write_str("Mouse Forward"),
                MouseButton::Other(n) => write!(f, "Mouse {n}"),
            },
            Self::Gamepad(button) => f.write_str(gamepad_button_name(button)),
            Self::Axis(axis, direction) => {
                let positive = direction == AxisDirection::Positive;
                f.write_str(match (axis, positive) {
                    (GamepadAxis::LeftStickX, false) => "Stick Left",
                    (GamepadAxis::LeftStickX, true) => "Stick Right",
                    (GamepadAxis::LeftStickY, false) => "Stick Down",
                    (GamepadAxis::LeftStickY, true) => "Stick Up",
                    (GamepadAxis::RightStickX, false) => "R-Stick Left",
                    (GamepadAxis::RightStickX, true) => "R-Stick Right",
                    (GamepadAxis::RightStickY, false) => "R-Stick Down",
                    (GamepadAxis::RightStickY, true) => "R-Stick Up",
                    (GamepadAxis::LeftTrigger, _) => "LT",
                    (GamepadAxis::RightTrigger, _) => "RT",
                })
            }
            Self::Touch(_) => f.write_str("Touch"),
        }
    }
}

fn gamepad_button_name(button: GamepadButton) -> &'static str {
    match button {
        GamepadButton::South => "Pad A",
        GamepadButton::East => "Pad B",
        GamepadButton::West => "Pad X",
        GamepadButton::North => "Pad Y",
        GamepadButton::LeftBumper => "LB",
        GamepadButton::RightBumper => "RB",
        GamepadButton::LeftTrigger => "LT",
        GamepadButton::RightTrigger => "RT",
        GamepadButton::Select => "Back",
        GamepadButton::Start => "Start",
        GamepadButton::Mode => "Guide",
        GamepadButton::LeftStick => "L3",
        GamepadButton::RightStick => "R3",
        GamepadButton::DPadUp => "D-pad Up",
        GamepadButton::DPadDown => "D-pad Down",
        GamepadButton::DPadLeft => "D-pad Left",
        GamepadButton::DPadRight => "D-pad Right",
    }
}

/// A key's name as printed on a US keyboard, or winit's name for keys without a short one.
fn key_name(key: KeyCode) -> String {
    let named = match key {
        KeyCode::ArrowLeft => "Left",
        KeyCode::ArrowRight => "Right",
        KeyCode::ArrowUp => "Up",
        KeyCode::ArrowDown => "Down",
        KeyCode::Space => "Space",
        KeyCode::Enter => "Enter",
        KeyCode::NumpadEnter => "Num Enter",
        KeyCode::Escape => "Esc",
        KeyCode::Backspace => "Backspace",
        KeyCode::Tab => "Tab",
        KeyCode::ShiftLeft => "L-Shift",
        KeyCode::ShiftRight => "R-Shift",
        KeyCode::ControlLeft => "L-Ctrl",
        KeyCode::ControlRight => "R-Ctrl",
        KeyCode::AltLeft => "L-Alt",
        KeyCode::AltRight => "R-Alt",
        KeyCode::Comma => ",",
        KeyCode::Period => ".",
        KeyCode::Slash => "/",
        KeyCode::Backslash => "\\",
        KeyCode::Semicolon => ";",
        KeyCode::Quote => "'",
        KeyCode::BracketLeft => "[",
        KeyCode::BracketRight => "]",
        KeyCode::Minus => "-",
        KeyCode::Equal => "=",
        KeyCode::Backquote => "`",
        _ => "",
    };
    if !named.is_empty() {
        return named.to_owned();
    }
    let debug = format!("{key:?}");
    for prefix in ["Key", "Digit"] {
        if let Some(rest) = debug.strip_prefix(prefix)
            && rest.chars().count() == 1
        {
            return rest.to_owned();
        }
    }
    match debug.strip_prefix("Numpad") {
        Some(rest) => format!("Num {rest}"),
        None => debug,
    }
}

impl From<KeyCode> for Binding {
    fn from(key: KeyCode) -> Self {
        Self::Key(key)
    }
}

impl From<MouseButton> for Binding {
    fn from(button: MouseButton) -> Self {
        Self::Mouse(button)
    }
}

impl From<GamepadButton> for Binding {
    fn from(button: GamepadButton) -> Self {
        Self::Gamepad(button)
    }
}

impl From<ScreenRegion> for Binding {
    fn from(region: ScreenRegion) -> Self {
        Self::Touch(region)
    }
}

/// Which way an axis must be pushed for an [`Binding::Axis`] binding.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum AxisDirection {
    /// Right or up on a stick; pressed on a trigger.
    Positive,
    /// Left or down on a stick.
    Negative,
}

/// A rectangle of the window in normalized coordinates: `(0, 0)` is the top-left corner
/// and `(1, 1)` the bottom-right, whatever the window size.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct ScreenRegion {
    /// Top-left corner.
    pub min: Vec2,
    /// Bottom-right corner.
    pub max: Vec2,
}

impl ScreenRegion {
    /// The whole window.
    pub const FULL: Self = Self::new(0.0, 0.0, 1.0, 1.0);

    /// A region from its left, top, right and bottom edges.
    pub const fn new(left: f32, top: f32, right: f32, bottom: f32) -> Self {
        Self {
            min: Vec2::new(left, top),
            max: Vec2::new(right, bottom),
        }
    }

    /// Whether a normalized point lies inside the region (edges included).
    pub fn contains(&self, point: Vec2) -> bool {
        point.cmpge(self.min).all() && point.cmple(self.max).all()
    }
}

fn default_deadzone() -> f32 {
    0.15
}

fn default_press_threshold() -> f32 {
    0.5
}

/// Binds action names to inputs. Insert it as a resource; the input plugin reads it every
/// frame to update [`ActionState`].
///
/// ```
/// use cg_platform::input::{ActionMap, GamepadButton, KeyCode, ScreenRegion};
///
/// let actions = ActionMap::new()
///     .bind("jump", KeyCode::Space)
///     .bind("jump", GamepadButton::South)
///     .bind("jump", ScreenRegion::new(0.5, 0.0, 1.0, 1.0));
/// assert_eq!(actions.bindings("jump").len(), 3);
/// ```
///
/// The map is plain data and serializes with `serde`, so it can be loaded from a RON file
/// once the asset system exists:
///
/// ```ron
/// (
///     actions: {
///         "jump": [Key(Space), Gamepad(South), Touch((min: (0.5, 0.0), max: (1.0, 1.0)))],
///         "left": [Key(ArrowLeft), Axis(LeftStickX, Negative)],
///     },
/// )
/// ```
#[derive(Resource, Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ActionMap {
    /// Axis values below this magnitude count as zero.
    #[serde(default = "default_deadzone")]
    pub deadzone: f32,
    /// Axis magnitude at which an [`Binding::Axis`] binding counts as pressed.
    #[serde(default = "default_press_threshold")]
    pub press_threshold: f32,
    /// Bindings for each action name.
    #[serde(default)]
    pub actions: BTreeMap<String, Vec<Binding>>,
}

impl Default for ActionMap {
    fn default() -> Self {
        Self {
            deadzone: default_deadzone(),
            press_threshold: default_press_threshold(),
            actions: BTreeMap::new(),
        }
    }
}

impl ActionMap {
    /// An empty map with the default deadzone (0.15) and press threshold (0.5).
    pub fn new() -> Self {
        Self::default()
    }

    /// Adds a binding to an action and returns the map, for building it in one expression.
    pub fn bind(mut self, action: impl Into<String>, binding: impl Into<Binding>) -> Self {
        self.insert(action, binding);
        self
    }

    /// Adds a binding to an action.
    pub fn insert(&mut self, action: impl Into<String>, binding: impl Into<Binding>) {
        self.actions
            .entry(action.into())
            .or_default()
            .push(binding.into());
    }

    /// Removes an action and all its bindings.
    pub fn remove(&mut self, action: &str) {
        self.actions.remove(action);
    }

    /// The bindings of an action; empty if the action is unknown.
    pub fn bindings(&self, action: &str) -> &[Binding] {
        self.actions.get(action).map_or(&[], Vec::as_slice)
    }

    /// All action names, in sorted order.
    pub fn actions(&self) -> impl Iterator<Item = &str> {
        self.actions.keys().map(String::as_str)
    }
}

/// The raw input sources an action map is evaluated against.
pub(crate) struct Inputs<'a> {
    pub keys: &'a ButtonInput<KeyCode>,
    pub mouse: &'a ButtonInput<MouseButton>,
    pub gamepads: &'a Gamepads,
    pub pointers: &'a Pointers,
    pub virtual_input: &'a VirtualInput,
}

impl Inputs<'_> {
    /// Whether a binding is active this frame (held, or tapped within the frame) and its
    /// strength from `0.0` to `1.0`.
    fn evaluate(&self, binding: &Binding, map: &ActionMap) -> (bool, f32) {
        let button = |active: bool| (active, if active { 1.0 } else { 0.0 });
        match *binding {
            Binding::Key(key) => button(self.keys.pressed(key) || self.keys.just_pressed(key)),
            Binding::Mouse(b) => button(self.mouse.pressed(b) || self.mouse.just_pressed(b)),
            Binding::Gamepad(b) => {
                button(self.gamepads.any_pressed(b) || self.gamepads.any_just_pressed(b))
            }
            Binding::Axis(axis, direction) => {
                let value = self.gamepads.axis(axis);
                let value = match direction {
                    AxisDirection::Positive => value.max(0.0),
                    AxisDirection::Negative => (-value).max(0.0),
                };
                let value = if value < map.deadzone { 0.0 } else { value };
                (value > 0.0 && value >= map.press_threshold, value)
            }
            Binding::Touch(region) => button(self.pointers.iter().any(|p| {
                p.is_touch() && (p.pressed() || p.just_pressed()) && region.contains(p.normalized())
            })),
        }
    }
}

#[derive(Debug, Clone, Copy, Default)]
struct ActionData {
    pressed: bool,
    just_pressed: bool,
    just_released: bool,
    value: f32,
    fixed_just_pressed: bool,
    fixed_just_released: bool,
}

/// Which actions of the [`ActionMap`] are active, updated every frame in
/// [`PreUpdate`](cg_core::PreUpdate).
///
/// Use it in both [`Update`](cg_core::Update) and [`FixedUpdate`](cg_core::FixedUpdate):
/// inside a fixed step, [`just_pressed`](Self::just_pressed) and
/// [`just_released`](Self::just_released) report changes since the previous fixed step, so a
/// jump press is seen exactly once even when a frame runs zero or several fixed steps.
///
/// Unknown action names are never active.
#[derive(Resource, Debug, Clone, Default)]
pub struct ActionState {
    actions: HashMap<String, ActionData>,
    in_fixed_step: bool,
}

impl ActionState {
    /// Whether any binding of the action is held.
    pub fn pressed(&self, action: &str) -> bool {
        self.actions.get(action).is_some_and(|a| a.pressed)
    }

    /// Whether the action became active this frame, or since the previous fixed step when
    /// called from [`FixedUpdate`](cg_core::FixedUpdate).
    pub fn just_pressed(&self, action: &str) -> bool {
        self.actions.get(action).is_some_and(|a| {
            if self.in_fixed_step {
                a.fixed_just_pressed
            } else {
                a.just_pressed
            }
        })
    }

    /// Whether the action stopped being active this frame, or since the previous fixed step
    /// when called from [`FixedUpdate`](cg_core::FixedUpdate).
    pub fn just_released(&self, action: &str) -> bool {
        self.actions.get(action).is_some_and(|a| {
            if self.in_fixed_step {
                a.fixed_just_released
            } else {
                a.just_released
            }
        })
    }

    /// Strength from `0.0` to `1.0`: `1.0` for a held button, the deflection for an axis.
    pub fn value(&self, action: &str) -> f32 {
        self.actions.get(action).map_or(0.0, |a| a.value)
    }

    /// `value(positive) - value(negative)`, from `-1.0` to `1.0`. Handy for movement:
    /// `actions.axis("left", "right")`.
    pub fn axis(&self, negative: &str, positive: &str) -> f32 {
        self.value(positive) - self.value(negative)
    }

    /// Active action names, in no particular order.
    pub fn get_pressed(&self) -> impl Iterator<Item = &str> {
        self.actions
            .iter()
            .filter(|(_, a)| a.pressed)
            .map(|(name, _)| name.as_str())
    }

    /// Action names that became active this frame, in no particular order.
    pub fn get_just_pressed(&self) -> impl Iterator<Item = &str> {
        self.actions
            .iter()
            .filter(|(_, a)| a.just_pressed)
            .map(|(name, _)| name.as_str())
    }

    /// Action names that stopped being active this frame, in no particular order.
    pub fn get_just_released(&self) -> impl Iterator<Item = &str> {
        self.actions
            .iter()
            .filter(|(_, a)| a.just_released)
            .map(|(name, _)| name.as_str())
    }

    /// Evaluates every action of the map, and every action pressed through
    /// [`VirtualInput`], against this frame's input.
    pub(crate) fn update(&mut self, map: &ActionMap, inputs: &Inputs) {
        // Actions that are neither in the map nor pressed virtually are dropped once they
        // have been released, so their release is still seen.
        self.actions.retain(|name, data| {
            map.actions.contains_key(name.as_str())
                || inputs.virtual_input.active(name)
                || data.pressed
        });
        for name in map.actions().chain(inputs.virtual_input.active_names()) {
            if !self.actions.contains_key(name) {
                self.actions.insert(name.to_owned(), ActionData::default());
            }
        }
        for (name, data) in &mut self.actions {
            let (bound, value) = map
                .bindings(name)
                .iter()
                .map(|b| inputs.evaluate(b, map))
                .fold((false, 0.0f32), |(a, v), (ba, bv)| (a || ba, v.max(bv)));
            let pressed_virtually = inputs.virtual_input.active(name);
            let active = bound || pressed_virtually;
            let value = if pressed_virtually { 1.0 } else { value };
            data.just_pressed = active && !data.pressed;
            data.just_released = !active && data.pressed;
            data.pressed = active;
            data.value = value;
            data.fixed_just_pressed |= data.just_pressed;
            data.fixed_just_released |= data.just_released;
        }
    }

    pub(crate) fn begin_fixed_step(&mut self) {
        self.in_fixed_step = true;
    }

    pub(crate) fn end_fixed_step(&mut self) {
        self.in_fixed_step = false;
        for data in self.actions.values_mut() {
            data.fixed_just_pressed = false;
            data.fixed_just_released = false;
        }
    }
}

#[profiling::function]
pub(crate) fn update_action_state(
    map: Res<ActionMap>,
    keys: Res<ButtonInput<KeyCode>>,
    mouse: Res<ButtonInput<MouseButton>>,
    gamepads: Res<Gamepads>,
    pointers: Res<Pointers>,
    mut virtual_input: ResMut<VirtualInput>,
    mut state: ResMut<ActionState>,
) {
    state.update(
        &map,
        &Inputs {
            keys: &keys,
            mouse: &mouse,
            gamepads: &gamepads,
            pointers: &pointers,
            virtual_input: &virtual_input,
        },
    );
    virtual_input.end_update();
}

pub(crate) fn begin_fixed_step(mut state: ResMut<ActionState>) {
    state.begin_fixed_step();
}

pub(crate) fn end_fixed_step(mut state: ResMut<ActionState>) {
    state.end_fixed_step();
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::input::{GamepadId, PointerId};

    #[derive(Default)]
    struct Fixture {
        map: ActionMap,
        keys: ButtonInput<KeyCode>,
        mouse: ButtonInput<MouseButton>,
        gamepads: Gamepads,
        pointers: Pointers,
        virtual_input: VirtualInput,
        state: ActionState,
    }

    impl Fixture {
        fn new(map: ActionMap) -> Self {
            Self {
                map,
                ..Self::default()
            }
        }

        /// Evaluates the map, as `PreUpdate` does.
        fn update(&mut self) {
            let inputs = Inputs {
                keys: &self.keys,
                mouse: &self.mouse,
                gamepads: &self.gamepads,
                pointers: &self.pointers,
                virtual_input: &self.virtual_input,
            };
            self.state.update(&self.map, &inputs);
            self.virtual_input.end_update();
        }

        /// Clears per-frame input, as `Last` does.
        fn end_frame(&mut self) {
            self.keys.clear();
            self.mouse.clear();
            self.gamepads.clear();
            self.pointers.clear();
        }

        /// Runs `steps` fixed steps and returns whether "jump" was just pressed in each.
        fn fixed_steps(&mut self, steps: usize) -> Vec<bool> {
            (0..steps)
                .map(|_| {
                    self.state.begin_fixed_step();
                    let seen = self.state.just_pressed("jump");
                    self.state.end_fixed_step();
                    seen
                })
                .collect()
        }
    }

    fn jump_map() -> ActionMap {
        ActionMap::new()
            .bind("jump", KeyCode::Space)
            .bind("jump", GamepadButton::South)
    }

    #[test]
    fn any_binding_activates_the_action() {
        let mut f = Fixture::new(jump_map());
        let pad = GamepadId(0);
        f.gamepads.connect(pad, "pad");
        f.gamepads.press(pad, GamepadButton::South);
        f.update();
        assert!(f.state.pressed("jump") && f.state.just_pressed("jump"));
        assert_eq!(f.state.value("jump"), 1.0);

        // Pressing a second binding while held is not a new press.
        f.end_frame();
        f.keys.press(KeyCode::Space);
        f.update();
        assert!(f.state.pressed("jump") && !f.state.just_pressed("jump"));

        f.end_frame();
        f.gamepads.release(pad, GamepadButton::South);
        f.keys.release(KeyCode::Space);
        f.update();
        assert!(!f.state.pressed("jump") && f.state.just_released("jump"));
        assert!(!f.state.pressed("unknown"));
    }

    #[test]
    fn taps_inside_one_frame_are_not_lost() {
        let mut f = Fixture::new(jump_map());
        f.keys.press(KeyCode::Space);
        f.keys.release(KeyCode::Space);
        f.update();
        assert!(f.state.just_pressed("jump"));
        f.end_frame();
        f.update();
        assert!(f.state.just_released("jump"));
    }

    #[test]
    fn fixed_steps_see_each_press_exactly_once() {
        let mut f = Fixture::new(jump_map());
        // Pressed in a frame that runs no fixed step...
        f.keys.press(KeyCode::Space);
        f.update();
        assert_eq!(f.fixed_steps(0), Vec::<bool>::new());
        f.end_frame();
        // ...is still seen by the first step of the next frame, and only by that one.
        f.update();
        assert_eq!(f.fixed_steps(3), [true, false, false]);
        f.end_frame();
        f.update();
        assert_eq!(f.fixed_steps(1), [false]);
        // Per-frame queries are unaffected by fixed steps.
        assert!(!f.state.just_pressed("jump") && f.state.pressed("jump"));
    }

    #[test]
    fn axis_bindings_use_deadzone_and_threshold() {
        let map = ActionMap::new()
            .bind(
                "left",
                Binding::Axis(GamepadAxis::LeftStickX, AxisDirection::Negative),
            )
            .bind(
                "right",
                Binding::Axis(GamepadAxis::LeftStickX, AxisDirection::Positive),
            );
        let mut f = Fixture::new(map);
        let pad = GamepadId(0);
        f.gamepads.connect(pad, "pad");

        f.gamepads.set_axis(pad, GamepadAxis::LeftStickX, 0.1);
        f.update();
        assert_eq!(f.state.value("right"), 0.0, "inside the deadzone");

        f.gamepads.set_axis(pad, GamepadAxis::LeftStickX, 0.3);
        f.update();
        assert_eq!(f.state.value("right"), 0.3);
        assert!(!f.state.pressed("right"), "below the press threshold");

        f.gamepads.set_axis(pad, GamepadAxis::LeftStickX, -0.75);
        f.update();
        assert!(f.state.pressed("left") && !f.state.pressed("right"));
        assert_eq!(f.state.axis("left", "right"), -0.75);
    }

    #[test]
    fn touch_regions_ignore_the_mouse() {
        let map = ActionMap::new().bind("jump", ScreenRegion::new(0.5, 0.0, 1.0, 1.0));
        let mut f = Fixture::new(map);
        let window = Vec2::new(100.0, 100.0);

        f.pointers
            .move_to(PointerId::Mouse, Vec2::new(90.0, 50.0), window);
        f.pointers.press(PointerId::Mouse);
        f.update();
        assert!(!f.state.pressed("jump"));

        let left = PointerId::Touch(1);
        f.pointers.move_to(left, Vec2::new(10.0, 50.0), window);
        f.pointers.press(left);
        f.update();
        assert!(!f.state.pressed("jump"));

        let right = PointerId::Touch(2);
        f.pointers.move_to(right, Vec2::new(90.0, 50.0), window);
        f.pointers.press(right);
        f.update();
        assert!(f.state.just_pressed("jump"));
    }

    #[test]
    fn removed_actions_disappear() {
        let mut f = Fixture::new(jump_map());
        f.keys.press(KeyCode::Space);
        f.update();
        f.map.remove("jump");
        f.update();
        assert!(!f.state.pressed("jump"));
        assert_eq!(f.state.get_pressed().count(), 0);
    }

    #[test]
    fn action_map_round_trips_through_ron() {
        let map = ActionMap::new()
            .bind("jump", KeyCode::Space)
            .bind("jump", GamepadButton::South)
            .bind("jump", ScreenRegion::new(0.5, 0.0, 1.0, 1.0))
            .bind("fire", MouseButton::Left)
            .bind(
                "left",
                Binding::Axis(GamepadAxis::LeftStickX, AxisDirection::Negative),
            );
        let text = ron::to_string(&map).unwrap();
        let back: ActionMap = ron::from_str(&text).unwrap();
        assert_eq!(back, map);
    }

    #[test]
    fn virtual_presses_count_like_bindings() {
        let mut f = Fixture::new(jump_map());
        f.virtual_input.press("jump");
        f.update();
        assert!(f.state.just_pressed("jump"));
        assert_eq!(f.state.value("jump"), 1.0);
        f.end_frame();
        f.update();
        assert!(f.state.pressed("jump") && !f.state.just_pressed("jump"));
        f.virtual_input.release("jump");
        f.update();
        assert!(f.state.just_released("jump"));
    }

    #[test]
    fn virtual_actions_need_no_binding_and_taps_last_one_update() {
        let mut f = Fixture::new(ActionMap::new());
        f.virtual_input.tap("pause");
        f.update();
        assert!(f.state.just_pressed("pause"));
        f.update();
        assert!(f.state.just_released("pause"));
        f.update();
        assert!(!f.state.pressed("pause") && !f.state.just_released("pause"));
        assert_eq!(f.state.get_pressed().count(), 0);
    }

    #[test]
    fn action_map_reads_the_documented_ron() {
        let text = r#"(
            actions: {
                "jump": [Key(Space), Gamepad(South), Touch((min: (0.5, 0.0), max: (1.0, 1.0)))],
                "left": [Key(ArrowLeft), Axis(LeftStickX, Negative)],
            },
        )"#;
        let map: ActionMap = ron::from_str(text).unwrap();
        assert_eq!(map.deadzone, 0.15);
        assert_eq!(map.press_threshold, 0.5);
        assert_eq!(map.bindings("jump").len(), 3);
        assert_eq!(
            map.bindings("left")[1],
            Binding::Axis(GamepadAxis::LeftStickX, AxisDirection::Negative)
        );
    }

    #[test]
    fn bindings_have_short_names() {
        let names: Vec<String> = [
            Binding::Key(KeyCode::KeyW),
            Binding::Key(KeyCode::Digit7),
            Binding::Key(KeyCode::ArrowLeft),
            Binding::Key(KeyCode::ShiftLeft),
            Binding::Key(KeyCode::Numpad4),
            Binding::Key(KeyCode::F5),
            Binding::Key(KeyCode::Backslash),
            Binding::Mouse(MouseButton::Left),
            Binding::Gamepad(GamepadButton::South),
            Binding::Gamepad(GamepadButton::DPadUp),
            Binding::Axis(GamepadAxis::LeftStickX, AxisDirection::Negative),
            Binding::Touch(ScreenRegion::FULL),
        ]
        .iter()
        .map(ToString::to_string)
        .collect();
        assert_eq!(
            names,
            [
                "W",
                "7",
                "Left",
                "L-Shift",
                "Num 4",
                "F5",
                "\\",
                "Mouse Left",
                "Pad A",
                "D-pad Up",
                "Stick Left",
                "Touch"
            ]
        );
    }

    #[test]
    fn the_first_pressed_key_or_button_is_captured() {
        let mut keys = ButtonInput::default();
        let mut pads = Gamepads::default();
        assert_eq!(Binding::just_pressed(&keys, &pads), None);
        pads.connect(GamepadId(0), "pad");
        pads.press(GamepadId(0), GamepadButton::West);
        assert_eq!(
            Binding::just_pressed(&keys, &pads),
            Some(Binding::Gamepad(GamepadButton::West))
        );
        keys.press(KeyCode::KeyJ);
        assert_eq!(
            Binding::just_pressed(&keys, &pads),
            Some(Binding::Key(KeyCode::KeyJ))
        );
        assert!(Binding::Key(KeyCode::KeyJ).is_key());
        assert!(Binding::Gamepad(GamepadButton::West).is_gamepad());
    }
}
