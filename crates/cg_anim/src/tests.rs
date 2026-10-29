//! Tests that run the animation systems in an app, without a window.

use crate::prelude::*;
use crate::{ClipFrame, SheetFrame};
use bevy_ecs::prelude::*;
use cg_assets::{AssetPlugin, Assets, Handle};
use cg_core::glam::{Vec2, vec2};
use cg_core::{App, Rect, Time, Update};
use cg_render::{Camera2D, Color, Sprite};
use cg_world::{AppWorldExt, Interpolated, State, Transform2D};
use core::time::Duration;

fn app(budget: u32) -> App {
    let mut app = App::new();
    app.add_plugins(AssetPlugin {
        hot_reload: Some(false),
        ..Default::default()
    })
    .add_plugins(AnimPlugin {
        particle_budget: budget,
    });
    app
}

fn step(app: &App) -> Duration {
    app.world().resource::<Time>().fixed_delta()
}

/// Runs `steps` frames of one fixed step each.
fn run(app: &mut App, steps: u32) {
    let dt = step(app);
    for _ in 0..steps {
        app.update(dt);
    }
}

/// Three 16×16 frames side by side, a tenth of a second each, with a looping "walk"
/// (footsteps on frames 0 and 2), a once-only "hit" and a ping-pong "wave".
fn sheet() -> SpriteSheet {
    let mut sheet = SpriteSheet::new(Handle::default());
    for i in 0..3 {
        let x = i as f32 * 16.0;
        sheet.add_frame(
            format!("f{i}"),
            SheetFrame::new(Rect::new(x, 0.0, x + 16.0, 16.0), 0.1),
        );
    }
    let frames = |range: core::ops::Range<usize>| {
        range.map(|frame| ClipFrame {
            frame,
            duration: 0.1,
        })
    };
    sheet.add_clip(
        "walk",
        AnimationClip::new(frames(0..3))
            .with_event(0, "footstep")
            .with_event(2, "footstep"),
    );
    sheet.add_clip("hit", AnimationClip::new(frames(1..3)).once());
    sheet.add_clip(
        "wave",
        AnimationClip::new(frames(0..3)).with_direction(Direction::PingPong),
    );
    sheet
}

/// Everything the animation systems said, collected each frame.
#[derive(Resource, Default)]
struct Heard {
    events: Vec<(String, usize)>,
    finished: Vec<String>,
    states: Vec<String>,
    tweens: Vec<Option<String>>,
}

fn listen(
    mut heard: ResMut<Heard>,
    mut events: MessageReader<AnimationEvent>,
    mut finished: MessageReader<AnimationFinished>,
    mut states: MessageReader<AnimationStateChanged>,
    mut tweens: MessageReader<TweenFinished>,
) {
    heard
        .events
        .extend(events.read().map(|e| (e.name.clone(), e.frame)));
    heard
        .finished
        .extend(finished.read().map(|f| f.clip.clone()));
    heard.states.extend(states.read().map(|s| s.to.clone()));
    heard.tweens.extend(tweens.read().map(|t| t.tag.clone()));
}

fn listening(budget: u32) -> App {
    let mut app = app(budget);
    app.init_resource::<Heard>()
        .add_systems(Update, listen.after(AnimSystems::Effects));
    app
}

fn source_x(app: &App, entity: Entity) -> f32 {
    app.world()
        .get::<Sprite>(entity)
        .unwrap()
        .source
        .unwrap()
        .min
        .x
}

