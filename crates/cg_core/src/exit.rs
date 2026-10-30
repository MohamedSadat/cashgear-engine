//! Leaving the app: the [`AppExit`] message and the `--frames N` frame limit.

use crate::{App, Last, Plugin, Time};
use bevy_ecs::prelude::*;
use core::num::NonZeroU8;

/// Write this message to stop the app after the current frame.
///
/// ```
/// use cg_core::prelude::*;
///
/// fn quit(mut exit: MessageWriter<AppExit>) {
///     exit.write(AppExit::Success);
/// }
/// ```
///
/// On the web, exiting stops the game loop but leaves the page and canvas in place.
#[derive(Message, Debug, Default, Clone, Copy, PartialEq, Eq, Hash)]
pub enum AppExit {
    /// The app finished normally.
    #[default]
    Success,
    /// The app failed, with a process exit code.
    Error(NonZeroU8),
}

impl AppExit {
    /// An error exit with code 1.
    pub const fn error() -> Self {
        Self::Error(NonZeroU8::MIN)
    }

    /// Whether this is [`AppExit::Success`].
    pub const fn is_success(&self) -> bool {
        matches!(self, Self::Success)
    }
}

/// Lets `fn main() -> AppExit { app.run() }` end the process with the exit's code (0 for
/// [`AppExit::Success`]), so a script or CI sees a game that failed.
impl std::process::Termination for AppExit {
    fn report(self) -> std::process::ExitCode {
        match self {
            Self::Success => std::process::ExitCode::SUCCESS,
            Self::Error(code) => std::process::ExitCode::from(code.get()),
        }
    }
}

/// Exits the app with [`AppExit::Success`] once this many frames have run.
#[derive(Resource, Debug, Clone, Copy, PartialEq, Eq)]
pub struct FrameLimit(pub u64);

/// Supports the `--frames N` flag that every example accepts: run N frames, then exit.
///
/// CI uses it to smoke-test examples. Without the flag, the plugin does nothing.
#[derive(Debug, Default, Clone, Copy)]
pub struct FrameLimitPlugin {
    /// Frames to run before exiting; `None` runs until something else exits the app.
    pub frames: Option<u64>,
}

impl FrameLimitPlugin {
    /// Reads `--frames N` (or `--frames=N`) from the process arguments.
    ///
    /// On the web there are no arguments, so this returns a plugin that does nothing.
    pub fn from_args() -> Self {
        Self {
            frames: frames_arg(std::env::args().skip(1)),
        }
    }
}

impl Plugin for FrameLimitPlugin {
    fn build(&self, app: &mut App) {
        if let Some(frames) = self.frames {
            log::info!("--frames {frames}: exiting after {frames} frames");
            app.insert_resource(FrameLimit(frames));
        }
        app.add_systems(Last, exit_after_frame_limit);
    }
}

fn exit_after_frame_limit(
    limit: Option<Res<FrameLimit>>,
    time: Res<Time>,
    mut exit: MessageWriter<AppExit>,
) {
    if limit.is_some_and(|limit| time.frame_count() >= limit.0) {
        exit.write(AppExit::Success);
    }
}

/// Parses `--frames N` or `--frames=N` from command-line arguments.
fn frames_arg(mut args: impl Iterator<Item = String>) -> Option<u64> {
    while let Some(arg) = args.next() {
        let value = match arg.strip_prefix("--frames") {
            Some("") => args.next(),
            Some(rest) => rest.strip_prefix('=').map(str::to_owned),
            None => continue,
        };
        match value.as_deref().map(str::parse) {
            Some(Ok(frames)) => return Some(frames),
            _ => log::warn!("ignoring --frames: expected a frame count"),
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use core::time::Duration;

    fn args(list: &[&str]) -> impl Iterator<Item = String> {
        list.iter().map(|s| s.to_string())
    }

    #[test]
    fn parses_frames_flag() {
        assert_eq!(frames_arg(args(&["--frames", "120"])), Some(120));
        assert_eq!(frames_arg(args(&["-v", "--frames=30"])), Some(30));
        assert_eq!(frames_arg(args(&["--frames"])), None);
        assert_eq!(frames_arg(args(&["--frames", "lots"])), None);
        assert_eq!(frames_arg(args(&["--framesx"])), None);
        assert_eq!(frames_arg(args(&[])), None);
    }

    #[test]
    fn frame_limit_exits_on_the_last_frame() {
        let mut app = App::new();
        app.add_plugins(FrameLimitPlugin { frames: Some(3) });
        for _ in 0..2 {
            app.update(Duration::from_millis(16));
            assert_eq!(app.should_exit(), None);
        }
        app.update(Duration::from_millis(16));
        assert_eq!(app.should_exit(), Some(AppExit::Success));
    }
}
