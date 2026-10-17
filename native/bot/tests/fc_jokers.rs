//! Identity check for src/forecast_jokers.rs against scripts/port/fixtures/fc_jokers.py: the jokers
//! of Python's describe(build_state(pub)). sort_id is a process-global counter in Python, so only
//! its step inside a row (+1 per joker) is checked; every other field must match as serialised.

use jevbot::forecast_jokers as fj;
use jevbot::trace::{fixtures_dir, read_jsonl, short};
use serde_json::Value;
use std::collections::BTreeMap;

#[test]
fn fc_jokers() {
    let rows = read_jsonl(&fixtures_dir().join("fc_jokers.jsonl.gz"));
    let mut tally: BTreeMap<String, (usize, usize)> = BTreeMap::new();
    let mut bad: Vec<String> = Vec::new();
    for r in &rows {
        let want = r["ret"].as_array().unwrap();
        let cards = r["pub"]["jokers"]["cards"].as_array().unwrap();
        let first = want[0]["sort_id"].as_u64().unwrap();
        let got = fj::joker_values_from_pub(&r["pub"], first);
        assert_eq!(got.len(), want.len());
        // the typed entry point must accept every description
        assert_eq!(fj::joker_descs_from_pub(&r["pub"]).len(), want.len());
        for (i, ((g, w), c)) in got.iter().zip(want).zip(cards).enumerate() {
            let key = c["key"].as_str().unwrap().to_string();
            let t = tally.entry(key.clone()).or_insert((0, 0));
            t.1 += 1;
            let mut w = w.clone();
            w["sort_id"] = Value::from(first + i as u64);
            if g.to_string() == w.to_string() {
                t.0 += 1;
            } else if bad.len() < 5 {
                let fields: Vec<&str> = w
                    .as_object()
                    .unwrap()
                    .keys()
                    .chain(g.as_object().unwrap().keys())
                    .filter(|k| g.get(k.as_str()).map(|x| x.to_string()) != w.get(k.as_str()).map(|x| x.to_string()))
                    .map(|k| k.as_str())
                    .collect();
                bad.push(format!(
                    "{key} fields {fields:?}\n  effect {}\n  want {}\n  got  {}",
                    short(&c["value"]["effect"], 200),
                    short(&w, 300),
                    short(g, 300)
                ));
            }
        }
    }
    let mut all_ok = true;
    for (k, (ok, n)) in &tally {
        println!("{k}: {ok}/{n} identical");
        all_ok &= ok == n;
    }
    for b in &bad {
        println!("{b}");
    }
    assert!(all_ok, "mismatches");
}