#[test]
fn clips_step_through_frames_and_send_events() {
    let mut app = listening(100);
    let handle = app
        .world_mut()
        .resource_mut::<Assets<SpriteSheet>>()
        .add(sheet());
    let entity = app
        .world_mut()
        .spawn(SpriteAnimation::new(handle.clone(), "walk"))
        .id();
    run(&mut app, 1);
    assert_eq!(source_x(&app, entity), 0.0, "the first frame shows at once");
    assert_eq!(
        app.world().get::<Sprite>(entity).unwrap().texture,
        handle_image(&app, &handle)
    );
    // A tenth of a second is six steps at 60 Hz.
    run(&mut app, 6);
    assert_eq!(source_x(&app, entity), 16.0);
    run(&mut app, 6);
    assert_eq!(source_x(&app, entity), 32.0);
    run(&mut app, 6);
    assert_eq!(source_x(&app, entity), 0.0, "loops");
    let heard = app.world().resource::<Heard>();
    assert_eq!(
        heard.events,
        [
            ("footstep".to_owned(), 0),
            ("footstep".to_owned(), 2),
            ("footstep".to_owned(), 0)
        ]
    );

    // A once-only clip ends on its last frame and says so once.
    app.world_mut()
        .get_mut::<SpriteAnimation>(entity)
        .unwrap()
        .play("hit");
    run(&mut app, 30);
    assert_eq!(source_x(&app, entity), 32.0);
    let animation = app.world().get::<SpriteAnimation>(entity).unwrap();
    assert!(animation.is_finished());
    assert_eq!(app.world().resource::<Heard>().finished, ["hit"]);

    // Speed 2 runs twice as fast; paused stops.
    {
        let mut animation = app.world_mut().get_mut::<SpriteAnimation>(entity).unwrap();
        animation.play("wave");
        animation.speed = 2.0;
    }
    run(&mut app, 1 + 3);
    assert_eq!(source_x(&app, entity), 16.0, "frame 1 after half the time");
    app.world_mut()
        .get_mut::<SpriteAnimation>(entity)
        .unwrap()
        .paused = true;
    run(&mut app, 20);
    assert_eq!(source_x(&app, entity), 16.0);
}

fn handle_image(app: &App, sheet: &Handle<SpriteSheet>) -> Handle<cg_render::Image> {
    app.world()
        .resource::<Assets<SpriteSheet>>()
        .get(sheet)
        .unwrap()
        .image
        .clone()
}

#[test]
fn flipped_trimmed_frames_turn_round_on_the_spot() {
    let mut app = app(100);
    let mut trimmed = SpriteSheet::new(Handle::default());
    trimmed.add_frame(
        "f",
        SheetFrame {
            rect: Rect::new(0.0, 0.0, 10.0, 20.0),
            source_size: vec2(32.0, 32.0),
            offset: vec2(4.0, 12.0),
            duration: 0.1,
        },
    );
    trimmed.add_clip(
        "still",
        AnimationClip::new([ClipFrame {
            frame: 0,
            duration: 0.1,
        }]),
    );
    let handle = app
        .world_mut()
        .resource_mut::<Assets<SpriteSheet>>()
        .add(trimmed);
    let entity = app
        .world_mut()
        .spawn(SpriteAnimation::new(handle, "still").with_pivot(FEET))
        .id();
    run(&mut app, 1);
    // The feet (16, 0) of the untrimmed frame are at x 12, y 0 of the trimmed box.
    assert_eq!(
        app.world().get::<Sprite>(entity).unwrap().pivot,
        vec2(1.2, 0.0)
    );
    app.world_mut().get_mut::<Sprite>(entity).unwrap().flip_x = true;
    run(&mut app, 1);
    let pivot = app.world().get::<Sprite>(entity).unwrap().pivot;
    assert!((pivot - vec2(-0.2, 0.0)).length() < 1e-6, "{pivot}");
}

/// The graph the example uses, built in code.
fn hero_graph() -> AnimationGraph {
    AnimationGraph::new("idle")
        .with_state(AnimationState::new("idle").with_clip("walk"))
        .with_state(
            AnimationState::new("run")
                .with_clip("walk")
                .scaled_by("speed", 0.01),
        )
        .with_state(AnimationState::new("land").with_clip("hit"))
        .with_transition(Transition::to("land").when("landed", Test::Trigger))
        .with_transition(
            Transition::to("run")
                .from(["idle"])
                .when("speed", Test::Gt(5.0)),
        )
        .with_transition(
            Transition::to("idle")
                .from(["run"])
                .when("speed", Test::Le(5.0)),
        )
        .with_transition(Transition::to("idle").from(["land"]).after_clip())
}

