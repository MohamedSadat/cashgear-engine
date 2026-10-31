//! The HUD over a level: hearts and coins at the top left, the level's name and time at
//! the top right, the level's name in large letters for a moment at the start, and the
//! touch controls.

use crate::level::LEVELS;
use crate::menus::{GOLD, INK};
use crate::progress::{MAX_HEARTS, Session, write_time};
use crate::{Art, Screen, tile_rect};
use cashgear_engine::prelude::*;
use cashgear_engine::ui::DEFAULT_FONT_SIZE;

const HEART_FULL: u32 = 44;
const HEART_EMPTY: u32 = 46;
const COIN: u32 = 151;
// Touch controls in `images/ui.png`.
const DPAD: Rect = Rect::new(0.0, 40.0, 48.0, 88.0);
const JUMP_BUTTON: Rect = Rect::new(48.0, 40.0, 80.0, 72.0);
const PAUSE_BUTTON: Rect = Rect::new(80.0, 40.0, 104.0, 64.0);
/// Seconds the level's name shows at the start, the last of them fading.
const CARD: f32 = 2.5;
const CARD_FADE: f32 = 0.8;

pub(crate) fn plugin(app: &mut App) {
    app.add_systems(OnEnter(Screen::Playing), spawn_hud)
        .add_systems(
            Update,
            (update_hud, fade_card).run_if(state_in_stack(Screen::Playing)),
        );
}

#[derive(Component)]
struct Heart(u32);

#[derive(Component)]
struct CoinCount;

#[derive(Component)]
struct Clock;

/// The level's name at the start, and how long it has shown.
#[derive(Component)]
struct Card(f32);

fn spawn_hud(mut commands: Commands, art: Res<Art>, session: Res<Session>) {
    let root = commands
        .spawn((
            Name::new("hud"),
            Node {
                padding: Edges::all(8.0),
                ..Node::fill()
            },
            StateScoped(Screen::Playing),
        ))
        .id();
    // Hearts and coins in the corner where lines start.
    let corner = commands
        .spawn((
            Node {
                layout: Layout::Column,
                gap: 3.0,
                ..default()
            },
            ChildOf(root),
        ))
        .id();
    let row = |commands: &mut Commands, parent: Entity| {
        commands
            .spawn((
                Node {
                    layout: Layout::Row,
                    gap: 2.0,
                    align: Align::Center,
                    ..default()
                },
                ChildOf(parent),
            ))
            .id()
    };
    let hearts = row(&mut commands, corner);
    for i in 0..MAX_HEARTS {
        commands.spawn((
            Name::new(format!("heart {i}")),
            Node::sized(18.0, 18.0),
            UiImage::new(art.tiles.clone()).with_source(tile_rect(HEART_FULL)),
            Heart(i),
            ChildOf(hearts),
        ));
    }
    let coins = row(&mut commands, corner);
    commands.spawn((
        Node::sized(18.0, 18.0),
        UiImage::new(art.tiles.clone()).with_source(tile_rect(COIN)),
        ChildOf(coins),
    ));
    commands.spawn((
        Text::new("0/0")
            .with_font(art.digits.clone())
            .with_size(18.0),
        CoinCount,
        ChildOf(coins),
    ));
    // The level and the time in the other corner.
    let other = commands
        .spawn((
            Node {
                anchor: Anchor::TopRight,
                layout: Layout::Column,
                gap: 3.0,
                align: Align::End,
                ..default()
            },
            ChildOf(root),
        ))
        .id();
    commands.spawn((
        Text::default().with_color(Color::hex(INK)),
        Localized::new(LEVELS[session.level].name),
        ChildOf(other),
    ));
    commands.spawn((
        Text::new("00:00.0")
            .with_font(art.digits.clone())
            .with_size(18.0),
        Clock,
        ChildOf(other),
    ));
    // The level's name, large, for a moment.
    let card = commands
        .spawn((
            Name::new("level card"),
            Node::default()
                .at(Anchor::Center)
                .with_offset(vec2(0.0, -40.0)),
            Card(0.0),
            ChildOf(root),
        ))
        .id();
    for (offset, color) in [(vec2(2.0, 2.0), INK), (Vec2::ZERO, GOLD)] {
        commands.spawn((
            Node::default().with_offset(offset),
            Text::default()
                .with_size(DEFAULT_FONT_SIZE * 3.0)
                .with_color(Color::hex(color)),
            Localized::new(LEVELS[session.level].name),
            ChildOf(card),
        ));
    }
    // Moving stays on the left and jumping on the right in every language; pause is in the
    // top corner.
    let thumbs = commands
        .spawn((
            Name::new("touch controls"),
            Node {
                padding: Edges::all(14.0),
                direction: LayoutDirection::LeftToRight,
                ..Node::fill()
            },
            ZIndex(1),
            StateScoped(Screen::Playing),
        ))
        .id();
    commands.spawn((
        Name::new("touch d-pad"),
        Node::sized(48.0, 48.0).at(Anchor::BottomLeft),
        UiImage::new(art.ui.clone()).with_source(DPAD),
        TouchDpad {
            up: String::new(),
            ..default()
        },
        ChildOf(thumbs),
    ));
    commands.spawn((
        Name::new("touch jump"),
        Node::sized(32.0, 32.0).at(Anchor::BottomRight),
        UiImage::new(art.ui.clone()).with_source(JUMP_BUTTON),
        TouchButton::new("jump"),
        ChildOf(thumbs),
    ));
    commands.spawn((
        Name::new("touch pause"),
        Node::sized(24.0, 24.0)
            .at(Anchor::Top)
            .with_offset(vec2(0.0, -6.0)),
        UiImage::new(art.ui.clone()).with_source(PAUSE_BUTTON),
        TouchButton::new("pause"),
        ChildOf(thumbs),
    ));
}

