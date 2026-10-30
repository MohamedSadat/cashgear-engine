// Tests write a small map, tileset and image into a temporary asset folder.
#![allow(clippy::disallowed_methods)]

use super::*;
use cg_assets::{AssetPlugin, AssetServer, Assets, Handle, LoadState};
use cg_core::glam::{IVec2, UVec2, uvec2, vec2};
use cg_core::prelude::*;
use cg_platform::Window;
use cg_render::{FrameView, Image, ImageLoader, Layer, RenderStats, Sprite, Sprites};
use cg_world::prelude::*;
use core::time::Duration;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

const TILESET: &str = r#"<?xml version="1.0" encoding="UTF-8"?>
<tileset version="1.10" tiledversion="1.10.2" name="tiles" tilewidth="8" tileheight="8" tilecount="4" columns="2">
 <image source="../images/tiles.png" width="16" height="16"/>
 <tile id="1" type="Ground"><properties><property name="solid" type="bool" value="true"/></properties></tile>
 <tile id="3" type="Gem"><properties><property name="value" type="int" value="5"/></properties></tile>
</tileset>
"#;

/// A 4×3 map of 8-pixel tiles: a ground layer with a flipped tile, a parallax group, a
/// repeating image layer and four objects.
fn level(ground_row: &str) -> String {
    format!(
        r#"<?xml version="1.0" encoding="UTF-8"?>
<map version="1.10" tiledversion="1.10.2" orientation="orthogonal" renderorder="right-down" width="4" height="3" tilewidth="8" tileheight="8" infinite="0" nextlayerid="6" nextobjectid="5">
 <properties><property name="music" value="calm"/></properties>
 <tileset firstgid="1" source="../tilesets/tiles.tsx"/>
 <layer id="1" name="ground" width="4" height="3">
  <data encoding="csv">
0,0,0,0,
0,1,0,0,
{ground_row}
</data>
 </layer>
 <group id="2" name="back" offsetx="4" offsety="2" parallaxx="0.5">
  <layer id="3" name="hills" width="4" height="3" opacity="0.5">
   <data encoding="csv">
3,0,0,0,
0,0,0,0,
0,0,0,0
</data>
  </layer>
 </group>
 <imagelayer id="4" name="sky" repeatx="1"><image source="../images/tiles.png" width="16" height="16"/></imagelayer>
 <objectgroup id="5" name="things">
  <object id="1" name="coin1" type="Coin" x="8" y="0" width="8" height="8"/>
  <object id="2" gid="4" x="16" y="24" width="8" height="8"/>
  <object id="3" name="start" type="PlayerStart" x="4" y="12"><point/></object>
  <object id="4" name="guard" x="0" y="0" width="4" height="2">
   <properties>
    <property name="Layer" type="int" value="7"/>
    <property name="Patrol" type="class" propertytype="Patrol">
     <properties><property name="speed" type="float" value="40"/></properties>
    </property>
   </properties>
  </object>
 </objectgroup>
</map>
"#
    )
}

/// A component games might set from Tiled.
#[derive(Component, Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
struct Patrol {
    speed: f32,
    distance: f32,
}

impl Default for Patrol {
    fn default() -> Self {
        Self {
            speed: 10.0,
            distance: 64.0,
        }
    }
}

/// What the `Coin` prefab adds.
#[derive(Component, Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
struct Coin;

struct Folder(PathBuf);

