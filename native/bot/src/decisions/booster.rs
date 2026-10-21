//! Port of src/decisions.py lines 851-987: booster, _gain. See docs/port-handoff/decisions.md.

use super::shop::{score_items, ShopItems};
use super::{recent, run_context, PACK_SCORE, PICK_CLEAR};
use crate::build;
use crate::gs::{self, area, cards_of};
use crate::hands;
use crate::jev_bot::{item_desc, JevBot, Stop};
use crate::jokers;
use crate::strategy;
use crate::tarots;
use pycompat::fmt::{fixed, pyfloat, pyround_int};
use serde_json::{json, Map, Value};
use std::cmp::Ordering;

/// `gs.get("money") or 0`
fn or_zero(v: &Value) -> Value {
    if gs::truthy(v) { v.clone() } else { json!(0) }
}

/// `m.get(k, d)` for an int field.
fn int_or(m: &Map<String, Value>, k: &str, d: i64) -> i64 {
    m.get(k).map_or(d, gs::int)
}

/// Position of an item key (keys are unique).
fn find(items: &ShopItems, key: &str) -> usize {
    items.iter().position(|(k, _)| k == key).expect("booster item key")
}

/// `params["card"]` of a pack item.
fn card_of(params: &Value) -> usize {
    gs::int(gs::get(params, "card")) as usize
}

/// Python `max(keys, key=...)` over indices: the first maximal element wins.
fn argmax(vals: &[f64]) -> usize {
    let mut b = 0;
    for (j, v) in vals.iter().enumerate() {
        if *v > vals[b] {
            b = j;
        }
    }
    b
}

