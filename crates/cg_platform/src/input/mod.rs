//! Input state: keyboard, mouse, unified pointer/touch, gamepads and named actions.
//!
//! | Resource | Holds |
//! | --- | --- |
//! | [`ButtonInput<KeyCode>`] | Keys, by physical position |
//! | [`ButtonInput<MouseButton>`] | Mouse buttons |
//! | [`Pointers`] | Mouse cursor and touches as one pointer input, plus scroll |
//! | [`Gamepads`] | Connected gamepads, their buttons and axes |
//! | [`ActionMap`] | Action names and their bindings (set by the game) |
//! | [`ActionState`] | Which actions are active (read by the game) |
//! | [`VirtualInput`] | Actions pressed by code: on-screen controls, menus, demo bots |
//! | [`TextInput`] | The text typed this frame |
//!
//! Game code should prefer actions: they merge keyboard, gamepad and touch, and they work
//! correctly inside [`FixedUpdate`](cg_core::FixedUpdate).

mod action;
mod button;
mod gamepad;
mod pointer;
mod text;
mod virtual_input;

pub use action::{ActionMap, ActionState, AxisDirection, Binding, ScreenRegion};
pub use button::ButtonInput;
pub use gamepad::{Gamepad, GamepadAxis, GamepadButton, GamepadId, Gamepads};
pub use pointer::{Pointer, PointerId, Pointers};
pub use text::TextInput;
pub use virtual_input::VirtualInput;
pub use winit::event::MouseButton;
pub use winit::keyboard::KeyCode;

use cg_core::ecs::prelude::*;
use cg_core::{App, FixedPostUpdate, FixedPreUpdate, Last, Plugin, PreUpdate};

/// System sets of the input systems in [`PreUpdate`], in this order.
#[derive(SystemSet, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum InputSystems {
    /// Polls the gamepads.
    Gamepads,
    /// Evaluates the [`ActionMap`] into the [`ActionState`]. Systems that press actions
    /// through [`VirtualInput`] (on-screen controls) run `.before(InputSystems::Actions)`.
    Actions,
}

/// Adds the input resources and the systems that maintain them.
///
/// The window runner feeds keyboard, mouse and touch events in as they arrive; gamepads are
/// polled at the start of each frame.
#[derive(Debug, Clone, Copy)]
pub struct InputPlugin {
    /// Whether to open the platform gamepad API. Tests and headless tools turn it off.
    pub gamepads: bool,
}

impl Default for InputPlugin {
    fn default() -> Self {
        Self { gamepads: true }
    }
}

impl Plugin for InputPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<ButtonInput<KeyCode>>()
            .init_resource::<ButtonInput<MouseButton>>()
            .init_resource::<Pointers>()
            .init_resource::<Gamepads>()
            .init_resource::<ActionMap>()
            .init_resource::<ActionState>()
            .init_resource::<VirtualInput>()
            .init_resource::<TextInput>()
            .configure_sets(
                PreUpdate,
                (InputSystems::Gamepads, InputSystems::Actions).chain(),
            )
            .add_systems(
                PreUpdate,
                (
                    gamepad::poll_gamepads.in_set(InputSystems::Gamepads),
                    action::update_action_state.in_set(InputSystems::Actions),
                ),
            )
            .add_systems(FixedPreUpdate, action::begin_fixed_step)
            .add_systems(FixedPostUpdate, action::end_fixed_step)
            .add_systems(Last, clear_input);

        if self.gamepads
            && let Some((context, connected)) = gamepad::GilrsContext::new()
        {
            let mut gamepads = app.world_mut().resource_mut::<Gamepads>();
            for (id, name) in connected {
                log::info!("gamepad {} connected: {name}", id.0);
                gamepads.connect(id, name);
            }
            app.world_mut().insert_non_send(context);
        }
    }
}

fn clear_input(
    mut keys: ResMut<ButtonInput<KeyCode>>,
    mut mouse: ResMut<ButtonInput<MouseButton>>,
    mut pointers: ResMut<Pointers>,
    mut gamepads: ResMut<Gamepads>,
    mut text: ResMut<TextInput>,
) {
    text.clear();
    keys.clear();
    mouse.clear();
    pointers.clear();
    gamepads.clear();
}

#[cfg(test)]
mod tests {
    use super::*;
    use cg_core::{FixedUpdate, Time, Update};
    use core::time::Duration;

    #[derive(Resource, Default)]
    struct Jumps {
        fixed: u32,
        frame: u32,
    }

    fn app() -> App {
        let mut app = App::new();
        app.add_plugins(InputPlugin { gamepads: false })
            .insert_resource(ActionMap::new().bind("jump", KeyCode::Space))
            .init_resource::<Jumps>()
            .add_systems(FixedUpdate, |a: Res<ActionState>, mut j: ResMut<Jumps>| {
                j.fixed += u32::from(a.just_pressed("jump"));
            })
            .add_systems(Update, |a: Res<ActionState>, mut j: ResMut<Jumps>| {
                j.frame += u32::from(a.just_pressed("jump"));
            });
        app
    }

    #[test]
    fn a_press_reaches_update_and_fixed_update_once() {
        let mut app = app();
        let step = app.world().resource::<Time>().fixed_delta();
        app.world_mut()
            .resource_mut::<ButtonInput<KeyCode>>()
            .press(KeyCode::Space);
        // A short frame with no fixed step, then a long one with three.
        app.update(Duration::ZERO);
        app.update(step * 3);
        let jumps = app.world().resource::<Jumps>();
        assert_eq!((jumps.frame, jumps.fixed), (1, 1));
        assert!(
            !app.world()
                .resource::<ButtonInput<KeyCode>>()
                .just_pressed(KeyCode::Space)
        );
    }
}
