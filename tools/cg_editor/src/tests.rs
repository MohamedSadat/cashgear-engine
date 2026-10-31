// Tests write a small asset folder to a temporary directory.

use crate::doc::{Document, Selected};
use crate::tmx::PropertyData;
use crate::value::Value;
use crate::{Editor, EditorPlugin, Mode, PlayMode, Prompt, Then, play, view};
use cashgear_engine::audio::AudioOutput;
use cashgear_engine::platform::input::{
    ButtonInput, InputPlugin, MouseButton, PointerId, Pointers,
};
use cashgear_engine::prelude::*;
use cashgear_engine::script::ScriptPlugin;
use cashgear_engine::tiled::TiledObject;
use cashgear_engine::world::PrefabRegistry;
use std::path::{Path, PathBuf};

const TILESET: &str = r#"<?xml version="1.0" encoding="UTF-8"?>
<tileset version="1.10" tiledversion="1.10.2" name="tiles" tilewidth="8" tileheight="8" tilecount="4" columns="2">
 <image source="../images/tiles.png" width="16" height="16"/>
 <tile id="1" type="Ground"><properties><property name="solid" type="bool" value="true"/></properties></tile>
 <tile id="3" type="Gem"><properties><property name="value" type="int" value="5"/></properties></tile>
</tileset>
"#;

/// A 6×4 map of 8-pixel tiles with a floor and three objects: a gem tile, a box that
/// falls (its prefab has a kinematic body) and a point.
const MAP: &str = r#"<?xml version="1.0" encoding="UTF-8"?>
<map version="1.10" tiledversion="1.10.2" orientation="orthogonal" renderorder="right-down" width="6" height="4" tilewidth="8" tileheight="8" infinite="0" nextlayerid="3" nextobjectid="4">
 <tileset firstgid="1" source="../tilesets/tiles.tsx"/>
 <layer id="1" name="ground" width="6" height="4">
  <properties>
   <property name="collision" type="bool" value="true"/>
  </properties>
  <data encoding="csv">
0,0,0,0,0,0,
0,0,0,0,0,0,
0,0,0,0,0,0,
2,2,2,2,2,2
</data>
 </layer>
 <objectgroup id="2" name="things">
  <object id="1" gid="4" x="8" y="24" width="8" height="8"/>
  <object id="2" name="box" type="Faller" x="24" y="0" width="8" height="8"/>
  <object id="3" name="start" type="Start" x="40" y="16">
   <point/>
  </object>
 </objectgroup>
</map>
"#;

const FALLER: &str = r#"// Falls when the scene plays.
(
    components: {
        "Collider": (shape: Rect((8, 8))),
        "KinematicBody": (),
    },
    children: [
        (
            components: {
                "Transform2D": (translation: (0, 6)),
            },
        ),
    ],
)
"#;

struct Folder(PathBuf);

