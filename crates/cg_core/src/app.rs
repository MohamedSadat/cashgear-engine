//! The [`App`] builder and the frame loop it drives.

use crate::schedule::{
    First, FixedPostUpdate, FixedPreUpdate, FixedUpdate, Last, PostUpdate, PreUpdate, Render,
    Startup, StateTransition, Update,
};
use crate::{AppExit, Plugin, Time};
use bevy_ecs::message::{
    MessageCursor, MessageRegistry, ShouldUpdateMessages, message_update_condition,
    message_update_system, signal_message_update_system,
};
use bevy_ecs::prelude::*;
use bevy_ecs::schedule::{InternedSystemSet, ScheduleLabel};
use bevy_ecs::system::ScheduleSystem;
use core::time::Duration;
use std::collections::HashSet;

type Runner = Box<dyn FnOnce(App) -> AppExit>;

/// The app: an ECS [`World`], the schedules that run each frame, and a runner that
/// drives the frames.
///
/// Build it with plugins and systems, then call [`run`](Self::run):
///
/// ```
/// use cg_core::prelude::*;
///
/// fn hello() {
///     info!("hello");
/// }
///
/// App::new().add_systems(Startup, hello).run();
/// ```
///
/// Without a runner plugin (the platform layer's window plugin), `run` runs a single frame.
/// Runners call [`update`](Self::update) once per frame with the measured frame delta.
pub struct App {
    world: World,
    plugins: HashSet<String>,
    runner: Runner,
    started: bool,
    exit_cursor: MessageCursor<AppExit>,
}

impl Default for App {
    fn default() -> Self {
        Self::new()
    }
}

impl App {
    /// Creates an app with the frame schedules, the [`Time`] resource and the [`AppExit`]
    /// message, and no plugins.
    pub fn new() -> Self {
        let mut world = World::new();
        for label in [
            Startup.intern(),
            First.intern(),
            PreUpdate.intern(),
            StateTransition.intern(),
            FixedPreUpdate.intern(),
            FixedUpdate.intern(),
            FixedPostUpdate.intern(),
            Update.intern(),
            PostUpdate.intern(),
            Render.intern(),
            Last.intern(),
        ] {
            world.add_schedule(Schedule::new(label));
        }
        world.init_resource::<Time>();

        let mut app = Self {
            world,
            plugins: HashSet::new(),
            runner: Box::new(run_once),
            started: false,
            exit_cursor: MessageCursor::default(),
        };
        // Message buffers swap at most once per frame, and only after a fixed step has run,
        // so gameplay systems in `FixedUpdate` never miss a message. While time is paused no
        // fixed steps run, so buffers then swap every frame instead of growing.
        app.add_message::<AppExit>();
        app.world.resource_mut::<MessageRegistry>().should_update = ShouldUpdateMessages::Waiting;
        app.add_systems(
            First,
            message_update_system.run_if(message_update_condition),
        );
        app.add_systems(FixedPostUpdate, signal_message_update_system);
        app.add_systems(Last, signal_message_update_while_paused);
        app
    }

    /// Adds a plugin and runs its [`build`](Plugin::build).
    ///
    /// # Panics
    ///
    /// Panics if a plugin with the same [`name`](Plugin::name) was already added.
    pub fn add_plugins(&mut self, plugin: impl Plugin) -> &mut Self {
        let name = plugin.name().to_owned();
        if !self.plugins.insert(name.clone()) {
            panic!("plugin `{name}` was added twice");
        }
        plugin.build(self);
        self
    }

    /// Whether a plugin of type `P` was added (by its default name).
    pub fn is_plugin_added<P: Plugin>(&self) -> bool {
        self.plugins.contains(core::any::type_name::<P>())
    }

    /// Adds systems to a schedule.
    pub fn add_systems<M>(
        &mut self,
        schedule: impl ScheduleLabel,
        systems: impl IntoScheduleConfigs<ScheduleSystem, M>,
    ) -> &mut Self {
        self.world
            .resource_mut::<Schedules>()
            .add_systems(schedule, systems);
        self
    }

    /// Configures system sets in a schedule: their order and run conditions.
    pub fn configure_sets<M>(
        &mut self,
        schedule: impl ScheduleLabel,
        sets: impl IntoScheduleConfigs<InternedSystemSet, M>,
    ) -> &mut Self {
        self.world
            .resource_mut::<Schedules>()
            .configure_sets(schedule, sets);
        self
    }

