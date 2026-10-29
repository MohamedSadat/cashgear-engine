//! The menus: loading bar, title, level select, settings, controls, pause, level complete,
//! game over and the final screen. Pixel-art panels and buttons from `images/ui.png`,
//! navigable by keyboard, gamepad, mouse and touch, in English and Arabic.

use crate::controls::{self, ACTIONS, Controls, PAD_SLOT, Rebinding};
use crate::level::LEVELS;
use crate::progress::{Progress, Session, format_time};
use crate::{Art, Screen};
use cashgear_engine::prelude::*;
use cashgear_engine::ui::{DEFAULT_FONT_SIZE, actions};

pub(crate) const GOLD: u32 = 0xffcd75;
pub(crate) const INK: u32 = 0x1a1c2c;
const PALE: u32 = 0xf4f4f4;

// Parts of `images/ui.png`, in texels.
pub(crate) const PANEL: Rect = Rect::new(0.0, 0.0, 24.0, 24.0);
const BUTTON: Rect = Rect::new(24.0, 0.0, 48.0, 24.0);
const BUTTON_FOCUS: Rect = Rect::new(48.0, 0.0, 72.0, 24.0);
const BUTTON_PRESSED: Rect = Rect::new(72.0, 0.0, 96.0, 24.0);
const BUTTON_OFF: Rect = Rect::new(96.0, 0.0, 120.0, 24.0);
const TRACK: Rect = Rect::new(0.0, 24.0, 16.0, 32.0);
const GREEN: Rect = Rect::new(16.0, 24.0, 32.0, 32.0);
const THUMB: Rect = Rect::new(32.0, 24.0, 40.0, 38.0);
const THUMB_FOCUS: Rect = Rect::new(40.0, 24.0, 48.0, 38.0);
pub(crate) const PANEL_SLICE: Edges = Edges::all(4.0);
const BUTTON_SLICE: Edges = Edges {
    left: 4.0,
    right: 4.0,
    top: 4.0,
    bottom: 5.0,
};
const BAR_SLICE: Edges = Edges::all(3.0);

pub(crate) fn plugin(app: &mut App) {
    use Screen::*;
    app.add_systems(OnEnter(Title), (spawn_title, set_time_scale(1.0)))
        .add_systems(OnEnter(Levels), spawn_levels)
        .add_systems(OnEnter(Settings), spawn_settings)
        .add_systems(OnEnter(Controls), spawn_controls)
        .add_systems(OnEnter(Paused), (spawn_pause, set_time_scale(0.0)))
        .add_systems(OnEnter(Complete), (spawn_complete, set_time_scale(0.0)))
        .add_systems(OnEnter(GameOver), (spawn_game_over, set_time_scale(0.0)))
        .add_systems(OnEnter(Victory), (spawn_victory, set_time_scale(0.0)))
        .add_systems(OnResume(Playing), set_time_scale(1.0))
        .add_systems(
            Update,
            (
                on_click,
                on_slider,
                back_and_pause,
                update_labels,
                widget_looks,
                click_sounds,
                report,
            )
                .run_if(|state: Res<State<Screen>>| *state.current() != Loading),
        );
}

fn set_time_scale(scale: f32) -> impl FnMut(ResMut<Time>) {
    move |mut time: ResMut<Time>| time.set_time_scale(scale)
}

// --- Loading ---------------------------------------------------------------------------

#[derive(Component)]
pub(crate) struct LoadingBar;

/// A bar of plain colours: the UI sheet may not have loaded yet.
pub(crate) fn spawn_loading(mut commands: Commands) {
    let bar = commands
        .spawn((
            Name::new("loading"),
            Node::sized(160.0, 6.0).at(Anchor::Center),
            BackgroundColor(Color::hex(INK)),
            ProgressBar::default(),
            LoadingBar,
            StateScoped(Screen::Loading),
        ))
        .id();
    commands.spawn((
        Node {
            height: Val::Fill,
            ..default()
        },
        BackgroundColor(Color::hex(GOLD)),
        BarFill,
        ChildOf(bar),
    ));
}

