# Physics

The physics is made for platformers: kinematic bodies that move exactly as told and stop at walls, on tiles and colliders, with slopes, one-way and moving platforms, sensors and queries. It runs in fixed steps, in `FixedPostUpdate`, after your gameplay in `FixedUpdate`.

## Bodies

```rust,ignore
commands.spawn((
    KinematicBody { max_fall_speed: 480.0, ..default() },
    Collider::rect(12.0, 18.0),
    Transform2D::from_xy(64.0, 120.0),
));
```

Set `KinematicBody::velocity` in `FixedUpdate`; gravity (the `Gravity` resource, 980 units per second squared down by default) is added for you. The body moves along x, then y, swept so it never passes through anything at any speed. `KinematicContacts` reports the result: `on_ground()`, `on_ceiling()`, `wall_left`/`wall_right`, `on_slope()`, `just_landed()`, `just_left_ground()` (for coyote time) and `hazard`. A body stands on a slope by the middle of its feet, and walks up and down it without bumps. `drop_through` drops through one-way platforms.

Bodies are boxes (the bounding box of their collider) and do not push each other.

## Colliders

`Collider::rect(w, h)` and `Collider::circle(r)`, with an offset. An entity with a collider and no body is static: bodies stop at it. Static colliders that game code moves in `FixedUpdate` are moving platforms: they carry the bodies standing on them, lift those under them and push those beside them. `OneWay` makes a collider block only from above.

A `Sensor` collider blocks nothing; overlaps with it are reported. `CollisionLayers::new(memberships, filters)` chooses which colliders meet.

## Overlaps

`CollisionStarted(a, b)` and `CollisionEnded(a, b)` messages report pairs that began or stopped overlapping, ready in the next `FixedUpdate`; `other(entity)` gives the other side. The `Overlaps` resource lists every pair overlapping now. A fast body is tested along its whole move, so it never skips a coin.

## Queries

`SpatialQuery` (a system parameter) looks at the latest step: `raycast(origin, direction, max, filter)`, `tile_at(point, filter)`, `overlap_shape` and `overlap_point`. A `QueryFilter` narrows them: `without_sensors()`, `without_tiles()`, `with_mask(layers)`, `exclude(entity)`. The template's bot looks for walls, gaps and spikes this way.

## Seeing it

F1 (or `--physics-debug`) draws every collider, tile edge, slope and contact.
