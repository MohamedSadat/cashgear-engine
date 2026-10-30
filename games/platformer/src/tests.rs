// The capture test writes files on desktop only.
#![allow(clippy::disallowed_methods)]

use super::*;
use crate::bot::{Bot, TIMEOUT};
use crate::controls::{Controls, Rebinding};
use crate::menus::{Clicks, MenuAction};
use crate::progress::{Progress, Session};
use cashgear_engine::audio::AudioOutput;
use cashgear_engine::platform::input::InputPlugin;
use cashgear_engine::render::{FrameView, OffscreenRenderer, RenderSystems};

/// The game without a window, with a mock audio device and memory storage, and Lua
/// scripts.
fn app(options: Options) -> App {
    app_with(options, true)
}

/// The game with the walkers and checkpoints in Lua (`scripts`) or in Rust.
fn app_with(options: Options, scripts: bool) -> App {
    let mut app = App::new();
    app.add_plugins(AssetPlugin {
        hot_reload: Some(false),
        ..default()
    })
    .add_plugins(InputPlugin { gamepads: false })
    .add_plugins(StoragePlugin {
        memory: true,
        ..default()
    })
    .add_plugins(TiledPlugin)
    .add_plugins(PhysicsPlugin::default())
    .add_plugins(AnimPlugin::default())
    .add_plugins(AudioPlugin {
        output: AudioOutput::Mock,
        settings_key: None,
        ..default()
    })
    .add_plugins(UiPlugin {
        locale: Some(LocaleSettings::new(&["en", "ar"])),
        ..default()
    })
    .add_plugins(ScriptPlugin {
        enabled: scripts,
        show_errors: false,
        ..default()
    })
    .add_plugins(Game { options })
    .init_resource::<Clicks>();
    app.insert_resource(Window::with_size(UVec2::new(1280, 720), 1.0));
    app
}

fn frame(app: &mut App) {
    app.update(core::time::Duration::from_millis(16));
}

fn screen(app: &App) -> Screen {
    *app.world().resource::<State<Screen>>().current()
}

#[test]
fn the_bot_plays_the_whole_game() {
    let mut app = app(Options {
        demo: true,
        level: None,
    });
    let mut exit = None;
    let mut reached = Vec::new();
    let trace = std::env::var("CG_BOT_TRACE")
        .ok()
        .and_then(|n| n.parse::<u64>().ok());
    for i in 0..TIMEOUT + 10 {
        frame(&mut app);
        if let Some(every) = trace
            && i % every == 0
        {
            trace_bot(&mut app);
        }
        let session = app.world().resource::<Session>();
        if session.finished.is_some() && !reached.contains(&session.level) {
            reached.push(session.level);
        }
        if let Some(code) = app.should_exit() {
            exit = Some(code);
            break;
        }
    }
    let bot = app.world().resource::<Bot>();
    assert_eq!(bot.failed, None);
    assert_eq!(exit, Some(AppExit::Success), "the bot stopped");
    assert_eq!(reached, [0, 1, 2]);
    assert_eq!(bot.levels_done, 3);
    assert_eq!(bot.game_overs, 0, "lost a level on the way");
    let progress = app.world().resource::<Progress>();
    assert_eq!(progress.unlocked, 3);
    for level in 0..3 {
        let best = progress.best(level).unwrap();
        assert!(best.coins > 0 && best.coins <= best.total, "{best:?}");
    }
    use MenuAction as M;
    let clicks = &app.world().resource::<Clicks>().0;
    assert_eq!(
        clicks[..5],
        [M::Settings, M::Controls, M::Play, M::Resume, M::Next]
    );
    assert_eq!(clicks.last(), Some(&M::Quit));
}

/// What a whole game by the bot looked like: every walker's position (by spawn order in
/// its level) and the checkpoint at every frame of play, and each level's result.
#[derive(Debug, Default, PartialEq)]
struct Run {
    frames: Vec<(usize, Vec<Vec2>, Option<Vec2>)>,
    levels: Vec<String>,
}

