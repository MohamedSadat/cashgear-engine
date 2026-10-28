//! Log output: the terminal on desktop, the browser console on the web.

use cg_core::{App, Plugin};
use log::LevelFilter;

/// Sends `log` records to the terminal (`env_logger`) on desktop and to the browser console
/// on the web, where it also turns panics into readable console errors.
///
/// On desktop the `RUST_LOG` environment variable overrides [`level`](Self::level), with
/// the usual `env_logger` syntax such as `RUST_LOG=debug,gilrs=warn`.
#[derive(Debug, Clone, Copy)]
pub struct LogPlugin {
    /// The most detailed level to show.
    pub level: LevelFilter,
}

impl Default for LogPlugin {
    fn default() -> Self {
        Self {
            level: LevelFilter::Info,
        }
    }
}

impl Plugin for LogPlugin {
    fn build(&self, _app: &mut App) {
        init(self.level);
    }
}

#[cfg(not(target_arch = "wasm32"))]
fn init(level: LevelFilter) {
    let env = env_logger::Env::default().default_filter_or(level.as_str());
    // A logger may already be installed, by a test harness for example; keep it.
    let _ = env_logger::Builder::from_env(env).try_init();
}

#[cfg(target_arch = "wasm32")]
fn init(level: LevelFilter) {
    console_error_panic_hook::set_once();
    if let Some(level) = level.to_level() {
        let _ = console_log::init_with_level(level);
    }
}
