//! Port of src/decisions.py lines 158-452 (except 181-199, which are in mod.rs): the hand handler and
//! its helpers. Plays are Python dicts, kept as `serde_json::Value` objects (the helpers add keys such
//! as "outlook"). See docs/port-handoff/decisions.md.
//!
//! Scorers and boss restrictions are real `jokers::Scorer` / `hands::BossAllow` values passed to hands.rs
//! as `ScorerArg::Jokers` / `Allowed::Boss`, so the kernel runs exactly when Python's native path does.

use super::{deck_remaining, instr, levels as levels_of, play as do_play, NEAR_PACE};
use crate::build;
use crate::gs::{self, area, cards_of};
use crate::hands::{
    card_info, classify, describe, discard_candidates, play_candidates, Allowed, Info, Levels, ScorerArg,
};
use crate::jev_bot::{choice_q_map, item_desc, noul_q, JevBot, Stop};
use crate::jokers::{self, Rnd, Scorer};
use crate::odds::{self, DrawCount};
use crate::prompts::{GAME_RULES, PLAY_NOW_CRITERIA};
use crate::strategy;
use pycompat::fmt::pyround_int;
use serde_json::{json, Map, Value};
use std::collections::HashSet;

// decisions.py 200-231 live in hands.rs; re-exported under their hands.rs names.
#[allow(unused_imports)]
pub(crate) use crate::hands::{BossAllow, _boss_allows, _boss_levels};

// decisions.py 158-159
pub(crate) const CLEAR_MARGIN: f64 = 1.1;
pub(crate) const PLAY_NOW_UNDER_PACE: f64 = 0.85;

// decisions.py 257-262
pub(crate) const PLAY_FORECAST_BOSSES: &[&str] = &[
    "Small Blind", "Big Blind", "The Water", "The Goad", "The Club", "The Head",
    "The Window", "The Plant", "The Psychic", "The Eye", "The Mouth", "The Flint",
    "The Needle", "The Serpent", "The Wall", "The Manacle", "Violet Vessel",
];

// decisions.py 162-163
pub(crate) fn clears(p: &Value, info: &Value, margin: f64) -> bool {
    gs::num(gs::get(p, "estimate")) >= gs::num(gs::get(info, "chips_still_needed")) * margin
}

/// `info["chips_needed_per_remaining_hand"] or 1`
fn pace_of(info: &Value) -> i64 {
    let v = gs::get(info, "chips_needed_per_remaining_hand");
    if gs::truthy(v) { gs::int(v) } else { 1 }
}

fn strs(v: &Value) -> Vec<&str> {
    gs::list(v).iter().map(gs::s).collect()
}

// decisions.py 166-178
pub(crate) fn play_obj(p: &Value, info: &Value, margin: f64) -> Value {
    let pace = pace_of(info);
    let est = gs::int(gs::get(p, "estimate"));
    let mut obj = Map::new();
    obj.insert("hand".into(), gs::get(p, "hand").clone());
    obj.insert("scoring_cards".into(), gs::get(p, "scoring").clone());
    obj.insert("also_played".into(), gs::get(p, "kicked").clone());
    obj.insert("estimated_score".into(), gs::get(p, "estimate").clone());
    obj.insert("share_of_pace".into(), json!(format!("{}%", pyround_int(100.0 * est as f64 / pace.max(1) as f64))));
    obj.insert("clears_blind_alone".into(), json!(clears(p, info, margin)));
    if gs::d(p).contains_key("outlook") {
        obj.insert("next_hand_outlook".into(), gs::get(p, "outlook").clone());
    }
    if gs::truthy(gs::get(p, "setup")) {
        obj.insert("setup".into(), gs::get(p, "setup").clone());
    }
    Value::Object(obj)
}

use super::shop::discard_obj;

fn idx_list(v: &Value) -> Vec<usize> {
    gs::list(v).iter().map(|x| x.as_u64().expect("card index") as usize).collect()
}

