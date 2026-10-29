//! The [`Color`] type.

use core::fmt;
use serde::de::{self, MapAccess, SeqAccess, Visitor};
use serde::ser::SerializeStruct;
use serde::{Deserialize, Deserializer, Serialize, Serializer};

/// An sRGB colour with straight (not premultiplied) alpha, each channel in `0.0..=1.0`.
///
/// Colours are written the way image editors such as Aseprite show them. The renderer
/// converts them to linear light before blending.
///
/// ```
/// use cg_render::Color;
///
/// let grass = Color::hex(0x5ac54f);
/// assert_eq!(grass, Color::rgb8(0x5a, 0xc5, 0x4f));
/// let ghost = Color::WHITE.with_alpha(0.5);
/// ```
///
/// In RON (prefabs) a colour is a hex string, `"#5ac54f"` or `"#ffffff80"` with alpha, or
/// its channels: `(r: 1, g: 0.5, b: 0)` with an optional `a`, or `(1, 0.5, 0, 1)`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Color {
    /// Red.
    pub r: f32,
    /// Green.
    pub g: f32,
    /// Blue.
    pub b: f32,
    /// Alpha: 0 is fully transparent, 1 fully opaque.
    pub a: f32,
}

impl Default for Color {
    /// White, which leaves a sprite's texture unchanged when used as a tint.
    fn default() -> Self {
        Self::WHITE
    }
}

impl Color {
    /// Opaque white.
    pub const WHITE: Self = Self::rgb(1.0, 1.0, 1.0);
    /// Opaque black.
    pub const BLACK: Self = Self::rgb(0.0, 0.0, 0.0);
    /// Fully transparent black.
    pub const TRANSPARENT: Self = Self::rgba(0.0, 0.0, 0.0, 0.0);
    /// Opaque red.
    pub const RED: Self = Self::rgb(1.0, 0.0, 0.0);
    /// Opaque green.
    pub const GREEN: Self = Self::rgb(0.0, 1.0, 0.0);
    /// Opaque blue.
    pub const BLUE: Self = Self::rgb(0.0, 0.0, 1.0);
    /// Opaque yellow.
    pub const YELLOW: Self = Self::rgb(1.0, 1.0, 0.0);
    /// Opaque cyan.
    pub const CYAN: Self = Self::rgb(0.0, 1.0, 1.0);
    /// Opaque magenta.
    pub const MAGENTA: Self = Self::rgb(1.0, 0.0, 1.0);
    /// Opaque mid grey.
    pub const GRAY: Self = Self::rgb(0.5, 0.5, 0.5);

    /// An opaque colour from sRGB channels in `0.0..=1.0`.
    pub const fn rgb(r: f32, g: f32, b: f32) -> Self {
        Self { r, g, b, a: 1.0 }
    }

    /// A colour from sRGB channels and alpha in `0.0..=1.0`.
    pub const fn rgba(r: f32, g: f32, b: f32, a: f32) -> Self {
        Self { r, g, b, a }
    }

    /// An opaque colour from 8-bit sRGB channels.
    pub const fn rgb8(r: u8, g: u8, b: u8) -> Self {
        Self::rgba8(r, g, b, 255)
    }

    /// A colour from 8-bit sRGB channels and alpha.
    pub const fn rgba8(r: u8, g: u8, b: u8, a: u8) -> Self {
        Self::rgba(
            r as f32 / 255.0,
            g as f32 / 255.0,
            b as f32 / 255.0,
            a as f32 / 255.0,
        )
    }

    /// An opaque colour from a `0xRRGGBB` hex code.
    pub const fn hex(rgb: u32) -> Self {
        Self::rgb8((rgb >> 16) as u8, (rgb >> 8) as u8, rgb as u8)
    }

    /// Parses `#rrggbb` or `#rrggbbaa`; the `#` is optional.
    pub fn from_hex_str(text: &str) -> Option<Self> {
        let digits = text.strip_prefix('#').unwrap_or(text);
        if !matches!(digits.len(), 6 | 8) || !digits.is_ascii() {
            return None;
        }
        let byte = |i: usize| u8::from_str_radix(&digits[i..i + 2], 16).ok();
        let alpha = if digits.len() == 8 { byte(6)? } else { 255 };
        Some(Self::rgba8(byte(0)?, byte(2)?, byte(4)?, alpha))
    }

    /// The colour as `#rrggbb`, or `#rrggbbaa` when it is not opaque, rounded to 8 bits.
    pub fn to_hex_string(self) -> String {
        let [r, g, b, a] = self.to_rgba8();
        if a == 255 {
            format!("#{r:02x}{g:02x}{b:02x}")
        } else {
            format!("#{r:02x}{g:02x}{b:02x}{a:02x}")
        }
    }

    /// The same colour with a different alpha.
    pub const fn with_alpha(self, a: f32) -> Self {
        Self { a, ..self }
    }

    /// The channels as 8-bit sRGB, rounded and clamped.
    pub fn to_rgba8(self) -> [u8; 4] {
        [self.r, self.g, self.b, self.a].map(|c| (c.clamp(0.0, 1.0) * 255.0).round() as u8)
    }

    /// The channels as `[r, g, b, a]` in sRGB.
    pub const fn to_array(self) -> [f32; 4] {
        [self.r, self.g, self.b, self.a]
    }

    /// The colour in linear light (alpha unchanged), as the GPU blends it.
    pub fn to_linear(self) -> [f32; 4] {
        [
            srgb_to_linear(self.r),
            srgb_to_linear(self.g),
            srgb_to_linear(self.b),
            self.a,
        ]
    }

