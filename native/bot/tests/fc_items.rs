//! End-to-end check of src/forecast_items.rs: every rollout_items.forecast oracle line of the 60 traces, Rust ret
//! (minus `seconds`) equals the recorded ret. FC_STRIDE=5 checks every 5th call.

use jevbot::forecast_items::items_forecast_clock;
use jevbot::trace::{load_trace, trace_files};
use serde_json::Value;
use std::collections::BTreeMap;

fn strip(v: &Value) -> String {
    let mut v = v.clone();
    v.as_object_mut().unwrap().remove("seconds");
    serde_json::to_string(&v).unwrap()
}

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
        _ => (serde_json::to_string(a).unwrap() != serde_json::to_string(b).unwrap()).then(|| format!("{path}: rust {a} python {b}")),
    }
}

#[test]
fn fc_items_identical() {
    let stride: usize = std::env::var("FC_STRIDE").ok().and_then(|s| s.parse().ok()).unwrap_or(1);
    let mut files = trace_files("bot-forecast-long");
    files.extend(trace_files("bot-forecast-items"));
    let (mut total, mut ok, mut idx) = (0usize, 0usize, 0usize);
    let mut classes: BTreeMap<String, (usize, Vec<String>)> = BTreeMap::new();
    for f in files {
        let lines = load_trace(&f);
        let mut last: Option<&Value> = None;
        for l in &lines {
            match l["k"].as_str() {
                Some("call") => last = l.get("ok"),
                Some("oracle") if l["fn"] == "rollout_items.forecast" => {
                    idx += 1;
                    if (idx - 1) % stride != 0 {
                        continue;
                    }
                    total += 1;
                    let pub_ = last.expect("pub");
                    let a = &l["args"];
                    let keys: Option<Vec<String>> =
                        a["keys"].as_array().map(|v| v.iter().map(|k| k.as_str().unwrap().to_string()).collect());
                    let bud = a["budget_s"].as_f64();
                    let n = if bud.is_some() { l["ret"]["rollouts"].as_u64().unwrap() } else { a["n"].as_u64().unwrap() } as usize;
                    let got = items_forecast_clock(pub_, n, a["bosses"].as_i64().unwrap(), keys.as_deref(), None, &|| 0.0);
                    if strip(&got) == strip(&l["ret"]) {
                        ok += 1;
                        continue;
                    }
                    let (mut ga, mut gb) = (got.clone(), l["ret"].clone());
                    ga.as_object_mut().unwrap().remove("seconds");
                    gb.as_object_mut().unwrap().remove("seconds");
                    let d = diff(&ga, &gb, "").unwrap_or_default();
                    if d.is_empty() && std::env::var("FC_DUMP").is_ok() {
                        println!("RUST {}
PY   {}", strip(&got), strip(&l["ret"]));
                        panic!("dump");
                    }
                    let cls = d
                        .split(['.', '['])
                        .find(|p| p.starts_with("buy_") || p.starts_with("replace") || *p == "skip")
                        .unwrap_or("other")
                        .trim_end_matches(|c: char| c.is_ascii_digit() || c == ']')
                        .to_string();
                    let e = classes.entry(cls).or_default();
                    e.0 += 1;
                    if e.1.len() < 5 {
                        e.1.push(format!("{}: {}", f.file_name().unwrap().to_string_lossy(), d.chars().take(200).collect::<String>()));
                    }
                }
                _ => {}
            }
        }
    }
    println!("{ok}/{total} identical");
    for (c, (n, ex)) in &classes {
        println!("class {c}: {n}");
        for e in ex {
            println!("   {e}");
        }
    }
    assert_eq!(ok, total);
}
