//! Component data as a tree the inspector can show and edit: any `Serialize` value is
//! captured with its exact serde shape (struct and variant names included), edited field
//! by field, and written back as RON that the component's own `Deserialize` reads. Fields
//! come from the same serde data prefabs use, so every registered prefab component can be
//! edited without reflection.

use cashgear_engine::world::{Prefab, PrefabRegistry};
use core::fmt::{self, Write as _};
use serde::ser::{self, Serialize};

/// A value in serde's data model.
#[derive(Debug, Clone, PartialEq)]
pub enum Value {
    /// `bool`.
    Bool(bool),
    /// Any signed integer.
    Int(i64),
    /// Any unsigned integer.
    UInt(u64),
    /// `f32`.
    F32(f32),
    /// `f64`.
    F64(f64),
    /// `char`.
    Char(char),
    /// A string.
    Str(String),
    /// Bytes.
    Bytes(Vec<u8>),
    /// `None`.
    None,
    /// `Some(value)`.
    Some(Box<Value>),
    /// `()`.
    Unit,
    /// A struct without fields.
    UnitStruct(&'static str),
    /// An enum variant without data.
    UnitVariant {
        /// The enum's name.
        name: &'static str,
        /// The variant's name.
        variant: &'static str,
    },
    /// A struct with one unnamed field.
    NewtypeStruct(&'static str, Box<Value>),
    /// An enum variant with one unnamed field.
    NewtypeVariant {
        /// The enum's name.
        name: &'static str,
        /// The variant's name.
        variant: &'static str,
        /// Its field.
        value: Box<Value>,
    },
    /// A list.
    Seq(Vec<Value>),
    /// A tuple.
    Tuple(Vec<Value>),
    /// A struct with unnamed fields (glam's vectors).
    TupleStruct(&'static str, Vec<Value>),
    /// An enum variant with unnamed fields.
    TupleVariant {
        /// The enum's name.
        name: &'static str,
        /// The variant's name.
        variant: &'static str,
        /// Its fields.
        fields: Vec<Value>,
    },
    /// A map.
    Map(Vec<(Value, Value)>),
    /// A struct with named fields.
    Struct(&'static str, Vec<(&'static str, Value)>),
    /// An enum variant with named fields.
    StructVariant {
        /// The enum's name.
        name: &'static str,
        /// The variant's name.
        variant: &'static str,
        /// Its fields.
        fields: Vec<(&'static str, Value)>,
    },
}

impl Value {
    /// Captures `value`.
    pub fn from_serialize<T: Serialize + ?Sized>(value: &T) -> Result<Self, String> {
        value.serialize(ValueSerializer).map_err(|err| err.0)
    }

    /// The value as one line of RON.
    pub fn to_ron(&self) -> String {
        let mut out = String::new();
        self.write_ron(&mut out);
        out
    }

    fn write_ron(&self, out: &mut String) {
        let list = |out: &mut String, open: &str, items: &[Value], close: &str| {
            out.push_str(open);
            for (i, item) in items.iter().enumerate() {
                if i > 0 {
                    out.push_str(", ");
                }
                item.write_ron(out);
            }
            out.push_str(close);
        };
        let fields = |out: &mut String, fields: &[(&str, Value)]| {
            out.push('(');
            for (i, (name, value)) in fields.iter().enumerate() {
                if i > 0 {
                    out.push_str(", ");
                }
                out.push_str(name);
                out.push_str(": ");
                value.write_ron(out);
            }
            out.push(')');
        };
        match self {
            Self::Bool(value) => out.push_str(if *value { "true" } else { "false" }),
            Self::Int(value) => {
                let _ = write!(out, "{value}");
            }
            Self::UInt(value) => {
                let _ = write!(out, "{value}");
            }
            Self::F32(value) => out.push_str(&float(f64::from(*value), value.to_string())),
            Self::F64(value) => out.push_str(&float(*value, value.to_string())),
            Self::Char(value) => {
                let _ = write!(out, "{value:?}");
            }
            Self::Str(value) => {
                let _ = write!(out, "{value:?}");
            }
            Self::Bytes(bytes) => {
                let items: Vec<Value> = bytes.iter().map(|b| Self::UInt(u64::from(*b))).collect();
                list(out, "[", &items, "]");
            }
            Self::None => out.push_str("None"),
            Self::Some(value) => {
                out.push_str("Some(");
                value.write_ron(out);
                out.push(')');
            }
            Self::Unit | Self::UnitStruct(_) => out.push_str("()"),
            Self::UnitVariant { variant, .. } => out.push_str(variant),
            Self::NewtypeStruct(_, value) => {
                out.push('(');
                value.write_ron(out);
                out.push(')');
            }
            Self::NewtypeVariant { variant, value, .. } => {
                out.push_str(variant);
                out.push('(');
                value.write_ron(out);
                out.push(')');
            }
            Self::Seq(items) => list(out, "[", items, "]"),
            Self::Tuple(items) | Self::TupleStruct(_, items) => list(out, "(", items, ")"),
            Self::TupleVariant {
                variant, fields, ..
            } => {
                out.push_str(variant);
                list(out, "(", fields, ")");
            }
            Self::Map(entries) => {
                out.push('{');
                for (i, (key, value)) in entries.iter().enumerate() {
                    if i > 0 {
                        out.push_str(", ");
                    }
                    key.write_ron(out);
                    out.push_str(": ");
                    value.write_ron(out);
                }
                out.push('}');
            }
            Self::Struct(_, members) => fields(out, members),
            Self::StructVariant {
                variant,
                fields: members,
                ..
            } => {
                out.push_str(variant);
                fields(out, members);
            }
        }
    }

    /// Whether the value is shown on one line: numbers, strings and the like.
    pub fn is_scalar(&self) -> bool {
        matches!(
            self,
            Self::Bool(_)
                | Self::Int(_)
                | Self::UInt(_)
                | Self::F32(_)
                | Self::F64(_)
                | Self::Char(_)
                | Self::Str(_)
                | Self::None
                | Self::Unit
                | Self::UnitStruct(_)
                | Self::UnitVariant { .. }
        )
    }

    /// Two numbers in a row, as glam's `Vec2` and friends serialize: `(x, y)`.
    pub fn as_pair_mut(&mut self) -> Option<(&mut Value, &mut Value)> {
        match self {
            Self::Tuple(items) | Self::TupleStruct(_, items) | Self::Seq(items)
                if items.len() == 2 && items.iter().all(Value::is_number) =>
            {
                let (a, b) = items.split_at_mut(1);
                Some((&mut a[0], &mut b[0]))
            }
            _ => None,
        }
    }

    /// Whether the value is two numbers in a row (see [`as_pair_mut`](Self::as_pair_mut)).
    pub fn is_pair(&self) -> bool {
        matches!(
            self,
            Self::Tuple(items) | Self::TupleStruct(_, items) | Self::Seq(items)
                if items.len() == 2 && items.iter().all(Value::is_number)
        )
    }

    /// Whether the value is a number.
    pub fn is_number(&self) -> bool {
        matches!(
            self,
            Self::Int(_) | Self::UInt(_) | Self::F32(_) | Self::F64(_)
        )
    }

    /// The value as an `f64`, if it is a number.
    pub fn as_f64(&self) -> Option<f64> {
        match *self {
            Self::Int(v) => Some(v as f64),
            Self::UInt(v) => Some(v as f64),
            Self::F32(v) => Some(f64::from(v)),
            Self::F64(v) => Some(v),
            _ => None,
        }
    }

    /// The field called `name`, for structs.
    pub fn field_mut(&mut self, name: &str) -> Option<&mut Value> {
        match self {
            Self::Struct(_, fields) | Self::StructVariant { fields, .. } => fields
                .iter_mut()
                .find(|(field, _)| *field == name)
                .map(|(_, value)| value),
            _ => None,
        }
    }
}

/// A float as RON: whole numbers without a fraction (`12`, as hand-written prefabs have
/// them; the component's own `Deserialize` reads them as floats), others in Rust's
/// shortest form.
fn float(value: f64, text: String) -> String {
    if value.is_finite() && value.fract() == 0.0 && value.abs() < 1e15 {
        format!("{}", value as i64)
    } else if value.is_nan() {
        "NaN".into()
    } else {
        text
    }
}

/// A `'static` copy of a name (an enum variant the user picked), made once per name.
pub fn intern(name: &str) -> &'static str {
    static NAMES: std::sync::Mutex<Vec<&'static str>> = std::sync::Mutex::new(Vec::new());
    let mut names = NAMES
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    if let Some(existing) = names.iter().find(|n| **n == name) {
        return existing;
    }
    let leaked: &'static str = Box::leak(name.to_owned().into_boxed_str());
    names.push(leaked);
    leaked
}

/// Parses `ron` as the registered component `name` and captures it as a [`Value`]: the
/// component's own serde reads it, so fields the text leaves out get their defaults.
pub fn parse_component(registry: &PrefabRegistry, name: &str, ron: &str) -> Result<Value, String> {
    let prefab = component_prefab(registry, name, ron)?;
    let (_, data) = prefab
        .components()
        .next()
        .ok_or_else(|| format!("`{name}` is empty"))?;
    Value::from_serialize(data)
}

/// A one-component prefab of the registered component `name` read from `ron`.
pub fn component_prefab(
    registry: &PrefabRegistry,
    name: &str,
    ron: &str,
) -> Result<Prefab, String> {
    Prefab::from_ron(&format!("(components: {{ {name:?}: {ron} }})"), registry)
        .map_err(|err| strip_position(&err.to_string()))
}

/// Errors from a one-line wrapper point at its columns, which mean nothing to the user.
fn strip_position(message: &str) -> String {
    match message.split_once(": ") {
        Some((position, rest)) if position.starts_with("line ") => rest.to_owned(),
        _ => message.to_owned(),
    }
}

/// The variants an enum inside a component may take, found by asking the component's own
/// `Deserialize` what it expected instead of a variant that cannot exist. `path` leads
/// from the component's value to the enum (see [`Value::at_mut`]). Empty if the enum's
/// variants cannot be found that way.
pub fn variants(
    registry: &PrefabRegistry,
    name: &str,
    component: &Value,
    path: &[usize],
) -> Vec<String> {
    const PROBE: &str = "CgEditorNoSuchVariant";
    let mut probe = component.clone();
    let Some(
        Value::UnitVariant { variant, .. }
        | Value::NewtypeVariant { variant, .. }
        | Value::TupleVariant { variant, .. }
        | Value::StructVariant { variant, .. },
    ) = probe.at_mut(path)
    else {
        return Vec::new();
    };
    *variant = PROBE;
    let Err(message) = component_prefab(registry, name, &probe.to_ron()) else {
        return Vec::new();
    };
    // "Unexpected variant named `X` in enum `E`, expected one of `A`, `B` instead".
    let Some((_, expected)) = message.split_once(", expected ") else {
        return Vec::new();
    };
    expected
        .split('`')
        .skip(1)
        .step_by(2)
        .map(|word| word.trim_start_matches("r#").to_owned())
        .filter(|word| word != PROBE)
        .collect()
}

impl Value {
    /// The value at `path`: each step is a field, item or entry index into the value
    /// before it (an `Option`'s or newtype's inner value is step 0).
    pub fn at_mut(&mut self, path: &[usize]) -> Option<&mut Value> {
        let Some((&first, rest)) = path.split_first() else {
            return Some(self);
        };
        let next = match self {
            Self::Some(value)
            | Self::NewtypeStruct(_, value)
            | Self::NewtypeVariant { value, .. } => (first == 0).then_some(&mut **value)?,
            Self::Seq(items)
            | Self::Tuple(items)
            | Self::TupleStruct(_, items)
            | Self::TupleVariant { fields: items, .. } => items.get_mut(first)?,
            Self::Map(entries) => &mut entries.get_mut(first)?.1,
            Self::Struct(_, fields) | Self::StructVariant { fields, .. } => {
                &mut fields.get_mut(first)?.1
            }
            _ => return None,
        };
        next.at_mut(rest)
    }
}

/// An error from capturing a value.
#[derive(Debug)]
pub struct CaptureError(String);

impl fmt::Display for CaptureError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for CaptureError {}

impl ser::Error for CaptureError {
    fn custom<T: fmt::Display>(msg: T) -> Self {
        Self(msg.to_string())
    }
}

/// Serializes anything into a [`Value`].
struct ValueSerializer;

type Result<T, E = CaptureError> = core::result::Result<T, E>;

impl ser::Serializer for ValueSerializer {
    type Ok = Value;
    type Error = CaptureError;
    type SerializeSeq = Collect;
    type SerializeTuple = Collect;
    type SerializeTupleStruct = Collect;
    type SerializeTupleVariant = Collect;
    type SerializeMap = CollectMap;
    type SerializeStruct = CollectFields;
    type SerializeStructVariant = CollectFields;

    fn serialize_bool(self, v: bool) -> Result<Value> {
        Ok(Value::Bool(v))
    }
    fn serialize_i8(self, v: i8) -> Result<Value> {
        Ok(Value::Int(v.into()))
    }
    fn serialize_i16(self, v: i16) -> Result<Value> {
        Ok(Value::Int(v.into()))
    }
    fn serialize_i32(self, v: i32) -> Result<Value> {
        Ok(Value::Int(v.into()))
    }
    fn serialize_i64(self, v: i64) -> Result<Value> {
        Ok(Value::Int(v))
    }
    fn serialize_u8(self, v: u8) -> Result<Value> {
        Ok(Value::UInt(v.into()))
    }
    fn serialize_u16(self, v: u16) -> Result<Value> {
        Ok(Value::UInt(v.into()))
    }
    fn serialize_u32(self, v: u32) -> Result<Value> {
        Ok(Value::UInt(v.into()))
    }
    fn serialize_u64(self, v: u64) -> Result<Value> {
        Ok(Value::UInt(v))
    }
    fn serialize_f32(self, v: f32) -> Result<Value> {
        Ok(Value::F32(v))
    }
    fn serialize_f64(self, v: f64) -> Result<Value> {
        Ok(Value::F64(v))
    }
    fn serialize_char(self, v: char) -> Result<Value> {
        Ok(Value::Char(v))
    }
    fn serialize_str(self, v: &str) -> Result<Value> {
        Ok(Value::Str(v.to_owned()))
    }
    fn serialize_bytes(self, v: &[u8]) -> Result<Value> {
        Ok(Value::Bytes(v.to_owned()))
    }
    fn serialize_none(self) -> Result<Value> {
        Ok(Value::None)
    }
    fn serialize_some<T: Serialize + ?Sized>(self, value: &T) -> Result<Value> {
        Ok(Value::Some(Box::new(value.serialize(self)?)))
    }
    fn serialize_unit(self) -> Result<Value> {
        Ok(Value::Unit)
    }
    fn serialize_unit_struct(self, name: &'static str) -> Result<Value> {
        Ok(Value::UnitStruct(name))
    }
    fn serialize_unit_variant(
        self,
        name: &'static str,
        _index: u32,
        variant: &'static str,
    ) -> Result<Value> {
        Ok(Value::UnitVariant { name, variant })
    }
    fn serialize_newtype_struct<T: Serialize + ?Sized>(
        self,
        name: &'static str,
        value: &T,
    ) -> Result<Value> {
        Ok(Value::NewtypeStruct(name, Box::new(value.serialize(self)?)))
    }
    fn serialize_newtype_variant<T: Serialize + ?Sized>(
        self,
        name: &'static str,
        _index: u32,
        variant: &'static str,
        value: &T,
    ) -> Result<Value> {
        Ok(Value::NewtypeVariant {
            name,
            variant,
            value: Box::new(value.serialize(self)?),
        })
    }
    fn serialize_seq(self, len: Option<usize>) -> Result<Collect> {
        Ok(Collect::new(Shape::Seq, len))
    }
    fn serialize_tuple(self, len: usize) -> Result<Collect> {
        Ok(Collect::new(Shape::Tuple, Some(len)))
    }
    fn serialize_tuple_struct(self, name: &'static str, len: usize) -> Result<Collect> {
        Ok(Collect::new(Shape::TupleStruct(name), Some(len)))
    }
    fn serialize_tuple_variant(
        self,
        name: &'static str,
        _index: u32,
        variant: &'static str,
        len: usize,
    ) -> Result<Collect> {
        Ok(Collect::new(Shape::TupleVariant(name, variant), Some(len)))
    }
    fn serialize_map(self, len: Option<usize>) -> Result<CollectMap> {
        Ok(CollectMap {
            entries: Vec::with_capacity(len.unwrap_or(0)),
            key: None,
        })
    }
    fn serialize_struct(self, name: &'static str, len: usize) -> Result<CollectFields> {
        Ok(CollectFields {
            name,
            variant: None,
            fields: Vec::with_capacity(len),
        })
    }
    fn serialize_struct_variant(
        self,
        name: &'static str,
        _index: u32,
        variant: &'static str,
        len: usize,
    ) -> Result<CollectFields> {
        Ok(CollectFields {
            name,
            variant: Some(variant),
            fields: Vec::with_capacity(len),
        })
    }
}

enum Shape {
    Seq,
    Tuple,
    TupleStruct(&'static str),
    TupleVariant(&'static str, &'static str),
}

struct Collect {
    shape: Shape,
    items: Vec<Value>,
}

impl Collect {
    fn new(shape: Shape, len: Option<usize>) -> Self {
        Self {
            shape,
            items: Vec::with_capacity(len.unwrap_or(0)),
        }
    }

    fn push<T: Serialize + ?Sized>(&mut self, value: &T) -> Result<()> {
        self.items.push(value.serialize(ValueSerializer)?);
        Ok(())
    }

    fn finish(self) -> Value {
        match self.shape {
            Shape::Seq => Value::Seq(self.items),
            Shape::Tuple => Value::Tuple(self.items),
            Shape::TupleStruct(name) => Value::TupleStruct(name, self.items),
            Shape::TupleVariant(name, variant) => Value::TupleVariant {
                name,
                variant,
                fields: self.items,
            },
        }
    }
}

impl ser::SerializeSeq for Collect {
    type Ok = Value;
    type Error = CaptureError;
    fn serialize_element<T: Serialize + ?Sized>(&mut self, value: &T) -> Result<()> {
        self.push(value)
    }
    fn end(self) -> Result<Value> {
        Ok(self.finish())
    }
}

impl ser::SerializeTuple for Collect {
    type Ok = Value;
    type Error = CaptureError;
    fn serialize_element<T: Serialize + ?Sized>(&mut self, value: &T) -> Result<()> {
        self.push(value)
    }
    fn end(self) -> Result<Value> {
        Ok(self.finish())
    }
}

impl ser::SerializeTupleStruct for Collect {
    type Ok = Value;
    type Error = CaptureError;
    fn serialize_field<T: Serialize + ?Sized>(&mut self, value: &T) -> Result<()> {
        self.push(value)
    }
    fn end(self) -> Result<Value> {
        Ok(self.finish())
    }
}

impl ser::SerializeTupleVariant for Collect {
    type Ok = Value;
    type Error = CaptureError;
    fn serialize_field<T: Serialize + ?Sized>(&mut self, value: &T) -> Result<()> {
        self.push(value)
    }
    fn end(self) -> Result<Value> {
        Ok(self.finish())
    }
}

struct CollectMap {
    entries: Vec<(Value, Value)>,
    key: Option<Value>,
}

impl ser::SerializeMap for CollectMap {
    type Ok = Value;
    type Error = CaptureError;
    fn serialize_key<T: Serialize + ?Sized>(&mut self, key: &T) -> Result<()> {
        self.key = Some(key.serialize(ValueSerializer)?);
        Ok(())
    }
    fn serialize_value<T: Serialize + ?Sized>(&mut self, value: &T) -> Result<()> {
        let key = self
            .key
            .take()
            .ok_or_else(|| CaptureError("a map value without a key".into()))?;
        self.entries.push((key, value.serialize(ValueSerializer)?));
        Ok(())
    }
    fn end(self) -> Result<Value> {
        Ok(Value::Map(self.entries))
    }
}

struct CollectFields {
    name: &'static str,
    variant: Option<&'static str>,
    fields: Vec<(&'static str, Value)>,
}

impl CollectFields {
    fn push<T: Serialize + ?Sized>(&mut self, key: &'static str, value: &T) -> Result<()> {
        self.fields.push((key, value.serialize(ValueSerializer)?));
        Ok(())
    }

    fn finish(self) -> Value {
        match self.variant {
            None => Value::Struct(self.name, self.fields),
            Some(variant) => Value::StructVariant {
                name: self.name,
                variant,
                fields: self.fields,
            },
        }
    }
}

impl ser::SerializeStruct for CollectFields {
    type Ok = Value;
    type Error = CaptureError;
    fn serialize_field<T: Serialize + ?Sized>(
        &mut self,
        key: &'static str,
        value: &T,
    ) -> Result<()> {
        self.push(key, value)
    }
    fn skip_field(&mut self, _key: &'static str) -> Result<()> {
        Ok(())
    }
    fn end(self) -> Result<Value> {
        Ok(self.finish())
    }
}

impl ser::SerializeStructVariant for CollectFields {
    type Ok = Value;
    type Error = CaptureError;
    fn serialize_field<T: Serialize + ?Sized>(
        &mut self,
        key: &'static str,
        value: &T,
    ) -> Result<()> {
        self.push(key, value)
    }
    fn end(self) -> Result<Value> {
        Ok(self.finish())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use cashgear_engine::prelude::*;
    use cashgear_engine::render::ScalingMode;

    fn registry() -> App {
        let mut app = App::new();
        app.add_plugins(WorldPlugin)
            .register_component::<Camera2D>("Camera2D")
            .register_component::<Layer>("Layer");
        app
    }

    #[test]
    fn values_keep_their_shape_and_read_back() {
        let app = registry();
        let registry = app.world().resource::<PrefabRegistry>();
        let mut camera = parse_component(registry, "Camera2D", "(zoom: 2)").unwrap();
        // Fields left out took their defaults.
        assert_eq!(camera.field_mut("zoom"), Some(&mut Value::F32(2.0)));
        assert!(matches!(
            camera.field_mut("scaling"),
            Some(Value::NewtypeVariant {
                variant: "PixelPerfect",
                ..
            })
        ));
        // Edit a field, write it, and the component reads it back.
        *camera.field_mut("pixel_snap").unwrap() = Value::Bool(false);
        let prefab = component_prefab(registry, "Camera2D", &camera.to_ron()).unwrap();
        let again = Value::from_serialize(prefab.components().next().unwrap().1).unwrap();
        assert_eq!(again, camera);

        let layer = parse_component(registry, "Layer", "(7)").unwrap();
        assert_eq!(
            layer,
            Value::NewtypeStruct("Layer", Box::new(Value::Int(7)))
        );
        assert_eq!(layer.to_ron(), "(7)");
    }

    #[test]
    fn floats_and_strings_are_written_as_ron() {
        assert_eq!(Value::F32(12.0).to_ron(), "12");
        assert_eq!(Value::F32(0.1).to_ron(), "0.1");
        assert_eq!(Value::F32(-2.5).to_ron(), "-2.5");
        assert_eq!(Value::F64(f64::INFINITY).to_ron(), "inf");
        assert_eq!(Value::Str("a \"b\"\n".into()).to_ron(), r#""a \"b\"\n""#);
        let pair = Value::from_serialize(&vec2(3.0, 4.5)).unwrap();
        assert_eq!(pair.to_ron(), "(3, 4.5)");
    }

    #[test]
    fn enum_variants_are_found_by_probing() {
        let app = registry();
        let registry = app.world().resource::<PrefabRegistry>();
        let camera = Value::from_serialize(&Camera2D {
            scaling: ScalingMode::WindowSize,
            ..default()
        })
        .unwrap();
        let Value::Struct(_, fields) = &camera else {
            panic!("a struct");
        };
        let scaling = fields
            .iter()
            .position(|(name, _)| *name == "scaling")
            .unwrap();
        assert_eq!(
            variants(registry, "Camera2D", &camera, &[scaling]),
            ["WindowSize", "Stretch", "Letterbox", "PixelPerfect"]
        );
        assert!(
            variants(registry, "Camera2D", &camera, &[0]).is_empty(),
            "zoom is no enum"
        );
    }
}