pub(crate) fn show_loading(
    loading: Res<LoadingAssets>,
    server: Res<AssetServer>,
    mut bars: Query<&mut ProgressBar, With<LoadingBar>>,
) {
    let fraction = loading.progress(&server).fraction();
    for mut bar in &mut bars {
        bar.value = fraction;
    }
}

// --- Widgets ---------------------------------------------------------------------------

/// What a button does.
#[derive(Component, Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) enum MenuAction {
    Play,
    Levels,
    Settings,
    Language,
    Quit,
    Back,
    Start(usize),
    TouchMode,
    Controls,
    Rebind(usize, usize),
    ResetControls,
    Resume,
    Restart,
    MainMenu,
    Next,
}

/// A label whose text changes with the game: it is on the label's button.
#[derive(Component, Clone, Copy, PartialEq, Eq, Debug)]
enum Live {
    Language,
    TouchMode,
    Continue,
    Slot(usize, usize),
    LevelName(usize),
    LevelBest(usize),
}

/// Which setting a slider changes.
#[derive(Component, Clone, Copy, PartialEq, Eq, Debug)]
enum SettingSlider {
    Master,
    Music,
    Effects,
}

/// A full-screen root for a screen's UI.
fn screen_root(commands: &mut Commands, name: &str, screen: Screen, z: i32, dim: bool) -> Entity {
    let mut root = commands.spawn((
        Name::new(name.to_owned()),
        Node::fill(),
        ZIndex(z),
        StateScoped(screen),
    ));
    if dim {
        root.insert(BackgroundColor(Color::hex(INK).with_alpha(0.6)));
    }
    root.id()
}

/// A nine-slice panel stacking its children in a column.
fn panel(commands: &mut Commands, art: &Art, parent: Entity, width: f32, offset: Vec2) -> Entity {
    commands
        .spawn((
            Name::new("panel"),
            Node {
                anchor: Anchor::Center,
                offset,
                width: Val::Px(width),
                padding: Edges::all(9.0),
                layout: Layout::Column,
                gap: 5.0,
                align: Align::Stretch,
                ..default()
            },
            UiImage::new(art.ui.clone())
                .with_source(PANEL)
                .sliced(PANEL_SLICE),
            ChildOf(parent),
        ))
        .id()
}

/// A localized line of text in a colour, centred.
fn line(commands: &mut Commands, parent: Entity, key: &str, color: u32) -> Entity {
    commands
        .spawn((
            Node::default(),
            Text::default()
                .with_color(Color::hex(color))
                .with_align(TextAlign::Center),
            Localized::new(key),
            ChildOf(parent),
        ))
        .id()
}

/// A localized heading in gold.
fn heading(commands: &mut Commands, parent: Entity, key: &str) {
    let heading = line(commands, parent, key, GOLD);
    commands.entity(heading).insert(Node {
        padding: Edges {
            bottom: 3.0,
            ..Edges::ZERO
        },
        ..default()
    });
}

/// A button with a localized label.
fn button(
    commands: &mut Commands,
    art: &Art,
    parent: Entity,
    key: &str,
    action: MenuAction,
) -> Entity {
    let button = button_frame(commands, art, parent, 24.0, action);
    commands.spawn((
        Name::new("label"),
        Node::default()
            .at(Anchor::Center)
            .with_offset(vec2(0.0, -1.0)),
        Text::default(),
        Localized::new(key),
        ChildOf(button),
    ));
    button
}

fn button_frame(
    commands: &mut Commands,
    art: &Art,
    parent: Entity,
    height: f32,
    action: MenuAction,
) -> Entity {
    commands
        .spawn((
            Name::new(format!("button {action:?}")),
            Node {
                height: Val::Px(height),
                ..default()
            },
            Button::default(),
            UiImage::new(art.ui.clone())
                .with_source(BUTTON)
                .sliced(BUTTON_SLICE),
            StateImages {
                normal: Some(BUTTON),
                hovered: Some(BUTTON_FOCUS),
                pressed: Some(BUTTON_PRESSED),
                disabled: Some(BUTTON_OFF),
            },
            action,
            ChildOf(parent),
        ))
        .id()
}

