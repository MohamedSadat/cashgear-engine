//! Tests of the UI as a whole: layout, text, input, focus, touch and localization in an app
//! without a window.

use super::*;
use crate::font::DEFAULT_FONT_DATA;
use cg_assets::{AssetPlugin, Handle};
use cg_core::Rect;
use cg_core::glam::{Vec2, uvec2, vec2};
use cg_platform::Window;
use cg_platform::input::{ActionState, ButtonInput, PointerId, Pointers, VirtualInput};
use cg_render::{Color, Image, Sprites, Visibility};
use core::time::Duration;

/// The Arabic font the `ui` example ships, for shaping and fallback tests.
const ARABIC_FONT: &[u8] = include_bytes!("../../../examples/ui/assets/fonts/NotoKufiArabic.ttf");

/// An app with the UI and a 1280×720 window: the default 640×360 pixel-perfect view at
/// twice the size.
fn app() -> App {
    app_with(UiPlugin::default())
}

fn app_with(plugin: UiPlugin) -> App {
    let mut app = App::new();
    app.add_plugins(AssetPlugin {
        hot_reload: Some(false),
        ..Default::default()
    })
    .add_plugins(InputPlugin { gamepads: false })
    .add_plugins(plugin)
    .insert_resource(Window::with_size(uvec2(1280, 720), 1.0))
    .init_resource::<Seen>()
    .add_systems(cg_core::Update, record);
    app
}

/// Messages seen in `Update`.
#[derive(Resource, Default)]
struct Seen {
    clicks: Vec<Entity>,
    changes: Vec<(Entity, f32)>,
}

fn record(
    mut seen: ResMut<Seen>,
    mut clicks: MessageReader<Clicked>,
    mut changes: MessageReader<SliderChanged>,
) {
    seen.clicks.extend(clicks.read().map(|c| c.entity));
    seen.changes
        .extend(changes.read().map(|c| (c.entity, c.value)));
}

fn frame(app: &mut App) {
    app.update(Duration::from_millis(16));
}

fn rect(app: &App, entity: Entity) -> Rect {
    app.world().get::<ComputedNode>(entity).unwrap().rect()
}

/// Moves the mouse to a UI position (the window is at scale 2).
fn mouse_to(app: &mut App, ui: Vec2) {
    let size = app.world().resource::<Window>().logical_size();
    app.world_mut()
        .resource_mut::<Pointers>()
        .move_to(PointerId::Mouse, ui * 2.0, size);
}

/// Taps an action, then lets a frame pass so the next tap is a new press.
fn tap(app: &mut App, action: &str) {
    app.world_mut().resource_mut::<VirtualInput>().tap(action);
    frame(app);
    frame(app);
}

#[test]
fn free_layout_places_nodes_at_their_anchors() {
    let mut app = app();
    let root = app.world_mut().spawn(Node::fill()).id();
    let corner = app
        .world_mut()
        .spawn((
            Node::sized(20.0, 10.0)
                .at(Anchor::BottomRight)
                .with_offset(vec2(-4.0, -2.0)),
            ChildOf(root),
        ))
        .id();
    let centred = app
        .world_mut()
        .spawn((Node::sized(100.0, 40.0).at(Anchor::Center), ChildOf(root)))
        .id();
    let half = app
        .world_mut()
        .spawn((
            Node {
                width: Val::Percent(50.0),
                height: Val::Fill,
                ..Node::default()
            },
            ChildOf(root),
        ))
        .id();
    frame(&mut app);
    assert_eq!(rect(&app, root), Rect::new(0.0, 0.0, 640.0, 360.0));
    assert_eq!(rect(&app, corner), Rect::new(616.0, 348.0, 636.0, 358.0));
    assert_eq!(rect(&app, centred), Rect::new(270.0, 160.0, 370.0, 200.0));
    assert_eq!(rect(&app, half), Rect::new(0.0, 0.0, 320.0, 360.0));
}

