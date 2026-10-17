//! Identity test for src/jokers.py score/Scorer and src/hands.py evaluated_plays, best_score,
//! best_play and the native score_many, against scripts/port/fixtures/scorer.py. Every row runs with
//! the native kernel on and off (hands::native::set_enabled). Rows match by serde_json::to_string.

use jevbot::hands::{self, Allowed, BossAllow, ScorerArg};
use jevbot::{gs, jokers, trace};
use serde_json::{json, Map, Value};
use std::collections::BTreeMap;

/// One fixture row's input, built as decisions.py builds it for that SELECTING_HAND state.
fn run_case(a: &Value, all: &[Value], want: &str) -> Value {
    let mut g = all[a["gs"].as_u64().unwrap() as usize].clone();
    if let Some(k) = a["suit_x"].as_u64() {
        g["hand"]["cards"][k as usize]["value"]["suit"] = json!("X");
    }
    let boss = a["boss"].as_str().unwrap();
    let infos: Vec<hands::Info> = gs::cards_of(&g, "hand").iter().map(hands::card_info).collect();
    let raw = gs::get(&g, "hands");
    let levels = hands::_boss_levels(hands::_d(raw), boss);
    let rnd = jokers::Rnd::from_value(gs::get(&g, "round"));
    let scorer = jokers::Scorer::new(levels, jokers::build(&g, None), rnd);
    let ba = (a["allowed"] == "boss").then(|| BossAllow::new(hands::_d(raw), boss));
    let allowed = match &ba {
        Some(b) => Allowed::Boss(b),
        None => Allowed::None,
    };
    let sc = ScorerArg::Jokers(&scorer);
    match want {
        "best_score" => json!(hands::best_score(&infos, sc, allowed)),
        "best_play" => {
            let (s, n) = hands::best_play(&infos, &Map::new(), sc, allowed);
            json!([s, n])
        }
        "evaluated_plays" => Value::Array(
            hands::evaluated_plays(&infos, &Map::new(), sc, allowed)
                .iter()
                .map(|p| json!([p.score, p.name, p.combo, p.scoring]))
                .collect(),
        ),
        "score_many" => {
            let plays = hands::evaluated_plays(&infos, &Map::new(), sc, allowed);
            // score() wants the scoring positions within the combo (combos are ascending)
            let sub: Vec<(Vec<usize>, &str, Vec<usize>)> = plays
                .iter()
                .take(40)
                .map(|p| {
                    let pos = (0..p.combo.len()).filter(|&j| p.scoring.contains(&p.combo[j])).collect();
                    (p.combo.clone(), p.name, pos)
                })
                .collect();
            let got = hands::native::score_many(sc, &infos, &sub).unwrap_or_else(|| {
                sub.iter().map(|(c, n, s)| scorer.call(&infos, c, n, s)).collect()
            });
            json!(got)
        }
        _ => panic!("unknown fn {want}"),
    }
}

#[test]
fn scorer() {
    let mut all = trace::states("bot-forecast-items", Some("SELECTING_HAND"));
    all.extend(trace::states("bot-forecast-long", Some("SELECTING_HAND")));
    let rows = trace::read_jsonl(&trace::fixtures_dir().join("scorer.jsonl.gz"));
    let mut tally: BTreeMap<String, (usize, usize)> = BTreeMap::new();
    let mut bad = Vec::new();
    for native_on in [true, false] {
        hands::native::set_enabled(native_on);
        let mode = if native_on { "native" } else { "python" };
        for r in &rows {
            let f = r["fn"].as_str().unwrap().to_string();
            let got = run_case(&r["args"], &all, &f);
            let t = tally.entry(format!("{f} [{mode}]")).or_insert((0, 0));
            t.1 += 1;
            if serde_json::to_string(&got).unwrap() == serde_json::to_string(&r["ret"]).unwrap() {
                t.0 += 1;
            } else if bad.len() < 5 {
                bad.push(format!(
                    "{f} [{mode}] {}\n  want {}\n  got  {}",
                    trace::short(&r["args"], 300),
                    trace::short(&r["ret"], 400),
                    trace::short(&got, 400)
                ));
            }
        }
    }
    for (k, (ok, n)) in &tally {
        println!("{k}: {ok}/{n} identical");
    }
    let ok: usize = tally.values().map(|t| t.0).sum();
    let n: usize = tally.values().map(|t| t.1).sum();
    println!("total: {ok}/{n} identical");
    for b in &bad {
        println!("{b}");
    }
    hands::native::set_enabled(true);
    assert!(bad.is_empty() && ok == n, "{} mismatches", n - ok);
}
