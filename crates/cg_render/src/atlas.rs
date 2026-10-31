//! Texture atlases: one image holding many named sprites, packed by `cg_pack atlas`.

use crate::{Image, Sprite};
use bevy_ecs::prelude::*;
use cg_assets::{
    Asset, AssetLoader, AssetServer, Assets, BoxError, BoxedFuture, Handle, LoadContext,
};
use cg_core::Rect;
use cg_core::glam::{UVec2, vec2};
use cg_world::PrefabComponent;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// An `.atlas.ron` file, as `cg_pack atlas` writes it:
///
/// ```ron
/// (
///     image: "characters.png",
///     size: (64, 32),
///     regions: {
///         "player": (x: 0, y: 0, w: 24, h: 24),
///         "walker": (x: 25, y: 0, w: 24, h: 24),
///     },
/// )
/// ```
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct AtlasFile {
    /// The atlas image, relative to the atlas file.
    pub image: String,
    /// The image's width and height in pixels.
    pub size: (u32, u32),
    /// The named regions, in pixels from the image's top-left corner.
    pub regions: BTreeMap<String, AtlasRegion>,
}

/// A rectangle of an atlas image, in pixels from its top-left corner.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
pub struct AtlasRegion {
    /// Left edge.
    pub x: u32,
    /// Top edge.
    pub y: u32,
    /// Width.
    pub w: u32,
    /// Height.
    pub h: u32,
}

impl AtlasRegion {
    /// The region as a [`Rect`] in texels, ready for [`Sprite::source`].
    pub fn rect(self) -> Rect {
        Rect::from_min_size(
            vec2(self.x as f32, self.y as f32),
            vec2(self.w as f32, self.h as f32),
        )
    }
}

/// A loaded texture atlas: an image and its named regions. Load `.atlas.ron` files with
/// the asset server, then draw regions with [`AtlasSprite`] or [`sprite`](Self::sprite).
#[derive(Debug, Clone)]
pub struct TextureAtlas {
    /// The atlas image.
    pub image: Handle<Image>,
    size: UVec2,
    regions: BTreeMap<String, Rect>,
}

impl Asset for TextureAtlas {}

impl TextureAtlas {
    /// An atlas over `image` with regions in texels.
    pub fn new(image: Handle<Image>, size: UVec2, regions: BTreeMap<String, Rect>) -> Self {
        Self {
            image,
            size,
            regions,
        }
    }

    /// The region called `name`, in texels from the image's top-left corner.
    pub fn region(&self, name: &str) -> Option<Rect> {
        self.regions.get(name).copied()
    }

    /// A sprite showing the region called `name`, one texel per world unit.
    pub fn sprite(&self, name: &str) -> Option<Sprite> {
        Some(Sprite {
            source: Some(self.region(name)?),
            ..Sprite::new(self.image.clone())
        })
    }

    /// The region names, sorted.
    pub fn names(&self) -> impl Iterator<Item = &str> {
        self.regions.keys().map(String::as_str)
    }

    /// The number of regions.
    pub fn len(&self) -> usize {
        self.regions.len()
    }

    /// Whether the atlas has no regions.
    pub fn is_empty(&self) -> bool {
        self.regions.is_empty()
    }

    /// The image size in pixels, as the atlas file gives it.
    pub fn size(&self) -> UVec2 {
        self.size
    }
}

/// Loads `.atlas.ron` files as [`TextureAtlas`]es; the image loads as a dependency.
/// `RenderPlugin` registers it.
#[derive(Debug, Default)]
pub struct AtlasLoader;

impl AssetLoader for AtlasLoader {
    type Asset = TextureAtlas;

    fn extensions(&self) -> &[&str] {
        &["atlas.ron"]
    }

    fn load<'a>(
        &'a self,
        ctx: &'a mut LoadContext,
    ) -> BoxedFuture<'a, Result<TextureAtlas, BoxError>> {
        Box::pin(async move {
            let file: AtlasFile = ron::from_str(core::str::from_utf8(ctx.bytes())?)?;
            let image = ctx.load(ctx.resolve(&file.image));
            let regions = file
                .regions
                .into_iter()
                .map(|(name, region)| (name, region.rect()))
                .collect();
            Ok(TextureAtlas::new(
                image,
                UVec2::new(file.size.0, file.size.1),
                regions,
            ))
        })
    }
}

/// Draws the entity's [`Sprite`] from a named region of a texture atlas. Once the atlas
/// has loaded, and whenever it or this component changes, the sprite's texture and source
/// are set from the region; its other fields (pivot, colour, flips) stay as they are.
/// Adds a `Sprite` if the entity has none.
///
/// ```
/// use cg_assets::AssetServer;
/// use cg_core::prelude::*;
/// use cg_render::AtlasSprite;
///
/// fn spawn_coin(mut commands: Commands, server: Res<AssetServer>) {
///     commands.spawn(AtlasSprite::new(server.load("atlases/items.atlas.ron"), "coin"));
/// }
/// ```
///
/// In prefabs: `"AtlasSprite": (atlas: "atlases/items.atlas.ron", region: "coin")`.
#[derive(Component, Debug, Clone)]
#[require(Sprite)]
pub struct AtlasSprite {
    /// The atlas.
    pub atlas: Handle<TextureAtlas>,
    /// The region's name. Change it to show another region.
    pub region: String,
    /// The atlas version last applied to the sprite.
    applied: Option<u64>,
}

