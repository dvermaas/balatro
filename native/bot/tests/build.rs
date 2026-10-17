//! Identity test for src/build.py (decide, describe, card_value, fits) against
//! scripts/port/fixtures/build.py. Every row must match by serde_json::to_string (key order and
//! int/float types included).

use jevbot::{build, hands::Info, trace};
use serde_json::{json, Value};
use std::collections::BTreeMap;

#[test]
fn build_identical() {
    let rows = trace::read_jsonl(&trace::fixtures_dir().join("build.jsonl.gz"));
    let mut tally: BTreeMap<String, (usize, usize)> = BTreeMap::new();
    let mut bad: Vec<String> = Vec::new();
    for r in &rows {
        let f = r["fn"].as_str().expect("fn");
        let a = r["args"].as_array().expect("args");
        let got: Value = match f {
            "decide" => build::decide(&a[0]),
            "describe" => json!(build::describe(&a[0])),
            "card_value" => json!(build::card_value(&Info::from_json(&a[0]), &a[1])),
            "fits" => json!(build::fits(&a[0], &a[1])),
            other => panic!("unknown fn {other}"),
        };
        let ok = serde_json::to_string(&got).unwrap() == serde_json::to_string(&r["ret"]).unwrap();
        let t = tally.entry(f.to_string()).or_insert((0, 0));
        t.1 += 1;
        if ok {
            t.0 += 1;
        } else if bad.len() < 5 {
            bad.push(format!("{f} args={} got={} want={}", trace::short(&r["args"], 200),
                             trace::short(&got, 200), trace::short(&r["ret"], 200)));
        }
    }
    for (f, (ok, n)) in &tally {
        println!("{f}: {ok}/{n} identical");
    }
    for b in &bad {
        println!("MISMATCH {b}");
    }
    assert!(bad.is_empty(), "{} mismatches", bad.len());
}
