//! Asset bundles: a whole asset folder in one file, so a web page downloads one file
//! (with a progress bar) instead of one request per asset.
//!
//! `cg_pack bundle` writes them; [`AssetIo`](super::AssetIo) reads from one when it has
//! one. The format (version 1), all numbers little-endian:
//!
//! | Bytes | Field |
//! | --- | --- |
//! | 6 | `CGPAK\0` |
//! | 2 | Format version (1) |
//! | 4 | Size of the whole bundle in bytes, so a page can show progress from the first bytes |
//! | 4 | Number of entries |
//! | ... | Entries: path length (2), the path (UTF-8, `/`-separated), method (1: 0 stored, 1 deflate), offset from the start of the bundle (4), stored length (4), unpacked length (4) |
//! | ... | The entries' data, in the same order |
//!
//! Deflate is the raw stream of RFC 1951, without zlib or gzip framing. A bundle holds at
//! most 4 GiB.

use core::fmt;
use std::collections::HashMap;

/// The first bytes of every bundle.
pub const BUNDLE_MAGIC: &[u8; 6] = b"CGPAK\0";

/// The format version this engine reads and writes.
pub const BUNDLE_VERSION: u16 = 1;

/// Bytes before the entries: magic, version, total size and entry count.
const HEADER: usize = 16;

/// Entries are compressed only when that saves at least 1/20 of their size; images and
/// sounds, which are compressed already, are stored as they are.
const MIN_SAVING: usize = 20;

/// How an entry's bytes are stored.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Method {
    Stored,
    Deflate,
}

#[derive(Debug, Clone, Copy)]
struct Entry {
    method: Method,
    offset: usize,
    stored: usize,
    size: usize,
}

/// Why a bundle could not be read.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BundleError {
    /// The bytes do not start with [`BUNDLE_MAGIC`].
    NotABundle,
    /// The bundle was written in a format version this engine does not know.
    Version(u16),
    /// The bundle is cut short or its index points outside it.
    Corrupt(String),
}

impl fmt::Display for BundleError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NotABundle => f.write_str("not an asset bundle"),
            Self::Version(version) => write!(
                f,
                "asset bundle format {version} is not supported (this engine reads {BUNDLE_VERSION})"
            ),
            Self::Corrupt(why) => write!(f, "corrupt asset bundle: {why}"),
        }
    }
}

impl std::error::Error for BundleError {}

/// The files of an asset folder packed into one file, read into memory.
///
/// ```
/// use cg_platform::io::Bundle;
///
/// let bytes = Bundle::write(vec![("maps/a.txt".to_owned(), b"hello".to_vec())]);
/// let bundle = Bundle::parse(bytes).unwrap();
/// assert_eq!(bundle.read("maps/a.txt"), Some(Ok(b"hello".to_vec())));
/// assert_eq!(bundle.read("maps/b.txt"), None);
/// ```
pub struct Bundle {
    data: Vec<u8>,
    entries: HashMap<String, Entry>,
}

impl fmt::Debug for Bundle {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Bundle")
            .field("files", &self.entries.len())
            .field("bytes", &self.data.len())
            .finish()
    }
}

impl Bundle {
    /// Reads a bundle's index. The files stay packed until [`read`](Self::read).
    pub fn parse(data: Vec<u8>) -> Result<Self, BundleError> {
        let corrupt = |why: &str| BundleError::Corrupt(why.to_owned());
        if data.len() < BUNDLE_MAGIC.len() || &data[..BUNDLE_MAGIC.len()] != BUNDLE_MAGIC {
            return Err(BundleError::NotABundle);
        }
        let mut reader = Reader {
            data: &data,
            at: BUNDLE_MAGIC.len(),
        };
        let version = reader.u16().ok_or_else(|| corrupt("no header"))?;
        if version != BUNDLE_VERSION {
            return Err(BundleError::Version(version));
        }
        let total = reader.u32().ok_or_else(|| corrupt("no header"))? as usize;
        if total != data.len() {
            return Err(BundleError::Corrupt(format!(
                "{} bytes instead of {total}",
                data.len()
            )));
        }
        let count = reader.u32().ok_or_else(|| corrupt("no header"))? as usize;
        let mut entries = HashMap::with_capacity(count.min(data.len() / 16));
        for _ in 0..count {
            let entry = (|| {
                let length = reader.u16()? as usize;
                let path = reader.bytes(length)?;
                let method = match reader.u8()? {
                    0 => Method::Stored,
                    1 => Method::Deflate,
                    _ => return None,
                };
                let offset = reader.u32()? as usize;
                let stored = reader.u32()? as usize;
                let size = reader.u32()? as usize;
                let entry = Entry {
                    method,
                    offset,
                    stored,
                    size,
                };
                Some((path, entry))
            })();
            let (path, entry) = entry.ok_or_else(|| corrupt("the index is cut short"))?;
            let path = core::str::from_utf8(path)
                .map_err(|_| corrupt("a path is not UTF-8"))?
                .to_owned();
            let end = entry.offset.checked_add(entry.stored);
            if entry.offset < HEADER || end.is_none_or(|end| end > data.len()) {
                return Err(BundleError::Corrupt(format!("`{path}` lies outside it")));
            }
            if entry.method == Method::Stored && entry.stored != entry.size {
                return Err(BundleError::Corrupt(format!("`{path}` has the wrong size")));
            }
            entries.insert(path, entry);
        }
        Ok(Self { data, entries })
    }

