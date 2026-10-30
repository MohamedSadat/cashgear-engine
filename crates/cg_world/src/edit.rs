//! What an editor tells the game it runs: whether the scene is being edited or played, and
//! when Play and Stop are pressed. The editor (`cg_editor`) is a desktop tool, but these
//! types are on every target, so a game reacts to them without any `cfg`.

use bevy_ecs::prelude::*;

/// Present while an editor runs the app (`cg_editor`'s `EditorPlugin` inserts it before the
/// game's plugins are added): whether the scene is being edited or played.
///
/// A game that supports play-in-editor checks for it while it builds, to start in an idle
/// state of its own instead of its title screen, and reacts to [`PlayStarted`] and
/// [`PlayStopped`]:
///
/// ```
/// use cg_core::prelude::*;
/// use cg_world::prelude::*;
/// use cg_world::{EditMode, PlayStarted, PlayStopped};
///
/// #[derive(Clone, PartialEq, Eq, Hash, Debug)]
/// enum Screen { Title, Editor, Playing }
///
/// fn play_in_editor(
///     mut started: MessageReader<PlayStarted>,
///     mut stopped: MessageReader<PlayStopped>,
///     mut next: ResMut<NextState<Screen>>,
/// ) {
///     if started.read().last().is_some() {
///         next.set(Screen::Playing);
///     }
///     if stopped.read().last().is_some() {
///         next.set(Screen::Editor);
///     }
/// }
///
/// let mut app = App::new();
/// app.add_plugins(WorldPlugin);
/// let first = if app.world().contains_resource::<EditMode>() {
///     Screen::Editor
/// } else {
///     Screen::Title
/// };
/// app.init_state(first).add_systems(PreUpdate, play_in_editor);
/// ```
#[derive(Resource, Debug, Clone, Copy, PartialEq, Eq)]
pub enum EditMode {
    /// The scene is being edited: fixed steps are paused and the game should stay idle.
    Editing,
    /// The scene is being played: the game runs as it would on its own.
    Playing,
}

/// Sent when the editor's Play is pressed, after the edited scene is in place.
#[derive(Message, Debug, Clone, PartialEq, Eq)]
pub struct PlayStarted {
    /// The asset path of the map being edited (`maps/level1.tmx`), if a map is open. The
    /// map asset already holds the edits, saved or not.
    pub map: Option<String>,
}

/// Sent when the editor's Stop is pressed. The game goes back to its idle state; in the
/// next frame the editor despawns every entity made since Play and shows the edited scene
/// again.
#[derive(Message, Debug, Clone, Copy, PartialEq, Eq)]
pub struct PlayStopped;

/// A run condition: whether an editor runs the app (see [`EditMode`]).
pub fn in_editor(mode: Option<Res<EditMode>>) -> bool {
    mode.is_some()
}

/// A run condition: whether an editor runs the app and the scene is being edited, not
/// played.
pub fn editing(mode: Option<Res<EditMode>>) -> bool {
    mode.is_some_and(|mode| *mode == EditMode::Editing)
}
