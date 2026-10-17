//! Identity check for src/forecast_apply.rs against scripts/port/fixtures/fc_apply.py
//! (runs/_port/fixtures/fc_apply.jsonl.gz): for every supported option of ~400 rollout.forecast pubs, the
//! Rust post-option state description equals Python's `describe(apply_option(snapshot with rng=PseudoRandom
//! ("RO000000")), rng=True)` (sort_ids relative to the first deck card), incl. the RNG pools when it drew.

use jevbot::forecast_apply::apply_option_json;
use jevbot::forecast_stats::options;
use jevbot::trace::{fixtures_dir, read_jsonl};
use serde_json::Value;
use std::collections::BTreeMap;

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
fn fc_apply_identical() {
    let rows = read_jsonl(&fixtures_dir().join("fc_apply.jsonl.gz"));
    // kind -> (n, identical, first diffs)
    let mut stats: BTreeMap<String, (usize, usize, Vec<String>)> = BTreeMap::new();
    for r in &rows {
        let key = r["key"].as_str().unwrap();
        let opts = options(&r["pub"]);
        let opt = opts.iter().find(|o| o["key"] == r["key"]).expect("option");
        let kind = if key.starts_with("replace") {
            "replace".to_string()
        } else if key.starts_with("buy_voucher") {
            format!("voucher:{}", r["pub"]["vouchers"]["cards"][key.rsplit('_').next().unwrap().parse::<usize>().unwrap()]["key"].as_str().unwrap())
        } else if key.starts_with("buy_card") {
            format!("buy:{}", opt["set"].as_str().unwrap())
        } else {
            key.to_string()
        };
        let e = stats.entry(kind.split(':').next().unwrap().to_string()).or_default();
        e.0 += 1;
        let seed = r["seed"].as_str().unwrap();
        let got = apply_option_json(&r["pub"], opt, seed);
        let res: Result<(), String> = match (got, r.get("error")) {
            (Err(_), Some(_)) => Ok(()),
            (Err(e), None) => Err(format!("rust error: {e}")),
            (Ok(_), Some(err)) => Err(format!("python refused ({err}), rust applied")),
            (Ok((d, pools)), None) => {
                let mut r1 = diff(&d, &r["desc"], "");
                if r1.is_none() {
                    let drew = r["drew"].as_bool().unwrap();
                    r1 = match (&pools, drew) {
                        (None, false) => None,
                        (Some(p), true) => {
                            let want: serde_json::Map<String, Value> = r["rng_pools"].as_object().unwrap().clone();
                            let got = serde_json::to_value(p).unwrap();
                            let got: serde_json::Map<String, Value> = got.as_object().unwrap().clone();
                            let mut dd = None;
                            for (k, v) in &want {
                                if got.get(k).map(|x| x.as_f64()) != Some(v.as_f64()) {
                                    dd = Some(format!("rng_pools.{k}: rust {:?} python {v}", got.get(k)));
                                    break;
                                }
                            }
                            dd.or_else(|| (got.len() != want.len()).then(|| format!("rng_pools: {} pools vs {}; extra in rust {:?}", got.len(), want.len(), got.keys().filter(|k| !want.contains_key(*k)).collect::<Vec<_>>())))
                        }
                        (None, true) => Some("python drew RNG, rust did not".into()),
                        (Some(_), false) => Some("rust drew RNG, python did not".into()),
                    };
                }
                r1.map_or(Ok(()), Err)
            }
        };
        match res {
            Ok(()) => e.1 += 1,
            Err(m) => {
                if e.2.len() < 4 {
                    e.2.push(format!("{key} ({kind}): {m}"));
                }
            }
        }
    }
    let mut bad = 0;
    for (k, (n, ok, ds)) in &stats {
        println!("fc_apply {k}: {ok}/{n} identical");
        for d in ds {
            println!("  {d}");
        }
        bad += n - ok;
    }
    assert_eq!(bad, 0, "{bad} mismatches");
}
