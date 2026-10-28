# Changelog

All notable changes to CashGear Engine are recorded here, one section per phase.
The format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/).

## Phase 0 — Repository and workspace

### Added

- A Cargo workspace (edition 2024, resolver 3) with skeleton crates: the `cashgear_engine` facade with a `prelude`, eleven `cg_*` engine crates, the `games/platformer` stub, and the `tools/cg_editor` and `tools/cg_pack` stubs.
- Lua scripting (`cg_script`) behind the facade's `script` feature, off by default.
- `rust-toolchain.toml` pinned to Rust 1.99.1 with `rustfmt`, `clippy` and the `wasm32-unknown-unknown` target.
- `rustfmt.toml`, `.gitattributes` (LF line endings) and `.gitignore`.
- A `clippy.toml` portability guard that rejects `Instant::now`, threads and direct `std::fs` calls in engine and game crates.
- A `cargo check-wasm` alias for the wasm check.
- A workspace-wide `missing_docs` lint.
- GitHub Actions CI: fmt, then clippy `-D warnings`, tests and the wasm check on Windows and Ubuntu.
- A dual MIT/Apache-2.0 license, `README.md`, `docs/PROGRESS.md` and this changelog.

### Changed

- Renamed `docs/plan.md` to `docs/PLAN.md` to match the workflow references.