    /// Inserts a resource, replacing any existing one of the same type.
    pub fn insert_resource<R: Resource>(&mut self, resource: R) -> &mut Self {
        self.world.insert_resource(resource);
        self
    }

    /// Inserts a resource built with [`Default`] or [`FromWorld`], unless one exists.
    pub fn init_resource<R: Resource + FromWorld>(&mut self) -> &mut Self {
        self.world.init_resource::<R>();
        self
    }

    /// Registers a message type so systems can write and read it. Each message is kept
    /// for two frames.
    pub fn add_message<M: Message>(&mut self) -> &mut Self {
        if !self.world.contains_resource::<Messages<M>>() {
            MessageRegistry::register_message::<M>(&mut self.world);
        }
        self
    }

    /// The ECS world.
    pub fn world(&self) -> &World {
        &self.world
    }

    /// The ECS world, mutably. Runners use it to feed in input and window events.
    pub fn world_mut(&mut self) -> &mut World {
        &mut self.world
    }

    /// Sets the function that drives the frames when [`run`](Self::run) is called.
    pub fn set_runner(&mut self, runner: impl FnOnce(App) -> AppExit + 'static) -> &mut Self {
        self.runner = Box::new(runner);
        self
    }

    /// Hands the app to its runner and returns how it exited.
    ///
    /// On the web the runner returns at once with [`AppExit::Success`] and the browser
    /// drives the frames afterwards.
    pub fn run(&mut self) -> AppExit {
        let mut app = core::mem::take(self);
        let runner = core::mem::replace(&mut app.runner, Box::new(run_once));
        runner(app)
    }

    /// Runs one frame. `delta` is the real time since the previous frame; the first frame
    /// should pass [`Duration::ZERO`].
    pub fn update(&mut self, delta: Duration) {
        let world = &mut self.world;
        if !self.started {
            self.started = true;
            world.run_schedule(Startup);
        }
        world.resource_mut::<Time>().begin_frame(delta);
        world.run_schedule(First);
        world.run_schedule(PreUpdate);
        world.run_schedule(StateTransition);
        while world.resource_mut::<Time>().expend_fixed_step() {
            world.run_schedule(FixedPreUpdate);
            world.run_schedule(FixedUpdate);
            world.run_schedule(FixedPostUpdate);
        }
        world.resource_mut::<Time>().end_fixed_steps();
        world.run_schedule(Update);
        world.run_schedule(PostUpdate);
        world.run_schedule(Render);
        world.run_schedule(Last);
        world.clear_trackers();
    }

    /// Returns an exit request written since the last call, if any. Errors win over
    /// [`AppExit::Success`]. Runners call it after each [`update`](Self::update).
    pub fn should_exit(&mut self) -> Option<AppExit> {
        let messages = self.world.get_resource::<Messages<AppExit>>()?;
        let mut exit = None;
        for request in self.exit_cursor.read(messages) {
            if exit.is_none() || !request.is_success() {
                exit = Some(*request);
            }
        }
        exit
    }
}

fn signal_message_update_while_paused(time: Res<Time>, mut registry: ResMut<MessageRegistry>) {
    if time.time_scale() == 0.0 {
        registry.should_update = ShouldUpdateMessages::Ready;
    }
}