#[test]
fn rows_and_columns_stack_with_gaps_fill_and_alignment() {
    let mut app = app();
    let row = app
        .world_mut()
        .spawn(Node {
            width: Val::Px(200.0),
            height: Val::Px(50.0),
            padding: Edges::axes(10.0, 5.0),
            layout: Layout::Row,
            gap: 4.0,
            align: Align::Center,
            ..Node::default()
        })
        .id();
    let a = app
        .world_mut()
        .spawn((Node::sized(30.0, 20.0), ChildOf(row)))
        .id();
    let fill = app
        .world_mut()
        .spawn((
            Node {
                width: Val::Fill,
                height: Val::Px(10.0),
                ..Node::default()
            },
            ChildOf(row),
        ))
        .id();
    let b = app
        .world_mut()
        .spawn((Node::sized(30.0, 40.0), ChildOf(row)))
        .id();
    // An auto-sized column fits its children.
    let column = app
        .world_mut()
        .spawn(Node {
            anchor: Anchor::TopRight,
            padding: Edges::all(2.0),
            layout: Layout::Column,
            gap: 3.0,
            align: Align::End,
            ..Node::default()
        })
        .id();
    let wide = app
        .world_mut()
        .spawn((Node::sized(50.0, 10.0), ChildOf(column)))
        .id();
    let narrow = app
        .world_mut()
        .spawn((Node::sized(20.0, 10.0), ChildOf(column)))
        .id();
    frame(&mut app);
    assert_eq!(rect(&app, a), Rect::new(10.0, 15.0, 40.0, 35.0));
    // 180 wide inside, minus 30 + 30 and two gaps of 4: 112 for the fill.
    assert_eq!(rect(&app, fill), Rect::new(44.0, 20.0, 156.0, 30.0));
    assert_eq!(rect(&app, b), Rect::new(160.0, 5.0, 190.0, 45.0));
    assert_eq!(rect(&app, column), Rect::new(586.0, 0.0, 640.0, 27.0));
    assert_eq!(rect(&app, wide), Rect::new(588.0, 2.0, 638.0, 12.0));
    assert_eq!(rect(&app, narrow), Rect::new(618.0, 15.0, 638.0, 25.0));
}

#[test]
fn right_to_left_mirrors_anchors_rows_and_padding() {
    let mut app = app();
    let root = app
        .world_mut()
        .spawn(Node {
            direction: LayoutDirection::RightToLeft,
            ..Node::fill()
        })
        .id();
    let start = app
        .world_mut()
        .spawn((
            Node::sized(20.0, 10.0).with_offset(vec2(5.0, 0.0)),
            ChildOf(root),
        ))
        .id();
    let row = app
        .world_mut()
        .spawn((
            Node {
                anchor: Anchor::Bottom,
                padding: Edges {
                    left: 8.0,
                    ..Edges::ZERO
                },
                layout: Layout::Row,
                gap: 2.0,
                ..Node::default()
            },
            ChildOf(root),
        ))
        .id();
    let first = app
        .world_mut()
        .spawn((Node::sized(10.0, 10.0), ChildOf(row)))
        .id();
    let second = app
        .world_mut()
        .spawn((Node::sized(10.0, 10.0), ChildOf(row)))
        .id();
    let ltr = app
        .world_mut()
        .spawn((
            Node {
                direction: LayoutDirection::LeftToRight,
                ..Node::sized(10.0, 10.0)
            },
            ChildOf(root),
        ))
        .id();
    frame(&mut app);
    // TopLeft means top-right, and the offset moves it inwards.
    assert_eq!(rect(&app, start), Rect::new(615.0, 0.0, 635.0, 10.0));
    // The row is 30 wide (8 padding on the right now), centred at the bottom.
    let row_rect = rect(&app, row);
    assert_eq!(row_rect, Rect::new(305.0, 350.0, 335.0, 360.0));
    assert_eq!(rect(&app, first), Rect::new(317.0, 350.0, 327.0, 360.0));
    assert_eq!(rect(&app, second), Rect::new(305.0, 350.0, 315.0, 360.0));
    assert!(app.world().get::<ComputedNode>(first).unwrap().is_rtl());
    // A left-to-right child is still placed by its right-to-left parent, but is not mirrored
    // inside.
    assert!(!app.world().get::<ComputedNode>(ltr).unwrap().is_rtl());
}