fn update_hud(
    session: Res<Session>,
    mut hearts: Query<(&Heart, &mut UiImage)>,
    mut coins: Query<&mut Text, (With<CoinCount>, Without<Clock>)>,
    mut clocks: Query<&mut Text, (With<Clock>, Without<CoinCount>)>,
    mut scratch: Local<String>,
) {
    use core::fmt::Write;
    for (heart, mut image) in &mut hearts {
        let tile = if heart.0 < session.hearts {
            HEART_FULL
        } else {
            HEART_EMPTY
        };
        let source = Some(tile_rect(tile));
        if image.source != source {
            image.source = source;
        }
    }
    // Written into one string that keeps its memory, and copied only when it changed, so
    // the HUD allocates nothing in a steady frame.
    scratch.clear();
    let _ = write!(scratch, "{}/{}", session.coins, session.coins_total);
    for mut text in &mut coins {
        if text.value != *scratch {
            text.value.clone_from(&scratch);
        }
    }
    write_time(&mut scratch, session.finished.unwrap_or(session.time));
    for mut text in &mut clocks {
        if text.value != *scratch {
            text.value.clone_from(&scratch);
        }
    }
}

/// Fades the level's name out, then removes it.
fn fade_card(
    mut commands: Commands,
    time: Res<Time>,
    mut cards: Query<(Entity, &mut Card, &Children)>,
    mut texts: Query<&mut Text>,
) {
    for (entity, mut card, children) in &mut cards {
        card.0 += time.raw_delta_secs();
        let alpha = ((CARD - card.0) / CARD_FADE).clamp(0.0, 1.0);
        if alpha <= 0.0 {
            commands.entity(entity).despawn();
            continue;
        }
        for child in children.iter() {
            if let Ok(mut text) = texts.get_mut(child) {
                let color = text.style.color.with_alpha(alpha);
                if text.style.color != color {
                    text.style.color = color;
                }
            }
        }
    }
}