/// decisions.py 851-947 `booster(bot, gs)`.
pub fn booster(bot: &mut JevBot, gs_: &Value) -> Result<(), Stop> {
    let jok = area(gs_, "jokers");
    let jokers_full = int_or(jok, "count", 0) >= int_or(jok, "limit", 5);
    let (mains, mut state) = run_context(gs_);
    state["pack_selection"] = json!(
        "The pack is already open: taking a card costs $0. Rate the benefit of keeping it versus skipping, \
         not whether we should have bought the pack. A temporary sellable joker can help while slots are \
         empty; account for harmful effects, rental costs and permanent slot commitments."
    );
    if crate::experiment::get().pack_forecast {
        if let Some((i, d, se)) = pack_joker_forecast(bot, gs_, jokers_full) {
            let se = fixed(se, 2);
            let label = gs::get(&cards_of(gs_, "pack")[i], "label").as_str().unwrap_or("?").to_string();
            bot.emit("auto", json!({"question": "pack", "choice": format!("take_{i}"), "desc": label,
                "reason": format!("pack forecast: beats skipping by {} blinds (> {} x SE {se})", fixed(d, 2), super::SURE_SE)}));
            if bot.act("pack", Some(&json!({"card": i}))).is_ok() {
                return Ok(());
            }
        }
    }
    let hand = cards_of(gs_, "hand"); // only arcana/spectral packs deal a hand to target
    if !hand.is_empty() {
        state["cards_in_hand"] = json!(hand.iter().map(hands::describe).collect::<Vec<_>>());
    }
    let b = build::decide(gs_);
    state["our_build"] = json!(build::describe(&b));
    let money = or_zero(gs::get(gs_, "money"));
    let jokers_ = cards_of(gs_, "jokers");
    let recent_ = recent(bot, gs_);
    let pack = cards_of(gs_, "pack");
    let mut items: ShopItems = vec![];
    for (i, c) in pack.iter().enumerate() {
        let set_v = gs::get(c, "set");
        if gs::s(set_v) == "JOKER" && jokers_full {
            continue;
        }
        let kind = if gs::truthy(set_v) { gs::s(set_v).to_lowercase() } else { "card".to_string() };
        let what = item_desc(c, Some(&mains));
        let mut o = Map::new();
        o.insert("kind".into(), json!(kind));
        o.insert("what".into(), json!(what));
        o.insert("cost".into(), json!(0));
        if let Value::Object(g) = gain(gs_, &recent_, c, None) {
            o.extend(g);
        }
        if let Some(cash) = tarots::cash_outcome(c, &money, &json!(0), jokers_) {
            let mut co = gs::d(&cash).clone();
            co.insert("interest_after_use".into(), json!(strategy::interest(gs::get(&cash, "cash_after_use"))));
            o.insert("cash_outcome".into(), Value::Object(co));
        }
        let key = gs::s(gs::get(c, "key"));
        let mut params = Map::new();
        params.insert("card".into(), json!(i));
        if tarots::targeted(key).is_some() {
            let measured = if crate::experiment::get().tarot_measure { crate::readiness::measured_plan(gs_, key, hand) } else { None };
            match measured.or_else(|| tarots::plan(key, hand, &b)) {
                Some((targets, text)) => {
                    o.insert("use".into(), json!(text));
                    params.insert("targets".into(), json!(targets));
                }
                None => {
                    o.insert("use".into(), json!("no good target in this hand"));
                }
            }
        } else if let Some(n) = tarots::note(c, &b, &money, jokers_) {
            if !n.is_empty() {
                o.insert("use".into(), json!(n));
            }
        }
        items.push((format!("take_{i}"), (what, Value::Object(o), ("pack".to_string(), Some(Value::Object(params))))));
    }
    let skip: (String, Option<Value>) = ("pack".to_string(), Some(json!({"skip": true})));
    if items.is_empty() {
        bot.emit("forced", json!({"question": "pack", "choice": "skip", "desc": "nothing we can take"}));
        bot.act(&skip.0, skip.1.as_ref())?;
        return Ok(());
    }
    // v21: rollout_items.Picker (the pick rule the shop's pack forecast assumes) values every card; a clear
    // winner is taken by code, otherwise JEV rates the cards with the Picker values as a field.
    let args = json!({
        "pack": pack, "hand": hand, "money": money, "jokers": jokers_,
        "joker_limit": jok.get("limit").cloned().unwrap_or(json!(5)),
    });
    let vals = bot.oracle.borrow_mut().picker_values(gs_, &args);
    let mut ranked: Vec<(f64, usize, Option<Vec<usize>>)> = vec![];
    for v in vals.as_array().map(|a| a.as_slice()).unwrap_or(&[]) {
        let arr = v.as_array().expect("Picker.values row");
        let i = gs::int(&arr[1]) as usize;
        if items.iter().any(|(k, _)| *k == format!("take_{i}")) {
            let t = arr.get(2).and_then(|t| t.as_array()).map(|t| t.iter().map(|x| gs::int(x) as usize).collect());
            ranked.push((gs::num(&arr[0]), i, t));
        }
    }
    ranked.sort_by(|a, b| b.0.partial_cmp(&a.0).unwrap_or(Ordering::Equal));
    for (v, i, _) in &ranked {
        let idx = find(&items, &format!("take_{i}"));
        items[idx].1 .1["pick_value"] = json!(format!(
            "{} (per-hand score gain equivalent, $1 = 0.01; best here {})",
            fixed(*v, 3),
            fixed(ranked[0].0, 3)
        ));
    }
    if !ranked.is_empty()
        && ranked[0].0 > 0.0
        && (ranked.len() == 1 || ranked[0].0 >= PICK_CLEAR * ranked[1].0.max(1e-9))
    {
        let (v, i, t) = ranked[0].clone();
        let key = format!("take_{i}");
        let idx = find(&items, &key);
        let desc = items[idx].1 .0.clone();
        let method = items[idx].1 .2 .0.clone();
        let mut p = items[idx].1 .2 .1.as_ref().and_then(|x| x.as_object()).cloned().unwrap_or_default();
        if let Some(t) = t.filter(|t| !t.is_empty()) {
            if !p.contains_key("targets") {
                p.insert("targets".into(), json!(t));
            }
        }
        let runner = if ranked.len() > 1 { fixed(ranked[1].0, 3) } else { "none".to_string() };
        bot.emit(
            "auto",
            json!({
                "question": "pack", "choice": key, "action": [method, p], "desc": desc,
                "reason": format!("pack pick: value {} >= {}x the runner-up ({runner})", fixed(v, 3), pyfloat(PICK_CLEAR)),
                "msg": format!("Pack pick {}", desc.chars().take(40).collect::<String>()),
            }),
        );
        match bot.act(&method, Some(&Value::Object(p))) {
            Ok(_) => return Ok(()),
            Err(e) => bot.emit("info", json!({"msg": format!("Pack pick failed ({e}); asking JEV")})),
        }
    }
    let (score_map, meta) = score_items(bot, gs_, "pack", &state, &items)?;
    let scores: Vec<f64> = items.iter().map(|(k, _)| gs::num(score_map.get(k).unwrap_or(&Value::Null))).collect();
    let bi = argmax(&scores);
    let best = items[bi].0.clone();
    let best_score = scores[bi];
    let (mut choice, mut method, mut params): (String, String, Value) = if best_score >= PACK_SCORE {
        let a = items[bi].1 .2.clone();
        (best.clone(), a.0, a.1.unwrap_or(Value::Null))
    } else {
        ("skip".to_string(), skip.0.clone(), skip.1.clone().unwrap_or(Value::Null))
    };
    if choice == "skip" {
        // v11: the pack is paid for, so a tarot we have a good use for is free value: take the best-rated one.
        let params_of = |j: usize| items[j].1 .2 .1.clone().unwrap_or(Value::Null);
        let usable: Vec<usize> = (0..items.len())
            .filter(|&j| {
                let p = params_of(j);
                p.get("targets").is_some() || tarots::safe_untargeted(gs::s(gs::get(&pack[card_of(&p)], "key")))
            })
            .collect();
        if !usable.is_empty() {
            let mut bj = usable[0];
            for &j in &usable {
                if scores[j] > scores[bj] {
                    bj = j;
                }
            }
            choice = items[bj].0.clone();
            method = items[bj].1 .2 .0.clone();
            params = params_of(bj);
        }
        // v15: a paid-for Celestial pack always has value: take the planet with the biggest measured gain.
        let planets: Vec<usize> = (0..items.len())
            .filter(|&j| gs::s(gs::get(&pack[card_of(&params_of(j))], "set")) == "PLANET")
            .collect();
        if !planets.is_empty() && usable.is_empty() {
            // key = (items[k][1].get("_gain") or 0, scores[k])
            let key = |j: usize| (gs::num(gs::get(&items[j].1 .1, "_gain")), scores[j]);
            let better = |a: (f64, f64), b: (f64, f64)| a.0 > b.0 || (a.0 == b.0 && a.1 > b.1);
            let mut bj = planets[0];
            for &j in &planets[1..] {
                if better(key(j), key(bj)) {
                    bj = j;
                }
            }
            choice = items[bj].0.clone();
            method = items[bj].1 .2 .0.clone();
            params = params_of(bj);
        }
    }
    let options: Map<String, Value> = items.iter().map(|(k, it)| (k.clone(), json!(it.0))).collect();
    let probs: Map<String, Value> = items.iter().zip(&scores).map(|((k, _), s)| (k.clone(), json!(s / 4.0))).collect();
    bot.record_decision(
        "pack",
        gs_,
        &json!(choice),
        &json!([method, params]),
        &Value::Object(options),
        &Value::Object(probs),
        &json!(best_score / 4.0),
        &meta,
        "score",
    );
    match bot.act(&method, Some(&params)) {
        Ok(_) => Ok(()),
        Err(e) => {
            if gs::truthy(gs::get(&params, "skip")) {
                return Err(e.into());
            }
            // Tarot/Spectral that needs hand targets: aim at the first two cards, else give up on the pack.
            bot.emit("info", json!({"msg": format!("Pack pick needs targets ({e}); retrying with targets")}));
            let with_targets = |t: Vec<usize>| {
                let mut p = params.clone();
                p["targets"] = json!(t);
                p
            };
            if bot.act("pack", Some(&with_targets(vec![0, 1]))).is_ok() {
                return Ok(());
            }
            if bot.act("pack", Some(&with_targets(vec![0]))).is_ok() {
                return Ok(());
            }
            bot.act("pack", Some(&json!({"skip": true})))?;
            Ok(())
        }
    }
}

