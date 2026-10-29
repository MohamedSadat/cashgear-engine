//! Packing a folder of PNGs into one atlas image and its `.atlas.ron` file.

use cashgear_engine::render::{AtlasFile, AtlasRegion, Image};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

/// Settings for one atlas.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Settings {
    /// Transparent pixels between regions and around the edge.
    pub padding: u32,
    /// Pixels by which each region's edge is repeated outwards, so linear filtering does
    /// not bleed in the neighbours. It must fit in the padding.
    pub extrude: u32,
    /// The largest width or height the atlas may have.
    pub max_size: u32,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            padding: 1,
            extrude: 0,
            max_size: 4096,
        }
    }
}

/// One image to pack and the name its region gets.
pub struct Input {
    pub name: String,
    pub image: Image,
}

/// Where each input went, and the atlas size.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Layout {
    pub width: u32,
    pub height: u32,
    /// The top-left corner of each input, in input order.
    pub positions: Vec<(u32, u32)>,
}

/// Packs rectangles of `sizes` into shelves, tallest first, in the narrowest power-of-two
/// width that keeps the atlas no taller than it is wide (or within `max_size`). The result
/// depends only on the sizes and their order, so the same folder always packs the same way.
pub fn pack(sizes: &[(u32, u32)], settings: &Settings) -> Result<Layout, String> {
    let pad = settings.padding;
    let widest = sizes.iter().map(|s| s.0).max().unwrap_or(1);
    let area: u64 = sizes
        .iter()
        .map(|&(w, h)| u64::from(w + pad) * u64::from(h + pad))
        .sum();
    let mut width = (widest + 2 * pad)
        .max((area as f64).sqrt() as u32)
        .next_power_of_two();
    // Tallest first; ties keep their (sorted) input order.
    let mut order: Vec<usize> = (0..sizes.len()).collect();
    order.sort_by_key(|&i| core::cmp::Reverse(sizes[i].1));
    let too_big = || {
        format!(
            "the images do not fit in a {0}×{0} atlas; raise --max-size",
            settings.max_size
        )
    };
    loop {
        if width > settings.max_size {
            return Err(too_big());
        }
        let (positions, height) = shelves(sizes, &order, width, pad);
        let widest_allowed = width * 2 > settings.max_size;
        if height <= width || widest_allowed {
            if height > settings.max_size {
                return Err(too_big());
            }
            return Ok(Layout {
                width,
                height,
                positions,
            });
        }
        width *= 2;
    }
}

/// Places rectangles left to right on shelves as tall as their first (tallest) one.
fn shelves(sizes: &[(u32, u32)], order: &[usize], width: u32, pad: u32) -> (Vec<(u32, u32)>, u32) {
    let mut positions = vec![(0, 0); sizes.len()];
    let (mut x, mut y, mut shelf) = (pad, pad, 0);
    for &i in order {
        let (w, h) = sizes[i];
        if x + w + pad > width && x > pad {
            y += shelf + pad;
            x = pad;
            shelf = 0;
        }
        positions[i] = (x, y);
        x += w + pad;
        shelf = shelf.max(h);
    }
    (positions, y + shelf + pad)
}

/// Every PNG under `folder`, named by its path inside it without the extension, sorted.
pub fn read_folder(folder: &Path) -> Result<Vec<Input>, String> {
    let mut files = Vec::new();
    collect(folder, folder, &mut files)?;
    files.sort();
    files
        .into_iter()
        .map(|(name, path)| {
            let bytes = std::fs::read(&path).map_err(|e| format!("{}: {e}", path.display()))?;
            let image = Image::from_png(&bytes).map_err(|e| format!("{}: {e}", path.display()))?;
            Ok(Input { name, image })
        })
        .collect()
}

