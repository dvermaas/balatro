//! Port of src/tarots.py. See docs/rust-port-guide.md.
//!
//! Tarot and spectral use (v10): code knows each card's mechanics and picks targets that fit the build.
//! Jev decides what to buy or take (with the plan text as a fact); code decides when and on which
//! cards to use them.
//!
//! Python dict results are `serde_json::Value` with keys in insertion order. Python's int/float
//! arithmetic on money and sell values is kept by `py_bin`, `py_min` and `py_max`.

use crate::build;
use crate::gs;
use crate::hands::{self, card_info, describe, Info, Levels};
use crate::jokers;
use pycompat::{pyfloat, pyround_int};
use serde_json::{json, Value};

// tarots.py 11-25: TARGETED[key] = (kind, count, arg). Kinds: enhance, suit, rank_up, remove, copy, best, held.
// Untargeted keys are not in this table.
pub fn targeted(key: &str) -> Option<(&'static str, usize, Option<&'static str>)> {
    Some(match key {
        "c_magician" => ("enhance", 2, Some("LUCKY")),
        "c_empress" => ("enhance", 2, Some("MULT")),
        "c_heirophant" => ("enhance", 2, Some("BONUS")),
        "c_lovers" => ("enhance", 1, Some("WILD")),
        "c_chariot" => ("enhance", 1, Some("STEEL")),
        "c_justice" => ("enhance", 1, Some("GLASS")),
        "c_devil" => ("enhance", 1, Some("GOLD")),
        "c_tower" => ("enhance", 1, Some("STONE")),
        "c_strength" => ("rank_up", 2, None),
        "c_hanged_man" => ("remove", 2, None),
        "c_death" => ("copy", 2, None),
        "c_star" => ("suit", 3, Some("D")),
        "c_moon" => ("suit", 3, Some("C")),
        "c_sun" => ("suit", 3, Some("H")),
        "c_world" => ("suit", 3, Some("S")),
        // spectrals that target one card
        "c_aura" => ("best", 1, Some("edition")),
        "c_deja_vu" => ("best", 1, Some("red seal")),
        "c_talisman" => ("held", 1, Some("gold seal")),
        "c_medium" => ("best", 1, Some("purple seal")),
        "c_trance" => ("best", 1, Some("blue seal")),
        "c_cryptid" => ("best", 1, Some("2 copies")),
        _ => return None,
    })
}

// tarots.py 26-28
fn enhance_text(arg: &str) -> &'static str {
    match arg {
        "LUCKY" => "Lucky (chance of +20 Mult or $20)",
        "MULT" => "Mult cards (+4 Mult when scored)",
        "BONUS" => "Bonus cards (+30 Chips when scored)",
        "WILD" => "Wild (counts as every suit)",
        "STEEL" => "Steel (x1.5 Mult while held in hand)",
        "GLASS" => "Glass (x2 Mult when scored, may break)",
        "GOLD" => "Gold ($3 if held at end of round)",
        "STONE" => "Stone (+50 Chips, no rank or suit)",
        _ => panic!("KeyError: {arg:?}"),
    }
}

// tarots.py 30: enhancements that pay off when the card is held rather than played go on low-value cards.
fn held_enh(arg: &str) -> bool {
    matches!(arg, "STEEL" | "GOLD")
}

// tarots.py 32-33: untargeted cards that are always worth using right away (when the condition holds).
pub fn safe_untargeted(key: &str) -> bool {
    matches!(key, "c_hermit" | "c_temperance" | "c_high_priestess" | "c_emperor" | "c_judgement"
        | "c_wheel_of_fortune" | "c_fool" | "c_black_hole")
}

// Python falsy string or None (`not i["enh"]`).
fn falsy_str(o: &Option<String>) -> bool {
    o.as_deref().map_or(true, str::is_empty)
}

// Python arithmetic on JSON numbers: int op int stays int, anything with a float is float.
fn py_bin(a: &Value, b: &Value, op: char) -> Value {
    match (a.as_i64(), b.as_i64()) {
        (Some(x), Some(y)) => json!(if op == '+' { x + y } else { x - y }),
        _ => {
            let (x, y) = (gs::num(a), gs::num(b));
            json!(if op == '+' { x + y } else { x - y })
        }
    }
}

fn py_lt(a: &Value, b: &Value) -> bool {
    match (a.as_i64(), b.as_i64()) {
        (Some(x), Some(y)) => x < y,
        _ => gs::num(a) < gs::num(b),
    }
}

