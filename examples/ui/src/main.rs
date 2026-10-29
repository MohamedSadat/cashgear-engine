//! `ui`: a main menu, a settings screen, a HUD with on-screen touch controls and a pause
//! menu, navigable by keyboard, gamepad, mouse and touch, in English and Arabic, with the
//! debug overlay on F12.
//!
//! - Desktop: `cargo run -p ui`. Add `-- --demo` to let a bot click through every screen
//!   (it switches to Arabic and back, changes a setting, plays and pauses, then quits);
//!   `--touch` shows the touch controls on desktop, `--overlay` opens the debug overlay.
//! - Web: run `trunk serve --port 8087` in `examples/ui` and open <http://127.0.0.1:8087>
//!   (add `?demo`, `?touch`). On a phone the touch controls show by themselves.
//!
//! What it shows:
//!
//! - **Layout**: anchored nodes, columns of buttons, rows of label and slider, a HUD in the
//!   corners, all in the 640×360 virtual pixels and scaled with the game.
//! - **Pixel art UI**: nine-slice panels and buttons from `images/ui.png`, with images
//!   for the focused and pressed states; labels drop a pixel when pressed.
//! - **Text**: the built-in pixel font (Pixelify Sans) for the menus, a bitmap font for
//!   the HUD's digits (`fonts/digits.fnt`), and Noto Kufi Arabic for Arabic.
//! - **Focus**: arrows, WASD, the d-pad or the left stick move between widgets; Enter,
//!   Space or the south button choose; Escape or the east button go back. The mouse hovers
//!   and clicks; a finger taps.
//! - **Widgets**: buttons, sliders (volume levels, saved by the audio settings) and a
//!   progress bar (energy).
//! - **Localization**: `strings/en.strings.ron` and `strings/ar.strings.ron`. Arabic is
//!   shaped and written right to left, and every screen mirrors. The language is saved.
//! - **Touch controls**: a d-pad, a jump button and a pause button, shown on touch
//!   screens (or always, or never, from the settings screen).
//! - **The debug overlay** (F12, or from the settings screen): frame times, render stats,
//!   the entity list and inspector, collider and UI outlines.
//!
//! | Action | Keyboard | Gamepad | Touch |
//! | --- | --- | --- | --- |
//! | Move between widgets | Arrows, WASD | D-pad, left stick | Tap |
//! | Choose | Enter, Space | South (A / cross) | Tap |
//! | Back, resume | Escape | East (B / circle) | |
//! | Run | Arrows, A / D | D-pad, left stick | D-pad |
//! | Jump | Space, W, Up | South | Jump button |
//! | Pause | Escape, P | Start | Pause button |
//! | Debug overlay | F12 | | Settings screen |
//!
//! Art: the alien and hills are Kenney's Pixel Platformer (CC0, www.kenney.nl); the UI art,
//! digits and click are original. Fonts: Pixelify Sans (built in) and Noto Kufi Arabic,
//! both SIL Open Font License 1.1 (`assets/fonts/NotoKufiArabic-OFL.txt`).

use cashgear_engine::prelude::*;
use cashgear_engine::ui::{DEFAULT_FONT_SIZE, DebugOverlay, actions};

const UI_SHEET: &str = "images/ui.png";
const DIGITS: &str = "fonts/digits.fnt";
const SHEET: &str = "images/alien.json";
const GRAPH: &str = "anims/alien.anim.ron";
const HILLS: &str = "images/hills.png";
const CLICK: &str = "sounds/click.ogg";

/// The sky behind the hills.
const SKY: u32 = 0xdff6f5;
const GOLD: u32 = 0xffcd75;
const INK: u32 = 0x1a1c2c;

// Parts of `images/ui.png`, in texels.
const PANEL: Rect = Rect::new(0.0, 0.0, 24.0, 24.0);
const BUTTON: Rect = Rect::new(24.0, 0.0, 48.0, 24.0);
const BUTTON_FOCUS: Rect = Rect::new(48.0, 0.0, 72.0, 24.0);
const BUTTON_PRESSED: Rect = Rect::new(72.0, 0.0, 96.0, 24.0);
const BUTTON_OFF: Rect = Rect::new(96.0, 0.0, 120.0, 24.0);
const TRACK: Rect = Rect::new(0.0, 24.0, 16.0, 32.0);
const GREEN: Rect = Rect::new(16.0, 24.0, 32.0, 32.0);
const ORANGE: Rect = Rect::new(48.0, 24.0, 64.0, 32.0);
const THUMB: Rect = Rect::new(32.0, 24.0, 40.0, 38.0);
const THUMB_FOCUS: Rect = Rect::new(40.0, 24.0, 48.0, 38.0);
const DPAD: Rect = Rect::new(0.0, 40.0, 48.0, 88.0);
const JUMP_BUTTON: Rect = Rect::new(48.0, 40.0, 80.0, 72.0);
const PAUSE_BUTTON: Rect = Rect::new(80.0, 40.0, 104.0, 64.0);
const PANEL_SLICE: Edges = Edges::all(4.0);
const BUTTON_SLICE: Edges = Edges {
    left: 4.0,
    right: 4.0,
    top: 4.0,
    bottom: 5.0,
};
const BAR_SLICE: Edges = Edges::all(3.0);