/// A row with a label and a slider for a setting.
fn slider_row(
    commands: &mut Commands,
    art: &Art,
    parent: Entity,
    key: &str,
    setting: SettingSlider,
    value: f32,
) -> Entity {
    let row = commands
        .spawn((
            Node {
                height: Val::Px(18.0),
                layout: Layout::Row,
                gap: 8.0,
                align: Align::Center,
                ..default()
            },
            ChildOf(parent),
        ))
        .id();
    commands.spawn((
        Node {
            width: Val::Fill,
            ..default()
        },
        Text::default(),
        Localized::new(key),
        ChildOf(row),
    ));
    let slider = commands
        .spawn((
            Name::new(format!("slider {key}")),
            Node::sized(110.0, 8.0),
            Slider::new(value, 0.1),
            UiImage::new(art.ui.clone())
                .with_source(TRACK)
                .sliced(BAR_SLICE),
            setting,
            ChildOf(row),
        ))
        .id();
    commands.spawn((
        Node {
            height: Val::Fill,
            ..default()
        },
        UiImage::new(art.ui.clone())
            .with_source(GREEN)
            .sliced(BAR_SLICE),
        BarFill,
        ChildOf(slider),
    ));
    commands.spawn((
        Node::sized(8.0, 14.0),
        UiImage::new(art.ui.clone()).with_source(THUMB),
        SliderThumb,
        ChildOf(slider),
    ));
    slider
}

/// A row laid out left to right with a gap.
fn row(commands: &mut Commands, parent: Entity, height: f32, gap: f32) -> Entity {
    commands
        .spawn((
            Node {
                height: Val::Px(height),
                layout: Layout::Row,
                gap,
                align: Align::Center,
                ..default()
            },
            ChildOf(parent),
        ))
        .id()
}

// --- Screens ---------------------------------------------------------------------------

fn spawn_title(
    mut commands: Commands,
    art: Res<Art>,
    progress: Res<Progress>,
    options: Res<crate::Options>,
) {
    let root = screen_root(&mut commands, "title", Screen::Title, 0, false);
    // The title, with a shadow a pixel down and right.
    let title = commands
        .spawn((
            Name::new("title"),
            Node::default().at(Anchor::Top).with_offset(vec2(0.0, 22.0)),
            ChildOf(root),
        ))
        .id();
    for (offset, color) in [(vec2(2.0, 2.0), INK), (Vec2::ZERO, GOLD)] {
        commands.spawn((
            Node::default().with_offset(offset),
            Text::new("CashGear")
                .with_size(DEFAULT_FONT_SIZE * 4.0)
                .with_color(Color::hex(color)),
            ChildOf(title),
        ));
    }
    commands.spawn((
        Node::default().at(Anchor::Top).with_offset(vec2(0.0, 72.0)),
        Text::default()
            .with_size(DEFAULT_FONT_SIZE * 2.0)
            .with_color(Color::hex(INK)),
        Localized::new("subtitle"),
        ChildOf(root),
    ));
    let menu = panel(&mut commands, &art, root, 196.0, vec2(0.0, 42.0));
    let started = progress.unlocked > 0 || progress.best(0).is_some();
    let play = button(
        &mut commands,
        &art,
        menu,
        if started {
            "menu.continue"
        } else {
            "menu.play"
        },
        MenuAction::Play,
    );
    commands
        .entity(play)
        .insert((Focusable { auto_focus: true }, Live::Continue));
    button(&mut commands, &art, menu, "menu.levels", MenuAction::Levels);
    button(
        &mut commands,
        &art,
        menu,
        "menu.settings",
        MenuAction::Settings,
    );
    let language = button(
        &mut commands,
        &art,
        menu,
        "menu.language",
        MenuAction::Language,
    );
    commands.entity(language).insert(Live::Language);
    // There is nowhere to quit to in a browser.
    if Target::CURRENT.can_quit() || options.demo {
        button(&mut commands, &art, menu, "menu.quit", MenuAction::Quit);
    }
    commands.spawn((
        Node {
            anchor: Anchor::Bottom,
            offset: vec2(0.0, -6.0),
            width: Val::Px(560.0),
            ..default()
        },
        Text::default()
            .with_color(Color::hex(INK))
            .with_align(TextAlign::Center),
        Localized::new("menu.hint"),
        ChildOf(root),
    ));
}

