//! The [`TextSystem`] resource: the fonts registered with cosmic-text, shaping and line
//! layout, bitmap-font layout, and glyph rasterizing into the atlas.

use super::atlas::{AtlasGlyph, GlyphAtlas};
use super::{PositionedGlyph, TextAlign};
use crate::font::{BitmapFont, Font, FontKind};
use bevy_ecs::resource::Resource;
use cg_assets::{AssetId, Assets};
use cg_core::Rect;
use cg_core::glam::Vec2;
use cg_render::{Color, Image};
use cosmic_text::fontdb::{self, Database};
use cosmic_text::{
    Align, Attrs, Buffer, CacheKeyFlags, Family, FontSystem, Metrics, Shaping, SwashCache, Wrap,
};
use std::collections::{HashMap, HashSet};

/// A font registered with cosmic-text.
struct OutlineEntry {
    version: u64,
    faces: Vec<fontdb::ID>,
    family: String,
    /// The font's own line spacing, as a multiple of the size.
    line_height: f32,
}

/// What to lay out.
#[derive(Debug, Clone, Copy)]
pub(crate) struct TextRequest<'a> {
    pub(crate) text: &'a str,
    pub(crate) font: AssetId<Font>,
    pub(crate) size: f32,
    pub(crate) color: Color,
    pub(crate) line_height: Option<f32>,
    pub(crate) align: TextAlign,
    /// Wrap lines to `width`.
    pub(crate) wrap: bool,
    /// The width to align (and wrap) in; `None` for the text's own width.
    pub(crate) width: Option<f32>,
    /// Whether `Start` and `End` mean right and left.
    pub(crate) rtl: bool,
}

/// Laid-out text: glyph quads relative to the text's top-left corner and the text's size.
#[derive(Debug, Clone, Default, PartialEq)]
pub(crate) struct Shaped {
    pub(crate) glyphs: Vec<PositionedGlyph>,
    pub(crate) size: Vec2,
}

/// Fonts, shaping and the glyph atlas for all text.
///
/// The UI lays text out through it; it is a resource so games can measure text too. Fonts
/// are registered from `Assets<Font>` as they load and change.
#[derive(Resource)]
pub struct TextSystem {
    fonts: FontSystem,
    swash: SwashCache,
    outlines: HashMap<AssetId<Font>, OutlineEntry>,
    pixel_faces: HashSet<fontdb::ID>,
    bitmaps: HashMap<AssetId<Font>, u64>,
    revision: u64,
    pub(crate) atlas: GlyphAtlas,
    buffer: Buffer,
}

/// cosmic-text's fallback lists name system fonts, which games do not load; with empty
/// lists it tries every loaded font for a character the text's own font lacks.
struct LoadedFontsOnly;

impl cosmic_text::Fallback for LoadedFontsOnly {
    fn common_fallback(&self) -> &[&'static str] {
        &[]
    }

    fn forbidden_fallback(&self) -> &[&'static str] {
        &[]
    }

    fn script_fallback(&self, _script: unicode_script::Script, _locale: &str) -> &[&'static str] {
        &[]
    }
}

impl Default for TextSystem {
    fn default() -> Self {
        let fonts = FontSystem::new_with_locale_and_db_and_fallback(
            "en-US".into(),
            Database::new(),
            LoadedFontsOnly,
        );
        // Not `Buffer::new`: it shapes right away, and with no fonts loaded yet that panics.
        let buffer = Buffer::new_empty(Metrics::new(16.0, 20.0));
        Self {
            fonts,
            swash: SwashCache::new(),
            outlines: HashMap::new(),
            pixel_faces: HashSet::new(),
            bitmaps: HashMap::new(),
            revision: 0,
            atlas: GlyphAtlas::default(),
            buffer,
        }
    }
}

impl TextSystem {
    /// Changes whenever a font is added, changed or removed, so laid-out text can tell it
    /// is stale.
    pub fn revision(&self) -> u64 {
        self.revision + self.atlas.generation
    }