/// Units per second, per second squared, and the jump's take-off speed.
const RUN_SPEED: f32 = 120.0;
const RUN_ACCEL: f32 = 900.0;
const JUMP_SPEED: f32 = 330.0;
const GRAVITY: f32 = 980.0;
/// How far the alien may walk.
const WORLD: Rect = Rect::new(-600.0, -200.0, 1500.0, 400.0);

#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
enum Screen {
    Loading,
    Menu,
    Settings,
    Play,
    Paused,
}

fn main() {
    let args = Args::from_env();
    App::new()
        .add_plugins(DefaultPlugins {
            window: WindowConfig {
                title: "CashGear: ui".into(),
                canvas_id: Some("cashgear".into()),
                ..default()
            },
            storage: StoragePlugin {
                namespace: Some("cashgear-ui-example".into()),
                ..default()
            },
            ui: UiPlugin {
                locale: Some(LocaleSettings::new(&["en", "ar"])),
                ..default()
            },
            ..default()
        })
        .add_plugins(Example {
            demo: args.flag("demo"),
        })
        .run();
}

/// Everything but the engine's plugins, so the test can build it without a window.
struct Example {
    /// Let the bot click through, and exit when it is done.
    demo: bool,
}

impl Plugin for Example {
    fn build(&self, app: &mut App) {
        use Screen::{Loading, Menu, Paused, Play, Settings};
        app.insert_resource(action_map())
            .init_resource::<Hud>()
            .insert_resource(Bot {
                enabled: self.demo,
                ..default()
            })
            .init_state(Loading)
            .add_loading_state(Loading, Menu)
            .add_state_fade::<Screen>()
            .add_systems(Startup, setup)
            .add_systems(OnEnter(Loading), spawn_loading)
            .add_systems(OnExit(Loading), spawn_world)
            .add_systems(OnEnter(Menu), spawn_menu)
            .add_systems(OnEnter(Settings), spawn_settings)
            .add_systems(OnEnter(Play), spawn_hud)
            .add_systems(OnEnter(Paused), (spawn_pause, set_time_scale(0.0)))
            .add_systems(OnExit(Paused), set_time_scale(1.0))
            .add_systems(
                PreUpdate,
                drive_bot
                    .run_if(|bot: Res<Bot>| bot.enabled)
                    .before(InputSystems::Actions),
            )
            .add_systems(
                FixedUpdate,
                (move_alien, feed_animator).chain().run_if(in_state(Play)),
            )
            .add_systems(Update, show_loading.run_if(in_state(Loading)))
            .add_systems(
                Update,
                (
                    on_click,
                    on_slider,
                    back_and_pause,
                    update_labels,
                    update_hud,
                    widget_looks,
                    click_sounds,
                    report,
                )
                    .run_if(|state: Res<State<Screen>>| *state.current() != Screen::Loading),
            );
    }
}

fn action_map() -> ActionMap {
    use AxisDirection::{Negative, Positive};
    ActionMap::new()
        .bind("left", KeyCode::ArrowLeft)
        .bind("left", KeyCode::KeyA)
        .bind("left", GamepadButton::DPadLeft)
        .bind("left", Binding::Axis(GamepadAxis::LeftStickX, Negative))
        .bind("right", KeyCode::ArrowRight)
        .bind("right", KeyCode::KeyD)
        .bind("right", GamepadButton::DPadRight)
        .bind("right", Binding::Axis(GamepadAxis::LeftStickX, Positive))
        .bind("jump", KeyCode::Space)
        .bind("jump", KeyCode::KeyW)
        .bind("jump", KeyCode::ArrowUp)
        .bind("jump", GamepadButton::South)
        .bind("pause", KeyCode::Escape)
        .bind("pause", KeyCode::KeyP)
        .bind("pause", GamepadButton::Start)
}

fn set_time_scale(scale: f32) -> impl FnMut(ResMut<Time>) {
    move |mut time: ResMut<Time>| time.set_time_scale(scale)
}

