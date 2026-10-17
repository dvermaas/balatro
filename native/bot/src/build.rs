//! Port of src/build.py. See docs/rust-port-guide.md.
//!
//! The run's committed build (v10): one target hand type (and suit for flushes) that discards, tarot
//! targets and shop choices steer toward. Re-decided at every decision from hand levels, jokers and
//! deck makeup, so it settles as the run develops.

use crate::hands::{card_info, rank_index, Info};
use crate::{gs, readiness, strategy};
use regex::Regex;
use serde_json::{json, Value};
use std::sync::OnceLock;

// build.py 15-19
const PAIR_FAMILY: &[&str] = &["Pair", "Two Pair", "Three of a Kind", "Full House", "Four of a Kind", "Five of a Kind"];
const FLUSH_FAMILY: &[&str] = &["Flush", "Straight Flush", "Flush House", "Flush Five"];
const FLUSH_WITH_SUPPORT: f64 = 0.25; // flush share that suffices when a joker makes or rewards flushes
const SUITS: &[(&str, &str)] = &[("H", "Hearts"), ("D", "Diamonds"), ("C", "Clubs"), ("S", "Spades")];

fn suit_name(letter: &str) -> &'static str {
    match letter {
        "H" => "Hearts",
        "D" => "Diamonds",
        "C" => "Clubs",
        "S" => "Spades",
        _ => panic!("KeyError: {letter:?}"),
    }
}

// `x or default` for a number field
fn or_f(v: &Value, default: f64) -> f64 {
    if gs::truthy(v) { gs::num(v) } else { default }
}

// `" ".join(_d(c.get("value")).get("effect") or "" for c in _d(gs.get("jokers")).get("cards") or [])`
fn joker_effects(g: &Value) -> String {
    let effects: Vec<&str> = gs::cards_of(g, "jokers")
        .iter()
        .map(|c| gs::s(gs::get(gs::get(c, "value"), "effect")))
        .collect();
    effects.join(" ")
}

// build.py 25-28
fn _all_cards(g: &Value) -> Vec<Info> {
    let mut cards: Vec<Info> = Vec::new();
    for area in ["cards", "hand"] {
        cards.extend(gs::cards_of(g, area).iter().map(card_info));
    }
    cards.into_iter().filter(|c| c.rank_truthy().is_some()).collect()
}