fn play_the_game(scripts: bool) -> Run {
    let mut app = app_with(
        Options {
            demo: true,
            level: None,
        },
        scripts,
    );
    let mut run = Run::default();
    let mut done = Vec::new();
    let mut exit = None;
    for _ in 0..TIMEOUT {
        frame(&mut app);
        if screen(&app) == Screen::Playing {
            let world = app.world_mut();
            let session = world.resource::<Session>().clone();
            let mut walkers: Vec<(u32, Vec2)> = world
                .query::<(Entity, &enemies::Enemy, &GlobalTransform2D)>()
                .iter(world)
                .filter(|(_, e, _)| e.kind == enemies::Kind::Walker)
                .map(|(entity, _, t)| (entity.index_u32(), t.translation()))
                .collect();
            walkers.sort_by_key(|(index, _)| *index);
            let at = walkers.into_iter().map(|(_, at)| at).collect();
            run.frames.push((session.level, at, session.checkpoint));
            if session.finished.is_some() && !done.contains(&session.level) {
                done.push(session.level);
                run.levels.push(format!(
                    "level {} in {:.3} s, {} of {} coins, {} hits, {} falls, {} stomps",
                    session.level + 1,
                    session.time,
                    session.coins,
                    session.coins_total,
                    session.hits,
                    session.falls,
                    session.stomps
                ));
            }
        }
        exit = app.should_exit();
        if exit.is_some() {
            break;
        }
    }
    assert_eq!(exit, Some(AppExit::Success), "scripts: {scripts}");
    let errors = app.world().resource::<ScriptErrors>();
    assert!(errors.is_empty(), "{:?}", errors.last());
    if scripts {
        let status = app
            .world()
            .resource::<cashgear_engine::script::ScriptStatus>();
        let paths: Vec<&str> = status.scripts.iter().map(|s| s.path.as_str()).collect();
        assert!(paths.contains(&"scripts/walker.lua"), "{paths:?}");
        assert!(paths.contains(&"scripts/checkpoint.lua"), "{paths:?}");
    }
    run
}

/// The Lua walkers and checkpoints play exactly like the Rust ones: the same walker
/// positions at every frame of the bot's whole game, the same checkpoints, the same
/// results.
#[test]
fn the_lua_walker_and_checkpoint_match_the_rust_ones() {
    let lua = play_the_game(true);
    let rust = play_the_game(false);
    assert_eq!(lua.levels.len(), 3, "{:?}", lua.levels);
    assert_eq!(lua.levels, rust.levels);
    assert_eq!(lua.frames.len(), rust.frames.len());
    let checkpoints = lua.frames.iter().filter_map(|(_, _, c)| *c).count();
    assert!(checkpoints > 0, "no checkpoint was reached");
    for (i, (a, b)) in lua.frames.iter().zip(&rust.frames).enumerate() {
        assert_eq!(a, b, "frame {i} of play: Lua then Rust");
    }
}

/// One line about the bot's run, for `CG_BOT_TRACE=<frames>`.
fn trace_bot(app: &mut App) {
    let world = app.world_mut();
    let session = world.resource::<Session>().clone();
    let stack = world.resource::<State<Screen>>().stack().to_vec();
    let mode = world.resource::<Bot>().mode;
    let player = world
        .query::<(
            &player::Player,
            &GlobalTransform2D,
            &KinematicBody,
            &KinematicContacts,
        )>()
        .iter(world)
        .next()
        .map(|(_, t, b, c)| (t.translation(), b.velocity, c.on_ground()));
    let near: Vec<String> = match player {
        Some((at, ..)) => world
            .query::<(&enemies::Enemy, &GlobalTransform2D)>()
            .iter(world)
            .filter(|(_, t)| (t.translation() - at).length() < 120.0)
            .map(|(e, t)| format!("{:?}@{:.0}", e.kind, t.translation()))
            .collect(),
        None => Vec::new(),
    };
    eprintln!(
        "{stack:?} level {} {mode:?} {player:?} near {near:?} hearts {} coins {}/{} hits {}          falls {} t {:.1}",
        session.level + 1,
        session.hearts,
        session.coins,
        session.coins_total,
        session.hits,
        session.falls,
        session.time
    );
}

#[test]
fn levels_load_with_their_objects() {
    for level in 0..level::LEVELS.len() {
        let mut app = app(Options {
            demo: false,
            level: None,
        });
        for _ in 0..5 {
            frame(&mut app);
        }
        assert_eq!(screen(&app), Screen::Title);
        app.world_mut().resource_mut::<Session>().level = level;
        app.world_mut()
            .resource_mut::<NextState<Screen>>()
            .set(Screen::Playing);
        for _ in 0..5 {
            frame(&mut app);
        }
        let world = app.world_mut();
        let session = world.resource::<Session>().clone();
        assert_eq!(session.level, level);
        assert!(session.coins_total >= 20, "level {level}: {session:?}");
        let players = world.query::<&player::Player>().iter(world).count();
        assert_eq!(players, 1, "level {level}");
        let enemies = world.query::<&enemies::Enemy>().iter(world).count();
        let platforms = world.query::<&things::Platform>().iter(world).count();
        let exits = world.query::<&things::Exit>().iter(world).count();
        assert!(enemies >= 4, "level {level}: {enemies} enemies");
        assert!(platforms >= 1, "level {level}: no platform");
        // Checkpoints and walkers run their scripts.
        let mut objects = world.query::<(&TiledObject, Has<Script>)>();
        let mut scripted = |class: &str| -> (usize, usize) {
            objects
                .iter(world)
                .filter(|(o, _)| o.class == class)
                .fold((0, 0), |(n, s), (_, scripted)| {
                    (n + 1, s + usize::from(scripted))
                })
        };
        let (checkpoints, scripted_checkpoints) = scripted("Checkpoint");
        let (walkers, scripted_walkers) = scripted("Walker");
        assert!(checkpoints >= 1 && exits == 1, "level {level}");
        assert_eq!(scripted_checkpoints, checkpoints, "level {level}");
        assert_eq!(scripted_walkers, walkers, "level {level}");
        assert_eq!(world.query::<&things::Checkpoint>().iter(world).count(), 0);
    }
}

