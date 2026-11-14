//! One full Red Deck / White Stake game on a real `rumbot_sim` GameState.
//!
//! - BlindSelect, RoundEval, Shop, Pack: the policy picks one rlcore vocabulary id among the legal ones;
//!   `rlcore::vocab::to_action_from` maps it to the rumbot action.
//! - Playing: `simcore::play_round`, the forecast rollouts' round player (`Policy::decide` via `policy_step`), with
//!   in-round item use as the items forecast's rollouts do it (`simcore::use_items` with the bot's
//!   `forecast_items::items_desc`). Its joker policy is the bot's (`forecast::policy_of(view::jokers_json(g))`,
//!   as `value_model::power_log` builds it), rebuilt at every round start and refreshed / rebuilt in the round
//!   when jokers change (`simcore::refresh_or_rebuild`).
//! - Code, not the policy: card targets (`simcore::plan` on the build's suit, as the items rollouts target),
//!   joker order (v77's rule: `strategy::dagger_last` then `readiness::best_joker_order`, in the shop after any
//!   joker change and at every round start).
//!
//! Everything a game needs is owned by `play_game`'s frame; the bot's thread-local order cache and rumbot's
//! sort-id counter are reset at the start, so a game depends only on (seed, policy, sampling seed).

use crate::policy::{Ctx, Policy};
use jevbot::engine::RumbotGame;
use rlcore::obs::Encoder;
use rlcore::vocab::{legal_with_ids, strategic_phase, to_action_from};
use rumbot_sim::game::{Action, ActionType, GameState, PackChoice, Phase};
use serde_json::Value;
use std::collections::HashSet;

#[derive(Clone)]
pub struct EnvCfg {
    pub max_decisions: usize,
    /// `simcore::power` (log10, 32 hands, as the lab's snaps.power) at each ante's first shop
    pub power: bool,
    /// keep per-decision records for a trajectory file
    pub record: bool,
    pub order: JokerOrder,
}

/// How code orders the jokers (after joker changes in the shop, and at every round start).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum JokerOrder {
    /// v77: Ceremonial Dagger last, then `readiness::best_joker_order` (12 sampled deals; ~10 ms a call)
    Search,
    /// v56's rule (strategy.rs, lab `no-order-search`): Dagger last, xMult jokers to the right, nothing around a
    /// copier; cheap
    Xmult,
    /// leave the order as the game made it
    None,
}

impl Default for EnvCfg {
    fn default() -> Self {
        EnvCfg { max_decisions: 3000, power: false, record: false, order: JokerOrder::Search }
    }
}

/// One policy decision, in the rlb record layout plus the RL fields.
pub struct StepRec {
    pub step: u32,
    pub ante: i16,
    pub phase: u8,
    pub blind_index: u8,
    pub round: u16,
    pub money: i32,
    pub blinds_cleared: u16,
    pub action_id: u16,
    pub atype: u8,
    pub aindex: i16,
    pub cards: Vec<u8>,
    pub obs_idx: Vec<u16>,
    pub obs_val: Vec<f32>,
    pub legal: Vec<u16>,
    pub logp: f32,
    pub value: [f32; 2],
    /// blinds cleared between this decision and the next one (or the game's end)
    pub reward: f32,
}

#[derive(Default)]
pub struct GameOut {
    pub seed: String,
    pub won: bool,
    pub ante: i32,
    pub blinds: i32,
    pub round: i32,
    pub decisions: usize,
    pub steps: usize,
    pub truncated: bool,
    /// the round ended with an empty hand (items destroyed it), counted as a loss
    pub empty_hand: bool,
    pub targets_planned: usize,
    pub targets_fallback: usize,
    pub reorders: usize,
    /// (ante, log10 power samples) at each ante's first shop
    pub power: Vec<(i32, Vec<f32>)>,
    pub power_s: f64,
    /// seconds in the round player / joker ordering and suit (views) / the policy's decide
    pub round_s: f64,
    pub order_s: f64,
    pub policy_s: f64,
    pub records: Vec<StepRec>,
    pub error: Option<String>,
}

/// The gamestate view of `g` (BalatroBot shape), built by the bot's rumbot backend.
fn view_of(eng: &mut RumbotGame) -> Value {
    eng.state().expect("view")
}

/// v77's joker order (strategy.rs auto_action, SHOP / SELECTING_HAND branch): Ceremonial Dagger last, then the
/// searched best order (`readiness::best_joker_order`, 12 sampled deals) when it keeps the Dagger last.
fn order_jokers(eng: &mut RumbotGame, mode: JokerOrder) -> bool {
    match mode {
        JokerOrder::None => false,
        JokerOrder::Xmult => order_xmult(eng.g.as_mut().unwrap()),
        JokerOrder::Search => order_search(eng),
    }
}

