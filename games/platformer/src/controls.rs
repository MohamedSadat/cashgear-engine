//! Rebindable controls: each game action has two keyboard keys and one gamepad button the
//! player can change in Settings, Controls. They are saved, and the action map is built
//! from them plus fixed extras (the left stick). The menus keep the engine's UI bindings.

use cashgear_engine::prelude::*;
use serde::{Deserialize, Serialize};

/// The actions that can be rebound, in the order the controls screen lists them.
pub(crate) const ACTIONS: [&str; 5] = ["left", "right", "jump", "down", "pause"];
/// Slots per action: two keys, then a gamepad button.
pub(crate) const SLOTS: usize = 3;
/// The slot that takes gamepad buttons.
pub(crate) const PAD_SLOT: usize = 2;
const SAVE_KEY: &str = "controls.ron";

pub(crate) fn plugin(app: &mut App) {
    app.init_resource::<Rebinding>()
        .add_systems(Startup, load_controls)
        .add_systems(
            PreUpdate,
            (
                cool_down,
                capture.run_if(|r: Res<Rebinding>| r.target.is_some()),
            )
                .chain()
                .after(InputSystems::Actions),
        );
}

/// The player's bindings: per action, two keys and a gamepad button (any may be empty).
#[derive(Resource, Debug, Clone, PartialEq, Serialize, Deserialize)]
pub(crate) struct Controls {
    pub slots: Vec<(String, [Option<Binding>; SLOTS])>,
}

impl Default for Controls {
    fn default() -> Self {
        use GamepadButton as Pad;
        let row = |action: &str, a: KeyCode, b: KeyCode, pad: Pad| {
            (
                action.to_owned(),
                [Some(a.into()), Some(b.into()), Some(pad.into())],
            )
        };
        Self {
            slots: vec![
                row("left", KeyCode::ArrowLeft, KeyCode::KeyA, Pad::DPadLeft),
                row("right", KeyCode::ArrowRight, KeyCode::KeyD, Pad::DPadRight),
                row("jump", KeyCode::Space, KeyCode::ArrowUp, Pad::South),
                row("down", KeyCode::ArrowDown, KeyCode::KeyS, Pad::DPadDown),
                row("pause", KeyCode::Escape, KeyCode::KeyP, Pad::Start),
            ],
        }
    }
}

impl Controls {
    /// The binding in a slot.
    pub fn get(&self, action: usize, slot: usize) -> Option<Binding> {
        self.slots.get(action).and_then(|(_, slots)| slots[slot])
    }

    /// Puts `binding` in a slot, taking it out of any other slot that had it, so one key
    /// never does two things.
    pub fn set(&mut self, action: usize, slot: usize, binding: Binding) {
        for (_, slots) in &mut self.slots {
            for other in slots.iter_mut() {
                if *other == Some(binding) {
                    *other = None;
                }
            }
        }
        if let Some((_, slots)) = self.slots.get_mut(action) {
            slots[slot] = Some(binding);
        }
    }

    /// The action map: the slots, the left stick for moving, and the menus' defaults
    /// (added by the UI plugin).
    pub fn action_map(&self) -> ActionMap {
        use AxisDirection::{Negative, Positive};
        let mut map = ActionMap::new()
            .bind("left", Binding::Axis(GamepadAxis::LeftStickX, Negative))
            .bind("right", Binding::Axis(GamepadAxis::LeftStickX, Positive))
            .bind("down", Binding::Axis(GamepadAxis::LeftStickY, Negative));
        for (action, slots) in &self.slots {
            for binding in slots.iter().flatten() {
                map.insert(action.as_str(), *binding);
            }
        }
        map
    }

    fn save(&self, storage: &Storage) {
        let text = ron::ser::to_string_pretty(self, ron::ser::PrettyConfig::new().new_line("\n"));
        match text.map(|text| storage.write(SAVE_KEY, &text)) {
            Ok(Ok(())) => {}
            Ok(Err(err)) => warn!("could not save the controls: {err}"),
            Err(err) => warn!("could not write the controls: {err}"),
        }
    }
}

fn load_controls(mut commands: Commands, storage: Res<Storage>) {
    let controls = match storage.read(SAVE_KEY) {
        Ok(Some(text)) => match ron::from_str::<Controls>(&text) {
            Ok(saved) if saved.slots.len() == ACTIONS.len() => saved,
            Ok(_) => Controls::default(),
            Err(err) => {
                warn!("ignoring the saved controls: {err}");
                Controls::default()
            }
        },
        _ => Controls::default(),
    };
    commands.insert_resource(controls.action_map());
    commands.insert_resource(controls);
}

