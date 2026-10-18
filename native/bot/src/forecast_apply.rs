//! rollout.apply_option (src/rollout.py 332-340) on the forecast state description: the option's engine
//! actions (Jackdaw game.py `_handle_buy_card` / `_handle_sell_card` / `_handle_redeem_voucher`, card.py
//! add_to_deck / remove_from_deck, vouchers.py apply_voucher, jokers.py campfire / hologram) as edits of
//! `state_json_from_pub(pub)`; the result equals `sim_bridge.describe(gs, rng=True)` of the Jackdaw state
//! after `rng = PseudoRandom(seed)` and the same actions. `apply_option` builds the rumbot game from it.
//!
//! Item ids continue the build's numbering: deck, jokers, consumables, then the shop cards (Jackdaw
//! creates them in that order in build_state).
//! RNG: only Overstock / Overstock Plus draw (shop refill, game.py `_handle_redeem_voucher` ->
//! shop.fill_shop_slots); that refill runs on rumbot's own shop code over placeholder slots
//! (`overstock_pools`) and the pools go out as `rng_pools`. Selling an Invisible Joker that is ready
//! (its duplicate draws) is refused.

use crate::forecast_jokers::{hands_played, joker_value};
use crate::forecast_state::{consumable_name, playing, state_json_from_pub};
use crate::gs;
use rumbot_sim::rng::RunRng;
use serde_json::{json, Value};
use simcore::StateDesc;
use std::collections::HashMap;

fn int(v: &Value) -> i64 {
    v.as_i64().unwrap_or(0)
}

fn set_int(d: &mut Value, k: &str, f: impl Fn(i64) -> i64) {
    let n = f(int(&d[k]));
    d[k] = json!(n);
}

fn is_negative(src: &Value) -> bool {
    gs::get(gs::get(src, "modifier"), "edition").as_str() == Some("NEGATIVE")
}

/// add_to_deck (sign = 1) / remove_from_deck (sign = -1) of a joker: hand size and slots.
fn joker_passives(d: &mut Value, key: &str, text: &str, negative: bool, sign: i64) {
    let h = match key {
        "j_juggler" => 1,
        "j_merry_andy" => -1,
        "j_troubadour" => 2,
        "j_stuntman" => -2,
        "j_turtle_bean" => {
            // the pub text carries the current value (joker_card patches extra.h_size from it)
            let re = regex::Regex::new(r"^\+(\d+) hand size").unwrap();
            re.captures(text).map_or(5, |c| c[1].parse().unwrap_or(5))
        }
        _ => 0,
    };
    if h != 0 {
        set_int(d, "hand_size", |n| n + sign * h);
    }
    if negative {
        set_int(d, "joker_slots", |n| n + sign);
    }
}

/// jokers.py campfire (selling_card) / hologram (playing_card_added): `counter` is the running xMult.
fn bump_jokers(d: &mut Value, name: &str, by: f64) {
    for j in d["jokers"].as_array_mut().unwrap() {
        if j["name"].as_str() == Some(name) && !j["debuffed"].as_bool().unwrap_or(false) {
            let c = j["counter"].as_f64().unwrap_or(1.0);
            j["counter"] = json!(c + by);
        }
    }
}

/// The state description after `opt` (an entry of `forecast_stats::options`), plus the Jackdaw RNG pools
/// when the option drew (None: the rollout's streams start fresh from the seed).
pub fn apply_option_json(p: &Value, opt: &Value, seed: &str) -> Result<(Value, Option<HashMap<String, f64>>), String> {
    apply_option_json_on(state_json_from_pub(p)?, p, opt, seed)
}

