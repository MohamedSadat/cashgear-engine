//! Game states as a stack: [`State`], [`NextState`], the [`OnEnter`], [`OnExit`],
//! [`OnPause`] and [`OnResume`] schedules, run conditions and [`StateScoped`] entities.

use bevy_ecs::prelude::*;
use bevy_ecs::schedule::ScheduleLabel;
use core::fmt::Debug;
use core::hash::Hash;

/// A type whose values name game states, usually an enum. Every `Clone + Eq + Hash + Debug`
/// type that is `Send + Sync + 'static` qualifies.
///
/// ```
/// #[derive(Clone, PartialEq, Eq, Hash, Debug)]
/// enum GameState {
///     Menu,
///     Playing,
///     Paused,
/// }
/// ```
pub trait States: Clone + Eq + Hash + Debug + Send + Sync + 'static {}

impl<T: Clone + Eq + Hash + Debug + Send + Sync + 'static> States for T {}

/// Runs when a state becomes the top of the stack by being pushed, replacing the top, or
/// being set.
#[derive(ScheduleLabel, Debug, Clone, PartialEq, Eq, Hash)]
pub struct OnEnter<S: States>(pub S);

/// Runs when a state leaves the stack: popped, replaced, or cleared by
/// [`NextState::set`]. Its [`StateScoped`] entities are despawned right after.
#[derive(ScheduleLabel, Debug, Clone, PartialEq, Eq, Hash)]
pub struct OnExit<S: States>(pub S);

/// Runs when another state is pushed on top of this one.
#[derive(ScheduleLabel, Debug, Clone, PartialEq, Eq, Hash)]
pub struct OnPause<S: States>(pub S);

/// Runs when this state is the top of the stack again because the state above it popped.
#[derive(ScheduleLabel, Debug, Clone, PartialEq, Eq, Hash)]
pub struct OnResume<S: States>(pub S);

/// The stack of active states of type `S`. The last one is the current state; the ones
/// below it are paused. Change it through [`NextState`].
///
/// Add it with [`AppWorldExt::init_state`](crate::AppWorldExt::init_state).
#[derive(Resource, Debug, Clone, PartialEq, Eq)]
pub struct State<S: States> {
    stack: Vec<S>,
}

impl<S: States> State<S> {
    /// The current state: the top of the stack.
    pub fn current(&self) -> &S {
        self.stack.last().expect("the state stack is never empty")
    }

    /// The whole stack, bottom first.
    pub fn stack(&self) -> &[S] {
        &self.stack
    }

    /// Whether `state` is anywhere in the stack, current or paused.
    pub fn contains(&self, state: &S) -> bool {
        self.stack.contains(state)
    }
}

/// A requested change to the [`State`] stack.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StateChange<S> {
    /// Pause the current state and enter a new one on top.
    Push(S),
    /// Exit the current state and resume the one below. Ignored, with a warning, when only
    /// one state is left.
    Pop,
    /// Exit the current state and enter another in its place.
    Replace(S),
    /// Exit every state, top first, and enter a new one as the only state.
    Set(S),
}

/// Requested changes to the [`State`] stack. They take effect, in order, in the
/// [`StateTransition`](cg_core::StateTransition) schedule, after
/// [`PreUpdate`](cg_core::PreUpdate) and before the frame's fixed steps; until then
/// [`State`] still shows the old stack. A change requested in the middle of a frame's
/// fixed steps waits for the next frame.
///
/// ```
/// use cg_core::prelude::*;
/// use cg_world::NextState;
///
/// #[derive(Clone, PartialEq, Eq, Hash, Debug)]
/// enum GameState { Menu, Playing, Paused }
///
/// fn pause(mut next: ResMut<NextState<GameState>>) {
///     next.push(GameState::Paused);
/// }
/// ```
#[derive(Resource, Debug)]
pub struct NextState<S: States> {
    changes: Vec<StateChange<S>>,
}

impl<S: States> Default for NextState<S> {
    fn default() -> Self {
        Self {
            changes: Vec::new(),
        }
    }
}

impl<S: States> NextState<S> {
    /// Pauses the current state and enters `state` on top of it.
    pub fn push(&mut self, state: S) {
        self.changes.push(StateChange::Push(state));
    }

    /// Exits the current state and resumes the one below it.
    pub fn pop(&mut self) {
        self.changes.push(StateChange::Pop);
    }

    /// Exits the current state and enters `state` in its place.
    pub fn replace(&mut self, state: S) {
        self.changes.push(StateChange::Replace(state));
    }

    /// Exits every state and enters `state` as the only one.
    pub fn set(&mut self, state: S) {
        self.changes.push(StateChange::Set(state));
    }

    /// The changes waiting to be applied.
    pub fn pending(&self) -> &[StateChange<S>] {
        &self.changes
    }
}