// Python min(a, b) / max(a, b): the first argument unless the second is strictly smaller / larger.
fn py_min(a: &Value, b: &Value) -> Value {
    if py_lt(b, a) { b.clone() } else { a.clone() }
}

fn py_max(a: &Value, b: &Value) -> Value {
    if py_lt(a, b) { b.clone() } else { a.clone() }
}

// Python str() of an int or float value, as in an f-string.
fn py_str(v: &Value) -> String {
    match v.as_i64() {
        Some(i) => i.to_string(),
        None => pyfloat(gs::num(v)),
    }
}

// tarots.py 47-100
pub fn plan(key: &str, hand_cards: &[Value], b: &Value) -> Option<(Vec<usize>, String)> {
    let (kind, n, arg) = targeted(key)?;
    let infos: Vec<Info> = hand_cards.iter().map(card_info).collect();
    if infos.len() < if kind == "copy" { 2 } else { 1 } {
        return None;
    }
    // val: card value per index, None for debuffed cards (Python omits them from the dict)
    let val: Vec<Option<f64>> = infos
        .iter()
        .map(|i| if i.debuff { None } else { Some(build::card_value(i, b)) })
        .collect();
    let keys: Vec<usize> = (0..infos.len()).filter(|&i| val[i].is_some()).collect();
    let vv = |i: usize| val[i].unwrap();
    // best first; a stable sort on the value keeps index order for ties, as sorted(key=-v) does
    let mut order = keys.clone();
    order.sort_by(|&a, &b| vv(b).partial_cmp(&vv(a)).unwrap());
    let name = |idx: &[usize]| idx.iter().map(|&i| describe(&hand_cards[i])).collect::<Vec<_>>().join(", ");
    let arg = arg.unwrap_or("");
    if kind == "enhance" {
        let fresh: Vec<usize> = order.iter().copied().filter(|&i| falsy_str(&infos[i].enh)).collect();
        let pick: Vec<usize> = if arg == "WILD" {
            if !gs::truthy(gs::get(b, "suit")) {
                return None; // Wild only matters for flushes
            }
            fresh.iter().copied().filter(|&i| infos[i].suit.as_deref() != gs::get(b, "suit").as_str()).take(n).collect()
        } else if held_enh(arg) || arg == "STONE" {
            fresh.iter().rev().copied().take(n).collect()
        } else {
            fresh.iter().copied().take(n).collect()
        };
        return if pick.is_empty() {
            None
        } else {
            let text = format!("turns {} into {}", name(&pick), enhance_text(arg));
            Some((pick, text))
        };
    }
    if kind == "suit" {
        let off: Vec<usize> = order
            .iter()
            .rev()
            .copied()
            .filter(|&i| infos[i].suit.as_deref() != Some(arg) && !matches!(infos[i].enh.as_deref(), Some("WILD") | Some("STONE")))
            .collect();
        if gs::truthy(gs::get(b, "suit")) && gs::get(b, "suit").as_str() != Some(arg) {
            return None; // would pull cards away from our flush suit
        }
        let pick: Vec<usize> = off.into_iter().take(n).collect();
        return if pick.is_empty() {
            None
        } else {
            let text = format!("converts {} to {}", name(&pick), hands::suit_name(arg));
            Some((pick, text))
        };
    }
    if kind == "remove" {
        let pick: Vec<usize> = order.iter().rev().copied().take(n).collect();
        let text = format!("destroys {} (thins the deck of junk)", name(&pick));
        return Some((pick, text));
    }
    if kind == "rank_up" {
        let pick: Vec<usize> = order
            .iter()
            .copied()
            .filter(|&i| matches!(infos[i].rank_truthy(), Some(r) if r != "A"))
            .take(n)
            .collect();
        return if pick.is_empty() {
            None
        } else {
            let text = format!("raises the rank of {} by one", name(&pick));
            Some((pick, text))
        };
    }
    if kind == "copy" {
        // Death copies the rightmost selected card onto the other: needs a good card right of a bad one.
        // max(... key=...) returns the first maximal pair; pairs come in index order.
        let mut best: Option<(usize, usize)> = None;
        let mut best_key = 0.0;
        for &i in &keys {
            for &j in &keys {
                if i < j {
                    let k = vv(j) - vv(i);
                    if best.is_none() || k > best_key {
                        best = Some((i, j));
                        best_key = k;
                    }
                }
            }
        }
        let (bi, bj) = best?;
        if vv(bj) - vv(bi) < 0.3 {
            return None;
        }
        let text = format!("turns {} into a copy of {}", name(&[bi]), name(&[bj]));
        return Some((vec![bi, bj], text));
    }
    if kind == "best" {
        let i = *order.first().expect("IndexError: list index out of range");
        return Some((vec![i], format!("adds {arg} to {}", name(&[i]))));
    }
    if kind == "held" {
        let i = *order.last().expect("IndexError: list index out of range");
        return Some((vec![i], format!("adds {arg} to {}", name(&[i]))));
    }
    None
}

