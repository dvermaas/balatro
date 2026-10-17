//! Identity test for src/hands.py (card_info, describe, classify, estimate, play_candidates,
//! discard_candidates) against scripts/port/fixtures/hands.py. Every row must match by
//! serde_json::to_string (key order and int/float types included).

use jevbot::{gs, hands, trace};
use serde_json::{json, Value};
use std::collections::BTreeMap;

fn sel(all: &[Value], i: &Value) -> Value {
    all[i.as_u64().expect("gs index") as usize].clone()
}

fn run(f: &str, a: &Value, all: &[Value]) -> Value {
    match f {
        "card_info" => {
            let g = sel(all, &a["gs"]);
            let c = &gs::cards_of(&g, "hand")[a["i"].as_u64().expect("i") as usize];
            hands::card_info(c).to_json()
        }
        "describe" => {
            if let Some(info) = a.get("info") {
                json!(hands::describe(info))
            } else {
                let g = sel(all, &a["gs"]);
                let c = &gs::cards_of(&g, "hand")[a["i"].as_u64().expect("i") as usize];
                json!(hands::describe(c))
            }
        }
        "classify" => {
            let g = sel(all, &a["gs"]);
            let hand = gs::cards_of(&g, "hand");
            let cs = combo_infos(hand, &a["combo"]);
            let (name, sc) = hands::classify(&cs);
            json!([name, sc])
        }
        "estimate" => {
            let g = sel(all, &a["gs"]);
            let hand = gs::cards_of(&g, "hand");
            let cs = combo_infos(hand, &a["combo"]);
            let scoring: Vec<usize> = a["scoring"].as_array().expect("scoring").iter()
                .map(|v| v.as_u64().expect("pos") as usize).collect();
            let levels = gs::d(gs::get(&g, "hands"));
            json!(hands::estimate(&cs, a["name"].as_str().expect("name"), &scoring, levels))
        }
        "play_candidates" => {
            let g = sel(all, &a["gs"]);
            let levels = gs::d(gs::get(&g, "hands"));
            let cands = hands::play_candidates(gs::cards_of(&g, "hand"), levels, 8,
                hands::ScorerArg::None, hands::Allowed::None);
            Value::Array(cands.iter().map(|p| p.to_json()).collect())
        }
        "discard_candidates" => {
            let g = sel(all, &a["gs"]);
            let plans = hands::discard_candidates(gs::cards_of(&g, "hand"), 5);
            Value::Array(plans.iter().map(|p| p.to_json()).collect())
        }
        other => panic!("unknown fixture fn {other}"),
    }
}

fn combo_infos(hand: &[Value], combo: &Value) -> Vec<hands::Info> {
    combo.as_array().expect("combo").iter()
        .map(|i| hands::card_info(&hand[i.as_u64().expect("pos") as usize]))
        .collect()
}

#[test]
fn hands_identical() {
    let rows = trace::read_jsonl(&trace::fixtures_dir().join("hands.jsonl.gz"));
    let mut all = trace::states("bot-forecast-items", Some("SELECTING_HAND"));
    all.extend(trace::states("bot-forecast-long", Some("SELECTING_HAND")));

    let mut tally: BTreeMap<String, (usize, usize)> = BTreeMap::new();
    let mut bad: Vec<String> = Vec::new();
    for r in &rows {
        let f = r["fn"].as_str().expect("fn").to_string();
        let got = run(&f, &r["args"], &all);
        let (g, w) = (got.to_string(), r["ret"].to_string());
        let t = tally.entry(f.clone()).or_insert((0, 0));
        t.1 += 1;
        if g == w {
            t.0 += 1;
        } else if bad.len() < 5 {
            bad.push(format!(
                "{f} {}\n  want {}\n  got  {}",
                trace::short(&r["args"], 200),
                trace::short(&r["ret"], 300),
                trace::short(&got, 300)
            ));
        }
    }
    for (f, (ok, n)) in &tally {
        println!("{f}: {ok}/{n} identical");
    }
    let total_ok: usize = tally.values().map(|t| t.0).sum();
    let total: usize = tally.values().map(|t| t.1).sum();
    println!("total: {total_ok}/{total} identical ({} gamestates)", all.len());
    for b in &bad {
        println!("{b}");
    }
    assert!(bad.is_empty() && total_ok == total, "{} mismatches", total - total_ok);
}