// decisions.py 232-254. Returns the discard-plan dict.
pub(crate) fn preserve_discard(hand: &[Value], play: &Value, scorer: &Scorer, max_discard: usize) -> Value {
    let infos: Vec<Info> = hand.iter().map(card_info).collect();
    let combo = idx_list(gs::get(play, "cards"));
    let sel: Vec<Info> = combo.iter().map(|&i| infos[i].clone()).collect();
    let (_, sc) = classify(&sel);
    let scoring: HashSet<usize> = sc.iter().map(|&p| combo[p]).collect();
    let estimate = gs::int(gs::get(play, "estimate"));
    let mut toss: Vec<usize> = Vec::new();
    for i in 0..infos.len() {
        if scoring.contains(&i) {
            continue;
        }
        let mut remaining: Vec<Info> = infos[..i].to_vec();
        remaining.extend_from_slice(&infos[i + 1..]);
        let sub: Vec<usize> = combo.iter().filter(|&&p| p != i).map(|&p| if p > i { p - 1 } else { p }).collect();
        let subinfos: Vec<Info> = sub.iter().map(|&p| remaining[p].clone()).collect();
        let (sub_name, sub_sc) = classify(&subinfos);
        if scorer.call(&remaining, &sub, sub_name, &sub_sc) >= estimate {
            toss.push(i);
        }
    }
    // "23456789TJQKA".find(rank or ""): "" -> 0, unknown -> -1
    let find = |i: usize| -> i64 {
        let r = infos[i].rank.as_deref().unwrap_or("");
        "23456789TJQKA".find(r).map_or(-1, |p| p as i64)
    };
    toss.sort_by_key(|&i| (!infos[i].debuff, find(i)));
    toss.truncate(max_discard);
    json!({"label": "Keep current scoring cards and useful held cards; cycle the rest",
           "cards": toss, "tossed": toss.iter().map(|&i| describe(&hand[i])).collect::<Vec<_>>(),
           "goal": ["preserve", Value::Null]})
}

/// `{**levels, name: {**h, "played": +1, "played_this_round": +1}}`
fn next_levels_of(levels: &Levels, name: &str) -> Levels {
    let mut h = gs::d(levels.get(name).unwrap_or(&Value::Null)).clone();
    let bump = |h: &Map<String, Value>, k: &str| -> Value {
        let v = gs::mget(h, k);
        json!((if gs::truthy(v) { gs::int(v) } else { 0 }) + 1)
    };
    let (a, b) = (bump(&h, "played"), bump(&h, "played_this_round"));
    h.insert("played".into(), a);
    h.insert("played_this_round".into(), b);
    let mut out = levels.clone();
    out.insert(name.to_string(), Value::Object(h));
    out
}