    /// The number of glyphs rasterized so far.
    pub fn glyph_count(&self) -> usize {
        self.atlas.len()
    }

    /// Whether a font is ready to draw with.
    pub fn has_font(&self, font: AssetId<Font>) -> bool {
        self.outlines.contains_key(&font) || self.bitmaps.contains_key(&font)
    }

    /// Registers new and changed fonts and forgets removed ones.
    pub(crate) fn sync_fonts(&mut self, fonts: &Assets<Font>) {
        let mut changed = false;
        let stale: Vec<_> = self
            .outlines
            .keys()
            .chain(self.bitmaps.keys())
            .copied()
            .filter(|id| !fonts.contains(*id))
            .collect();
        for id in stale {
            self.forget(id);
            changed = true;
        }
        for (id, font, version) in fonts.iter_versions() {
            let known = self
                .outlines
                .get(&id)
                .map(|e| e.version)
                .or_else(|| self.bitmaps.get(&id).copied());
            if known == Some(version) {
                continue;
            }
            self.forget(id);
            changed = true;
            match &font.kind {
                FontKind::Outline(outline) => {
                    let faces = self
                        .fonts
                        .db_mut()
                        .load_font_source(fontdb::Source::Binary(outline.data.clone()));
                    let faces: Vec<_> = faces.into_iter().collect();
                    if font.settings.pixel {
                        self.pixel_faces.extend(faces.iter().copied());
                    }
                    let line_height = faces
                        .first()
                        .and_then(|&face| self.fonts.get_font(face, fontdb::Weight::NORMAL))
                        .map(|f| {
                            let m = f.as_swash().metrics(&[]);
                            (m.ascent + m.descent + m.leading) / f32::from(m.units_per_em.max(1))
                        })
                        .unwrap_or(1.2);
                    self.outlines.insert(
                        id,
                        OutlineEntry {
                            version,
                            faces,
                            family: outline.family().to_owned(),
                            line_height,
                        },
                    );
                }
                FontKind::Bitmap(_) => {
                    self.bitmaps.insert(id, version);
                }
            }
        }
        if changed {
            self.revision += 1;
        }
    }

    fn forget(&mut self, id: AssetId<Font>) {
        if let Some(entry) = self.outlines.remove(&id) {
            for face in entry.faces {
                self.pixel_faces.remove(&face);
                self.fonts.db_mut().remove_face(face);
            }
        }
        self.bitmaps.remove(&id);
    }

    /// Lays out text at `scale` raster pixels per UI pixel (the screen's scale; pixel fonts
    /// use 1), rasterizing new glyphs into the atlas. `None` if the font is not loaded.
    pub(crate) fn shape(
        &mut self,
        request: &TextRequest<'_>,
        fonts: &Assets<Font>,
        images: &mut Assets<Image>,
        scale: f32,
    ) -> Option<Shaped> {
        let font = fonts.get(request.font)?;
        match &font.kind {
            FontKind::Bitmap(bitmap) => Some(layout_bitmap(request, bitmap, font)),
            FontKind::Outline(_) => {
                for _ in 0..2 {
                    match self.shape_outline(request, font, images, scale) {
                        Some(Ok(shaped)) => return Some(shaped),
                        Some(Err(())) => self.atlas.reset(images),
                        None => return None,
                    }
                }
                log::warn!("text needs more glyphs than the glyph atlas holds; some are missing");
                None
            }
        }
    }

    /// The size of text laid out without rasterizing it.
    pub(crate) fn measure(
        &mut self,
        request: &TextRequest<'_>,
        fonts: &Assets<Font>,
    ) -> Option<Vec2> {
        let font = fonts.get(request.font)?;
        match &font.kind {
            FontKind::Bitmap(bitmap) => Some(layout_bitmap(request, bitmap, font).size),
            FontKind::Outline(_) => {
                self.run_layout(request, font)?;
                Some(self.buffer_size(request))
            }
        }
    }

