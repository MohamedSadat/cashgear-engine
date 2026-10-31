# Sprites and the camera

The renderer draws sprites through `wgpu`: Vulkan, Direct3D 12 or Metal on desktop, WebGPU (or WebGL2 where a browser lacks it) on the web, Vulkan or OpenGL ES on Android. It batches sprites by layer and texture into instanced draws, so thousands of sprites cost a handful of draw calls.

## Sprites

An entity with a `Sprite` and a `Transform2D` is drawn:

```rust,ignore
commands.spawn((
    Sprite::new(server.load("images/coin.png")),
    Transform2D::from_xy(100.0, 40.0),
    Layer(5),
));
```

A sprite has a texture (a `Handle<Image>`), an optional source rectangle (part of a sheet), a size, a pivot (0 to 1 across the sprite; the centre by default), a tint `color`, `flip_x`/`flip_y` and a hit `flash` colour. `Sprite::solid(color, size)` is a coloured rectangle. `Layer` orders drawing (higher on top); within a layer, sprites are grouped by texture, so give overlapping sprites of different textures different layers when their order matters. `Visibility::Hidden` hides an entity and its children.

`AtlasSprite { atlas, region }` points a sprite at a named region of a texture atlas packed by `cg_pack` (see [Assets](assets.md)).

Sprites outside the camera's view are skipped before they reach the GPU. For drawing that is not an entity, `Sprites::draw` queues one sprite for the current frame (thousands of particles, a tile layer).

## Units and coordinates

One world unit is one pixel of your art at 1x. The y axis points up. Tiled maps put their bottom-left corner at their entity's origin.

## The camera

An entity with `Camera2D` and a `Transform2D` is the camera; the transform places it, `zoom` scales the view and `clear_color` fills it. `MainCamera` (a system parameter) converts between screen and world positions and gives the visible rectangle.

`Camera2D::scaling` decides how the world fits the window:

| Mode | Result |
| --- | --- |
| `PixelPerfect(size)` (default, 640x360) | The scene is drawn at that size and scaled by the largest whole number that fits, with bars. Pixel art stays exact. |
| `Letterbox(size)` | Scaled to fit, keeping the aspect ratio, with bars |
| `Stretch(size)` | Fills the window, stretching |
| `WindowSize` | One world unit per logical pixel; the view grows with the window |

640x360 scales by whole numbers to 720p (2x), 1080p (3x), 1440p (4x) and 4K (6x). Images use nearest-neighbour sampling unless set to `Filter::Linear`. Colours are sRGB, as your art tools show them; blending happens in linear light.

On phones, the view keeps clear of display cutouts (`Window::safe_area`).

## Debug shapes

`DebugShapes` draws lines, rectangles and circles over everything, at window resolution, for one frame: handy for showing colliders, paths or AI targets. F1 draws the physics colliders this way.

## Render stats

`RenderStats` has the frame rate, frame time, draw calls, sprites, batches and culled sprites; the F12 overlay shows them.