/// Changes the controls, rebuilds the action map and saves them.
pub(crate) fn apply(controls: &Controls, map: &mut ActionMap, storage: &Storage, save: bool) {
    *map = controls.action_map();
    if save {
        controls.save(storage);
    }
}

/// A "press a key" prompt: which slot is waiting for a key or button.
#[derive(Resource, Debug, Default)]
pub(crate) struct Rebinding {
    /// (action, slot) waiting for input.
    pub target: Option<(usize, usize)>,
    /// The frame the prompt opened; the press that opened it does not count.
    armed: bool,
    /// Frames left before the menus take input again after a prompt closed, so the key
    /// that closed it does not also click or go back.
    cooldown: u8,
    /// What the last prompt ended with, for the log and the tests.
    pub last: Option<Binding>,
}

impl Rebinding {
    /// Opens the prompt for a slot. Keyboard and gamepad navigation pause until it closes.
    pub fn start(&mut self, action: usize, slot: usize, focus: &mut UiFocus) {
        self.target = Some((action, slot));
        self.armed = false;
        focus.set_paused(true);
    }

    /// Whether a prompt is open or just closed: the menus ignore their keys then.
    pub fn is_busy(&self) -> bool {
        self.target.is_some() || self.cooldown > 0
    }

    fn finish(&mut self) {
        self.target = None;
        self.cooldown = 2;
    }
}

/// Lets the menus take keys again a little after a prompt closed.
fn cool_down(mut rebinding: ResMut<Rebinding>, mut focus: ResMut<UiFocus>) {
    if rebinding.cooldown > 0 {
        rebinding.cooldown -= 1;
        if rebinding.cooldown == 0 {
            focus.set_paused(false);
        }
    }
}

/// While a prompt is open: Escape or Android's back button cancels it (so neither can be
/// bound by the prompt; "Reset" brings Escape back for pausing); the first other key (for
/// a key slot) or gamepad button (for the pad slot) pressed after it opened is bound.
fn capture(
    keys: Res<ButtonInput<KeyCode>>,
    gamepads: Res<Gamepads>,
    mut rebinding: ResMut<Rebinding>,
    mut controls: ResMut<Controls>,
    mut map: ResMut<ActionMap>,
    storage: Res<Storage>,
    options: Res<crate::Options>,
) {
    let Some((action, slot)) = rebinding.target else {
        return;
    };
    if !rebinding.armed {
        rebinding.armed = true;
        return;
    }
    if keys.any_just_pressed([KeyCode::Escape, KeyCode::BrowserBack]) {
        info!("rebinding cancelled");

        rebinding.last = None;
        rebinding.finish();
        return;
    }
    let Some(binding) = Binding::just_pressed(&keys, &gamepads) else {
        return;
    };
    let fits = if slot == PAD_SLOT {
        binding.is_gamepad()
    } else {
        binding.is_key()
    };
    if !fits {
        return;
    }
    info!("{} now on {binding}", ACTIONS[action]);
    controls.set(action, slot, binding);
    // The demo keeps the player's saved controls as they are.
    apply(&controls, &mut map, &storage, !options.demo);
    rebinding.last = Some(binding);
    rebinding.finish();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_cover_every_action_and_build_the_map() {
        let controls = Controls::default();
        assert_eq!(controls.slots.len(), ACTIONS.len());
        for (i, (action, slots)) in controls.slots.iter().enumerate() {
            assert_eq!(action, ACTIONS[i]);
            assert!(slots[PAD_SLOT].unwrap().is_gamepad());
            assert!(slots[0].unwrap().is_key() && slots[1].unwrap().is_key());
        }
        let map = controls.action_map();
        assert!(map.bindings("jump").contains(&Binding::Key(KeyCode::Space)));
        assert!(
            map.bindings("left").len() == 4,
            "{:?}",
            map.bindings("left")
        );
    }

    #[test]
    fn a_key_moves_to_its_new_action() {
        let mut controls = Controls::default();
        // Space jumps; give it to "down" and jump loses it.
        controls.set(3, 0, KeyCode::Space.into());
        assert_eq!(controls.get(3, 0), Some(KeyCode::Space.into()));
        assert_eq!(controls.get(2, 0), None);
        assert!(
            !controls
                .action_map()
                .bindings("jump")
                .contains(&Binding::Key(KeyCode::Space))
        );
    }
}
