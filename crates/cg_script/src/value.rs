//! [`ScriptValue`]: the values that cross between scripts and Rust (event data, the
//! arguments and results of game functions), and the conversions to and from Rust types.

use bevy_ecs::entity::Entity;
use cg_core::glam::{Vec2, vec2};
use core::fmt;
use std::collections::BTreeMap;

/// A value passed between a script and Rust: the data of events, and the arguments and
/// results of the functions a game adds with
/// [`AppScriptExt::add_script_function`](crate::AppScriptExt::add_script_function).
///
/// Lua tables become a [`List`](Self::List) when their keys are exactly `1..=n` and a
/// [`Map`](Self::Map) otherwise (keys turned into strings), so the empty table is an empty
/// list. Functions and other userdata cannot cross.
#[derive(Debug, Clone, PartialEq, Default)]
pub enum ScriptValue {
    /// Lua's `nil`.
    #[default]
    Nil,
    /// A boolean.
    Bool(bool),
    /// An integer.
    Int(i64),
    /// A floating-point number.
    Number(f64),
    /// A string.
    String(String),
    /// An entity.
    Entity(Entity),
    /// A sequence.
    List(Vec<ScriptValue>),
    /// A table with named fields, in key order.
    Map(BTreeMap<String, ScriptValue>),
}

impl ScriptValue {
    /// Whether it is `nil`.
    pub fn is_nil(&self) -> bool {
        matches!(self, Self::Nil)
    }

    /// The value as a number (integers convert).
    pub fn as_f64(&self) -> Option<f64> {
        match *self {
            Self::Int(n) => Some(n as f64),
            Self::Number(n) => Some(n),
            _ => None,
        }
    }

    /// The value as an integer (numbers with no fractional part convert).
    pub fn as_i64(&self) -> Option<i64> {
        match *self {
            Self::Int(n) => Some(n),
            Self::Number(n) if n.fract() == 0.0 && n.abs() < 9.007_199_254_740_992e15 => {
                Some(n as i64)
            }
            _ => None,
        }
    }

    /// The value as a boolean.
    pub fn as_bool(&self) -> Option<bool> {
        match *self {
            Self::Bool(b) => Some(b),
            _ => None,
        }
    }

    /// The value as a string.
    pub fn as_str(&self) -> Option<&str> {
        match self {
            Self::String(s) => Some(s),
            _ => None,
        }
    }

    /// The value as an entity.
    pub fn as_entity(&self) -> Option<Entity> {
        match *self {
            Self::Entity(e) => Some(e),
            _ => None,
        }
    }

    /// The field `key` of a map (`Nil` if absent or not a map).
    pub fn get(&self, key: &str) -> &ScriptValue {
        match self {
            Self::Map(map) => map.get(key).unwrap_or(&Self::Nil),
            _ => &Self::Nil,
        }
    }

    /// A short name of the value's kind, for error messages.
    pub fn kind(&self) -> &'static str {
        match self {
            Self::Nil => "nil",
            Self::Bool(_) => "a boolean",
            Self::Int(_) | Self::Number(_) => "a number",
            Self::String(_) => "a string",
            Self::Entity(_) => "an entity",
            Self::List(_) => "a list",
            Self::Map(_) => "a table",
        }
    }
}

impl fmt::Display for ScriptValue {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Nil => f.write_str("nil"),
            Self::Bool(b) => write!(f, "{b}"),
            Self::Int(n) => write!(f, "{n}"),
            Self::Number(n) => write!(f, "{n}"),
            Self::String(s) => f.write_str(s),
            Self::Entity(e) => write!(f, "entity {e}"),
            Self::List(items) => {
                f.write_str("{")?;
                for (i, item) in items.iter().enumerate() {
                    if i > 0 {
                        f.write_str(", ")?;
                    }
                    write!(f, "{item}")?;
                }
                f.write_str("}")
            }
            Self::Map(map) => {
                f.write_str("{")?;
                for (i, (key, value)) in map.iter().enumerate() {
                    if i > 0 {
                        f.write_str(", ")?;
                    }
                    write!(f, "{key} = {value}")?;
                }
                f.write_str("}")
            }
        }
    }
}

/// Converts a Rust value into a [`ScriptValue`]: the results of game functions and the data
/// of events.
pub trait IntoScriptValue {
    /// The value for scripts.
    fn into_script_value(self) -> ScriptValue;
}

