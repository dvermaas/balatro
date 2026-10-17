//! rollout_items.apply_option for packs (src/rollout_items.py 213-242, 396-415): Jackdaw `OpenBooster` (cost off,
//! contents from the rollout's pseudoseed streams) then `open_and_pick` (Picker over the `pack`/`hand`/`jokers`
//! views, max 6 picks, PickPackCard / SkipPack) on the rumbot engine; shop tarots/spectrals are bought as in
//! `forecast_apply`. Uses the engine's BalatroBot-shaped gamestate as the `_area_cards` views.

use crate::engine::RumbotGame;
use crate::forecast_apply::apply_option;
use crate::forecast_state::state_json_from_pub;
use crate::gs;
use crate::iface::Game;
use crate::picker::Picker;
use rumbot_sim::game::GameState;
use rumbot_sim::rng::RunRng;
use serde_json::{json, Value};
use simcore::StateDesc;

pub struct Applied {
    pub g: GameState,
    /// the opened pack's cards as `card_sig`
    pub pack: Vec<Value>,
    /// keys of the cards taken
    pub taken: Vec<Value>,
    /// the first pack view (debug)
    pub view0: Value,
}

/// (key, edition, enhancement, seal, rank, suit) of a pack card view.
pub fn card_sig(c: &Value) -> Value {
    let m = gs::get(c, "modifier");
    let v = gs::get(c, "value");
    json!([gs::get(c, "key"), gs::get(m, "edition"), gs::get(m, "enhancement"), gs::get(m, "seal"), gs::get(v, "rank"), gs::get(v, "suit")])
}

fn cards(v: &Value, area: &str) -> Vec<Value> {
    gs::cards_of(v, area).to_vec()
}

/// build_state(pub) + option (`buy_card_i` / `buy_pack_i`) at rollout seed `seed`.
pub fn apply_item_option(p: &Value, opt: &Value, seed: &str, picker: &mut Picker) -> Result<Applied, String> {
    let acts = opt["actions"].as_array().ok_or("actions")?;
    let pack_i = acts.iter().find(|a| a[0] == "pack").map(|a| a[1].as_u64().unwrap_or(0) as usize);
    let Some(pi) = pack_i else {
        return Ok(Applied { g: apply_option(p, opt, seed)?, pack: vec![], taken: vec![], view0: Value::Null });
    };
    let packs = gs::cards_of(p, "packs");
    let src = packs.get(pi).ok_or_else(|| format!("Invalid booster index {pi}"))?;
    let cost = gs::int(gs::get(gs::get(src, "cost"), "buy"));
    let mut d = state_json_from_pub(p)?;
    if cost > d["money"].as_i64().unwrap_or(0) {
        return Err("Cannot afford booster".into());
    }
    d["money"] = json!(d["money"].as_i64().unwrap_or(0) - cost);
    let desc: StateDesc = serde_json::from_value(d).map_err(|e| format!("desc: {e}"))?;
    let mut g = simcore::build(&desc)?;
    g.rng = RunRng::new(seed);
    // Jackdaw polls the rental pool (packssjr) on every stake; rumbot only from stake 8 unless all stickers are on.
    let rules = g.sticker_rules();
    g.all_stickers = true;
    g.fool_always_usable = true;
    g.pack_ignores_played = true;
    let spec = rumbot_sim::shop::pack_from_key(gs::s(gs::get(src, "key")));
    // build_state never fills Jackdaw's used_jokers, so a Buffoon pack may roll a joker the player owns: hide the
    // owned jokers from rumbot's pool while the pack fills (those with an open-pack hook, Hallucination, stay).
    let owned = std::mem::take(&mut g.jokers);
    let (keep, hide): (Vec<_>, Vec<_>) = owned.iter().cloned().enumerate().partition(|(_, j)| j.borrow().spec.on_pack_open.is_some());
    g.jokers = keep.iter().map(|(_, j)| j.clone()).collect();
    g._open_pack(spec, false);
    g.all_stickers = false;
    g.pack_ignores_played = false;
    for c in &g.pack_options {
        if let rumbot_sim::game::PackChoice::Joker(j) = c {
            let mut j = j.borrow_mut();
            j.eternal &= rules.eternals;
            j.perishable &= rules.perishables;
            j.rental &= rules.rentals;
        }
    }
    let mut all: Vec<_> = keep.into_iter().chain(hide).collect();
    let _ = std::mem::replace(&mut g.jokers, Vec::new());
    all.sort_by_key(|(i, _)| *i);
    g.jokers = all.into_iter().map(|(_, j)| j).collect();
    let mut eng = RumbotGame::new();
    eng.g = Some(g);
    let mut pack0 = None;
    let mut view0 = Value::Null;
    let mut taken = vec![];
    for _ in 0..6 {
        if eng.g.as_ref().unwrap().phase != rumbot_sim::game::Phase::Pack {
            break;
        }
        let st = eng.state().map_err(|e| e.message)?;
        let pack = cards(&st, "pack");
        if pack0.is_none() {
            view0 = json!({"pack": pack, "money": gs::get(&st, "money"), "jokers": cards(&st, "jokers"), "lim": gs::get(gs::get(&st, "jokers"), "limit")});
            pack0 = Some(pack.iter().map(card_sig).collect::<Vec<_>>());
        }
        let m = gs::get(&st, "money");
        let lim = gs::get(gs::get(&st, "jokers"), "limit");
        let zero = json!(0);
        let choice = picker.pick(&pack, &cards(&st, "hand"), if gs::truthy(m) { m } else { &zero }, &cards(&st, "jokers"),
                                 if gs::truthy(lim) { gs::int(lim) } else { 5 });
        match choice {
            None => {
                eng.call("pack", Some(&json!({"skip": true}))).map_err(|e| e.message)?;
                break;
            }
            Some((idx, targets)) => {
                let params = json!({"card": idx, "targets": targets});
                match eng.call("pack", Some(&params)) {
                    Ok(_) => taken.push(gs::get(&pack[idx], "key").clone()),
                    Err(_) => {
                        eng.call("pack", Some(&json!({"skip": true}))).map_err(|e| e.message)?;
                        break;
                    }
                }
            }
        }
    }
    Ok(Applied { g: eng.g.take().unwrap(), pack: pack0.unwrap_or_default(), taken, view0 })
}

/// The part of `sim_bridge.describe(gs, rng=True)` the pack comparison covers.
pub fn describe_lite(g: &GameState) -> Value {
    let mut pools: Vec<(String, f64)> = g.rng.state().into_iter().collect();
    pools.sort_by(|a, b| a.0.cmp(&b.0));
    json!({
        "money": g.money,
        "jokers": g.jokers.iter().map(|j| j.borrow().spec.name.to_string()).collect::<Vec<_>>(),
        "consumables": g.consumables.iter().map(|c| c.borrow().spec.name.to_string()).collect::<Vec<_>>(),
        "n_cards": g.full_deck.len(),
        "hand_size": g.hand_size(),
        "joker_slots": g.joker_slots(),
        "rng_pools": pools.into_iter().collect::<std::collections::BTreeMap<_, _>>(),
    })
}