fn spawn_levels(mut commands: Commands, art: Res<Art>, progress: Res<Progress>) {
    let root = screen_root(&mut commands, "levels", Screen::Levels, 10, true);
    let menu = panel(&mut commands, &art, root, 250.0, Vec2::ZERO);
    heading(&mut commands, menu, "levels.title");
    for (i, _) in LEVELS.iter().enumerate() {
        let button = button_frame(&mut commands, &art, menu, 36.0, MenuAction::Start(i));
        if !progress.is_unlocked(i) {
            commands.entity(button).insert(Button { disabled: true });
        }
        if i == progress.unlocked.min(LEVELS.len() - 1) {
            commands
                .entity(button)
                .insert(Focusable { auto_focus: true });
        }
        let lines = commands
            .spawn((
                Node {
                    anchor: Anchor::Center,
                    offset: vec2(0.0, -1.0),
                    layout: Layout::Column,
                    gap: 1.0,
                    align: Align::Center,
                    ..default()
                },
                ChildOf(button),
            ))
            .id();
        let name = commands
            .spawn((
                Node::default(),
                Text::default(),
                Localized::new("levels.entry"),
                ChildOf(lines),
            ))
            .id();
        commands.entity(name).insert(Live::LevelName(i));
        let best = commands
            .spawn((
                Node::default(),
                Text::default().with_color(Color::hex(GOLD)),
                Localized::new("levels.new"),
                ChildOf(lines),
            ))
            .id();
        commands.entity(best).insert(Live::LevelBest(i));
    }
    button(&mut commands, &art, menu, "back", MenuAction::Back);
}

fn spawn_settings(
    mut commands: Commands,
    art: Res<Art>,
    audio: Res<AudioSettings>,
    stack: Res<State<Screen>>,
) {
    // Over the pause menu the level shows through, dimmed; over the title too.
    let dim = stack.stack().len() > 1;
    let root = screen_root(&mut commands, "settings", Screen::Settings, 20, dim);
    let settings = panel(&mut commands, &art, root, 270.0, Vec2::ZERO);
    heading(&mut commands, settings, "settings.title");
    let master = slider_row(
        &mut commands,
        &art,
        settings,
        "settings.master",
        SettingSlider::Master,
        audio.master,
    );
    commands
        .entity(master)
        .insert(Focusable { auto_focus: true });
    slider_row(
        &mut commands,
        &art,
        settings,
        "settings.music",
        SettingSlider::Music,
        audio.level(Bus::Music),
    );
    slider_row(
        &mut commands,
        &art,
        settings,
        "settings.sfx",
        SettingSlider::Effects,
        audio.level(Bus::Sfx),
    );
    let language = button(
        &mut commands,
        &art,
        settings,
        "menu.language",
        MenuAction::Language,
    );
    commands.entity(language).insert(Live::Language);
    let touch = button(
        &mut commands,
        &art,
        settings,
        "settings.touch",
        MenuAction::TouchMode,
    );
    commands.entity(touch).insert(Live::TouchMode);
    button(
        &mut commands,
        &art,
        settings,
        "settings.controls",
        MenuAction::Controls,
    );
    button(&mut commands, &art, settings, "back", MenuAction::Back);
}

