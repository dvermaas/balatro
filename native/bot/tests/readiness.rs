//! Identity test for src/readiness.py (upcoming, base_amount, power, assess, play_mix, lasting) against
//! scripts/port/fixtures/readiness.py. Rows run in file order (play_mix has a process-wide cache).
//! Every row must match by serde_json::to_string (key order and int/float types included).

use jevbot::{readiness, trace};
use serde_json::{json, Value};
use std::collections::BTreeMap;

fn sel(all: &[Value], i: &Value) -> Value {
    all[i.as_u64().expect("gs index") as usize].clone()
}

fn run(f: &str, a: &Value, all: &[Value]) -> Value {
    let samples = || a["samples"].as_u64().unwrap_or(24) as usize;
    match f {
        "base_amount" => json!(readiness::base_amount(a["ante"].as_i64().expect("ante"))),
        "lasting" => readiness::lasting(&a["gain"], a["kind"].as_str(), &a["wall_ratio"]),
        "upcoming" => {
            let g = sel(all, &a["gs"]);
            json!(readiness::upcoming(&g, 3).into_iter().map(|(l, t)| json!([l, t])).collect::<Vec<Value>>())
        }
        "power" => {
            let g = sel(all, &a["gs"]);
            readiness::power(&g, samples()).map_or(Value::Null, |p| p.to_json())
        }
        "assess" => {
            let g = sel(all, &a["gs"]);
            readiness::assess(&g, a["hands"].as_i64(), samples()).unwrap_or(Value::Null)
        }
        "play_mix" => {
            let g = sel(all, &a["gs"]);
            readiness::play_mix(&g, 24)
        }
        other => panic!("unknown fixture fn {other}"),
    }
}

#[test]
fn readiness_identical() {
    let rows = trace::read_jsonl(&trace::fixtures_dir().join("readiness.jsonl.gz"));
    // the same order as the fixture: SHOP (both arms), then SELECTING_HAND (both arms)
    let mut all = trace::states("bot-forecast-items", Some("SHOP"));
    all.extend(trace::states("bot-forecast-long", Some("SHOP")));
    all.extend(trace::states("bot-forecast-items", Some("SELECTING_HAND")));
    all.extend(trace::states("bot-forecast-long", Some("SELECTING_HAND")));

    let mut tally: BTreeMap<String, (usize, usize)> = BTreeMap::new();
    let mut bad: Vec<String> = Vec::new();
    for r in &rows {
        let f = r["fn"].as_str().expect("fn").to_string();
        let got = run(&f, &r["args"], &all).to_string();
        let want = r["ret"].to_string();
        let t = tally.entry(f.clone()).or_insert((0, 0));
        t.1 += 1;
        if got == want {
            t.0 += 1;
        } else if bad.len() < 5 {
            bad.push(format!(
                "{f} args={} got={} want={}",
                trace::short(&r["args"], 200),
                trace::short(&serde_json::from_str(&got).unwrap(), 300),
                trace::short(&r["ret"], 300)
            ));
        }
    }
    let (mut ok, mut n) = (0, 0);
    for (f, (o, t)) in &tally {
        println!("{f}: {o}/{t} identical");
        ok += o;
        n += t;
    }
    println!("total: {ok}/{n} identical ({} gamestates)", all.len());
    for b in &bad {
        println!("MISMATCH {b}");
    }
    assert!(bad.is_empty(), "{} mismatching rows", n - ok);
}
