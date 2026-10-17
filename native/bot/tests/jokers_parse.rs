//! Identity test for src/jokers.py parse/entry/build/unmodelled against scripts/port/fixtures/jokers_parse.py.
//! Every row must match by serde_json::to_string (key order and int/float types included).

use jevbot::{jokers, trace};
use serde_json::Value;
use std::collections::BTreeMap;

#[test]
fn jokers_parse() {
    let mut all = trace::states("bot-forecast-items", None);
    all.extend(trace::states("bot-forecast-long", None));
    let rows = trace::read_jsonl(&trace::fixtures_dir().join("jokers_parse.jsonl.gz"));
    let mut tally: BTreeMap<String, (usize, usize)> = BTreeMap::new();
    let mut bad = Vec::new();
    for r in &rows {
        let f = r["fn"].as_str().unwrap().to_string();
        let a = &r["args"];
        let got: Value = match f.as_str() {
            "parse" => jokers::parse(a["effect"].as_str().unwrap_or("")).map_or(Value::Null, |r| r.to_json()),
            "entry" => jokers::entry(&a["card"]).to_json(),
            "build" => Value::Array(jokers::build(&all[a["gs"].as_u64().unwrap() as usize], None).iter().map(|e| e.to_json()).collect()),
            _ => panic!("unknown fn {f}"),
        };
        let t = tally.entry(f.clone()).or_insert((0, 0));
        t.1 += 1;
        if got.to_string() == r["ret"].to_string() {
            t.0 += 1;
        } else if bad.len() < 5 {
            bad.push(format!("{f} {}\n  want {}\n  got  {}", trace::short(a, 300), trace::short(&r["ret"], 400), trace::short(&got, 400)));
        }
    }
    for (f, (ok, n)) in &tally {
        println!("{f}: {ok}/{n} identical");
    }
    let ok: usize = tally.values().map(|t| t.0).sum();
    let n: usize = tally.values().map(|t| t.1).sum();
    println!("total: {ok}/{n} identical");
    for b in &bad {
        println!("{b}");
    }
    assert!(bad.is_empty() && ok == n, "{} mismatches", n - ok);
}
