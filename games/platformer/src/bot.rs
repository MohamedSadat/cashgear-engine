//! The demo's bot (`--demo`): it clicks through the menus (settings and controls first),
//! then plays all three levels and quits. In a level it runs right and jumps by reflex at
//! walls, gaps, spikes and enemies, and follows hints placed in the maps (`BotHint`
//! objects): `jump` (jump here), `board` (wait at the edge for a moving platform and ride
//! it), `walljump` (climb the shaft by wall jumps) and `left` / `right` (change direction).
//! It fills the same per-step `Intent` the controls do, so its run depends only on the
//! number of fixed steps, never the frame rate. It exits with an error if it gets stuck or
//! loses a level.

use crate::enemies::Enemy;
use crate::level::BotHint;
use crate::menus::MenuAction;
use crate::player::{Intent, Player, SIZE};
use crate::progress::Session;
use crate::things::Platform;
use crate::{GameSystems, Screen};
use cashgear_engine::prelude::*;
use cashgear_engine::ui::actions;
use std::collections::{HashSet, VecDeque};

/// Frames between a tap and the next step, so taps stay separate presses.
const TAP_GAP: u32 = 6;
/// The bot gives up after this many frames: three levels at a few minutes each.
pub(crate) const TIMEOUT: u64 = 60 * 60 * 12;
/// Steps the jump button is held for a full jump.
const FULL_JUMP: u32 = 22;

pub(crate) fn plugin(app: &mut App) {
    let demo = |options: Res<crate::Options>| options.demo;
    app.init_resource::<Bot>()
        .add_systems(
            PreUpdate,
            drive_menus.run_if(demo).before(InputSystems::Actions),
        )
        .add_systems(FixedUpdate, play.in_set(GameSystems::Input).run_if(demo));
}

/// One step of a menu script.
#[derive(Debug, Clone, Copy)]
enum Step {
    Wait(u32),
    Tap(&'static str),
    /// Taps down until the button for this action has the focus (menus whose rows depend
    /// on the platform).
    DownTo(MenuAction),
}

/// What the bot is doing in a level.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub(crate) enum Mode {
    #[default]
    Run,
    /// Waiting at an edge for a platform to step onto.
    Board,
    /// On a moving platform, waiting for somewhere to step off.
    Ride,
    /// Climbing a shaft between two walls.
    WallJump,
}

#[derive(Resource, Debug, Default)]
pub(crate) struct Bot {
    pub frames: u64,
    steps: VecDeque<Step>,
    wait: u32,
    screen: Option<Screen>,
    /// How many times each screen was entered.
    visits: Vec<(Screen, u32)>,
    pub mode: Mode,
    dir: f32,
    /// Steps left holding jump.
    hold: u32,
    /// Hints the player is inside, so each acts once on the way in.
    inside: HashSet<Entity>,
    /// `jump` hints already jumped from since the player went in.
    jumped: HashSet<Entity>,
    pub levels_done: u32,
    pub game_overs: u32,
    pub failed: Option<String>,
    /// The furthest x reached in the level, and the frame it was reached.
    best_x: f32,
    best_at: u64,
}

impl Bot {
    fn visit(&mut self, screen: Screen) -> u32 {
        match self.visits.iter_mut().find(|(s, _)| *s == screen) {
            Some((_, n)) => {
                *n += 1;
                *n
            }
            None => {
                self.visits.push((screen, 1));
                1
            }
        }
    }
}