/// Each action's three slots: two keys and a gamepad button.
fn spawn_controls(mut commands: Commands, art: Res<Art>) {
    let root = screen_root(&mut commands, "controls", Screen::Controls, 30, true);
    let menu = panel(&mut commands, &art, root, 360.0, Vec2::ZERO);
    heading(&mut commands, menu, "controls.title");
    let header = row(&mut commands, menu, 12.0, 4.0);
    commands.spawn((
        Node {
            width: Val::Fill,
            ..default()
        },
        ChildOf(header),
    ));
    for (key, width) in [("controls.keys", 148.0), ("controls.pad", 88.0)] {
        commands.spawn((
            Node {
                width: Val::Px(width),
                ..default()
            },
            Text::default()
                .with_color(Color::hex(GOLD))
                .with_align(TextAlign::Center),
            Localized::new(key),
            ChildOf(header),
        ));
    }
    for (action, name) in ACTIONS.iter().enumerate() {
        let line = row(&mut commands, menu, 20.0, 4.0);
        commands.spawn((
            Node {
                width: Val::Fill,
                ..default()
            },
            Text::default(),
            Localized::new(format!("controls.{name}")),
            ChildOf(line),
        ));
        for slot in 0..controls::SLOTS {
            let width = if slot == PAD_SLOT { 88.0 } else { 72.0 };
            let button = button_frame(
                &mut commands,
                &art,
                line,
                20.0,
                MenuAction::Rebind(action, slot),
            );
            commands.entity(button).insert((
                Node {
                    width: Val::Px(width),
                    height: Val::Px(20.0),
                    ..default()
                },
                Live::Slot(action, slot),
            ));
            if action == 0 && slot == 0 {
                commands
                    .entity(button)
                    .insert(Focusable { auto_focus: true });
            }
            commands.spawn((
                Name::new("label"),
                Node::default()
                    .at(Anchor::Center)
                    .with_offset(vec2(0.0, -1.0)),
                Text::default(),
                Localized::new("controls.empty"),
                ChildOf(button),
            ));
        }
    }
    commands.spawn((
        Node::default(),
        Text::default()
            .with_color(Color::hex(PALE))
            .with_align(TextAlign::Center),
        Localized::new("controls.hint"),
        ChildOf(menu),
    ));
    let buttons = row(&mut commands, menu, 24.0, 6.0);
    for (key, action) in [
        ("controls.reset", MenuAction::ResetControls),
        ("back", MenuAction::Back),
    ] {
        let button = button(&mut commands, &art, buttons, key, action);
        commands.entity(button).insert(Node {
            width: Val::Fill,
            height: Val::Px(24.0),
            ..default()
        });
    }
}

fn spawn_pause(mut commands: Commands, art: Res<Art>) {
    let root = screen_root(&mut commands, "pause", Screen::Paused, 10, true);
    let menu = panel(&mut commands, &art, root, 176.0, Vec2::ZERO);
    heading(&mut commands, menu, "pause.title");
    let resume = button(
        &mut commands,
        &art,
        menu,
        "pause.resume",
        MenuAction::Resume,
    );
    commands
        .entity(resume)
        .insert(Focusable { auto_focus: true });
    button(
        &mut commands,
        &art,
        menu,
        "pause.restart",
        MenuAction::Restart,
    );
    button(
        &mut commands,
        &art,
        menu,
        "menu.settings",
        MenuAction::Settings,
    );
    button(
        &mut commands,
        &art,
        menu,
        "pause.menu",
        MenuAction::MainMenu,
    );
}

fn spawn_complete(mut commands: Commands, art: Res<Art>, session: Res<Session>) {
    let root = screen_root(&mut commands, "complete", Screen::Complete, 10, true);
    let menu = panel(&mut commands, &art, root, 200.0, Vec2::ZERO);
    heading(&mut commands, menu, "complete.title");
    let coins = line(&mut commands, menu, "complete.coins", PALE);
    commands.entity(coins).insert(
        Localized::new("complete.coins")
            .with_arg("coins", session.coins)
            .with_arg("total", session.coins_total),
    );
    let time = line(&mut commands, menu, "complete.time", PALE);
    commands
        .entity(time)
        .insert(Localized::new("complete.time").with_arg("time", format_time(session.time)));
    if session.new_best {
        line(&mut commands, menu, "complete.best", GOLD);
    }
    let last = session.level + 1 >= LEVELS.len();
    let next = button(
        &mut commands,
        &art,
        menu,
        if last {
            "complete.finish"
        } else {
            "complete.next"
        },
        MenuAction::Next,
    );
    commands.entity(next).insert(Focusable { auto_focus: true });
    button(
        &mut commands,
        &art,
        menu,
        "complete.replay",
        MenuAction::Restart,
    );
    button(
        &mut commands,
        &art,
        menu,
        "pause.menu",
        MenuAction::MainMenu,
    );
}