impl Folder {
    fn new(name: &str) -> Self {
        let dir = std::env::temp_dir().join(format!("cg_tiled_{name}_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let folder = Self(dir);
        let image = Image::from_fn(16, 16, |x, y| [(x * 16) as u8, (y * 16) as u8, 0, 255]);
        folder.write_bytes("images/tiles.png", &image.to_png().unwrap());
        folder.write("tilesets/tiles.tsx", TILESET);
        folder.write("maps/level.tmx", &level("2,2,2147483650,2"));
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
}

impl Drop for Folder {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

/// The tilemap systems without a GPU, drawing into a 640×360 view.
fn app(folder: &Folder) -> App {
    let mut app = App::new();
    app.add_plugins(AssetPlugin {
        root: Some(folder.0.to_string_lossy().into_owned()),
        hot_reload: Some(false),
        ..default()
    })
    .init_asset::<Image>()
    .register_loader(ImageLoader)
    .init_asset::<TiledMap>()
    .register_loader(TmxLoader)
    .register_prefab_component::<Sprite>("Sprite")
    .register_component::<Layer>("Layer")
    .register_component::<Patrol>("Patrol")
    .register_component::<Coin>("Coin")
    .add_prefab(
        "Coin",
        r#"(components: { "Coin": (), "Transform2D": (scale: (2, 2)) })"#,
    )
    .init_resource::<Sprites>()
    .init_resource::<FrameView>()
    .init_resource::<RenderStats>()
    .insert_resource(Window::with_size(uvec2(640, 360), 1.0))
    .add_systems(PreUpdate, spawn::build_tilemaps)
    .add_systems(Render, draw::queue_tilemaps);
    app
}

fn load(app: &mut App) -> Handle<TiledMap> {
    let handle = app.world().resource::<AssetServer>().load("maps/level.tmx");
    app.update(Duration::ZERO);
    handle
}

#[test]
fn maps_load_with_tilesets_layers_and_objects() {
    let folder = Folder::new("load");
    let mut app = app(&folder);
    let handle = load(&mut app);
    let server = app.world().resource::<AssetServer>();
    assert_eq!(
        server.load_state(&handle),
        LoadState::Loaded,
        "{:?}",
        server.load_error(&handle)
    );
    assert!(
        server.is_loaded(&handle),
        "the tileset image is a loaded dependency"
    );

    let maps = app.world().resource::<Assets<TiledMap>>();
    let map = maps.get(&handle).unwrap();
    assert_eq!(
        (map.width, map.height, map.tile_size),
        (4, 3, UVec2::splat(8))
    );
    assert_eq!(map.properties.str("music"), Some("calm"));

    let tileset = &map.tilesets[0];
    assert_eq!((tileset.columns, tileset.tile_count), (2, 4));
    assert_eq!(
        server.path(tileset.image.as_ref().unwrap()).as_deref(),
        Some("images/tiles.png")
    );
    assert_eq!(tileset.tiles[&1].class, "Ground");
    assert_eq!(tileset.tiles[&1].properties.bool("solid"), Some(true));

    let names: Vec<_> = map.layers.iter().map(|l| l.name.as_str()).collect();
    assert_eq!(names, ["ground", "hills", "sky", "things"]);
    let ground = map.tile("ground", IVec2::new(1, 1)).unwrap();
    assert_eq!((ground.id, ground.flip_x), (0, false));
    let flipped = map.tile("ground", IVec2::new(2, 2)).unwrap();
    assert_eq!(
        (flipped.id, flipped.flip_x, flipped.flip_y),
        (1, true, false)
    );
    assert_eq!(map.tile_info(flipped).unwrap().class, "Ground");
    let hills = map.layer("hills").unwrap();
    assert_eq!(
        (hills.offset, hills.parallax, hills.opacity),
        (vec2(4.0, 2.0), vec2(0.5, 1.0), 0.5)
    );
    let LayerKind::Image(sky) = &map.layer("sky").unwrap().kind else {
        panic!("sky is an image layer");
    };
    assert!(sky.repeat_x && !sky.repeat_y);

    let objects: Vec<_> = map.objects().map(|(_, o)| o).collect();
    assert_eq!(objects.len(), 4);
    assert_eq!(objects[0].class, "Coin");
    // The tile object takes its tile's class and properties.
    assert_eq!(objects[1].class, "Gem");
    assert_eq!(objects[1].properties.int("value"), Some(5));
    assert_eq!(objects[1].tile.map(|t| t.id), Some(3));
    assert_eq!(objects[2].shape, ObjectShape::Point);

    // Global tile ids, as the file stores them, both ways.
    assert_eq!(map.tile_for_gid(4), objects[1].tile);
    assert_eq!(map.gid(flipped), 2_147_483_650);
    assert_eq!(map.tile_for_gid(2_147_483_650), Some(flipped));
    assert_eq!(map.tile_for_gid(0), None);
    assert_eq!(map.tile_for_gid(5), None, "the tileset has four tiles");
}

#[test]
fn tilemaps_build_chunks_and_spawn_objects() {
    let folder = Folder::new("spawn");
    let mut app = app(&folder);
    let handle = load(&mut app);
    let map_entity = app
        .world_mut()
        .spawn((
            Tilemap::new(handle).with_layer(10),
            Transform2D::from_xy(100.0, 0.0),
        ))
        .id();
    app.update(Duration::ZERO);

    let state = app.world().get::<TilemapState>(map_entity).unwrap();
    assert!(state.is_built());
    assert_eq!(state.tile_count(), 6, "5 ground tiles and 1 hill");
    assert_eq!(state.objects().len(), 4);

    let mut query = app
        .world_mut()
        .query::<(Entity, &TiledObject, &Transform2D, &Layer, Option<&Sprite>)>();
    let world = app.world();
    let objects: Vec<_> = query
        .iter(world)
        .map(|(e, o, t, l, s)| (e, o.clone(), *t, *l, s.cloned()))
        .collect();
    let find = |name: &str| {
        objects
            .iter()
            .find(|(_, o, ..)| o.name == name || o.class == name)
            .cloned()
            .unwrap()
    };
    // Rectangles are centred: (8, 0)-(16, 8) in Tiled is centred at (12, 24 - 4).
    let (coin, _, transform, layer, _) = find("coin1");
    assert_eq!(transform.translation, vec2(12.0, 20.0));
    assert_eq!(transform.scale, vec2(2.0, 2.0), "the prefab's scale stays");
    assert_eq!(layer.0, 13, "map layer 3 on top of 10");
    assert!(
        world.get::<Coin>(coin).is_some(),
        "spawned as the Coin prefab"
    );
    assert_eq!(world.get::<ChildOf>(coin).unwrap().parent(), map_entity);

    // Tile objects hang from their bottom-left corner and draw their tile.
    let (_, gem, transform, _, sprite) = find("Gem");
    assert_eq!(transform.translation, vec2(20.0, 4.0));
    let sprite = sprite.expect("tile objects get a sprite");
    assert_eq!(
        sprite.source,
        Some(cg_core::Rect::new(8.0, 8.0, 16.0, 16.0))
    );
    assert_eq!(gem.properties.int("value"), Some(5));

    let (_, _, transform, _, _) = find("start");
    assert_eq!(transform.translation, vec2(4.0, 12.0));

    // Properties named after components set them; class members fill fields.
    let (guard, _, _, layer, _) = find("guard");
    assert_eq!(layer.0, 7);
    assert_eq!(
        world.get::<Patrol>(guard),
        Some(&Patrol {
            speed: 40.0,
            distance: 64.0
        })
    );
}

#[test]
fn chunks_are_queued_with_parallax_and_culled() {
    let folder = Folder::new("queue");
    let mut app = app(&folder);
    let handle = load(&mut app);
    app.world_mut().spawn(Tilemap::new(handle));
    app.update(Duration::ZERO);
    let sprites = app.world().resource::<Sprites>().queued().to_vec();
    // 6 tiles, plus the sky repeated across the 640-unit-wide view in 16-unit steps.
    let tiles: Vec<_> = sprites
        .iter()
        .filter(|s| s.size == Some(vec2(1.0, 1.0)))
        .collect();
    assert_eq!(tiles.len(), 6);
    let sky = sprites.len() - tiles.len();
    assert!(
        (40..=41).contains(&sky),
        "copies from x = -320 to 320, not {sky}"
    );
    // The ground tile at cell (1, 1) has its bottom-left corner at (8, 8).
    assert!(
        tiles
            .iter()
            .any(|s| s.transform.translation == vec2(8.0, 8.0))
    );
    // The flipped tile at (2, 2) is mirrored: its x axis points left from x = 24.
    assert!(
        tiles
            .iter()
            .any(|s| s.transform.translation == vec2(24.0, 0.0)
                && s.transform.matrix2.x_axis.x < 0.0)
    );
    let hill = tiles.iter().find(|s| s.color.a == 0.5).unwrap();
    // Offset (4, 2) in Tiled: x + 4, y - 2. The camera is at the origin, as is the
    // parallax origin, so parallax adds nothing yet.
    assert_eq!(hill.transform.translation, vec2(4.0, 14.0));

    // Move the camera: the half-speed layer follows it by half the distance.
    app.world_mut().resource_mut::<Sprites>().clear();
    app.world_mut().resource_mut::<FrameView>().0.position = vec2(100.0, 0.0);
    app.update(Duration::ZERO);
    let sprites = app.world().resource::<Sprites>().queued().to_vec();
    let hill = sprites.iter().find(|s| s.color.a == 0.5).unwrap();
    assert_eq!(hill.transform.translation, vec2(54.0, 14.0));

    // Far away, every chunk is culled.
    app.world_mut().resource_mut::<Sprites>().clear();
    app.world_mut().resource_mut::<FrameView>().0.position = vec2(5000.0, 5000.0);
    app.update(Duration::ZERO);
    let sprites = app.world().resource::<Sprites>().queued();
    assert!(sprites.iter().all(|s| s.size != Some(vec2(1.0, 1.0))));
    assert_eq!(app.world().resource::<RenderStats>().culled(), 6);
}

#[test]
fn changed_maps_and_tilesets_rebuild_the_tilemap() {
    let folder = Folder::new("reload");
    let mut app = app(&folder);
    let handle = load(&mut app);
    let map_entity = app.world_mut().spawn(Tilemap::new(handle)).id();
    app.update(Duration::ZERO);
    let before: Vec<Entity> = app
        .world()
        .get::<TilemapState>(map_entity)
        .unwrap()
        .objects()
        .to_vec();

    folder.write("maps/level.tmx", &level("2,2,0,0"));
    let server = app.world().resource::<AssetServer>().clone();
    assert_eq!(server.reload("maps/level.tmx"), 1);
    app.update(Duration::ZERO);
    app.update(Duration::ZERO);
    let state = app.world().get::<TilemapState>(map_entity).unwrap();
    assert_eq!(state.tile_count(), 4, "two ground tiles fewer");
    assert!(
        before.iter().all(|e| app.world().get_entity(*e).is_err()),
        "old objects are gone"
    );
    assert_eq!(state.objects().len(), 4);

    // The tileset was read as part of the map, so changing it reloads the map too.
    assert_eq!(server.reload("tilesets/tiles.tsx"), 1);
}

#[test]
fn a_missing_tileset_fails_the_map_with_its_path() {
    let folder = Folder::new("missing");
    std::fs::remove_file(folder.0.join("tilesets/tiles.tsx")).unwrap();
    let mut app = app(&folder);
    let handle = load(&mut app);
    let server = app.world().resource::<AssetServer>();
    assert_eq!(server.load_state(&handle), LoadState::Failed);
    assert_eq!(
        server.load_error(&handle).unwrap().to_string(),
        "could not load `maps/level.tmx`: `tilesets/tiles.tsx` not found"
    );
}

/// Two animated tiles: tile 2 shows itself for 0.1 s, then tile 3 for 0.3 s; tile 3
/// shows itself for 0.2 s, then tile 2 for 0.2 s.
const ANIMATED_TILESET: &str = r#"<?xml version="1.0" encoding="UTF-8"?>
<tileset version="1.10" tiledversion="1.10.2" name="tiles" tilewidth="8" tileheight="8" tilecount="4" columns="2">
 <image source="../images/tiles.png" width="16" height="16"/>
 <tile id="2">
  <animation>
   <frame tileid="2" duration="100"/>
   <frame tileid="3" duration="300"/>
  </animation>
 </tile>
 <tile id="3">
  <animation>
   <frame tileid="3" duration="200"/>
   <frame tileid="2" duration="200"/>
  </animation>
 </tile>
</tileset>
"#;

#[test]
fn animated_tiles_play_in_layers_and_on_tile_objects() {
    let folder = Folder::new("animated");
    folder.write("tilesets/tiles.tsx", ANIMATED_TILESET);
    let mut app = app(&folder);
    app.add_systems(PostUpdate, animation::animate_tile_objects);
    let handle = load(&mut app);
    let maps = app.world().resource::<Assets<TiledMap>>();
    let tileset = &maps.get(&handle).unwrap().tilesets[0];
    assert_eq!(
        tileset.tiles[&2].animation,
        [
            TileFrame {
                tile: 2,
                duration: 0.1
            },
            TileFrame {
                tile: 3,
                duration: 0.3
            }
        ]
    );
    let (two, three) = (tileset.tile_rect(2), tileset.tile_rect(3));
    app.world_mut().spawn(Tilemap::new(handle));
    app.update(Duration::ZERO);

    // The hills layer holds one tile 2; the tile object (gid 4) is tile 3.
    let step = |app: &mut App, millis: u64| -> (Vec<Rect>, Option<Rect>) {
        app.world_mut().resource_mut::<Sprites>().clear();
        app.update(Duration::from_millis(millis));
        let layer = app
            .world()
            .resource::<Sprites>()
            .queued()
            .iter()
            .filter_map(|s| s.source)
            .filter(|r| *r == two || *r == three)
            .collect();
        let world = app.world_mut();
        let mut objects = world.query_filtered::<&Sprite, With<TileAnimation>>();
        let object = objects.single(world).ok().and_then(|s| s.source);
        (layer, object)
    };
    assert_eq!(step(&mut app, 16), (vec![two], Some(three)), "first frames");
    assert_eq!(
        step(&mut app, 100),
        (vec![three], Some(three)),
        "the layer tile moves on"
    );
    assert_eq!(
        step(&mut app, 100),
        (vec![three], Some(two)),
        "then the object"
    );
    assert_eq!(step(&mut app, 200), (vec![two], Some(three)), "both loop");
}

#[test]
fn tile_objects_without_an_image_still_spawn() {
    // A map whose only tileset is an image collection: tiles draw nothing, objects spawn.
    let folder = Folder::new("collection");
    folder.write(
        "tilesets/tiles.tsx",
        r#"<?xml version="1.0" encoding="UTF-8"?>
<tileset version="1.10" name="tiles" tilewidth="8" tileheight="8" tilecount="4" columns="0">
 <tile id="0"><image source="../images/tiles.png" width="16" height="16"/></tile>
</tileset>
"#,
    );
    let mut app = app(&folder);
    let handle = load(&mut app);
    let map_entity = app.world_mut().spawn(Tilemap::new(handle)).id();
    app.update(Duration::ZERO);
    let state = app.world().get::<TilemapState>(map_entity).unwrap();
    assert_eq!(state.tile_count(), 0);
    assert_eq!(state.objects().len(), 4);
}
