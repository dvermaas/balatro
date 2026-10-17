//! Identity check for src/forecast_state.rs against scripts/port/fixtures/fc_state.py
//! (runs/_port/fixtures/fc_state.jsonl.gz): the Rust description of each pub equals Python's
//! `sim_bridge.describe(rollout.build_state(pub))` (jokers removed) field by field.

use jevbot::forecast_state::{state_desc_from_pub, state_json_from_pub};
use jevbot::trace::{fixtures_dir, read_jsonl};
use serde_json::Value;

/// First differing path, or None when equal (ints and floats kept apart).
fn diff(a: &Value, b: &Value, path: &str) -> Option<String> {
    match (a, b) {
        (Value::Object(x), Value::Object(y)) => {
            for (k, v) in x {
                match y.get(k) {
                    None => return Some(format!("{path}.{k}: only in rust")),
                    Some(w) => {
                        if let Some(d) = diff(v, w, &format!("{path}.{k}")) {
                            return Some(d);
                        }
                    }
                }
            }
            y.keys().find(|k| !x.contains_key(*k)).map(|k| format!("{path}.{k}: only in python"))
        }
        (Value::Array(x), Value::Array(y)) => {
            for (i, (v, w)) in x.iter().zip(y).enumerate() {
                if let Some(d) = diff(v, w, &format!("{path}[{i}]")) {
                    return Some(d);
                }
            }
            (x.len() != y.len()).then(|| format!("{path}: len {} vs {}", x.len(), y.len()))
        }
        _ => (serde_json::to_string(a).unwrap() != serde_json::to_string(b).unwrap())
            .then(|| format!("{path}: rust {a} python {b}")),
    }
}

#[test]
fn fc_state_identical() {
    let rows = read_jsonl(&fixtures_dir().join("fc_state.jsonl.gz"));
    let mut bad = Vec::new();
    for (i, r) in rows.iter().enumerate() {
        let mut got = match state_json_from_pub(&r["pub"]) {
            Ok(v) => v,
            Err(e) => {
                bad.push(format!("row {i}: error {e}"));
                continue;
            }
        };
        let jokers = got.as_object_mut().unwrap().remove("jokers");
        assert_eq!(jokers, Some(Value::Array(vec![])));
        if let Some(d) = diff(&got, &r["desc"], "") {
            bad.push(format!("row {i}: {d}"));
        }
        // the full desc must also deserialize (jokers still empty)
        state_desc_from_pub(&r["pub"]).expect("StateDesc");
    }
    println!("fc_state: {}/{} identical", rows.len() - bad.len(), rows.len());
    for b in bad.iter().take(8) {
        println!("  {b}");
    }
    assert!(bad.is_empty(), "{} mismatches", bad.len());
}
