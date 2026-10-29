//! Reading Aseprite's sprite sheet exports (`File > Export Sprite Sheet`, or
//! `aseprite -b --sheet x.png --data x.json`) into [`SpriteSheet`]s.

use crate::sheet::{AnimationClip, ClipEvent, ClipFrame, Direction, SheetFrame, SpriteSheet};
use cg_assets::{AssetLoader, BoxError, BoxedFuture, Handle, LoadContext};
use cg_core::Rect;
use cg_core::glam::vec2;
use cg_render::Image;
use serde::de::{self, MapAccess, SeqAccess, Visitor};
use serde::{Deserialize, Deserializer};
use std::fmt;

/// Loads Aseprite sprite sheet exports (`.json`) as [`SpriteSheet`]s, with the sheet image
/// as a dependency. `AnimPlugin` registers it.
///
/// Both of Aseprite's layouts work: "Hash" (frames keyed by file name, the default) and
/// "Array". From the export it reads:
///
/// - each frame's rectangle and duration, and for trimmed exports where the frame sat in
///   the untrimmed canvas (so pivots stay put);
/// - each tag as a clip named after the tag, with its direction (forward, reverse,
///   ping-pong, ping-pong reverse) and repeat count (a tag repeated once plays once and
///   holds its last frame; no repeat count loops for ever);
/// - events from a tag's user data, written `frame:name` and separated by commas or
///   spaces, with frames counted from the tag's first frame: `0:footstep, 3:footstep`.
///
/// A sheet without tags gets one looping clip, `default`, over all its frames. Rotated
/// frames (which Aseprite never writes) are an error.
///
/// Export with "Tags" ticked in the output options, or `--list-tags` on the command line,
/// so the JSON has `meta.frameTags`.
#[derive(Debug, Default)]
pub struct AsepriteLoader;

impl AssetLoader for AsepriteLoader {
    type Asset = SpriteSheet;

    fn extensions(&self) -> &[&str] {
        &["json"]
    }

    fn load<'a>(
        &'a self,
        ctx: &'a mut LoadContext,
    ) -> BoxedFuture<'a, Result<SpriteSheet, BoxError>> {
        Box::pin(async move {
            let text = core::str::from_utf8(ctx.bytes())?;
            let export = AsepriteExport::parse(text)?;
            let image = ctx.load(ctx.resolve(&export.image));
            let (sheet, warnings) = export.into_sheet(image)?;
            for warning in warnings {
                log::warn!("{}: {warning}", ctx.path());
            }
            Ok(sheet)
        })
    }
}

/// An Aseprite JSON export, parsed but not yet turned into a [`SpriteSheet`].
#[derive(Debug, Clone, PartialEq)]
pub struct AsepriteExport {
    /// The sheet image's path, relative to the JSON file.
    pub image: String,
    frames: Vec<(String, JsonFrame)>,
    tags: Vec<JsonTag>,
}

impl AsepriteExport {
    /// Parses the JSON text of an export.
    pub fn parse(text: &str) -> Result<Self, String> {
        let file: JsonFile = serde_json::from_str(text).map_err(|err| err.to_string())?;
        Ok(Self {
            image: file.meta.image,
            frames: file.frames.0,
            tags: file.meta.frame_tags,
        })
    }