/// Reads a Rust value from a [`ScriptValue`]: the arguments of game functions.
pub trait FromScriptValue: Sized {
    /// The value, or what was wrong with it (`expected a number, got a string`).
    fn from_script_value(value: &ScriptValue) -> Result<Self, String>;
}

impl IntoScriptValue for ScriptValue {
    fn into_script_value(self) -> ScriptValue {
        self
    }
}

impl FromScriptValue for ScriptValue {
    fn from_script_value(value: &ScriptValue) -> Result<Self, String> {
        Ok(value.clone())
    }
}

impl IntoScriptValue for () {
    fn into_script_value(self) -> ScriptValue {
        ScriptValue::Nil
    }
}

impl IntoScriptValue for bool {
    fn into_script_value(self) -> ScriptValue {
        ScriptValue::Bool(self)
    }
}

impl FromScriptValue for bool {
    fn from_script_value(value: &ScriptValue) -> Result<Self, String> {
        value.as_bool().ok_or_else(|| expected("a boolean", value))
    }
}

macro_rules! numbers {
    ($($t:ty => $variant:ident),*) => {$(
        impl IntoScriptValue for $t {
            fn into_script_value(self) -> ScriptValue {
                ScriptValue::$variant(self.into())
            }
        }
    )*};
}

numbers!(i32 => Int, u32 => Int, i64 => Int, f32 => Number, f64 => Number);

impl IntoScriptValue for usize {
    fn into_script_value(self) -> ScriptValue {
        ScriptValue::Int(self as i64)
    }
}

impl FromScriptValue for f64 {
    fn from_script_value(value: &ScriptValue) -> Result<Self, String> {
        value.as_f64().ok_or_else(|| expected("a number", value))
    }
}

impl FromScriptValue for f32 {
    fn from_script_value(value: &ScriptValue) -> Result<Self, String> {
        f64::from_script_value(value).map(|n| n as f32)
    }
}

impl FromScriptValue for i64 {
    fn from_script_value(value: &ScriptValue) -> Result<Self, String> {
        value.as_i64().ok_or_else(|| expected("an integer", value))
    }
}

macro_rules! integers {
    ($($t:ty),*) => {$(
        impl FromScriptValue for $t {
            fn from_script_value(value: &ScriptValue) -> Result<Self, String> {
                let n = i64::from_script_value(value)?;
                <$t>::try_from(n).map_err(|_| format!("{n} is out of range"))
            }
        }
    )*};
}

integers!(i32, u32, usize);

impl IntoScriptValue for String {
    fn into_script_value(self) -> ScriptValue {
        ScriptValue::String(self)
    }
}

impl IntoScriptValue for &str {
    fn into_script_value(self) -> ScriptValue {
        ScriptValue::String(self.to_owned())
    }
}

impl FromScriptValue for String {
    fn from_script_value(value: &ScriptValue) -> Result<Self, String> {
        value
            .as_str()
            .map(str::to_owned)
            .ok_or_else(|| expected("a string", value))
    }
}

impl IntoScriptValue for Entity {
    fn into_script_value(self) -> ScriptValue {
        ScriptValue::Entity(self)
    }
}

impl FromScriptValue for Entity {
    fn from_script_value(value: &ScriptValue) -> Result<Self, String> {
        value
            .as_entity()
            .ok_or_else(|| expected("an entity", value))
    }
}

/// A `Vec2` is the list `{x, y}`; a table `{x = .., y = ..}` reads as one too.
impl IntoScriptValue for Vec2 {
    fn into_script_value(self) -> ScriptValue {
        ScriptValue::List(vec![
            ScriptValue::Number(self.x.into()),
            ScriptValue::Number(self.y.into()),
        ])
    }
}

impl FromScriptValue for Vec2 {
    fn from_script_value(value: &ScriptValue) -> Result<Self, String> {
        match value {
            ScriptValue::List(items) if items.len() == 2 => Ok(vec2(
                f32::from_script_value(&items[0])?,
                f32::from_script_value(&items[1])?,
            )),
            ScriptValue::Map(_) => Ok(vec2(
                f32::from_script_value(value.get("x"))?,
                f32::from_script_value(value.get("y"))?,
            )),
            _ => Err(expected("a vector {x, y}", value)),
        }
    }
}

impl<T: IntoScriptValue> IntoScriptValue for Option<T> {
    fn into_script_value(self) -> ScriptValue {
        self.map_or(ScriptValue::Nil, T::into_script_value)
    }
}

