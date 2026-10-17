//! Reference traces and fixtures recorded from the v27 Python bot (by the former scripts/port tools; the
//! recordings stay valid without the Python code), and their paths.

use flate2::read::GzDecoder;
use serde_json::Value;
use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};

/// Where the traces and fixtures live (they are not in git): `$JEV_PORT_DATA`, else this repo's
/// `runs/_port`, else `runs/_port` of the port worktree they were recorded in.
pub fn data_dir() -> PathBuf {
    if let Some(d) = std::env::var_os("JEV_PORT_DATA") {
        return PathBuf::from(d);
    }
    let here = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../runs/_port");
    if here.is_dir() {
        return here;
    }
    PathBuf::from("C:/Users/admin/Documents/Repos/balatro-rust/runs/_port")
}

pub fn fixtures_dir() -> PathBuf {
    data_dir().join("fixtures")
}

/// All trace files of one lab arm ("bot-forecast-items" = F, "bot-forecast-long" = E), sorted.
pub fn trace_files(arm: &str) -> Vec<PathBuf> {
    let mut v: Vec<PathBuf> = std::fs::read_dir(data_dir().join("traces").join(arm))
        .map(|rd| rd.filter_map(|e| e.ok().map(|e| e.path())).collect())
        .unwrap_or_default();
    v.retain(|p| p.to_string_lossy().ends_with(".jsonl.gz"));
    v.sort();
    v
}

/// Read a .jsonl or .jsonl.gz file into JSON values, one per line.
pub fn read_jsonl(path: &Path) -> Vec<Value> {
    let f = std::fs::File::open(path).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
    let r: Box<dyn BufRead> = if path.to_string_lossy().ends_with(".gz") {
        Box::new(BufReader::new(GzDecoder::new(f)))
    } else {
        Box::new(BufReader::new(f))
    };
    r.lines()
        .map(|l| l.expect("read line"))
        .filter(|l| !l.trim().is_empty())
        .map(|l| serde_json::from_str(&l).expect("json line"))
        .collect()
}

/// A trace with `{"same": true}` call results expanded: each call record gets its full result in
/// "ok" (or keeps "err").
pub fn load_trace(path: &Path) -> Vec<Value> {
    let mut out = read_jsonl(path);
    let mut last: Option<Value> = None;
    for r in out.iter_mut() {
        if r.get("k").and_then(Value::as_str) != Some("call") {
            continue;
        }
        if r.get("same").is_some() {
            let v = last.clone().expect("'same' before any result");
            let m = r.as_object_mut().unwrap();
            m.remove("same");
            m.insert("ok".into(), v);
        } else if let Some(v) = r.get("ok") {
            last = Some(v.clone());
        } else {
            last = None;
        }
    }
    out
}

/// Every distinct gamestate in the traces of `arm` (call results), in trace order, optionally only
/// those whose "state" is `state`. Unit fixtures are generated from these same states in Python.
pub fn states(arm: &str, state: Option<&str>) -> Vec<Value> {
    let mut out = Vec::new();
    for p in trace_files(arm) {
        let mut prev: Option<Value> = None;
        for r in read_jsonl(&p) {
            if r.get("k").and_then(Value::as_str) != Some("call") {
                continue;
            }
            if let Some(gs) = r.get("ok") {
                if prev.as_ref() != Some(gs)
                    && state.map_or(true, |s| gs.get("state").and_then(Value::as_str) == Some(s))
                {
                    out.push(gs.clone());
                }
                prev = Some(gs.clone());
            }
        }
    }
    out
}

/// Compact JSON for a mismatch message: the first `max` chars.
pub fn short(v: &Value, max: usize) -> String {
    let s = v.to_string();
    if s.len() <= max { s } else { format!("{}...", &s[..s.char_indices().take_while(|(i, _)| *i < max).last().map_or(0, |(i, _)| i)]) }
}
