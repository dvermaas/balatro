//! The action vocabulary: a fixed global list of strategic actions keyed by (action type, item
//! identity), not by slot. Covers the BlindSelect, RoundEval, Shop and Pack phases; the Playing phase
//! (plays, discards, in-round item use) belongs to the round player.
//!
//! Decisions (see also README in `lib.rs`):
//! - Duplicates: one vocabulary entry stands for every slot holding that item. `to_action` resolves it
//!   to one slot with a fixed rule (`canonical`): buys and pack picks take the best copy (edition
//!   Negative > Polychrome > Holo > Foil > none, then fewer stickers, then cheaper, then leftmost);
//!   sells take the worst copy (lowest edition, then leftmost); consumables and packs take the cheapest,
//!   then leftmost.
//! - Playing cards (a Standard pack's options, or a shop's cards under Magic Trick) have no small
//!   identity, so they are addressed by slot (`pick_pack_card[slot]`, `buy_card[slot]`) and the
//!   observation carries each slot's card.
//! - Card targets (`Action.cards`: a Tarot used or picked from an Arcana pack while a hand is dealt) are
//!   not part of the vocabulary: the entry names the item, and `to_action` takes the targets from a hint
//!   when legal, else the first legal target set (`legal_actions` order). A target chooser plugs in there.
//! - Joker order (`SwapJokerLeft`) is left to code and is not in the vocabulary; `BuyAndUse` is not
//!   offered by rumbot's `legal_actions` and is not either.

use crate::ids::*;
use crate::obs::{PACK_SLOTS, SHOP_CARD_SLOTS};
use rumbot_sim::cards::Edition;
use rumbot_sim::game::{Action, ActionType, GameState, PackChoice, Phase};
use std::sync::OnceLock;

pub const SELECT_BLIND: usize = 0;
pub const SKIP_BLIND: usize = 1;
pub const REROLL_BOSS: usize = 2;
pub const CASH_OUT: usize = 3;
pub const REROLL: usize = 4;
pub const LEAVE_SHOP: usize = 5;
pub const SKIP_PACK: usize = 6;
const N_SINGLE: usize = 7;
const SINGLE_NAMES: [&str; N_SINGLE] = ["select_blind", "skip_blind", "reroll_boss", "cash_out", "reroll", "leave_shop", "skip_pack"];

/// Item-keyed families, in vocabulary order after the singles.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Family {
    BuyJoker,
    BuyConsumable,
    BuyVoucher,
    BuyPack,
    BuyCard,
    SellJoker,
    SellConsumable,
    UseConsumable,
    PickPackJoker,
    PickPackConsumable,
    PickPackCard,
}

pub const FAMILIES: [(Family, &str, usize); 11] = [
    (Family::BuyJoker, "buy_joker", N_JOKERS),
    (Family::BuyConsumable, "buy_consumable", N_CONSUMABLES),
    (Family::BuyVoucher, "buy_voucher", N_VOUCHERS),
    (Family::BuyPack, "buy_pack", N_PACK_KINDS),
    (Family::BuyCard, "buy_card", SHOP_CARD_SLOTS),
    (Family::SellJoker, "sell_joker", N_JOKERS),
    (Family::SellConsumable, "sell_consumable", N_CONSUMABLES),
    (Family::UseConsumable, "use_consumable", N_CONSUMABLES),
    (Family::PickPackJoker, "pick_pack_joker", N_JOKERS),
    (Family::PickPackConsumable, "pick_pack_consumable", N_CONSUMABLES),
    (Family::PickPackCard, "pick_pack_card", PACK_SLOTS),
];

struct Table {
    starts: Vec<usize>,
    names: Vec<String>,
}

static TABLE: OnceLock<Table> = OnceLock::new();

fn table() -> &'static Table {
    TABLE.get_or_init(|| {
        let ids = ids();
        let mut names: Vec<String> = SINGLE_NAMES.iter().map(|s| s.to_string()).collect();
        let mut starts = Vec::new();
        for (f, fname, n) in FAMILIES {
            starts.push(names.len());
            for i in 0..n {
                let item = match f {
                    Family::BuyJoker | Family::SellJoker | Family::PickPackJoker => ids.joker_keys[i].to_string(),
                    Family::BuyConsumable | Family::SellConsumable | Family::UseConsumable | Family::PickPackConsumable => ids.consumable_keys[i].to_string(),
                    Family::BuyVoucher => ids.voucher_keys[i].to_string(),
                    Family::BuyPack => pack_name(i),
                    Family::BuyCard | Family::PickPackCard => format!("slot{i}"),
                };
                names.push(format!("{fname}:{item}"));
            }
        }
        Table { starts, names }
    })
}

/// Size of the vocabulary.
pub fn n_actions() -> usize {
    table().names.len()
}

pub fn action_name(id: usize) -> &'static str {
    &table().names[id]
}

pub fn family_start(f: Family) -> usize {
    table().starts[FAMILIES.iter().position(|x| x.0 == f).unwrap()]
}