// decisions.py 264-332. Returns the (possibly extended) list of plays.
#[allow(clippy::too_many_arguments)]
pub(crate) fn setup_plays(
    gs_: &Value, hand: &[Value], plays: &[Value], levels: &Levels, scorer: &Scorer, allowed: &BossAllow,
    info: &Value, margin: f64,
) -> Vec<Value> {
    let blind = gs::get(info, "blind");
    let hands_left = gs::num(gs::get(info, "hands_left"));
    if (gs::truthy(gs::get(info, "discards_left")) && gs::num(gs::get(info, "discards_left")) > 0.0)
        || hands_left < 2.0
        || !blind.as_str().map_or(false, |b| PLAY_FORECAST_BOSSES.contains(&b))
        || hand.len() > 10
        || (blind.as_str() == Some("The Serpent") && hand.len() > 8)
        || cards_of(gs_, "cards").is_empty()
        || !jokers::play_forecast_supported(gs_)
    {
        return plays.to_vec();
    }
    let infos: Vec<Info> = hand.iter().map(card_info).collect();
    if hand.iter().zip(&infos).any(|(c, i)| {
        (i.rank_truthy().is_none() && !i.enh_is("STONE")) || gs::truthy(gs::get(gs::get(c, "state"), "hidden"))
    }) {
        return plays.to_vec();
    }
    let mut candidates: Vec<Value> = plays.to_vec();
    let mut seen: Vec<Vec<usize>> = plays.iter().map(|p| { let mut c = idx_list(gs::get(p, "cards")); c.sort(); c.dedup(); c }).collect();
    let hl = gs::mget(area(gs_, "hand"), "highlighted_limit");
    let cap = 5.min(if gs::truthy(hl) { gs::int(hl) } else { 5 }) as usize;
    for plan in discard_candidates(hand, cap) {
        let mut combo = plan.cards.clone();
        combo.sort(); // preserve the live hand's scoring order
        let mut key = combo.clone();
        key.dedup();
        if seen.contains(&key) {
            continue;
        }
        let sel: Vec<Info> = combo.iter().map(|&i| infos[i].clone()).collect();
        let (name, sc) = classify(&sel);
        if !allowed.call(&combo, name) {
            continue;
        }
        let scoring: HashSet<usize> = sc.iter().map(|&i| combo[i]).collect();
        candidates.push(json!({
            "hand": name, "cards": combo,
            "estimate": scorer.call(&infos, &combo, name, &sc),
            "scoring": combo.iter().filter(|i| scoring.contains(i)).map(|&i| describe(&hand[i])).collect::<Vec<_>>(),
            "kicked": combo.iter().filter(|i| !scoring.contains(i)).map(|&i| describe(&hand[i])).collect::<Vec<_>>(),
            "setup": format!("Use a hand to cycle cards: {}", plan.label),
        }));
        seen.push(key);
    }
    if candidates.len() == plays.len() {
        return plays.to_vec();
    }
    let deck: Vec<Info> = cards_of(gs_, "cards").iter().map(card_info).collect();
    let rnd = gs::get(gs_, "round");
    let mut next_round = gs::d(rnd).clone();
    next_round.insert("hands_left".into(), json!(gs::int(gs::get(info, "hands_left")) - 1));
    let hp = gs::mget(gs::d(rnd), "hands_played");
    next_round.insert("hands_played".into(), json!((if gs::truthy(hp) { gs::int(hp) } else { 0 }) + 1));
    let next_round = Value::Object(next_round);
    let ls = gs::mget(area(gs_, "hand"), "limit");
    let hand_size = if gs::truthy(ls) { gs::int(ls) } else { hand.len() as i64 };
    let blind_name = blind.as_str().unwrap_or("").to_string();
    let serpent = blind_name == "The Serpent";
    let need_base = gs::num(gs::get(info, "chips_still_needed")) * margin;
    for p in candidates.iter_mut() {
        let name = gs::get(p, "hand").as_str().expect("hand").to_string();
        let next_levels = next_levels_of(levels, &name);
        let next_allowed = BossAllow::new(&next_levels, &blind_name);
        let factory = |_played: &[Info], drawn: usize| -> Scorer {
            Scorer::new(next_levels.clone(), jokers::after_play(gs_, drawn as i64), Rnd::from_value(&next_round))
        };
        let est = gs::int(gs::get(p, "estimate"));
        let mut forecast = vec![json!({"cards": gs::get(p, "cards")})];
        let need = (need_base - est as f64).max(0.0);
        let draw_count = |n: usize| -> i64 { if serpent { 3 } else { (hand_size - hand.len() as i64 + n as i64).max(0) } };
        odds::forecast(&mut forecast, &infos, &deck, &factory, Allowed::Boss(&next_allowed), need, 0.0,
            DrawCount::Func(&draw_count), 32);
        let f = &forecast[0];
        let (exp, low) = (gs::int(gs::get(f, "expected_score")), gs::int(gs::get(f, "low_score")));
        let pc = idx_list(gs::get(p, "cards"));
        let keeps: Vec<String> = (0..hand.len()).filter(|i| !pc.contains(i)).map(|i| describe(&hand[i])).collect();
        let outlook = json!({
            "average_next_play": exp,
            "average_two_play_total": est + exp,
            "poor_draw_two_play_total": est + low,
            "poor_draw_definition": "20th percentile, not a guaranteed minimum",
            "chance_two_plays_clear": format!("{}%", pyround_int(100.0 * gs::num(gs::get(f, "chance_to_clear")))),
            "keeps": keeps,
            "cards_drawn": gs::get(f, "drawn"),
            "basis": format!("{} {} unordered draws; best modeled next play, not a full-round win probability",
                gs::int(gs::get(f, "samples")),
                if gs::truthy(gs::get(f, "exact_draws")) { "all possible" } else { "sampled" }),
        });
        p.as_object_mut().unwrap().insert("outlook".into(), outlook);
    }
    // Keep the immediate-score ordering and all original plays; show numerical rankings to JEV.
    let key = |p: &Value| -> (i64, i64) {
        let o = gs::get(p, "outlook");
        (gs::int(gs::get(o, "average_two_play_total")), gs::int(gs::get(o, "poor_draw_two_play_total")))
    };
    let mut order: Vec<usize> = (0..candidates.len()).collect();
    order.sort_by(|&a, &b| key(&candidates[b]).cmp(&key(&candidates[a]))); // stable reverse
    for (rank, &i) in order.iter().enumerate() {
        let o = candidates[i].get_mut("outlook").unwrap().as_object_mut().unwrap();
        o.insert("rank_by_two_play_average".into(), json!(rank + 1));
    }
    candidates
}