/// strategy.rs's `no-order-search` branch (v56), after its Dagger-last rule.
fn order_xmult(g: &mut GameState) -> bool {
    if g.jokers.len() < 2 {
        return false;
    }
    let js = jevbot::engine::view::jokers_json(g);
    let labels: Vec<&str> = js.iter().map(|j| jevbot::gs::get(j, "label").as_str().unwrap_or("")).collect();
    let ident: Vec<usize> = (0..js.len()).collect();
    let mut order = jevbot::strategy::dagger_last(&labels, ident.clone());
    if order == ident && !labels.iter().any(|l| jevbot::strategy::COPIERS.contains(l)) {
        let x: Vec<bool> = js.iter().map(|j| jevbot::strategy::joker_kind(jevbot::gs::get(jevbot::gs::get(j, "value"), "effect")) == "xmult").collect();
        order = (0..x.len()).filter(|&i| !x[i]).collect();
        order.extend((0..x.len()).filter(|&i| x[i]));
    }
    if order == ident {
        return false;
    }
    g.jokers = order.iter().map(|&i| g.jokers[i].clone()).collect();
    true
}

fn order_search(eng: &mut RumbotGame) -> bool {
    let mut moved = false;
    for _ in 0..2 {
        let g = eng.g.as_ref().unwrap();
        if g.jokers.len() < 2 {
            return moved;
        }
        let view = view_of(eng);
        let labels: Vec<String> = jevbot::gs::cards_of(&view, "jokers").iter().map(|j| jevbot::gs::get(j, "label").as_str().unwrap_or("").to_string()).collect();
        let lr: Vec<&str> = labels.iter().map(String::as_str).collect();
        let ident: Vec<usize> = (0..labels.len()).collect();
        let dl = jevbot::strategy::dagger_last(&lr, ident.clone());
        let order = if dl != ident {
            Some(dl)
        } else {
            jevbot::readiness::best_joker_order(&view, 12).filter(|o| !lr.contains(&"Ceremonial Dagger") || jevbot::strategy::dagger_last(&lr, o.clone()) == *o)
        };
        match order {
            Some(o) if o != ident && o.len() == labels.len() => {
                let g = eng.g.as_mut().unwrap();
                g.jokers = o.iter().map(|&i| g.jokers[i].clone()).collect();
                moved = true;
            }
            _ => return moved,
        }
    }
    moved
}

fn build_suit(eng: &mut RumbotGame) -> Option<String> {
    let view = view_of(eng);
    jevbot::build::decide(&view).get("suit").and_then(Value::as_str).map(str::to_string)
}

fn round_policy(g: &GameState) -> simcore::Policy {
    jevbot::forecast::policy_of(&jevbot::engine::view::jokers_json(g))
}

fn uids(g: &GameState) -> Vec<u64> {
    g.jokers.iter().map(|j| j.borrow().uid).collect()
}

/// Card targets for a targeted consumable (held or in a pack): `simcore::plan` with the items rollouts' table.
fn plan_targets(g: &GameState, name: &str, suit: Option<&str>, items: &simcore::ItemsDesc) -> Option<Vec<usize>> {
    let (kind, n, arg) = items.targeted.get(name)?;
    match simcore::plan(kind, *n, arg.as_deref(), &g.hand, suit) {
        Ok(Some(mut p)) => {
            p.sort_unstable();
            Some(p)
        }
        _ => None,
    }
}

fn final_blinds(g: &GameState) -> i32 {
    match g.phase {
        Phase::GameOver => g.round_number - 1,
        _ => g.round_number,
    }
}

