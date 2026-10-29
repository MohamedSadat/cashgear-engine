//! `cg_pack bundle`: a whole asset folder in one file, for the web.

use cashgear_engine::platform::io::Bundle;
use std::path::{Path, PathBuf};

/// Every file under `folder`, named by its `/`-separated path inside it, sorted. Hidden
/// files and folders (`.DS_Store`, `.git`) are left out.
pub fn read_folder(folder: &Path) -> Result<Vec<(String, Vec<u8>)>, String> {
    if !folder.is_dir() {
        return Err(format!("{} is not a folder", folder.display()));
    }
    let mut paths = Vec::new();
    collect(folder, folder, &mut paths)?;
    paths.sort();
    paths
        .into_iter()
        .map(|(name, path)| {
            let bytes = std::fs::read(&path).map_err(|e| format!("{}: {e}", path.display()))?;
            Ok((name, bytes))
        })
        .collect()
}

fn collect(root: &Path, dir: &Path, out: &mut Vec<(String, PathBuf)>) -> Result<(), String> {
    let entries = std::fs::read_dir(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    for entry in entries {
        let path = entry.map_err(|e| e.to_string())?.path();
        if path
            .file_name()
            .is_some_and(|name| name.to_string_lossy().starts_with('.'))
        {
            continue;
        }
        if path.is_dir() {
            collect(root, &path, out)?;
        } else {
            let relative = path.strip_prefix(root).map_err(|e| e.to_string())?;
            let name = relative
                .components()
                .map(|c| c.as_os_str().to_string_lossy().into_owned())
                .collect::<Vec<_>>()
                .join("/");
            out.push((name, path));
        }
    }
    Ok(())
}

/// Packs `folder` into the bundle `out`. Returns a summary line.
pub fn write(folder: &Path, out: &Path) -> Result<String, String> {
    let files = read_folder(folder)?;
    let count = files.len();
    let unpacked: usize = files.iter().map(|(_, bytes)| bytes.len()).sum();
    let bytes = Bundle::write(files);
    super::write(out, &bytes)?;
    Ok(format!(
        "wrote {}: {count} files, {:.2} MB packed into {:.2} MB",
        out.display(),
        unpacked as f64 / 1e6,
        bytes.len() as f64 / 1e6
    ))
}

/// The `trunk-bundle` command, run by trunk as a `post_build` hook: in release builds it
/// packs the page's `assets/` folder into `assets.cgpak` next to `index.html` and removes
/// the loose copy trunk made, so the page downloads one file. Debug builds (`trunk serve`)
/// keep the loose files.
pub fn trunk_hook() -> Result<String, String> {
    let var = |name: &str| {
        std::env::var_os(name)
            .map(PathBuf::from)
            .ok_or_else(|| format!("{name} is not set; run this as a trunk post_build hook"))
    };
    let profile = std::env::var("TRUNK_PROFILE")
        .map_err(|_| "TRUNK_PROFILE is not set; run this as a trunk post_build hook".to_owned())?;
    if profile != "release" {
        return Ok("debug build: the assets stay loose files".into());
    }
    let source = var("TRUNK_SOURCE_DIR")?.join("assets");
    let staging = var("TRUNK_STAGING_DIR")?;
    let summary = write(&source, &staging.join("assets.cgpak"))?;
    let copied = staging.join("assets");
    if copied.is_dir() {
        std::fs::remove_dir_all(&copied).map_err(|e| format!("{}: {e}", copied.display()))?;
    }
    Ok(summary)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn folders_pack_by_relative_path_without_hidden_files() {
        let dir = std::env::temp_dir().join(format!("cg_pack_bundle_{}", std::process::id()));
        let assets = dir.join("assets");
        std::fs::create_dir_all(assets.join("maps")).unwrap();
        std::fs::create_dir_all(assets.join(".git")).unwrap();
        std::fs::write(assets.join("maps").join("level.tmx"), "<map/>".repeat(100)).unwrap();
        std::fs::write(assets.join("readme.txt"), "hi").unwrap();
        std::fs::write(assets.join(".DS_Store"), "junk").unwrap();
        std::fs::write(assets.join(".git").join("HEAD"), "junk").unwrap();

        let out = dir.join("out").join("assets.cgpak");
        let summary = write(&assets, &out).unwrap();
        assert!(summary.contains("2 files"), "{summary}");
        let bundle = Bundle::parse(std::fs::read(&out).unwrap()).unwrap();
        let mut paths: Vec<_> = bundle.paths().collect();
        paths.sort();
        assert_eq!(paths, ["maps/level.tmx", "readme.txt"]);
        assert_eq!(
            bundle.read("maps/level.tmx"),
            Some(Ok("<map/>".repeat(100).into_bytes()))
        );
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
