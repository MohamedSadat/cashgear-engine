# Examples

The engine's repository has one example per feature under `examples/`, and the sample game under `games/platformer`. Run any with `cargo run -p <name>`; every example takes `--frames N` (run N frames, then exit), and most have a `--demo` bot. Those with a web page run in a browser with `trunk serve` from their folder.

| Example | Shows | Web |
| --- | --- | --- |
| `hello_window` | The window, the game loop, input logging, an interpolated box | port 8080 |
| `bunnymark` | Thousands of bouncing sprites, scaling modes, camera controls, debug shapes; `--ecs` makes each bunny an entity | 8081 |
| `ecs_sprites` | Prefabs, images loaded as assets, a hierarchy, interpolation, culling, a state stack | 8082 |
| `tiled_map` | A Tiled level: loading bar, parallax, chunked tiles, object prefabs, an atlas, live editing | 8083 |
| `collision` | Slopes, one-way platforms, spikes, crates, coin sensors, a dash and a slam; F1 shows colliders | 8084 |
| `animation` | An Aseprite character with a state machine, dust, squash and stretch, hit flashes, camera follow, shake and zoom, a particle gallery | 8085 |
| `audio` | Footsteps, positional sounds, crossfading music by area, a saved volume panel | 8086 |
| `ui` | Menus by keyboard, gamepad, mouse and touch, English and Arabic, touch controls, the debug overlay | 8087 |
| `scripting` | A level whose gameplay is all Lua, reloaded when saved, with a Lua bot | desktop only |
| `games/platformer` | The sample game: three levels, enemies, moving platforms, checkpoints, settings, saves, two languages | 8088 |
| `games/platformer/editor` | The platformer with the scene editor built in | desktop only |

```sh
cargo run -p bunnymark --release -- --bunnies 50000 --no-vsync
cargo run -p platformer -- --demo
cd examples/animation && trunk serve --port 8085
```
