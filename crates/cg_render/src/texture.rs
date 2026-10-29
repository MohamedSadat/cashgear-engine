//! The [`Textures`] store and [`TextureId`] handles.

use crate::{Color, Image};
use bevy_ecs::resource::Resource;
use cg_core::glam::UVec2;
use std::collections::HashMap;

/// Identifies a texture in [`Textures`]. Ids are never reused, so a removed texture's id
/// draws nothing rather than some other texture.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct TextureId(u32);

impl TextureId {
    /// A 1×1 white texture that is always present. Tint it to draw solid rectangles.
    pub const WHITE: Self = Self(0);

    /// The id as a number, for debugging.
    pub fn index(self) -> u32 {
        self.0
    }
}

/// How a texture is sampled when it is drawn larger or smaller than its size.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Filter {
    /// The nearest texel: crisp pixel art. The default.
    #[default]
    Nearest,
    /// A blend of the four nearest texels: smooth art. Mipmaps are not generated.
    Linear,
}

pub(crate) struct Entry {
    pub(crate) image: Image,
    pub(crate) filter: Filter,
    /// Changes whenever the image or filter changes, so the GPU copy knows to update.
    pub(crate) version: u64,
}

/// The textures the renderer can draw, kept in CPU memory and uploaded to the GPU when it
/// is ready.
///
/// Textures can be added at any time, including in [`Startup`](cg_core::Startup) before the
/// GPU exists (on the web it appears a few frames later). Phase 4's asset server will load
/// them from files; until then, embed PNGs with `include_bytes!`:
///
/// ```
/// use cg_render::{Image, Textures};
///
/// let mut textures = Textures::default();
/// let id = textures.add(Image::from_fn(16, 16, |x, y| [x as u8 * 16, y as u8 * 16, 0, 255]));
/// assert_eq!(textures.size(id).unwrap().x, 16);
/// // let bunny = textures.add(Image::from_png(include_bytes!("bunny.png"))?);
/// ```
///
/// Textures can also have names, which is how prefabs refer to them until Phase 4 gives
/// them asset paths. [`TextureId::WHITE`] is called `"white"`.
#[derive(Resource)]
pub struct Textures {
    entries: Vec<Option<Entry>>,
    names: HashMap<String, TextureId>,
    next_version: u64,
}

impl Default for Textures {
    fn default() -> Self {
        let mut textures = Self {
            entries: Vec::new(),
            names: HashMap::new(),
            next_version: 0,
        };
        textures.add_named("white", Image::filled(1, 1, Color::WHITE));
        textures
    }
}

impl Textures {
    /// Adds an image with [`Filter::Nearest`] sampling.
    pub fn add(&mut self, image: Image) -> TextureId {
        self.add_with_filter(image, Filter::Nearest)
    }

    /// Adds an image with the given sampling filter.
    pub fn add_with_filter(&mut self, image: Image, filter: Filter) -> TextureId {
        let id = TextureId(self.entries.len() as u32);
        let version = self.bump();
        self.entries.push(Some(Entry {
            image,
            filter,
            version,
        }));
        id
    }

    /// Adds an image with [`Filter::Nearest`] sampling under a name. A texture that had the
    /// name before keeps its id but loses the name.
    pub fn add_named(&mut self, name: impl Into<String>, image: Image) -> TextureId {
        let id = self.add(image);
        self.names.insert(name.into(), id);
        id
    }

    /// The texture with this name.
    pub fn named(&self, name: &str) -> Option<TextureId> {
        self.names
            .get(name)
            .copied()
            .filter(|&id| self.entry(id).is_some())
    }

    /// A name of this texture, if it has one.
    pub fn name_of(&self, id: TextureId) -> Option<&str> {
        self.names
            .iter()
            .find(|(_, named)| **named == id)
            .map(|(name, _)| name.as_str())
    }

    /// Replaces a texture's image. Returns `false` if the id is unknown or removed.
    pub fn replace(&mut self, id: TextureId, image: Image) -> bool {
        let version = self.bump();
        match self.entry_mut(id) {
            Some(entry) => {
                entry.image = image;
                entry.version = version;
                true
            }
            None => false,
        }
    }

