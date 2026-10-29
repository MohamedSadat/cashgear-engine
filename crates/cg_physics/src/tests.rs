// The Tiled test writes a small map, tileset and image into a temporary asset folder.
#![allow(clippy::disallowed_methods)]

use crate::prelude::*;
use crate::{PhysicsWorld, WorldShape};
use cg_assets::{AssetPlugin, AssetServer};
use cg_core::glam::{IVec2, Vec2, vec2};
use cg_core::prelude::*;
use cg_render::Image;
use cg_tiled::{TiledObject, TiledPlugin, Tilemap};
use cg_world::prelude::*;
use core::time::Duration;
use std::path::PathBuf;

/// The physics without a window.
fn app() -> App {
    let mut app = App::new();
    app.add_plugins(WorldPlugin)
        .add_plugins(PhysicsPlugin::default())
        .init_resource::<Log>()
        .add_systems(FixedUpdate, record);
    app
}

/// Runs `steps` frames of one fixed step each.
fn run(app: &mut App, steps: usize) {
    let dt = app.world().resource::<Time>().fixed_delta();
    for _ in 0..steps {
        app.update(dt);
    }
}

/// The overlap messages seen by gameplay code, in order.
#[derive(Resource, Default)]
struct Log(Vec<(bool, Entity, Entity)>);

fn record(
    mut log: ResMut<Log>,
    mut started: MessageReader<CollisionStarted>,
    mut ended: MessageReader<CollisionEnded>,
) {
    log.0.extend(started.read().map(|m| (true, m.0, m.1)));
    log.0.extend(ended.read().map(|m| (false, m.0, m.1)));
}

fn floor(app: &mut App) -> Entity {
    let grid = TileGrid::parse(Vec2::splat(16.0), "##########");
    app.world_mut().spawn(TileCollider::new(grid)).id()
}

fn body(app: &mut App, at: Vec2) -> Entity {
    app.world_mut()
        .spawn((
            KinematicBody::default(),
            Collider::rect(12.0, 14.0),
            Transform2D::from_translation(at),
        ))
        .id()
}

#[test]
fn bodies_land_on_tile_colliders_and_touch_sensors() {
    let mut app = app();
    let grid = floor(&mut app);
    let player = body(&mut app, vec2(40.0, 100.0));
    let coin = app
        .world_mut()
        .spawn((
            Collider::circle(4.0),
            Sensor,
            Transform2D::from_xy(40.0, 30.0),
        ))
        .id();
    run(&mut app, 120);

    let world = app.world();
    let contacts = world.get::<KinematicContacts>(player).unwrap();
    assert!(contacts.on_ground());
    assert_eq!(contacts.ground.unwrap().entity, grid);
    // The box is 14 tall, so its centre rests 7 above the floor's top at 16.
    assert_eq!(
        world.get::<Transform2D>(player).unwrap().translation,
        vec2(40.0, 23.0)
    );
    assert!(
        world.get::<Interpolated>(player).is_some(),
        "bodies draw interpolated"
    );
    assert!(world.resource::<Overlaps>().contains(coin, player));
    let pair = if player.index_u32() < coin.index_u32() {
        (player, coin)
    } else {
        (coin, player)
    };
    assert_eq!(world.resource::<Log>().0, [(true, pair.0, pair.1)]);
    assert_eq!(
        world.resource::<PhysicsWorld>().shape(coin),
        Some(WorldShape::Circle {
            center: vec2(40.0, 30.0),
            radius: 4.0
        })
    );

    app.world_mut().despawn(coin);
    run(&mut app, 2);
    assert_eq!(
        app.world().resource::<Log>().0.last(),
        Some(&(false, pair.0, pair.1))
    );
    assert!(app.world().resource::<Overlaps>().is_empty());
}