#[test]
fn text_sizes_its_node_wraps_and_draws_crisp_pixel_glyphs() {
    let mut app = app();
    let label = app.world_mut().spawn(Text::new("Hello")).id();
    let wrapped = app
        .world_mut()
        .spawn((
            Node {
                width: Val::Px(40.0),
                anchor: Anchor::Center,
                ..Node::default()
            },
            Text::new("one two three four"),
        ))
        .id();
    frame(&mut app);
    let size = rect(&app, label).size();
    assert!(size.x > 15.0 && size.x < 40.0, "{size}");
    let line = size.y;
    assert!((12.0..=16.0).contains(&line), "{line}");
    let tall = rect(&app, wrapped).size();
    assert_eq!(tall.x, 40.0);
    assert!(tall.y >= line * 3.0, "{tall}");

    // The built-in pixel font is rasterized at UI pixels, on whole pixels, fully on or
    // off.
    let layout = app.world().get::<TextLayout>(label).unwrap();
    assert_eq!(layout.glyphs().len(), 5);
    let images = app.world().resource::<Assets<Image>>();
    for glyph in layout.glyphs() {
        assert_eq!(glyph.rect.min, glyph.rect.min.round(), "{glyph:?}");
        assert_eq!(glyph.rect.size(), glyph.source.size());
        let page = images.get(glyph.texture).unwrap();
        let mut lit = 0;
        for y in glyph.source.min.y as u32..glyph.source.max.y as u32 {
            for x in glyph.source.min.x as u32..glyph.source.max.x as u32 {
                let alpha = page.pixel(x, y).unwrap()[3];
                assert!(alpha == 0 || alpha == 255, "alpha {alpha}");
                lit += u32::from(alpha == 255);
            }
        }
        assert!(lit > 0);
    }

    // The glyphs reach the screen-space queue when the frame is drawn... and the queue is
    // emptied after drawing, so look at it from a system in the render schedule.
}

#[test]
fn smooth_fonts_rasterize_at_the_screen_scale_and_shape_arabic() {
    let mut app = app();
    let arabic = app
        .world_mut()
        .resource_mut::<Assets<Font>>()
        .add(Font::outline(ARABIC_FONT.to_vec(), FontSettings::default()).unwrap());
    let word = "مرحبا";
    let text = app
        .world_mut()
        .spawn(Text::new(word).with_font(arabic.clone()).with_size(20.0))
        .id();
    // The built-in font has no Arabic: those letters fall back to the loaded Arabic font.
    let fallback = app
        .world_mut()
        .spawn(Text::new(format!("Hi {word}")).with_size(22.0))
        .id();
    frame(&mut app);
    let layout = app.world().get::<TextLayout>(text).unwrap().clone();
    // Five letters: joined forms, plus the dot of the B as a mark of its own.
    assert_eq!(layout.glyphs().len(), 6);
    // The font's own line spacing (1.9 em) sizes the node.
    assert_eq!(rect(&app, text).height(), 38.0);
    let images = app.world().resource::<Assets<Image>>();
    let mut partial = false;
    for glyph in layout.glyphs() {
        // Twice as many texels as UI pixels: the window is at scale 2.
        assert_eq!(glyph.source.size(), glyph.rect.size() * 2.0);
        let page = images.get(glyph.texture).unwrap();
        for y in glyph.source.min.y as u32..glyph.source.max.y as u32 {
            for x in glyph.source.min.x as u32..glyph.source.max.x as u32 {
                let alpha = page.pixel(x, y).unwrap()[3];
                partial |= alpha > 0 && alpha < 255;
            }
        }
    }
    assert!(partial, "smooth glyphs have soft edges");
    let mixed = app.world().get::<TextLayout>(fallback).unwrap();
    // "Hi " in the pixel font (H, i) plus the Arabic glyphs from the fallback.
    assert!(
        mixed.glyphs().len() >= 2 + layout.glyphs().len().min(3),
        "{}",
        mixed.glyphs().len()
    );
    let pixel = mixed
        .glyphs()
        .iter()
        .filter(|g| g.rect.size() == g.source.size())
        .count();
    assert_eq!(pixel, 2);
}