    /// The sheet, drawing from `image`, and warnings about anything skipped (user data
    /// that is not events).
    pub fn into_sheet(self, image: Handle<Image>) -> Result<(SpriteSheet, Vec<String>), String> {
        let mut sheet = SpriteSheet::new(image);
        let mut warnings = Vec::new();
        for (name, frame) in &self.frames {
            if frame.rotated {
                return Err(format!("frame `{name}` is rotated, which is not supported"));
            }
            let r = frame.frame;
            let rect = Rect::new(r.x, r.y, r.x + r.w, r.y + r.h);
            let duration = frame.duration / 1000.0;
            let sheet_frame = match (frame.sprite_source_size, frame.source_size) {
                (Some(placed), Some(source)) => SheetFrame {
                    rect,
                    source_size: vec2(source.w, source.h),
                    offset: vec2(placed.x, placed.y),
                    duration,
                },
                _ => SheetFrame::new(rect, duration),
            };
            sheet.add_frame(name.clone(), sheet_frame);
        }

        let count = sheet.frames.len();
        let clip_over = |from: usize, to: usize| {
            AnimationClip::new((from..=to).map(|frame| ClipFrame {
                frame,
                duration: sheet.frames[frame].duration,
            }))
        };
        let mut clips = Vec::new();
        if self.tags.is_empty() && count > 0 {
            clips.push(("default".to_owned(), clip_over(0, count - 1)));
        }
        for tag in &self.tags {
            if tag.from > tag.to || tag.to >= count {
                return Err(format!(
                    "tag `{}` covers frames {} to {}, but the sheet has {count}",
                    tag.name, tag.from, tag.to
                ));
            }
            let direction = match tag.direction.as_str() {
                "" | "forward" => Direction::Forward,
                "reverse" => Direction::Reverse,
                "pingpong" => Direction::PingPong,
                "pingpong_reverse" => Direction::PingPongReverse,
                other => {
                    return Err(format!(
                        "tag `{}` has an unknown direction `{other}`",
                        tag.name
                    ));
                }
            };
            let mut clip = clip_over(tag.from, tag.to).with_direction(direction);
            clip.repeat = match &tag.repeat {
                None => None,
                Some(repeat) => match repeat.count() {
                    Some(0) => None,
                    Some(times) => Some(times),
                    None => {
                        return Err(format!("tag `{}` has a bad repeat count", tag.name));
                    }
                },
            };
            let (events, skipped) = parse_events(&tag.data);
            for event in &events {
                if event.frame >= clip.frames.len() {
                    return Err(format!(
                        "tag `{}` has an event on frame {}, but only {} frames",
                        tag.name,
                        event.frame,
                        clip.frames.len()
                    ));
                }
            }
            clip.events = events;
            if !skipped.is_empty() {
                warnings.push(format!(
                    "tag `{}`: ignoring user data `{}` (events are written `frame:name`)",
                    tag.name,
                    skipped.join(" ")
                ));
            }
            clips.push((tag.name.clone(), clip));
        }
        for (name, clip) in clips {
            sheet.add_clip(name, clip);
        }
        Ok((sheet, warnings))
    }
}

/// Reads `0:footstep, 3:footstep` into events, returning the words that are not events.
fn parse_events(data: &str) -> (Vec<ClipEvent>, Vec<&str>) {
    let mut events = Vec::new();
    let mut skipped = Vec::new();
    for word in data
        .split(|c: char| c == ',' || c.is_whitespace())
        .filter(|w| !w.is_empty())
    {
        match word.split_once(':') {
            Some((frame, name)) if !name.is_empty() => match frame.parse() {
                Ok(frame) => events.push(ClipEvent {
                    frame,
                    name: name.to_owned(),
                }),
                Err(_) => skipped.push(word),
            },
            _ => skipped.push(word),
        }
    }
    (events, skipped)
}

#[derive(Deserialize)]
struct JsonFile {
    frames: JsonFrames,
    meta: JsonMeta,
}

#[derive(Deserialize)]
struct JsonMeta {
    image: String,
    #[serde(default, rename = "frameTags")]
    frame_tags: Vec<JsonTag>,
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
struct JsonFrame {
    #[serde(default)]
    filename: String,
    frame: JsonRect,
    #[serde(default)]
    rotated: bool,
    #[serde(default, rename = "spriteSourceSize")]
    sprite_source_size: Option<JsonRect>,
    #[serde(default, rename = "sourceSize")]
    source_size: Option<JsonSize>,
    /// Milliseconds.
    #[serde(default = "default_duration")]
    duration: f32,
}

fn default_duration() -> f32 {
    100.0
}

#[derive(Debug, Clone, Copy, PartialEq, Deserialize)]
struct JsonRect {
    x: f32,
    y: f32,
    w: f32,
    h: f32,
}

#[derive(Debug, Clone, Copy, PartialEq, Deserialize)]
struct JsonSize {
    w: f32,
    h: f32,
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
struct JsonTag {
    name: String,
    from: usize,
    to: usize,
    #[serde(default)]
    direction: String,
    /// Aseprite writes it as a string (`"repeat": "2"`); numbers are accepted too.
    #[serde(default)]
    repeat: Option<JsonRepeat>,
    #[serde(default)]
    data: String,
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(untagged)]
enum JsonRepeat {
    Number(u32),
    Text(String),
}

impl JsonRepeat {
    fn count(&self) -> Option<u32> {
        match self {
            Self::Number(n) => Some(*n),
            Self::Text(text) => text.trim().parse().ok(),
        }
    }
}

/// The frames in file order, from either layout: an object keyed by file name ("Hash")
/// or an array whose entries carry `filename` ("Array").
struct JsonFrames(Vec<(String, JsonFrame)>);

impl<'de> Deserialize<'de> for JsonFrames {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        struct FramesVisitor;

