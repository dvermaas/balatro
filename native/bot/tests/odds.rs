//! odds.rs against runs/_port/fixtures/odds.jsonl.gz (written by scripts/port/fixtures/odds.py).
//! Rows compare as serialized JSON, so float bits and int/float types are checked too.

use jevbot::{gs, hands, jokers, odds, trace};
use serde_json::{json, Value};
use std::collections::BTreeMap;

fn infos(g: &Value, name: &str) -> Vec<hands::Info> {
    gs::cards_of(g, name).iter().map(hands::card_info).collect()
}

fn ints(v: &Value) -> Vec<usize> {
    v.as_array().expect("list").iter().map(|x| x.as_u64().expect("int") as usize).collect()
}

fn run(f: &str, a: &Value, all: &[Value]) -> Value {
    match f {
        "forecast" => {
            let g = &all[a["gs"].as_u64().expect("gs") as usize];
            let boss = a["boss"].as_str().expect("boss");
            let hand = gs::cards_of(g, "hand");
            let hi = infos(g, "hand");
            let deck = infos(g, "cards");
            let raw = gs::d(gs::get(g, "hands")).clone();
            let levels = hands::_boss_levels(&raw, boss);
            let allowed = hands::BossAllow::new(&raw, boss);
            let mut plans: Vec<Value> = hands::discard_candidates(hand, 5).iter().map(|p| p.to_json()).collect();
            let (hand_size, hand_len) = (a["hand_size"].as_i64().expect("hs"), a["hand_len"].as_i64().expect("hl"));
            let serpent = boss == "The Serpent";
            let draw = |n: usize| -> i64 { if serpent { 3 } else { 0i64.max(hand_size - hand_len + n as i64) } };
            // decisions.selecting_hand: jokers.Scorer(levels, jokers.build(gs), gs["round"]) per call
            let factory = |_: &[hands::Info], _: usize| {
                jokers::Scorer::new(levels.clone(), jokers::build(g, None), jokers::Rnd::from_value(gs::get(g, "round")))
            };
            odds::forecast(
                &mut plans,
                &hi,
                &deck,
                &factory,
                hands::Allowed::Boss(&allowed),
                a["need"].as_f64().expect("need"),
                a["pace"].as_f64().expect("pace"),
                odds::DrawCount::Func(&draw),
                a["samples"].as_u64().expect("samples") as usize,
            );
            Value::Array(plans)
        }
        "annotate" => {
            let g = &all[a["gs"].as_u64().expect("gs") as usize];
            let boss = a["boss"].as_str().expect("boss");
            let hi = infos(g, "hand");
            let deck = infos(g, "cards");
            let levels = hands::_boss_levels(gs::d(gs::get(g, "hands")), boss);
            let sc = jokers::Scorer::new(levels, jokers::build(g, None), jokers::Rnd::from_value(gs::get(g, "round")));
            let mut plans: Vec<Value> = hands::discard_candidates(gs::cards_of(g, "hand"), 5).iter().map(|p| p.to_json()).collect();
            odds::annotate(&mut plans, &hi, &deck, &|cards: &[hands::Info]| {
                hands::best_score(cards, hands::ScorerArg::Jokers(&sc), hands::Allowed::None)
            });
            Value::Array(plans)
        }
        "arg_window" => {
            let g = &all[a["gs"].as_u64().expect("gs") as usize];
            let hi = infos(g, "hand");
            let missing: Vec<String> = a["missing"].as_array().expect("missing").iter().map(|x| x.as_str().expect("rank").to_string()).collect();
            json!(odds::arg_window(&missing, &hi, &ints(&a["keep"])))
        }
        "p_at_least" => {
            let g = |k: &str| a[k].as_u64().expect("int");
            json!(odds::p_at_least(g("n"), g("k"), g("d"), a["need"].as_i64().expect("need")))
        }
        "p_all_ranks" => {
            let counts: Vec<u64> = a["counts"].as_array().expect("counts").iter().map(|x| x.as_u64().expect("int")).collect();
            json!(odds::p_all_ranks(a["n"].as_u64().expect("n"), &counts, a["d"].as_u64().expect("d")))
        }
        other => panic!("unknown fn {other}"),
    }
}

#[test]
fn odds_identical() {
    let rows = trace::read_jsonl(&trace::fixtures_dir().join("odds.jsonl.gz"));
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
    assert!(bad.is_empty(), "mismatches:\n{}", bad.join("\n"));
}