/// The example's assets.
#[derive(Resource)]
struct Art {
    sheet: Handle<Image>,
    digits: Handle<Font>,
    alien: Handle<SpriteSheet>,
    graph: Handle<AnimationGraph>,
    hills: Handle<Image>,
    click: Handle<Sound>,
}

/// The HUD's numbers.
#[derive(Resource, Debug)]
struct Hud {
    time: f32,
    energy: f32,
}

impl Default for Hud {
    fn default() -> Self {
        Self {
            time: 0.0,
            energy: 1.0,
        }
    }
}

fn setup(mut commands: Commands, server: Res<AssetServer>, mut loading: ResMut<LoadingAssets>) {
    commands.spawn((
        Name::new("camera"),
        Camera2D {
            clear_color: Color::hex(SKY),
            ..default()
        },
        Transform2D::from_xy(0.0, 120.0),
        Interpolated::default(),
    ));
    let art = Art {
        sheet: server.load(UI_SHEET),
        digits: server.load(DIGITS),
        alien: server.load(SHEET),
        graph: server.load(GRAPH),
        hills: server.load(HILLS),
        click: server.load(CLICK),
    };
    loading.add(&art.sheet);
    loading.add(&art.digits);
    loading.add(&art.alien);
    loading.add(&art.graph);
    loading.add(&art.hills);
    loading.add(&art.click);
    commands.insert_resource(art);
}

// --- Loading -------------------------------------------------------------------------

#[derive(Component)]
struct LoadingBar;

/// A bar of plain colours: the UI sheet may not have loaded yet.
fn spawn_loading(mut commands: Commands) {
    let bar = commands
        .spawn((
            Name::new("loading"),
            Node::sized(160.0, 6.0).at(Anchor::Center),
            BackgroundColor(Color::hex(INK)),
            ProgressBar::default(),
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
        LoadingBar,
        ChildOf(bar),
    ));
}

fn show_loading(
    loading: Res<LoadingAssets>,
    server: Res<AssetServer>,
    mut bars: Query<&mut ProgressBar>,
) {
    let fraction = loading.progress(&server).fraction();
    for mut bar in &mut bars {
        bar.value = fraction;
    }
}

// --- The world behind the menus ------------------------------------------------------

/// The alien: its velocity and whether it stands on the ground.
#[derive(Component, Default)]
struct Alien {
    velocity: Vec2,
    grounded: bool,
}

fn spawn_world(mut commands: Commands, art: Res<Art>, camera: Single<Entity, With<Camera2D>>) {
    // Hills along the ground, and the ground: grass over dirt.
    for i in -8..18 {
        commands.spawn((
            Sprite {
                pivot: vec2(0.0, 0.0),
                ..Sprite::new(art.hills.clone())
            },
            Transform2D::from_xy(i as f32 * 96.0, -120.0),
            Layer(-10),
        ));
    }
    commands.spawn((
        Sprite {
            pivot: vec2(0.5, 1.0),
            ..Sprite::solid(Color::hex(0x6abe30), vec2(3000.0, 6.0))
        },
        Transform2D::from_xy(450.0, 0.0),
        Layer(-5),
    ));
    commands.spawn((
        Sprite {
            pivot: vec2(0.5, 1.0),
            ..Sprite::solid(Color::hex(0x8f563b), vec2(3000.0, 400.0))
        },
        Transform2D::from_xy(450.0, -6.0),
        Layer(-5),
    ));
    let alien = commands
        .spawn((
            Name::new("alien"),
            Alien {
                grounded: true,
                ..default()
            },
            SpriteAnimation::new(art.alien.clone(), "idle").with_pivot(FEET),
            Animator::new(art.graph.clone()),
            Transform2D::from_xy(0.0, 0.0),
            Interpolated::default(),
            Layer(5),
        ))
        .id();
    let mut follow = CameraFollow::new(alien);
    follow.offset = vec2(0.0, 110.0);
    follow.dead_zone = vec2(80.0, 60.0);
    follow.bounds = Some(WORLD);
    commands.entity(*camera).insert(follow);
}

