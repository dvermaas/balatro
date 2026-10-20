//! Learned P(win) of a rumbot position: per-ante logistic regressions on the first shop of each ante,
//! trained on 5,000 lab games (docs/lab-lessons.md, docs/samples/cloud-lab/vcurve.py). Since v53 every
//! forecast adds `experiment::VALUE_TAIL * P(win) * blinds left` to each rollout's blinds cleared
//! (forecast_stats::m_blinds), a value for the run after the forecast's horizon (`value-tail=<w>` overrides).

use rumbot_sim::cards::{Edition, Enhancement};
use rumbot_sim::game::{GameState, Phase};
use rumbot_sim::joker_data::joker_row;
use serde_json::Value;
use std::collections::HashMap;
use std::sync::OnceLock;

/// Every hand type, for lv_max / lv_total (the view's `hands` has all twelve).
const ALL_HANDS: [&str; 12] = ["Flush Five", "Flush House", "Five of a Kind", "Straight Flush", "Four of a Kind", "Full House",
                               "Flush", "Straight", "Three of a Kind", "Two Pair", "Pair", "High Card"];

struct AnteModel {
    mean: Vec<f64>,
    scale: Vec<f64>,
    coef: Vec<f64>,
    intercept: f64,
}

struct Model {
    keys: Vec<String>,
    hands: Vec<String>,
    n_base: usize,
    by_ante: HashMap<i32, AnteModel>,
}

fn model() -> &'static Model {
    static M: OnceLock<Model> = OnceLock::new();
    M.get_or_init(|| {
        let v: Value = serde_json::from_str(include_str!("../data/value_model.json")).expect("value_model.json");
        let strs = |k: &str| -> Vec<String> { v[k].as_array().unwrap().iter().map(|s| s.as_str().unwrap().to_string()).collect() };
        let fl = |x: &Value| -> Vec<f64> { x.as_array().unwrap().iter().map(|f| f.as_f64().unwrap()).collect() };
        let by_ante = v["models"].as_object().unwrap().iter().map(|(a, m)| {
            (a.parse().unwrap(), AnteModel { mean: fl(&m["mean"]), scale: fl(&m["scale"]), coef: fl(&m["coef"]), intercept: m["intercept"].as_f64().unwrap() })
        }).collect();
        Model { keys: strs("keys"), hands: strs("hands"), n_base: strs("features").len(), by_ante }
    })
}

/// The model's inputs, in vcurve.py's RICH order then the joker keys:
/// money (<= 200), jokers, lv_max, lv_total, negative jokers, jokers with an edition, deck size, enhanced
/// cards, hand size, each hand's level, each hand's times played.
pub fn features(g: &GameState) -> Vec<f64> {
    let m = model();
    let lv = |h: &str| simcore::hand_of(h).map_or(1, |t| g.hand_levels.level(t)) as f64;
    let pl = |h: &str| simcore::hand_of(h).map_or(0, |t| g.hand_levels.played(t)) as f64;
    let levels: Vec<f64> = ALL_HANDS.iter().map(|h| lv(h)).collect();
    let eds: Vec<Edition> = g.jokers.iter().map(|j| j.borrow().edition).collect();
    let mut x = vec![
        (g.money as f64).min(200.0),
        g.jokers.len() as f64,
        levels.iter().cloned().fold(1.0, f64::max),
        levels.iter().map(|l| l - 1.0).sum(),
        eds.iter().filter(|e| **e == Edition::Negative).count() as f64,
        eds.iter().filter(|e| **e != Edition::None).count() as f64,
        g.full_deck.len() as f64,
        g.full_deck.iter().filter(|c| c.borrow().enhancement != Enhancement::None).count() as f64,
        g.hand_size() as f64,
    ];
    x.extend(m.hands.iter().map(|h| lv(h)));
    x.extend(m.hands.iter().map(|h| pl(h)));
    debug_assert_eq!(x.len(), m.n_base);
    let owned: Vec<&str> = g.jokers.iter().map(|j| joker_row(j.borrow().spec.name).map_or("", |r| r.key)).collect();
    x.extend(m.keys.iter().map(|k| owned.contains(&k.as_str()) as i32 as f64));
    x
}