/// `apply_option_json` from `d = state_json_from_pub(p)`, which the forecasts build once for all their options.
pub fn apply_option_json_on(mut d: Value, p: &Value, opt: &Value, seed: &str) -> Result<(Value, Option<HashMap<String, f64>>), String> {
    let hp = hands_played(p);
    let next_id = d["cards"].as_array().unwrap().len() + d["jokers"].as_array().unwrap().len() + d["consumables"].as_array().unwrap().len();
    let shop = gs::cards_of(p, "shop");
    let vouchers = gs::cards_of(p, "vouchers");
    let pub_jokers = gs::cards_of(p, "jokers");
    // live lists of pub indices: the shop and voucher rows shrink as cards go, owned jokers keep their order
    let mut shop_idx: Vec<usize> = (0..shop.len()).collect();
    let mut vouchers_left: Vec<usize> = (0..vouchers.len()).collect();
    let mut owned: Vec<Value> = pub_jokers.to_vec();
    let mut overstock = false;
    let actions = opt["actions"].as_array().ok_or("actions")?;
    for a in actions {
        let kind = a[0].as_str().unwrap_or("");
        let i = a[1].as_u64().unwrap_or(0) as usize;
        match kind {
            "sell" => {
                if i >= owned.len() {
                    return Err(format!("Invalid jokers index {i}"));
                }
                let src = owned[i].clone();
                let key = gs::s(gs::get(&src, "key")).to_string();
                let text = gs::s(gs::get(gs::get(&src, "value"), "effect")).to_string();
                if gs::truthy(gs::get(gs::get(&src, "modifier"), "eternal")) {
                    return Err("Cannot sell eternal card".into());
                }
                if key == "j_invisible" && d["jokers"][i]["counter"].as_f64().unwrap_or(0.0) >= 2.0 {
                    return Err("selling a ready Invisible Joker draws RNG (duplicate_random_joker): not ported".into());
                }
                let sell = int(gs::get(gs::get(&src, "cost"), "sell"));
                d["money"] = json!(int(&d["money"]) + sell);
                d["jokers"].as_array_mut().unwrap().remove(i);
                owned.remove(i);
                joker_passives(&mut d, &key, &text, is_negative(&src), -1);
                bump_jokers(&mut d, "Campfire", 0.25);
            }
            "buy" => {
                let k = *shop_idx.get(i).ok_or_else(|| format!("Invalid shop index {i}"))?;
                let src = &shop[k];
                let id = next_id + k;
                let cost = int(gs::get(gs::get(src, "cost"), "buy"));
                if cost > int(&d["money"]) {
                    return Err("Cannot afford card".into());
                }
                let set = gs::s(gs::get(src, "set")).to_string();
                let neg = is_negative(src);
                if set == "JOKER" && !neg && d["jokers"].as_array().unwrap().len() as i64 >= int(&d["joker_slots"]) {
                    return Err("No room for joker".into());
                }
                if matches!(set.as_str(), "TAROT" | "PLANET" | "SPECTRAL")
                    && !neg
                    && d["consumables"].as_array().unwrap().len() as i64 >= int(&d["consumable_slots"])
                {
                    return Err("No room for consumable".into());
                }
                d["money"] = json!(int(&d["money"]) - cost);
                shop_idx.remove(i);
                match set.as_str() {
                    "JOKER" => {
                        let key = gs::s(gs::get(src, "key")).to_string();
                        let text = gs::s(gs::get(gs::get(src, "value"), "effect")).to_string();
                        d["jokers"].as_array_mut().unwrap().push(joker_value(src, hp, id as u64));
                        owned.push(src.clone());
                        joker_passives(&mut d, &key, &text, neg, 1);
                    }
                    "TAROT" | "PLANET" | "SPECTRAL" => {
                        let name = consumable_name(gs::s(gs::get(src, "key")))?;
                        d["consumables"].as_array_mut().unwrap().push(json!({"sort_id": id, "name": name, "negative": neg}));
                        if neg {
                            set_int(&mut d, "consumable_slots", |n| n + 1);
                        }
                    }
                    _ => {
                        d["cards"].as_array_mut().unwrap().push(playing(src, id)?);
                        bump_jokers(&mut d, "Hologram", 0.25);
                    }
                }
            }
            "voucher" => {
                let k = *vouchers_left.get(i).ok_or_else(|| format!("Invalid voucher index {i}"))?;
                let src = &vouchers[k];
                let cost = int(gs::get(gs::get(src, "cost"), "buy"));
                if cost > int(&d["money"]) {
                    return Err("Cannot afford voucher".into());
                }
                vouchers_left.remove(i);
                d["money"] = json!(int(&d["money"]) - cost);
                let key = gs::s(gs::get(src, "key")).to_string();
                d["vouchers"].as_array_mut().unwrap().push(json!(key));
                match key.as_str() {
                    "v_crystal_ball" => set_int(&mut d, "consumable_slots", |n| n + 1),
                    "v_paint_brush" | "v_palette" => set_int(&mut d, "hand_size", |n| n + 1),
                    "v_antimatter" => set_int(&mut d, "joker_slots", |n| n + 1),
                    "v_hieroglyph" | "v_petroglyph" => set_int(&mut d, "ante", |n| n - 1),
                    "v_overstock_norm" | "v_overstock_plus" => overstock = true,
                    _ => {}
                }
            }
            other => return Err(format!("unknown action {other:?}")),
        }
    }
    let pools = if overstock { Some(overstock_pools(&d, shop_idx.len(), seed)?) } else { None };
    Ok((d, pools))
}

/// The Jackdaw pseudoseed pools after the Overstock refill (`fill_shop_slots(new_max - len(shop_cards))`),
/// from rumbot's shop code over `n_shop` placeholder slots.
fn overstock_pools(d: &Value, n_shop: usize, seed: &str) -> Result<HashMap<String, f64>, String> {
    // build_state never fills Jackdaw's used_jokers, so the refill may roll a joker the player owns: no
    // jokers in the game it runs on (rumbot's pool drops owned ones and resamples).
    let mut d = d.clone();
    d["jokers"] = json!([]);
    let desc: StateDesc = serde_json::from_value(d).map_err(|e| format!("desc: {e}"))?;
    let mut g = simcore::build(&desc)?;
    g.rng = RunRng::new(seed);
    // Jackdaw polls the rental pool ('ssjr') on every stake; rumbot only from stake 8 unless all stickers are on.
    // The made cards are thrown away, only the draws count.
    g.all_stickers = true;
    let shop = g.shop.as_mut().ok_or("no shop")?;
    for _ in 0..n_shop {
        shop.slots.push(rumbot_sim::shop::ShopSlot::new("card", 0));
    }
    let v = *g.vouchers.last().ok_or("no voucher")?;
    g._redeem_voucher(v);
    Ok(g.rng.state())
}

/// The rumbot game after the option, its RNG at `RunRng::new(seed)` (plus the option's draws).
pub fn apply_option(p: &Value, opt: &Value, seed: &str) -> Result<rumbot_sim::game::GameState, String> {
    let (d, pools) = apply_option_json(p, opt, seed)?;
    game_of(d, pools, seed)
}

/// The rumbot game of an `apply_option_json` result, its RNG at `RunRng::new(seed)` plus the pools.
pub fn game_of(d: Value, pools: Option<HashMap<String, f64>>, seed: &str) -> Result<rumbot_sim::game::GameState, String> {
    let desc: StateDesc = serde_json::from_value(d).map_err(|e| format!("desc: {e}"))?;
    let mut g = simcore::build(&desc)?;
    g.rng = RunRng::new(seed);
    if let Some(pl) = pools {
        g.rng.pools = pl;
    }
    Ok(g)
}