fn collect(root: &Path, dir: &Path, out: &mut Vec<(String, PathBuf)>) -> Result<(), String> {
    let entries = std::fs::read_dir(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    for entry in entries {
        let path = entry.map_err(|e| e.to_string())?.path();
        if path.is_dir() {
            collect(root, &path, out)?;
        } else if path
            .extension()
            .is_some_and(|ext| ext.eq_ignore_ascii_case("png"))
        {
            let relative = path.strip_prefix(root).map_err(|e| e.to_string())?;
            let name = relative
                .with_extension("")
                .components()
                .map(|c| c.as_os_str().to_string_lossy().into_owned())
                .collect::<Vec<_>>()
                .join("/");
            out.push((name, path));
        }
    }
    Ok(())
}

/// Packs `inputs` and draws the atlas image. `image_name` is the file name the `.atlas.ron`
/// refers to.
pub fn build(
    inputs: &[Input],
    image_name: &str,
    settings: &Settings,
) -> Result<(Image, AtlasFile), String> {
    if inputs.is_empty() {
        return Err("no PNG files to pack".into());
    }
    if settings.extrude * 2 > settings.padding {
        return Err(format!(
            "--extrude {} needs --padding of at least {}",
            settings.extrude,
            settings.extrude * 2
        ));
    }
    let sizes: Vec<_> = inputs
        .iter()
        .map(|i| (i.image.width(), i.image.height()))
        .collect();
    let layout = pack(&sizes, settings)?;
    let mut pixels = vec![[0u8; 4]; (layout.width * layout.height) as usize];
    let mut regions = BTreeMap::new();
    let e = settings.extrude as i64;
    for (input, &(x, y)) in inputs.iter().zip(&layout.positions) {
        let (w, h) = (input.image.width(), input.image.height());
        // Copy the image, repeating its edge pixels `extrude` times outwards.
        for dy in -e..h as i64 + e {
            for dx in -e..w as i64 + e {
                let sx = dx.clamp(0, w as i64 - 1) as u32;
                let sy = dy.clamp(0, h as i64 - 1) as u32;
                let tx = (x as i64 + dx) as u32;
                let ty = (y as i64 + dy) as u32;
                pixels[(ty * layout.width + tx) as usize] =
                    input.image.pixel(sx, sy).unwrap_or_default();
            }
        }
        if regions
            .insert(input.name.clone(), AtlasRegion { x, y, w, h })
            .is_some()
        {
            return Err(format!("two images are called `{}`", input.name));
        }
    }
    let image = Image::from_fn(layout.width, layout.height, |x, y| {
        pixels[(y * layout.width + x) as usize]
    });
    let file = AtlasFile {
        image: image_name.to_owned(),
        size: (layout.width, layout.height),
        regions,
    };
    Ok((image, file))
}

#[cfg(test)]
mod tests {
    use super::*;
    use cashgear_engine::render::Color;

    fn overlaps(a: (u32, u32, u32, u32), b: (u32, u32, u32, u32)) -> bool {
        a.0 < b.0 + b.2 && b.0 < a.0 + a.2 && a.1 < b.1 + b.3 && b.1 < a.1 + a.3
    }

    #[test]
    fn packed_rectangles_stay_apart_and_inside() {
        let sizes: Vec<(u32, u32)> = (0..40).map(|i| (8 + i % 7 * 5, 6 + i % 5 * 9)).collect();
        let settings = Settings::default();
        let layout = pack(&sizes, &settings).unwrap();
        assert!(layout.width.is_power_of_two());
        let rects: Vec<_> = sizes
            .iter()
            .zip(&layout.positions)
            .map(|(&(w, h), &(x, y))| (x, y, w + 1, h + 1))
            .collect();
        for (i, a) in rects.iter().enumerate() {
            assert!(a.0 >= 1 && a.1 >= 1);
            assert!(a.0 + a.2 <= layout.width && a.1 + a.3 <= layout.height);
            for b in &rects[i + 1..] {
                assert!(!overlaps(*a, *b), "{a:?} and {b:?} overlap");
            }
        }
        assert_eq!(
            pack(&sizes, &settings).unwrap(),
            layout,
            "packing is deterministic"
        );
    }

    #[test]
    fn too_big_is_an_error() {
        let settings = Settings {
            max_size: 64,
            ..Settings::default()
        };
        assert!(pack(&[(100, 10)], &settings).is_err());
        assert!(pack(&[(30, 30); 4], &settings).is_ok());
        assert!(pack(&[(30, 30); 5], &settings).is_err());
    }

    #[test]
    fn build_copies_pixels_and_names_regions() {
        let inputs = vec![
            Input {
                name: "red".into(),
                image: Image::filled(2, 3, Color::RED),
            },
            Input {
                name: "items/blue".into(),
                image: Image::filled(4, 2, Color::BLUE),
            },
        ];
        let settings = Settings {
            padding: 2,
            extrude: 1,
            ..Settings::default()
        };
        let (image, file) = build(&inputs, "test.png", &settings).unwrap();
        assert_eq!(file.image, "test.png");
        assert_eq!(file.size, (image.width(), image.height()));
        let red = file.regions["red"];
        assert_eq!((red.w, red.h), (2, 3));
        assert_eq!(image.pixel(red.x, red.y), Some(Color::RED.to_rgba8()));
        // Extruded one pixel to the left and above.
        assert_eq!(
            image.pixel(red.x - 1, red.y - 1),
            Some(Color::RED.to_rgba8())
        );
        let blue = file.regions["items/blue"];
        assert_eq!(
            image.pixel(blue.x + 3, blue.y + 1),
            Some(Color::BLUE.to_rgba8())
        );
    }

    #[test]
    fn extrusion_needs_room() {
        let inputs = vec![Input {
            name: "a".into(),
            image: Image::filled(1, 1, Color::RED),
        }];
        let settings = Settings {
            padding: 1,
            extrude: 1,
            ..Settings::default()
        };
        assert!(build(&inputs, "a.png", &settings).is_err());
    }
}
