# Launch flags

`DefaultPlugins` reads these from the command line on desktop, from the page's query string on the web (`?frames=60&backend=gl`), and from the launch intent's `args` extra on Android.

| Flag | Does |
| --- | --- |
| `--frames N` | Run N frames, then exit |
| `--no-vsync` | Draw as fast as possible |
| `--backend <list>` | Pick the GPU API: `vulkan`, `dx12`, `metal`, `gl`, `webgpu` |
| `--hot-reload`, `--no-hot-reload` | Reload changed asset files (on by default in debug desktop builds) |
| `--asset-root <path>` | Where `assets/` is |
| `--asset-delay <seconds>` | Hold every asset load back, to see loading screens |
| `--bundle <path>` | Read assets from a `cg_pack bundle` file first |
| `--physics-debug` | Draw colliders from the start (F1 toggles) |
| `--no-audio` | No sound |
| `--touch`, `--no-touch` | Show or hide the on-screen touch controls on any device |
| `--overlay` | Open the debug overlay (F12 toggles) |
| `--profile` | Record profiling scopes and open the Profiler (`profile` feature) |
| `--no-scripts` | Do not run Lua scripts (`script` feature) |

Games read their own flags with `Args::from_env()`: `args.flag("demo")`, `args.parse::<usize>("level")`. The template's `--demo` is one.

On desktop, F11 and Alt+Enter toggle fullscreen.