    /// Shapes the request into the scratch buffer. `None` if its font is not registered.
    fn run_layout(&mut self, request: &TextRequest<'_>, font: &Font) -> Option<()> {
        let entry = self.outlines.get(&request.font)?;
        let size = request.size.max(1.0);
        let line_height = request
            .line_height
            .or(font.settings.line_height)
            .unwrap_or(entry.line_height)
            * size;
        let mut flags = CacheKeyFlags::empty();
        if font.settings.pixel {
            flags |= CacheKeyFlags::PIXEL_FONT | CacheKeyFlags::DISABLE_HINTING;
        }
        let attrs = Attrs::new()
            .family(Family::Name(&entry.family))
            .cache_key_flags(flags);
        let align = match (request.align, request.rtl) {
            (TextAlign::Start, false) | (TextAlign::End, true) | (TextAlign::Left, _) => {
                Align::Left
            }
            (TextAlign::Start, true) | (TextAlign::End, false) | (TextAlign::Right, _) => {
                Align::Right
            }
            (TextAlign::Center, _) => Align::Center,
            (TextAlign::Justified, _) => Align::Justified,
        };
        let wrap = if request.wrap && request.width.is_some() {
            Wrap::WordOrGlyph
        } else {
            Wrap::None
        };
        let mut buffer = self.buffer.borrow_with(&mut self.fonts);
        buffer.set_metrics_and_size(
            Metrics::new(size, line_height),
            request.width.map(|w| w.max(0.0)),
            None,
        );
        buffer.set_wrap(wrap);
        buffer.set_text(request.text, &attrs, Shaping::Advanced, Some(align));
        buffer.shape_until_scroll(false);
        Some(())
    }

    /// The size of the text in the scratch buffer.
    fn buffer_size(&self, request: &TextRequest<'_>) -> Vec2 {
        let line_height = self.buffer.metrics().line_height;
        let mut width: f32 = 0.0;
        let mut height: f32 = 0.0;
        for run in self.buffer.layout_runs() {
            width = width.max(run.line_w);
            height = height.max(run.line_top + run.line_height);
        }
        let height = if height > 0.0 { height } else { line_height };
        Vec2::new(request.width.unwrap_or(width.ceil()), height.ceil())
    }

    /// Shapes and rasterizes; `Some(Err(()))` when the atlas is full.
    fn shape_outline(
        &mut self,
        request: &TextRequest<'_>,
        font: &Font,
        images: &mut Assets<Image>,
        scale: f32,
    ) -> Option<Result<Shaped, ()>> {
        self.run_layout(request, font)?;
        let size = self.buffer_size(request);
        let mut glyphs = Vec::new();
        for run in self.buffer.layout_runs() {
            let baseline = (run.line_y * scale).round() as i32;
            for glyph in run.glyphs {
                let pixel = self.pixel_faces.contains(&glyph.font_id);
                let raster = if pixel { 1.0 } else { scale };
                let mut placed = glyph.clone();
                if pixel {
                    // Pixel fonts sit on whole UI pixels, with no sub-pixel offsets.
                    placed.x = placed.x.round();
                }
                let physical = placed.physical((0.0, 0.0), raster);
                let atlas_glyph = match self.atlas.cached(&physical.cache_key) {
                    Some(cached) => cached,
                    None => {
                        let image = self
                            .swash
                            .get_image_uncached(&mut self.fonts, physical.cache_key);
                        match self
                            .atlas
                            .insert(physical.cache_key, image.as_ref(), pixel, images)
                        {
                            Ok(glyph) => glyph,
                            Err(()) => return Some(Err(())),
                        }
                    }
                };
                let Some(AtlasGlyph {
                    page,
                    source,
                    left,
                    top,
                }) = atlas_glyph
                else {
                    continue;
                };
                let Some(texture) = self.atlas.page(page) else {
                    continue;
                };
                let baseline = if pixel {
                    (run.line_y).round() as i32
                } else {
                    baseline
                };
                let x = (physical.x + left) as f32 / raster;
                let y = (baseline + physical.y - top) as f32 / raster;
                glyphs.push(PositionedGlyph {
                    texture: texture.id(),
                    source,
                    rect: Rect::from_min_size(Vec2::new(x, y), source.size() / raster),
                    color: request.color,
                });
            }
        }
        Some(Ok(Shaped { glyphs, size }))
    }
}

