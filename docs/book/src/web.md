# The web

Games run in browsers as WebAssembly, drawn with WebGPU where the browser has it and WebGL2 otherwise. [trunk](https://trunkrs.dev/) builds them; the template has the page, `Trunk.toml` and a loading screen.

```sh
trunk serve                                                     # a debug build at http://127.0.0.1:8080
trunk build --release --no-default-features --public-url ./     # the release, in dist/
```

The release uses the `web-release` profile (size-optimised, LTO) and wasm-opt, leaves out the debug overlay (`--no-default-features`), and packs `assets/` into one `assets.cgpak` that downloads alongside the code, with one progress bar for both. The sample platformer's release is about 4.6 MB to download.

Launch flags go in the page's URL: `?demo&backend=gl`.

## Hosting

`dist/` is static files with relative URLs, so it works from any folder of any host:

- **GitHub Pages:** publish `dist/` (the engine's own CI publishes the platformer this way).
- **itch.io:** zip `dist/` with `index.html` at the root, upload it as an HTML game, tick "Mobile friendly" and "Fullscreen button", and set the viewport to 1280x720.
- **Your own server:** serve `.wasm` as `application/wasm`, and gzip the code.

## In the page

- The canvas fills the page and follows its size and the device pixel ratio.
- The loading screen waits for the code, the bundle and the first frame, and shows errors (no WebGPU or WebGL2, a failed download, a panic) with a Reload button.
- Fullscreen works within a moment of a click or key press, as browsers require (`Window::set_fullscreen`).
- Sound starts after the first input. A hidden tab stops drawing and pauses sound.
- Phones get touch controls, a "turn your phone sideways" notice in portrait, and a web manifest for "Add to Home Screen".
- Saves go to `localStorage`.

## Limits

- No Lua: scripts do not run in browsers (see [Lua scripting](lua.md)).
- No threads: the engine runs on the browser's main thread.
- `AppExit` stops the game loop but leaves the page.
- trunk downloads wasm-opt on the first release build; on a slow connection that can time out. Install binaryen so `wasm-opt` is on the PATH, and trunk uses it.