/// Despawns the entity (and its children) when `S` exits, that is when it leaves the
/// stack. Pausing does not count.
///
/// ```
/// use cg_core::prelude::*;
/// use cg_world::{StateScoped, Transform2D};
///
/// #[derive(Clone, PartialEq, Eq, Hash, Debug)]
/// enum GameState { Menu, Playing }
///
/// fn spawn_level(mut commands: Commands) {
///     commands.spawn((Transform2D::default(), StateScoped(GameState::Playing)));
/// }
/// ```
#[derive(Component, Debug, Clone, PartialEq, Eq)]
pub struct StateScoped<S: States>(pub S);

/// A run condition: true while `state` is the current state (the top of the stack).
///
/// Gameplay usually runs only in its own state, so pushing a pause state stops it:
///
/// ```
/// use cg_core::prelude::*;
/// use cg_world::in_state;
///
/// #[derive(Clone, PartialEq, Eq, Hash, Debug)]
/// enum GameState { Playing }
///
/// fn walk() {}
///
/// App::new().add_systems(FixedUpdate, walk.run_if(in_state(GameState::Playing)));
/// ```
pub fn in_state<S: States>(state: S) -> impl FnMut(Option<Res<State<S>>>) -> bool + Clone {
    move |current: Option<Res<State<S>>>| current.is_some_and(|c| *c.current() == state)
}

/// A run condition: true while `state` is anywhere in the stack, current or paused under
/// another state. Useful for things that should keep running under a pause overlay.
pub fn state_in_stack<S: States>(state: S) -> impl FnMut(Option<Res<State<S>>>) -> bool + Clone {
    move |current: Option<Res<State<S>>>| current.is_some_and(|c| c.contains(&state))
}

/// The most changes applied in one frame, so states that keep requesting changes from
/// their enter schedules cannot hang the game.
const MAX_CHANGES_PER_FRAME: usize = 64;

/// Applies [`NextState`] requests, running the enter, exit, pause and resume schedules.
/// On the first run it enters the initial state.
pub(crate) fn apply_state_changes<S: States>(world: &mut World, mut entered: Local<bool>) {
    if !*entered {
        *entered = true;
        let initial = world.resource::<State<S>>().current().clone();
        run(world, OnEnter(initial));
    }
    for _ in 0..MAX_CHANGES_PER_FRAME {
        let change = {
            let mut next = world.resource_mut::<NextState<S>>();
            if next.changes.is_empty() {
                return;
            }
            next.changes.remove(0)
        };
        apply(world, change);
    }
    log::warn!(
        "more than {MAX_CHANGES_PER_FRAME} state changes for `{}` in one frame; the rest wait \
         for the next frame",
        core::any::type_name::<S>()
    );
}

fn apply<S: States>(world: &mut World, change: StateChange<S>) {
    let current = world.resource::<State<S>>().current().clone();
    match change {
        StateChange::Push(state) => {
            run(world, OnPause(current));
            world.resource_mut::<State<S>>().stack.push(state.clone());
            run(world, OnEnter(state));
        }
        StateChange::Pop => {
            if world.resource::<State<S>>().stack.len() == 1 {
                log::warn!("cannot pop `{current:?}`: it is the only state");
                return;
            }
            exit(world, current);
            let resumed = world.resource::<State<S>>().current().clone();
            run(world, OnResume(resumed));
        }
        StateChange::Replace(state) => {
            exit(world, current);
            world.resource_mut::<State<S>>().stack.push(state.clone());
            run(world, OnEnter(state));
        }
        StateChange::Set(state) => {
            while let Some(top) = world.resource::<State<S>>().stack.last().cloned() {
                exit(world, top);
            }
            world.resource_mut::<State<S>>().stack.push(state.clone());
            run(world, OnEnter(state));
        }
    }
}

/// Runs the exit schedule of `state`, the top of the stack, then pops it and despawns its
/// scoped entities. While its exit schedule runs, it is still the current state.
fn exit<S: States>(world: &mut World, state: S) {
    run(world, OnExit(state.clone()));
    world.resource_mut::<State<S>>().stack.pop();
    let scoped: Vec<Entity> = world
        .query::<(Entity, &StateScoped<S>)>()
        .iter(world)
        .filter(|(_, scope)| scope.0 == state)
        .map(|(entity, _)| entity)
        .collect();
    for entity in scoped {
        // A scoped child may already be gone with its scoped parent.
        if let Ok(entity) = world.get_entity_mut(entity) {
            entity.despawn();
        }
    }
}

fn run(world: &mut World, label: impl ScheduleLabel) {
    // Schedules nobody added systems to do not exist; that is fine.
    let _ = world.try_run_schedule(label);
}