/// Lays out text in a bitmap font: no shaping, left to right, wrapped at spaces.
fn layout_bitmap(request: &TextRequest<'_>, bitmap: &BitmapFont, font: &Font) -> Shaped {
    let k = request.size.max(1.0) / bitmap.size;
    let line_height = request
        .line_height
        .or(font.settings.line_height)
        .map_or(bitmap.line_height * k, |m| m * request.size);
    let advance = |c: char, next: Option<char>| {
        let glyph = bitmap.glyph(c).map_or(0.0, |g| g.advance);
        let kern = next
            .and_then(|n| bitmap.kerning.get(&(c, n)))
            .copied()
            .unwrap_or(0.0);
        (glyph + kern) * k
    };
    let width_of = |word: &str| {
        let chars: Vec<char> = word.chars().collect();
        (0..chars.len())
            .map(|i| advance(chars[i], chars.get(i + 1).copied()))
            .sum::<f32>()
    };
    let space = advance(' ', None);

    // Break into lines of words.
    let mut lines: Vec<String> = Vec::new();
    for paragraph in request.text.split('\n') {
        let mut line = String::new();
        let mut line_width = 0.0;
        for word in paragraph.split(' ') {
            let w = width_of(word);
            let fits = match request.width {
                Some(max) if request.wrap && !line.is_empty() => line_width + space + w <= max,
                _ => true,
            };
            if !fits {
                lines.push(core::mem::take(&mut line));
                line_width = 0.0;
            }
            if !line.is_empty() {
                line.push(' ');
                line_width += space;
            }
            line.push_str(word);
            line_width += w;
        }
        lines.push(line);
    }

    let widths: Vec<f32> = lines.iter().map(|l| width_of(l)).collect();
    let natural = widths.iter().copied().fold(0.0, f32::max);
    let box_width = request.width.unwrap_or(natural);
    let mut glyphs = Vec::new();
    for (row, (line, width)) in lines.iter().zip(&widths).enumerate() {
        let free = box_width - width;
        let start = match (request.align, request.rtl) {
            (TextAlign::Start, false) | (TextAlign::End, true) | (TextAlign::Left, _) => 0.0,
            (TextAlign::Justified, _) => 0.0,
            (TextAlign::Start, true) | (TextAlign::End, false) | (TextAlign::Right, _) => free,
            (TextAlign::Center, _) => (free * 0.5).floor(),
        };
        let top = row as f32 * line_height;
        let chars: Vec<char> = line.chars().collect();
        let mut pen = start;
        for (i, &c) in chars.iter().enumerate() {
            if let Some(glyph) = bitmap.glyph(c)
                && let Some(page) = bitmap.pages.get(glyph.page)
                && !glyph.source.is_empty()
            {
                let min = Vec2::new(pen.round(), top.round()) + glyph.offset * k;
                glyphs.push(PositionedGlyph {
                    texture: page.id(),
                    source: glyph.source,
                    rect: Rect::from_min_size(min, glyph.source.size() * k),
                    color: request.color,
                });
            }
            pen += advance(c, chars.get(i + 1).copied());
        }
    }
    Shaped {
        glyphs,
        size: Vec2::new(box_width, (lines.len() as f32 * line_height).ceil()),
    }
}