fn id_of(f: Family, item: usize) -> usize {
    family_start(f) + item
}

/// (family, item) of a vocabulary id; None for the singles.
pub fn decode(id: usize) -> Option<(Family, usize)> {
    let t = table();
    if id < N_SINGLE {
        return None;
    }
    let k = t.starts.iter().rposition(|&s| s <= id)?;
    Some((FAMILIES[k].0, id - t.starts[k]))
}

/// Why a concrete action has no vocabulary entry.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Unexpressible {
    /// Play / Discard: the round player's.
    RoundAction,
    /// SwapJokerLeft: ordering is code's.
    JokerOrder,
    /// BuyAndUse (not in rumbot's legal set).
    BuyAndUse,
    /// A slot index past the slot-addressed families' range.
    SlotOutOfRange,
    /// An item whose key is not in the frozen vocabulary.
    UnknownItem,
    /// The index does not address anything in this state.
    BadIndex,
}

impl Unexpressible {
    pub fn as_str(self) -> &'static str {
        match self {
            Unexpressible::RoundAction => "round_action",
            Unexpressible::JokerOrder => "joker_order",
            Unexpressible::BuyAndUse => "buy_and_use",
            Unexpressible::SlotOutOfRange => "slot_out_of_range",
            Unexpressible::UnknownItem => "unknown_item",
            Unexpressible::BadIndex => "bad_index",
        }
    }
}

/// The vocabulary entry of a concrete action on the state it is taken from (before stepping).
pub fn vocab_of(g: &GameState, a: &Action) -> Result<usize, Unexpressible> {
    use Unexpressible::*;
    let ids = ids();
    let i = a.index;
    let ix = if i >= 0 { Some(i as usize) } else { None };
    let known = |x: Option<usize>| x.ok_or(UnknownItem);
    Ok(match a.r#type {
        ActionType::SelectBlind => SELECT_BLIND,
        ActionType::SkipBlind => SKIP_BLIND,
        ActionType::RerollBoss => REROLL_BOSS,
        ActionType::CashOut => CASH_OUT,
        ActionType::Reroll => REROLL,
        ActionType::LeaveShop => LEAVE_SHOP,
        ActionType::SkipPack => SKIP_PACK,
        ActionType::Play | ActionType::Discard => return Err(RoundAction),
        ActionType::SwapJokerLeft => return Err(JokerOrder),
        ActionType::BuyAndUse => return Err(BuyAndUse),
        ActionType::Buy => {
            let slot = ix.and_then(|k| g.shop.as_ref()?.slots.get(k)).ok_or(BadIndex)?;
            if let Some(j) = &slot.joker {
                id_of(Family::BuyJoker, known(ids.joker(j.borrow().spec.name))?)
            } else if let Some(c) = slot.consumable {
                id_of(Family::BuyConsumable, known(ids.consumable(c.name))?)
            } else if slot.card.is_some() {
                let k = ix.unwrap();
                if k >= SHOP_CARD_SLOTS {
                    return Err(SlotOutOfRange);
                }
                id_of(Family::BuyCard, k)
            } else {
                return Err(BadIndex);
            }
        }
        ActionType::BuyVoucher => {
            let v = ix.and_then(|k| g.shop.as_ref()?.vouchers_on_offer().get(k)).ok_or(BadIndex)?;
            id_of(Family::BuyVoucher, known(ids.voucher(v.key))?)
        }
        ActionType::BuyPack => {
            let p = ix.and_then(|k| g.shop.as_ref()?.packs.get(k)).ok_or(BadIndex)?;
            id_of(Family::BuyPack, pack_id(p))
        }
        ActionType::SellJoker => {
            let j = ix.and_then(|k| g.jokers.get(k)).ok_or(BadIndex)?;
            id_of(Family::SellJoker, known(ids.joker(j.borrow().spec.name))?)
        }
        ActionType::SellConsumable | ActionType::UseConsumable => {
            let c = ix.and_then(|k| g.consumables.get(k)).ok_or(BadIndex)?;
            let f = if a.r#type == ActionType::SellConsumable { Family::SellConsumable } else { Family::UseConsumable };
            id_of(f, known(ids.consumable(c.borrow().spec.name))?)
        }
        ActionType::PickPack => {
            let k = ix.ok_or(BadIndex)?;
            match g.pack_options.get(k).ok_or(BadIndex)? {
                PackChoice::Joker(j) => id_of(Family::PickPackJoker, known(ids.joker(j.borrow().spec.name))?),
                PackChoice::Consumable(spec) => id_of(Family::PickPackConsumable, known(ids.consumable(spec.name))?),
                PackChoice::Card(_) => {
                    if k >= PACK_SLOTS {
                        return Err(SlotOutOfRange);
                    }
                    id_of(Family::PickPackCard, k)
                }
            }
        }
    })
}

