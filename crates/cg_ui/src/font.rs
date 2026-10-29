//! Fonts: outline fonts (TrueType and OpenType files) shaped by cosmic-text, and bitmap
//! fonts in the BMFont text format, both as [`Font`] assets.

use cg_assets::{AssetId, AssetLoader, BoxError, BoxedFuture, Handle, LoadContext};
use cg_core::Rect;
use cg_core::glam::Vec2;
use cg_render::Image;
use core::fmt;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;

/// The embedded default font: Pixelify Sans (SIL Open Font License 1.1).
pub(crate) const DEFAULT_FONT_DATA: &[u8] = include_bytes!("../fonts/PixelifySans.ttf");

/// The size at which the default font's pixels are one UI pixel each: 11. Multiples of it
/// (22, 33) stay sharp too.
pub const DEFAULT_FONT_SIZE: f32 = 11.0;

/// A font for [`Text`](crate::Text): an outline font (`.ttf`, `.otf`), a bitmap font
/// (`.fnt`, BMFont's text format), or a `.font.ron` file naming either with
/// [`FontSettings`].
///
/// The default handle is the font built into the engine, Pixelify Sans, a pixel font that
/// is sharp at multiples of [`DEFAULT_FONT_SIZE`].
///
/// ```ron
/// // fonts/title.font.ron
/// (file: "PixelifySans.ttf", pixel: true, line_height: Some(1.1))
/// ```
///
/// Outline fonts are shaped by cosmic-text, so Arabic and other scripts join and order
/// correctly, and a character the font lacks is drawn from another loaded font that has
/// it. Bitmap fonts draw their glyph images as they are, left to right, without shaping.
#[derive(Debug, Clone, PartialEq)]
pub struct Font {
    /// The font's glyphs.
    pub kind: FontKind,
    /// How it is drawn.
    pub settings: FontSettings,
}

impl cg_assets::Asset for Font {}

impl Font {
    /// The id of the built-in font, which the default `Handle<Font>` points at.
    pub const DEFAULT: AssetId<Font> = AssetId::DEFAULT;

    /// An outline font from the bytes of a TrueType or OpenType file.
    pub fn outline(data: Vec<u8>, settings: FontSettings) -> Result<Self, FontError> {
        Ok(Self {
            kind: FontKind::Outline(OutlineFont::new(data)?),
            settings,
        })
    }

    /// The built-in font.
    pub fn default_font() -> Self {
        Self::outline(
            DEFAULT_FONT_DATA.to_vec(),
            FontSettings {
                pixel: true,
                ..FontSettings::default()
            },
        )
        .unwrap_or_else(|err| unreachable!("the built-in font is valid: {err}"))
    }
}

/// The glyphs of a [`Font`].
#[derive(Debug, Clone, PartialEq)]
pub enum FontKind {
    /// A TrueType or OpenType font.
    Outline(OutlineFont),
    /// A BMFont bitmap font.
    Bitmap(BitmapFont),
}

/// How a [`Font`] is drawn. In a `.font.ron` file every field but `file` is optional.
#[derive(Debug, Clone, Copy, PartialEq, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct FontSettings {
    /// A pixel font: glyphs are rasterized at the UI's own pixel size without smoothing,
    /// then scaled up with the UI, so they stay sharp in a pixel-art game. Use sizes that
    /// are multiples of the font's pixel grid. Other fonts are rasterized at the screen's
    /// resolution with smooth edges.
    pub pixel: bool,
    /// The distance between lines as a multiple of the text size. `None` uses the font's
    /// own spacing (for bitmap fonts, its `lineHeight`).
    pub line_height: Option<f32>,
}

/// The contents of a `.font.ron` file.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FontFile {
    /// The font file, relative to the `.font.ron` file: `.ttf`, `.otf` or `.fnt`.
    pub file: String,
    /// See [`FontSettings::pixel`].
    #[serde(default)]
    pub pixel: bool,
    /// See [`FontSettings::line_height`].
    #[serde(default)]
    pub line_height: Option<f32>,
}

/// A TrueType or OpenType font file.
#[derive(Clone, PartialEq)]
pub struct OutlineFont {
    pub(crate) data: Arc<Vec<u8>>,
    family: String,
}