#[test]
fn bitmap_fonts_lay_out_from_their_pages_with_kerning() {
    let mut app = app();
    let page = app
        .world_mut()
        .resource_mut::<Assets<Image>>()
        .add(Image::filled(32, 16, Color::WHITE));
    let fnt = "info size=8\ncommon lineHeight=10 base=8\npage id=0 file=\"p.png\"\n\
        char id=65 x=0 y=0 width=5 height=7 xoffset=0 yoffset=1 xadvance=6 page=0\n\
        char id=66 x=6 y=0 width=5 height=7 xoffset=1 yoffset=1 xadvance=6 page=0\n\
        char id=32 x=0 y=0 width=0 height=0 xoffset=0 yoffset=0 xadvance=4 page=0\n\
        kerning first=65 second=66 amount=-1";
    let bitmap = BitmapFont::parse(fnt, |_| page.clone()).unwrap();
    let font = app.world_mut().resource_mut::<Assets<Font>>().add(Font {
        kind: FontKind::Bitmap(bitmap),
        settings: FontSettings {
            pixel: true,
            ..FontSettings::default()
        },
    });
    let text = app
        .world_mut()
        .spawn(Text::new("AB A").with_font(font).with_size(16.0))
        .id();
    frame(&mut app);
    let layout = app.world().get::<TextLayout>(text).unwrap();
    let mins: Vec<Vec2> = layout.glyphs().iter().map(|g| g.rect.min).collect();
    // Twice the font's size: A at 0, B kerned one font pixel closer (6 - 1) and offset by
    // one, then B's 6 and a space of 4.
    assert_eq!(mins, [vec2(0.0, 2.0), vec2(12.0, 2.0), vec2(30.0, 2.0)]);
    assert_eq!(layout.glyphs()[0].rect.size(), vec2(10.0, 14.0));
    assert_eq!(rect(&app, text).size(), vec2(42.0, 20.0));
}

#[test]
fn buttons_click_with_the_mouse_and_show_their_state() {
    let mut app = app();
    let button = app
        .world_mut()
        .spawn((
            Node::sized(60.0, 20.0).with_offset(vec2(100.0, 50.0)),
            Button::default(),
            BackgroundColor(Color::BLACK),
            StateColors {
                normal: Color::BLACK,
                hovered: Color::GRAY,
                pressed: Color::WHITE,
                disabled: Color::RED,
            },
        ))
        .id();
    frame(&mut app);
    mouse_to(&mut app, vec2(110.0, 60.0));
    frame(&mut app);
    assert_eq!(
        *app.world().get::<Interaction>(button).unwrap(),
        Interaction::Hovered
    );
    assert_eq!(
        app.world().get::<BackgroundColor>(button).unwrap().0,
        Color::GRAY
    );
    assert!(app.world().resource::<UiPointer>().over_ui());

    app.world_mut()
        .resource_mut::<Pointers>()
        .press(PointerId::Mouse);
    frame(&mut app);
    assert_eq!(
        *app.world().get::<Interaction>(button).unwrap(),
        Interaction::Pressed
    );
    assert_eq!(
        app.world().get::<BackgroundColor>(button).unwrap().0,
        Color::WHITE
    );
    app.world_mut()
        .resource_mut::<Pointers>()
        .release(PointerId::Mouse);
    frame(&mut app);
    assert_eq!(app.world().resource::<Seen>().clicks, [button]);

    // Released somewhere else: no click.
    app.world_mut()
        .resource_mut::<Pointers>()
        .press(PointerId::Mouse);
    frame(&mut app);
    mouse_to(&mut app, vec2(300.0, 300.0));
    app.world_mut()
        .resource_mut::<Pointers>()
        .release(PointerId::Mouse);
    frame(&mut app);
    assert_eq!(app.world().resource::<Seen>().clicks.len(), 1);
    assert!(!app.world().resource::<UiPointer>().over_ui());

    // Disabled buttons ignore the pointer.
    app.world_mut().get_mut::<Button>(button).unwrap().disabled = true;
    mouse_to(&mut app, vec2(110.0, 60.0));
    frame(&mut app);
    app.world_mut()
        .resource_mut::<Pointers>()
        .press(PointerId::Mouse);
    frame(&mut app);
    app.world_mut()
        .resource_mut::<Pointers>()
        .release(PointerId::Mouse);
    frame(&mut app);
    assert_eq!(app.world().resource::<Seen>().clicks.len(), 1);
    assert_eq!(
        app.world().get::<BackgroundColor>(button).unwrap().0,
        Color::RED
    );
}

/// A column menu of three buttons and a slider, in a full-screen root.
fn menu(app: &mut App) -> (Entity, [Entity; 3], Entity) {
    let root = app
        .world_mut()
        .spawn(Node {
            anchor: Anchor::Center,
            layout: Layout::Column,
            gap: 4.0,
            ..Node::default()
        })
        .id();
    let buttons = [0, 1, 2].map(|_| {
        app.world_mut()
            .spawn((Node::sized(80.0, 16.0), Button::default(), ChildOf(root)))
            .id()
    });
    let slider = app
        .world_mut()
        .spawn((Node::sized(80.0, 8.0), Slider::new(0.5, 0.1), ChildOf(root)))
        .id();
    (root, buttons, slider)
}