#[test]
fn animators_follow_their_parameters() {
    let mut app = listening(100);
    let sheet = app
        .world_mut()
        .resource_mut::<Assets<SpriteSheet>>()
        .add(sheet());
    let graph = app
        .world_mut()
        .resource_mut::<Assets<AnimationGraph>>()
        .add(hero_graph());
    let entity = app
        .world_mut()
        .spawn((SpriteAnimation::new(sheet, "walk"), Animator::new(graph)))
        .id();
    let state = |app: &App| {
        app.world()
            .get::<Animator>(entity)
            .unwrap()
            .state()
            .map(str::to_owned)
    };
    run(&mut app, 1);
    assert_eq!(state(&app).as_deref(), Some("idle"));

    app.world_mut()
        .get_mut::<Animator>(entity)
        .unwrap()
        .set("speed", 200.0);
    run(&mut app, 1);
    assert_eq!(state(&app).as_deref(), Some("run"));
    let speed = app.world().get::<SpriteAnimation>(entity).unwrap().speed;
    assert_eq!(speed, 2.0, "the run plays faster at a higher speed");

    // A trigger wins from any state, then lasts only one step.
    app.world_mut()
        .get_mut::<Animator>(entity)
        .unwrap()
        .trigger("landed");
    run(&mut app, 1);
    assert_eq!(state(&app).as_deref(), Some("land"));
    // `land` waits for its clip (two frames, 0.2 s), then goes idle and on to run, since
    // the speed is still high: two transitions in one step.
    run(&mut app, 10);
    assert_eq!(state(&app).as_deref(), Some("land"));
    run(&mut app, 3);
    assert_eq!(state(&app).as_deref(), Some("run"));
    assert_eq!(
        app.world().resource::<Heard>().states,
        ["idle", "run", "land", "idle", "run"]
    );

    // Forcing a state skips the transitions.
    app.world_mut()
        .get_mut::<Animator>(entity)
        .unwrap()
        .force("land");
    run(&mut app, 1);
    assert_eq!(state(&app).as_deref(), Some("land"));
}

#[test]
fn tweens_run_on_the_right_clock_and_say_when_they_finish() {
    let mut app = listening(100);
    let mover = app
        .world_mut()
        .spawn((
            Transform2D::IDENTITY,
            Interpolated::default(),
            Sprite::default(),
        ))
        .id();
    let camera = app.world_mut().spawn(Camera2D::default()).id();
    app.world_mut()
        .commands()
        .entity(mover)
        .tween(Tween::translation(vec2(60.0, 0.0), 1.0).tag("moved"))
        .tween(Tween::flash(Color::WHITE, 0.5));
    app.world_mut().commands().entity(camera).tween(
        Tween::zoom(2.0, 1.0)
            .from(1.0)
            .ease(Ease::QuadInOut)
            .then(Tween::zoom(1.0, 1.0)),
    );
    app.world_mut().flush();

    // Half a second as one big frame of 30 fixed steps: the transform tween advances
    // step by step, the others by the frame.
    let half = step(&app) * 30;
    app.world_mut()
        .resource_mut::<Time>()
        .set_max_fixed_steps(100);
    app.world_mut()
        .resource_mut::<Time>()
        .set_max_delta(Duration::from_secs(1));
    app.update(half);
    let x = app.world().get::<Transform2D>(mover).unwrap().translation.x;
    assert!((x - 30.0).abs() < 1e-3, "{x}");
    let flash = app.world().get::<Sprite>(mover).unwrap().flash;
    assert_eq!(flash.a, 0.0, "the flash has faded");
    let zoom = app.world().get::<Camera2D>(camera).unwrap().zoom;
    assert!((zoom - 1.5).abs() < 1e-4, "{zoom}");

    app.update(half);
    app.update(half);
    let zoom = app.world().get::<Camera2D>(camera).unwrap().zoom;
    assert!((zoom - 1.5).abs() < 1e-4, "half way back: {zoom}");
    app.update(half);
    let world = app.world();
    assert_eq!(world.get::<Transform2D>(mover).unwrap().translation.x, 60.0);
    assert_eq!(world.get::<Camera2D>(camera).unwrap().zoom, 1.0);
    assert!(world.get::<Tweens>(mover).unwrap().is_empty());
    let mut heard = world.resource::<Heard>().tweens.clone();
    heard.sort();
    assert_eq!(heard, [None, None, None, Some("moved".to_owned())]);
}

#[test]
fn the_particle_budget_holds_across_emitters() {
    let mut app = app(500);
    let settings = ParticleSettings {
        rate: 0.0,
        burst: 400,
        lifetime: (10.0, 10.0),
        max: 10_000,
        ..ParticleSettings::default()
    };
    let a = app
        .world_mut()
        .spawn(ParticleEmitter::new(settings.clone()))
        .id();
    let b = app.world_mut().spawn(ParticleEmitter::new(settings)).id();
    run(&mut app, 2);
    let count = |e| app.world().get::<ParticleEmitter>(e).unwrap().len();
    assert_eq!(count(a) + count(b), 500);
    let budget = app.world().resource::<ParticleBudget>();
    assert_eq!(budget.live(), 500);
    assert_eq!(budget.dropped(), 300);
}