#[test]
fn queries_see_the_latest_step() {
    let mut app = app();
    let grid = floor(&mut app);
    let crate_ = app
        .world_mut()
        .spawn((Collider::rect(16.0, 16.0), Transform2D::from_xy(88.0, 24.0)))
        .id();
    run(&mut app, 1);
    let world = app.world_mut();
    let mut state = bevy_ecs::system::SystemState::<SpatialQuery>::new(world);
    let spatial = state.get(world).unwrap();
    let hit = spatial
        .raycast(vec2(8.0, 24.0), Vec2::X, 500.0, QueryFilter::default())
        .unwrap();
    assert_eq!((hit.entity, hit.point), (crate_, vec2(80.0, 24.0)));
    let down = spatial
        .raycast(vec2(8.0, 24.0), Vec2::NEG_Y, 500.0, QueryFilter::default())
        .unwrap();
    assert_eq!(
        (down.entity, down.cell, down.distance),
        (grid, Some(IVec2::ZERO), 8.0)
    );
    assert_eq!(
        spatial.overlap_point(vec2(90.0, 20.0), QueryFilter::default()),
        [crate_]
    );
    let tile = spatial
        .tile_at(vec2(20.0, 4.0), QueryFilter::default())
        .unwrap();
    assert_eq!((tile.entity, tile.cell), (grid, IVec2::new(1, 0)));
}

#[test]
fn child_bodies_move_in_their_parents_space() {
    let mut app = app();
    floor(&mut app);
    let parent = app
        .world_mut()
        .spawn(Transform2D::from_xy(100.0, 10.0))
        .id();
    let child = body(&mut app, vec2(0.0, 50.0));
    app.world_mut().entity_mut(child).insert(ChildOf(parent));
    run(&mut app, 120);
    let world = app.world();
    assert!(world.get::<KinematicContacts>(child).unwrap().on_ground());
    // World y 23, so 13 above the parent.
    assert_eq!(
        world.get::<Transform2D>(child).unwrap().translation,
        vec2(0.0, 13.0)
    );
    assert_eq!(
        world.get::<GlobalTransform2D>(child).unwrap().translation(),
        vec2(100.0, 23.0)
    );
}

#[test]
fn gravity_and_layers_are_settings() {
    let mut app = app();
    floor(&mut app);
    app.insert_resource(Gravity(Vec2::ZERO));
    let floating = body(&mut app, vec2(40.0, 100.0));
    let ghost = body(&mut app, vec2(80.0, 100.0));
    app.world_mut()
        .entity_mut(ghost)
        .insert(CollisionLayers::new(2, 2));
    app.world_mut()
        .get_mut::<KinematicBody>(ghost)
        .unwrap()
        .velocity = vec2(0.0, -600.0);
    run(&mut app, 30);
    let world = app.world();
    assert_eq!(
        world.get::<Transform2D>(floating).unwrap().translation,
        vec2(40.0, 100.0)
    );
    assert!(
        world.get::<Transform2D>(ghost).unwrap().translation.y < 0.0,
        "fell through"
    );
}

#[test]
fn prefabs_name_the_physics_components() {
    let mut app = app();
    app.add_prefab(
        "coin",
        r#"(components: {
            "Collider": (shape: Circle(5), offset: (0, 2)),
            "Sensor": (),
            "CollisionLayers": (memberships: 4, filters: 3),
        })"#,
    )
    .add_prefab(
        "slime",
        r#"(components: {
            "Collider": (shape: Rect((16, 10))),
            "KinematicBody": (gravity_scale: 0.5, max_fall_speed: 300),
        })"#,
    );
    let world = app.world_mut();
    let coin = world.commands().spawn_prefab("coin").id();
    let slime = world.commands().spawn_prefab("slime").id();
    world.flush();
    assert_eq!(
        world.get::<Collider>(coin),
        Some(&Collider::circle(5.0).with_offset(vec2(0.0, 2.0)))
    );
    assert!(world.get::<Sensor>(coin).is_some());
    assert_eq!(
        world.get::<CollisionLayers>(coin),
        Some(&CollisionLayers::new(4, 3))
    );
    let body = world.get::<KinematicBody>(slime).unwrap();
    assert_eq!((body.gravity_scale, body.max_fall_speed), (0.5, 300.0));
    assert!(world.get::<KinematicContacts>(slime).is_some());
}

const TILESET: &str = r#"<?xml version="1.0" encoding="UTF-8"?>
<tileset version="1.10" tiledversion="1.10.2" name="tiles" tilewidth="8" tileheight="8" tilecount="4" columns="2">
 <image source="../images/tiles.png" width="16" height="16"/>
 <tile id="0"><properties><property name="solid" type="bool" value="true"/></properties></tile>
 <tile id="1"><properties><property name="one_way" type="bool" value="true"/></properties></tile>
 <tile id="2"><properties><property name="slope_left" type="int" value="0"/><property name="slope_right" type="int" value="8"/></properties></tile>
 <tile id="3"><properties><property name="hazard" type="bool" value="true"/></properties></tile>