#[test]
fn focus_moves_with_the_ui_actions_and_accept_clicks() {
    let mut app = app();
    let (_, buttons, slider) = menu(&mut app);
    frame(&mut app);
    assert_eq!(app.world().resource::<UiFocus>().get(), None);
    tap(&mut app, actions::DOWN);
    assert_eq!(app.world().resource::<UiFocus>().get(), Some(buttons[0]));
    assert_eq!(
        *app.world().get::<Interaction>(buttons[0]).unwrap(),
        Interaction::Hovered
    );
    tap(&mut app, actions::DOWN);
    tap(&mut app, actions::DOWN);
    assert_eq!(app.world().resource::<UiFocus>().get(), Some(buttons[2]));
    tap(&mut app, actions::ACCEPT);
    assert_eq!(app.world().resource::<Seen>().clicks, [buttons[2]]);

    // On the slider, left and right change the value instead of moving.
    tap(&mut app, actions::DOWN);
    assert_eq!(app.world().resource::<UiFocus>().get(), Some(slider));
    tap(&mut app, actions::RIGHT);
    let value = app.world().get::<Slider>(slider).unwrap().value;
    assert!((value - 0.6).abs() < 1e-5, "{value}");
    assert_eq!(app.world().resource::<Seen>().changes.len(), 1);
    // Past the bottom the focus wraps to the top.
    tap(&mut app, actions::DOWN);
    assert_eq!(app.world().resource::<UiFocus>().get(), Some(buttons[0]));

    // Held down, the action repeats.
    app.world_mut()
        .resource_mut::<VirtualInput>()
        .press(actions::DOWN);
    for _ in 0..30 {
        frame(&mut app);
    }
    app.world_mut()
        .resource_mut::<VirtualInput>()
        .release(actions::DOWN);
    frame(&mut app);
    // 0.5 s: the first press, then repeats after 0.4 s every 0.11 s.
    assert_ne!(app.world().resource::<UiFocus>().get(), Some(buttons[1]));

    // Paused (a "press a key" prompt), the actions neither move nor click.
    let focused = app.world().resource::<UiFocus>().get();
    app.world_mut().resource_mut::<UiFocus>().set_paused(true);
    tap(&mut app, actions::DOWN);
    tap(&mut app, actions::ACCEPT);
    assert_eq!(app.world().resource::<UiFocus>().get(), focused);
    assert_eq!(app.world().resource::<Seen>().clicks.len(), 1);
    app.world_mut().resource_mut::<UiFocus>().set_paused(false);
    tap(&mut app, actions::ACCEPT);
    assert_eq!(app.world().resource::<Seen>().clicks.len(), 2);
}

#[test]
fn keyboard_defaults_are_bound_and_kept_when_the_map_is_replaced() {
    let mut app = app();
    let (_, buttons, _) = menu(&mut app);
    frame(&mut app);
    app.insert_resource(cg_platform::input::ActionMap::new().bind("jump", KeyCode::Space));
    frame(&mut app);
    app.world_mut()
        .resource_mut::<ButtonInput<KeyCode>>()
        .press(KeyCode::ArrowDown);
    frame(&mut app);
    assert_eq!(app.world().resource::<UiFocus>().get(), Some(buttons[0]));
    let map = app.world().resource::<cg_platform::input::ActionMap>();
    assert!(!map.bindings("jump").is_empty() && !map.bindings(actions::UP).is_empty());
}

#[test]
fn a_newer_menu_on_top_takes_the_focus() {
    let mut app = app();
    let (_, buttons, _) = menu(&mut app);
    frame(&mut app);
    tap(&mut app, actions::DOWN);
    assert_eq!(app.world().resource::<UiFocus>().get(), Some(buttons[0]));
    let dialog = app.world_mut().spawn((Node::fill(), ZIndex(5))).id();
    let ok = app
        .world_mut()
        .spawn((
            Node::sized(40.0, 16.0).at(Anchor::Center),
            Button::default(),
            Focusable { auto_focus: true },
            ChildOf(dialog),
        ))
        .id();
    frame(&mut app);
    frame(&mut app);
    // The old focus is under the dialog; it moves to the dialog's button.
    assert_eq!(app.world().resource::<UiFocus>().get(), Some(ok));
    tap(&mut app, actions::DOWN);
    assert_eq!(app.world().resource::<UiFocus>().get(), Some(ok));
    app.world_mut()
        .entity_mut(dialog)
        .insert(Visibility::Hidden);
    frame(&mut app);
    frame(&mut app);
    assert!(buttons.contains(&app.world().resource::<UiFocus>().get().unwrap()));
}

