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

// ---------------------------------------------------------------------------
// rollout_items.forecast / _forecast (lines 416-470)
// ---------------------------------------------------------------------------

const TARGETED_KEYS: [&str; 21] = [
    "c_magician", "c_empress", "c_heirophant", "c_lovers", "c_chariot", "c_justice", "c_devil", "c_tower", "c_strength",
    "c_hanged_man", "c_death", "c_star", "c_moon", "c_sun", "c_world", "c_aura", "c_deja_vu", "c_talisman", "c_medium",
    "c_trance", "c_cryptid",
];
const SAFE_KEYS: [&str; 8] = ["c_hermit", "c_temperance", "c_high_priestess", "c_emperor", "c_judgement", "c_wheel_of_fortune", "c_fool", "c_black_hole"];

/// rollout_items._items_desc(b): tarots.TARGETED / SAFE_UNTARGETED by rumbot name and the build's flush suit.
pub fn items_desc(suit: Option<&str>) -> simcore::ItemsDesc {
    let mut targeted = std::collections::HashMap::new();
    for k in TARGETED_KEYS {
        let (kind, n, arg) = crate::tarots::targeted(k).expect("targeted");
        let name = crate::forecast_state::consumable_name(k).expect("name");
        targeted.insert(name.to_string(), (kind.to_string(), n, arg.map(str::to_string)));
    }
    let mut safe = std::collections::HashMap::new();
    for k in SAFE_KEYS {
        let cond = match k {
            "c_hermit" => Some("money4"),
            "c_judgement" => Some("joker_room"),
            "c_wheel_of_fortune" => Some("jokers"),
            _ => None,
        };
        safe.insert(crate::forecast_state::consumable_name(k).expect("name").to_string(), cond.map(str::to_string));
    }
    simcore::ItemsDesc { targeted, safe, suit: suit.map(str::to_string) }
}

/// rollout.Policy(_area_cards(gs, "jokers")) on the engine's view of the jokers.
fn policy_of_game(g: GameState) -> (GameState, simcore::Policy) {
    let mut eng = RumbotGame::new();
    eng.g = Some(g);
    let st = eng.state().expect("state");
    let pol = crate::forecast::policy_of(&cards(&st, "jokers"));
    (eng.g.take().unwrap(), pol)
}

pub fn items_forecast(pub_: &Value, n: usize, bosses: i64, keys: Option<&[String]>, budget_s: Option<f64>) -> Value {
    let t0 = std::time::Instant::now();
    items_forecast_clock(pub_, n, bosses, keys, budget_s, &|| t0.elapsed().as_secs_f64())
}

pub fn items_forecast_clock(pub_: &Value, n: usize, bosses: i64, keys: Option<&[String]>, budget_s: Option<f64>, clock: &dyn Fn() -> f64) -> Value {
    let all = crate::forecast_stats::items_options(pub_);
    let mut picker = Picker::new(pub_);
    let b = crate::build::decide(pub_);
    let items = items_desc(gs::get(&b, "suit").as_str());
    let opts: Vec<&Value> = all
        .iter()
        .filter(|o| {
            let key = gs::s(gs::get(o, "key"));
            keys.map_or(true, |ks| ks.iter().any(|k| k == key) || key == "skip")
        })
        .collect();
    let mut errors: Vec<Option<String>> = vec![None; opts.len()];
    let mut runs: Vec<Vec<Value>> = vec![Vec::new(); opts.len()];
    // pols: policy cache keyed by the jokers' keys (rollout_items._jsig)
    let mut pols: std::collections::HashMap<Vec<String>, simcore::Policy> = std::collections::HashMap::new();
    let mut k = 0usize;
    while k < n {
        for (oi, o) in opts.iter().enumerate() {
            if errors[oi].is_some() {
                continue;
            }
            let sd = crate::forecast::seed(k);
            let applied = match apply_item_option(pub_, o, &sd, &mut picker) {
                Ok(a) => a,
                Err(e) => {
                    errors[oi] = Some(e);
                    continue;
                }
            };
            let (mut g, pol) = {
                let sig: Vec<String> = applied.g.jokers.iter().map(|j| j.borrow().spec.name.to_string()).collect();
                let (g, p) = {
                    policy_of_game(applied.g)
                };
                let p = pols.entry(sig).or_insert(p).clone();
                (g, p)
            };
            let pols_ref = std::cell::RefCell::new(&mut pols);
            let mut rebuild = |gm: &GameState| -> simcore::Policy {
                let sig: Vec<String> = gm.jokers.iter().map(|j| j.borrow().spec.name.to_string()).collect();
                if let Some(p) = pols_ref.borrow().get(&sig) {
                    return p.clone();
                }
                let (_, p) = policy_of_game(gm.deep_clone());
                pols_ref.borrow_mut().insert(sig, p.clone());
                p
            };
            let rows = simcore::play_horizon_rebuild(&mut g, &pol, bosses as usize, 900, None, Some(&items), Some(&mut rebuild)).expect("items rollout");
            let rj = Value::Array(rows.iter().map(crate::forecast::row_json).collect());
            if std::env::var("FC_ROWS").is_ok() {
                eprintln!("ROWS {} {} {}", gs::s(gs::get(o, "key")), k, rj);
            }
            runs[oi].push(rj);
        }
        k += 1;
        if let Some(bd) = budget_s.filter(|b| *b != 0.0) {
            if clock() > bd && k >= 8 {
                break;
            }
        }
    }
    let mut rmap = serde_json::Map::new();
    let mut with_supported: Vec<Value> = Vec::new();
    for (oi, o) in opts.iter().enumerate() {
        let key = gs::s(gs::get(o, "key")).to_string();
        let mut o2 = (*o).clone();
        o2["supported"] = json!(errors[oi].is_none());
        with_supported.push(o2);
        rmap.insert(key, Value::Array(std::mem::take(&mut runs[oi])));
    }
    let mut out = crate::forecast_stats::assemble(&with_supported, None, &rmap, bosses, k as i64);
    let mut m = serde_json::Map::new();
    for key in ["options", "rollouts", "bosses"] {
        m.insert(key.into(), out[key].clone());
    }
    m.insert("seconds".into(), json!(pycompat::pyround(clock(), 2)));
    let mut errs = serde_json::Map::new();
    for (oi, o) in opts.iter().enumerate() {
        if let Some(e) = &errors[oi] {
            errs.insert(gs::s(gs::get(o, "key")).to_string(), json!(e));
        }
    }
    m.insert("errors".into(), Value::Object(errs));
    Value::Object(m)
}
