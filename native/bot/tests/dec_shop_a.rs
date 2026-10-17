//! Checks the pure helpers of decisions/shop.rs against scripts/port/fixtures/dec_shop_a.py: each row
//! holds the Python helper's output (`want`) for a SHOP state of bot-forecast-items (index `i` into
//! trace::states(arm, Some("SHOP"))) or for a synthetic input. Compared as serde JSON text, key order too.
//! score_items, shop_forecast, item_forecast and log_forecast_error need a bot and are checked by the replay.

use jevbot::decisions::shop;
use jevbot::trace;
use serde_json::{json, Map, Value};
use std::collections::BTreeMap;

fn text(v: &Value) -> String {
    serde_json::to_string(v).expect("json")
}

#[test]
fn dec_shop_a_identical() {
    let rows = trace::read_jsonl(&trace::fixtures_dir().join("dec_shop_a.jsonl.gz"));
    let shops = trace::states("bot-forecast-items", Some("SHOP"));
    let mut tally: BTreeMap<String, (usize, usize)> = BTreeMap::new();
    let mut bad: Vec<String> = Vec::new();
    for (idx, r) in rows.iter().enumerate() {
        let f = r["fn"].as_str().expect("fn").to_string();
        let gs = || &shops[r["i"].as_u64().expect("i") as usize];
        let got: Value = match f.as_str() {
            "pct" => json!(shop::pct(r["p"].as_f64().expect("p"))),
            "discard_obj" => shop::discard_obj(&r["d"]),
            "weakest_jokers" => {
                let recent: Vec<Value> = r["recent"].as_array().expect("recent").clone();
                let n = r["n"].as_u64().expect("n") as usize;
                let mut v: Vec<usize> = shop::weakest_jokers(gs(), &recent, n).into_iter().collect();
                v.sort_unstable();
                json!(v)
            }
            "economic" => json!(shop::economic(&r["obj"], &r["c"])),
            "forecast_fields" => {
                let mut m = Map::new();
                for k in r["want"].as_object().expect("want").keys() {
                    m.insert(k.clone(), shop::forecast_fields(&r["fc"], k));
                }
                Value::Object(m)
            }
            "forecast_note" => json!(shop::forecast_note(gs(), &r["fc"])),
            other => panic!("unknown fixture fn {other}"),
        };
        let entry = tally.entry(f.clone()).or_insert((0, 0));
        entry.1 += 1;
        if text(&got) == text(&r["want"]) {
            entry.0 += 1;
        } else if bad.len() < 5 {
            bad.push(format!("row {idx} {f}: got {} want {}", trace::short(&got, 300), trace::short(&r["want"], 300)));
        }
    }
    for (f, (ok, n)) in &tally {
        println!("dec_shop_a {f}: {ok}/{n} identical");
    }
    assert!(bad.is_empty(), "mismatches:\n{}", bad.join("\n"));
}