impl fmt::Debug for OutlineFont {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("OutlineFont")
            .field("family", &self.family)
            .field("bytes", &self.data.len())
            .finish()
    }
}

impl OutlineFont {
    /// Reads the font's name from the file. Fails if the bytes are not a font.
    pub fn new(data: Vec<u8>) -> Result<Self, FontError> {
        let data = Arc::new(data);
        let mut db = cosmic_text::fontdb::Database::new();
        db.load_font_source(cosmic_text::fontdb::Source::Binary(data.clone()));
        let family = db
            .faces()
            .next()
            .and_then(|face| face.families.first().map(|(name, _)| name.clone()))
            .ok_or(FontError::NotAFont)?;
        Ok(Self { data, family })
    }

    /// The font's family name, as the file gives it.
    pub fn family(&self) -> &str {
        &self.family
    }
}

/// A bitmap font in the BMFont format: glyph images on one or more page images.
///
/// Read from BMFont's text format (`.fnt`), which BMFont, Hiero, Littera, ShoeBox and
/// other tools export. Sizes are in the font's pixels; text is drawn at
/// `size / font.size()` times, so whole multiples of [`size`](Self::size) stay sharp.
#[derive(Debug, Clone, PartialEq)]
pub struct BitmapFont {
    pub(crate) size: f32,
    pub(crate) line_height: f32,
    pub(crate) base: f32,
    pub(crate) pages: Vec<Handle<Image>>,
    pub(crate) glyphs: HashMap<char, BitmapGlyph>,
    pub(crate) kerning: HashMap<(char, char), f32>,
}

/// One glyph of a [`BitmapFont`].
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct BitmapGlyph {
    /// Where the glyph is on its page, in texels from the top-left.
    pub source: Rect,
    /// Where the glyph's image goes relative to the pen, y down from the line's top.
    pub offset: Vec2,
    /// How far the pen moves after the glyph.
    pub advance: f32,
    /// The page image it is on.
    pub page: usize,
}

impl BitmapFont {
    /// The size the font was drawn at (BMFont's `info size`).
    pub fn size(&self) -> f32 {
        self.size
    }

    /// The distance between lines, in the font's pixels.
    pub fn line_height(&self) -> f32 {
        self.line_height
    }

    /// The glyph for a character, if the font has it.
    pub fn glyph(&self, c: char) -> Option<&BitmapGlyph> {
        self.glyphs.get(&c)
    }

    /// The page images, in page order.
    pub fn pages(&self) -> &[Handle<Image>] {
        &self.pages
    }

    /// Parses BMFont's text format. `load_page` gets each page's file name and returns the
    /// image handle to draw it from.
    pub fn parse(
        text: &str,
        mut load_page: impl FnMut(&str) -> Handle<Image>,
    ) -> Result<Self, FontError> {
        let mut font = Self {
            size: 0.0,
            line_height: 0.0,
            base: 0.0,
            pages: Vec::new(),
            glyphs: HashMap::new(),
            kerning: HashMap::new(),
        };
        let mut page_files: Vec<(usize, String)> = Vec::new();
        for (number, line) in text.lines().enumerate() {
            let line = line.trim();
            let (tag, rest) = line.split_once(char::is_whitespace).unwrap_or((line, ""));
            let fields = fields(rest).map_err(|err| FontError::Syntax {
                line: number + 1,
                message: err,
            })?;
            let get = |key: &str| -> Result<f32, FontError> {
                fields
                    .get(key)
                    .ok_or_else(|| FontError::Syntax {
                        line: number + 1,
                        message: format!("`{tag}` has no `{key}`"),
                    })?
                    .parse::<f32>()
                    .map_err(|_| FontError::Syntax {
                        line: number + 1,
                        message: format!("`{key}` is not a number"),
                    })
            };
            match tag {
                "info" => font.size = get("size")?.abs(),
                "common" => {
                    font.line_height = get("lineHeight")?;
                    font.base = get("base")?;
                }
                "page" => {
                    let file = fields.get("file").ok_or_else(|| FontError::Syntax {
                        line: number + 1,
                        message: "`page` has no `file`".into(),
                    })?;
                    page_files.push((get("id")? as usize, file.clone()));
                }
                "char" => {
                    let id = get("id")? as u32;
                    let Some(c) = char::from_u32(id) else {
                        continue;
                    };
                    let (x, y, w, h) = (get("x")?, get("y")?, get("width")?, get("height")?);
                    font.glyphs.insert(
                        c,
                        BitmapGlyph {
                            source: Rect::new(x, y, x + w, y + h),
                            offset: Vec2::new(get("xoffset")?, get("yoffset")?),
                            advance: get("xadvance")?,
                            page: get("page").unwrap_or(0.0) as usize,
                        },
                    );
                }
                "kerning" => {
                    let pair = (
                        char::from_u32(get("first")? as u32),
                        char::from_u32(get("second")? as u32),
                    );
                    if let (Some(a), Some(b)) = pair {
                        font.kerning.insert((a, b), get("amount")?);
                    }
                }
                _ => {}
            }
        }
        if font.line_height <= 0.0 {
            return Err(FontError::Syntax {
                line: 0,
                message: "no `common` line with a `lineHeight`".into(),
            });
        }
        if font.size <= 0.0 {
            font.size = font.line_height;
        }
        page_files.sort_by_key(|(id, _)| *id);
        font.pages = page_files.iter().map(|(_, file)| load_page(file)).collect();
        if let Some(glyph) = font.glyphs.values().find(|g| g.page >= font.pages.len()) {
            return Err(FontError::Syntax {
                line: 0,
                message: format!("a glyph is on page {}, which is not listed", glyph.page),
            });
        }
        Ok(font)
    }
}