fn move_alien(
    actions: Res<ActionState>,
    time: Res<Time>,
    mut hud: ResMut<Hud>,
    alien: Single<(&mut Alien, &mut Transform2D, &mut Sprite)>,
) {
    let (mut alien, mut transform, mut sprite) = alien.into_inner();
    let dt = time.delta_secs();
    let tired = hud.energy <= 0.0;
    let target = actions.axis("left", "right") * if tired { RUN_SPEED * 0.4 } else { RUN_SPEED };
    let step = RUN_ACCEL * dt;
    alien.velocity.x = target.clamp(alien.velocity.x - step, alien.velocity.x + step);
    if actions.just_pressed("jump") && alien.grounded && !tired {
        alien.velocity.y = JUMP_SPEED;
        alien.grounded = false;
        hud.energy -= 0.1;
    }
    alien.velocity.y -= GRAVITY * dt;
    let mut at = transform.translation + alien.velocity * dt;
    if at.y <= 0.0 {
        at.y = 0.0;
        alien.velocity.y = 0.0;
        alien.grounded = true;
    }
    at.x = at.x.clamp(WORLD.min.x + 8.0, WORLD.max.x - 8.0);
    transform.translation = at;
    if alien.velocity.x.abs() > 1.0 {
        sprite.flip_x = alien.velocity.x < 0.0;
    }
    let drain = if alien.velocity.x.abs() > 5.0 {
        -0.12
    } else {
        0.25
    };
    hud.energy = (hud.energy + drain * dt).clamp(0.0, 1.0);
    hud.time += dt;
}

fn feed_animator(alien: Single<(&Alien, &mut Animator)>) {
    let (alien, mut animator) = alien.into_inner();
    animator.set("speed", alien.velocity.x.abs());
    animator.set("vy", alien.velocity.y);
    animator.set_bool("grounded", alien.grounded);
}

// --- Widgets -------------------------------------------------------------------------

/// What a button does.
#[derive(Component, Clone, Copy, PartialEq, Eq, Debug)]
enum MenuAction {
    Play,
    Settings,
    Language,
    Quit,
    Back,
    Resume,
    MainMenu,
    TouchMode,
    Overlay,
}

/// A label whose string takes an argument that changes with the game.
#[derive(Component, Clone, Copy, PartialEq, Eq, Debug)]
enum LiveLabel {
    Language,
    TouchMode,
    Overlay,
}

/// Which setting a slider changes.
#[derive(Component, Clone, Copy, PartialEq, Eq, Debug)]
enum SettingSlider {
    Volume,
    MenuSounds,
}

/// The HUD's clock, in the bitmap font.
#[derive(Component)]
struct Clock;

/// The HUD's energy bar.
#[derive(Component)]
struct Energy;

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
            UiImage::new(art.sheet.clone())
                .with_source(PANEL)
                .sliced(PANEL_SLICE),
            ChildOf(parent),
        ))
        .id()
}

/// A localized heading in gold.
fn heading(commands: &mut Commands, parent: Entity, key: &str) {
    commands.spawn((
        Node {
            padding: Edges {
                bottom: 3.0,
                ..Edges::ZERO
            },
            ..default()
        },
        Text::default()
            .with_color(Color::hex(GOLD))
            .with_align(TextAlign::Center),
        Localized::new(key),
        ChildOf(parent),
    ));
}

/// A button with a localized label.
fn button(
    commands: &mut Commands,
    art: &Art,
    parent: Entity,
    key: &str,
    action: MenuAction,
) -> Entity {
    let button = commands
        .spawn((
            Name::new(format!("button {key}")),
            Node {
                height: Val::Px(24.0),
                ..default()
            },
            Button::default(),
            UiImage::new(art.sheet.clone())
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
        .id();
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
            UiImage::new(art.sheet.clone())
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
        UiImage::new(art.sheet.clone())
            .with_source(GREEN)
            .sliced(BAR_SLICE),
        BarFill,
        ChildOf(slider),
    ));
    commands.spawn((
        Node::sized(8.0, 14.0),
        UiImage::new(art.sheet.clone()).with_source(THUMB),
        SliderThumb,
        ChildOf(slider),
    ));
    slider
}

// --- Screens -------------------------------------------------------------------------

