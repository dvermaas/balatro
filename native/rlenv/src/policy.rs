//! Strategic-decision policies: uniform random, the rollout shopper, and the network.

use crate::net::Net;
use rlcore::vocab::{decode, family_start, Family, CASH_OUT, LEAVE_SHOP, SELECT_BLIND, SKIP_PACK};
use rumbot_sim::consumables::ConsumableKind;
use rumbot_sim::game::{GameState, Phase};
use std::sync::Arc;

/// splitmix64
pub struct Rng(u64);
impl Rng {
    pub fn new(seed: u64) -> Self {
        Rng(seed)
    }
    pub fn next_u64(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^ (z >> 31)
    }
    /// uniform in [0, 1)
    pub fn f64(&mut self) -> f64 {
        (self.next_u64() >> 11) as f64 / (1u64 << 53) as f64
    }
}

pub struct Ctx<'a> {
    pub g: &'a GameState,
    pub obs_idx: &'a [u16],
    pub obs_val: &'a [f32],
    /// legal vocabulary ids, ascending
    pub legal: &'a [u16],
    pub rng: &'a mut Rng,
}

pub struct Decision {
    pub id: usize,
    /// log-probability of `id` under the distribution the policy sampled from (0 for deterministic rules)
    pub logp: f32,
    /// the network's value outputs [P(win), expected final blinds] (0 for the other policies)
    pub value: [f32; 2],
}

pub trait Policy {
    fn decide(&mut self, c: &mut Ctx) -> Decision;
}

pub struct RandomPolicy;
impl Policy for RandomPolicy {
    fn decide(&mut self, c: &mut Ctx) -> Decision {
        let n = c.legal.len();
        let k = (c.rng.next_u64() % n as u64) as usize;
        Decision { id: c.legal[k] as usize, logp: -(n as f32).ln(), value: [0.0; 2] }
    }
}

/// What `simcore::play_horizon_rebuild` does outside rounds: in the shop use held planets (`use_planets`), buy
/// the priciest joker affordable with $10 left while a slot is free (`rollout_shop`), then the most played hand's
/// planet while $4 stays (the rollout uses BuyAndUse; here buy, then use it on the next decision, and only with a
/// free consumable slot), then leave; select every blind; skip every pack; cash out.
pub struct ShopperPolicy {
    pub floor: i32,
}

impl Default for ShopperPolicy {
    fn default() -> Self {
        ShopperPolicy { floor: simcore::ROLLOUT_SHOP_FLOOR.load(std::sync::atomic::Ordering::Relaxed) }
    }
}

impl ShopperPolicy {
    fn shop(&self, c: &Ctx) -> usize {
        let g = c.g;
        let ids = rlcore::ids::ids();
        let legal = |id: usize| c.legal.binary_search(&(id as u16)).is_ok();
        // use_planets: the first held planet that is usable
        for h in &g.consumables {
            let spec = h.borrow().spec;
            if spec.kind == ConsumableKind::Planet {
                if let Some(ci) = ids.consumable(spec.name) {
                    let id = family_start(Family::UseConsumable) + ci;
                    if legal(id) {
                        return id;
                    }
                }
            }
        }
        if let Some(shop) = g.shop.as_ref() {
            if (g.jokers.len() as i32) < g.joker_slots() {
                let pick = shop
                    .slots
                    .iter()
                    .enumerate()
                    .filter(|(_, sl)| sl.joker.is_some() && g.money - g.slot_price(sl) >= self.floor)
                    .max_by_key(|(i, sl)| (g.slot_price(sl), std::cmp::Reverse(*i)));
                if let Some((_, sl)) = pick {
                    if let Some(ji) = ids.joker(sl.joker.as_ref().unwrap().borrow().spec.name) {
                        let id = family_start(Family::BuyJoker) + ji;
                        if legal(id) {
                            return id;
                        }
                    }
                }
            }
            let mut top: Option<(rumbot_sim::hands::HandType, i32)> = None;
            for (h, &n) in g.hand_levels.plays.iter() {
                if n > 0 && top.map_or(true, |t| n > t.1) {
                    top = Some((*h, n));
                }
            }
            if let Some((hand, _)) = top {
                let name = rumbot_sim::hands::planet_for_hand(hand);
                if shop.slots.iter().any(|sl| sl.consumable.is_some_and(|x| x.name == name) && g.money - g.slot_price(sl) >= 4) {
                    if let Some(ci) = ids.consumable(name) {
                        let id = family_start(Family::BuyConsumable) + ci;
                        if legal(id) {
                            return id;
                        }
                    }
                }
            }
        }
        LEAVE_SHOP
    }
}

impl Policy for ShopperPolicy {
    fn decide(&mut self, c: &mut Ctx) -> Decision {
        let want = match c.g.phase {
            Phase::Shop => self.shop(c),
            Phase::BlindSelect => SELECT_BLIND,
            Phase::Pack => SKIP_PACK,
            _ => CASH_OUT,
        };
        let id = if c.legal.binary_search(&(want as u16)).is_ok() { want } else { c.legal[0] as usize };
        Decision { id, logp: 0.0, value: [0.0; 2] }
    }
}

#[derive(Clone, Copy, Debug)]
pub enum NetMode {
    Argmax,
    /// sample from softmax(logits / temperature) over the legal ids
    Sample(f32),
}

pub struct NetPolicy {
    pub net: Arc<Net>,
    pub mode: NetMode,
}

impl Policy for NetPolicy {
    fn decide(&mut self, c: &mut Ctx) -> Decision {
        let temp = match self.mode {
            NetMode::Argmax => None,
            NetMode::Sample(t) => Some(t),
        };
        let u = if temp.is_some() { c.rng.f64() } else { 0.0 };
        let (id, logp, value) = self.net.choose(c.obs_idx, c.obs_val, c.legal, temp, u);
        Decision { id, logp, value }
    }
}

/// The family name of a vocabulary id (for stats).
pub fn family_name(id: usize) -> &'static str {
    match decode(id) {
        None => rlcore::vocab::action_name(id),
        Some((f, _)) => rlcore::vocab::FAMILIES.iter().find(|x| x.0 == f).unwrap().1,
    }
}