#[test]
fn sliders_follow_drags_and_run_backwards_right_to_left() {
    let mut app = app();
    let slider = app
        .world_mut()
        .spawn((
            Node::sized(100.0, 10.0).with_offset(vec2(10.0, 10.0)),
            Slider::new(0.0, 0.0),
        ))
        .id();
    let fill = app
        .world_mut()
        .spawn((
            Node {
                height: Val::Fill,
                ..Node::default()
            },
            BarFill,
            ChildOf(slider),
        ))
        .id();
    frame(&mut app);
    mouse_to(&mut app, vec2(35.0, 15.0));
    frame(&mut app);
    app.world_mut()
        .resource_mut::<Pointers>()
        .press(PointerId::Mouse);
    frame(&mut app);
    assert!((app.world().get::<Slider>(slider).unwrap().value - 0.25).abs() < 1e-5);
    mouse_to(&mut app, vec2(85.0, 40.0));
    frame(&mut app);
    assert!((app.world().get::<Slider>(slider).unwrap().value - 0.75).abs() < 1e-5);
    frame(&mut app);
    assert_eq!(rect(&app, fill), Rect::new(10.0, 10.0, 85.0, 20.0));
    app.world_mut()
        .resource_mut::<Pointers>()
        .release(PointerId::Mouse);
    frame(&mut app);

    app.world_mut().get_mut::<Node>(slider).unwrap().direction = LayoutDirection::RightToLeft;
    frame(&mut app);
    frame(&mut app);
    // At 0.75 the fill runs from the right edge.
    assert_eq!(rect(&app, fill), Rect::new(35.0, 10.0, 110.0, 20.0));
    app.world_mut().resource_mut::<UiFocus>().set(Some(slider));
    tap(&mut app, actions::LEFT);
    let value = app.world().get::<Slider>(slider).unwrap().value;
    assert!((value - 0.85).abs() < 1e-5, "{value}");
}

#[test]
fn touch_controls_press_actions_and_hide_on_keys() {
    let mut app = app();
    let jump = app
        .world_mut()
        .spawn(
            Node::sized(30.0, 30.0)
                .at(Anchor::BottomRight)
                .with_offset(vec2(-10.0, -10.0)),
        )
        .id();
    app.world_mut()
        .entity_mut(jump)
        .insert(TouchButton::new("jump"));
    let pad = app
        .world_mut()
        .spawn((
            Node::sized(60.0, 60.0)
                .at(Anchor::BottomLeft)
                .with_offset(vec2(10.0, -10.0)),
            TouchDpad::default(),
        ))
        .id();
    frame(&mut app);
    assert!(!app.world().resource::<TouchControls>().visible());
    assert!(!app.world().get::<ComputedNode>(jump).unwrap().is_visible());

    let size = app.world().resource::<Window>().logical_size();
    let finger = PointerId::Touch(7);
    let at = rect(&app, jump).center() * 2.0;
    let mut pointers = app.world_mut().resource_mut::<Pointers>();
    pointers.move_to(finger, at, size);
    pointers.press(finger);
    frame(&mut app);
    // The first touch shows the controls; from the next frame they work.
    assert!(app.world().resource::<TouchControls>().visible());
    frame(&mut app);
    assert!(app.world().resource::<ActionState>().pressed("jump"));
    app.world_mut().resource_mut::<Pointers>().release(finger);
    frame(&mut app);
    frame(&mut app);
    assert!(!app.world().resource::<ActionState>().pressed("jump"));

    // A finger on the pad's left part presses left, and keeps steering past its edge.
    let thumb = PointerId::Touch(8);
    let pad_rect = rect(&app, pad);
    let mut pointers = app.world_mut().resource_mut::<Pointers>();
    pointers.move_to(thumb, (pad_rect.center() - vec2(20.0, 0.0)) * 2.0, size);
    pointers.press(thumb);
    frame(&mut app);
    assert!(app.world().resource::<ActionState>().pressed("left"));
    app.world_mut().resource_mut::<Pointers>().move_to(
        thumb,
        (pad_rect.center() + vec2(200.0, -150.0)) * 2.0,
        size,
    );
    frame(&mut app);
    let actions = app.world().resource::<ActionState>();
    assert!(actions.pressed("right") && actions.pressed("up") && !actions.pressed("left"));
    app.world_mut().resource_mut::<Pointers>().release(thumb);
    frame(&mut app);

    // A key press hides them again.
    app.world_mut()
        .resource_mut::<ButtonInput<KeyCode>>()
        .press(KeyCode::KeyX);
    frame(&mut app);
    assert!(!app.world().resource::<TouchControls>().visible());
}

