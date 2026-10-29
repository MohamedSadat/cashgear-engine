//! Animation state machines: an [`AnimationGraph`] of states and transitions, run on an
//! entity by an [`Animator`] whose parameters the game sets.

use crate::player::SpriteAnimation;
use bevy_ecs::prelude::*;
use cg_assets::{
    Asset, AssetLoader, AssetServer, Assets, BoxError, BoxedFuture, Handle, LoadContext,
};
use cg_core::Time;
use cg_world::PrefabComponent;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// States, each playing a clip, and the transitions between them, chosen by parameters
/// such as `speed` or `grounded` that the game sets on an [`Animator`].
///
/// Graphs load from `.anim.ron` files (hot-reloaded like any asset) or are built in code:
///
/// ```ron
/// (
///     start: "idle",
///     states: [
///         (name: "idle"),
///         // Plays the "run" clip faster the faster the character runs: at speed 120,
///         // at its own pace.
///         (name: "run", speed_param: "speed", speed: 0.00833),
///         (name: "jump"),
///         (name: "fall"),
///         (name: "land"),
///     ],
///     transitions: [
///         (from: ["idle", "run", "land"], to: "jump", when: [("grounded", False), ("vy", Gt(0))]),
///         (from: ["idle", "run", "land", "jump"], to: "fall", when: [("grounded", False), ("vy", Le(0))]),
///         (from: ["jump", "fall"], to: "land", when: [("grounded", True)]),
///         (from: ["idle", "land"], to: "run", when: [("speed", Gt(5))]),
///         (from: ["run"], to: "idle", when: [("speed", Le(5))]),
///         (from: ["land"], to: "idle", after_clip: true),
///     ],
/// )
/// ```
///
/// Each fixed step the animator takes the first transition, in file order, that leaves
/// the current state (an empty `from` means any other state) and whose conditions all
/// hold, then looks again from the new state, up to eight times a step.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct AnimationGraph {
    /// The state entered first, and whenever the current one disappears in a reload.
    pub start: String,
    /// The states.
    pub states: Vec<AnimationState>,
    /// The transitions, in the order they are tried.
    pub transitions: Vec<Transition>,
}

impl Asset for AnimationGraph {}

impl AnimationGraph {
    /// An empty graph that starts in `start`.
    pub fn new(start: impl Into<String>) -> Self {
        Self {
            start: start.into(),
            ..Self::default()
        }
    }

    /// Adds a state.
    pub fn with_state(mut self, state: AnimationState) -> Self {
        self.states.push(state);
        self
    }

    /// Adds a transition, tried after the ones before it.
    pub fn with_transition(mut self, transition: Transition) -> Self {
        self.transitions.push(transition);
        self
    }

    /// The state called `name`.
    pub fn state(&self, name: &str) -> Option<&AnimationState> {
        self.states.iter().find(|s| s.name == name)
    }

    /// Checks that the start state and every state a transition names exist.
    pub fn validate(&self) -> Result<(), String> {
        let check = |name: &str, what: &str| {
            if self.state(name).is_some() {
                Ok(())
            } else {
                Err(format!(
                    "{what} names a state `{name}` that is not in the graph"
                ))
            }
        };
        check(&self.start, "`start`")?;
        for (index, transition) in self.transitions.iter().enumerate() {
            let what = format!("transition {index}");
            check(&transition.to, &what)?;
            for from in &transition.from {
                check(from, &what)?;
            }
        }
        Ok(())
    }
}

/// One state of an [`AnimationGraph`]: a clip and how fast it plays.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct AnimationState {
    /// The state's name.
    pub name: String,
    /// The clip it plays; empty means the clip named like the state.
    pub clip: String,
    /// Playback speed. With a `speed_param`, it is multiplied by the parameter's absolute
    /// value, so a run cycle can keep pace with the character.
    pub speed: f32,
    /// A parameter that scales the playback speed; empty for none.
    pub speed_param: String,
}

impl Default for AnimationState {
    fn default() -> Self {
        Self {
            name: String::new(),
            clip: String::new(),
            speed: 1.0,
            speed_param: String::new(),
        }
    }
}

