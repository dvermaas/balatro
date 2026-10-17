//! Identity test for src/jokers.py play_forecast_supported, after_discard, after_play, sample_plays, gain,
//! planet_level against scripts/port/fixtures/jokers_rest.py. Every row must match: JSON by
//! serde_json::to_string (key order and int/float types included), gain by f64 bits.

use jevbot::{hands, jokers, trace};
use serde_json::{json, Value};
use std::collections::BTreeMap;

fn entries(v: Vec<jokers::JokerEntry>) -> Value {
    Value::Array(v.iter().map(|e| e.to_json()).collect())
}



#[test]
fn jokers_rest() {
    let mut all = trace::states("bot-forecast-items", None);
    all.extend(trace::states("bot-forecast-long", None));
    let rows = trace::read_jsonl(&trace::fixtures_dir().join("jokers_rest.jsonl.gz"));
    let mut tally: BTreeMap<String, (usize, usize)> = BTreeMap::new();
    let mut bad = Vec::new();
    for r in &rows {
        let f = r["fn"].as_str().unwrap().to_string();
        let a = &r["args"];
        let exp = &r["ret"];
        let ok = match f.as_str() {
            "gain" => {
                let add = if a["add"].is_null() { None } else { Some(&a["add"]) };
                let remove = a["remove"].as_u64().map(|x| x as usize);
                let lv = a["level_up"].as_array().map(|x| (x[0].as_str().unwrap().to_string(), x[1].as_i64().unwrap(), x[2].as_i64().unwrap()));
                let lv_ref = lv.as_ref().map(|(h, c, m)| (h.as_str(), *c, *m));
                let got = jokers::gain(&all[a["gs"].as_u64().unwrap() as usize], a["recent"].as_array().unwrap(), add, remove, lv_ref);
                match (got, exp.as_f64()) {
                    (None, _) if exp.is_null() => true,
                    (Some(g), Some(e)) => g.to_bits() == e.to_bits(),
                    _ => false,
                }
            }
            _ => {
                let got: Value = match f.as_str() {
                    "play_forecast_supported" => json!(jokers::play_forecast_supported(&all[a["gs"].as_u64().unwrap() as usize])),
                    "after_discard" => entries(jokers::after_discard(&all[a["gs"].as_u64().unwrap() as usize], a["discarded"].as_array().unwrap(), a["drawn"].as_i64().unwrap())),
                    "after_play" => entries(jokers::after_play(&all[a["gs"].as_u64().unwrap() as usize], a["drawn"].as_i64().unwrap())),
                    "planet_level" => jokers::planet_level(a["effect"].as_str().unwrap_or("")).map_or(Value::Null, |(h, c, m)| json!([h, c, m])),
                    "sample_plays" => Value::Array(
                        jokers::sample_plays(&all[a["gs"].as_u64().unwrap() as usize], a["recent"].as_array().unwrap())
                            .iter()
                            .map(|s| json!([s.infos.iter().map(|i| i.to_json()).collect::<Vec<_>>(), s.combo, s.name, s.sc, s.rnd]))
                            .collect(),
                    ),
                    _ => panic!("unknown fn {f}"),
                };
                got.to_string() == exp.to_string()
            }
        };
        let t = tally.entry(f.clone()).or_insert((0, 0));
        t.1 += 1;
        if ok {
            t.0 += 1;
        } else if bad.len() < 5 {
            bad.push(format!("{f} {}\n  want {}", trace::short(a, 300), trace::short(exp, 400)));
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
    assert_eq!(ok, n, "{} mismatches", n - ok);
}