#[test]
fn touch_controls_show_from_the_start_on_touch_screens() {
    let mut app = app();
    app.insert_resource(Window::with_size(uvec2(1280, 720), 1.0).with_touch_screen(true));
    frame(&mut app);
    assert!(app.world().resource::<TouchControls>().visible());
    let mut never = app_with(UiPlugin {
        touch: TouchMode::Never,
        ..UiPlugin::default()
    });
    never.insert_resource(Window::with_size(uvec2(1280, 720), 1.0).with_touch_screen(true));
    frame(&mut never);
    assert!(!never.world().resource::<TouchControls>().visible());
}

fn strings(
    language: &str,
    rtl: bool,
    font: Option<Handle<Font>>,
    pairs: &[(&str, &str)],
) -> Strings {
    Strings {
        language: language.into(),
        name: language.into(),
        right_to_left: rtl,
        font,
        font_scale: if rtl { 1.5 } else { 1.0 },
        strings: pairs
            .iter()
            .map(|(k, v)| ((*k).to_owned(), (*v).to_owned()))
            .collect(),
    }
}

#[test]
fn localized_text_follows_the_language_its_font_and_direction() {
    let mut app = app();
    let arabic = app
        .world_mut()
        .resource_mut::<Assets<Font>>()
        .add(Font::outline(ARABIC_FONT.to_vec(), FontSettings::default()).unwrap());
    let mut assets = app.world_mut().resource_mut::<Assets<Strings>>();
    let en = assets.add(strings(
        "en",
        false,
        None,
        &[("play", "Play"), ("coins", "Coins: {n}")],
    ));
    let ar = assets.add(strings(
        "ar",
        true,
        Some(arabic.clone()),
        &[("play", "العب")],
    ));
    app.insert_resource(Locale::new(
        [("en".into(), en), ("ar".into(), ar)],
        "en",
        "en",
    ));
    let root = app.world_mut().spawn(Node::fill()).id();
    let play = app
        .world_mut()
        .spawn((Localized::new("play"), ChildOf(root)))
        .id();
    let coins = app
        .world_mut()
        .spawn((
            Node::default().at(Anchor::Bottom),
            Localized::new("coins").with_arg("n", 3),
            ChildOf(root),
        ))
        .id();
    frame(&mut app);
    let text = |app: &App, e| app.world().get::<Text>(e).unwrap().clone();
    assert_eq!(text(&app, play).value, "Play");
    assert_eq!(text(&app, coins).value, "Coins: 3");
    assert_eq!(rect(&app, play).min.x, 0.0);

    app.world_mut().resource_mut::<Locale>().set_language("ar");
    frame(&mut app);
    frame(&mut app);
    let ar_play = text(&app, play);
    assert_eq!(ar_play.value, "العب");
    assert_eq!(ar_play.style.font, arabic);
    assert_eq!(ar_play.style.size, DEFAULT_FONT_SIZE * 1.5);
    // Missing in Arabic: from English, still in the language's font.
    assert_eq!(text(&app, coins).value, "Coins: 3");
    // Right to left: the top-left text moved to the top-right.
    assert_eq!(rect(&app, play).max.x, 640.0);
    assert!(
        !app.world()
            .get::<TextLayout>(play)
            .unwrap()
            .glyphs()
            .is_empty()
    );

    app.world_mut()
        .get_mut::<Localized>(coins)
        .unwrap()
        .set_arg("n", 4);
    app.world_mut().resource_mut::<Locale>().set_language("en");
    frame(&mut app);
    assert_eq!(text(&app, coins).value, "Coins: 4");
    assert_eq!(text(&app, play).style.font, Handle::default());
}

