//! Python dict semantics over the BalatroBot gamestate (`serde_json::Value`, insertion-ordered).
//!
//! The Python bot reads the gamestate with `_d(x).get(k)`, `x or 0`, `isinstance(x, list)`. These
//! helpers do the same so ports stay line-by-line. BalatroBot sends empty Lua tables as `[]`, so a
//! dict field can arrive as a list: `d()` treats anything that is not an object as `{}`.

use serde_json::{Map, Value};
use std::sync::OnceLock;

pub static NULL: Value = Value::Null;

fn empty_map() -> &'static Map<String, Value> {
    static M: OnceLock<Map<String, Value>> = OnceLock::new();
    M.get_or_init(Map::new)
}

/// `hands._d(x)`: x if it is a dict, else `{}`.
pub fn d(v: &Value) -> &Map<String, Value> {
    match v {
        Value::Object(m) => m,
        _ => empty_map(),
    }
}

/// `_d(v).get(k)`: `Null` when missing or when v is not a dict.
pub fn get<'a>(v: &'a Value, k: &str) -> &'a Value {
    d(v).get(k).unwrap_or(&NULL)
}

/// `m.get(k)` on a map.
pub fn mget<'a>(m: &'a Map<String, Value>, k: &str) -> &'a Value {
    m.get(k).unwrap_or(&NULL)
}

/// Python truthiness: None, False, 0, 0.0, "", [], {} are false.
pub fn truthy(v: &Value) -> bool {
    match v {
        Value::Null => false,
        Value::Bool(b) => *b,
        Value::Number(n) => n.as_f64().map_or(false, |x| x != 0.0),
        Value::String(s) => !s.is_empty(),
        Value::Array(a) => !a.is_empty(),
        Value::Object(m) => !m.is_empty(),
    }
}

/// `x or default` as a Value.
pub fn or<'a>(v: &'a Value, default: &'a Value) -> &'a Value {
    if truthy(v) { v } else { default }
}

/// `(x or 0)` as f64 (bools count as 0/1 like Python). Non-numbers count as 0.
pub fn num(v: &Value) -> f64 {
    match v {
        Value::Number(n) => n.as_f64().unwrap_or(0.0),
        Value::Bool(b) => *b as i64 as f64,
        _ => 0.0,
    }
}

/// `(x or 0)` as i64, for values that are ints in Python. Floats are truncated: only use this
/// where the Python value is an int.
pub fn int(v: &Value) -> i64 {
    match v {
        Value::Number(n) => n.as_i64().unwrap_or_else(|| n.as_f64().unwrap_or(0.0) as i64),
        Value::Bool(b) => *b as i64,
        _ => 0,
    }
}

/// True when the JSON number is an int (Python `int`), false for floats and non-numbers.
pub fn is_int(v: &Value) -> bool {
    matches!(v, Value::Number(n) if n.is_i64() || n.is_u64())
}

/// The string, or "" (`x or ""`).
pub fn s(v: &Value) -> &str {
    v.as_str().unwrap_or("")
}

/// `x if isinstance(x, list) else []`.
pub fn list(v: &Value) -> &[Value] {
    match v {
        Value::Array(a) => a,
        _ => &[],
    }
}

/// `jev_bot.area(gs, name)`: `_d(gs.get(name))`.
pub fn area<'a>(gs: &'a Value, name: &str) -> &'a Map<String, Value> {
    d(get(gs, name))
}

/// `jev_bot.cards_of(gs, name)`: the area's card list, or [].
pub fn cards_of<'a>(gs: &'a Value, name: &str) -> &'a [Value] {
    match area(gs, name).get("cards") {
        Some(Value::Array(a)) => a,
        _ => &[],
    }
}