impl Folder {
    fn new(name: &str) -> Self {
        let dir = std::env::temp_dir().join(format!("cg_editor_{name}_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let folder = Self(dir);
        let image = Image::from_fn(16, 16, |x, y| [(x * 16) as u8, (y * 16) as u8, 90, 255]);
        folder.write_bytes("images/tiles.png", &image.to_png().unwrap());
        folder.write("tilesets/tiles.tsx", TILESET);
        folder.write("maps/room.tmx", MAP);
        folder.write("prefabs/faller.prefab.ron", FALLER);
        folder
    }

    fn write(&self, path: &str, text: &str) {
        self.write_bytes(path, text.as_bytes());
    }

    fn write_bytes(&self, path: &str, bytes: &[u8]) {
        let full = self.0.join(path);
        std::fs::create_dir_all(full.parent().unwrap()).unwrap();
        std::fs::write(full, bytes).unwrap();
    }

    fn read(&self, path: &str) -> String {
        std::fs::read_to_string(self.0.join(path)).unwrap()
    }
}

impl Drop for Folder {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

/// The engine without a window, and the editor in its scene mode, opening `open`.
fn app(folder: &Folder, open: &str) -> App {
    app_in(&folder.0, open)
}

/// The same for the asset folder `root`.
fn app_in(root: &Path, open: &str) -> App {
    let mut app = App::new();
    app.add_plugins(AssetPlugin {
        root: Some(root.to_string_lossy().into_owned()),
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
    .add_plugins(UiPlugin::default())
    .add_plugins(ScriptPlugin {
        show_errors: false,
        ..default()
    })
    .add_plugins(EditorPlugin {
        load_prefabs: true,
        ..EditorPlugin::new(PlayMode::Scene).open(open)
    })
    .add_message::<Lifecycle>();
    app.insert_resource(Window::with_size(UVec2::new(1280, 720), 1.0));
    app
}

fn frames(app: &mut App, n: usize) {
    for _ in 0..n {
        app.update(core::time::Duration::from_millis(16));
    }
}

fn editor(app: &mut App) -> Mut<'_, Editor> {
    app.world_mut().resource_mut::<Editor>()
}

/// The x and y of the scene's entity for map object `id`.
fn object_at(app: &mut App, id: u32) -> Option<Vec2> {
    let scene = app.world().resource::<Editor>().scene?;
    let world = app.world_mut();
    let mut query = world.query::<(&TiledObject, &ChildOf, &GlobalTransform2D)>();
    query
        .iter(world)
        .find(|(o, parent, _)| o.id == id && parent.parent() == scene)
        .map(|(.., t)| t.translation())
}

fn x_of(app: &App, id: u32) -> f32 {
    app.world()
        .resource::<Editor>()
        .map()
        .unwrap()
        .find(id)
        .unwrap()
        .1
        .x
}

#[test]
fn map_objects_move_undo_and_save_in_place() {
    let folder = Folder::new("save");
    let mut app = app(&folder, "maps/room.tmx");
    frames(&mut app, 5);
    assert_eq!(
        object_at(&mut app, 1),
        Some(vec2(12.0, 12.0)),
        "the gem's centre"
    );

    editor(&mut app).select_object(1);
    assert_eq!(x_of(&app, 1), 8.0);
    app.world_mut()
        .resource_scope::<Editor, _>(|world, mut editor| {
            view::nudge(&mut editor, world, vec2(8.0, 0.0));
        });
    assert_eq!(x_of(&app, 1), 16.0);
    frames(&mut app, 2);
    assert_eq!(
        object_at(&mut app, 1),
        Some(vec2(20.0, 12.0)),
        "the view follows"
    );

    editor(&mut app).undo();
    assert_eq!(x_of(&app, 1), 8.0);
    editor(&mut app).redo();
    assert_eq!(x_of(&app, 1), 16.0);
    assert_eq!(editor(&mut app).history().undo_label(), Some("move"));

    // A property, then save.
    editor(&mut app).edit("edit", None, |doc, _| {
        if let Document::Map(map) = doc {
            let gem = map.find_mut(1).unwrap();
            crate::doc::set_property(gem, "value", PropertyData::Int(9));
        }
    });
    app.world_mut()
        .resource_scope::<Editor, _>(|world, mut editor| assert!(crate::save(&mut editor, world)));
    let saved = folder.read("maps/room.tmx");
    // Only the gem changed: the rest of the file is as it was.
    let gem = r#"  <object id="1" gid="4" x="8" y="24" width="8" height="8"/>"#;
    let (head, tail) = MAP.split_once(gem).unwrap();
    assert!(saved.starts_with(head) && saved.ends_with(tail), "{saved}");
    assert_eq!(
        &saved[head.len()..saved.len() - tail.len()],
        "  <object id=\"1\" gid=\"4\" x=\"16\" y=\"24\" width=\"8\" height=\"8\">
   <properties>
    <property name=\"value\" type=\"int\" value=\"9\"/>
   </properties>
  </object>"
    );
    assert!(!editor(&mut app).document().unwrap().dirty());

    // The game's own map reader (the `tiled` crate) reads the saved file the same way.
    app.world()
        .resource::<AssetServer>()
        .reload("maps/room.tmx");
    frames(&mut app, 3);
    let editor = app.world().resource::<Editor>();
    let doc = editor.map().unwrap();
    let maps = app.world().resource::<Assets<TiledMap>>();
    let (_, gem) = maps
        .get(&doc.handle)
        .unwrap()
        .objects()
        .find(|(_, o)| o.id == 1)
        .unwrap();
    assert_eq!(
        (gem.position, gem.class.as_str()),
        (vec2(16.0, 24.0), "Gem")
    );
    assert_eq!(gem.properties.int("value"), Some(9));
    assert!(
        doc.objects == doc.saved && !doc.conflict,
        "the reload is the editor's own save"
    );
}

#[test]
fn placing_deleting_and_duplicating() {
    let folder = Folder::new("place");
    let mut app = app(&folder, "maps/room.tmx");
    frames(&mut app, 5);
    app.world_mut()
        .resource_scope::<Editor, _>(|world, mut editor| {
            let placement = crate::Placement::Tile {
                tileset: 0,
                tile: 3,
            };
            view::place(&mut editor, world, &placement, vec2(31.0, 13.0), true);
            let map = editor.map().unwrap();
            let placed = map.objects.layers[0].objects.last().unwrap();
            // The tile around the pointer, its corner on the 8-pixel grid.
            assert_eq!(
                (placed.id, placed.gid, placed.x, placed.y),
                (4, 4, 24.0, 24.0)
            );
            assert_eq!(map.objects.next_object_id, 5);
            assert_eq!(editor.selection(), [Selected::Object(4)]);
            view::duplicate(&mut editor, world);
            assert_eq!(editor.selection(), [Selected::Object(5)]);
            assert_eq!(editor.map().unwrap().find(5).unwrap().1.x, 32.0);
            view::delete(&mut editor);
            view::delete(&mut editor);
            assert!(editor.map().unwrap().find(5).is_none());
            editor.undo();
            assert!(editor.map().unwrap().find(5).is_some());
        });
}

#[test]
fn play_runs_the_scene_and_stop_puts_it_back() {
    let folder = Folder::new("play");
    let mut app = app(&folder, "maps/room.tmx");
    frames(&mut app, 10);
    let start = object_at(&mut app, 2).expect("the box");
    // While editing, nothing simulates: the box stays in the air.
    frames(&mut app, 30);
    assert_eq!(object_at(&mut app, 2), Some(start));
    let before = play::entities(app.world_mut()).len();

    editor(&mut app).request_play();
    frames(&mut app, 60);
    assert_eq!(editor(&mut app).mode, Mode::Playing);
    let fallen = object_at(&mut app, 2).unwrap();
    assert!(
        fallen.y < start.y - 4.0,
        "the box fell to {fallen} from {start}"
    );
    // Something made while playing goes again at Stop.
    app.world_mut()
        .spawn((Name::new("made in play"), Transform2D::default()));

    app.world_mut()
        .resource_scope::<Editor, _>(|world, mut editor| play::stop(&mut editor, world));
    frames(&mut app, 6);
    assert_eq!(editor(&mut app).mode, Mode::Editing);
    assert_eq!(object_at(&mut app, 2), Some(start), "the box is back");
    assert_eq!(play::entities(app.world_mut()).len(), before);
    assert!(app.world().resource::<Time>().fixed_paused());
    assert!(!editor(&mut app).document().unwrap().dirty());
}

/// The standalone editor's download: the scripting example's level, played without the
/// example's Rust, with the editor's controls. Its Lua player runs right while the right
/// arrow is held.
#[test]
fn the_scripting_example_plays_with_the_standalone_controls() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples/scripting/assets");
    let mut app = app_in(&root, "maps/playground.tmx");
    app.insert_resource(crate::play_controls());
    let player_x = |app: &mut App| {
        let world = app.world_mut();
        let mut query = world.query::<(&Name, &GlobalTransform2D)>();
        query
            .iter(world)
            .find(|(name, _)| name.as_str() == "player")
            .map(|(_, t)| t.translation().x)
    };
    for _ in 0..600 {
        if player_x(&mut app).is_some() {
            break;
        }
        frames(&mut app, 1);
    }
    assert!(
        player_x(&mut app).is_some(),
        "the map's player never spawned"
    );

    editor(&mut app).request_play();
    // It lands at its start, then runs.
    frames(&mut app, 60);
    assert_eq!(editor(&mut app).mode, Mode::Playing);
    let start = player_x(&mut app).expect("the player while playing");
    app.world_mut()
        .resource_mut::<ButtonInput<KeyCode>>()
        .press(KeyCode::ArrowRight);
    frames(&mut app, 30);
    let ran = player_x(&mut app).unwrap();
    assert!(ran > start + 24.0, "the player ran from {start} to {ran}");
}

#[test]
fn prefabs_edit_reparent_and_save() {
    let folder = Folder::new("prefab");
    let mut app = app(&folder, "prefabs/faller.prefab.ron");
    frames(&mut app, 3);
    let root = editor(&mut app).scene.expect("the preview");
    assert!(app.world().get::<KinematicBody>(root).is_some());

    // A child gets a child of its own, by dragging in the hierarchy.
    editor(&mut app).edit("add child", None, |doc, _| {
        if let Document::Prefab(prefab) = doc {
            prefab.root.children.push(crate::doc::PrefabNode::default());
            assert!(prefab.root.reparent(&[1], &[0]));
        }
    });
    // The collider grows, field by field.
    editor(&mut app).edit("edit", None, |doc, _| {
        if let Document::Prefab(prefab) = doc {
            let (_, collider) = &mut prefab.root.components[0];
            let Some(Value::NewtypeVariant { value, .. }) = collider.field_mut("shape") else {
                panic!("a shape: {collider:?}");
            };
            **value = Value::from_serialize(&vec2(10.0, 12.0)).unwrap();
        }
    });
    frames(&mut app, 2);
    let root = editor(&mut app).scene.unwrap();
    let child = app.world().get::<Children>(root).unwrap()[0];
    assert_eq!(app.world().get::<Children>(child).map(|c| c.len()), Some(1));

    app.world_mut()
        .resource_scope::<Editor, _>(|world, mut editor| assert!(crate::save(&mut editor, world)));
    let saved = folder.read("prefabs/faller.prefab.ron");
    assert!(
        saved.starts_with("// Falls when the scene plays.\n("),
        "{saved}"
    );
    assert!(saved.contains("shape: Rect((10, 12))"), "{saved}");
    let registry = app.world().resource::<PrefabRegistry>();
    let prefab = Prefab::from_ron(&saved, registry).unwrap();
    assert_eq!(prefab.children()[0].children().len(), 1);
}

#[test]
fn a_map_saved_elsewhere_reloads_or_asks() {
    let folder = Folder::new("disk");
    let mut app = app(&folder, "maps/room.tmx");
    frames(&mut app, 5);
    // Saved in Tiled with no edits here: the editor follows.
    folder.write(
        "maps/room.tmx",
        &MAP.replace(r#"x="40" y="16""#, r#"x="48" y="16""#),
    );
    app.world()
        .resource::<AssetServer>()
        .reload("maps/room.tmx");
    frames(&mut app, 3);
    assert_eq!(x_of(&app, 3), 48.0);
    assert!(!editor(&mut app).document().unwrap().dirty());

    // Edits here and there: the editor keeps its own and asks.
    app.world_mut()
        .resource_scope::<Editor, _>(|world, mut editor| {
            editor.select_object(1);
            view::nudge(&mut editor, world, vec2(8.0, 0.0));
        });
    folder.write(
        "maps/room.tmx",
        &MAP.replace(r#"x="40" y="16""#, r#"x="56" y="16""#),
    );
    app.world()
        .resource::<AssetServer>()
        .reload("maps/room.tmx");
    frames(&mut app, 3);
    let editor = app.world().resource::<Editor>();
    let map = editor.map().unwrap();
    assert!(map.conflict);
    assert_eq!(map.find(1).unwrap().1.x, 16.0, "the edit stays");
}

#[test]
fn closing_with_unsaved_changes_asks_first() {
    let folder = Folder::new("close");
    let mut app = app(&folder, "maps/room.tmx");
    frames(&mut app, 5);
    app.world_mut()
        .resource_scope::<Editor, _>(|world, mut editor| {
            editor.select_object(3);
            view::nudge(&mut editor, world, vec2(8.0, 0.0));
        });
    app.world_mut().write_message(Lifecycle::CloseRequested);
    frames(&mut app, 1);
    assert_eq!(editor(&mut app).prompt, Some(Prompt::Unsaved(Then::Quit)));
    assert!(app.should_exit().is_none());

    editor(&mut app).prompt = None;
    editor(&mut app).undo();
    app.world_mut().write_message(Lifecycle::CloseRequested);
    frames(&mut app, 1);
    assert_eq!(app.should_exit(), Some(AppExit::Success));
}

/// Where a world point shows in the window while editing, in logical pixels.
fn on_screen(app: &App, world: Vec2) -> Vec2 {
    let editor = app.world().resource::<Editor>();
    let window = app.world().resource::<Window>();
    view::edit_view(editor).world_to_screen(world, window)
}

/// Puts the mouse at `at` with the left button `down` or not, then runs a frame.
fn mouse(app: &mut App, at: Vec2, down: bool) {
    let size = app.world().resource::<Window>().logical_size();
    let world = app.world_mut();
    world
        .resource_mut::<Pointers>()
        .move_to(PointerId::Mouse, at, size);
    let mut buttons = world.resource_mut::<ButtonInput<MouseButton>>();
    match (down, buttons.pressed(MouseButton::Left)) {
        (true, false) => {
            buttons.press(MouseButton::Left);
            world.resource_mut::<Pointers>().press(PointerId::Mouse);
        }
        (false, true) => {
            buttons.release(MouseButton::Left);
            world.resource_mut::<Pointers>().release(PointerId::Mouse);
        }
        _ => {}
    }
    frames(app, 1);
}

#[test]
fn the_viewport_picks_drags_boxes_resizes_and_zooms() {
    let folder = Folder::new("mouse");
    let mut app = app(&folder, "maps/room.tmx");
    frames(&mut app, 5);
    let zoom = editor(&mut app).view.zoom;
    assert!(
        zoom >= 1.0,
        "a small map is framed whole, zoomed in: {zoom}"
    );

    // Click the gem and drag it one grid step right: it snaps to the grid.
    let gem = on_screen(&app, vec2(12.0, 12.0));
    mouse(&mut app, gem, true);
    assert_eq!(editor(&mut app).selection(), [Selected::Object(1)]);
    mouse(&mut app, gem + vec2(6.0 * zoom, 1.0), true);
    mouse(&mut app, gem + vec2(9.0 * zoom, 2.0), true);
    mouse(&mut app, gem + vec2(9.0 * zoom, 2.0), false);
    assert_eq!(x_of(&app, 1), 16.0);
    // The whole drag is one step to undo.
    editor(&mut app).undo();
    assert_eq!(x_of(&app, 1), 8.0);
    assert_eq!(editor(&mut app).history().undo_label(), None);

    // A box across the whole view selects all three objects.
    let view = editor(&mut app).viewport;
    // Clear of the panels' resize handles at the edges.
    let (from, to) = (
        vec2(view.min.x + 12.0, view.min.y + 12.0),
        vec2(view.max.x - 12.0, view.max.y - 12.0),
    );
    mouse(&mut app, from, true);
    mouse(&mut app, to, true);
    mouse(&mut app, to, false);
    assert_eq!(editor(&mut app).selection().len(), 3);

    // The box object, resized by its bottom-right handle: its top-left corner stays.
    editor(&mut app).select_object(2);
    editor(&mut app).tool = crate::Tool::Scale;
    frames(&mut app, 1);
    let at = on_screen(&app, vec2(32.0, 24.0));
    mouse(&mut app, at, true);
    let at = on_screen(&app, vec2(39.0, 17.0));
    mouse(&mut app, at, true);
    let at = on_screen(&app, vec2(39.0, 17.0));
    mouse(&mut app, at, false);
    let map = app.world().resource::<Editor>().map().unwrap();
    let object = map.find(2).unwrap().1;
    assert_eq!(
        (object.x, object.y, object.width, object.height),
        (24.0, 0.0, 16.0, 16.0)
    );

    // The wheel zooms in at the pointer, which stays over the same spot.
    let at: Vec2 = on_screen(&app, vec2(20.0, 20.0));
    let size = app.world().resource::<Window>().logical_size();
    let world = app.world_mut();
    world
        .resource_mut::<Pointers>()
        .move_to(PointerId::Mouse, at, size);
    world
        .resource_mut::<Pointers>()
        .scroll_lines(vec2(0.0, 1.0));
    frames(&mut app, 1);
    assert!((editor(&mut app).view.zoom - zoom * 1.2).abs() < 1e-4);
    assert!((on_screen(&app, vec2(20.0, 20.0)) - at).length() < 0.01);
}

#[test]
fn the_rotate_gizmo_turns_objects_about_their_centre() {
    let folder = Folder::new("rotate");
    let mut app = app(&folder, "maps/room.tmx");
    frames(&mut app, 5);
    editor(&mut app).select_object(1);
    editor(&mut app).tool = crate::Tool::Rotate;
    frames(&mut app, 1);
    // From the right of the ring to its top: a quarter turn counter-clockwise.
    let centre = on_screen(&app, vec2(12.0, 12.0));
    mouse(&mut app, centre + vec2(view::GIZMO, 0.0), true);
    mouse(
        &mut app,
        centre + vec2(view::GIZMO * 0.7, -view::GIZMO * 0.7),
        true,
    );
    mouse(&mut app, centre + vec2(1.0, -view::GIZMO), true);
    mouse(&mut app, centre + vec2(1.0, -view::GIZMO), false);
    let editor = app.world().resource::<Editor>();
    let doc = editor.map().unwrap();
    // Tiled turns clockwise: a quarter turn back is 270 degrees, snapped to 15.
    assert_eq!(doc.find(1).unwrap().1.rotation, 270.0);
    let maps = app.world().resource::<Assets<TiledMap>>();
    let object = crate::scene::engine_object(doc, maps, 1).unwrap();
    let placed = crate::doc::placed(maps.get(&doc.handle).unwrap(), &object);
    assert!(
        (placed.centre - vec2(12.0, 12.0)).length() < 1e-3,
        "{}",
        placed.centre
    );
}