impl AtlasSprite {
    /// The region `region` of `atlas`.
    pub fn new(atlas: Handle<TextureAtlas>, region: impl Into<String>) -> Self {
        Self {
            atlas,
            region: region.into(),
            applied: None,
        }
    }
}

impl PartialEq for AtlasSprite {
    fn eq(&self, other: &Self) -> bool {
        self.atlas == other.atlas && self.region == other.region
    }
}

/// The prefab form of an [`AtlasSprite`]: the atlas by its asset path and the region name.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct AtlasSpriteData {
    /// The atlas's asset path, such as `atlases/items.atlas.ron`.
    pub atlas: String,
    /// The region's name.
    pub region: String,
}

impl PrefabComponent for AtlasSprite {
    type Data = AtlasSpriteData;

    fn from_data(data: &AtlasSpriteData, world: &World) -> Result<Self, String> {
        let server = world
            .get_resource::<AssetServer>()
            .ok_or_else(|| format!("no asset server to load `{}`", data.atlas))?;
        Ok(Self::new(server.load(data.atlas.as_str()), &data.region))
    }
}

/// Points each [`AtlasSprite`]'s sprite at its region when the atlas loads or changes, or
/// the component changes.
#[profiling::function]
pub(crate) fn apply_atlas_sprites(
    mut sprites: Query<(Entity, &mut AtlasSprite, &mut Sprite)>,
    atlases: Res<Assets<TextureAtlas>>,
) {
    for (entity, mut atlas_sprite, mut sprite) in &mut sprites {
        let Some(version) = atlases.version(&atlas_sprite.atlas) else {
            continue;
        };
        if atlas_sprite.applied == Some(version) && !atlas_sprite.is_changed() {
            continue;
        }
        atlas_sprite.bypass_change_detection().applied = Some(version);
        let Some(atlas) = atlases.get(&atlas_sprite.atlas) else {
            continue;
        };
        match atlas.region(&atlas_sprite.region) {
            Some(region) => {
                sprite.texture = atlas.image.clone();
                sprite.source = Some(region);
            }
            None => log::warn!(
                "entity {entity}: the atlas has no region `{}`",
                atlas_sprite.region
            ),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn atlas_files_round_trip_through_ron() {
        let file: AtlasFile = ron::from_str(
            r#"(
                image: "characters.png",
                size: (64, 32),
                regions: {
                    "player": (x: 0, y: 0, w: 24, h: 24),
                    "walker": (x: 25, y: 0, w: 24, h: 24),
                },
            )"#,
        )
        .unwrap();
        assert_eq!(
            file.regions["walker"].rect(),
            Rect::new(25.0, 0.0, 49.0, 24.0)
        );
        let text = ron::ser::to_string_pretty(&file, ron::ser::PrettyConfig::new()).unwrap();
        assert_eq!(ron::from_str::<AtlasFile>(&text).unwrap(), file);
    }

    #[test]
    fn atlas_sprites_follow_their_atlas() {
        let mut world = World::new();
        let mut images = Assets::<Image>::default();
        let image = images.add(Image::filled(64, 32, crate::Color::RED));
        let mut atlases = Assets::<TextureAtlas>::default();
        let regions = BTreeMap::from([
            ("a".to_owned(), Rect::new(0.0, 0.0, 8.0, 8.0)),
            ("b".to_owned(), Rect::new(8.0, 0.0, 24.0, 16.0)),
        ]);
        let atlas = atlases.add(TextureAtlas::new(
            image.clone(),
            UVec2::new(64, 32),
            regions,
        ));
        assert_eq!(
            atlases.get(&atlas).unwrap().sprite("b").unwrap().source,
            Some(Rect::new(8.0, 0.0, 24.0, 16.0))
        );
        world.insert_resource(atlases);
        let entity = world.spawn(AtlasSprite::new(atlas, "a")).id();
        let mut schedule = Schedule::default();
        schedule.add_systems(apply_atlas_sprites);

        schedule.run(&mut world);
        let sprite = world.get::<Sprite>(entity).unwrap();
        assert_eq!(sprite.texture, image);
        assert_eq!(sprite.source, Some(Rect::new(0.0, 0.0, 8.0, 8.0)));

        world.get_mut::<AtlasSprite>(entity).unwrap().region = "b".into();
        schedule.run(&mut world);
        let sprite = world.get::<Sprite>(entity).unwrap();
        assert_eq!(sprite.source, Some(Rect::new(8.0, 0.0, 24.0, 16.0)));
    }
}
