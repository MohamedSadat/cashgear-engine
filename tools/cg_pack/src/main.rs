//! Asset command-line tool for CashGear Engine.
//!
//! ```text
//! cg_pack atlas <folder> <out.atlas.ron> [--padding N] [--extrude N] [--max-size N] [--check]
//! ```
//!
//! `atlas` packs every PNG under `<folder>` into one image, written next to the atlas file
//! with the same stem (`items.atlas.ron` gets `items.png`). Regions are named by their
//! path inside the folder without `.png`: `enemies/bat.png` becomes `enemies/bat`. The
//! same folder always packs the same way, so the outputs can be committed; `--check`
//! fails if they are out of date instead of writing them.
//!
//! Load the result with `server.load::<TextureAtlas>("atlases/items.atlas.ron")` and draw
//! regions with `AtlasSprite`.

mod atlas;

use std::path::{Path, PathBuf};
use std::process::ExitCode;

const USAGE: &str = "\
usage: cg_pack atlas <folder> <out.atlas.ron> [options]

Packs every PNG under <folder> into <out>.png and <out>.atlas.ron.

options:
  --padding N    transparent pixels between regions (default 1)
  --extrude N    repeat each region's edge N pixels outwards (default 0)
  --max-size N   largest atlas width or height (default 4096)
  --check        fail if the outputs are missing or out of date; write nothing";

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match run(&args) {
        Ok(message) => {
            println!("{message}");
            ExitCode::SUCCESS
        }
        Err(message) => {
            eprintln!("cg_pack: {message}");
            ExitCode::FAILURE
        }
    }
}

fn run(args: &[String]) -> Result<String, String> {
    match args.first().map(String::as_str) {
        Some("atlas") => atlas_command(&args[1..]),
        Some("--version") => Ok(format!("cg_pack {}", cashgear_engine::VERSION)),
        _ => Err(USAGE.into()),
    }
}

fn atlas_command(args: &[String]) -> Result<String, String> {
    let mut settings = atlas::Settings::default();
    let mut check = false;
    let mut paths = Vec::new();
    let mut rest = args.iter();
    while let Some(arg) = rest.next() {
        let mut number = |name: &str| -> Result<u32, String> {
            rest.next()
                .and_then(|v| v.parse().ok())
                .ok_or_else(|| format!("{name} needs a number"))
        };
        match arg.as_str() {
            "--padding" => settings.padding = number("--padding")?,
            "--extrude" => settings.extrude = number("--extrude")?,
            "--max-size" => settings.max_size = number("--max-size")?,
            "--check" => check = true,
            flag if flag.starts_with("--") => {
                return Err(format!("unknown option {flag}\n\n{USAGE}"));
            }
            path => paths.push(PathBuf::from(path)),
        }
    }
    let [folder, out] = paths.as_slice() else {
        return Err(USAGE.into());
    };
    let out_name = out
        .file_name()
        .and_then(|n| n.to_str())
        .and_then(|n| n.strip_suffix(".atlas.ron"))
        .ok_or("the output must be named like `items.atlas.ron`")?;
    let image_name = format!("{out_name}.png");
    let image_path = out.with_file_name(&image_name);

    let inputs = atlas::read_folder(folder)?;
    let (image, file) = atlas::build(&inputs, &image_name, &settings)?;
    let png = image.to_png().map_err(|e| e.to_string())?;
    // One line per region, and `\n` line endings on every platform so `--check` agrees
    // everywhere (ron writes `\r\n` on Windows by default).
    let config = ron::ser::PrettyConfig::new()
        .struct_names(false)
        .depth_limit(2)
        .new_line("\n".to_owned());
    let ron = ron::ser::to_string_pretty(&file, config).map_err(|e| e.to_string())? + "\n";

    let summary = format!(
        "{} regions in a {}×{} atlas",
        file.regions.len(),
        file.size.0,
        file.size.1
    );
    if check {
        let stale: Vec<_> = [(&image_path, png.as_slice()), (out, ron.as_bytes())]
            .into_iter()
            .filter(|(path, bytes)| std::fs::read(path).ok().as_deref() != Some(*bytes))
            .map(|(path, _)| path.display().to_string())
            .collect();
        if !stale.is_empty() {
            return Err(format!(
                "out of date: {}; run cg_pack without --check",
                stale.join(", ")
            ));
        }
        return Ok(format!("up to date: {summary}"));
    }
    write(&image_path, &png)?;
    write(out, ron.as_bytes())?;
    Ok(format!(
        "wrote {} and {}: {summary}",
        image_path.display(),
        out.display()
    ))
}

fn write(path: &Path, bytes: &[u8]) -> Result<(), String> {
    if let Some(parent) = path.parent().filter(|p| !p.as_os_str().is_empty()) {
        std::fs::create_dir_all(parent).map_err(|e| format!("{}: {e}", parent.display()))?;
    }
    std::fs::write(path, bytes).map_err(|e| format!("{}: {e}", path.display()))
}
