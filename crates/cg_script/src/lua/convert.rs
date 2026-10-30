//! Values between Lua and Rust: [`ScriptValue`]s, entities, vectors and colours.

use super::LuaEntity;
use crate::ScriptValue;
use cg_core::glam::{Vec2, vec2};
use cg_render::Color;
use cg_tiled::PropertyValue;
use mlua::prelude::*;
use std::collections::BTreeMap;

/// How deep tables may nest when they cross to Rust.
const MAX_DEPTH: u32 = 32;

pub(crate) fn to_lua(lua: &Lua, value: &ScriptValue) -> LuaResult<LuaValue> {
    Ok(match value {
        ScriptValue::Nil => LuaValue::Nil,
        ScriptValue::Bool(b) => LuaValue::Boolean(*b),
        ScriptValue::Int(n) => LuaValue::Integer(*n),
        ScriptValue::Number(n) => LuaValue::Number(*n),
        ScriptValue::String(s) => LuaValue::String(lua.create_string(s)?),
        ScriptValue::Entity(e) => LuaEntity(*e).into_lua(lua)?,
        ScriptValue::List(items) => {
            let table = lua.create_table_with_capacity(items.len(), 0)?;
            for item in items {
                table.raw_push(to_lua(lua, item)?)?;
            }
            LuaValue::Table(table)
        }
        ScriptValue::Map(map) => {
            let table = lua.create_table_with_capacity(0, map.len())?;
            for (key, item) in map {
                table.raw_set(key.as_str(), to_lua(lua, item)?)?;
            }
            LuaValue::Table(table)
        }
    })
}

pub(crate) fn from_lua(value: LuaValue) -> LuaResult<ScriptValue> {
    from_lua_at(value, 0)
}

fn from_lua_at(value: LuaValue, depth: u32) -> LuaResult<ScriptValue> {
    Ok(match value {
        LuaValue::Nil => ScriptValue::Nil,
        LuaValue::Boolean(b) => ScriptValue::Bool(b),
        LuaValue::Integer(n) => ScriptValue::Int(n),
        LuaValue::Number(n) => ScriptValue::Number(n),
        LuaValue::String(s) => ScriptValue::String(s.to_str()?.to_owned()),
        LuaValue::UserData(ud) => match ud.borrow::<LuaEntity>() {
            Ok(e) => ScriptValue::Entity(e.0),
            Err(_) => {
                return Err(LuaError::runtime(
                    "only entities can be passed, not other userdata",
                ));
            }
        },
        LuaValue::Table(table) => {
            // A script's `self` stands for its entity.
            if let Ok(LuaValue::UserData(ud)) = table.raw_get::<LuaValue>("entity")
                && let Ok(e) = ud.borrow::<LuaEntity>()
            {
                return Ok(ScriptValue::Entity(e.0));
            }
            if depth >= MAX_DEPTH {
                return Err(LuaError::runtime(
                    "tables nest too deeply (or contain themselves)",
                ));
            }
            let len = table.raw_len();
            let mut pairs = Vec::new();
            for pair in table.pairs::<LuaValue, LuaValue>() {
                pairs.push(pair?);
            }
            let is_list = pairs.len() == len
                && pairs.iter().all(
                    |(k, _)| matches!(k, LuaValue::Integer(i) if *i >= 1 && *i as usize <= len),
                );
            if is_list {
                let mut items = vec![ScriptValue::Nil; len];
                for (key, item) in pairs {
                    if let LuaValue::Integer(i) = key {
                        items[i as usize - 1] = from_lua_at(item, depth + 1)?;
                    }
                }
                ScriptValue::List(items)
            } else {
                let mut map = BTreeMap::new();
                for (key, item) in pairs {
                    let key = match key {
                        LuaValue::String(s) => s.to_str()?.to_owned(),
                        LuaValue::Integer(i) => i.to_string(),
                        LuaValue::Number(n) => n.to_string(),
                        other => {
                            return Err(LuaError::runtime(format!(
                                "table keys must be strings or numbers, not {}",
                                other.type_name()
                            )));
                        }
                    };
                    map.insert(key, from_lua_at(item, depth + 1)?);
                }
                ScriptValue::Map(map)
            }
        }
        other => {
            return Err(LuaError::runtime(format!(
                "a {} cannot be passed to the engine",
                other.type_name()
            )));
        }
    })
}

