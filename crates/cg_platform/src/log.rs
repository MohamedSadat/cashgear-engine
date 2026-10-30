//! Log output: the terminal on desktop, the browser console on the web, logcat on Android.

use cg_core::{App, Plugin};
use log::LevelFilter;

/// Libraries whose routine messages are hidden below a level: `wgpu`'s low-level backend
/// (harmless driver chatter), symphonia's OGG reader (a line each time a stream opens) and
/// egui's renderer (a warning that the engine draws into sRGB targets, which it supports).
const QUIET: [(&str, LevelFilter); 3] = [
    ("wgpu_hal", LevelFilter::Error),
    ("symphonia", LevelFilter::Warn),
    ("egui_wgpu", LevelFilter::Error),
];

/// Sends `log` records to the terminal (`env_logger`) on desktop, to the browser console on
/// the web, where it also turns panics into readable console errors and tells the page
/// (the `cashgear-error` event of [`page`](crate::page)), and to logcat on Android (tag
/// `cashgear`; `adb logcat -s cashgear` shows them), panics included with a backtrace.
///
/// On desktop the `RUST_LOG` environment variable overrides [`level`](Self::level), with
/// the usual `env_logger` syntax such as `RUST_LOG=debug,gilrs=warn`. Without it, `wgpu`'s
/// low-level backend and egui's renderer log only errors and symphonia (audio decoding)
/// only warnings, which hides harmless chatter. The web and Android apply the same filter.
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

#[cfg(not(any(target_arch = "wasm32", target_os = "android")))]
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
    static HOOK: std::sync::Once = std::sync::Once::new();
    HOOK.call_once(|| {
        std::panic::set_hook(Box::new(|info| {
            // The panic in the console with its message, then the page's error event.
            console_error_panic_hook::hook(info);
            crate::page::web::dispatch(crate::page::ERROR_EVENT, Some(&info.to_string()));
        }));
    });
    if log::set_boxed_logger(Box::new(ConsoleLogger { level })).is_ok() {
        log::set_max_level(level);
    }
}

/// Whether a record passes `level` and the [`QUIET`] filter.
#[cfg(any(target_arch = "wasm32", target_os = "android"))]
fn enabled(level: LevelFilter, metadata: &log::Metadata) -> bool {
    let limit = QUIET
        .iter()
        .find(|(target, _)| metadata.target().starts_with(target))
        .map_or(level, |(_, quiet)| level.min(*quiet));
    metadata.level() <= limit
}

/// `console_log`'s output with the [`QUIET`] filter, which `console_log` alone cannot do.
#[cfg(target_arch = "wasm32")]
struct ConsoleLogger {
    level: LevelFilter,
}

#[cfg(target_arch = "wasm32")]
impl log::Log for ConsoleLogger {
    fn enabled(&self, metadata: &log::Metadata) -> bool {
        enabled(self.level, metadata)
    }

    fn log(&self, record: &log::Record) {
        if self.enabled(record.metadata()) {
            console_log::log(record);
        }
    }

    fn flush(&self) {}
}

#[cfg(target_os = "android")]
fn init(level: LevelFilter) {
    install_panic_hook();
    if log::set_boxed_logger(Box::new(LogcatLogger { level })).is_ok() {
        log::set_max_level(level);
    }
}

/// Writes panics to logcat with a backtrace. Android drops whatever a process writes to
/// stderr, so without it a crash leaves only "Fatal signal 6 (SIGABRT)" behind.
#[cfg(target_os = "android")]
pub(crate) fn install_panic_hook() {
    static HOOK: std::sync::Once = std::sync::Once::new();
    HOOK.call_once(|| {
        let default = std::panic::take_hook();
        std::panic::set_hook(Box::new(move |info| {
            let backtrace = std::backtrace::Backtrace::force_capture();
            logcat::write(log::Level::Error, &format!("{info}\n{backtrace}"));
            default(info);
        }));
    });
}

/// Records to logcat under the `cashgear` tag, with the [`QUIET`] filter.
#[cfg(target_os = "android")]
struct LogcatLogger {
    level: LevelFilter,
}

#[cfg(target_os = "android")]
impl log::Log for LogcatLogger {
    fn enabled(&self, metadata: &log::Metadata) -> bool {
        enabled(self.level, metadata)
    }

    fn log(&self, record: &log::Record) {
        if self.enabled(record.metadata()) {
            let text = format!("{}: {}", record.target(), record.args());
            logcat::write(record.level(), &text);
        }
    }

    fn flush(&self) {}
}

#[cfg(target_os = "android")]
mod logcat {
    use std::ffi::{CString, c_char, c_int};

    /// logcat cuts entries a little over 4 KB; longer lines go out in pieces.
    const MAX_ENTRY: usize = 4000;

    #[link(name = "log")]
    unsafe extern "C" {
        fn __android_log_write(priority: c_int, tag: *const c_char, text: *const c_char) -> c_int;
    }

    /// Writes `text` to logcat, one entry per line.
    pub(super) fn write(level: log::Level, text: &str) {
        // android/log.h: ANDROID_LOG_VERBOSE is 2, up to ANDROID_LOG_ERROR at 6.
        let priority = match level {
            log::Level::Error => 6,
            log::Level::Warn => 5,
            log::Level::Info => 4,
            log::Level::Debug => 3,
            log::Level::Trace => 2,
        };
        for line in text.lines() {
            let mut rest = line;
            loop {
                let mut cut = rest.len().min(MAX_ENTRY);
                while !rest.is_char_boundary(cut) {
                    cut -= 1;
                }
                let (piece, after) = rest.split_at(cut);
                let piece = CString::new(piece.replace('\0', " ")).unwrap_or_default();
                // SAFETY: both strings are NUL-terminated and outlive the call.
                unsafe { __android_log_write(priority, c"cashgear".as_ptr(), piece.as_ptr()) };
                if after.is_empty() {
                    break;
                }
                rest = after;
            }
        }
    }
}