#[test]
fn the_last_heart_ends_the_level_and_try_again_restarts_it() {
    let mut app = app(Options {
        demo: false,
        level: Some(1),
    });
    for _ in 0..10 {
        frame(&mut app);
    }
    assert_eq!(screen(&app), Screen::Playing);
    assert_eq!(app.world().resource::<Session>().level, 1);
    // One heart left, then a fall out of the level.
    app.world_mut().resource_mut::<Session>().hearts = 1;
    let world = app.world_mut();
    let mut players = world.query::<(&player::Player, &mut Transform2D)>();
    let (_, mut transform) = players.single_mut(world).unwrap();
    transform.translation.y = -200.0;
    for _ in 0..100 {
        frame(&mut app);
    }
    assert_eq!(screen(&app), Screen::GameOver);
    assert_eq!(app.world().resource::<Session>().hearts, 0);
    // "Try again" has the focus: the level starts over with full hearts.
    app.world_mut()
        .resource_mut::<VirtualInput>()
        .tap(cashgear_engine::ui::actions::DOWN);
    frame(&mut app);
    frame(&mut app);
    app.world_mut()
        .resource_mut::<VirtualInput>()
        .tap(cashgear_engine::ui::actions::ACCEPT);
    for _ in 0..60 {
        frame(&mut app);
    }
    assert_eq!(screen(&app), Screen::Playing);
    let session = app.world().resource::<Session>();
    assert_eq!((session.level, session.hearts, session.falls), (1, 3, 0));
}

#[test]
fn looking_away_pauses_a_level_but_not_the_demo() {
    for demo in [false, true] {
        let mut app = app(Options {
            demo,
            level: Some(0),
        });
        for _ in 0..10 {
            frame(&mut app);
        }
        assert_eq!(screen(&app), Screen::Playing);
        // A hidden browser tab, then the player comes back.
        app.world_mut().write_message(Lifecycle::Occluded(true));
        app.world_mut().write_message(Lifecycle::Occluded(false));
        frame(&mut app);
        frame(&mut app);
        let expected = if demo {
            Screen::Playing
        } else {
            Screen::Paused
        };
        assert_eq!(screen(&app), expected, "demo: {demo}");
        assert_eq!(
            app.world().resource::<Time>().time_scale(),
            if demo { 1.0 } else { 0.0 }
        );
        // Losing the focus while paused changes nothing.
        app.world_mut().write_message(Lifecycle::Focused(false));
        frame(&mut app);
        assert_eq!(screen(&app), expected);
    }
}