impl AnimationState {
    /// A state called `name` that plays the clip of the same name.
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            ..Self::default()
        }
    }

    /// Plays `clip` instead of the clip named like the state.
    pub fn with_clip(mut self, clip: impl Into<String>) -> Self {
        self.clip = clip.into();
        self
    }

    /// Plays at `speed` times the absolute value of `param`.
    pub fn scaled_by(mut self, param: impl Into<String>, speed: f32) -> Self {
        self.speed_param = param.into();
        self.speed = speed;
        self
    }

    /// The clip this state plays.
    pub fn clip_name(&self) -> &str {
        if self.clip.is_empty() {
            &self.name
        } else {
            &self.clip
        }
    }
}

/// A way out of one or more states of an [`AnimationGraph`].
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct Transition {
    /// The states it leaves; empty means any state but `to`.
    pub from: Vec<String>,
    /// The state it enters.
    pub to: String,
    /// Conditions that must all hold.
    pub when: Vec<Condition>,
    /// Only once the current clip has finished (for clips that play once).
    pub after_clip: bool,
    /// Only after the current state has lasted this many seconds.
    pub min_time: f32,
}

impl Transition {
    /// A transition into `state`, from any other state until [`from`](Self::from) says
    /// otherwise.
    pub fn to(state: impl Into<String>) -> Self {
        Self {
            to: state.into(),
            ..Self::default()
        }
    }

    /// Leaves only these states.
    pub fn from<S: Into<String>>(mut self, states: impl IntoIterator<Item = S>) -> Self {
        self.from = states.into_iter().map(Into::into).collect();
        self
    }

    /// Also requires `param` to pass `test`.
    pub fn when(mut self, param: impl Into<String>, test: Test) -> Self {
        self.when.push(Condition(param.into(), test));
        self
    }

    /// Waits for the current clip to finish.
    pub fn after_clip(mut self) -> Self {
        self.after_clip = true;
        self
    }

    /// Waits until the current state has lasted `seconds`.
    pub fn after(mut self, seconds: f32) -> Self {
        self.min_time = seconds;
        self
    }

    fn leaves(&self, state: &str) -> bool {
        if self.from.is_empty() {
            self.to != state
        } else {
            self.from.iter().any(|s| s == state)
        }
    }
}

/// A test of one parameter: `("speed", Gt(5))` in RON.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Condition(pub String, pub Test);

/// What a [`Condition`] checks. Unset parameters are 0 (false).
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub enum Test {
    /// Greater than.
    Gt(f32),
    /// Greater than or equal.
    Ge(f32),
    /// Less than.
    Lt(f32),
    /// Less than or equal.
    Le(f32),
    /// Equal.
    Eq(f32),
    /// Not equal.
    Ne(f32),
    /// Set to true ([`Animator::set_bool`]), that is not 0.
    True,
    /// False or unset.
    False,
    /// Triggered this step ([`Animator::trigger`]). Taking the transition uses the
    /// trigger up.
    Trigger,
}

/// Runs an [`AnimationGraph`] on the entity's [`SpriteAnimation`]: each fixed step, after
/// the physics, it checks the transitions against its parameters and plays the clip of
/// the state it is in.
///
/// Set the parameters from gameplay, in `FixedUpdate`:
///
/// ```
/// use cg_anim::Animator;
/// use cg_core::prelude::*;
/// use cg_physics::{KinematicBody, KinematicContacts};
///
/// fn feed(mut heroes: Query<(&mut Animator, &KinematicBody, &KinematicContacts)>) {
///     for (mut animator, body, contacts) in &mut heroes {
///         animator.set("speed", body.velocity.x.abs());
///         animator.set("vy", body.velocity.y);
///         animator.set_bool("grounded", contacts.on_ground());
///     }
/// }
/// ```
///
/// Triggers last one step: set them in the step they happen.
///
/// In prefabs: `"Animator": (graph: "anims/hero.anim.ron")`. The entity needs a
/// `SpriteAnimation` with the sheet too.
#[derive(Component, Debug, Clone)]
pub struct Animator {
    /// The graph to run.
    pub graph: Handle<AnimationGraph>,
    params: BTreeMap<String, f32>,
    triggers: Vec<String>,
    state: Option<String>,
    state_time: f32,
    /// A state requested with [`force`](Self::force).
    forced: Option<String>,
    warned: bool,
}

impl Animator {
    /// Runs `graph`, starting in its start state.
    pub fn new(graph: Handle<AnimationGraph>) -> Self {
        Self {
            graph,
            params: BTreeMap::new(),
            triggers: Vec::new(),
            state: None,
            state_time: 0.0,
            forced: None,
            warned: false,
        }
    }