// build.py 31-53
pub fn decide(gs_: &Value) -> Value {
    let hands = gs::area(gs_, "hands");
    let effects = joker_effects(gs_);
    let deck = _all_cards(gs_);
    // Counter over suit letters (only the four SUITS keys can matter for the max)
    let mut counts = [0i64; 4];
    for c in &deck {
        if let Some(s) = c.suit.as_deref() {
            for (i, (letter, _)) in SUITS.iter().enumerate() {
                if s == *letter {
                    counts[i] += 1;
                }
            }
        }
    }
    // suit jokers (Greedy, Lusty, ...) pull the build toward their suit
    for (i, (_, name)) in SUITS.iter().enumerate() {
        counts[i] += 4 * effects.matches(&name[..name.len() - 1]).count() as i64;
    }
    // max(SUITS, key=...) returns the first maximal suit
    let mut best = 0;
    for i in 1..SUITS.len() {
        if counts[i] > counts[best] {
            best = i;
        }
    }
    let suit = SUITS[best].0;
    let mix = readiness::play_mix(gs_, 24);
    if let Some(mix) = mix.as_object().filter(|m| !m.is_empty()) {
        // v15: commit to the hand family the deck actually produces (score-weighted, jokers and levels
        // included). A suit joker or one Jupiter no longer makes a Flush build on its own.
        let mut flush = 0.0;
        let mut sets_map: Vec<(&String, f64)> = Vec::new();
        for (k, v) in mix.iter() {
            let x = gs::num(v);
            if FLUSH_FAMILY.contains(&k.as_str()) {
                flush += x;
            }
            if PAIR_FAMILY.contains(&k.as_str()) {
                sets_map.push((k, x));
            }
        }
        let mut sets = 0.0;
        for (_, x) in &sets_map {
            sets += x;
        }
        let straight = gs::num(gs::mget(mix, "Straight"));
        let high = gs::num(gs::mget(mix, "High Card"));
        let groups = [("Flush", flush), ("sets", sets), ("Straight", straight), ("High Card", high)];
        let mut top = groups[0].0;
        let mut top_v = groups[0].1;
        for (k, v) in &groups[1..] {
            if *v > top_v {
                top = *k;
                top_v = *v;
            }
        }
        static SUPPORT: OnceLock<Regex> = OnceLock::new();
        let support = SUPPORT.get_or_init(|| Regex::new("Flush|Smeared|Four Fingers").unwrap());
        if top == "Flush" || (flush >= FLUSH_WITH_SUPPORT && support.is_match(&effects)) {
            return json!({"hand": "Flush", "suit": suit});
        }
        if top == "sets" {
            let mut best: Option<(&String, f64)> = None;
            for (k, x) in &sets_map {
                match best {
                    Some((_, bx)) if *x <= bx => {}
                    _ => best = Some((*k, *x)),
                }
            }
            let (name, _) = best.expect("ValueError: max() arg is an empty sequence");
            return json!({"hand": name, "suit": Value::Null});
        }
        let hand = if top == "Straight" { "Straight" } else { "Pair" };
        return json!({"hand": hand, "suit": Value::Null});
    }
    let fl = gs::d(gs::mget(hands, "Flush"));
    static SIGNAL: OnceLock<Regex> = OnceLock::new();
    let signal = SIGNAL.get_or_init(|| Regex::new("Flush|Smeared|suit").unwrap());
    let flush_signal = (or_f(gs::mget(fl, "level"), 1.0) - 1.0)
        + or_f(gs::mget(fl, "played"), 0.0) / 3.0
        + signal.find_iter(&effects).count() as f64;
    let mains = strategy::main_hands(gs_, 2);
    let flush_mains = mains.first().map_or(false, |m| FLUSH_FAMILY.contains(&m.as_str()));
    if flush_signal >= 1.0 || flush_mains {
        return json!({"hand": "Flush", "suit": suit});
    }
    let hand = mains.first().map_or("Pair", |m| m.as_str());
    json!({"hand": hand, "suit": Value::Null})
}

// build.py 56-57
pub fn describe(b: &Value) -> String {
    let hand = gs::s(gs::get(b, "hand"));
    let suit = gs::get(b, "suit");
    if gs::truthy(suit) {
        format!("{hand} in {}", suit_name(gs::s(suit)))
    } else {
        hand.to_string()
    }
}

// build.py 60-72
pub fn card_value(info: &Info, b: &Value) -> f64 {
    if info.debuff {
        return -1.0;
    }
    let rank = match info.rank_truthy() {
        Some(r) if !info.enh_is("STONE") => r,
        _ => return 0.3,
    };
    let mut v = (rank_index(rank) + 2) as f64 / 14.0;
    let truthy_s = |o: &Option<String>| o.as_deref().map_or(false, |s| !s.is_empty());
    if truthy_s(&info.enh) || truthy_s(&info.edition) || truthy_s(&info.seal) {
        v += 0.5;
    }
    let suit = gs::get(b, "suit");
    if gs::truthy(suit) {
        v += if info.suit.as_deref() == Some(gs::s(suit)) || info.enh_is("WILD") { 2.0 } else { 0.0 };
    }
    v
}

// build.py 75-86
pub fn fits(plan_goal: &Value, b: &Value) -> bool {
    let (kind, arg) = if gs::truthy(plan_goal) {
        let a = plan_goal.as_array().map(Vec::as_slice).unwrap_or(&[]);
        (a.first().unwrap_or(&Value::Null), a.get(1).unwrap_or(&Value::Null))
    } else {
        (&Value::Null, &Value::Null)
    };
    let hand = gs::s(gs::get(b, "hand"));
    let bsuit = gs::get(b, "suit");
    if hand == "Flush" {
        return kind.as_str() == Some("flush") && arg == bsuit;
    }
    if PAIR_FAMILY.contains(&hand) {
        return kind.as_str() == Some("sets");
    }
    if hand == "Straight" {
        return kind.as_str() == Some("straight");
    }
    kind.as_str() == Some("high")
}