#[test]
fn one_shot_emitters_despawn_when_done() {
    let mut app = app(1000);
    let puff = app
        .world_mut()
        .spawn(
            ParticleEmitter::new(ParticleSettings {
                rate: 0.0,
                burst: 8,
                lifetime: (0.2, 0.3),
                ..ParticleSettings::default()
            })
            .once(),
        )
        .id();
    run(&mut app, 2);
    assert_eq!(app.world().get::<ParticleEmitter>(puff).unwrap().len(), 8);
    run(&mut app, 30);
    assert!(
        app.world().get_entity(puff).is_err(),
        "gone once its particles died"
    );
}

#[test]
fn the_camera_follows_inside_the_bounds() {
    let mut app = app(100);
    let target = app
        .world_mut()
        .spawn(Transform2D::from_xy(100.0, 100.0))
        .id();
    let camera = app
        .world_mut()
        .spawn((
            Camera2D::default(),
            Transform2D::IDENTITY,
            CameraFollow::new(target)
                .with_dead_zone(Vec2::ZERO)
                .with_smoothing(0.0)
                .with_bounds(Rect::new(0.0, 0.0, 2000.0, 400.0)),
        ))
        .id();
    run(&mut app, 2);
    let at = |app: &App| app.world().get::<Transform2D>(camera).unwrap().translation;
    // The default camera shows 640×360, so it stops 320 from the left and 180 from the
    // bottom.
    assert_eq!(at(&app), vec2(320.0, 180.0));
    app.world_mut()
        .get_mut::<Transform2D>(target)
        .unwrap()
        .translation = vec2(1000.0, 300.0);
    run(&mut app, 2);
    assert_eq!(at(&app), vec2(1000.0, 220.0));
}

#[derive(Clone, PartialEq, Eq, Hash, Debug)]
enum Screen {
    Title,
    Playing,
}

#[test]
fn state_fades_change_state_while_covered() {
    let mut app = app(100);
    app.init_state(Screen::Title).add_state_fade::<Screen>();
    {
        let mut fade = app.world_mut().resource_mut::<StateFade<Screen>>();
        fade.out_seconds = 0.1;
        fade.in_seconds = 0.1;
        fade.set(Screen::Playing);
    }
    let frame = Duration::from_millis(20);
    let mut covered_when_changed = None;
    let mut frames = 0;
    while *app.world().resource::<State<Screen>>().current() == Screen::Title {
        let alpha = app.world().resource::<ScreenFade>().alpha();
        app.update(frame);
        frames += 1;
        assert!(frames < 50, "the state never changed");
        if *app.world().resource::<State<Screen>>().current() == Screen::Playing {
            covered_when_changed = Some(alpha);
        }
    }
    assert_eq!(
        covered_when_changed,
        Some(1.0),
        "covered when the state changed"
    );
    for _ in 0..20 {
        app.update(frame);
    }
    assert_eq!(
        app.world().resource::<ScreenFade>().alpha(),
        0.0,
        "faded back in"
    );
    assert!(!app.world().resource::<StateFade<Screen>>().is_busy());
}

#[test]
fn shake_moves_the_view_not_the_camera() {
    use cg_render::FrameView;
    let mut app = app(100);
    let camera = app
        .world_mut()
        .spawn((
            Camera2D::default(),
            Transform2D::from_xy(50.0, 50.0),
            CameraShake {
                trauma: 1.0,
                decay: 0.0,
                ..CameraShake::default()
            },
        ))
        .id();
    let mut moved = false;
    for _ in 0..10 {
        run(&mut app, 1);
        let view = &app.world().resource::<FrameView>().0;
        moved |= view.position != vec2(50.0, 50.0);
        assert!((view.position - vec2(50.0, 50.0)).abs().max_element() <= 10.0 + 1e-3);
    }
    assert!(moved, "the view shook");
    assert_eq!(
        app.world().get::<Transform2D>(camera).unwrap().translation,
        vec2(50.0, 50.0)
    );
}
