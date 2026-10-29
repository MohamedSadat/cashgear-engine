//! Log output: the terminal on desktop, the browser console on the web.

use cg_core::{App, Plugin};
use log::LevelFilter;

/// Libraries whose routine messages are hidden below a level: `wgpu`'s low-level backend
/// (harmless driver chatter) and symphonia's OGG reader (a line each time a stream opens).
const QUIET: [(&str, LevelFilter); 2] = [
    ("wgpu_hal", LevelFilter::Error),
    ("symphonia", LevelFilter::Warn),
];

/// Sends `log` records to the terminal (`env_logger`) on desktop and to the browser console
/// on the web, where it also turns panics into readable console errors.
///
/// On desktop the `RUST_LOG` environment variable overrides [`level`](Self::level), with
/// the usual `env_logger` syntax such as `RUST_LOG=debug,gilrs=warn`. Without it, `wgpu`'s
/// low-level backend logs only errors and symphonia (audio decoding) only warnings, which
/// hides harmless chatter.
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
    let mut filter = level.as_str().to_lowercase();
    for (target, level) in QUIET {
        filter.push_str(&format!(",{target}={}", level.as_str().to_lowercase()));
    }
    let env = env_logger::Env::default().default_filter_or(filter);
    // A logger may already be installed, by a test harness for example; keep it.
    let _ = env_logger::Builder::from_env(env).try_init();
}

#[cfg(target_arch = "wasm32")]
fn init(level: LevelFilter) {
    console_error_panic_hook::set_once();
    if log::set_boxed_logger(Box::new(ConsoleLogger { level })).is_ok() {
        log::set_max_level(level);
    }
}

/// `console_log`'s output with the [`QUIET`] filter, which `console_log` alone cannot do.
#[cfg(target_arch = "wasm32")]
struct ConsoleLogger {
    level: LevelFilter,
}

#[cfg(target_arch = "wasm32")]
impl log::Log for ConsoleLogger {
    fn enabled(&self, metadata: &log::Metadata) -> bool {
        let limit = QUIET
            .iter()
            .find(|(target, _)| metadata.target().starts_with(target))
            .map_or(self.level, |(_, quiet)| self.level.min(*quiet));
        metadata.level() <= limit
    }

    fn log(&self, record: &log::Record) {
        if self.enabled(record.metadata()) {
            console_log::log(record);
        }
    }

    fn flush(&self) {}
}