// decisions.py 950-987: `{measured_gain, _gain}`-style dict (may be empty); `remove` = joker index or None
pub(crate) fn gain(gs_: &Value, recent: &[Value], c: &Value, remove: Option<usize>) -> Value {
    let set = gs::s(gs::get(c, "set"));
    let key = gs::s(gs::get(c, "key"));
    let g: Option<f64> = if set == "JOKER" {
        if key == "j_misprint" {
            // BalatroBot drops the animated number from Misprint's text (often just '  +').
            // Give JEV the real range without treating a random average as a guaranteed clear.
            return json!({
                "known_effect": "Adds a random 0 to 23 Mult each hand; not guaranteed scoring",
                "measured_gain": "Random Mult is not included in this item's recent-hand estimate",
            });
        }
        let parsed = jokers::entry(c);
        if parsed.rule.is_none() && !parsed.scoring {
            let mut facts = Map::new();
            facts.insert(
                "measured_gain".into(),
                json!("Not a direct scoring effect: this measurement does not value extra hands, \
                       joker generation, money or other utility. Judge the described effect, not a zero score gain."),
            );
            if key == "j_riff_raff" {
                let owned = cards_of(gs_, "jokers").len() as i64 - if remove.is_some() { 1 } else { 0 };
                let mut limit = int_or(area(gs_, "jokers"), "limit", 5);
                if gs::s(gs::mget(gs::d(gs::get(c, "modifier")), "edition")) == "NEGATIVE" {
                    limit += 1;
                }
                let free = (limit - owned - 1).max(0);
                facts.insert(
                    "next_blind_generation".into(),
                    json!({
                        "free_slots_after_taking": free,
                        "common_jokers_created": free.min(2),
                        "resulting_joker_count": owned + 1 + free.min(2),
                        "scoring": "random Common jokers; exact effects unknown",
                    }),
                );
            }
            return Value::Object(facts);
        }
        match jokers::gain(gs_, recent, Some(c), remove, None) {
            Some(x) => Some(x),
            None => return json!({"measured_gain": "can't be measured from its text; judge it yourself"}),
        }
    } else if set == "PLANET" {
        let effect = gs::s(gs::mget(gs::d(gs::get(c, "value")), "effect"));
        match jokers::planet_level(effect) {
            Some((name, a, b)) => jokers::gain(gs_, recent, None, None, Some((name.as_str(), a, b))),
            None => return json!({}),
        }
    } else if set == "TAROT" || set == "SPECTRAL" {
        return match tarots::measured(c, gs_, recent, &build::decide(gs_)) {
            Some((text, g)) => json!({"measured_gain": text, "_gain": g}),
            None => json!({}),
        };
    } else {
        return json!({});
    };
    let g = match g {
        Some(x) if x != 0.0 => x,
        _ => {
            let what = if set == "PLANET" {
                "levels a hand we haven't played recently"
            } else {
                "no immediate effect on our recent hands' scores (scaling jokers may grow)"
            };
            return json!({"measured_gain": what, "_gain": 0.0});
        }
    };
    let basis = if recent.len() >= 4 {
        format!("our last {} hands", recent.len())
    } else {
        "our recent and typical hands".to_string()
    };
    let sign = if g >= 0.0 { "+" } else { "" };
    json!({
        "measured_gain": format!("{sign}{}% score on {basis}", pyround_int(100.0 * g)),
        "_gain": g,
    })
}

