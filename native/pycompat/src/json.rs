//! `json.dumps(v)` with Python's defaults (separators ", " and ": ", ensure_ascii=True, float repr
//! via `fmt::pyfloat`, key order as stored). Used where the Python bot puts a dumps() string into
//! text or uses it as a key.

use serde_json::Value;

pub fn dumps(v: &Value) -> String {
    let _ = v;
    todo!("pycompat::json::dumps")
}