/// The default runner: one frame, then exit.
fn run_once(mut app: App) -> AppExit {
    app.update(Duration::ZERO);
    app.should_exit().unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Resource, Default)]
    struct Log(Vec<&'static str>);

    fn record(name: &'static str) -> impl FnMut(ResMut<Log>) {
        move |mut log: ResMut<Log>| log.0.push(name)
    }

    fn app_with_log() -> App {
        let mut app = App::new();
        app.init_resource::<Log>();
        app.add_systems(Startup, record("startup"))
            .add_systems(First, record("first"))
            .add_systems(PreUpdate, record("pre_update"))
            .add_systems(StateTransition, record("state_transition"))
            .add_systems(FixedPreUpdate, record("fixed_pre_update"))
            .add_systems(FixedUpdate, record("fixed_update"))
            .add_systems(FixedPostUpdate, record("fixed_post_update"))
            .add_systems(Update, record("update"))
            .add_systems(PostUpdate, record("post_update"))
            .add_systems(Render, record("render"))
            .add_systems(Last, record("last"));
        app
    }

    #[test]
    fn frame_runs_schedules_in_order() {
        let mut app = app_with_log();
        let step = app.world().resource::<Time>().fixed_delta();
        app.update(step * 2);
        assert_eq!(
            app.world().resource::<Log>().0,
            [
                "startup",
                "first",
                "pre_update",
                "state_transition",
                "fixed_pre_update",
                "fixed_update",
                "fixed_post_update",
                "fixed_pre_update",
                "fixed_update",
                "fixed_post_update",
                "update",
                "post_update",
                "render",
                "last",
            ]
        );
    }

    #[test]
    fn startup_runs_once_and_short_frames_skip_fixed_update() {
        let mut app = app_with_log();
        app.update(Duration::ZERO);
        app.update(Duration::ZERO);
        let log = &app.world().resource::<Log>().0;
        assert_eq!(log.iter().filter(|s| **s == "startup").count(), 1);
        assert_eq!(log.iter().filter(|s| **s == "fixed_update").count(), 0);
        assert_eq!(log.iter().filter(|s| **s == "update").count(), 2);
    }

    #[test]
    fn messages_survive_frames_without_fixed_steps() {
        #[derive(Message)]
        struct Ping;
        #[derive(Resource, Default)]
        struct Seen(usize);

        let mut app = App::new();
        app.add_message::<Ping>()
            .init_resource::<Seen>()
            .add_systems(Startup, |mut w: MessageWriter<Ping>| {
                w.write(Ping);
            })
            .add_systems(
                FixedUpdate,
                |mut r: MessageReader<Ping>, mut seen: ResMut<Seen>| {
                    seen.0 += r.read().count();
                },
            );
        // Three frames too short for a fixed step, then one long enough.
        for _ in 0..3 {
            app.update(Duration::ZERO);
        }
        let step = app.world().resource::<Time>().fixed_delta();
        app.update(step);
        assert_eq!(app.world().resource::<Seen>().0, 1);
    }

    #[test]
    fn messages_do_not_pile_up_while_paused() {
        #[derive(Message)]
        struct Ping;

        let mut app = App::new();
        app.add_message::<Ping>()
            .add_systems(Update, |mut w: MessageWriter<Ping>| {
                w.write(Ping);
            });
        app.world_mut().resource_mut::<Time>().set_time_scale(0.0);
        for _ in 0..10 {
            app.update(Duration::from_millis(16));
        }
        assert!(app.world().resource::<Messages<Ping>>().len() <= 2);
    }

    #[test]
    fn exit_is_reported_once_and_errors_win() {
        let mut app = App::new();
        app.add_systems(Update, |mut exit: MessageWriter<AppExit>| {
            exit.write(AppExit::Success);
            exit.write(AppExit::error());
            exit.write(AppExit::Success);
        });
        assert_eq!(app.should_exit(), None);
        app.update(Duration::ZERO);
        assert_eq!(app.should_exit(), Some(AppExit::error()));
        assert_eq!(app.should_exit(), None);
    }

    #[test]
    fn default_runner_runs_one_frame() {
        let mut app = app_with_log();
        assert_eq!(app.run(), AppExit::Success);
    }

    #[test]
    fn custom_runner_receives_the_app() {
        let mut app = App::new();
        app.set_runner(|mut app| {
            for _ in 0..3 {
                app.update(Duration::from_millis(10));
            }
            assert_eq!(app.world().resource::<Time>().frame_count(), 3);
            AppExit::error()
        });
        assert_eq!(app.run(), AppExit::error());
    }

    #[test]
    #[should_panic(expected = "was added twice")]
    fn duplicate_plugins_panic() {
        struct Nothing;
        impl Plugin for Nothing {
            fn build(&self, _: &mut App) {}
        }
        let mut app = App::new();
        app.add_plugins(Nothing).add_plugins(Nothing);
    }

    #[test]
    fn plugins_are_tracked() {
        struct Nothing;
        impl Plugin for Nothing {
            fn build(&self, _: &mut App) {}
        }
        let mut app = App::new();
        assert!(!app.is_plugin_added::<Nothing>());
        app.add_plugins(Nothing);
        assert!(app.is_plugin_added::<Nothing>());
    }
}