    /// Sets a number parameter.
    pub fn set(&mut self, name: &str, value: f32) {
        match self.params.get_mut(name) {
            Some(slot) => *slot = value,
            None => {
                self.params.insert(name.to_owned(), value);
            }
        }
    }

    /// Sets a true/false parameter (stored as 1 or 0).
    pub fn set_bool(&mut self, name: &str, value: bool) {
        self.set(name, if value { 1.0 } else { 0.0 });
    }

    /// Fires a trigger for this step.
    pub fn trigger(&mut self, name: &str) {
        if !self.triggers.iter().any(|t| t == name) {
            self.triggers.push(name.to_owned());
        }
    }

    /// A parameter's value; 0 if it was never set.
    pub fn get(&self, name: &str) -> f32 {
        self.params.get(name).copied().unwrap_or(0.0)
    }

    /// The current state, once the graph has loaded.
    pub fn state(&self) -> Option<&str> {
        self.state.as_deref()
    }

    /// Seconds in the current state.
    pub fn state_time(&self) -> f32 {
        self.state_time
    }

    /// Enters `state` at the next step, whatever the transitions say (after a respawn,
    /// say).
    pub fn force(&mut self, state: impl Into<String>) {
        self.forced = Some(state.into());
    }

    fn passes(&self, Condition(param, test): &Condition) -> bool {
        let value = self.get(param);
        match *test {
            Test::Gt(x) => value > x,
            Test::Ge(x) => value >= x,
            Test::Lt(x) => value < x,
            Test::Le(x) => value <= x,
            Test::Eq(x) => value == x,
            Test::Ne(x) => value != x,
            Test::True => value != 0.0,
            Test::False => value == 0.0,
            Test::Trigger => self.triggers.iter().any(|t| t == param),
        }
    }
}

/// An [`Animator`] entered a new state.
#[derive(Message, Debug, Clone, PartialEq)]
pub struct AnimationStateChanged {
    /// The animated entity.
    pub entity: Entity,
    /// The state it left; `None` when the animator started.
    pub from: Option<String>,
    /// The state it entered.
    pub to: String,
}

/// The prefab form of an [`Animator`]: the graph by its asset path.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct AnimatorData {
    /// The graph's asset path, such as `anims/hero.anim.ron`.
    pub graph: String,
}

impl PrefabComponent for Animator {
    type Data = AnimatorData;

    fn from_data(data: &AnimatorData, world: &World) -> Result<Self, String> {
        let server = world
            .get_resource::<AssetServer>()
            .ok_or_else(|| format!("no asset server to load `{}`", data.graph))?;
        Ok(Self::new(server.load(data.graph.as_str())))
    }
}

/// Loads `.anim.ron` files as [`AnimationGraph`]s, checking that every state they name
/// exists. `AnimPlugin` registers it.
#[derive(Debug, Default)]
pub struct AnimationGraphLoader;

impl AssetLoader for AnimationGraphLoader {
    type Asset = AnimationGraph;

    fn extensions(&self) -> &[&str] {
        &["anim.ron"]
    }

    fn load<'a>(
        &'a self,
        ctx: &'a mut LoadContext,
    ) -> BoxedFuture<'a, Result<AnimationGraph, BoxError>> {
        Box::pin(async move {
            let graph: AnimationGraph = ron::from_str(core::str::from_utf8(ctx.bytes())?)?;
            graph.validate()?;
            Ok(graph)
        })
    }
}

/// Transitions taken at most per step, so a loop of always-true transitions cannot hang.
const MAX_TRANSITIONS: usize = 8;