/// Splits `key=value key="quoted value"` pairs.
fn fields(text: &str) -> Result<HashMap<String, String>, String> {
    let mut out = HashMap::new();
    let mut rest = text.trim_start();
    while !rest.is_empty() {
        let (key, after) = rest
            .split_once('=')
            .ok_or_else(|| format!("expected `key=value` at `{rest}`"))?;
        let key = key.trim();
        let (value, after) = if let Some(quoted) = after.strip_prefix('"') {
            let end = quoted
                .find('"')
                .ok_or_else(|| format!("unclosed quote after `{key}=`"))?;
            (&quoted[..end], &quoted[end + 1..])
        } else {
            let end = after.find(char::is_whitespace).unwrap_or(after.len());
            (&after[..end], &after[end..])
        };
        out.insert(key.to_owned(), value.to_owned());
        rest = after.trim_start();
    }
    Ok(out)
}

/// Why a font could not be loaded.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FontError {
    /// The file is not a TrueType or OpenType font.
    NotAFont,
    /// A BMFont file could not be read.
    Syntax {
        /// The line, counting from 1 (0 for the file as a whole).
        line: usize,
        /// What is wrong.
        message: String,
    },
    /// A `.font.ron` file names a file of an unknown kind.
    UnknownFile(String),
}

impl fmt::Display for FontError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NotAFont => write!(f, "not a TrueType or OpenType font"),
            Self::Syntax { line: 0, message } => write!(f, "bad BMFont file: {message}"),
            Self::Syntax { line, message } => write!(f, "bad BMFont file, line {line}: {message}"),
            Self::UnknownFile(file) => {
                write!(f, "`{file}` is not a .ttf, .otf or .fnt file")
            }
        }
    }
}

impl std::error::Error for FontError {}

/// Loads [`Font`]s from `.ttf`, `.otf`, `.fnt` and `.font.ron` files. `UiPlugin` registers
/// it.
#[derive(Debug, Default)]
pub struct FontLoader;

impl AssetLoader for FontLoader {
    type Asset = Font;

    fn extensions(&self) -> &[&str] {
        &["ttf", "otf", "fnt", "font.ron"]
    }

