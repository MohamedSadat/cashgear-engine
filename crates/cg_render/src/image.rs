//! The [`Image`] type: pixels in memory.

use crate::Color;
use cg_core::glam::UVec2;
use core::fmt;

/// Pixels in memory: 8-bit sRGB RGBA with straight alpha, rows from top to bottom.
///
/// Add it to [`Textures`](crate::Textures) to draw it.
///
/// ```
/// use cg_render::{Color, Image};
///
/// let checker = Image::from_fn(4, 4, |x, y| {
///     if (x + y) % 2 == 0 { [255, 255, 255, 255] } else { [0, 0, 0, 255] }
/// });
/// assert_eq!(checker.pixel(1, 0), Some([0, 0, 0, 255]));
/// let red = Image::filled(2, 2, Color::RED);
/// assert_eq!(red.data().len(), 2 * 2 * 4);
/// ```
#[derive(Clone, PartialEq, Eq)]
pub struct Image {
    width: u32,
    height: u32,
    data: Vec<u8>,
}

impl fmt::Debug for Image {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Image")
            .field("width", &self.width)
            .field("height", &self.height)
            .finish_non_exhaustive()
    }
}

impl Image {
    /// An image from raw RGBA8 bytes, `width * height * 4` of them.
    pub fn new(width: u32, height: u32, data: Vec<u8>) -> Result<Self, ImageError> {
        if width == 0 || height == 0 {
            return Err(ImageError::Empty);
        }
        let expected = width as usize * height as usize * 4;
        if data.len() != expected {
            return Err(ImageError::Size {
                expected,
                actual: data.len(),
            });
        }
        Ok(Self {
            width,
            height,
            data,
        })
    }

    /// An image filled with one colour.
    ///
    /// # Panics
    ///
    /// Panics if `width` or `height` is zero.
    pub fn filled(width: u32, height: u32, color: Color) -> Self {
        let pixel = color.to_rgba8();
        Self::from_fn(width, height, |_, _| pixel)
    }

    /// An image whose pixel at `(x, y)` (from the top-left) is `pixel(x, y)`.
    ///
    /// # Panics
    ///
    /// Panics if `width` or `height` is zero.
    pub fn from_fn(width: u32, height: u32, mut pixel: impl FnMut(u32, u32) -> [u8; 4]) -> Self {
        assert!(width > 0 && height > 0, "images cannot be empty");
        let mut data = Vec::with_capacity(width as usize * height as usize * 4);
        for y in 0..height {
            for x in 0..width {
                data.extend_from_slice(&pixel(x, y));
            }
        }
        Self {
            width,
            height,
            data,
        }
    }

    /// Decodes a PNG file. Every PNG colour type is converted to RGBA8.
    pub fn from_png(bytes: &[u8]) -> Result<Self, ImageError> {
        let decoded = image::load_from_memory_with_format(bytes, image::ImageFormat::Png)
            .map_err(|err| ImageError::Decode(err.to_string()))?
            .into_rgba8();
        let (width, height) = decoded.dimensions();
        Self::new(width, height, decoded.into_raw())
    }

    /// Encodes the image as a PNG file.
    pub fn to_png(&self) -> Result<Vec<u8>, ImageError> {
        use image::ImageEncoder;
        let mut bytes = Vec::new();
        image::codecs::png::PngEncoder::new(&mut bytes)
            .write_image(
                &self.data,
                self.width,
                self.height,
                image::ExtendedColorType::Rgba8,
            )
            .map_err(|err| ImageError::Encode(err.to_string()))?;
        Ok(bytes)
    }

    /// Width in pixels.
    pub fn width(&self) -> u32 {
        self.width
    }

    /// Height in pixels.
    pub fn height(&self) -> u32 {
        self.height
    }

    /// Width and height in pixels.
    pub fn size(&self) -> UVec2 {
        UVec2::new(self.width, self.height)
    }

    /// The raw RGBA8 bytes, row by row from the top.
    pub fn data(&self) -> &[u8] {
        &self.data
    }

    /// The pixel at `(x, y)` from the top-left, if inside the image.
    pub fn pixel(&self, x: u32, y: u32) -> Option<[u8; 4]> {
        if x >= self.width || y >= self.height {
            return None;
        }
        let i = (y as usize * self.width as usize + x as usize) * 4;
        self.data[i..i + 4].try_into().ok()
    }
}

/// Why an [`Image`] could not be created, decoded or encoded.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ImageError {
    /// The image has no pixels.
    Empty,
    /// The byte count does not match `width * height * 4`.
    Size {
        /// Bytes needed.
        expected: usize,
        /// Bytes given.
        actual: usize,
    },
    /// The file could not be decoded.
    Decode(String),
    /// The image could not be encoded.
    Encode(String),
}

impl fmt::Display for ImageError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Empty => write!(f, "the image has no pixels"),
            Self::Size { expected, actual } => {
                write!(f, "expected {expected} bytes of RGBA8 data, got {actual}")
            }
            Self::Decode(err) => write!(f, "cannot decode the image: {err}"),
            Self::Encode(err) => write!(f, "cannot encode the image: {err}"),
        }
    }
}

impl std::error::Error for ImageError {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_checks_sizes() {
        assert_eq!(Image::new(0, 1, vec![]), Err(ImageError::Empty));
        assert_eq!(
            Image::new(2, 1, vec![0; 4]),
            Err(ImageError::Size {
                expected: 8,
                actual: 4
            })
        );
        assert!(Image::new(1, 1, vec![1, 2, 3, 4]).is_ok());
    }

    #[test]
    fn png_round_trip() {
        let image = Image::from_fn(3, 2, |x, y| [x as u8 * 80, y as u8 * 200, 7, 255 - x as u8]);
        let png = image.to_png().unwrap();
        assert_eq!(Image::from_png(&png).unwrap(), image);
    }

    #[test]
    fn bad_png_is_an_error() {
        assert!(matches!(
            Image::from_png(b"not a png"),
            Err(ImageError::Decode(_))
        ));
    }

    #[test]
    fn pixels_are_row_major() {
        let image = Image::from_fn(2, 2, |x, y| [x as u8, y as u8, 0, 0]);
        assert_eq!(image.pixel(1, 0), Some([1, 0, 0, 0]));
        assert_eq!(image.pixel(0, 1), Some([0, 1, 0, 0]));
        assert_eq!(image.pixel(2, 0), None);
    }
}