</tileset>
"#;

/// A 4×3 map of 8-pixel tiles: one-way platforms on top, a slope going each way and a
/// hazard in the middle row, solid ground below; a second collision layer with another
/// hazard; a decoration layer that does not collide; and objects.
const MAP: &str = r#"<?xml version="1.0" encoding="UTF-8"?>
<map version="1.10" tiledversion="1.10.2" orientation="orthogonal" renderorder="right-down" width="4" height="3" tilewidth="8" tileheight="8" infinite="0" nextlayerid="5" nextobjectid="4">
 <tileset firstgid="1" source="../tilesets/tiles.tsx"/>
 <layer id="1" name="ground" width="4" height="3">
  <properties><property name="collision" type="bool" value="true"/></properties>
  <data encoding="csv">
0,2,2,0,
0,3,2147483651,4,
1,1,1,1
</data>
 </layer>
 <layer id="2" name="spikes" width="4" height="3">
  <properties><property name="collision" type="bool" value="true"/></properties>
  <data encoding="csv">
0,0,0,0,
4,0,0,0,
0,0,0,0
</data>
 </layer>
 <layer id="3" name="decor" width="4" height="3">
  <data encoding="csv">
1,1,1,1,
0,0,0,0,
0,0,0,0
</data>
 </layer>
 <objectgroup id="4" name="things">
  <object id="1" name="trigger" x="0" y="0" width="8" height="8">
   <properties><property name="sensor" type="bool" value="true"/></properties>
  </object>
  <object id="2" name="spot" x="4" y="4">
   <properties><property name="collider" type="bool" value="true"/></properties>
   <point/>
  </object>
  <object id="3" name="ball" x="16" y="0" width="8" height="8">
   <properties><property name="collider" type="bool" value="true"/></properties>
   <ellipse/>
  </object>
 </objectgroup>
</map>
"#;

struct Folder(PathBuf);

