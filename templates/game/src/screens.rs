//! The camera and the screens: the title, the HUD (with on-screen controls on touch
//! screens) and the win screen. UI is made of entities with a `Node`, laid out in the
//! camera's 640x360 virtual pixels and scaled with the game.

use crate::level::{LevelAssets, Run};
use crate::player::Player;
use crate::Screen;
use cashgear_engine::prelude::*;
use cashgear_engine::ui::DEFAULT_FONT_SIZE;

const SKY: u32 = 0x8fd3ff;
const INK: u32 = 0x1a1c2c;
const GOLD: u32 = 0xffcd75;
/// The on-screen controls in `images/ui.png`.
const DPAD: Rect = Rect::new(0.0, 40.0, 48.0, 88.0);
const JUMP_BUTTON: Rect = Rect::new(48.0, 40.0, 80.0, 72.0);

/// What a button does.
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
enum Choice {
    Play,
    Title,
}

/// The HUD's coin count.
#[derive(Component)]
struct CoinCount;

pub(crate) fn plugin(app: &mut App) {
    app.add_systems(Startup, spawn_camera)
        .add_systems(OnEnter(Screen::Title), spawn_title)
        .add_systems(OnEnter(Screen::Playing), spawn_hud)
        .add_systems(OnExit(Screen::Playing), stop_following)
        .add_systems(OnEnter(Screen::Won), spawn_won)
        .add_systems(
            Update,
            (follow_player, update_hud.run_if(in_state(Screen::Playing)), choose),
        );
}

fn spawn_camera(mut commands: Commands) {
    commands.spawn((
        Name::new("camera"),
        Camera2D {
            clear_color: Color::hex(SKY),
            ..default()
        },
        Transform2D::from_xy(320.0, 180.0),
        Interpolated::default(),
    ));
}

/// A button with a label.
fn button(parent: Entity, label: &str, choice: Choice, first: bool) -> impl Bundle {
    (
        Node {
            padding: Edges::axes(10.0, 4.0),
            ..default()
        },
        Button::default(),
        Focusable { auto_focus: first },
        // `StateColors` tints the background as the button is hovered, focused or pressed.
        BackgroundColor(Color::hex(0x3b5dc9)),
        StateColors {
            normal: Color::hex(0x3b5dc9),
            hovered: Color::hex(0x41a6f6),
            pressed: Color::hex(0x29366f),
            disabled: Color::hex(0x566c86),
        },
        Text::new(label),
        choice,
        ChildOf(parent),
    )
}

fn panel(commands: &mut Commands, screen: Screen) -> Entity {
    commands
        .spawn((
            Node {
                anchor: Anchor::Center,
                layout: Layout::Column,
                align: Align::Center,
                gap: 8.0,
                padding: Edges::all(12.0),
                ..default()
            },
            BackgroundColor(Color::hex(INK).with_alpha(0.85)),
            StateScoped(screen),
        ))
        .id()
}

// ANCHOR: title
fn spawn_title(mut commands: Commands) {
    let panel = panel(&mut commands, Screen::Title);
    commands.spawn((
        Text::new("{{title}}")
            .with_size(DEFAULT_FONT_SIZE * 3.0)
            .with_color(Color::hex(GOLD)),
        Node::default(),
        ChildOf(panel),
    ));
    commands.spawn(button(panel, "Play", Choice::Play, true));
    commands.spawn((
        Text::new("Arrows or A/D to run, Space to jump").with_color(Color::hex(0xa0a6b8)),
        Node::default(),
        ChildOf(panel),
    ));
}
// ANCHOR_END: title

// ANCHOR: hud
fn spawn_hud(mut commands: Commands, server: Res<AssetServer>) {
    commands.spawn((
        Name::new("coins"),
        CoinCount,
        Text::new("").with_size(DEFAULT_FONT_SIZE * 2.0),
        Node::default()
            .at(Anchor::TopLeft)
            .with_offset(vec2(10.0, 8.0)),
        StateScoped(Screen::Playing),
    ));
    // Shown on touch screens only (and with `--touch`): moving on the left, jumping on
    // the right. They press the same actions the keys do.
    let ui = server.load("images/ui.png");
    let thumbs = commands
        .spawn((
            Name::new("touch controls"),
            Node {
                padding: Edges::all(14.0),
                ..Node::fill()
            },
            StateScoped(Screen::Playing),
        ))
        .id();
    commands.spawn((
        Node::sized(48.0, 48.0).at(Anchor::BottomLeft),
        UiImage::new(ui.clone()).with_source(DPAD),
        TouchDpad {
            up: String::new(),
            down: String::new(),
            ..default()
        },
        ChildOf(thumbs),
    ));
    commands.spawn((
        Node::sized(32.0, 32.0).at(Anchor::BottomRight),
        UiImage::new(ui).with_source(JUMP_BUTTON),
        TouchButton::new("jump"),
        ChildOf(thumbs),
    ));
}
// ANCHOR_END: hud

fn update_hud(
    run: Res<Run>,
    mut counts: Query<&mut Text, With<CoinCount>>,
    mut scratch: Local<String>,
) {
    use core::fmt::Write;
    // Into one string that keeps its memory, copied only when it changed: a steady frame
    // allocates nothing.
    scratch.clear();
    let _ = write!(scratch, "Coins {}/{}", run.coins, run.coins_total);
    for mut text in &mut counts {
        if text.value != *scratch {
            text.value.clone_from(&scratch);
        }
    }
}

/// The win screen, over the level, which stops while it shows.
fn spawn_won(mut commands: Commands, run: Res<Run>, mut time: ResMut<Time>) {
    time.set_time_scale(0.0);
    let panel = panel(&mut commands, Screen::Won);
    commands.spawn((
        Text::new("You made it!")
            .with_size(DEFAULT_FONT_SIZE * 2.0)
            .with_color(Color::hex(GOLD)),
        Node::default(),
        ChildOf(panel),
    ));
    commands.spawn((
        Text::new(format!("Coins {}/{}", run.coins, run.coins_total)),
        Node::default(),
        ChildOf(panel),
    ));
    commands.spawn(button(panel, "Play again", Choice::Play, true));
    commands.spawn(button(panel, "Title", Choice::Title, false));
}

/// Buttons: clicked, tapped, or chosen with the keys or a gamepad.
fn choose(
    mut clicks: MessageReader<Clicked>,
    choices: Query<&Choice>,
    mut next: ResMut<NextState<Screen>>,
    mut time: ResMut<Time>,
) {
    for click in clicks.read() {
        let Ok(choice) = choices.get(click.entity) else {
            continue;
        };
        time.set_time_scale(1.0);
        match choice {
            Choice::Play => next.set(Screen::Playing),
            Choice::Title => next.set(Screen::Title),
        }
    }
}

/// The camera follows a new player inside the level's edges.
fn follow_player(
    mut commands: Commands,
    players: Query<Entity, Added<Player>>,
    camera: Single<Entity, With<Camera2D>>,
    assets: Option<Res<LevelAssets>>,
    maps: Res<Assets<TiledMap>>,
) {
    for player in &players {
        let mut follow = CameraFollow::new(player)
            .with_dead_zone(vec2(48.0, 32.0))
            .with_smoothing(0.15);
        if let Some(map) = assets.as_ref().and_then(|a| maps.get(a.map.id())) {
            follow = follow.with_bounds(map.bounds());
        }
        commands.entity(*camera).insert(follow);
    }
}

fn stop_following(mut commands: Commands, camera: Single<Entity, With<Camera2D>>) {
    commands.entity(*camera).remove::<CameraFollow>();
}
