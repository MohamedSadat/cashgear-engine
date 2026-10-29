//! The glyph atlas: rasterized glyphs packed into a few large images that the sprite
//! batcher draws from.

use cg_assets::{Assets, Handle};
use cg_core::Rect;
use cg_render::{Color, Image};
use cosmic_text::{CacheKey, SwashContent, SwashImage};
use std::collections::HashMap;

/// The width and height of each atlas image.
pub(crate) const PAGE_SIZE: u32 = 1024;
/// At most this many pages; when they are full the atlas starts again empty.
const MAX_PAGES: usize = 4;
/// Empty texels between glyphs, so nearest sampling never picks up a neighbour.
const PADDING: u32 = 1;

/// A glyph in the atlas: where its image is and how it sits relative to the pen.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct AtlasGlyph {
    pub(crate) page: usize,
    /// Texels on the page.
    pub(crate) source: Rect,
    /// The image's left edge relative to the pen, in raster pixels.
    pub(crate) left: i32,
    /// The image's top edge above the baseline, in raster pixels.
    pub(crate) top: i32,
}

/// A row of glyphs on a page.
#[derive(Debug)]
struct Shelf {
    y: u32,
    height: u32,
    next_x: u32,
}

#[derive(Debug)]
struct Page {
    image: Handle<Image>,
    shelves: Vec<Shelf>,
    next_y: u32,
}

impl Page {
    /// Finds room for a `width` by `height` image, padding included.
    fn allocate(&mut self, width: u32, height: u32) -> Option<(u32, u32)> {
        let (w, h) = (width + PADDING, height + PADDING);
        // The shortest shelf that fits, wasting at most half its height.
        let shelf = self
            .shelves
            .iter_mut()
            .filter(|s| s.height >= h && s.height <= h * 2 && s.next_x + w <= PAGE_SIZE)
            .min_by_key(|s| s.height);
        if let Some(shelf) = shelf {
            let x = shelf.next_x;
            shelf.next_x += w;
            return Some((x, shelf.y));
        }
        if self.next_y + h > PAGE_SIZE || w > PAGE_SIZE {
            return None;
        }
        let y = self.next_y;
        self.next_y += h;
        self.shelves.push(Shelf {
            y,
            height: h,
            next_x: w,
        });
        Some((0, y))
    }
}

/// Rasterized glyphs, by cosmic-text cache key.
#[derive(Debug, Default)]
pub(crate) struct GlyphAtlas {
    pages: Vec<Page>,
    glyphs: HashMap<CacheKey, Option<AtlasGlyph>>,
    /// Counts resets; text laid out against an older generation must be laid out again.
    pub(crate) generation: u64,
}

impl GlyphAtlas {
    /// The image of a page.
    pub(crate) fn page(&self, page: usize) -> Option<&Handle<Image>> {
        self.pages.get(page).map(|p| &p.image)
    }

    /// Whether the glyph has been rasterized (it may have no image, like a space).
    pub(crate) fn cached(&self, key: &CacheKey) -> Option<Option<AtlasGlyph>> {
        self.glyphs.get(key).copied()
    }