/// The menus: a short script for each screen as it opens.
#[allow(clippy::too_many_arguments)]
fn drive_menus(
    mut bot: ResMut<Bot>,
    mut input: ResMut<VirtualInput>,
    focus: Res<UiFocus>,
    buttons: Query<&MenuAction>,
    state: Res<State<Screen>>,
    session: Res<Session>,
    fade: Res<StateFade<Screen>>,
    mut exit: MessageWriter<AppExit>,
) {
    bot.frames += 1;
    if bot.frames > TIMEOUT {
        let message = format!(
            "the bot ran out of time in level {} at x {:.0} ({:?})",
            session.level + 1,
            bot.best_x,
            state.current()
        );
        error!("{message}");
        bot.failed = Some(message);
        exit.write(AppExit::error());
        return;
    }
    let screen = *state.current();
    if bot.screen != Some(screen) && !fade.is_busy() {
        bot.screen = Some(screen);
        let visit = bot.visit(screen);
        use Step::{DownTo, Tap, Wait};
        let script: Vec<Step> = match (screen, visit) {
            // Settings, then controls, then back, then play.
            (Screen::Title, 1) => vec![
                Wait(30),
                Tap(actions::DOWN), // Play: the first press focuses it.
                Tap(actions::DOWN), // Levels
                Tap(actions::DOWN), // Settings
                Tap(actions::ACCEPT),
            ],
            // Down through every row (Fullscreen only where the platform has it).
            (Screen::Settings, 1) => {
                vec![Wait(20), DownTo(MenuAction::Controls), Tap(actions::ACCEPT)]
            }
            (Screen::Controls, _) => vec![Wait(30), Tap(actions::BACK)],
            (Screen::Settings, _) => vec![Wait(10), Tap(actions::BACK)],
            (Screen::Title, 2) => vec![Wait(20), Tap(actions::ACCEPT)],
            // After the last level: Quit, round from Play.
            (Screen::Title, _) => vec![Wait(30), Tap(actions::UP), Tap(actions::ACCEPT)],
            // Pause once, early in the first level.
            (Screen::Playing, 1) => vec![Wait(200), Tap("pause")],
            (Screen::Paused, _) => vec![Wait(30), Tap(actions::ACCEPT)],
            (Screen::Complete, _) => {
                bot.levels_done += 1;
                vec![Wait(60), Tap(actions::ACCEPT)]
            }
            // The bot's run is the same every time, so trying again would lose again.
            (Screen::GameOver, _) => {
                bot.game_overs += 1;
                let message = format!("the bot lost level {}", session.level + 1);
                error!("{message}");
                bot.failed = Some(message);
                exit.write(AppExit::error());
                Vec::new()
            }
            (Screen::Victory, _) => vec![Wait(90), Tap(actions::ACCEPT)],
            _ => Vec::new(),
        };
        bot.steps = script.into();
        bot.wait = 0;
        if screen == Screen::Playing {
            bot.mode = Mode::Run;
            bot.dir = 1.0;
            bot.best_x = 0.0;
            bot.best_at = bot.frames;
            bot.inside.clear();
        }
    }
    if bot.wait > 0 {
        bot.wait -= 1;
        return;
    }
    let Some(step) = bot.steps.pop_front() else {
        return;
    };
    match step {
        Step::Wait(frames) => bot.wait = frames,
        Step::Tap(action) => {
            input.tap(action);
            bot.wait = TAP_GAP;
        }
        Step::DownTo(target) => {
            let focused = focus.get().and_then(|entity| buttons.get(entity).ok());
            if focused != Some(&target) {
                input.tap(actions::DOWN);
                bot.wait = TAP_GAP;
                bot.steps.push_front(step);
            }
        }
    }
}

type PlayerItem = (
    Entity,
    &'static Player,
    &'static KinematicContacts,
    &'static GlobalTransform2D,
);

