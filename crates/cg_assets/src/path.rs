//! Asset paths: `/`-separated, relative to the asset root.

/// Tidies a path: `\` becomes `/`, `.` and empty parts are dropped, `..` steps out of the
/// folder before it, and a leading `/` is removed (paths are always relative to the asset
/// root). `..` past the root is kept, and reading such a path fails.
///
/// ```
/// use cg_assets::path::normalize;
///
/// assert_eq!(normalize("maps/../tilesets/./tiles.tsx"), "tilesets/tiles.tsx");
/// assert_eq!(normalize("\\images\\hero.png"), "images/hero.png");
/// ```
pub fn normalize(path: &str) -> String {
    let mut parts: Vec<&str> = Vec::new();
    for part in path.split(['/', '\\']) {
        match part {
            "" | "." => {}
            ".." if parts.last().is_some_and(|last| *last != "..") => {
                parts.pop();
            }
            _ => parts.push(part),
        }
    }
    parts.join("/")
}

/// The folder of `file`, without a trailing `/`; empty for files at the root.
pub fn parent(file: &str) -> &str {
    file.rfind('/').map_or("", |slash| &file[..slash])
}

/// `relative` resolved against the folder `file` is in, normalized. A `relative` starting
/// with `/` is taken from the asset root instead.
///
/// ```
/// use cg_assets::path::resolve;
///
/// assert_eq!(resolve("maps/level1.tmx", "../tilesets/tiles.tsx"), "tilesets/tiles.tsx");
/// assert_eq!(resolve("maps/level1.tmx", "/images/sky.png"), "images/sky.png");
/// ```
pub fn resolve(file: &str, relative: &str) -> String {
    if relative.starts_with(['/', '\\']) {
        return normalize(relative);
    }
    normalize(&format!("{}/{relative}", parent(file)))
}

/// The extensions a file name can be matched by, longest first: `a/b.prefab.ron` gives
/// `prefab.ron`, then `ron`. Extensions are lower-cased.
pub fn extensions(path: &str) -> Vec<String> {
    let name = path.rsplit('/').next().unwrap_or(path).to_ascii_lowercase();
    let mut found = Vec::new();
    // Skip a leading dot: `.hidden` has no extension.
    let mut rest = name.trim_start_matches('.');
    while let Some(dot) = rest.find('.') {
        rest = &rest[dot + 1..];
        if !rest.is_empty() {
            found.push(rest.to_owned());
        }
    }
    found
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalize_resolves_dots_and_separators() {
        assert_eq!(normalize("a/b/../c"), "a/c");
        assert_eq!(normalize("./a//b/"), "a/b");
        assert_eq!(normalize("../outside.png"), "../outside.png");
        assert_eq!(normalize("a/../../b"), "../b");
        assert_eq!(normalize("C:\\x/y"), "C:/x/y");
        assert_eq!(normalize(""), "");
    }

    #[test]
    fn resolve_is_relative_to_the_files_folder() {
        assert_eq!(resolve("level.tmx", "tiles.tsx"), "tiles.tsx");
        assert_eq!(resolve("a/b/c.ron", "../d.png"), "a/d.png");
        assert_eq!(parent("a/b/c.ron"), "a/b");
        assert_eq!(parent("c.ron"), "");
    }

    #[test]
    fn extensions_come_longest_first() {
        assert_eq!(
            extensions("prefabs/Bunny.Prefab.RON"),
            ["prefab.ron", "ron"]
        );
        assert_eq!(extensions("maps/level.tmx"), ["tmx"]);
        assert!(extensions("README").is_empty());
        assert!(extensions("dir.d/file").is_empty());
        assert!(extensions(".hidden").is_empty());
    }
}