/// Creates the [`State`] and [`NextState`] resources for a state type.
pub(crate) fn init<S: States>(world: &mut World, initial: S) {
    world.insert_resource(State {
        stack: vec![initial],
    });
    world.init_resource::<NextState<S>>();
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{AppWorldExt, Transform2D, WorldPlugin};
    use cg_core::{App, PreUpdate, StateTransition, Update};
    use core::time::Duration;

    #[derive(Clone, PartialEq, Eq, Hash, Debug)]
    enum Game {
        Menu,
        Playing,
        Paused,
    }

    #[derive(Resource, Default)]
    struct Log(Vec<String>);

    fn record(entry: &'static str) -> impl FnMut(ResMut<Log>) {
        move |mut log: ResMut<Log>| log.0.push(entry.to_owned())
    }

    fn app() -> App {
        let mut app = App::new();
        app.add_plugins(WorldPlugin)
            .init_resource::<Log>()
            .init_state(Game::Menu);
        for (state, name) in [
            (Game::Menu, "menu"),
            (Game::Playing, "playing"),
            (Game::Paused, "paused"),
        ] {
            let enter: &'static str = format!("enter {name}").leak();
            let exit: &'static str = format!("exit {name}").leak();
            let pause: &'static str = format!("pause {name}").leak();
            let resume: &'static str = format!("resume {name}").leak();
            app.add_systems(OnEnter(state.clone()), record(enter))
                .add_systems(OnExit(state.clone()), record(exit))
                .add_systems(OnPause(state.clone()), record(pause))
                .add_systems(OnResume(state), record(resume));
        }
        app.add_systems(
            Update,
            (
                record("update playing").run_if(in_state(Game::Playing)),
                record("update under").run_if(state_in_stack(Game::Playing)),
            )
                .chain(),
        );
        app
    }

    fn frame(app: &mut App) -> Vec<String> {
        app.update(Duration::ZERO);
        core::mem::take(&mut app.world_mut().resource_mut::<Log>().0)
    }

    fn next(app: &mut App) -> Mut<'_, NextState<Game>> {
        app.world_mut().resource_mut::<NextState<Game>>()
    }

    #[test]
    fn push_pop_replace_and_set_run_the_right_schedules() {
        let mut app = app();
        assert_eq!(frame(&mut app), ["enter menu"]);

        next(&mut app).replace(Game::Playing);
        assert_eq!(
            frame(&mut app),
            [
                "exit menu",
                "enter playing",
                "update playing",
                "update under"
            ]
        );

        next(&mut app).push(Game::Paused);
        assert_eq!(
            frame(&mut app),
            ["pause playing", "enter paused", "update under"]
        );
        assert_eq!(
            app.world().resource::<State<Game>>().stack(),
            [Game::Playing, Game::Paused]
        );

        next(&mut app).pop();
        assert_eq!(
            frame(&mut app),
            [
                "exit paused",
                "resume playing",
                "update playing",
                "update under"
            ]
        );

        next(&mut app).push(Game::Paused);
        next(&mut app).set(Game::Menu);
        assert_eq!(
            frame(&mut app),
            [
                "pause playing",
                "enter paused",
                "exit paused",
                "exit playing",
                "enter menu"
            ]
        );
        assert_eq!(app.world().resource::<State<Game>>().stack(), [Game::Menu]);
    }

    #[test]
    fn the_last_state_cannot_be_popped() {
        let mut app = app();
        frame(&mut app);
        next(&mut app).pop();
        assert!(frame(&mut app).is_empty());
        assert_eq!(*app.world().resource::<State<Game>>().current(), Game::Menu);
    }

    #[test]
    fn scoped_entities_despawn_on_exit_but_not_on_pause() {
        let mut app = app();
        frame(&mut app);
        next(&mut app).replace(Game::Playing);
        frame(&mut app);
        let level = app
            .world_mut()
            .spawn((Transform2D::default(), StateScoped(Game::Playing)))
            .id();
        let child = app.world_mut().spawn(ChildOf(level)).id();

        next(&mut app).push(Game::Paused);
        frame(&mut app);
        assert!(app.world().get_entity(level).is_ok());

        next(&mut app).set(Game::Menu);
        frame(&mut app);
        assert!(app.world().get_entity(level).is_err());
        assert!(app.world().get_entity(child).is_err());
    }

    #[test]
    fn changes_requested_while_entering_apply_in_the_same_frame() {
        let mut app = app();
        app.add_systems(OnEnter(Game::Menu), |mut next: ResMut<NextState<Game>>| {
            next.replace(Game::Playing);
        });
        let log = frame(&mut app);
        assert!(log.starts_with(&["enter menu".to_owned(), "exit menu".to_owned()]));
        assert_eq!(
            *app.world().resource::<State<Game>>().current(),
            Game::Playing
        );
    }

    #[test]
    fn transitions_happen_after_pre_update() {
        let mut app = app();
        app.add_systems(PreUpdate, record("pre_update"));
        assert_eq!(frame(&mut app), ["pre_update", "enter menu"]);
        // Requests made in the transition schedule itself still apply this frame.
        app.add_systems(
            StateTransition,
            (|mut next: ResMut<NextState<Game>>| next.replace(Game::Playing))
                .run_if(in_state(Game::Menu))
                .before(apply_state_changes::<Game>),
        );
        assert_eq!(
            frame(&mut app),
            [
                "pre_update",
                "exit menu",
                "enter playing",
                "update playing",
                "update under"
            ]
        );
    }
}