fn spawn_game_over(mut commands: Commands, art: Res<Art>) {
    let root = screen_root(&mut commands, "game over", Screen::GameOver, 10, true);
    let menu = panel(&mut commands, &art, root, 176.0, Vec2::ZERO);
    heading(&mut commands, menu, "over.title");
    let retry = button(&mut commands, &art, menu, "over.retry", MenuAction::Restart);
    commands
        .entity(retry)
        .insert(Focusable { auto_focus: true });
    button(
        &mut commands,
        &art,
        menu,
        "pause.menu",
        MenuAction::MainMenu,
    );
}

fn spawn_victory(
    mut commands: Commands,
    art: Res<Art>,
    progress: Res<Progress>,
    mut audio: ResMut<Audio>,
) {
    let root = screen_root(&mut commands, "victory", Screen::Victory, 10, true);
    let menu = panel(&mut commands, &art, root, 260.0, Vec2::ZERO);
    heading(&mut commands, menu, "victory.title");
    let (coins, total) = (0..LEVELS.len())
        .filter_map(|i| progress.best(i))
        .fold((0, 0), |(c, t), best| (c + best.coins, t + best.total));
    let text = line(&mut commands, menu, "victory.text", PALE);
    commands.entity(text).insert(
        Localized::new("victory.text")
            .with_arg("coins", coins)
            .with_arg("total", total),
    );
    let back = button(
        &mut commands,
        &art,
        menu,
        "pause.menu",
        MenuAction::MainMenu,
    );
    commands.entity(back).insert(Focusable { auto_focus: true });
    audio.play_music(
        &art.title_music,
        MusicSettings {
            volume: 0.55,
            crossfade: 1.0,
            ..default()
        },
    );
}

// --- Reacting to the menus -----------------------------------------------------------

/// The menus' choices, recorded for the tests.
#[derive(Resource, Debug, Default)]
pub(crate) struct Clicks(pub Vec<MenuAction>);

#[allow(clippy::too_many_arguments)]
fn on_click(
    mut clicks: MessageReader<Clicked>,
    buttons: Query<&MenuAction>,
    mut next: ResMut<NextState<Screen>>,
    mut fade: ResMut<StateFade<Screen>>,
    mut locale: ResMut<Locale>,
    mut touch: ResMut<TouchControls>,
    mut session: ResMut<Session>,
    progress: Res<Progress>,
    mut rebinding: ResMut<Rebinding>,
    mut focus: ResMut<UiFocus>,
    mut controls: ResMut<Controls>,
    mut map: ResMut<ActionMap>,
    storage: Res<Storage>,
    options: Res<crate::Options>,
    mut exit: MessageWriter<AppExit>,
    mut recorded: Option<ResMut<Clicks>>,
) {
    for click in clicks.read() {
        let Ok(&action) = buttons.get(click.entity) else {
            continue;
        };
        if rebinding.is_busy() || fade.is_busy() {
            continue;
        }
        if let Some(recorded) = recorded.as_mut() {
            recorded.0.push(action);
        }
        match action {
            MenuAction::Play => {
                session.level = progress.unlocked.min(LEVELS.len() - 1);
                fade.set(Screen::Playing);
            }
            MenuAction::Start(level) => {
                session.level = level;
                fade.set(Screen::Playing);
            }
            MenuAction::Levels => next.push(Screen::Levels),
            MenuAction::Settings => next.push(Screen::Settings),
            MenuAction::Controls => next.push(Screen::Controls),
            MenuAction::Language => {
                let languages: Vec<String> = locale.languages().map(str::to_owned).collect();
                let at = languages
                    .iter()
                    .position(|l| l == locale.language())
                    .unwrap_or(0);
                let language = languages[(at + 1) % languages.len()].clone();
                locale.set_language(&language);
            }
            MenuAction::Quit => {
                exit.write(AppExit::Success);
            }
            MenuAction::Back | MenuAction::Resume => next.pop(),
            MenuAction::TouchMode => {
                touch.mode = match touch.mode {
                    TouchMode::Auto => TouchMode::Always,
                    TouchMode::Always => TouchMode::Never,
                    TouchMode::Never => TouchMode::Auto,
                };
            }
            MenuAction::Rebind(action, slot) => rebinding.start(action, slot, &mut focus),
            MenuAction::ResetControls => {
                *controls = Controls::default();
                controls::apply(&controls, &mut map, &storage, !options.demo);
            }
            MenuAction::Restart => fade.set(Screen::Playing),
            MenuAction::MainMenu => fade.set(Screen::Title),
            MenuAction::Next => {
                if session.level + 1 < LEVELS.len() {
                    session.level += 1;
                    fade.set(Screen::Playing);
                } else {
                    next.replace(Screen::Victory);
                }
            }
        }
    }
}