    /// Linear interpolation in sRGB space: `t = 0` gives `self`, `t = 1` gives `other`.
    pub fn lerp(self, other: Self, t: f32) -> Self {
        let mix = |a: f32, b: f32| a + (b - a) * t;
        Self::rgba(
            mix(self.r, other.r),
            mix(self.g, other.g),
            mix(self.b, other.b),
            mix(self.a, other.a),
        )
    }
}

impl Serialize for Color {
    /// Writes a hex string when that is exact, and the four channels otherwise.
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let [r, g, b, a] = self.to_rgba8();
        if Self::rgba8(r, g, b, a) == *self {
            return serializer.serialize_str(&self.to_hex_string());
        }
        let mut color = serializer.serialize_struct("Color", 4)?;
        color.serialize_field("r", &self.r)?;
        color.serialize_field("g", &self.g)?;
        color.serialize_field("b", &self.b)?;
        color.serialize_field("a", &self.a)?;
        color.end()
    }
}

impl<'de> Deserialize<'de> for Color {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        deserializer.deserialize_any(ColorVisitor)
    }
}

struct ColorVisitor;

impl<'de> Visitor<'de> for ColorVisitor {
    type Value = Color;

    fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(
            "a colour: \"#rrggbb\", \"#rrggbbaa\", (r: .., g: .., b: .., a: ..) or (r, g, b, a)",
        )
    }

    fn visit_str<E: de::Error>(self, text: &str) -> Result<Color, E> {
        Color::from_hex_str(text)
            .ok_or_else(|| E::custom(format!("`{text}` is not a #rrggbb colour")))
    }

    fn visit_map<A: MapAccess<'de>>(self, mut map: A) -> Result<Color, A::Error> {
        let mut color = Color::WHITE;
        while let Some(key) = map.next_key::<String>()? {
            let channel = match key.as_str() {
                "r" => &mut color.r,
                "g" => &mut color.g,
                "b" => &mut color.b,
                "a" => &mut color.a,
                other => return Err(de::Error::unknown_field(other, &["r", "g", "b", "a"])),
            };
            *channel = map.next_value()?;
        }
        Ok(color)
    }

    fn visit_seq<A: SeqAccess<'de>>(self, mut seq: A) -> Result<Color, A::Error> {
        let mut channel = |i: usize| -> Result<f32, A::Error> {
            seq.next_element()?
                .ok_or_else(|| de::Error::invalid_length(i, &"3 or 4 channels"))
        };
        let (r, g, b) = (channel(0)?, channel(1)?, channel(2)?);
        let a = seq.next_element()?.unwrap_or(1.0);
        Ok(Color::rgba(r, g, b, a))
    }
}

/// The sRGB transfer function, inverted: an sRGB channel to linear light.
pub(crate) fn srgb_to_linear(c: f32) -> f32 {
    if c <= 0.04045 {
        c / 12.92
    } else {
        ((c + 0.055) / 1.055).powf(2.4)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hex_and_bytes_agree() {
        assert_eq!(Color::hex(0xff8000), Color::rgb8(255, 128, 0));
        assert_eq!(Color::hex(0xff8000).to_rgba8(), [255, 128, 0, 255]);
        assert_eq!(
            Color::rgba(2.0, -1.0, 0.5, 1.0).to_rgba8(),
            [255, 0, 128, 255]
        );
    }

    #[test]
    fn linear_conversion_matches_known_values() {
        let [r, g, b, a] = Color::rgba(0.0, 0.5, 1.0, 0.25).to_linear();
        assert_eq!(r, 0.0);
        assert!((g - 0.214).abs() < 1e-3);
        assert!((b - 1.0).abs() < 1e-6);
        assert_eq!(a, 0.25);
    }

    #[test]
    fn hex_strings_parse_and_print() {
        assert_eq!(Color::from_hex_str("#3a86ff"), Some(Color::hex(0x3a86ff)));
        assert_eq!(
            Color::from_hex_str("ffffff80"),
            Some(Color::rgba8(255, 255, 255, 128))
        );
        assert_eq!(Color::from_hex_str("#12345"), None);
        assert_eq!(Color::from_hex_str("#gg0000"), None);
        assert_eq!(Color::hex(0x3a86ff).to_hex_string(), "#3a86ff");
        assert_eq!(Color::WHITE.with_alpha(0.0).to_hex_string(), "#ffffff00");
    }

    #[test]
    fn ron_accepts_hex_and_channels_and_round_trips() {
        let read = |text: &str| ron::from_str::<Color>(text).unwrap();
        assert_eq!(read("\"#3a86ff\""), Color::hex(0x3a86ff));
        assert_eq!(read("(r: 1, g: 0.5, b: 0)"), Color::rgb(1.0, 0.5, 0.0));
        assert_eq!(read("(0, 0, 1, 0.25)"), Color::rgba(0.0, 0.0, 1.0, 0.25));
        for color in [Color::hex(0x5ac54f), Color::rgba(0.1, 0.2, 0.3, 0.4)] {
            assert_eq!(read(&ron::to_string(&color).unwrap()), color);
        }
        assert_eq!(ron::to_string(&Color::RED).unwrap(), "\"#ff0000\"");
    }

    #[test]
    fn default_is_white_and_lerp_ends() {
        assert_eq!(Color::default(), Color::WHITE);
        assert_eq!(Color::BLACK.lerp(Color::WHITE, 0.0), Color::BLACK);
        assert_eq!(Color::BLACK.lerp(Color::WHITE, 1.0), Color::WHITE);
    }
}
