//! Identity test for src/tarots.py (auto_use, plan, note, cash_outcome, measured) against
//! scripts/port/fixtures/tarots.py. Every row must match by serde_json::to_string (key order and
//! int/float types included). auto_use and measured read their gamestate from trace::states by (arm, i).

use jevbot::{tarots, trace};
use serde_json::{json, Value};
use std::collections::{BTreeMap, HashMap};

#[test]
fn tarots_identical() {
    let rows = trace::read_jsonl(&trace::fixtures_dir().join("tarots.jsonl.gz"));
    let mut states: HashMap<String, Vec<Value>> = HashMap::new();
    let mut tally: BTreeMap<String, (usize, usize)> = BTreeMap::new();
    let mut bad: Vec<String> = Vec::new();
    for r in &rows {
        let f = r["fn"].as_str().expect("fn");
        assert!(r.get("raises").is_none(), "fixture row raised in Python: {}", trace::short(r, 200));
        let mut gs_of = |arm: &Value, i: &Value| -> Value {
            let arm = arm.as_str().expect("arm").to_string();
            let v = states.entry(arm.clone()).or_insert_with(|| trace::states(&arm, None));
            v[i.as_u64().expect("i") as usize].clone()
        };
        let got: Value = match f {
            "auto_use" => {
                let gs = gs_of(&r["arm"], &r["i"]);
                match tarots::auto_use(&gs) {
                    Some((m, p, reason)) => json!([m, p, reason]),
                    None => Value::Null,
                }
            }
            "plan" => {
                let hand = r["hand"].as_array().expect("hand");
                match tarots::plan(r["key"].as_str().expect("key"), hand, &r["b"]) {
                    Some((pick, text)) => json!([pick, text]),
                    None => Value::Null,
                }
            }
            "note" => match tarots::note(&r["c"], &r["b"], &r["money"], as_slice(&r["jokers"])) {
                Some(s) => json!(s),
                None => Value::Null,
            },
            "cash_outcome" => tarots::cash_outcome(&r["c"], &r["money"], &r["cost"], as_slice(&r["jokers"]))
                .unwrap_or(Value::Null),
            "measured" => {
                let gs = gs_of(&r["arm"], &r["i"]);
                match tarots::measured(&r["c"], &gs, as_slice(&r["recent"]), &r["b"]) {
                    Some((text, per_hand)) => json!([text, per_hand]),
                    None => Value::Null,
                }
            }
            other => panic!("unknown fn {other}"),
        };
        let ok = serde_json::to_string(&got).unwrap() == serde_json::to_string(&r["ret"]).unwrap();
        let t = tally.entry(f.to_string()).or_insert((0, 0));
        t.1 += 1;
        if ok {
            t.0 += 1;
        } else if bad.len() < 5 {
            bad.push(format!("{f} row={} got={} want={}", trace::short(r, 300),
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

fn as_slice(v: &Value) -> &[Value] {
    v.as_array().map_or(&[], Vec::as_slice)
}