fn on_slider(
    mut changes: MessageReader<SliderChanged>,
    sliders: Query<&SettingSlider>,
    mut audio: ResMut<AudioSettings>,
) {
    for change in changes.read() {
        match sliders.get(change.entity) {
            Ok(SettingSlider::Master) => audio.master = change.value,
            Ok(SettingSlider::Music) => audio.set_level(Bus::Music, change.value),
            Ok(SettingSlider::Effects) => audio.set_level(Bus::Sfx, change.value),
            Err(_) => {}
        }
    }
}

/// Pause (Escape, P, the start button, the touch pause button) pauses and resumes; back
/// (Escape, the east button) leaves menus.
fn back_and_pause(
    action_state: Res<ActionState>,
    state: Res<State<Screen>>,
    mut next: ResMut<NextState<Screen>>,
    fade: Res<StateFade<Screen>>,
    rebinding: Res<Rebinding>,
    session: Res<Session>,
) {
    if fade.is_busy() || rebinding.is_busy() {
        return;
    }
    let back = action_state.just_pressed(actions::BACK);
    let pause = action_state.just_pressed("pause");
    match state.current() {
        Screen::Playing if pause && session.finished.is_none() => next.push(Screen::Paused),
        Screen::Paused if back || pause => next.pop(),
        Screen::Levels | Screen::Settings | Screen::Controls if back => next.pop(),
        _ => {}
    }
}