impl Folder {
    fn new(name: &str) -> Self {
        let dir = std::env::temp_dir().join(format!("cg_physics_{name}_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let folder = Self(dir);
        let image = Image::from_fn(16, 16, |_, _| [255, 255, 255, 255]);
        folder.write("images/tiles.png", &image.to_png().unwrap());
        folder.write("tilesets/tiles.tsx", TILESET.as_bytes());
        folder.write("maps/level.tmx", MAP.as_bytes());
        folder
    }

    fn write(&self, path: &str, bytes: &[u8]) {
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

#[test]
fn tiled_maps_collide_by_their_tile_properties() {
    let folder = Folder::new("tiled");
    let mut app = app();
    app.add_plugins(AssetPlugin {
        root: Some(folder.0.to_string_lossy().into_owned()),
        hot_reload: Some(false),
        ..default()
    })
    .add_plugins(TiledPlugin);
    let map = app.world().resource::<AssetServer>().load("maps/level.tmx");
    let level = app.world_mut().spawn(Tilemap::new(map)).id();
    app.update(Duration::ZERO);
    run(&mut app, 2);

    let world = app.world();
    let grid = &world
        .get::<TileCollider>(level)
        .expect("built from the map")
        .grid;
    assert_eq!(grid.size(), cg_core::glam::UVec2::new(4, 3));
    for x in 0..4 {
        assert_eq!(grid.get(IVec2::new(x, 0)), TileCell::SOLID);
    }
    assert_eq!(grid.get(IVec2::new(1, 1)), TileCell::slope(0.0, 1.0));
    assert_eq!(
        grid.get(IVec2::new(2, 1)),
        TileCell::slope(1.0, 0.0),
        "flipped"
    );
    assert_eq!(grid.get(IVec2::new(3, 1)), TileCell::EMPTY.with_hazard());
    assert_eq!(
        grid.get(IVec2::new(0, 1)),
        TileCell::EMPTY.with_hazard(),
        "from the second layer"
    );
    assert_eq!(grid.get(IVec2::new(1, 2)), TileCell::ONE_WAY);
    assert_eq!(
        grid.get(IVec2::new(0, 2)),
        TileCell::EMPTY,
        "decorations do not collide"
    );

    let world = app.world_mut();
    let mut objects = world.query::<(&TiledObject, Option<&Collider>, Has<Sensor>)>();
    let mut found: Vec<_> = objects
        .iter(world)
        .map(|(object, collider, sensor)| (object.name.clone(), collider.copied(), sensor))
        .collect();
    found.sort_by(|a, b| a.0.cmp(&b.0));
    assert_eq!(
        found,
        [
            ("ball".into(), Some(Collider::circle(4.0)), false),
            ("spot".into(), None, false),
            ("trigger".into(), Some(Collider::rect(8.0, 8.0)), true),
        ]
    );

    // A body dropped over the hazard on the right lands on the ground inside it.
    let player = app
        .world_mut()
        .spawn((
            KinematicBody::default(),
            Collider::rect(6.0, 6.0),
            Transform2D::from_xy(28.0, 40.0),
        ))
        .id();
    run(&mut app, 90);
    let contacts = app.world().get::<KinematicContacts>(player).unwrap();
    assert!(contacts.on_ground() && contacts.hazard, "{contacts:?}");
    assert_eq!(contacts.ground.unwrap().entity, level);

    // Editing the map (the hazard becomes a solid block) and reloading it rebuilds the
    // collision.
    folder.write(
        "maps/level.tmx",
        MAP.replace("0,3,2147483651,4,", "0,3,2147483651,1,")
            .as_bytes(),
    );
    app.world()
        .resource::<AssetServer>()
        .reload("maps/level.tmx");
    run(&mut app, 3);
    let grid = &app.world().get::<TileCollider>(level).unwrap().grid;
    assert_eq!(grid.get(IVec2::new(3, 1)), TileCell::SOLID);
}

/// Moves each `Path` entity to its position at the current fixed step.
#[derive(Component)]
struct Path(fn(u64) -> Vec2);

fn follow_paths(time: Res<Time>, mut paths: Query<(&Path, &mut Transform2D)>) {
    for (path, mut transform) in &mut paths {
        transform.translation = (path.0)(time.fixed_step_count());
    }
}

fn platform(app: &mut App, size: Vec2, path: fn(u64) -> Vec2) -> Entity {
    app.world_mut()
        .spawn((
            Collider::rect(size.x, size.y),
            Path(path),
            Transform2D::from_translation(path(0)),
        ))
        .id()
}

fn translation(app: &App, entity: Entity) -> Vec2 {
    app.world().get::<Transform2D>(entity).unwrap().translation
}

#[test]
fn moving_platforms_carry_their_riders() {
    let mut app = app();
    app.add_systems(FixedUpdate, follow_paths);
    // 48 wide and 8 tall, swinging 60 units either way while bobbing 20 up and down.
    let lift = platform(&mut app, vec2(48.0, 8.0), |step| {
        let t = step as f32 / 60.0;
        vec2(
            200.0 + 60.0 * (t * 1.5).sin(),
            100.0 + 20.0 * (t * 2.2).sin(),
        )
    });
    let rider = body(&mut app, vec2(200.0, 160.0));
    run(&mut app, 60);
    assert!(
        app.world()
            .get::<KinematicContacts>(rider)
            .unwrap()
            .on_ground()
    );
    let offset = translation(&app, rider) - translation(&app, lift);
    for step in 0..240 {
        run(&mut app, 1);
        let contacts = app.world().get::<KinematicContacts>(rider).unwrap();
        assert_eq!(contacts.ground.map(|g| g.entity), Some(lift), "step {step}");
        let now = translation(&app, rider) - translation(&app, lift);
        assert!((now - offset).length() < 1e-3, "step {step}: slid to {now}");
    }
    // The rider's feet stay on the top: 4 above the platform's centre, 7 below its own.
    assert!((offset.y - 11.0).abs() < 1e-3, "{offset}");
    let motion = app.world().resource::<PhysicsWorld>().motion(lift).unwrap();
    assert!(motion != Vec2::ZERO);
}

#[test]
fn rising_platforms_lift_and_walls_stop_the_ride() {
    let mut app = app();
    app.add_systems(FixedUpdate, follow_paths);
    floor(&mut app);
    // An elevator that rises 3 units a step from inside the floor to 128, and a wall at
    // x 120.
    let elevator = platform(&mut app, vec2(32.0, 16.0), |step| {
        vec2(40.0, (step as f32 * 3.0 - 6.0).min(128.0))
    });
    app.world_mut().spawn((
        Collider::rect(16.0, 200.0),
        Transform2D::from_xy(128.0, 100.0),
    ));
    let rider = body(&mut app, vec2(40.0, 23.0));
    run(&mut app, 60);
    let at = translation(&app, rider);
    assert_eq!(at, vec2(40.0, 128.0 + 8.0 + 7.0), "lifted to the top");
    let ground = app.world().get::<KinematicContacts>(rider).unwrap().ground;
    assert_eq!(ground.map(|g| g.entity), Some(elevator));
    // A platform sliding right (from step 61, 2 units a step) carries the body into the
    // wall, where it stops.
    platform(&mut app, vec2(64.0, 8.0), |step| {
        vec2(60.0 + (step as f32 - 61.0).max(0.0) * 2.0, 60.0)
    });
    let passenger = body(&mut app, vec2(60.0, 75.0));
    run(&mut app, 40);
    let at = translation(&app, passenger);
    assert_eq!(at.x, 120.0 - 6.0, "stopped at the wall's face");
}

#[test]
fn one_way_colliders_block_only_from_above() {
    let mut app = app();
    floor(&mut app);
    let plank = app
        .world_mut()
        .spawn((
            Collider::rect(48.0, 6.0),
            OneWay,
            Transform2D::from_xy(80.0, 50.0),
        ))
        .id();
    // Jumping up from below goes through the plank and lands on it.
    let jumper = body(&mut app, vec2(80.0, 23.0));
    run(&mut app, 2);
    app.world_mut()
        .get_mut::<KinematicBody>(jumper)
        .unwrap()
        .velocity
        .y = 400.0;
    run(&mut app, 90);
    let contacts = app.world().get::<KinematicContacts>(jumper).unwrap();
    assert_eq!(contacts.ground.map(|g| g.entity), Some(plank));
    assert_eq!(translation(&app, jumper).y, 53.0 + 7.0);
    // Walking sideways at its height is not blocked.
    let walker = body(&mut app, vec2(20.0, 48.0));
    app.world_mut().entity_mut(walker).insert(
        KinematicBody::default()
            .with_gravity_scale(0.0)
            .with_velocity(vec2(120.0, 0.0)),
    );
    run(&mut app, 60);
    assert!(translation(&app, walker).x > 130.0);
    // Dropping through it lands on the floor.
    app.world_mut()
        .get_mut::<KinematicBody>(jumper)
        .unwrap()
        .drop_through = true;
    run(&mut app, 60);
    assert_eq!(translation(&app, jumper).y, 23.0);
    // Rays pass it from below and hit it from above.
    let world = app.world_mut();
    let mut state = bevy_ecs::system::SystemState::<SpatialQuery>::new(world);
    let spatial = state.get(world).unwrap();
    let filter = QueryFilter::default().without_tiles();
    assert!(
        spatial
            .raycast(vec2(80.0, 40.0), Vec2::Y, 50.0, filter)
            .is_none()
    );
    let hit = spatial.raycast(vec2(80.0, 70.0), Vec2::NEG_Y, 50.0, filter);
    assert_eq!(hit.map(|h| (h.entity, h.point.y)), Some((plank, 53.0)));
}

#[test]
fn solid_colliders_push_bodies_sideways() {
    let mut app = app();
    app.add_systems(FixedUpdate, follow_paths);
    floor(&mut app);
    // A block sweeping left along the floor at 2 units a step, from x 120.
    platform(&mut app, vec2(16.0, 16.0), |step| {
        vec2(120.0 - step as f32 * 2.0, 24.0)
    });
    let pushed = body(&mut app, vec2(80.0, 23.0));
    run(&mut app, 30);
    // The block's left face is at 120 - 8 - 2 × 30 = 52; the body's right side sits on it.
    assert_eq!(translation(&app, pushed).x, 52.0 - 6.0);
}