/// The bot's hands in a level.
#[allow(clippy::too_many_arguments)]
fn play(
    mut bot: ResMut<Bot>,
    mut intent: ResMut<Intent>,
    player: Single<PlayerItem>,
    hints: Query<(Entity, &BotHint)>,
    platforms: Query<(&Collider, &GlobalTransform2D), With<Platform>>,
    enemies: Query<(&Enemy, &GlobalTransform2D)>,
    spatial: SpatialQuery,
) {
    let (_, player, contacts, transform) = *player;
    *intent = Intent::default();
    if player.dying > 0.0 || player.dead {
        bot.mode = Mode::Run;
        bot.dir = 1.0;
        bot.inside.clear();
        bot.jumped.clear();
        return;
    }
    let at = transform.translation();
    if at.x > bot.best_x + 4.0 {
        bot.best_x = at.x;
        bot.best_at = bot.frames;
    }
    let feet = at.y - SIZE.y / 2.0;
    let grounded = contacts.on_ground();
    let mut dir = if bot.dir == 0.0 { 1.0 } else { bot.dir };
    let front = at.x + dir * SIZE.x / 2.0;
    // The level only: tiles and static colliders, not the player or the enemies.
    let solid = QueryFilter::default()
        .without_sensors()
        .with_mask(CollisionLayers::DEFAULT.memberships);

    // Hints, as the player walks into them.
    let mut jump = false;
    for (hint_entity, hint) in &hints {
        let inside = hint.rect.contains(at);
        if !inside {
            bot.inside.remove(&hint_entity);
            bot.jumped.remove(&hint_entity);
            continue;
        }
        let entered = bot.inside.insert(hint_entity);
        match hint.action.as_str() {
            // Once per visit, as soon as the player stands inside.
            "jump" => {
                if grounded && bot.jumped.insert(hint_entity) {
                    jump = true;
                }
            }
            "board" if entered => bot.mode = Mode::Board,
            "walljump" if entered => bot.mode = Mode::WallJump,
            "right" if entered => {
                bot.mode = Mode::Run;
                dir = 1.0;
            }
            "left" if entered => {
                bot.mode = Mode::Run;
                dir = -1.0;
            }
            _ => {}
        }
    }

    // What is around.
    let wall_ahead = spatial
        .raycast(vec2(front, feet + 4.0), vec2(dir, 0.0), 8.0, solid)
        .is_some_and(|hit| hit.normal.x * dir < -0.9);
    let floor_below = |x: f32, reach: f32| {
        spatial
            .raycast(vec2(x, feet + 2.0), Vec2::NEG_Y, reach, solid)
            .is_some()
    };
    let gap_ahead = !floor_below(front + dir * 10.0, 60.0);
    let spikes_ahead = spatial
        .tile_at(vec2(front + dir * 12.0, feet + 4.0), solid)
        .is_some_and(|hit| hit.tile.hazard);
    let enemy_ahead = enemies.iter().any(|(enemy, enemy_at)| {
        let d = enemy_at.translation() - at;
        !enemy.dead && d.y.abs() < 26.0 && d.x * dir > 0.0 && d.x * dir < 46.0
    });
    let on_platform = contacts
        .ground
        .filter(|ground| ground.cell.is_none())
        .and_then(|ground| {
            platforms
                .get(ground.entity)
                .ok()
                .map(|p| (ground.entity, p))
        });

    // Standing on a moving platform is riding it, whatever the bot was doing.
    if on_platform.is_some() && matches!(bot.mode, Mode::Run | Mode::Board) {
        bot.mode = Mode::Ride;
    }
    let mut run = dir;
    match bot.mode {
        Mode::Run => {
            if grounded && (wall_ahead || gap_ahead || spikes_ahead || enemy_ahead) {
                jump = true;
            }
        }
        Mode::Board => {
            if on_platform.is_some() {
                bot.mode = Mode::Ride;
                run = 0.0;
            } else if grounded && !floor_below(front + dir * 3.0, 6.0) {
                // At the edge: step on once a platform's top is level with the feet just
                // ahead.
                run = if landing_ahead(&spatial, front + dir * 8.0, feet, solid) {
                    dir
                } else {
                    0.0
                };
            }
        }
        Mode::Ride => match on_platform {
            Some((platform, (collider, platform_at))) => {
                let rect = collider.world_shape(platform_at.0).aabb();
                let edge = if dir > 0.0 { rect.max.x } else { rect.min.x };
                let filter = solid.exclude(platform);
                // Walk to the platform's leading edge, then wait for a landing.
                run = if (edge - front) * dir > 4.0
                    || landing_ahead(&spatial, edge + dir * 8.0, feet, filter)
                {
                    dir
                } else {
                    0.0
                };
            }
            None if grounded => bot.mode = Mode::Run,
            None => {}
        },
        Mode::WallJump => {
            let touching = (dir > 0.0 && contacts.wall_right.is_some())
                || (dir < 0.0 && contacts.wall_left.is_some())
                || player.sliding;
            if grounded && wall_ahead {
                jump = true;
            } else if !grounded && touching {
                jump = true;
                dir = -dir;
                run = dir;
            }
        }
    }
    if jump && (grounded || bot.mode == Mode::WallJump) {
        intent.jump = true;
        bot.hold = FULL_JUMP;
    }
    intent.jump_held = bot.hold > 0;
    bot.hold = bot.hold.saturating_sub(1);
    intent.run = run;
    bot.dir = dir;
}

/// Whether there is a surface at `x` level with `feet` or a little below: the top of the
/// ground or of a platform the player can step onto.
fn landing_ahead(spatial: &SpatialQuery, x: f32, feet: f32, filter: QueryFilter) -> bool {
    spatial
        .raycast(vec2(x, feet + 5.0), Vec2::NEG_Y, 10.0, filter)
        .is_some_and(|hit| hit.distance > 4.5 && hit.distance < 8.5)
}