pub fn play_game(seed: &str, policy: &mut dyn Policy, rng_seed: u64, cfg: &EnvCfg) -> GameOut {
    rumbot_sim::cards::set_sort_id(0);
    jevbot::readiness::reset_mix_cache();
    let mut eng = RumbotGame::new();
    eng.g = Some(GameState::new(seed, "Red Deck", 1));
    let mut out = GameOut { seed: seed.to_string(), ..Default::default() };
    let mut rng = crate::policy::Rng::new(rlcore_seed_hash(seed) ^ rng_seed.wrapping_mul(0x9E37_79B9_7F4A_7C15));
    let mut enc = Encoder::new();
    let mut ordered_uids: Option<Vec<u64>> = None;
    let mut power_antes: HashSet<i32> = HashSet::new();
    let mut steps = 0usize;
    loop {
        let phase = {
            let g = eng.g.as_ref().unwrap();
            if g.is_over() {
                break;
            }
            g.phase
        };
        match phase {
            Phase::Playing => {
                // round start: v77's round-start joker order, the build's suit, a fresh round policy
                let t = std::time::Instant::now();
                if order_jokers(&mut eng, cfg.order) {
                    out.reorders += 1;
                }
                // the build's suit steers item targets only: skip the view when no item is held
                let suit = if eng.g.as_ref().unwrap().consumables.is_empty() { None } else { build_suit(&mut eng) };
                out.order_s += t.elapsed().as_secs_f64();
                let t = std::time::Instant::now();
                let items = jevbot::forecast_items::items_desc(suit.as_deref());
                let g = eng.g.as_mut().unwrap();
                let mut pol = round_policy(g);
                let mut failed = HashSet::new();
                let mut rebuild = |gm: &GameState| round_policy(gm);
                match simcore::play_round(g, &mut pol, Some(&items), &mut failed, Some(&mut rebuild), 5000) {
                    Ok(true) => {}
                    Ok(false) => {
                        out.empty_hand = true;
                        break;
                    }
                    Err(e) => {
                        out.error = Some(e);
                        break;
                    }
                }
                out.round_s += t.elapsed().as_secs_f64();
                ordered_uids = None;
            }
            p if strategic_phase(p) => {
                if out.decisions >= cfg.max_decisions {
                    out.truncated = true;
                    break;
                }
                if p == Phase::Shop {
                    if ordered_uids.as_ref() != Some(&uids(eng.g.as_ref().unwrap())) {
                        let t = std::time::Instant::now();
                        if order_jokers(&mut eng, cfg.order) {
                            out.reorders += 1;
                        }
                        ordered_uids = Some(uids(eng.g.as_ref().unwrap()));
                        out.order_s += t.elapsed().as_secs_f64();
                    }
                    let g = eng.g.as_ref().unwrap();
                    if cfg.power && power_antes.insert(g.ante) {
                        let t = std::time::Instant::now();
                        let pw = jevbot::value_model::power_log(g, 32);
                        out.power_s += t.elapsed().as_secs_f64();
                        out.power.push((g.ante, pw.iter().map(|x| *x as f32).collect()));
                    }
                }
                let g = eng.g.as_ref().unwrap();
                let (oi, ov) = enc.encode(g);
                let legal = legal_with_ids(g);
                let mut mask: Vec<u16> = legal.iter().map(|(v, _)| *v as u16).collect();
                mask.sort_unstable();
                mask.dedup();
                if mask.is_empty() {
                    out.error = Some(format!("no legal action in {:?}", g.phase));
                    break;
                }
                let t = std::time::Instant::now();
                let d = policy.decide(&mut Ctx { g, obs_idx: &oi, obs_val: &ov, legal: &mask, rng: &mut rng });
                out.policy_s += t.elapsed().as_secs_f64();
                let cands: Vec<Action> = legal.iter().filter(|(v, _)| *v == d.id).map(|(_, a)| a.clone()).collect();
                let Some(mut a) = to_action_from(g, &cands, None) else {
                    out.error = Some(format!("policy chose illegal id {}", d.id));
                    break;
                };
                if !a.cards.is_empty() {
                    // a targeted consumable: code picks the cards
                    let name = match a.r#type {
                        ActionType::PickPack => match &g.pack_options[a.index as usize] {
                            PackChoice::Consumable(spec) => Some(spec.name),
                            _ => None,
                        },
                        ActionType::UseConsumable => Some(g.consumables[a.index as usize].borrow().spec.name),
                        _ => None,
                    };
                    let suit = if name.is_some() { build_suit(&mut eng) } else { None };
                    let g = eng.g.as_ref().unwrap();
                    let items = jevbot::forecast_items::items_desc(suit.as_deref());
                    match name.and_then(|n| plan_targets(g, n, suit.as_deref(), &items)) {
                        Some(t) => match to_action_from(g, &cands, Some(&t)) {
                            Some(b) if b.cards == t => {
                                a = b;
                                out.targets_planned += 1;
                            }
                            _ => out.targets_fallback += 1,
                        },
                        None => out.targets_fallback += 1,
                    }
                }
                let g = eng.g.as_mut().unwrap();
                if cfg.record {
                    out.records.push(StepRec {
                        step: steps as u32,
                        ante: g.ante as i16,
                        phase: rlcore::log::phase_code(g.phase),
                        blind_index: g.blind_index.clamp(0, 255) as u8,
                        round: g.round_number.max(0) as u16,
                        money: g.money,
                        blinds_cleared: g.round_number.max(0) as u16,
                        action_id: d.id as u16,
                        atype: rlcore::log::action_type_code(a.r#type),
                        aindex: a.index as i16,
                        cards: a.cards.iter().map(|&c| c as u8).collect(),
                        obs_idx: oi,
                        obs_val: ov,
                        legal: mask,
                        logp: d.logp,
                        value: d.value,
                        reward: 0.0,
                    });
                }
                g.step(&a);
                out.decisions += 1;
                steps += 1;
            }
            _ => break,
        }
    }
    let g = eng.g.as_ref().unwrap();
    out.won = g.phase == Phase::Won;
    out.ante = if out.won { 8 } else { g.ante };
    out.blinds = if out.empty_hand { g.round_number - 1 } else { final_blinds(g) };
    out.round = g.round_number;
    out.steps = steps;
    // rewards: blinds cleared until the next decision
    let n = out.records.len();
    for i in 0..n {
        let next = if i + 1 < n { out.records[i + 1].blinds_cleared as i32 } else { out.blinds };
        out.records[i].reward = (next - out.records[i].blinds_cleared as i32) as f32;
    }
    out
}

pub fn rlcore_seed_hash(seed: &str) -> u64 {
    rumbot_sim::state::fnv1a64(seed.as_bytes())
}