#[test]
fn the_ui_is_queued_in_draw_order_as_screen_sprites() {
    #[derive(Resource, Default)]
    struct Queued(Vec<(Rect, Color, i32)>);
    fn grab(sprites: Res<Sprites>, mut queued: ResMut<Queued>) {
        queued.0 = sprites
            .screen_queued()
            .iter()
            .map(|s| {
                let size = s.size.unwrap_or(Vec2::ONE);
                let min = Vec2::new(
                    s.transform.translation.x,
                    s.transform.translation.y - size.y,
                );
                (Rect::from_min_size(min, size), s.color, s.layer)
            })
            .collect();
    }
    let mut app = app();
    app.init_resource::<Queued>().add_systems(
        cg_core::Render,
        grab.after(UiSystems::Draw)
            .before(cg_render::RenderSystems::Draw),
    );
    let panel = app
        .world_mut()
        .spawn((
            Node {
                border: 1.0,
                ..Node::sized(40.0, 20.0)
            },
            BackgroundColor(Color::BLUE),
            BorderColor(Color::WHITE),
            ZIndex(3),
        ))
        .id();
    app.world_mut()
        .spawn((Node::sized(10.0, 10.0), BackgroundColor(Color::RED)));
    app.world_mut().spawn((
        Node::sized(10.0, 10.0),
        BackgroundColor(Color::GREEN),
        Visibility::Hidden,
    ));
    let label = app.world_mut().spawn((Text::new("A"), ChildOf(panel))).id();
    frame(&mut app);
    frame(&mut app);
    let queued = &app.world().resource::<Queued>().0;
    // The red square (z 0) first, then the panel (z 3): background, four border pieces and
    // the glyph. The hidden green square is not drawn.
    assert_eq!(queued.len(), 7, "{queued:?}");
    assert_eq!((queued[0].1, queued[0].2), (Color::RED, 0));
    assert_eq!(queued[1], (Rect::new(0.0, 0.0, 40.0, 20.0), Color::BLUE, 3));
    assert_eq!(queued[2].0, Rect::new(0.0, 0.0, 40.0, 1.0));
    assert!(queued[1..].iter().all(|q| q.2 == 3));
    assert!(queued.iter().all(|q| q.1 != Color::GREEN));
    // The label sits inside the border.
    assert_eq!(rect(&app, label).min, vec2(1.0, 1.0));
}

#[test]
fn a_font_without_its_file_loaded_yet_draws_nothing_and_recovers() {
    let mut app = app();
    let id = cg_assets::AssetId::<Font>::reserved(5);
    let label = app
        .world_mut()
        .spawn(Text::new("Later").with_font(Handle::weak(id)))
        .id();
    frame(&mut app);
    assert!(
        app.world()
            .get::<TextLayout>(label)
            .unwrap()
            .glyphs()
            .is_empty()
    );
    app.world_mut().resource_mut::<Assets<Font>>().insert(
        id,
        Font::outline(DEFAULT_FONT_DATA.to_vec(), FontSettings::default()).unwrap(),
    );
    frame(&mut app);
    assert_eq!(
        app.world().get::<TextLayout>(label).unwrap().glyphs().len(),
        5
    );
}

#[cfg(feature = "debug-overlay")]
#[test]
fn the_debug_overlay_opens_with_its_key_and_builds_its_windows() {
    let mut app = app_with(UiPlugin {
        overlay_open: false,
        ..UiPlugin::default()
    });
    let button = app
        .world_mut()
        .spawn((
            Node::sized(40.0, 20.0),
            Button::default(),
            BackgroundColor(Color::BLUE),
        ))
        .id();
    frame(&mut app);
    assert!(app.world().resource::<EguiContext>().get().is_none());
    app.world_mut()
        .resource_mut::<ButtonInput<KeyCode>>()
        .press(KeyCode::F12);
    frame(&mut app);
    assert!(app.world().resource::<DebugOverlay>().open);
    {
        let mut overlay = app.world_mut().resource_mut::<DebugOverlay>();
        overlay.selected = Some(button);
        overlay.ui_outlines = true;
    }
    for _ in 0..3 {
        frame(&mut app);
    }
    assert_eq!(
        app.world().resource::<DebugOverlay>().frame_times().len(),
        5
    );
    // The pass ended in `Render`, so game systems no longer see the context.
    assert!(app.world().resource::<EguiContext>().get().is_none());
    // A mouse over the overlay's window blocks the game UI under it.
    mouse_to(&mut app, vec2(20.0, 20.0));
    frame(&mut app);
    frame(&mut app);
    assert!(app.world().resource::<UiPointer>().blocked);
    app.world_mut().despawn(button);
    frame(&mut app);
    assert_eq!(app.world().resource::<DebugOverlay>().selected, None);
}
