# CashGear Engine — Progress

This file tracks phase status, the decisions made at the start of each phase, known issues and the backlog. The plan itself is in [PLAN.md](PLAN.md).

## Phase status

| Phase | Title | Status | Tag |
| --- | --- | --- | --- |
| 0 | Repository and workspace | Done | `phase-0` |
| 1 | Platform layer and game loop | Next | |
| 2 | Renderer core | Not started | |
| 3 | ECS and scenes | Not started | |
| 4 | Assets and tilemaps | Not started | |
| 5 | Collision and physics | Not started | |
| 6 | Animation and effects | Not started | |
| 7 | Audio | Not started | |
| 8 | Text and UI | Not started | |
| 9 | Platformer sample game | Not started | |
| 10 | Web build | Not started | |
| 11 | Android build | Not started | |
| 12 | Lua scripting | Not started | |
| 13 | Scene editor | Not started | |
| 14 | Polish, performance and release | Not started | |

## Phase 0 — Repository and workspace

### Decisions

| Question | Answer |
| --- | --- |
| License | MIT OR Apache-2.0 (dual) |
| Copyright holder | The CashGear Engine contributors |
| Repository visibility | Public: github.com/MohamedSadat/cashgear-engine |
| Toolchain / MSRV | Pinned to Rust 1.99.1 in `rust-toolchain.toml`; `rust-version = "1.99"`. Bump between phases. |

Other choices made during the phase:

- **Layout.** Engine crates live in `crates/`, the sample game in `games/` and desktop tools in `tools/`. The root `Cargo.toml` is a virtual workspace.
- **Examples.** `examples/` will hold one binary crate per example, so `trunk` can build each one for the web. `examples/*` joins the workspace members with the first example in Phase 1.
- **Facade.** `cashgear_engine` re-exports each subsystem under a short module name (`core`, `platform`, `render`, ...). `cg_script` sits behind the `script` feature because `mlua` does not build for wasm.
- **Portability guard.** The root `clippy.toml` disallows `Instant::now`, `SystemTime::now`, `thread::spawn`, `thread::sleep` and direct `std::fs` calls. `cg_platform` opts out per module with `#[allow(clippy::disallowed_methods)]`. `tools/cg_editor` and `tools/cg_pack` have their own `clippy.toml` without the guard.
- **Docs lint.** `missing_docs` is on for the whole workspace. Combined with `-D warnings`, CI enforces rustdoc on public APIs.
- **Wasm check.** `cargo check-wasm`, an alias in `.cargo/config.toml`, checks the workspace for `wasm32-unknown-unknown` without the desktop-only tools.
- **Publishing.** `publish = false` for now; Phase 14 decides about crates.io.

### Done

- Workspace with all crate skeletons, the facade and `prelude`.
- `rust-toolchain.toml`, `rustfmt.toml`, `clippy.toml`, `.gitignore`, `.gitattributes`, license files.
- GitHub Actions: fmt, clippy `-D warnings`, test and the wasm check on Windows and Ubuntu.
- `README.md`, `CHANGELOG.md`, this file; `docs/plan.md` renamed to `docs/PLAN.md`.

## Known issues

- None.

## Notes for later phases

- **Phase 1:** use `web-time` instead of `std::time`; the clippy guard enforces this.
- **Phase 1:** `gilrs` needs `libudev-dev` on Ubuntu, so add an `apt-get install` step to the CI `check` job.
- **Phase 1–2:** if anything pulls in `getrandom` 0.3, wasm needs its `wasm_js` backend: the `wasm_js` feature plus `--cfg getrandom_backend="wasm_js"` in the wasm target rustflags.
- **Phase 2:** consider `[profile.dev.package."*"] opt-level = 2` so the dev-profile bunnymark stays near 60 FPS.
- **Phase 7:** `kira` (via `cpal`) needs `libasound2-dev` on Ubuntu CI.
- **Phase 11:** add `aarch64-linux-android` to `rust-toolchain.toml` targets and a CI job.
- **Phase 12:** `cg_script` will not build for wasm; keep it out of `check-wasm` or behind the `script` feature.

## Backlog

Features that came up but don't serve the platformer or a listed example go here.

- (empty)