#[test]
fn keys_can_be_rebound_and_escape_cancels() {
    let mut app = app(Options::default());
    for _ in 0..5 {
        frame(&mut app);
    }
    let press = |app: &mut App, key: KeyCode| {
        app.world_mut()
            .resource_mut::<ButtonInput<KeyCode>>()
            .press(key);
        frame(app);
        app.world_mut()
            .resource_mut::<ButtonInput<KeyCode>>()
            .release(key);
        frame(app);
    };
    // Jump's first key slot waits for a key; the press that opened it does not count.
    let mut focus = app.world_mut().resource_mut::<UiFocus>().clone();
    app.world_mut()
        .resource_mut::<Rebinding>()
        .start(2, 0, &mut focus);
    *app.world_mut().resource_mut::<UiFocus>() = focus;
    frame(&mut app);
    press(&mut app, KeyCode::KeyK);
    let controls = app.world().resource::<Controls>();
    assert_eq!(controls.get(2, 0), Some(KeyCode::KeyK.into()));
    let map = app.world().resource::<ActionMap>();
    assert!(map.bindings("jump").contains(&Binding::Key(KeyCode::KeyK)));
    assert!(!map.bindings("jump").contains(&Binding::Key(KeyCode::Space)));
    // The menus' own keys are still bound.
    assert!(
        !map.bindings(cashgear_engine::ui::actions::ACCEPT)
            .is_empty()
    );
    // Saved: a new run reads it back.
    let storage = app.world().resource::<Storage>().clone();
    let saved = storage.read("controls.ron").unwrap().unwrap();
    assert!(saved.contains("KeyK"), "{saved}");

    // Escape cancels a prompt and leaves the slot as it was.
    let mut focus = app.world().resource::<UiFocus>().clone();
    app.world_mut()
        .resource_mut::<Rebinding>()
        .start(0, 1, &mut focus);
    *app.world_mut().resource_mut::<UiFocus>() = focus;
    frame(&mut app);
    press(&mut app, KeyCode::Escape);
    // The menus wait a frame more, so Escape does not also leave the screen.
    assert!(app.world().resource::<Rebinding>().is_busy());
    frame(&mut app);
    assert_eq!(
        app.world().resource::<Controls>().get(0, 1),
        Some(KeyCode::KeyA.into())
    );
    assert!(!app.world().resource::<Rebinding>().is_busy());
    assert!(!app.world().resource::<UiFocus>().is_paused());
}

/// The queues of the frame being drawn, copied before the renderer empties them.
#[derive(Resource, Default)]
struct Snapshot {
    world: Vec<QueuedSprite>,
    screen: Vec<QueuedSprite>,
    view: CameraView,
}

fn snapshot(sprites: Res<Sprites>, view: Res<FrameView>, mut snapshot: ResMut<Snapshot>) {
    snapshot.world = sprites.queued().to_vec();
    snapshot.screen = sprites.screen_queued().to_vec();
    snapshot.view = view.0.clone();
}

/// Renders screens of the bot's run offscreen at 1280x720 and saves PNGs, for looking at
/// the game without a window: `cargo test -p platformer -- --ignored capture_screens`
/// writes them to `CG_CAPTURE_DIR` (default `target/platformer-captures`).
#[test]
#[ignore = "needs a GPU; writes PNGs for people to look at"]
fn capture_screens() {
    let dir = std::env::var("CG_CAPTURE_DIR").unwrap_or_else(|_| {
        concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../target/platformer-captures"
        )
        .into()
    });
    std::fs::create_dir_all(&dir).unwrap();
    let mut renderer = pollster::block_on(OffscreenRenderer::new()).expect("a GPU");
    let mut app = app(Options {
        demo: true,
        level: None,
    });
    app.init_resource::<Snapshot>().add_systems(
        Render,
        snapshot
            .after(RenderSystems::Queue)
            .before(RenderSystems::Draw),
    );
    // (name, screen, frames on it)
    let shots: &[(&str, Screen, u32)] = &[
        ("title", Screen::Title, 25),
        ("settings", Screen::Settings, 15),
        ("controls", Screen::Controls, 20),
        ("level1", Screen::Playing, 150),
        ("paused", Screen::Paused, 20),
        ("complete", Screen::Complete, 50),
        ("level2", Screen::Playing, 900),
        ("level3", Screen::Playing, 2400),
        ("victory", Screen::Victory, 60),
    ];
    let mut taken = 0;
    let mut current = (Screen::Loading, 0u32);
    let mut visits: Vec<(Screen, u32)> = Vec::new();
    for _ in 0..TIMEOUT {
        frame(&mut app);
        let now = screen(&app);
        if now != current.0 {
            current = (now, 0);
            match visits.iter_mut().find(|(s, _)| *s == now) {
                Some((_, n)) => *n += 1,
                None => visits.push((now, 1)),
            }
        }
        current.1 += 1;
        if taken < shots.len() {
            let (name, shot_screen, frames) = shots[taken];
            if now == shot_screen && current.1 == frames {
                let snapshot = app.world().resource::<Snapshot>();
                let mut sprites = Sprites::default();
                sprites.extend(snapshot.world.iter().copied());
                for sprite in &snapshot.screen {
                    sprites.push_screen(*sprite);
                }
                let image = renderer
                    .render(
                        UVec2::new(1280, 720),
                        &snapshot.view,
                        app.world().resource::<Assets<Image>>(),
                        &sprites,
                        &DebugShapes::default(),
                    )
                    .unwrap();
                std::fs::write(format!("{dir}/{name}.png"), image.to_png().unwrap()).unwrap();
                taken += 1;
            }
        }
        if app.should_exit().is_some() {
            break;
        }
    }
    assert_eq!(taken, shots.len());
}