fn spawn_menu(mut commands: Commands, art: Res<Art>) {
    let root = screen_root(&mut commands, "menu", Screen::Menu, 0, false);
    // The title, with a shadow a pixel down and right.
    let title = commands
        .spawn((
            Name::new("title"),
            Node::default().at(Anchor::Top).with_offset(vec2(0.0, 30.0)),
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
        Node::default().at(Anchor::Top).with_offset(vec2(0.0, 82.0)),
        Text::default().with_color(Color::hex(INK)),
        Localized::new("subtitle"),
        ChildOf(root),
    ));
    let menu = panel(&mut commands, &art, root, 176.0, vec2(0.0, 36.0));
    let play = button(&mut commands, &art, menu, "menu.play", MenuAction::Play);
    commands.entity(play).insert(Focusable { auto_focus: true });
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
    commands.entity(language).insert(LiveLabel::Language);
    button(&mut commands, &art, menu, "menu.quit", MenuAction::Quit);
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

fn spawn_settings(
    mut commands: Commands,
    art: Res<Art>,
    audio: Res<AudioSettings>,
    stack: Res<State<Screen>>,
) {
    // Over the pause menu the game shows through, dimmed; over the main menu too.
    let dim = stack.stack().len() > 1;
    let root = screen_root(&mut commands, "settings", Screen::Settings, 20, dim);
    let settings = panel(&mut commands, &art, root, 260.0, Vec2::ZERO);
    heading(&mut commands, settings, "settings.title");
    let volume = slider_row(
        &mut commands,
        &art,
        settings,
        "settings.volume",
        SettingSlider::Volume,
        audio.master,
    );
    commands
        .entity(volume)
        .insert(Focusable { auto_focus: true });
    slider_row(
        &mut commands,
        &art,
        settings,
        "settings.ui_sounds",
        SettingSlider::MenuSounds,
        audio.level(Bus::Ui),
    );
    let touch = button(
        &mut commands,
        &art,
        settings,
        "settings.touch",
        MenuAction::TouchMode,
    );
    commands.entity(touch).insert(LiveLabel::TouchMode);
    let overlay = button(
        &mut commands,
        &art,
        settings,
        "settings.overlay",
        MenuAction::Overlay,
    );
    commands.entity(overlay).insert(LiveLabel::Overlay);
    button(&mut commands, &art, settings, "back", MenuAction::Back);
}

fn spawn_pause(mut commands: Commands, art: Res<Art>) {
    let root = screen_root(&mut commands, "pause", Screen::Paused, 10, true);
    let menu = panel(&mut commands, &art, root, 160.0, Vec2::ZERO);
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

fn spawn_hud(mut commands: Commands, art: Res<Art>, mut hud: ResMut<Hud>) {
    *hud = Hud::default();
    let root = commands
        .spawn((
            Name::new("hud"),
            Node {
                padding: Edges::all(8.0),
                ..Node::fill()
            },
            StateScoped(Screen::Play),
        ))
        .id();
    // The clock and the energy bar in the top corner at the start of the line.
    let corner = commands
        .spawn((
            Node {
                layout: Layout::Column,
                gap: 4.0,
                ..default()
            },
            ChildOf(root),
        ))
        .id();
    let row = |commands: &mut Commands| {
        commands
            .spawn((
                Node {
                    layout: Layout::Row,
                    gap: 6.0,
                    align: Align::Center,
                    ..default()
                },
                ChildOf(corner),
            ))
            .id()
    };
    let clock_row = row(&mut commands);
    commands.spawn((
        Text::default().with_color(Color::hex(INK)),
        Localized::new("hud.time"),
        ChildOf(clock_row),
    ));
    commands.spawn((
        Text::new("00:00.0")
            .with_font(art.digits.clone())
            .with_size(18.0),
        Clock,
        ChildOf(clock_row),
    ));
    let energy_row = row(&mut commands);
    commands.spawn((
        Text::default().with_color(Color::hex(INK)),
        Localized::new("hud.energy"),
        ChildOf(energy_row),
    ));
    let bar = commands
        .spawn((
            Node::sized(64.0, 8.0),
            ProgressBar { value: 1.0 },
            UiImage::new(art.sheet.clone())
                .with_source(TRACK)
                .sliced(BAR_SLICE),
            Energy,
            ChildOf(energy_row),
        ))
        .id();
    commands.spawn((
        Node {
            height: Val::Fill,
            ..default()
        },
        UiImage::new(art.sheet.clone())
            .with_source(ORANGE)
            .sliced(BAR_SLICE),
        BarFill,
        ChildOf(bar),
    ));
    // The hint at the bottom, and the touch pause button in the far top corner.
    commands.spawn((
        Node {
            anchor: Anchor::Bottom,
            width: Val::Px(400.0),
            ..default()
        },
        Text::default()
            .with_color(Color::hex(INK))
            .with_align(TextAlign::Center),
        Localized::new("play.hint"),
        ChildOf(root),
    ));
    commands.spawn((
        Name::new("touch pause"),
        Node::sized(24.0, 24.0).at(Anchor::TopRight),
        UiImage::new(art.sheet.clone()).with_source(PAUSE_BUTTON),
        TouchButton::new("pause"),
        ChildOf(root),
    ));
    // Moving stays on the left and jumping on the right in every language.
    let thumbs = commands
        .spawn((
            Name::new("touch controls"),
            Node {
                padding: Edges::all(14.0),
                direction: LayoutDirection::LeftToRight,
                ..Node::fill()
            },
            ZIndex(1),
            StateScoped(Screen::Play),
        ))
        .id();
    commands.spawn((
        Name::new("touch d-pad"),
        Node::sized(48.0, 48.0).at(Anchor::BottomLeft),
        UiImage::new(art.sheet.clone()).with_source(DPAD),
        TouchDpad {
            up: String::new(),
            down: String::new(),
            ..default()
        },
        ChildOf(thumbs),
    ));
    commands.spawn((
        Name::new("touch jump"),
        Node::sized(32.0, 32.0).at(Anchor::BottomRight),
        UiImage::new(art.sheet.clone()).with_source(JUMP_BUTTON),
        TouchButton::new("jump"),
        ChildOf(thumbs),
    ));
}

// --- Reacting to the UI --------------------------------------------------------------

#[allow(clippy::too_many_arguments)]
fn on_click(
    mut clicks: MessageReader<Clicked>,
    buttons: Query<&MenuAction>,
    mut next: ResMut<NextState<Screen>>,
    mut fade: ResMut<StateFade<Screen>>,
    mut locale: ResMut<Locale>,
    mut touch: ResMut<TouchControls>,
    mut overlay: ResMut<DebugOverlay>,
    mut exit: MessageWriter<AppExit>,
    mut bot: ResMut<Bot>,
) {
    for click in clicks.read() {
        let Ok(action) = buttons.get(click.entity) else {
            continue;
        };
        bot.clicked.push(*action);
        match action {
            MenuAction::Play => fade.replace(Screen::Play),
            MenuAction::Settings => next.push(Screen::Settings),
            MenuAction::Language => {
                let languages: Vec<String> = locale.languages().map(str::to_owned).collect();
                let at = languages
                    .iter()
                    .position(|l| l == locale.language())
                    .unwrap_or(0);
                let next_language = languages[(at + 1) % languages.len()].clone();
                locale.set_language(&next_language);
            }
            MenuAction::Quit => {
                exit.write(AppExit::Success);
            }
            MenuAction::Back | MenuAction::Resume => next.pop(),
            MenuAction::MainMenu => fade.set(Screen::Menu),
            MenuAction::TouchMode => {
                touch.mode = match touch.mode {
                    TouchMode::Auto => TouchMode::Always,
                    TouchMode::Always => TouchMode::Never,
                    TouchMode::Never => TouchMode::Auto,
                };
            }
            MenuAction::Overlay => overlay.open = !overlay.open,
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
            Ok(SettingSlider::Volume) => audio.master = change.value,
            Ok(SettingSlider::MenuSounds) => audio.set_level(Bus::Ui, change.value),
            Err(_) => {}
        }
    }
}

/// Escape (and the gamepad's east button, and the touch pause button) pause, resume and
/// go back.
fn back_and_pause(
    actions: Res<ActionState>,
    state: Res<State<Screen>>,
    mut next: ResMut<NextState<Screen>>,
    fade: Res<StateFade<Screen>>,
) {
    if fade.is_busy() {
        return;
    }
    let back = actions.just_pressed(actions::BACK);
    match state.current() {
        Screen::Play if actions.just_pressed("pause") => next.push(Screen::Paused),
        Screen::Paused if back || actions.just_pressed("pause") => next.pop(),
        Screen::Settings if back => next.pop(),
        _ => {}
    }
}

/// Fills in the labels whose strings take changing arguments.
fn update_labels(
    locale: Res<Locale>,
    touch: Res<TouchControls>,
    overlay: Res<DebugOverlay>,
    buttons: Query<(&LiveLabel, &Children)>,
    mut labels: Query<&mut Localized>,
) {
    for (live, children) in &buttons {
        let (name, value) = match live {
            LiveLabel::Language => ("name", locale.language_name(locale.language()).to_owned()),
            LiveLabel::TouchMode => (
                "mode",
                locale
                    .text(match touch.mode {
                        TouchMode::Auto => "touch.auto",
                        TouchMode::Always => "touch.on",
                        TouchMode::Never => "touch.off",
                    })
                    .to_owned(),
            ),
            LiveLabel::Overlay => (
                "state",
                locale
                    .text(if overlay.open {
                        "touch.on"
                    } else {
                        "touch.off"
                    })
                    .to_owned(),
            ),
        };
        for child in children.iter() {
            if let Ok(mut label) = labels.get_mut(child)
                && !label.args.iter().any(|(n, v)| n == name && *v == value)
            {
                label.set_arg(name, &value);
            }
        }
    }
}

fn update_hud(
    hud: Res<Hud>,
    mut clocks: Query<&mut Text, With<Clock>>,
    mut bars: Query<&mut ProgressBar, With<Energy>>,
) {
    let tenths = (hud.time * 10.0) as u32;
    let value = format!(
        "{:02}:{:02}.{}",
        tenths / 600,
        tenths / 10 % 60,
        tenths % 10
    );
    for mut clock in &mut clocks {
        if clock.value != value {
            clock.value = value.clone();
        }
    }
    for mut bar in &mut bars {
        if bar.value != hud.energy {
            bar.value = hud.energy;
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
                node.offset.y = if *interaction == Interaction::Pressed {
                    0.0
                } else {
                    -1.0
                };
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
            &art.click,
            PlaySettings::new().with_bus(Bus::Ui).with_pitch_jitter(0.5),
        );
    }
}

// --- The demo bot --------------------------------------------------------------------

/// One step of the bot's script.
#[derive(Debug, Clone, Copy)]
enum Step {
    /// Do nothing for some frames.
    Wait(u32),
    /// Tap an action.
    Tap(&'static str),
    /// Hold an action for some frames.
    Hold(&'static str, u32),
    /// Check the screen.
    Expect(Screen),
}

const SCRIPT: &[Step] = {
    use Step::{Expect, Hold, Tap, Wait};
    &[
        Wait(30),
        Expect(Screen::Menu),
        Tap(actions::DOWN), // Play: the first press focuses the menu's first button.
        Tap(actions::DOWN), // Settings
        Tap(actions::DOWN), // Language
        Tap(actions::ACCEPT), // Arabic: everything mirrors.
        Wait(90),
        Tap(actions::ACCEPT), // English again.
        Wait(20),
        Tap(actions::UP), // Settings
        Tap(actions::ACCEPT),
        Wait(10),
        Expect(Screen::Settings),
        Tap(actions::LEFT), // The volume slider has the focus: down a step.
        Tap(actions::RIGHT),
        Tap(actions::DOWN),   // Menu sounds
        Tap(actions::DOWN),   // Touch controls
        Tap(actions::ACCEPT), // Auto to On: they show on desktop too.
        Wait(20),
        Tap(actions::BACK),
        Wait(10),
        Expect(Screen::Menu),
        Tap(actions::ACCEPT), // The focus went back to Play.
        Wait(60),
        Expect(Screen::Play),
        Hold("right", 90),
        Tap("jump"),
        Wait(40),
        Hold("left", 40),
        Tap("pause"),
        Wait(10),
        Expect(Screen::Paused),
        Tap(actions::DOWN), // Settings
        Tap(actions::DOWN), // Main menu
        Tap(actions::ACCEPT),
        Wait(60),
        Expect(Screen::Menu),
        Tap(actions::UP), // From Play, focused as the menu opened, round to Quit.
        Tap(actions::ACCEPT),
    ]
};

/// Frames between a tap and the next step, so taps stay separate presses.
const TAP_GAP: u32 = 6;
/// The bot gives up after this many frames.
const TIMEOUT: u32 = 2400;

#[derive(Resource, Default)]
struct Bot {
    enabled: bool,
    step: usize,
    wait: u32,
    holding: Option<&'static str>,
    frames: u32,
    clicked: Vec<MenuAction>,
    failed: Option<String>,
}

fn drive_bot(
    mut bot: ResMut<Bot>,
    mut input: ResMut<VirtualInput>,
    state: Option<Res<State<Screen>>>,
    mut exit: MessageWriter<AppExit>,
) {
    bot.frames += 1;
    if bot.frames > TIMEOUT {
        error!("the bot ran out of time at step {}", bot.step);
        exit.write(AppExit::error());
        return;
    }
    let Some(state) = state else {
        return;
    };
    if *state.current() == Screen::Loading {
        return;
    }
    if bot.wait > 0 {
        bot.wait -= 1;
        return;
    }
    if let Some(action) = bot.holding.take() {
        input.release(action);
        bot.wait = TAP_GAP;
        return;
    }
    let Some(&step) = SCRIPT.get(bot.step) else {
        return;
    };
    bot.step += 1;
    match step {
        Step::Wait(frames) => bot.wait = frames,
        Step::Tap(action) => {
            input.tap(action);
            bot.wait = TAP_GAP;
        }
        Step::Hold(action, frames) => {
            input.press(action);
            bot.holding = Some(action);
            bot.wait = frames;
        }
        Step::Expect(screen) => {
            if *state.current() != screen {
                let message = format!(
                    "step {}: expected {screen:?}, on {:?}",
                    bot.step - 1,
                    state.current()
                );
                error!("{message}");
                bot.failed = Some(message);
                exit.write(AppExit::error());
            }
        }
    }
}

/// Logs the screen, language and frame rate when the screen or language changes.
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

#[cfg(test)]
mod tests {
    use super::*;
    use cashgear_engine::audio::AudioOutput;
    use cashgear_engine::platform::input::InputPlugin;
    use cashgear_engine::render::{FrameView, OffscreenRenderer, RenderSystems};
    use cashgear_engine::ui::FontKind;

    /// The example without a window, with the bot clicking and a mock audio device.
    fn demo_app() -> App {
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
        .add_plugins(Example { demo: true });
        app.insert_resource(Window::with_size(UVec2::new(1280, 720), 1.0));
        app
    }

    #[test]
    fn the_bot_clicks_through_every_screen() {
        let mut app = demo_app();
        let mut exit = None;
        let mut saw_arabic = false;
        let mut mirrored = false;
        for _ in 0..TIMEOUT + 10 {
            app.update(core::time::Duration::from_millis(16));
            let world = app.world();
            if world.resource::<Locale>().language() == "ar" {
                saw_arabic = true;
                // The menu's title is centred, but the language button's label is
                // right to left.
                let mut nodes = app.world_mut().query::<(&ComputedNode, &Localized)>();
                mirrored |= nodes
                    .iter(app.world())
                    .any(|(node, label)| label.key == "menu.language" && node.is_rtl());
            }
            if let Some(code) = app.should_exit() {
                exit = Some(code);
                break;
            }
        }
        let bot = app.world().resource::<Bot>();
        assert_eq!(bot.failed, None);
        assert_eq!(exit, Some(AppExit::Success), "stopped at step {}", bot.step);
        assert!(saw_arabic && mirrored);
        use MenuAction as M;
        assert_eq!(
            bot.clicked,
            [
                M::Language,
                M::Language,
                M::Settings,
                M::TouchMode,
                M::Play,
                M::MainMenu,
                M::Quit
            ]
        );
        let audio = app.world().resource::<AudioSettings>();
        assert!((audio.master - 1.0).abs() < 1e-5, "{}", audio.master);
        assert_eq!(
            app.world().resource::<TouchControls>().mode,
            TouchMode::Always
        );
    }

    #[test]
    fn the_assets_and_both_languages_load() {
        let mut app = demo_app();
        app.world_mut().resource_mut::<Bot>().enabled = false;
        for _ in 0..10 {
            app.update(core::time::Duration::from_millis(16));
        }
        assert_eq!(
            *app.world().resource::<State<Screen>>().current(),
            Screen::Menu
        );
        let locale = app.world().resource::<Locale>();
        assert_eq!(locale.language_name("ar"), "العربية");
        assert_eq!(locale.text("menu.play"), "Play");
        let fonts = app.world().resource::<Assets<Font>>();
        assert!(matches!(
            fonts
                .get(app.world().resource::<Art>().digits.id())
                .map(|f| &f.kind),
            Some(FontKind::Bitmap(_))
        ));
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

    /// Renders each screen of the bot's run offscreen at 1280x720 (the UI at twice its
    /// size) and saves PNGs, for looking at the result without a window:
    /// `cargo test -p ui -- --ignored capture_screens` writes them to `CG_CAPTURE_DIR`
    /// (default `target/ui-captures`).
    #[test]
    #[ignore = "needs a GPU; writes PNGs for people to look at"]
    // It writes files on desktop only.
    #[allow(clippy::disallowed_methods)]
    fn capture_screens() {
        let dir = std::env::var("CG_CAPTURE_DIR").unwrap_or_else(|_| {
            concat!(env!("CARGO_MANIFEST_DIR"), "/../../target/ui-captures").into()
        });
        std::fs::create_dir_all(&dir).unwrap();
        let mut renderer = pollster::block_on(OffscreenRenderer::new()).expect("a GPU");
        let mut app = demo_app();
        app.init_resource::<Snapshot>().add_systems(
            Render,
            snapshot
                .after(RenderSystems::Queue)
                .before(RenderSystems::Draw),
        );
        // (name, bot step, frames left in that step's wait)
        let shots = [
            ("menu-en", 1, 1),
            ("menu-ar", 7, 1),
            ("settings", 19, 1),
            ("play-touch", 26, 1),
            ("paused", 31, 1),
        ];
        let mut taken = Vec::new();
        for _ in 0..TIMEOUT {
            app.update(core::time::Duration::from_millis(16));
            let bot = app.world().resource::<Bot>();
            let Some((name, ..)) = shots.iter().find(|(n, step, wait)| {
                bot.step == *step && bot.wait == *wait && !taken.contains(n)
            }) else {
                if app.should_exit().is_some() {
                    break;
                }
                continue;
            };
            taken.push(*name);
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
        }
        assert_eq!(taken.len(), shots.len(), "took {taken:?}");
    }
}