/// The same inputs from a BalatroBot-shaped shop state (jevlab `snap` + vcurve.py), for checking `features`.
pub fn features_from_view(gs_: &Value) -> Vec<f64> {
    let m = model();
    let cards = |a: &str| gs_[a]["cards"].as_array().cloned().unwrap_or_default();
    let deck: Vec<Value> = cards("cards").into_iter().chain(cards("hand")).collect();
    let hands = gs_["hands"].as_object().cloned().unwrap_or_default();
    let lv = |h: &str| hands.get(h).and_then(|v| v["level"].as_f64()).unwrap_or(1.0);
    let pl = |h: &str| hands.get(h).and_then(|v| v["played"].as_f64()).unwrap_or(0.0);
    let levels: Vec<f64> = hands.values().map(|v| v["level"].as_f64().unwrap_or(1.0)).collect();
    let js = cards("jokers");
    let ed = |c: &Value| c["modifier"]["edition"].as_str().map(String::from);
    let mut x = vec![
        gs_["money"].as_f64().unwrap_or(0.0).min(200.0),
        js.len() as f64,
        levels.iter().cloned().fold(1.0, f64::max),
        levels.iter().map(|l| l - 1.0).sum(),
        js.iter().filter(|c| ed(c).as_deref() == Some("NEGATIVE")).count() as f64,
        js.iter().filter(|c| ed(c).is_some()).count() as f64,
        deck.len() as f64,
        deck.iter().filter(|c| c["modifier"]["enhancement"].as_str().is_some_and(|e| !e.is_empty())).count() as f64,
        gs_["hand"]["limit"].as_f64().filter(|h| *h != 0.0).unwrap_or(8.0),
    ];
    x.extend(m.hands.iter().map(|h| lv(h)));
    x.extend(m.hands.iter().map(|h| pl(h)));
    let owned: Vec<&str> = js.iter().map(|c| c["key"].as_str().unwrap_or("")).collect();
    x.extend(m.keys.iter().map(|k| owned.contains(&k.as_str()) as i32 as f64));
    x
}

/// P(win) from inputs at the first shop of `ante` (clamped to the trained antes 2-8).
pub fn p_win_x(x: &[f64], ante: i32) -> f64 {
    let m = &model().by_ante[&ante.clamp(2, 8)];
    let z: f64 = x.iter().zip(&m.mean).zip(&m.scale).zip(&m.coef).map(|(((x, mu), s), c)| (x - mu) / s * c).sum::<f64>() + m.intercept;
    1.0 / (1.0 + (-z).exp())
}

/// The rollout's tail value in blinds: P(win) times the blinds left in the run, for a rollout that ended
/// alive after its horizon's last boss (rumbot has already moved to the next ante). 0 when it died or won.
pub fn tail(g: &GameState, last_cleared: bool) -> f64 {
    if !last_cleared || g.phase == Phase::Won || g.phase == Phase::GameOver || g.ante > 8 {
        return 0.0;
    }
    p_win_x(&features(g), g.ante) * (3 * (9 - g.ante)) as f64
}

/// VALUE_CHECK=1: compare `features` on a forecast's starting state with `features_from_view` on the
/// shop state it was built from (they should agree), printing any difference.
pub fn check(pub_: &Value, g: &GameState) {
    let (a, b) = (features(g), features_from_view(pub_));
    let m = model();
    let names: Vec<String> = ["money", "n_j", "lv_max", "lv_total", "neg", "ed", "deck", "enh", "hs"].iter().map(|s| s.to_string())
        .chain(m.hands.iter().map(|h| format!("lv:{h}"))).chain(m.hands.iter().map(|h| format!("pl:{h}"))).chain(m.keys.iter().cloned()).collect();
    let diff: Vec<String> = a.iter().zip(&b).zip(&names).filter(|((x, y), _)| x != y).map(|((x, y), n)| format!("{n} rumbot {x} view {y}")).collect();
    eprintln!("VALUE_CHECK ante {} {}", g.ante, if diff.is_empty() { "identical".to_string() } else { diff.join("; ") });
}
