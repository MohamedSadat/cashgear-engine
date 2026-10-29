//! Launch arguments that work the same on desktop and web.

use core::str::FromStr;

/// The app's launch arguments: the command line on desktop, the page's query string on the
/// web.
///
/// On the web, `?frames=120&backend=gl&no-vsync` reads as
/// `--frames=120 --backend=gl --no-vsync`, so examples take the same flags everywhere.
///
/// ```
/// use cg_platform::Args;
///
/// let args = Args::from_list(["--bunnies", "5000", "--no-vsync"]);
/// assert_eq!(args.parse::<u32>("bunnies"), Some(5000));
/// assert!(args.flag("no-vsync"));
/// assert_eq!(args.value("backend"), None);
/// ```
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Args(Vec<String>);

impl Args {
    /// Reads the arguments the app was launched with.
    pub fn from_env() -> Self {
        Self(platform_args())
    }

    /// Arguments from a list, as if given on the command line (without the program name).
    pub fn from_list<S: Into<String>>(args: impl IntoIterator<Item = S>) -> Self {
        Self(args.into_iter().map(Into::into).collect())
    }

    /// Whether `--name` is present.
    pub fn flag(&self, name: &str) -> bool {
        self.0
            .iter()
            .any(|arg| arg.strip_prefix("--") == Some(name))
    }

    /// The value of `--name value` or `--name=value`, if present.
    pub fn value(&self, name: &str) -> Option<&str> {
        let mut args = self.0.iter();
        while let Some(arg) = args.next() {
            let Some(rest) = arg.strip_prefix("--").and_then(|a| a.strip_prefix(name)) else {
                continue;
            };
            if rest.is_empty() {
                return args.next().map(String::as_str);
            }
            if let Some(value) = rest.strip_prefix('=') {
                return Some(value);
            }
        }
        None
    }

    /// The value of `--name` parsed as `T`. Logs a warning and returns `None` when it does
    /// not parse.
    pub fn parse<T: FromStr>(&self, name: &str) -> Option<T> {
        let value = self.value(name)?;
        let parsed = value.parse().ok();
        if parsed.is_none() {
            log::warn!("ignoring --{name} {value}: not a valid value");
        }
        parsed
    }

    /// All arguments in order.
    pub fn iter(&self) -> impl Iterator<Item = &str> {
        self.0.iter().map(String::as_str)
    }
}

#[cfg(not(target_arch = "wasm32"))]
fn platform_args() -> Vec<String> {
    std::env::args().skip(1).collect()
}

#[cfg(target_arch = "wasm32")]
fn platform_args() -> Vec<String> {
    let search = web_sys::window()
        .and_then(|window| window.location().search().ok())
        .unwrap_or_default();
    query_to_args(&search)
}

/// Turns `?a=1&b` into `["--a=1", "--b"]`.
#[cfg_attr(not(target_arch = "wasm32"), allow(dead_code))]
fn query_to_args(query: &str) -> Vec<String> {
    query
        .trim_start_matches('?')
        .split('&')
        .filter(|pair| !pair.is_empty())
        .map(|pair| format!("--{pair}"))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn values_in_both_forms() {
        let args = Args::from_list(["--frames", "60", "--backend=gl", "--x"]);
        assert_eq!(args.value("frames"), Some("60"));
        assert_eq!(args.value("backend"), Some("gl"));
        assert_eq!(args.value("x"), None);
        assert!(args.flag("x"));
        assert!(!args.flag("frames=60"));
        assert!(!args.flag("back"));
    }

    #[test]
    fn prefixes_do_not_match() {
        let args = Args::from_list(["--framesx", "3"]);
        assert_eq!(args.value("frames"), None);
    }

    #[test]
    fn parse_rejects_bad_values() {
        let args = Args::from_list(["--frames", "lots"]);
        assert_eq!(args.parse::<u64>("frames"), None);
    }

    #[test]
    fn query_strings_become_flags() {
        assert_eq!(
            query_to_args("?frames=120&backend=gl&no-vsync"),
            ["--frames=120", "--backend=gl", "--no-vsync"]
        );
        assert!(query_to_args("").is_empty());
        assert!(query_to_args("?").is_empty());
    }
}
