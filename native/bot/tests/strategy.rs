//! Identity test for src/prompts.py and src/strategy.py against scripts/port/fixtures/strategy.py.
//! Every row must match by serde_json::to_string (key order and int/float types included).

use jevbot::{prompts, strategy, trace};
use serde_json::{json, Value};
use std::collections::BTreeMap;

fn run(f: &str, a: &Value, all: &[Value]) -> Value {
    let gs = |a: &Value| &all[a["gs"].as_u64().expect("gs index") as usize];
    match f {
        "prompts.VERSION_NOTE" => json!(prompts::VERSION_NOTE),
        "prompts.GAME_RULES" => json!(prompts::GAME_RULES),
        "prompts.CASH_ITEM_QUESTION" => json!(prompts::CASH_ITEM_QUESTION),
        "prompts.CASH_STEP_QUESTION" => json!(prompts::CASH_STEP_QUESTION),
        "prompts.INSTRUCTIONS" => prompts::pairs_json(prompts::INSTRUCTIONS),
        "prompts.OPTIONS" => prompts::pairs_json(prompts::OPTIONS),
        "prompts.PLAY_NOW_CRITERIA" => prompts::pairs_json(prompts::PLAY_NOW_CRITERIA),
        "prompts.ITEM_VALUE_LEVELS" => json!(prompts::ITEM_VALUE_LEVELS),
        "HAND_NAMES" => json!(strategy::HAND_NAMES),
        "CONTAINED_IN" => strategy::contained_in_json(),
        "COPIERS" => json!(strategy::COPIERS),
        "joker_kind" => json!(strategy::joker_kind(&a["effect"])),
        "main_hands" => {
            let n = a.get("n").and_then(Value::as_u64).unwrap_or(2) as usize;
            json!(strategy::main_hands(gs(a), n))
        }
        "engine" => strategy::engine(a["effects"].as_array().expect("effects")),
        "joker_tags" => {
            let mains: Vec<String> = a["mains"]
                .as_array()
                .expect("mains")
                .iter()
                .map(|m| m.as_str().expect("main hand").to_string())
                .collect();
            json!(strategy::joker_tags(&a["effect"], &mains))
        }
        "interest" => json!(strategy::interest(&a["money"])),
        "_cards" => json!(strategy::_cards(gs(a), a["area"].as_str().expect("area"))),
        "auto_action" => match strategy::auto_action(gs(a)) {
            Some((m, p, r)) => json!([m, p, r]),
            None => Value::Null,
        },
        other => panic!("unknown fixture fn {other}"),
    }
}

#[test]
fn strategy_identical() {
    let rows = trace::read_jsonl(&trace::fixtures_dir().join("strategy.jsonl.gz"));
    let mut all = trace::states("bot-forecast-items", None);
    all.extend(trace::states("bot-forecast-long", None));

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
