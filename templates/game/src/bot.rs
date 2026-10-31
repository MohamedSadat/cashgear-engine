//! `--demo`: a bot plays the level, the way the test and CI check that the game still
//! works. It runs right and jumps at walls, gaps and spikes, looking with raycasts; at the
//! door the game exits with success, and a bot that is stuck or keeps dying exits with an
//! error. Its run depends only on the fixed steps, so it is the same on every machine.

use crate::level::Run;
use crate::player::{Intent, Player};
use crate::{GameSystems, Options, Screen};
use cashgear_engine::prelude::*;

/// Fixed steps the bot may take to reach the door.
pub const TIMEOUT: u32 = 60 * 60;
/// How far ahead the bot looks, from the middle of the player.
const LOOK_AHEAD: f32 = 16.0;

/// The bot's progress.
#[derive(Resource, Debug, Default)]
pub struct Bot {
    /// Fixed steps since it started.
    pub steps: u32,
    /// Frames on the current screen.
    frames: u32,
    /// Whether it reached the door.
    pub won: bool,
}

pub(crate) fn plugin(app: &mut App) {
    let demo = |options: Res<Options>| options.demo;
    app.init_resource::<Bot>()
        .add_systems(FixedUpdate, steer.in_set(GameSystems::Input).run_if(demo))
        .add_systems(Update, (menus, judge).chain().run_if(demo));
}

/// Runs right, jumping when a wall, a gap or spikes are just ahead.
fn steer(
    mut bot: ResMut<Bot>,
    mut intent: ResMut<Intent>,
    player: Single<(Entity, &GlobalTransform2D, &KinematicContacts), With<Player>>,
    space: SpatialQuery,
) {
    bot.steps += 1;
    let (entity, transform, contacts) = *player;
    let at = transform.translation();
    // A ray starting inside the player would hit the player first.
    let solid = QueryFilter::default().without_sensors().exclude(entity);
    let feet = at.y - crate::player::SIZE.y / 2.0;
    let ahead = vec2(at.x + LOOK_AHEAD, feet);
    let wall = space
        .raycast(at, Vec2::X, LOOK_AHEAD, solid)
        .is_some();
    let gap = space
        .raycast(ahead + vec2(0.0, 4.0), Vec2::NEG_Y, 40.0, solid)
        .is_none();
    let spikes = (0..3).any(|i| {
        space
            .tile_at(ahead + vec2(i as f32 * 8.0, 4.0), solid)
            .is_some_and(|hit| hit.tile.hazard)
    });
    let jump = contacts.on_ground() && (wall || gap || spikes);
    *intent = Intent {
        run: 1.0,
        jump,
        // Held a while, for full-height jumps.
        jump_held: jump || !contacts.on_ground(),
    };
}

/// Presses Play on the title and on the win screen's first visit.
fn menus(
    mut bot: ResMut<Bot>,
    state: Res<State<Screen>>,
    mut next: ResMut<NextState<Screen>>,
    mut time: ResMut<Time>,
    mut last: Local<Option<Screen>>,
) {
    let screen = *state.current();
    if *last != Some(screen) {
        *last = Some(screen);
        bot.frames = 0;
    }
    bot.frames += 1;
    if screen == Screen::Title && bot.frames == 30 {
        next.set(Screen::Playing);
    }
    if screen == Screen::Won {
        bot.won = true;
        time.set_time_scale(1.0);
    }
}

/// Exits when the bot has won, is out of time or keeps falling.
fn judge(bot: Res<Bot>, run: Res<Run>, mut exit: MessageWriter<AppExit>, mut done: Local<bool>) {
    if *done {
        return;
    }
    let verdict = if bot.won && bot.frames >= 60 {
        info!(
            "demo: the bot reached the door with {}/{} coins after {:.1} s",
            run.coins,
            run.coins_total,
            bot.steps as f32 / 60.0
        );
        Some(AppExit::Success)
    } else if bot.steps > TIMEOUT || run.deaths > 3 {
        error!(
            "demo: the bot failed ({} steps, {} falls)",
            bot.steps, run.deaths
        );
        Some(AppExit::error())
    } else {
        None
    };
    if let Some(verdict) = verdict {
        *done = true;
        exit.write(verdict);
    }
}