    /// The file at `path` (`/`-separated, as asset paths are), unpacked; `None` if the
    /// bundle does not have it.
    pub fn read(&self, path: &str) -> Option<Result<Vec<u8>, String>> {
        let entry = self.entries.get(path)?;
        let bytes = &self.data[entry.offset..entry.offset + entry.stored];
        Some(match entry.method {
            Method::Stored => Ok(bytes.to_vec()),
            Method::Deflate => {
                miniz_oxide::inflate::decompress_to_vec_with_limit(bytes, entry.size)
                    .map_err(|err| format!("cannot unpack it from the bundle: {err}"))
                    .and_then(|unpacked| {
                        if unpacked.len() == entry.size {
                            Ok(unpacked)
                        } else {
                            Err("it unpacks to the wrong size".to_owned())
                        }
                    })
            }
        })
    }

    /// Whether the bundle has a file at `path`.
    pub fn contains(&self, path: &str) -> bool {
        self.entries.contains_key(path)
    }

    /// The paths of every file in the bundle, in no particular order.
    pub fn paths(&self) -> impl Iterator<Item = &str> {
        self.entries.keys().map(String::as_str)
    }

    /// The number of files in the bundle.
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// Whether the bundle has no files.
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// The size of the whole bundle in bytes, as stored.
    pub fn size(&self) -> usize {
        self.data.len()
    }

    /// Packs files (asset path, contents) into a bundle. Files are sorted by path, so the
    /// same files always give the same bytes. Each file is deflated when that makes it at
    /// least 5% smaller, and stored as it is otherwise.
    ///
    /// # Panics
    ///
    /// If two files have the same path, a path is longer than 65,535 bytes, or the bundle
    /// would be larger than 4 GiB.
    pub fn write(mut files: Vec<(String, Vec<u8>)>) -> Vec<u8> {
        files.sort_by(|a, b| a.0.cmp(&b.0));
        for pair in files.windows(2) {
            assert!(
                pair[0].0 != pair[1].0,
                "`{}` is in the bundle twice",
                pair[0].0
            );
        }
        let packed: Vec<(Method, Vec<u8>)> = files
            .iter()
            .map(|(_, bytes)| {
                let deflated = miniz_oxide::deflate::compress_to_vec(bytes, 9);
                if deflated.len() + bytes.len() / MIN_SAVING <= bytes.len() {
                    (Method::Deflate, deflated)
                } else {
                    (Method::Stored, bytes.clone())
                }
            })
            .collect();
        let index: usize = files.iter().map(|(path, _)| 2 + path.len() + 13).sum();
        let total = HEADER + index + packed.iter().map(|(_, b)| b.len()).sum::<usize>();
        let number = |n: usize| u32::try_from(n).expect("asset bundles hold at most 4 GiB");

        let mut out = Vec::with_capacity(total);
        out.extend_from_slice(BUNDLE_MAGIC);
        out.extend_from_slice(&BUNDLE_VERSION.to_le_bytes());
        out.extend_from_slice(&number(total).to_le_bytes());
        out.extend_from_slice(&number(files.len()).to_le_bytes());
        let mut offset = HEADER + index;
        for ((path, bytes), (method, stored)) in files.iter().zip(&packed) {
            let length = u16::try_from(path.len()).expect("asset paths are under 64 KiB");
            out.extend_from_slice(&length.to_le_bytes());
            out.extend_from_slice(path.as_bytes());
            out.push(match method {
                Method::Stored => 0,
                Method::Deflate => 1,
            });
            out.extend_from_slice(&number(offset).to_le_bytes());
            out.extend_from_slice(&number(stored.len()).to_le_bytes());
            out.extend_from_slice(&number(bytes.len()).to_le_bytes());
            offset += stored.len();
        }
        for (_, stored) in &packed {
            out.extend_from_slice(stored);
        }
        debug_assert_eq!(out.len(), total);
        out
    }
}