    /// Changes a texture's sampling filter. Returns `false` if the id is unknown or removed.
    pub fn set_filter(&mut self, id: TextureId, filter: Filter) -> bool {
        let version = self.bump();
        match self.entry_mut(id) {
            Some(entry) => {
                if entry.filter != filter {
                    entry.filter = filter;
                    entry.version = version;
                }
                true
            }
            None => false,
        }
    }

    /// Removes a texture and returns its image. [`TextureId::WHITE`] cannot be removed.
    pub fn remove(&mut self, id: TextureId) -> Option<Image> {
        if id == TextureId::WHITE {
            return None;
        }
        let entry = self.entries.get_mut(id.0 as usize)?.take()?;
        self.names.retain(|_, named| *named != id);
        self.bump();
        Some(entry.image)
    }

    /// A texture's image.
    pub fn get(&self, id: TextureId) -> Option<&Image> {
        self.entry(id).map(|entry| &entry.image)
    }

    /// A texture's size in pixels.
    pub fn size(&self, id: TextureId) -> Option<UVec2> {
        self.entry(id).map(|entry| entry.image.size())
    }

    /// A texture's sampling filter.
    pub fn filter(&self, id: TextureId) -> Option<Filter> {
        self.entry(id).map(|entry| entry.filter)
    }

    /// The number of textures, including [`TextureId::WHITE`].
    pub fn len(&self) -> usize {
        self.entries.iter().flatten().count()
    }

    /// Whether there are no textures. Always `false`: [`TextureId::WHITE`] is always there.
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    pub(crate) fn entry(&self, id: TextureId) -> Option<&Entry> {
        self.entries.get(id.0 as usize)?.as_ref()
    }

    /// Every slot, including removed ones, by id.
    pub(crate) fn slots(&self) -> impl Iterator<Item = (TextureId, Option<&Entry>)> {
        self.entries
            .iter()
            .enumerate()
            .map(|(i, entry)| (TextureId(i as u32), entry.as_ref()))
    }

    /// The latest version handed out; it grows with every change.
    pub(crate) fn revision(&self) -> u64 {
        self.next_version
    }

    fn entry_mut(&mut self, id: TextureId) -> Option<&mut Entry> {
        self.entries.get_mut(id.0 as usize)?.as_mut()
    }

    fn bump(&mut self) -> u64 {
        self.next_version += 1;
        self.next_version
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn white_is_always_there() {
        let mut textures = Textures::default();
        assert_eq!(textures.size(TextureId::WHITE), Some(UVec2::ONE));
        assert_eq!(textures.remove(TextureId::WHITE), None);
        assert_eq!(textures.len(), 1);
    }

    #[test]
    fn names_find_textures_until_removed() {
        let mut textures = Textures::default();
        assert_eq!(textures.named("white"), Some(TextureId::WHITE));
        let bunny = textures.add_named("bunny", Image::filled(2, 2, Color::RED));
        assert_eq!(textures.named("bunny"), Some(bunny));
        assert_eq!(textures.name_of(bunny), Some("bunny"));
        textures.remove(bunny);
        assert_eq!(textures.named("bunny"), None);
    }

    #[test]
    fn ids_are_not_reused() {
        let mut textures = Textures::default();
        let a = textures.add(Image::filled(2, 2, Color::RED));
        textures.remove(a);
        let b = textures.add(Image::filled(3, 3, Color::RED));
        assert_ne!(a, b);
        assert_eq!(textures.get(a), None);
        assert_eq!(textures.size(b), Some(UVec2::splat(3)));
    }

    #[test]
    fn changes_bump_the_revision() {
        let mut textures = Textures::default();
        let id = textures.add(Image::filled(1, 1, Color::RED));
        let before = textures.revision();
        assert!(textures.replace(id, Image::filled(2, 1, Color::BLUE)));
        assert!(textures.revision() > before);
        assert_eq!(textures.entry(id).unwrap().version, textures.revision());
        assert!(textures.set_filter(id, Filter::Linear));
        assert_eq!(textures.filter(id), Some(Filter::Linear));
        assert!(!textures.replace(TextureId(99), Image::filled(1, 1, Color::RED)));
    }
}
