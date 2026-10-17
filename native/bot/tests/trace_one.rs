//! Debug: replay one recorded game to call N-1 and print rumbot's scoring log for call N.
//! ARM, SEED, UPTO env.
use jevbot::engine::RumbotGame;
use jevbot::iface::Game;
use jevbot::trace::{load_trace, trace_files};
use serde_json::Value;

#[test]
fn trace_one() {
    let arm = std::env::var("ARM").unwrap_or_else(|_| "bot-forecast-items".into());
    let seed = std::env::var("SEED").unwrap_or_default();
    let upto: usize = std::env::var("UPTO").ok().and_then(|s| s.parse().ok()).unwrap_or(0);
    if seed.is_empty() { return; }
    let path = trace_files(&arm).into_iter().find(|p| p.file_name().unwrap().to_string_lossy().starts_with(&seed)).unwrap();
    let rows = load_trace(&path);
    let mut eng = RumbotGame::new();
    for (i, r) in rows.iter().filter(|r| r.get("k").and_then(Value::as_str) == Some("call")).enumerate() {
        if i == upto {
            if let Some(g) = eng.g.as_mut() { g.verbose = true; }
        }
        let m = r["method"].as_str().unwrap();
        let _ = eng.call(m, r.get("params").filter(|p| !p.is_null()));
        if i == upto { break; }
    }
}