/// Runs every [`Animator`] for one fixed step: forced states, transitions, then the
/// state's clip and speed on the entity's [`SpriteAnimation`].
pub(crate) fn run_animators(
    time: Res<Time>,
    graphs: Res<Assets<AnimationGraph>>,
    mut animators: Query<(Entity, &mut Animator, &mut SpriteAnimation)>,
    mut changes: MessageWriter<AnimationStateChanged>,
) {
    let dt = time.fixed_delta_secs();
    for (entity, mut animator, mut animation) in &mut animators {
        let Some(graph) = graphs.get(&animator.graph) else {
            continue;
        };
        let animator = animator.bypass_change_detection();
        let mut enter = |animator: &mut Animator, animation: &mut SpriteAnimation, to: &str| {
            let Some(state) = graph.state(to) else {
                return false;
            };
            let from = animator.state.replace(to.to_owned());
            animator.state_time = 0.0;
            animation.restart_with(state.clip_name());
            changes.write(AnimationStateChanged {
                entity,
                from,
                to: to.to_owned(),
            });
            true
        };

        if let Some(forced) = animator.forced.take()
            && !enter(animator, &mut animation, &forced)
        {
            log::warn!("entity {entity}: the animation graph has no state `{forced}`");
        }
        let current_exists = animator
            .state
            .as_deref()
            .is_some_and(|s| graph.state(s).is_some());
        if current_exists {
            animator.state_time += dt;
        } else {
            let start = graph.start.clone();
            if !enter(animator, &mut animation, &start) {
                if !animator.warned {
                    animator.warned = true;
                    log::warn!("entity {entity}: the animation graph has no start state `{start}`");
                }
                continue;
            }
        }

        for _ in 0..MAX_TRANSITIONS {
            let Some(current) = animator.state.clone() else {
                break;
            };
            let next = graph.transitions.iter().find(|t| {
                t.leaves(&current)
                    && (!t.after_clip || animation.is_finished())
                    && animator.state_time >= t.min_time
                    && t.when.iter().all(|c| animator.passes(c))
            });
            let Some(transition) = next else {
                break;
            };
            for Condition(param, test) in &transition.when {
                if *test == Test::Trigger {
                    animator.triggers.retain(|t| t != param);
                }
            }
            enter(animator, &mut animation, &transition.to);
        }
        animator.triggers.clear();

        if let Some(state) = animator.state.as_deref().and_then(|s| graph.state(s)) {
            let speed = if state.speed_param.is_empty() {
                state.speed
            } else {
                state.speed * animator.get(&state.speed_param).abs()
            };
            if animation.speed != speed {
                animation.speed = speed;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn graphs_read_from_ron_and_validate() {
        let graph: AnimationGraph = ron::from_str(
            r#"(
                start: "idle",
                states: [(name: "idle"), (name: "run", speed_param: "speed", speed: 0.01), (name: "hurt", clip: "ouch")],
                transitions: [
                    (from: ["idle"], to: "run", when: [("speed", Gt(5)), ("grounded", True)]),
                    (to: "hurt", when: [("hit", Trigger)]),
                    (from: ["hurt"], to: "idle", after_clip: true, min_time: 0.2),
                ],
            )"#,
        )
        .unwrap();
        graph.validate().unwrap();
        assert_eq!(graph.state("hurt").unwrap().clip_name(), "ouch");
        assert_eq!(graph.state("idle").unwrap().clip_name(), "idle");
        assert_eq!(graph.state("run").unwrap().speed_param, "speed");
        assert_eq!(graph.transitions[1].from, Vec::<String>::new());
        assert_eq!(
            graph.transitions[1].when,
            [Condition("hit".into(), Test::Trigger)]
        );
        assert_eq!(graph.transitions[2].min_time, 0.2);

        let bad = AnimationGraph::new("idle")
            .with_state(AnimationState::new("idle"))
            .with_transition(Transition::to("swim").from(["idle"]));
        assert_eq!(
            bad.validate().unwrap_err(),
            "transition 0 names a state `swim` that is not in the graph"
        );
        assert!(AnimationGraph::new("nowhere").validate().is_err());
    }

    #[test]
    fn conditions_test_parameters() {
        let mut animator = Animator::new(Handle::default());
        animator.set("speed", 6.0);
        animator.set_bool("grounded", true);
        animator.trigger("jump");
        let passes = |param: &str, test| animator.passes(&Condition(param.into(), test));
        assert!(passes("speed", Test::Gt(5.0)));
        assert!(!passes("speed", Test::Lt(5.0)));
        assert!(passes("speed", Test::Ge(6.0)) && passes("speed", Test::Le(6.0)));
        assert!(passes("speed", Test::Eq(6.0)) && !passes("speed", Test::Ne(6.0)));
        assert!(passes("grounded", Test::True) && !passes("grounded", Test::False));
        assert!(passes("unset", Test::False), "unset parameters are 0");
        assert!(passes("jump", Test::Trigger) && !passes("land", Test::Trigger));
        let any = Transition::to("run");
        assert!(any.leaves("idle") && !any.leaves("run"));
    }
}