/// Fills in the labels whose text changes.
#[allow(clippy::too_many_arguments)]
fn update_labels(
    locale: Res<Locale>,
    touch: Res<TouchControls>,
    progress: Res<Progress>,
    controls: Res<Controls>,
    rebinding: Res<Rebinding>,
    lives: Query<(Entity, &Live, Option<&Children>)>,
    mut labels: Query<&mut Localized>,
) {
    for (entity, live, children) in &lives {
        let target = match live {
            Live::LevelName(_) | Live::LevelBest(_) => entity,
            _ => match children.and_then(|c| c.iter().next()) {
                Some(child) => child,
                None => continue,
            },
        };
        let Ok(mut label) = labels.get_mut(target) else {
            continue;
        };
        let set = |label: &mut Localized, key: &str, args: &[(&str, String)]| {
            let same = label.key == key
                && args
                    .iter()
                    .all(|(n, v)| label.args.iter().any(|(m, w)| m == n && w == v));
            if !same {
                label.key = key.to_owned();
                for (name, value) in args {
                    label.set_arg(*name, value);
                }
            }
        };
        match *live {
            Live::Language => set(
                &mut label,
                "menu.language",
                &[("name", locale.language_name(locale.language()).to_owned())],
            ),
            Live::TouchMode => {
                let mode = match touch.mode {
                    TouchMode::Auto => "touch.auto",
                    TouchMode::Always => "touch.on",
                    TouchMode::Never => "touch.off",
                };
                set(
                    &mut label,
                    "settings.touch",
                    &[("mode", locale.text(mode).to_owned())],
                );
            }
            Live::Continue => {
                let key = label.key.clone();
                let level = progress.unlocked.min(LEVELS.len() - 1);
                set(
                    &mut label,
                    &key,
                    &[("level", locale.text(LEVELS[level].name).to_owned())],
                );
            }
            Live::Slot(action, slot) => {
                let text = if rebinding.target == Some((action, slot)) {
                    locale.text("controls.press").to_owned()
                } else {
                    controls.get(action, slot).map_or_else(
                        || locale.text("controls.empty").to_owned(),
                        |b| b.to_string(),
                    )
                };
                // Binding names are not translated; "{name}" shows them as they are.
                set(&mut label, "controls.slot", &[("name", text)]);
            }
            Live::LevelName(i) => set(
                &mut label,
                "levels.entry",
                &[
                    ("n", (i + 1).to_string()),
                    ("name", locale.text(LEVELS[i].name).to_owned()),
                ],
            ),
            Live::LevelBest(i) => match progress.best(i) {
                Some(best) => set(
                    &mut label,
                    "levels.best",
                    &[
                        ("coins", best.coins.to_string()),
                        ("total", best.total.to_string()),
                        ("time", format_time(best.time)),
                    ],
                ),
                None if progress.is_unlocked(i) => set(&mut label, "levels.new", &[]),
                None => set(&mut label, "levels.locked", &[]),
            },
        }
    }
}

/// Labels drop a pixel while their button is held; slider thumbs light up with focus.
fn widget_looks(
    widgets: Query<(&Interaction, &Children, Has<Slider>), Changed<Interaction>>,
    mut nodes: Query<&mut Node, Without<SliderThumb>>,
    mut thumbs: Query<&mut UiImage, With<SliderThumb>>,
) {
    for (interaction, children, slider) in &widgets {
        for child in children.iter() {
            if !slider && let Ok(mut node) = nodes.get_mut(child) {
                let y = if *interaction == Interaction::Pressed {
                    0.0
                } else {
                    -1.0
                };
                if node.offset.y != y {
                    node.offset.y = y;
                }
            }
            if let Ok(mut thumb) = thumbs.get_mut(child) {
                thumb.source = Some(if *interaction == Interaction::None {
                    THUMB
                } else {
                    THUMB_FOCUS
                });
            }
        }
    }
}

/// A click on the interface bus for each choice, change and focus move.
fn click_sounds(
    mut clicks: MessageReader<Clicked>,
    mut changes: MessageReader<SliderChanged>,
    focus: Res<UiFocus>,
    mut last_focus: Local<Option<Entity>>,
    art: Res<Art>,
    mut audio: ResMut<Audio>,
) {
    let moved = focus.get() != *last_focus && focus.get().is_some() && focus.from_keys();
    *last_focus = focus.get();
    let count = clicks.read().count() + changes.read().count() + usize::from(moved);
    if count > 0 {
        audio.play_with(
            &art.sounds.click,
            PlaySettings::new().with_bus(Bus::Ui).with_pitch_jitter(0.5),
        );
    }
}

/// Logs the screen and frame rate when the screen or language changes.
fn report(
    state: Res<State<Screen>>,
    locale: Res<Locale>,
    stats: Res<RenderStats>,
    mut last: Local<Option<(Screen, String)>>,
) {
    let now = (*state.current(), locale.language().to_owned());
    if last.as_ref() != Some(&now) {
        info!(
            "screen {:?}, language {}, {:.0} fps, {} draw calls",
            now.0,
            now.1,
            stats.fps(),
            stats.draw_calls()
        );
        *last = Some(now);
    }
}