/// Whether the vocabulary covers this phase (the agent's decision phases).
pub fn strategic_phase(p: Phase) -> bool {
    matches!(p, Phase::BlindSelect | Phase::RoundEval | Phase::Shop | Phase::Pack)
}

/// The legal concrete actions of a strategic phase, each with its vocabulary id (unexpressible ones —
/// joker swaps — dropped). Empty outside the strategic phases (in Playing it would enumerate plays).
pub fn legal_with_ids(g: &GameState) -> Vec<(usize, Action)> {
    if !strategic_phase(g.phase) {
        return Vec::new();
    }
    g.legal_actions().into_iter().filter_map(|a| vocab_of(g, &a).ok().map(|v| (v, a))).collect()
}

/// The legal vocabulary ids, ascending and unique.
pub fn legal_mask_ids(g: &GameState) -> Vec<u16> {
    let mut v: Vec<u16> = legal_with_ids(g).into_iter().map(|(v, _)| v as u16).collect();
    v.sort_unstable();
    v.dedup();
    v
}

/// Dense boolean mask of length `n_actions()`.
pub fn legal_mask(g: &GameState) -> Vec<bool> {
    let mut m = vec![false; n_actions()];
    for v in legal_mask_ids(g) {
        m[v as usize] = true;
    }
    m
}

fn edition_rank(e: Edition) -> i32 {
    match e {
        Edition::None => 0,
        Edition::Foil => 1,
        Edition::Holographic => 2,
        Edition::Polychrome => 3,
        Edition::Negative => 4,
    }
}

/// Sort key for the slot a vocabulary entry resolves to (smaller is preferred); `index` breaks ties.
fn canonical(g: &GameState, a: &Action) -> (i32, i32, i32, i32) {
    let i = a.index.max(0) as usize;
    let joker_key = |j: &rumbot_sim::jokers::JokerRef, price: i32| {
        let j = j.borrow();
        let stickers = j.eternal as i32 + j.perishable as i32 + j.rental as i32;
        (-edition_rank(j.edition), stickers, price, a.index)
    };
    match a.r#type {
        ActionType::Buy => {
            let slot = &g.shop.as_ref().unwrap().slots[i];
            let price = g.slot_price(slot);
            match &slot.joker {
                Some(j) => joker_key(j, price),
                None => (0, 0, price, a.index),
            }
        }
        ActionType::BuyPack => (0, 0, g.pack_price(&g.shop.as_ref().unwrap().packs[i]), a.index),
        ActionType::PickPack => match &g.pack_options[i] {
            PackChoice::Joker(j) => joker_key(j, 0),
            _ => (0, 0, 0, a.index),
        },
        ActionType::SellJoker => (edition_rank(g.jokers[i].borrow().edition), 0, 0, a.index),
        _ => (0, 0, 0, a.index),
    }
}

/// The concrete action for vocabulary id `id` in state `g`, or None when it is not legal here.
/// `targets`: preferred card targets, used when they are legal for the chosen slot.
pub fn to_action(g: &GameState, id: usize, targets: Option<&[usize]>) -> Option<Action> {
    let cands: Vec<Action> = legal_with_ids(g).into_iter().filter(|(v, _)| *v == id).map(|(_, a)| a).collect();
    to_action_from(g, &cands, targets)
}

/// `to_action` over precomputed candidates (all legal, all with the same vocabulary id).
pub fn to_action_from(g: &GameState, cands: &[Action], targets: Option<&[usize]>) -> Option<Action> {
    let best = cands.iter().min_by_key(|a| canonical(g, a))?;
    let same: Vec<&Action> = cands.iter().filter(|a| a.index == best.index).collect();
    if let Some(t) = targets {
        let mut t = t.to_vec();
        t.sort_unstable();
        if let Some(a) = same.iter().find(|a| a.cards == t) {
            return Some((*a).clone());
        }
    }
    Some(same[0].clone())
}

/// Whether pack option `slot` can be taken now (any legal pick of it).
pub fn pack_pickable(g: &GameState, slot: usize) -> bool {
    match g.pack_options.get(slot) {
        Some(PackChoice::Joker(j)) => g.room_for_joker(j),
        Some(PackChoice::Card(_)) => true,
        Some(PackChoice::Consumable(spec)) => !g._pack_consumable_actions(slot, spec).is_empty(),
        None => false,
    }
}

/// The vocabulary as JSON (names in id order).
pub fn vocab_json() -> String {
    let t = table();
    let names: Vec<String> = t.names.iter().map(|n| format!("\"{n}\"")).collect();
    let fams: Vec<String> = FAMILIES
        .iter()
        .enumerate()
        .map(|(k, (_, name, n))| format!("{{\"name\":\"{name}\",\"start\":{},\"len\":{n}}}", t.starts[k]))
        .collect();
    format!("{{\"singles\":[{}],\"families\":[{}],\"names\":[{}]}}", SINGLE_NAMES.iter().map(|s| format!("\"{s}\"")).collect::<Vec<_>>().join(","), fams.join(","), names.join(","))
}
