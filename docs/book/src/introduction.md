# CashGear Engine

CashGear Engine is a 2D game engine written in Rust. One game's code runs on Windows, Linux and macOS, in web browsers (WebAssembly, with WebGPU or WebGL2), and on Android phones. It is made for pixel-art games first, and a short platformer, built with it from start to finish, proves it: [play it in your browser](https://mohamedsadat.github.io/cashgear-engine/).

This guide takes you from an empty folder to a game on all three kinds of platform, then explains each part of the engine. The API reference, generated from the code, is at [/api](https://mohamedsadat.github.io/cashgear-engine/api/cashgear_engine/).

## What is in the engine

| Part | Crate | What it does |
| --- | --- | --- |
| The app | `cg_core` | Plugins, schedules, a fixed-step game loop with interpolation, `Time`, profiling |
| The platform | `cg_platform` | Windows and canvases, input and actions, files on every target, save storage, logging |
| Drawing | `cg_render` | `wgpu` sprite batching, a 2D camera, pixel-perfect scaling, debug shapes, atlases |
| The world | `cg_world` | Entities and components (`bevy_ecs`), transforms and hierarchy, a state stack, prefabs |
| Assets | `cg_assets` | Handles, background loading, loading screens, hot reload, bundles |
| Tiled maps | `cg_tiled` | TMX maps drawn in chunks, parallax, tile animations, objects as entities |
| Physics | `cg_physics` | Kinematic bodies on tiles, slopes, one-way and moving platforms, sensors, raycasts |
| Animation | `cg_anim` | Aseprite sheets, animation state machines, tweens, particles, camera follow and shake |
| Audio | `cg_audio` | OGG and WAV, streamed music with crossfades, buses, positional sound |
| UI | `cg_ui` | Text (with Arabic and right-to-left), widgets, focus, touch controls, an egui debug overlay |
| Scripts | `cg_script` | Lua 5.4 gameplay scripts, reloaded while the game runs |
| The editor | `cg_editor` | A scene editor for Tiled object layers and prefabs, with play-in-editor |
| Tools | `cg_pack` | Packs texture atlases and asset bundles |

Games depend on one crate, `cashgear_engine`, which re-exports them all and has a `prelude`.

## How a game is built

A game is a library with a plugin that adds its systems, and small entry points around it: a `main` for desktop and the web, a `cdylib` for Android, and optionally a build with the scene editor. The [template](template.md) sets all of that up.

Gameplay runs in fixed steps of 1/60 s, so a game plays the same at any frame rate; drawing blends between steps. Levels come from [Tiled](https://www.mapeditor.org/), sprite animations from [Aseprite](https://www.aseprite.org/) exports, and game objects are entities with components, which prefab files can describe.

## Versions

This guide is for CashGear Engine 0.1. The engine follows semantic versioning with 0.x numbers: a new minor version (0.2) may break games, and its changelog says how to update. Games pin the engine's version in their `Cargo.toml`.
