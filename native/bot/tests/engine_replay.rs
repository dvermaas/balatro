//! Replays the recorded games of the v27 Python bot (Jackdaw) on the rumbot-backed `Game`
//! (`jevbot::engine`) and compares every recorded result with the Rust view.
//!
//!   cargo test --release --test engine_replay -- --nocapture
//!
//! Env: `ARMS` (comma list, default "bot-forecast-items,bot-forecast-long"), `LIMIT` (games per arm),
//! `IGNORE` (comma list of path substrings to skip, indices normalised to `[]`), `STRICT=1` (assert
//! no divergence). Text fields (`*.value.effect`, `*.effect` of blinds, `*.tag_effect`) are always
//! ignored (phase 2: sim_text).

use jevbot::engine::RumbotGame;
use jevbot::iface::Game;
use jevbot::trace::{load_trace, short, trace_files};
use serde_json::Value;
use std::collections::BTreeMap;

fn norm(path: &str) -> String {
    // 12 -> []
    let mut out = String::new();
    let mut chars = path.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '[' {
            while let Some(&d) = chars.peek() {
                chars.next();
                if d == ']' {
                    break;
                }
            }
            out.push_str("[]");
        } else {
            out.push(c);
        }
    }
    out
}

fn text_field(p: &str) -> bool {
    let p = norm(p);
    p.ends_with(".value.effect") || p.ends_with(".tag_effect") || (p.starts_with(".blinds.") && p.ends_with(".effect"))
}

/// First difference (path, expected, actual), by structure: key order, int vs float, values.
fn diff(path: &str, a: &Value, b: &Value, ignore: &[String]) -> Option<(String, String, String)> {
    if text_field(path) || ignore.iter().any(|i| norm(path).contains(i.as_str())) {
        return None;
    }
    match (a, b) {
        (Value::Object(x), Value::Object(y)) => {
            let kx: Vec<&String> = x.keys().collect();
            let ky: Vec<&String> = y.keys().collect();
            for (k, v) in x {
                match y.get(k) {
                    Some(w) => {
                        if let Some(d) = diff(&format!("{path}.{k}"), v, w, ignore) {
                            return Some(d);
                        }
                    }
                    None => return Some((format!("{path}.{k}"), short(v, 80), "<missing>".into())),
                }
            }
            for (k, w) in y {
                if !x.contains_key(k) {
                    return Some((format!("{path}.{k}"), "<missing>".into(), short(w, 80)));
                }
            }
            let visible = |ks: &[&String]| -> Vec<String> {
                ks.iter().filter(|k| !ignore.iter().any(|i| norm(&format!("{path}.{k}")).contains(i.as_str()))).map(|k| k.to_string()).collect()
            };
            if visible(&kx) != visible(&ky) {
                return Some((format!("{path}{{key order}}"), format!("{:?}", kx), format!("{:?}", ky)));
            }
            None
        }
        (Value::Array(x), Value::Array(y)) => {
            if x.len() != y.len() {
                return Some((format!("{path}.len"), x.len().to_string(), y.len().to_string()));
            }
            for (i, (v, w)) in x.iter().zip(y).enumerate() {
                if let Some(d) = diff(&format!("{path}[{i}]"), v, w, ignore) {
                    return Some(d);
                }
            }
            None
        }
        (Value::Number(x), Value::Number(y)) => {
            if x.is_f64() != y.is_f64() || x != y {
                Some((path.into(), x.to_string(), y.to_string()))
            } else {
                None
            }
        }
        _ => {
            if a == b {
                None
            } else {
                Some((path.into(), short(a, 80), short(b, 80)))
            }
        }
    }
}

#[test]
fn engine_replay() {
    let arms: Vec<String> = std::env::var("ARMS")
        .unwrap_or_else(|_| "bot-forecast-items,bot-forecast-long".into())
        .split(',')
        .map(String::from)
        .collect();
    let limit: usize = std::env::var("LIMIT").ok().and_then(|s| s.parse().ok()).unwrap_or(usize::MAX);
    let ignore: Vec<String> = std::env::var("IGNORE")
        .unwrap_or_default()
        .split(',')
        .filter(|s| !s.is_empty())
        .map(String::from)
        .collect();
    let mut classes: BTreeMap<String, Vec<String>> = BTreeMap::new();
    let (mut games, mut clean, mut total_calls, mut total_matched) = (0, 0, 0usize, 0usize);
    for arm in &arms {
        for path in trace_files(arm).into_iter().take(limit) {
            let seed = path.file_name().unwrap().to_string_lossy().split('.').next().unwrap().to_string();
            let rows = load_trace(&path);
            let mut eng = RumbotGame::new();
            let (mut calls, mut matched) = (0usize, 0usize);
            let mut first: Option<(usize, String, String, String, String)> = None;
            for r in rows.iter().filter(|r| r.get("k").and_then(Value::as_str) == Some("call")) {
                let method = r["method"].as_str().unwrap();
                let params = r.get("params").filter(|p| !p.is_null());
                if method == "start" && calls == 0 {
                    // keep going: start is the first call
                }
                let got = eng.call(method, params);
                calls += 1;
                let desc = format!("{method} {}", params.map_or("".into(), |p| short(p, 60)));
                let bad: Option<(String, String, String)> = match (r.get("ok"), r.get("err"), &got) {
                    (Some(exp), _, Ok(v)) => diff("", exp, v, &ignore),
                    (Some(exp), _, Err(e)) => Some(("<action error>".into(), short(exp, 40), format!("{} {} {:?}", e.code, e.message, e.name))),
                    (None, Some(exp), Ok(v)) => Some(("<expected error>".into(), short(exp, 100), short(v, 40))),
                    (None, Some(exp), Err(e)) => {
                        let same = exp["code"].as_i64() == Some(e.code)
                            && exp["message"].as_str() == Some(e.message.as_str())
                            && exp["name"].as_str() == e.name.as_deref();
                        if same { None } else { Some(("<error>".into(), short(exp, 100), format!("{} {} {:?}", e.code, e.message, e.name))) }
                    }
                    _ => None,
                };
                match bad {
                    None => matched += 1,
                    Some((p, a, b)) => {
                        first = Some((calls - 1, desc, p, a, b));
                        break;
                    }
                }
            }
            games += 1;
            total_calls += calls;
            total_matched += matched;
            match first {
                None => {
                    clean += 1;
                    println!("{arm} {seed}: {matched}/{calls} calls identical (whole game)");
                }
                Some((i, desc, p, a, b)) => {
                    let total = rows.iter().filter(|r| r.get("k").and_then(Value::as_str) == Some("call")).count();
                    println!("{arm} {seed}: {matched}/{total} calls matched; call {i} [{desc}] {p}: python={a} rust={b}");
                    classes.entry(norm(&p)).or_default().push(format!("{arm}/{seed}@{i}"));
                }
            }
        }
    }
    println!("\n{clean}/{games} games replayed identically; {total_matched} calls matched before first divergence ({total_calls} replayed)");
    for (p, v) in &classes {
        println!("  {:4} x {p}   e.g. {}", v.len(), v.iter().take(2).cloned().collect::<Vec<_>>().join(", "));
    }
    if std::env::var("STRICT").is_ok() {
        assert_eq!(clean, games);
    }
}