/// A Tiled property as a script value: colours as `"#rrggbbaa"` strings, object references
/// as their ids, classes as tables of their members.
pub(crate) fn property(value: &PropertyValue) -> ScriptValue {
    match value {
        PropertyValue::Bool(b) => ScriptValue::Bool(*b),
        PropertyValue::Int(n) => ScriptValue::Int((*n).into()),
        PropertyValue::Float(n) => ScriptValue::Number((*n).into()),
        PropertyValue::String(s) | PropertyValue::File(s) => ScriptValue::String(s.clone()),
        PropertyValue::Color(c) => ScriptValue::String(c.to_hex_string()),
        PropertyValue::Object(id) => ScriptValue::Int((*id).into()),
        PropertyValue::List(items) => ScriptValue::List(items.iter().map(property).collect()),
        PropertyValue::Class { properties, .. } => ScriptValue::Map(
            properties
                .iter()
                .map(|(k, v)| (k.to_owned(), property(v)))
                .collect(),
        ),
    }
}

/// A vector from `{x, y}`, `{x = .., y = ..}` or one number for both.
pub(crate) fn vector(value: &LuaValue) -> Result<Vec2, String> {
    match value {
        LuaValue::Integer(_) | LuaValue::Number(_) => {
            let n = number(value)?;
            Ok(vec2(n, n))
        }
        LuaValue::Table(t) => {
            let first: LuaValue = t.raw_get(1).unwrap_or(LuaValue::Nil);
            let (x, y): (LuaValue, LuaValue) = if first.is_nil() {
                (
                    t.raw_get("x").unwrap_or(LuaValue::Nil),
                    t.raw_get("y").unwrap_or(LuaValue::Nil),
                )
            } else {
                (first, t.raw_get(2).unwrap_or(LuaValue::Nil))
            };
            Ok(vec2(number(&x)?, number(&y)?))
        }
        other => Err(format!(
            "expected a vector {{x, y}}, got a {}",
            other.type_name()
        )),
    }
}

pub(crate) fn number(value: &LuaValue) -> Result<f32, String> {
    match *value {
        LuaValue::Integer(n) => Ok(n as f32),
        LuaValue::Number(n) => Ok(n as f32),
        ref other => Err(format!("expected a number, got a {}", other.type_name())),
    }
}

/// A colour from `0xRRGGBB`, `"#rrggbb"` / `"#rrggbbaa"`, or `{r, g, b, a}` (0 to 1, alpha
/// optional).
pub(crate) fn color(value: &LuaValue) -> Result<Color, String> {
    match value {
        LuaValue::Integer(n) => u32::try_from(*n)
            .ok()
            .filter(|n| *n <= 0xff_ffff)
            .map(Color::hex)
            .ok_or_else(|| format!("{n:#x} is not a colour 0xRRGGBB")),
        LuaValue::String(s) => {
            let s = s.to_string_lossy();
            Color::from_hex_str(&s).ok_or_else(|| format!("`{s}` is not a colour \"#rrggbb\""))
        }
        LuaValue::Table(t) => {
            let get = |i: usize, key: &str| -> Result<Option<f32>, String> {
                let v: LuaValue = t.raw_get(i).map_err(|e| e.to_string())?;
                let v = if v.is_nil() {
                    t.raw_get(key).map_err(|e| e.to_string())?
                } else {
                    v
                };
                if v.is_nil() {
                    Ok(None)
                } else {
                    number(&v).map(Some)
                }
            };
            let channel = |i, key| get(i, key)?.ok_or_else(|| format!("the colour has no `{key}`"));
            Ok(Color::rgba(
                channel(1, "r")?,
                channel(2, "g")?,
                channel(3, "b")?,
                get(4, "a")?.unwrap_or(1.0),
            ))
        }
        other => Err(format!("expected a colour, got a {}", other.type_name())),
    }
}
