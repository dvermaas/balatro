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
/// Per-forecast cache of the option-applied base games that do not depend on the rollout seed (the build of the pub state is
/// the same for every rollout; only `g.rng` differs, and it is replaced right after).
#[derive(Default)]
pub struct ItemCache {
    base: std::collections::HashMap<String, GameState>,
    pack_base: std::collections::HashMap<usize, GameState>,
}

pub fn apply_item_option(p: &Value, opt: &Value, seed: &str, picker: &mut Picker) -> Result<Applied, String> {
    apply_item_option_c(p, opt, seed, picker, &mut ItemCache::default())
}

pub fn apply_item_option_c(p: &Value, opt: &Value, seed: &str, picker: &mut Picker, cache: &mut ItemCache) -> Result<Applied, String> {
    let acts = opt["actions"].as_array().ok_or("actions")?;
    let pack_i = acts.iter().find(|a| a[0] == "pack").map(|a| a[1].as_u64().unwrap_or(0) as usize);
    let Some(pi) = pack_i else {
        let key = opt["actions"].to_string();
        if let Some(b) = cache.base.get(&key) {
            let mut g = b.deep_clone();
            g.rng = RunRng::new(seed);
            return Ok(Applied { g, pack: vec![], taken: vec![], view0: Value::Null });
        }
        let (d, pools) = crate::forecast_apply::apply_option_json(p, opt, seed)?;
        let desc: StateDesc = serde_json::from_value(d).map_err(|e| format!("desc: {e}"))?;
        let mut g = simcore::build(&desc)?;
        g.rng = RunRng::new(seed);
        match pools {
            Some(pl) => g.rng.pools = pl,
            None => {
                cache.base.insert(key, g.deep_clone());
            }
        }
        return Ok(Applied { g, pack: vec![], taken: vec![], view0: Value::Null });
    };
    let packs = gs::cards_of(p, "packs");
    let src = packs.get(pi).ok_or_else(|| format!("Invalid booster index {pi}"))?;
    let cost = gs::int(gs::get(gs::get(src, "cost"), "buy"));
    let mut g = if let Some(b) = cache.pack_base.get(&pi) {
        b.deep_clone()
    } else {
        let mut d = state_json_from_pub(p)?;
        if cost > d["money"].as_i64().unwrap_or(0) {
            return Err("Cannot afford booster".into());
        }
        d["money"] = json!(d["money"].as_i64().unwrap_or(0) - cost);
        let desc: StateDesc = serde_json::from_value(d).map_err(|e| format!("desc: {e}"))?;
        let g = simcore::build(&desc)?;
        cache.pack_base.insert(pi, g.deep_clone());
        g
    };
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
    // Only a Buffoon pack rolls jokers; hiding jokers for the other kinds would change the hand an Arcana/Spectral pack deals
    // (Juggler, Merry Andy...).
    let buffoon = matches!(spec.kind, rumbot_sim::shop::PackKind::Buffoon);
    let (keep, hide): (Vec<_>, Vec<_>) = owned.iter().cloned().enumerate().partition(|(_, j)| !buffoon || j.borrow().spec.on_pack_open.is_some());
    g.jokers = keep.iter().map(|(_, j)| j.clone()).collect();
    crate::prof::time("pk.open_pack", || g._open_pack(spec, false));
    g.all_stickers = false;
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
        let st = crate::prof::time("pk.state", || eng.pack_view());
        let pack = cards(&st, "pack");
        if pack0.is_none() {
            view0 = json!({"pack": pack, "money": gs::get(&st, "money"), "jokers": cards(&st, "jokers"), "lim": gs::get(gs::get(&st, "jokers"), "limit")});
            pack0 = Some(pack.iter().map(card_sig).collect::<Vec<_>>());
        }
        let m = gs::get(&st, "money");
        let lim = gs::get(gs::get(&st, "jokers"), "limit");
        let zero = json!(0);
        let choice = crate::prof::time("pk.pick", || picker.pick(&pack, &cards(&st, "hand"), if gs::truthy(m) { m } else { &zero }, &cards(&st, "jokers"),
                                 if gs::truthy(lim) { gs::int(lim) } else { 5 }));
        match choice {
            None => {
                eng.act_only("pack", &json!({"skip": true})).map_err(|e| e.message)?;
                break;
            }
            Some((idx, targets)) => {
                let params = json!({"card": idx, "targets": targets});
                // Jackdaw's use_consumeable appends Cryptid's copies to the deck without firing playing_card_added
                // (game.py:2606-2625), so Hologram does not grow; the modded game does (card.lua), rumbot follows it.
                let holo: Vec<f64> = if gs::get(&pack[idx], "key") == "c_cryptid" {
                    eng.g.as_ref().unwrap().jokers.iter().filter(|j| j.borrow().name() == "Hologram").map(|j| j.borrow().counter).collect()
                } else { vec![] };
                match crate::prof::time("pk.call", || eng.act_only("pack", &params)) {
                    Ok(()) => {
                        if !holo.is_empty() {
                            let mut it = holo.into_iter();
                            for j in eng.g.as_ref().unwrap().jokers.iter().filter(|j| j.borrow().name() == "Hologram") {
                                j.borrow_mut().counter = it.next().unwrap();
                            }
                        }
                        taken.push(gs::get(&pack[idx], "key").clone());
                    }
                    Err(_) => {
                        eng.act_only("pack", &json!({"skip": true})).map_err(|e| e.message)?;
                        break;
                    }
                }
            }
        }
    }
    let mut g = eng.g.take().unwrap();
    g.pack_ignores_played = false;
    // Python hands the post-pick state to the rollout through describe -> build, which orders the deck by sort_id;
    // rumbot puts cards a pack creates (Cryptid, Familiar...) at the front. uids grow in creation order, like sort_id.
    g.full_deck.sort_by_key(|c| c.borrow().uid);
    g.draw_pile.sort_by_key(|c| c.borrow().uid);
    // ...and carries only the rng pools, the seed (no live Lua state).
    g.seed = seed.to_string();
    g.rng.live = None;
    Ok(Applied { g: g, pack: pack0.unwrap_or_default(), taken, view0 })
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
    let all = crate::prof::time("fi.items_options", || crate::forecast_stats::items_options(pub_));
    let mut picker = crate::prof::time("fi.Picker::new", || Picker::new(pub_));
    let mut icache = ItemCache::default();
    let b = crate::prof::time("fi.build_decide", || crate::build::decide(pub_));
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
            let applied = match crate::prof::time(if o["actions"].as_array().map_or(false, |a| a.iter().any(|x| x[0] == "pack")) { "fi.apply_PACK" } else { "fi.apply_nopack" }, || apply_item_option_c(pub_, o, &sd, &mut picker, &mut icache)) {
                Ok(a) => a,
                Err(e) => {
                    errors[oi] = Some(e);
                    continue;
                }
            };
            let from_pack = !applied.pack.is_empty();
            let pack_sigs: Vec<Value> = applied.pack.clone();
            let (mut g, pol) = {
                let sig: Vec<String> = applied.g.jokers.iter().map(|j| j.borrow().spec.name.to_string()).collect();
                match pols.get(&sig) {
                    Some(p) => (applied.g, p.clone()),
                    None => {
                        let (g, p) = crate::prof::time("fi.policy_of_game", || policy_of_game(applied.g));
                        pols.insert(sig, p.clone());
                        (g, p)
                    }
                }
            };
            // Python's policy is built for Jackdaw's joker order, but the rollout's game comes from describe -> build, which
            // orders the jokers by sort_id (a pack joker created before a later shop joker lands before it); the policy stays
            // positional. Mirror both: policy above from the unsorted order, game sorted here.
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
            // Python's run_strict (Rust) gives up on a joker added mid-horizon; Python then replays the WHOLE horizon
            // on Jackdaw. Mirror that: restart from the applied state with Jackdaw's scoring quirk (Bootstraps/Bull
            // do not see the hand's card pay) and the policy rebuilds Python's loop does.
            let g0 = g.deep_clone();
            // (the Jackdaw replay after a STRICT_ABORT restarts from g0, which keeps Jackdaw's own order)
            if from_pack {
                g.jokers.sort_by_key(|j| j.borrow().uid);
            }
            if let (Ok(path), Ok(kk)) = (std::env::var("FC_CMP"), std::env::var("FC_CMPK")) {
                if kk == k.to_string() && gs::s(gs::get(o, "key")) == std::env::var("FC_CMPO").unwrap_or_default() {
                    let d: StateDesc = serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).expect("desc");
                    let mut gp = simcore::build(&d).expect("build"); gp.seed = sd.clone(); gp.rng = RunRng::new(sd.clone()); if let Some(pl) = &d.rng_pools { gp.rng.pools = pl.clone(); }
                    std::fs::write(format!("{path}.py.txt"), format!("{:#?}", gp)).unwrap();
                    std::fs::write(format!("{path}.rs.txt"), format!("{:#?}", g)).unwrap();
                }
            }
            if std::env::var("FC_ROWS").is_ok() { eprintln!("START {} {} $={} jokers={:?} deck={} draw={}", gs::s(gs::get(o, "key")), k, g.money, g.jokers.iter().map(|j| format!("{}#{}", j.borrow().spec.name, j.borrow().uid)).collect::<Vec<_>>(), g.full_deck.len(), g.draw_pile.len()); }
            let mut tr0: Vec<String> = Vec::new();
            let r0 = crate::prof::time("fi.play_horizon", || simcore::play_horizon_rebuild(&mut g, &pol, bosses as usize, 900, if std::env::var("FC_TRACE").is_ok() { Some(&mut tr0) } else { None }, Some(&items), None));
            for l in &tr0 { eprintln!("TR0 {} {} {}", gs::s(gs::get(o, "key")), k, l); }
            let rows = match r0 {
                Ok(r) => r,
                Err(e) if e == simcore::STRICT_ABORT => {
                    g = g0;
                    if std::env::var("FC_ROWS").is_ok() { eprintln!("ABORT {} {}", gs::s(gs::get(o, "key")), k); }
                    g.jackdaw_card_pay_unseen = true;
                    g.jackdaw_unmarked_jokers = gs::cards_of(pub_, "jokers").iter().map(|c| gs::s(gs::get(c, "key")).to_string()).collect(); // jokers owned before the option (pick/buy marks its own)
                    // A pack marks all its contents when it opens; the unpicked ones are released on skip/close unless another copy is still in
                    // play (Jackdaw _release_used_key, game.py:1160-1175), so an owned joker that was in the pack stays marked.
                    for sig in &pack_sigs { let k = gs::s(&sig[0]).to_string(); if let Some(p) = g.jackdaw_unmarked_jokers.iter().position(|x| *x == k) { g.jackdaw_unmarked_jokers.remove(p); } }
                    if std::env::var("FC_ROWS").is_ok() { eprintln!("UNMARKED {} {} {:?}", gs::s(gs::get(o, "key")), k, g.jackdaw_unmarked_jokers); }
                    let mut tr: Vec<String> = Vec::new();
                    let r = crate::prof::time("fi.play_horizon_abort", || simcore::play_horizon_rebuild(&mut g, &pol, bosses as usize, 900, if std::env::var("FC_TRACE").is_ok() { Some(&mut tr) } else { None }, Some(&items), Some(&mut rebuild))).expect("items rollout");
                    for l in &tr { eprintln!("TR {} {} {}", gs::s(gs::get(o, "key")), k, l); }
                    r
                }
                Err(e) => panic!("items rollout: {e}"),
            };
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
    let mut out = crate::prof::time("fi.assemble", || crate::forecast_stats::assemble(&with_supported, None, &rmap, bosses, k as i64));
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
