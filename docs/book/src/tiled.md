# Tiled maps

[Tiled](https://www.mapeditor.org/) is the engine's level editor. A map is a `.tmx` file with external `.tsx` tilesets (orthogonal maps, tilesets of one image each). Spawn one with a `Tilemap`:

```rust,ignore
commands.spawn(Tilemap::new(server.load("maps/level1.tmx")));
```

The map's bottom-left corner sits at its entity's origin, one Tiled pixel per world unit, y up. Tile layers are drawn in cached chunks culled against the view; image layers repeat across it; layer parallax works as in Tiled (the platformer's hills scroll at 0.3). Animated tiles animate. Saving the map in Tiled while the game runs rebuilds the level.

## Collision from tiles

Layers with a `collision` property set to `true` collide, by their tiles' custom properties in the tileset:

| Property | Meaning |
| --- | --- |
| `solid` | Blocks from every side |
| `one_way` | Blocks only from above; bodies jump up through it and drop down with `drop_through` |
| `slope_left`, `slope_right` | A slope, by its heights at the tile's left and right edges (pixels from the bottom) |
| `hazard` | Reported in a body's contacts (spikes, lava) |

## Objects become entities

Each object on an object layer becomes a child entity of the map with a `TiledObject` (its id, name, class, shape and custom properties), a `Transform2D` (the centre of rectangles and tile objects) and a `Name`. Tile objects get a `Sprite` of their tile, and take their tile's class and properties when they have none of their own.

Then, by class:

- If a prefab is loaded under the object's class (or its `prefab` property), the prefab's components are added: a `Coin` tile object gets the `Coin` prefab.
- Each property named after a registered component sets that component: a class-typed property gives its fields, a string is read as RON.
- `collider` or `sensor` properties give it a collider of its shape (see [Physics](physics.md)).

Games look at the rest themselves, usually in a system over `Added<TiledObject>`, as the template's `tag_objects` does. When a map is rebuilt (hot reload, the editor), its objects spawn again from scratch.

## Editing objects in the editor

The [scene editor](editor.md) edits a map's object layers in place: place, move and tune objects with an inspector for every component, then save into the `.tmx`. Tiled stays the editor for tiles.
