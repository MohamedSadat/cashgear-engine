# Text and UI

The game UI is entities with a `Node`, arranged by the hierarchy and laid out every frame in UI pixels: the camera's virtual pixels (640x360 by default), scaled with the game, so pixel-art panels and pixel fonts stay sharp. It is drawn over the world at the window's resolution.

```rust,ignore
let menu = commands
    .spawn((
        Node { anchor: Anchor::Center, layout: Layout::Column, gap: 6.0, padding: Edges::all(8.0), ..default() },
        BackgroundColor(Color::hex(0x1a1c2c)),
    ))
    .id();
commands.spawn((Text::new("Paused"), Node::default(), ChildOf(menu)));
commands.spawn((Button::default(), Focusable { auto_focus: true }, Text::new("Resume"), Node::default(), ChildOf(menu)));
```

## Layout

A `Node` has a width and height (`Val::Px`, `Percent`, `Auto` from its content, `Fill` to share a row's space), an anchor and offset in its parent, padding, a border, and how it lays out its children: `Free` (each at its own anchor), `Row` or `Column`, with a gap, alignment and justification. `ZIndex` puts menus over the HUD. `ComputedNode` holds the result.

## Widgets

| Component | Does |
| --- | --- |
| `Text` | Text in a font, size and colour, aligned and wrapped |
| `UiImage` | An image or part of one, optionally nine-sliced |
| `BackgroundColor`, `BorderColor` | Fills and borders |
| `Button` | Sends `Clicked` when clicked, tapped or chosen with keys or a gamepad |
| `Slider` | A value set by dragging or keys; sends `SliderChanged` |
| `ProgressBar` | A value shown by a `BarFill` child |
| `StateColors`, `StateImages` | A widget's look when hovered, focused, pressed or disabled |
| `TouchButton`, `TouchDpad` | On-screen controls for touch screens (see [Input](input.md)) |

## Focus

Arrows, WASD, the d-pad and the left stick move the focus to the nearest widget in that direction; Enter, Space and the south button click. `Focusable { auto_focus: true }` marks the widget that gets the focus when a menu opens on keys or a gamepad. Only the topmost menu takes the focus.

## Fonts

Outline fonts (TTF, OTF) are shaped by cosmic-text, so Arabic joins and mixed-direction text is ordered right; a character missing from a font is found in another loaded font. Fonts marked `pixel` (in a `.font.ron` file) are rasterized at UI pixels without smoothing, sharp at whole multiples of their size. BMFont bitmap fonts (`.fnt`) draw exactly as drawn. The built-in font is Pixelify Sans, sharp at 11 UI pixels and multiples (`DEFAULT_FONT_SIZE`).

## Languages

One `strings/<language>.strings.ron` file per language, keyed by id, with `{name}` arguments. `Localized::new("menu.play")` sets a text from the current language, and `Locale` switches it (saved through `Storage`). A right-to-left language mirrors the whole UI: anchors, rows, padding and alignment.

```ron
(name: "English", strings: { "menu.play": "Play", "hud.coins": "Coins {count}" })
```

## The debug overlay

With the `debug-overlay` feature (on by default), F12 opens an egui overlay: frame times, render stats, the entity list, a component inspector, collider and UI outline toggles, and with the `profile` feature the Profiler (see [Profiling](profiling.md)). Game code can draw its own egui windows through `EguiContext` while it is open. Release builds for the web and phones leave it out (`--no-default-features` in the template).