/// max(d, key=d.get) over an ordered (key, prob) list: the first maximum.
fn argmax(m: &[(String, f64)]) -> String {
    let mut best = &m[0];
    for e in &m[1..] {
        if e.1 > best.1 {
            best = e;
        }
    }
    best.0.clone()
}

fn probs_of(a: &Value) -> Vec<(String, f64)> {
    gs::d(gs::get(a, "probabilities")).iter().map(|(k, v)| (k.clone(), gs::num(v))).collect()
}

/// decisions.py 335-452 `selecting_hand(bot, gs)`.
const LEAF_KEEP: &[&str] = &["Troubadour", "Juggler", "Turtle Bean", "Merry Andy", "Burglar", "Drunkard", "Mr. Bones", "Chicot"];

// decisions.py 339-363
/// v30: Verdant Leaf debuffs every card until a joker is sold; sell the joker whose loss on recent
/// hands is smallest (eternal and Negative ones kept) before the first play. False if not applicable.
fn _leaf_sale(bot: &mut JevBot, gs_: &Value, boss: &str, hand: &[Value]) -> bool {
    if boss != "Verdant Leaf" || !hand.iter().any(|c| card_info(c).debuff) {
        return false;
    }
    let key = json!([gs::get(gs_, "seed"), gs::get(gs_, "ante_num"), gs::get(gs_, "round_num")]);
    if bot._leaf_failed == key {
        return false;
    }
    let js = cards_of(gs_, "jokers");
    // v38: the bait bought for this boss goes first
    if let Some(j) = js.iter().position(|c| !bot._leaf_bait.is_null() && gs::get(c, "id") == &bot._leaf_bait) {
        let label = gs::get(&js[j], "label").as_str().unwrap_or("None").to_string();
        bot.emit("auto", json!({"question": "move", "choice": "sell_joker", "desc": format!("sell {label}"),
            "reason": "Verdant Leaf: sell the bait joker bought for it"}));
        bot._leaf_bait = Value::Null;
        if bot.act("sell", Some(&json!({"joker": j}))).is_err() {
            bot._leaf_failed = key;
            return false;
        }
        return true;
    }
    // hand size and hands don't show in measured scores: sell those last (TIXQA7PH sold Troubadour)
    let keep: HashSet<usize> = (0..js.len()).filter(|&j| gs::get(&js[j], "label").as_str().is_some_and(|l| LEAF_KEEP.contains(&l))).collect();
    let recent = super::recent(bot, gs_);
    let mut weak: Vec<usize> = super::shop::weakest_jokers(gs_, &recent, js.len()).difference(&keep).copied().collect();
    if weak.is_empty() {
        weak = super::shop::weakest_jokers(gs_, &recent, 1).into_iter().collect();
    }
    weak.sort();
    if weak.is_empty() {
        return false;
    }
    let mut best: Option<(f64, f64, usize)> = None;
    for &k in &weak {
        let loss = -jokers::gain(gs_, &recent, None, Some(k), None).unwrap_or(0.0);
        let sell = gs::num(gs::d(gs::get(&js[k], "cost")).get("sell").unwrap_or(&Value::Null));
        let cand = (loss, sell, k);
        if best.map_or(true, |b| cand.0 < b.0 || (cand.0 == b.0 && (cand.1 < b.1 || (cand.1 == b.1 && cand.2 < b.2)))) {
            best = Some(cand);
        }
    }
    let j = best.unwrap().2;
    let label = gs::get(&js[j], "label").as_str().unwrap_or("None").to_string();
    bot.emit("auto", json!({"question": "move", "choice": "sell_joker", "desc": format!("sell {label}"),
        "reason": "Verdant Leaf: every card stays debuffed until a joker is sold"}));
    if bot.act("sell", Some(&json!({"joker": j}))).is_err() {
        bot._leaf_failed = key; // the API refused: play on with the debuff
        return false;
    }
    true
}