// tarots.py 102-115. `cost` is the price paid (0 when free); `jokers` is the joker card list.
pub fn cash_outcome(c: &Value, money: &Value, cost: &Value, jokers: &[Value]) -> Option<Value> {
    let key = gs::s(gs::get(c, "key"));
    let left = py_bin(money, cost, '-');
    let payout = if key == "c_hermit" {
        py_max(&json!(0), &py_min(&left, &json!(20)))
    } else if key == "c_temperance" {
        let total = sum_sells(jokers);
        py_max(&json!(0), &py_min(&total, &json!(50)))
    } else {
        return None;
    };
    Some(json!({
        "cash_before": money,
        "purchase_cost": cost,
        "cash_after_purchase": left,
        "payout_if_used_now": payout,
        "net_cash_gain": py_bin(&payout, cost, '-'),
        "cash_after_use": py_bin(&left, &payout, '+'),
    }))
}

// sum(_d(j.get("cost")).get("sell", 0) for j in jokers): Python sum starts at int 0.
fn sum_sells(jokers: &[Value]) -> Value {
    jokers.iter().fold(json!(0), |acc, j| {
        let v = gs::mget(gs::d(gs::get(j, "cost")), "sell");
        let sell = if v.is_null() { json!(0) } else { v.clone() };
        py_bin(&acc, &sell, '+')
    })
}

// tarots.py 118-140: short fact about what a tarot/spectral would do for us, for Jev's item scoring.
pub fn note(c: &Value, b: &Value, money: &Value, jokers: &[Value]) -> Option<String> {
    let key = gs::s(gs::get(c, "key"));
    if let Some((kind, n, arg)) = targeted(key) {
        let arg = arg.unwrap_or("");
        if kind == "suit" {
            if gs::truthy(gs::get(b, "suit")) && gs::get(b, "suit").as_str() != Some(arg) {
                return Some(format!("converts cards to {}, against our {} build", hands::suit_name(arg), build::describe(b)));
            }
            let flush = if gs::truthy(gs::get(b, "suit")) { ", feeding our flush" } else { "" };
            return Some(format!("converts up to 3 cards to {}{flush}", hands::suit_name(arg)));
        }
        if kind == "enhance" {
            return Some(if !held_enh(arg) {
                format!("makes {n} of our best cards {}", enhance_text(arg))
            } else {
                format!("makes {n} low card {}", enhance_text(arg))
            });
        }
        return Some(match kind {
            "remove" => "removes 2 junk cards from the deck".to_string(),
            "rank_up" => "raises 2 cards by one rank".to_string(),
            "copy" => "copies a strong card over a weak one".to_string(),
            "best" => format!("adds {arg} to our best card"),
            "held" => format!("adds {arg} to a card we hold"),
            _ => panic!("KeyError: {kind:?}"),
        });
    }
    if key == "c_hermit" {
        return Some(format!("gives +${} now (doubles money, max $20)", py_str(&py_max(&json!(0), &py_min(money, &json!(20))))));
    }
    if key == "c_temperance" {
        let total = sum_sells(jokers);
        return Some(format!("gives +${} now (our jokers' sell value)", py_str(&py_min(&total, &json!(50)))));
    }
    None
}