    /// Adds a rasterized glyph. `pixel` turns coverage into fully on or off texels.
    /// Returns `Err(())` when the atlas is full; the caller resets it and lays out again.
    pub(crate) fn insert(
        &mut self,
        key: CacheKey,
        image: Option<&SwashImage>,
        pixel: bool,
        images: &mut Assets<Image>,
    ) -> Result<Option<AtlasGlyph>, ()> {
        let Some(image) = image.filter(|i| i.placement.width > 0 && i.placement.height > 0) else {
            self.glyphs.insert(key, None);
            return Ok(None);
        };
        let (width, height) = (image.placement.width, image.placement.height);
        let mut placed = None;
        for (index, page) in self.pages.iter_mut().enumerate() {
            if let Some(at) = page.allocate(width, height) {
                placed = Some((index, at));
                break;
            }
        }
        if placed.is_none() && self.pages.len() < MAX_PAGES {
            let mut page = Page {
                image: images.add(Image::filled(PAGE_SIZE, PAGE_SIZE, Color::TRANSPARENT)),
                shelves: Vec::new(),
                next_y: 0,
            };
            placed = page
                .allocate(width, height)
                .map(|at| (self.pages.len(), at));
            self.pages.push(page);
        }
        let Some((page, (x, y))) = placed else {
            return Err(());
        };
        if let Some(target) = images.get_mut(self.pages[page].image.id()) {
            blit(target, image, x, y, pixel);
        }
        let glyph = AtlasGlyph {
            page,
            source: Rect::new(x as f32, y as f32, (x + width) as f32, (y + height) as f32),
            left: image.placement.left,
            top: image.placement.top,
        };
        self.glyphs.insert(key, Some(glyph));
        Ok(Some(glyph))
    }

    /// Forgets every glyph and clears the pages, keeping their images.
    pub(crate) fn reset(&mut self, images: &mut Assets<Image>) {
        self.glyphs.clear();
        for page in &mut self.pages {
            page.shelves.clear();
            page.next_y = 0;
            if let Some(image) = images.get_mut(page.image.id()) {
                image.data_mut().fill(0);
            }
        }
        self.generation += 1;
        log::debug!("the glyph atlas was full and started again");
    }

    /// The number of glyphs rasterized so far.
    pub(crate) fn len(&self) -> usize {
        self.glyphs.len()
    }
}

/// Copies a glyph image into the page at `(x, y)` as white texels with the glyph's
/// coverage as alpha (or the glyph's own colours, for colour glyphs).
fn blit(target: &mut Image, image: &SwashImage, x: u32, y: u32, pixel: bool) {
    let stride = target.width() as usize;
    let (width, height) = (
        image.placement.width as usize,
        image.placement.height as usize,
    );
    let data = target.data_mut();
    for row in 0..height {
        for col in 0..width {
            let texel = ((y as usize + row) * stride + x as usize + col) * 4;
            let src = row * width + col;
            let rgba = match image.content {
                SwashContent::Mask => {
                    let alpha = image.data[src];
                    let alpha = if pixel {
                        if alpha >= 128 { 255 } else { 0 }
                    } else {
                        alpha
                    };
                    [255, 255, 255, alpha]
                }
                SwashContent::Color => {
                    let i = src * 4;
                    [
                        image.data[i],
                        image.data[i + 1],
                        image.data[i + 2],
                        image.data[i + 3],
                    ]
                }
                SwashContent::SubpixelMask => {
                    let i = src * 4;
                    let alpha = image.data[i..i + 3]
                        .iter()
                        .map(|&a| u16::from(a))
                        .sum::<u16>()
                        / 3;
                    [255, 255, 255, alpha as u8]
                }
            };
            data[texel..texel + 4].copy_from_slice(&rgba);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shelves_pack_rows_and_new_shelves_start_below() {
        let mut page = Page {
            image: Handle::default(),
            shelves: Vec::new(),
            next_y: 0,
        };
        assert_eq!(page.allocate(10, 12), Some((0, 0)));
        assert_eq!(page.allocate(5, 12), Some((11, 0)));
        // Much shorter glyphs get their own shelf rather than wasting a tall one.
        assert_eq!(page.allocate(4, 4), Some((0, 13)));
        assert_eq!(page.allocate(4, 4), Some((5, 13)));
        assert_eq!(page.allocate(PAGE_SIZE + 1, 1), None);
    }

    #[test]
    fn a_full_page_reports_no_room() {
        let mut page = Page {
            image: Handle::default(),
            shelves: Vec::new(),
            next_y: 0,
        };
        let big = PAGE_SIZE - PADDING;
        assert_eq!(page.allocate(big, big), Some((0, 0)));
        assert_eq!(page.allocate(1, 1), None);
    }
}