        impl<'de> Visitor<'de> for FramesVisitor {
            type Value = JsonFrames;

            fn expecting(&self, f: &mut fmt::Formatter) -> fmt::Result {
                f.write_str("an object or array of frames")
            }

            fn visit_map<A: MapAccess<'de>>(self, mut map: A) -> Result<JsonFrames, A::Error> {
                let mut frames = Vec::new();
                while let Some((name, frame)) = map.next_entry::<String, JsonFrame>()? {
                    frames.push((name, frame));
                }
                Ok(JsonFrames(frames))
            }

            fn visit_seq<A: SeqAccess<'de>>(self, mut seq: A) -> Result<JsonFrames, A::Error> {
                let mut frames = Vec::new();
                while let Some(frame) = seq.next_element::<JsonFrame>()? {
                    if frame.filename.is_empty() {
                        return Err(de::Error::missing_field("filename"));
                    }
                    frames.push((frame.filename.clone(), frame));
                }
                Ok(JsonFrames(frames))
            }
        }

        deserializer.deserialize_any(FramesVisitor)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A hash-layout export as Aseprite 1.3 writes it, trimmed, with tags, repeat counts
    /// and user data. Keys are deliberately not in alphabetical order.
    const HASH: &str = r##"{ "frames": {
       "hero 2.aseprite": {
        "frame": { "x": 40, "y": 0, "w": 18, "h": 20 },
        "rotated": false,
        "trimmed": true,
        "spriteSourceSize": { "x": 3, "y": 4, "w": 18, "h": 20 },
        "sourceSize": { "w": 24, "h": 24 },
        "duration": 120
       },
       "hero 0.aseprite": {
        "frame": { "x": 0, "y": 0, "w": 20, "h": 24 },
        "rotated": false,
        "trimmed": true,
        "spriteSourceSize": { "x": 2, "y": 0, "w": 20, "h": 24 },
        "sourceSize": { "w": 24, "h": 24 },
        "duration": 100
       },
       "hero 1.aseprite": {
        "frame": { "x": 20, "y": 0, "w": 20, "h": 24 },
        "rotated": false,
        "trimmed": true,
        "spriteSourceSize": { "x": 2, "y": 0, "w": 20, "h": 24 },
        "sourceSize": { "w": 24, "h": 24 },
        "duration": 100
       }
     },
     "meta": {
      "app": "https://www.aseprite.org/",
      "version": "1.3.10",
      "image": "hero.png",
      "format": "RGBA8888",
      "size": { "w": 58, "h": 24 },
      "scale": "1",
      "frameTags": [
       { "name": "run", "from": 0, "to": 1, "direction": "forward", "color": "#000000ff", "data": "0:footstep, 1:footstep" },
       { "name": "hit", "from": 2, "to": 2, "direction": "forward", "color": "#000000ff", "repeat": "1" },
       { "name": "wave", "from": 0, "to": 2, "direction": "pingpong", "color": "#000000ff", "repeat": "2", "data": "sparkle" }
      ],
      "layers": [
       { "name": "Layer 1", "opacity": 255, "blendMode": "normal" }
      ],
      "slices": [
      ]
     }
    }
    "##;

    /// The same kind of sheet in the array layout, untrimmed, without tags.
    const ARRAY: &str = r#"{ "frames": [
       { "filename": "coin 0", "frame": { "x": 0, "y": 0, "w": 8, "h": 8 }, "rotated": false, "trimmed": false,
         "spriteSourceSize": { "x": 0, "y": 0, "w": 8, "h": 8 }, "sourceSize": { "w": 8, "h": 8 }, "duration": 50 },
       { "filename": "coin 1", "frame": { "x": 8, "y": 0, "w": 8, "h": 8 }, "rotated": false, "trimmed": false,
         "spriteSourceSize": { "x": 0, "y": 0, "w": 8, "h": 8 }, "sourceSize": { "w": 8, "h": 8 }, "duration": 150 }
     ],
     "meta": { "image": "coin.png", "size": { "w": 16, "h": 8 } }
    }"#;

    fn sheet(text: &str) -> (SpriteSheet, Vec<String>) {
        AsepriteExport::parse(text)
            .unwrap()
            .into_sheet(Handle::default())
            .unwrap()
    }

    #[test]
    fn hash_exports_keep_frame_order_trims_and_tags() {
        let (sheet, warnings) = sheet(HASH);
        assert_eq!(
            sheet.frame_names,
            ["hero 2.aseprite", "hero 0.aseprite", "hero 1.aseprite"],
            "file order, not key order"
        );
        let first = sheet.frames[1];
        assert_eq!(first.rect, Rect::new(0.0, 0.0, 20.0, 24.0));
        assert_eq!(first.source_size, vec2(24.0, 24.0));
        assert_eq!(first.offset, vec2(2.0, 0.0));
        assert_eq!(sheet.frames[0].duration, 0.12);

        let run = sheet.clip("run").unwrap();
        assert_eq!(run.frames.len(), 2);
        assert_eq!(run.repeat, None, "no repeat count loops");
        assert_eq!(
            run.events,
            [
                ClipEvent {
                    frame: 0,
                    name: "footstep".into()
                },
                ClipEvent {
                    frame: 1,
                    name: "footstep".into()
                }
            ]
        );
        assert_eq!(sheet.clip("hit").unwrap().repeat, Some(1));
        let wave = sheet.clip("wave").unwrap();
        assert_eq!(wave.direction, Direction::PingPong);
        assert_eq!(wave.repeat, Some(2));
        assert!(wave.events.is_empty());
        assert_eq!(warnings.len(), 1, "{warnings:?}");
        assert!(warnings[0].contains("sparkle"));
        assert!(sheet.clip("default").is_none());
    }

    #[test]
    fn array_exports_without_tags_get_a_default_clip() {
        let (sheet, warnings) = sheet(ARRAY);
        assert!(warnings.is_empty());
        assert_eq!(sheet.frame_names, ["coin 0", "coin 1"]);
        let clip = sheet.clip("default").unwrap();
        assert_eq!(clip.frames[1].duration, 0.15);
        assert_eq!(sheet.frames[1].offset, cg_core::glam::Vec2::ZERO);
    }

    #[test]
    fn bad_exports_say_what_is_wrong() {
        let error = |text: &str| {
            AsepriteExport::parse(text)
                .and_then(|export| export.into_sheet(Handle::default()))
                .unwrap_err()
        };
        let tag =
            |tag: &str| ARRAY.replace(r#""size""#, &format!(r#""frameTags": [{tag}], "size""#));
        assert!(
            error(&tag(r#"{ "name": "x", "from": 0, "to": 5 }"#)).contains("the sheet has 2"),
            "out of range"
        );
        assert!(
            error(&tag(
                r#"{ "name": "x", "from": 0, "to": 1, "direction": "sideways" }"#
            ))
            .contains("unknown direction `sideways`")
        );
        assert!(
            error(&tag(
                r#"{ "name": "x", "from": 0, "to": 1, "data": "4:boom" }"#
            ))
            .contains("event on frame 4")
        );
        assert!(error("{ \"frames\": 3 }").contains("object or array"));
        assert!(
            error(&ARRAY.replace(r#""rotated": false"#, r#""rotated": true"#)).contains("rotated")
        );
    }

    #[test]
    fn events_parse_from_user_data() {
        let (events, skipped) = parse_events("0:step,2:step  5:land hello 3: x:y");
        assert_eq!(
            events
                .iter()
                .map(|e| (e.frame, e.name.as_str()))
                .collect::<Vec<_>>(),
            [(0, "step"), (2, "step"), (5, "land")]
        );
        assert_eq!(skipped, ["hello", "3:", "x:y"]);
    }
}