/// `nil` reads as `None`.
impl<T: FromScriptValue> FromScriptValue for Option<T> {
    fn from_script_value(value: &ScriptValue) -> Result<Self, String> {
        match value {
            ScriptValue::Nil => Ok(None),
            value => T::from_script_value(value).map(Some),
        }
    }
}

impl<T: IntoScriptValue> IntoScriptValue for Vec<T> {
    fn into_script_value(self) -> ScriptValue {
        ScriptValue::List(self.into_iter().map(T::into_script_value).collect())
    }
}

impl<T: FromScriptValue> FromScriptValue for Vec<T> {
    fn from_script_value(value: &ScriptValue) -> Result<Self, String> {
        match value {
            ScriptValue::List(items) => items.iter().map(T::from_script_value).collect(),
            _ => Err(expected("a list", value)),
        }
    }
}

impl<T: IntoScriptValue> IntoScriptValue for BTreeMap<String, T> {
    fn into_script_value(self) -> ScriptValue {
        ScriptValue::Map(
            self.into_iter()
                .map(|(k, v)| (k, v.into_script_value()))
                .collect(),
        )
    }
}

/// The arguments of a game function, read from what the script passed. Implemented for
/// tuples of up to six [`FromScriptValue`] types (missing arguments read as `nil`, so
/// `Option`s can be left out) and for `Vec<ScriptValue>`, which takes them all as they are.
pub trait FromScriptArgs: Sized {
    /// The arguments, or what was wrong with them.
    fn from_script_args(args: &[ScriptValue]) -> Result<Self, String>;
}

impl FromScriptArgs for Vec<ScriptValue> {
    fn from_script_args(args: &[ScriptValue]) -> Result<Self, String> {
        Ok(args.to_vec())
    }
}

macro_rules! args {
    ($($name:ident $index:tt),*) => {
        impl<$($name: FromScriptValue),*> FromScriptArgs for ($($name,)*) {
            #[allow(unused_variables)]
            fn from_script_args(args: &[ScriptValue]) -> Result<Self, String> {
                Ok(($(
                    $name::from_script_value(args.get($index).unwrap_or(&ScriptValue::Nil))
                        .map_err(|err| format!("argument {}: {err}", $index + 1))?,
                )*))
            }
        }
    };
}

args!();
args!(A 0);
args!(A 0, B 1);
args!(A 0, B 1, C 2);
args!(A 0, B 1, C 2, D 3);
args!(A 0, B 1, C 2, D 3, E 4);
args!(A 0, B 1, C 2, D 3, E 4, F 5);

fn expected(what: &str, got: &ScriptValue) -> String {
    format!("expected {what}, got {}", got.kind())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn arguments_read_by_position() {
        let args = [ScriptValue::Int(3), ScriptValue::String("hi".into())];
        let (n, s, missing): (f32, String, Option<bool>) =
            FromScriptArgs::from_script_args(&args).unwrap();
        assert_eq!((n, s.as_str(), missing), (3.0, "hi", None));
        let err = <(String,)>::from_script_args(&args).unwrap_err();
        assert_eq!(err, "argument 1: expected a string, got a number");
        let err = <(u32,)>::from_script_args(&[ScriptValue::Int(-1)]).unwrap_err();
        assert_eq!(err, "argument 1: -1 is out of range");
    }

    #[test]
    fn vectors_read_from_lists_and_tables() {
        let list = vec2(1.0, 2.0).into_script_value();
        assert_eq!(Vec2::from_script_value(&list), Ok(vec2(1.0, 2.0)));
        let map = ScriptValue::Map(BTreeMap::from([
            ("x".to_owned(), ScriptValue::Int(4)),
            ("y".to_owned(), ScriptValue::Number(0.5)),
        ]));
        assert_eq!(Vec2::from_script_value(&map), Ok(vec2(4.0, 0.5)));
        assert_eq!(map.to_string(), "{x = 4, y = 0.5}");
        assert_eq!(vec![1, 2].into_script_value().to_string(), "{1, 2}");
    }

    #[test]
    fn whole_numbers_are_integers() {
        assert_eq!(ScriptValue::Number(4.0).as_i64(), Some(4));
        assert_eq!(ScriptValue::Number(4.5).as_i64(), None);
        assert_eq!(i32::from_script_value(&ScriptValue::Number(-2.0)), Ok(-2));
    }
}