pub fn selecting_hand(bot: &mut JevBot, gs_: &Value) -> Result<(), Stop> {
    let hand = cards_of(gs_, "hand");
    let info = super::round_info(gs_);
    let boss = gs::get(&info, "blind").as_str().unwrap_or("").to_string();
    if _leaf_sale(bot, gs_, &boss, hand) {
        return Ok(());
    }
    let rnd = gs::get(gs_, "round");
    let js = crate::prof::time("sh.jokers_build", || jokers::build(gs_, None));
    let base_levels = gs::d(gs::get(gs_, "hands"));
    let levels = _boss_levels(base_levels, &boss);
    let infos: Vec<Info> = hand.iter().map(card_info).collect();
    let scorer = Scorer::new(levels.clone(), js.clone(), Rnd::from_value(rnd));
    let mut allowed = BossAllow::new(base_levels, &boss);
    allowed.forced = hand.iter().position(|c| gs::truthy(gs::get(gs::get(c, "state"), "forced")));
    let mut plays: Vec<Value> = crate::prof::time("sh.play_candidates", || play_candidates(hand, &levels, 8, ScorerArg::Jokers(&scorer), Allowed::Boss(&allowed)))
        .iter().map(|p| p.to_json()).collect();
    if plays.is_empty() {
        // Nothing scores under Eye/Mouth yet: a play may still cycle cards, but cannot be a sure clear.
        plays = play_candidates(hand, &levels, 8, ScorerArg::Jokers(&scorer), Allowed::None).iter().map(|p| {
            let mut v = p.to_json();
            v.as_object_mut().unwrap().insert("estimate".into(), json!(0));
            v
        }).collect();
    }
    if crate::experiment::get().pair_play && !plays.is_empty() {
        // lab: JEVLAB_EXP=pair-play plays a Pair scoring within PAIR_PLAY of the best play
        let top = gs::num(gs::get(&plays[0], "estimate"));
        if let Some(i) = plays.iter().position(|p| gs::get(p, "hand").as_str() == Some("Pair")
            && gs::num(gs::get(p, "estimate")) >= crate::experiment::PAIR_PLAY * top)
        {
            let p = plays.remove(i);
            plays.insert(0, p);
        }
    }
    let margin = if !js.is_empty() { CLEAR_MARGIN } else { 1.0 };
    if let Some(c) = plays.iter().find(|p| clears(p, &info, margin)) {
        let desc = format!("{} ({}) clears the blind", gs::s(gs::get(c, "hand")), gs::int(gs::get(c, "estimate")));
        let cards = idx_list(gs::get(c, "cards"));
        bot.emit("forced", json!({"question": "move", "choice": "play_1", "desc": desc}));
        do_play(bot, gs_, hand, &cards)?;
        return Ok(());
    }

    let plays = crate::prof::time("sh.setup_plays", || setup_plays(gs_, hand, &plays, &levels, &scorer, &allowed, &info, margin));
    let hl = gs::mget(area(gs_, "hand"), "highlighted_limit");
    let cap = if gs::truthy(hl) { gs::int(hl) } else { 5 } as usize;
    let discards_left = gs::get(&info, "discards_left");
    let hands_left = gs::num(gs::get(&info, "hands_left"));
    let mut discards: Vec<Value> = if gs::truthy(discards_left) && gs::num(discards_left) > 0.0 {
        crate::prof::time("sh.discard_cands", || discard_candidates(hand, cap)).iter().map(|d| d.to_json()).collect()
    } else {
        vec![]
    };
    let b = crate::prof::time("sh.build_decide", || build::decide(gs_));
    let est0 = gs::int(gs::get(&plays[0], "estimate"));
    let pace_v = gs::get(&info, "chips_needed_per_remaining_hand");
    let pace_f = if gs::truthy(pace_v) { gs::num(pace_v) } else { 0.0 };
    if !discards.is_empty() {
        let preserve = crate::prof::time("sh.preserve", || preserve_discard(hand, &plays[0], &scorer, cap));
        let pc: HashSet<usize> = idx_list(gs::get(&preserve, "cards")).into_iter().collect();
        if !pc.is_empty()
            && discards.iter().all(|d| idx_list(gs::get(d, "cards")).into_iter().collect::<HashSet<_>>() != pc)
        {
            discards.push(preserve);
        }
        let mut next_round = gs::d(rnd).clone();
        let dl = gs::mget(gs::d(rnd), "discards_left");
        next_round.insert("discards_left".into(),
            json!((if gs::truthy(dl) { gs::int(dl) } else { 0 } - 1).max(0)));
        let next_round = Value::Object(next_round);
        let factory = |tossed: &[Info], drawn: usize| -> Scorer {
            let t: Vec<Value> = tossed.iter().map(|i| i.to_json()).collect();
            Scorer::new(levels.clone(), jokers::after_discard(gs_, &t, drawn as i64), Rnd::from_value(&next_round))
        };
        let ls = gs::mget(area(gs_, "hand"), "limit");
        let hand_size = if gs::truthy(ls) { gs::int(ls) } else { hand.len() as i64 };
        let serpent = boss == "The Serpent";
        let draw_count = |n: usize| -> i64 { if serpent { 3 } else { (hand_size - hand.len() as i64 + n as i64).max(0) } };
        let deck: Vec<Info> = cards_of(gs_, "cards").iter().map(card_info).collect();
        crate::prof::time("sh.odds_forecast", || odds::forecast(&mut discards, &infos, &deck, &factory, Allowed::Boss(&allowed),
            gs::num(gs::get(&info, "chips_still_needed")) * margin, pace_f, DrawCount::Func(&draw_count), 32));
        for d in discards.iter_mut() {
            let fits = build::fits(gs::get(d, "goal"), &b);
            let e = gs::num(gs::get(d, "expected_score"));
            let gain = pyround_int(100.0 * (e / est0.max(1) as f64 - 1.0));
            let o = d.as_object_mut().unwrap();
            o.insert("fits_build".into(), json!(fits));
            o.insert("gain_percent".into(), json!(gain));
        }
        // Order the alternatives by measured survival value, not suit enumeration or build preference.
        let key = |d: &Value| -> (f64, f64) {
            let (c, e) = (gs::num(gs::get(d, "chance_to_clear")), gs::num(gs::get(d, "expected_score")));
            if hands_left <= 1.0 { (c, e) } else { (e, c) }
        };
        discards.sort_by(|x, y| key(y).partial_cmp(&key(x)).unwrap()); // reverse=True, stable
    }

    let play_opts: Vec<(String, &Value)> = plays.iter().enumerate().map(|(n, p)| (format!("play_{}", n + 1), p)).collect();
    let disc_opts: Vec<(String, &Value)> = discards.iter().enumerate().map(|(n, d)| (format!("discard_{}", n + 1), d)).collect();
    let mains = strategy::main_hands(gs_, 2);
    let hand_names: HashSet<String> = plays.iter().map(|p| gs::s(gs::get(p, "hand")).to_string()).collect();
    let mut state = Map::new();
    state.insert("game_rules".into(), json!(GAME_RULES));
    state.insert("round".into(), info.clone());
    state.insert("jokers".into(), json!(cards_of(gs_, "jokers").iter().map(|c| item_desc(c, Some(&mains))).collect::<Vec<_>>()));
    state.insert("hand_levels".into(), levels_of(gs_, &hand_names));
    state.insert("cards_in_hand".into(), json!(hand.iter().map(describe).collect::<Vec<_>>()));
    state.insert("deck_remaining".into(), deck_remaining(gs_));
    state.insert("strongest_play".into(), play_obj(&plays[0], &info, margin));
    state.insert("best_draw_by_measurement".into(), if discards.is_empty() { Value::Null } else { discard_obj(&discards[0]) });
    state.insert("our_build".into(), json!(build::describe(&b)));
    let unm = jokers::unmodelled(&js);
    if !unm.is_empty() {
        state.insert("jokers_not_in_estimates".into(), json!(unm));
    }
    let state = Value::Object(state);
    let mut questions = Map::new();
    if play_opts.len() > 1 {
        let mut ins = instr("best_play").to_string();
        if gs::d(&plays[0]).contains_key("outlook") {
            ins.push_str(instr("setup_play"));
        }
        let crit: Map<String, Value> = play_opts.iter().map(|(k, p)| (k.clone(), play_obj(p, &info, margin))).collect();
        questions.insert("best_play".into(), choice_q_map(&ins, crit));
    }
    if !disc_opts.is_empty() {
        let crit: Map<String, Value> = PLAY_NOW_CRITERIA.iter().map(|(k, v)| (k.to_string(), json!(v))).collect();
        questions.insert("play_now".into(), noul_q(instr("play_now"), Value::Object(crit)));
        if disc_opts.len() > 1 {
            let crit: Map<String, Value> = disc_opts.iter().map(|(k, d)| (k.clone(), discard_obj(d))).collect();
            questions.insert("best_discard".into(), choice_q_map(instr("best_discard"), crit));
        }
    }
    if questions.is_empty() {
        bot.emit("forced", json!({"question": "move", "choice": "play_1", "desc": gs::get(&plays[0], "hand")}));
        do_play(bot, gs_, hand, &idx_list(gs::get(&plays[0], "cards")))?;
        return Ok(());
    }

    let (answers, meta) = crate::prof::time("sh.query", || bot.query(gs_, &state, &questions))?;
    let p_play = answers.get("play_now").map_or(1.0, |a| gs::num(gs::get(a, "noul")));
    let pp: Vec<(String, f64)> = match answers.get("best_play") {
        Some(a) => probs_of(a),
        None => vec![("play_1".to_string(), 1.0)],
    };
    let dp: Vec<(String, f64)> = match answers.get("best_discard") {
        Some(a) => probs_of(a),
        None if !disc_opts.is_empty() => vec![("discard_1".to_string(), 1.0)],
        None => vec![],
    };
    let lookup = |m: &[(String, f64)], k: &str| -> f64 { m.iter().find(|e| e.0 == k).map_or(0.0, |e| e.1) };
    let mut probs = Map::new();
    for (k, _) in &play_opts {
        probs.insert(k.clone(), json!(p_play * lookup(&pp, k)));
    }
    for (k, _) in &disc_opts {
        probs.insert(k.clone(), json!((1.0 - p_play) * lookup(&dp, k)));
    }
    // Jev's play_now leans yes (0.5-0.8) even far below pace, and v7 lost ante 1 with discards unused.
    // Below pace with hands to spare, discards are free: only play when Jev is clearly sure.
    // On the last hand a play that can't clear loses anyway, so dig with the discards first (The Needle).
    let est_f = est0 as f64;
    let under_pace = est_f < pace_f;
    let pace = pace_f;
    let spare = (if gs::truthy(discards_left) { gs::num(discards_left) } else { 0.0 }) >= hands_left;
    let payoff = discards.iter().map(|d| gs::int(gs::get(d, "expected_score"))).fold(0, i64::max);
    let near_no_gain = est_f >= NEAR_PACE * pace && !spare && (payoff as f64) < 1.2 * est_f;
    let bar = if !disc_opts.is_empty() && hands_left <= 1.0 {
        1.01
    } else if under_pace && near_no_gain {
        0.5 // close to pace, discards scarce and no plan clearly better: let Jev decide normally
    } else if under_pace {
        PLAY_NOW_UNDER_PACE
    } else {
        0.5
    };
    let best = if p_play >= bar { argmax(&pp) } else { argmax(&dp) };
    let mut options = Map::new();
    for (k, p) in &play_opts {
        let mut s = format!("{}: {}", gs::s(gs::get(p, "hand")), strs(gs::get(p, "scoring")).join(", "));
        if !gs::list(gs::get(p, "kicked")).is_empty() {
            s += &format!(" (+{})", strs(gs::get(p, "kicked")).join(", "));
        }
        s += &format!(" ~{}", gs::int(gs::get(p, "estimate")));
        if gs::d(p).contains_key("outlook") {
            let o = gs::get(p, "outlook");
            s += &format!(" (two-play average ~{}; poor draw ~{})", gs::int(gs::get(o, "average_two_play_total")),
                gs::int(gs::get(o, "poor_draw_two_play_total")));
        }
        options.insert(k.clone(), json!(s));
    }
    for (k, d) in &disc_opts {
        options.insert(k.clone(), json!(format!("{}: toss {} (modeled next play ~{}, {:+}% vs playing now)",
            gs::s(gs::get(d, "label")), strs(gs::get(d, "tossed")).join(", "), gs::int(gs::get(d, "expected_score")),
            gs::int(gs::get(d, "gain_percent")))));
    }
    let (options, probs) = (Value::Object(options), Value::Object(probs));
    let conf = gs::get(&probs, &best).clone();
    if best.starts_with("play") {
        let cards = gs::get(play_opts.iter().find(|(k, _)| *k == best).unwrap().1, "cards").clone();
        bot.record_decision("move", gs_, &json!(best), &json!(["play", {"cards": cards}]), &options, &probs, &conf, &meta, "prob");
        do_play(bot, gs_, hand, &idx_list(&cards))?;
    } else {
        let cards = gs::get(disc_opts.iter().find(|(k, _)| *k == best).unwrap().1, "cards").clone();
        let cards = json!(super::adjust_discard(gs_, hand, idx_list(&cards)));
        let params = json!({"cards": cards});
        bot.record_decision("move", gs_, &json!(best), &json!(["discard", params]), &options, &probs, &conf, &meta, "prob");
        bot.act("discard", Some(&params))?;
    }
    Ok(())
}