/// Lab (`pack-forecast`): the pack's jokers through the shop forecast (rollouts, as shop jokers are judged) instead
/// of the Picker's measured gain on recent hands. The pack's jokers are shown to the forecast as a shop of $0 jokers;
/// the best one beating buying nothing by more than SURE_SE paired SEs is taken: (pack index, delta, se).
fn pack_joker_forecast(bot: &mut JevBot, gs_: &Value, jokers_full: bool) -> Option<(usize, f64, f64)> {
    let pack = cards_of(gs_, "pack");
    let idx: Vec<usize> = (0..pack.len())
        .filter(|&i| gs::get(&pack[i], "set").as_str() == Some("JOKER")
            && (!jokers_full || gs::get(gs::get(&pack[i], "modifier"), "edition").as_str() == Some("NEGATIVE")))
        .collect();
    if idx.is_empty() {
        return None;
    }
    let mut g = gs_.clone();
    let shop: Vec<Value> = idx.iter().map(|&i| {
        let mut c = pack[i].clone();
        let mut cost = gs::d(gs::get(&c, "cost")).clone();
        cost.insert("buy".into(), json!(0));
        c["cost"] = Value::Object(cost);
        c
    }).collect();
    let o = g.as_object_mut()?;
    o.insert("state".into(), json!("SHOP"));
    o.insert("shop".into(), json!({"cards": shop, "count": idx.len(), "limit": idx.len()}));
    o.insert("packs".into(), json!({"cards": [], "count": 0}));
    o.insert("vouchers".into(), json!({"cards": [], "count": 0}));
    let keys: Vec<String> = (0..idx.len()).map(|k| format!("buy_card_{k}")).collect();
    let (n, bosses) = (bot.dec.forecast_n, bot.dec.forecast_bosses);
    let f = bot.oracle.borrow_mut().forecast(&g, n, bosses, &keys, None);
    let options = gs::get(&f, "options");
    let mut best: Option<(usize, f64, f64)> = None;
    for (k, &i) in idx.iter().enumerate() {
        let r = gs::get(options, &format!("buy_card_{k}"));
        let pair = gs::list(gs::get(gs::get(r, "vs_skip"), "blinds"));
        if pair.len() < 2 {
            continue;
        }
        let (d, se) = (gs::num(&pair[0]), gs::num(&pair[1]));
        if d > super::SURE_SE as f64 * se.max(1e-9) && best.map_or(true, |b| d > b.1) {
            best = Some((i, d, se));
        }
    }
    best
}