    fn load<'a>(&'a self, ctx: &'a mut LoadContext) -> BoxedFuture<'a, Result<Font, BoxError>> {
        Box::pin(async move {
            let path = ctx.path().to_ascii_lowercase();
            if path.ends_with(".font.ron") {
                let file: FontFile = ron::de::from_bytes(ctx.bytes())?;
                let settings = FontSettings {
                    pixel: file.pixel,
                    line_height: file.line_height,
                };
                let target = ctx.resolve(&file.file);
                let bytes = ctx.read(&target).await?;
                let lower = target.to_ascii_lowercase();
                let kind = if lower.ends_with(".fnt") {
                    FontKind::Bitmap(bitmap(ctx, &target, &bytes)?)
                } else if lower.ends_with(".ttf") || lower.ends_with(".otf") {
                    FontKind::Outline(OutlineFont::new(bytes)?)
                } else {
                    return Err(FontError::UnknownFile(file.file).into());
                };
                Ok(Font { kind, settings })
            } else if path.ends_with(".fnt") {
                let path = ctx.path().to_owned();
                let bytes = ctx.take_bytes();
                Ok(Font {
                    kind: FontKind::Bitmap(bitmap(ctx, &path, &bytes)?),
                    settings: FontSettings {
                        pixel: true,
                        ..FontSettings::default()
                    },
                })
            } else {
                Ok(Font::outline(ctx.take_bytes(), FontSettings::default())?)
            }
        })
    }
}

/// Parses a BMFont file read from `path`, loading its pages through the context.
fn bitmap(ctx: &mut LoadContext, path: &str, bytes: &[u8]) -> Result<BitmapFont, BoxError> {
    let text = core::str::from_utf8(bytes)?;
    Ok(BitmapFont::parse(text, |file| {
        ctx.load(cg_assets::path::resolve(path, file))
    })?)
}

#[cfg(test)]
mod tests {
    use super::*;

    const FNT: &str = r#"info face="Tiny" size=-8 bold=0 italic=0 charset="" unicode=1 padding=0,0,0,0 spacing=1,1
common lineHeight=10 base=8 scaleW=32 scaleH=16 pages=1 packed=0
page id=0 file="tiny_0.png"
chars count=2
char id=65   x=0  y=0 width=5 height=7 xoffset=0 yoffset=1 xadvance=6 page=0 chnl=15
char id=66   x=6  y=0 width=5 height=7 xoffset=0 yoffset=1 xadvance=6 page=0 chnl=15
kernings count=1
kerning first=65 second=66 amount=-1
"#;

    #[test]
    fn bmfont_text_format_is_read() {
        let mut pages = Vec::new();
        let font = BitmapFont::parse(FNT, |file| {
            pages.push(file.to_owned());
            Handle::default()
        })
        .unwrap();
        assert_eq!(pages, ["tiny_0.png"]);
        assert_eq!(
            (font.size(), font.line_height(), font.base),
            (8.0, 10.0, 8.0)
        );
        let b = font.glyph('B').unwrap();
        assert_eq!(b.source, Rect::new(6.0, 0.0, 11.0, 7.0));
        assert_eq!((b.offset, b.advance, b.page), (Vec2::new(0.0, 1.0), 6.0, 0));
        assert_eq!(font.kerning[&('A', 'B')], -1.0);
        assert!(font.glyph('C').is_none());
    }

    #[test]
    fn bad_bmfont_files_say_where() {
        let err = BitmapFont::parse("common lineHeight=x base=1", |_| Handle::default());
        assert!(
            matches!(err, Err(FontError::Syntax { line: 1, .. })),
            "{err:?}"
        );
        let err = BitmapFont::parse("info size=8", |_| Handle::default()).unwrap_err();
        assert!(err.to_string().contains("lineHeight"), "{err}");
        let err = BitmapFont::parse(
            "common lineHeight=8 base=6\nchar id=65 x=0 y=0 width=1 height=1 xoffset=0 yoffset=0 xadvance=1 page=2",
            |_| Handle::default(),
        )
        .unwrap_err();
        assert!(err.to_string().contains("page 2"), "{err}");
    }

    #[test]
    fn the_built_in_font_reads_its_name() {
        let font = Font::default_font();
        let FontKind::Outline(outline) = &font.kind else {
            panic!("not an outline font");
        };
        assert_eq!(outline.family(), "Pixelify Sans");
        assert!(font.settings.pixel);
        assert_eq!(OutlineFont::new(b"nope".to_vec()), Err(FontError::NotAFont));
    }

    #[test]
    fn font_files_read_with_defaults() {
        let file: FontFile = ron::from_str(r#"(file: "a.ttf")"#).unwrap();
        assert_eq!((file.pixel, file.line_height), (false, None));
        let file: FontFile =
            ron::from_str(r#"(file: "a.ttf", pixel: true, line_height: Some(1.5))"#).unwrap();
        assert_eq!((file.pixel, file.line_height), (true, Some(1.5)));
    }
}