/// Reads little-endian numbers and byte runs, failing past the end.
struct Reader<'a> {
    data: &'a [u8],
    at: usize,
}

impl<'a> Reader<'a> {
    fn bytes(&mut self, length: usize) -> Option<&'a [u8]> {
        let bytes = self.data.get(self.at..self.at.checked_add(length)?)?;
        self.at += length;
        Some(bytes)
    }

    fn u8(&mut self) -> Option<u8> {
        Some(self.bytes(1)?[0])
    }

    fn u16(&mut self) -> Option<u16> {
        Some(u16::from_le_bytes(self.bytes(2)?.try_into().ok()?))
    }

    fn u32(&mut self) -> Option<u32> {
        Some(u32::from_le_bytes(self.bytes(4)?.try_into().ok()?))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample() -> Vec<(String, Vec<u8>)> {
        let text = "<map>".repeat(200).into_bytes();
        // Bytes that do not compress, like a PNG or an OGG file.
        let mut noise = Vec::new();
        let mut x = 12345u32;
        for _ in 0..2000 {
            x = x.wrapping_mul(1_103_515_245).wrapping_add(12345);
            noise.push((x >> 16) as u8);
        }
        vec![
            ("sounds/noise.ogg".to_owned(), noise),
            ("maps/level.tmx".to_owned(), text),
            ("empty.txt".to_owned(), Vec::new()),
        ]
    }

    #[test]
    fn files_round_trip_and_only_text_is_compressed() {
        let files = sample();
        let bytes = Bundle::write(files.clone());
        let bundle = Bundle::parse(bytes.clone()).unwrap();
        assert_eq!(bundle.len(), 3);
        for (path, contents) in &files {
            assert_eq!(bundle.read(path), Some(Ok(contents.clone())), "{path}");
        }
        assert_eq!(bundle.read("missing.png"), None);
        assert_eq!(bundle.entries["maps/level.tmx"].method, Method::Deflate);
        assert!(bundle.entries["maps/level.tmx"].stored < 100);
        assert_eq!(bundle.entries["sounds/noise.ogg"].method, Method::Stored);
        assert_eq!(bundle.entries["empty.txt"].method, Method::Stored);
        // The header gives the size, so a page can show progress from the first bytes.
        assert_eq!(&bytes[..6], BUNDLE_MAGIC);
        assert_eq!(
            u32::from_le_bytes(bytes[8..12].try_into().unwrap()) as usize,
            bytes.len()
        );
        assert_eq!(bundle.size(), bytes.len());
    }

    #[test]
    fn the_same_files_always_give_the_same_bytes() {
        let mut reversed = sample();
        reversed.reverse();
        assert_eq!(Bundle::write(sample()), Bundle::write(reversed));
    }

    #[test]
    fn bad_bundles_are_errors() {
        let bytes = Bundle::write(sample());
        assert_eq!(
            Bundle::parse(b"<!doctype html>".to_vec()).unwrap_err(),
            BundleError::NotABundle
        );
        let mut newer = bytes.clone();
        newer[6] = 2;
        assert_eq!(Bundle::parse(newer).unwrap_err(), BundleError::Version(2));
        for cut in [10, 20, bytes.len() - 1] {
            assert!(
                matches!(
                    Bundle::parse(bytes[..cut].to_vec()),
                    Err(BundleError::Corrupt(_))
                ),
                "cut at {cut}"
            );
        }
        // A damaged deflate stream is an error when the file is read, not a panic.
        let bundle = Bundle::parse(bytes.clone()).unwrap();
        let entry = bundle.entries["maps/level.tmx"];
        let mut damaged = bytes;
        for byte in &mut damaged[entry.offset..entry.offset + entry.stored] {
            *byte = !*byte;
        }
        let bundle = Bundle::parse(damaged).unwrap();
        assert!(matches!(bundle.read("maps/level.tmx"), Some(Err(_))));
    }
}