// tarots.py 143-172: (method, params, reason) to use a held consumable now, or None. Untargeted ones in
// the shop or blind select; targeted ones while a hand is showing (they need cards selected).
pub fn auto_use(gs_: &Value) -> Option<(&'static str, Value, String)> {
    let state = gs::s(gs::get(gs_, "state"));
    let cons_area = gs::area(gs_, "consumables");
    let cons = gs::cards_of(gs_, "consumables");
    let jok = gs::area(gs_, "jokers");
    let jok_limit = gs::mget(jok, "limit");
    let jokers_free = (if gs::truthy(jok_limit) { gs::int(jok_limit) } else { 5 }) - gs::int(gs::mget(jok, "count"));
    let cons_limit = gs::mget(cons_area, "limit");
    let cons_free = (if gs::truthy(cons_limit) { gs::int(cons_limit) } else { 2 }) - cons.len() as i64;
    let mut b: Option<Value> = None; // build.decide only when a targeted card needs a plan
    for (i, c) in cons.iter().enumerate() {
        let key = gs::s(gs::get(c, "key"));
        let label = if gs::truthy(gs::get(c, "label")) { gs::s(gs::get(c, "label")) } else { key };
        if safe_untargeted(key) && matches!(state, "SHOP" | "BLIND_SELECT" | "SELECTING_HAND") {
            if key == "c_hermit" && gs::num(gs::get(gs_, "money")) < 4.0 {
                continue;
            }
            if key == "c_judgement" && jokers_free < 1 {
                continue;
            }
            if key == "c_wheel_of_fortune" && !gs::truthy(gs::mget(jok, "count")) {
                continue;
            }
            if matches!(key, "c_high_priestess" | "c_emperor" | "c_fool") && cons_free < 0 {
                continue;
            }
            return Some(("use", json!({"consumable": i}), format!("use {label}")));
        }
        if targeted(key).is_some() && state == "SELECTING_HAND" {
            let bv = b.get_or_insert_with(|| build::decide(gs_));
            if let Some((pick, text)) = plan(key, gs::cards_of(gs_, "hand"), bv) {
                return Some(("use", json!({"consumable": i, "cards": pick}), format!("use {label}: {text}")));
            }
        }
    }
    None
}

// ---- measured value (v11) -------------------------------------------------------

// Chance that a given card is among the scored cards of a hand (8-card hands from a ~52-card deck,
// with discards): the enhanced card only pays when it shows up and scores.
const SHOW_RATE: f64 = 0.12;

// tarots.py MEASURABLE
fn measurable(arg: &str) -> Option<&'static str> {
    match arg {
        "MULT" => Some("MULT"),
        "BONUS" => Some("BONUS"),
        "GLASS" => Some("GLASS"),
        "LUCKY" => Some("MULT"),
        "STEEL" => Some("STEEL"),
        _ => None,
    }
}

// tarots.py 178-214: (text, per_hand_gain) for an enhancement tarot, measured on our recent hands with
// jokers, or None. Lucky counts as its average (+4 Mult). Steel is measured on a held card.
pub fn measured(c: &Value, gs_: &Value, recent: &[Value], b: &Value) -> Option<(String, f64)> {
    let key = gs::s(gs::get(c, "key"));
    let (kind, n, arg) = targeted(key)?;
    if kind != "enhance" {
        return None;
    }
    let enh = measurable(arg?)?;
    let js = jokers::build(gs_, None);
    let levels: &Levels = gs::area(gs_, "hands");
    let mut before: i64 = 0;
    let mut after: i64 = 0;
    for s in jokers::sample_plays(gs_, recent) {
        let mut new = s.infos.clone();
        let t = if enh == "STEEL" {
            let held: Vec<usize> = (0..new.len()).filter(|i| !s.combo.contains(i)).collect();
            if held.is_empty() {
                continue;
            }
            // min(held, key=...): first minimal
            let mut best = held[0];
            for &i in &held[1..] {
                if build::card_value(&new[i], b) < build::card_value(&new[best], b) {
                    best = i;
                }
            }
            best
        } else {
            let scoring: Vec<usize> = s.sc.iter().map(|&p| s.combo[p]).collect();
            if scoring.is_empty() {
                continue;
            }
            // max(scoring, key=...): first maximal
            let mut best = scoring[0];
            for &i in &scoring[1..] {
                if build::card_value(&new[i], b) > build::card_value(&new[best], b) {
                    best = i;
                }
            }
            best
        };
        new[t].enh = Some(enh.to_string());
        let rnd = jokers::Rnd::from_value(&s.rnd);
        before += jokers::score(&s.infos, &s.combo, s.name, &s.sc, levels, &js, &rnd);
        after += jokers::score(&new, &s.combo, s.name, &s.sc, levels, &js, &rnd);
    }
    if before == 0 {
        return None;
    }
    let when = after as f64 / before as f64 - 1.0;
    let per_hand = when * n as f64 * SHOW_RATE;
    let where_ = if enh == "STEEL" { "is held" } else { "scores" };
    let risk = if enh == "GLASS" { " (Glass has a 1 in 4 chance to break when it scores)" } else { "" };
    let plural = if n > 1 { "s" } else { "" };
    let text = format!(
        "permanent: +{}% on a hand where the enhanced card {where_}{risk}; {n} card{plural} = about +{}% per hand on average",
        pyround_int(100.0 * when),
        pyround_int(100.0 * per_hand)
    );
    Some((text, per_hand))
}
