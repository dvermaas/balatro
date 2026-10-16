//! PyO3 module `jev_sim`: shop-forecast rollouts on rumbot-sim (stage B of the Rust port).
//!
//! `rollouts(state_json, policy_jokers, seeds, bosses)` builds one rumbot-sim game from a converted
//! Jackdaw shop state (src/sim_bridge.py), then for each seed reseeds a copy and plays it with a
//! port of `rollout.Policy` until `bosses` boss blinds are done or the run ends, exactly like
//! `rollout.play_horizon`. The policy scores its candidates with the bot's text scorer (the shared
//! jev_kernel, the same rules src/native.py encodes).

use std::collections::{HashMap, HashSet};

use jev_kernel::{self as kernel, RuleIn, HAND_NAMES};
use pyo3::exceptions::{PyRuntimeError, PyValueError};
use pyo3::prelude::*;
use rumbot_sim::cards::{self, Card, CardRef, Edition, Enhancement, Rank, Seal, Suit};
use rumbot_sim::consumables::{self, ConsumableInstance, ConsumableKind};
use rumbot_sim::game::{Action, ActionType, GameState, Phase};
use rumbot_sim::hands::HandType;
use rumbot_sim::jokers::{self as rjokers, JokerInstance};
use rumbot_sim::rng::RunRng;
use serde::Deserialize;

// ---------------------------------------------------------------------------
// State description (src/sim_bridge.py: Jackdaw state -> JSON)
// ---------------------------------------------------------------------------

#[derive(Deserialize)]
struct CardDesc {
    sort_id: u64,
    rank: String,
    suit: String,
    #[serde(default)]
    enh: Option<String>,
    #[serde(default)]
    edition: Option<String>,
    #[serde(default)]
    seal: Option<String>,
    #[serde(default)]
    extra_chips: i32,
    #[serde(default)]
    debuff: bool,
    #[serde(default)]
    played_this_ante: bool,
}

#[derive(Deserialize, Default)]
struct JokerDesc {
    sort_id: u64,
    name: String,
    #[serde(default)]
    edition: Option<String>,
    #[serde(default)]
    eternal: bool,
    #[serde(default)]
    perishable: bool,
    #[serde(default)]
    perish_tally: i32,
    #[serde(default)]
    rental: bool,
    #[serde(default)]
    debuffed: bool,
    #[serde(default)]
    counter: Option<f64>,
    #[serde(default)]
    secondary: Option<f64>,
    #[serde(default)]
    hands_at_create: Option<i32>,
    #[serde(default)]
    named_hand: Option<String>,
    #[serde(default)]
    extra_sell_value: Option<f64>,
}

#[derive(Deserialize)]
struct ConsumableDesc {
    sort_id: u64,
    name: String,
    #[serde(default)]
    negative: bool,
    #[serde(default)]
    extra_sell_value: i32,
}

#[derive(Deserialize)]
struct LevelDesc {
    level: i32,
    played: i32,
    played_this_round: bool,
}

#[derive(Deserialize)]
struct StateDesc {
    deck: String,
    stake: i32,
    ante: i32,
    blind_index: i32,
    round: i32,
    money: i32,
    hands_played: i32,
    skips: i32,
    boss: String,
    bosses_used: HashMap<String, i32>,
    cards: Vec<CardDesc>,
    jokers: Vec<JokerDesc>,
    consumables: Vec<ConsumableDesc>,
    vouchers: Vec<String>,
    levels: HashMap<String, LevelDesc>,
    #[serde(default)]
    idol: Option<(String, String)>,
    #[serde(default)]
    mail: Option<String>,
    #[serde(default)]
    ancient: Option<String>,
    #[serde(default)]
    castle: Option<String>,
    tarots_used: i32,
    planets_used: i32,
    unique_planets: Vec<String>,
    #[serde(default)]
    pool_flags: Vec<String>,
    #[serde(default)]
    most_played: Option<String>,
    hand_size: i32,
    joker_slots: i32,
    consumable_slots: i32,
    /// v25: the Jackdaw rollout's pseudoseed streams at the handoff (rollout_items), so the rollout
    /// continues exactly; absent, every rollout starts its streams fresh from its seed.
    #[serde(default)]
    rng_pools: Option<HashMap<String, f64>>,
    /// v25: the bot's consumable use (rollout_items.use_items / tarots.plan) for strict rollouts.
    #[serde(default)]
    items: Option<ItemsDesc>,
    /// v25: G.GAME.last_tarot_planet, the card The Fool copies
    #[serde(default)]
    last_tarot_planet: Option<String>,
}

/// tarots.TARGETED and SAFE_UNTARGETED by rumbot name, and the build's flush suit (build.decide).
#[derive(Deserialize, Clone)]
struct ItemsDesc {
    /// name -> (kind, count, arg): enhance/suit/rank_up/remove/copy/best/held
    targeted: HashMap<String, (String, usize, Option<String>)>,
    /// name -> condition: "money4" (The Hermit), "joker_room" (Judgement), "jokers" (Wheel), or none
    safe: HashMap<String, Option<String>>,
    #[serde(default)]
    suit: Option<String>,
}

fn rank_of(code: &str) -> PyResult<Rank> {
    Rank::from_code(code).ok_or_else(|| PyValueError::new_err(format!("unknown rank {code:?}")))
}

fn suit_of(code: &str) -> PyResult<Suit> {
    Suit::from_code(code).ok_or_else(|| PyValueError::new_err(format!("unknown suit {code:?}")))
}

fn edition_of(e: &Option<String>) -> PyResult<Edition> {
    Ok(match e.as_deref() {
        None => Edition::None,
        Some("foil") => Edition::Foil,
        Some("holo") => Edition::Holographic,
        Some("polychrome") => Edition::Polychrome,
        Some("negative") => Edition::Negative,
        Some(x) => return Err(PyValueError::new_err(format!("unknown edition {x:?}"))),
    })
}

fn hand_of(label: &str) -> PyResult<HandType> {
    HandType::ALL
        .iter()
        .copied()
        .find(|h| h.label() == label)
        .ok_or_else(|| PyValueError::new_err(format!("unknown hand {label:?}")))
}

enum Item<'a> {
    Card(&'a CardDesc),
    Joker(&'a JokerDesc),
    Consumable(&'a ConsumableDesc),
}

/// A rumbot-sim game in the SHOP phase (shop left empty) equal to the described Jackdaw state.
fn build(desc: &StateDesc) -> PyResult<GameState> {
    let mut g = GameState::new("JEVROLLOUT", &desc.deck, desc.stake);
    g.no_shop = true;
    g.ante = desc.ante;
    g.blind_index = desc.blind_index;
    g.round_number = desc.round;
    g.money = desc.money;
    g.hands_played = desc.hands_played;
    g.blinds_skipped = desc.skips;
    g.ante_boss = desc.boss.clone();
    g.bosses_used = desc.bosses_used.clone();
    g.tarots_used = desc.tarots_used;
    g.planets_used = desc.planets_used;
    g.last_tarot_planet = desc.last_tarot_planet.clone().unwrap_or_default();
    g.unique_planets = desc.unique_planets.iter().cloned().collect();
    g.pool_flags = desc.pool_flags.iter().cloned().collect();
    g.idol_rank = None;
    g.idol_suit = None;
    if let Some((r, s)) = &desc.idol {
        g.idol_rank = Some(rank_of(r)?);
        g.idol_suit = Some(suit_of(s)?);
    }
    g.mail_rank = desc.mail.as_deref().map(rank_of).transpose()?;
    g.ancient_suit = desc.ancient.as_deref().map(suit_of).transpose()?;
    g.castle_suit = desc.castle.as_deref().map(suit_of).transpose()?;

    g.vouchers = Vec::new();
    for key in &desc.vouchers {
        let v = rumbot_sim::shop::voucher_by_key(key)
            .ok_or_else(|| PyValueError::new_err(format!("unknown voucher {key:?}")))?;
        g.vouchers.push(v);
    }

    for h in HandType::ALL {
        g.hand_levels.levels.insert(h, 1);
        g.hand_levels.plays.insert(h, 0);
    }
    g.hands_played_this_round.clear();
    for (label, lv) in &desc.levels {
        let h = hand_of(label)?;
        g.hand_levels.levels.insert(h, lv.level);
        g.hand_levels.plays.insert(h, lv.played);
        if lv.played_this_round {
            g.hands_played_this_round.insert(h);
        }
    }

    match &desc.most_played {
        Some(label) => g.most_played_hand = hand_of(label)?,
        None => g._snapshot_most_played(),
    }

    // Every card, joker and consumable in Jackdaw sort_id order, so rumbot-sim's uids (which order
    // shuffles and random picks) keep the same relative order.
    let mut items: Vec<(u64, Item)> = Vec::new();
    items.extend(desc.cards.iter().map(|c| (c.sort_id, Item::Card(c))));
    items.extend(desc.jokers.iter().map(|j| (j.sort_id, Item::Joker(j))));
    items.extend(desc.consumables.iter().map(|c| (c.sort_id, Item::Consumable(c))));
    items.sort_by_key(|(sid, _)| *sid);
    let mut deck: Vec<(u64, CardRef)> = Vec::new();
    let mut jokers = Vec::new();
    let mut cons = Vec::new();
    for (sid, item) in items {
        match item {
            Item::Card(c) => {
                let mut card = Card::new(rank_of(&c.rank)?, suit_of(&c.suit)?);
                card.enhancement = match c.enh.as_deref() {
                    None => Enhancement::None,
                    Some("bonus") => Enhancement::Bonus,
                    Some("mult") => Enhancement::Mult,
                    Some("wild") => Enhancement::Wild,
                    Some("glass") => Enhancement::Glass,
                    Some("steel") => Enhancement::Steel,
                    Some("stone") => Enhancement::Stone,
                    Some("gold") => Enhancement::Gold,
                    Some("lucky") => Enhancement::Lucky,
                    Some(x) => return Err(PyValueError::new_err(format!("unknown enhancement {x:?}"))),
                };
                card.edition = edition_of(&c.edition)?;
                card.seal = match c.seal.as_deref() {
                    None => Seal::None,
                    Some("gold") => Seal::Gold,
                    Some("red") => Seal::Red,
                    Some("blue") => Seal::Blue,
                    Some("purple") => Seal::Purple,
                    Some(x) => return Err(PyValueError::new_err(format!("unknown seal {x:?}"))),
                };
                card.extra_chips = c.extra_chips;
                card.debuffed = c.debuff;
                card.played_this_ante = c.played_this_ante;
                deck.push((sid, cards::wrap(card)));
            }
            Item::Joker(j) => {
                let spec = rjokers::spec(&j.name)
                    .ok_or_else(|| PyValueError::new_err(format!("unknown joker {:?}", j.name)))?;
                let mut inst = JokerInstance::new(spec);
                inst.edition = edition_of(&j.edition)?;
                inst.eternal = j.eternal;
                inst.perishable = j.perishable;
                inst.perish_tally = j.perish_tally;
                inst.rental = j.rental;
                inst.debuffed = j.debuffed;
                if let Some(v) = j.counter {
                    inst.counter = v;
                }
                if let Some(v) = j.secondary {
                    inst.secondary = v;
                }
                if let Some(v) = j.hands_at_create {
                    inst.hands_at_create = v;
                }
                if let Some(v) = j.extra_sell_value {
                    inst.extra_sell_value = v;
                }
                inst.named_hand = j.named_hand.as_deref().map(hand_of).transpose()?;
                jokers.push(rjokers::make_ref(inst));
            }
            Item::Consumable(c) => {
                let spec = consumables::spec(&c.name)
                    .ok_or_else(|| PyValueError::new_err(format!("unknown consumable {:?}", c.name)))?;
                let mut inst = ConsumableInstance::new(spec, if c.negative { Edition::Negative } else { Edition::None });
                inst.extra_sell_value = c.extra_sell_value;
                cons.push(consumables::make_ref(inst));
            }
        }
    }
    g.full_deck = deck.iter().map(|(_, c)| c.clone()).collect();
    g.draw_pile = g.full_deck.clone();
    g.hand = Vec::new();
    g.discard_pile = Vec::new();
    g.jokers = jokers;
    g.consumables = cons;

    // Sizes: rumbot-sim derives them from vouchers, jokers and the deck; set the bases so they come
    // out as Jackdaw's.
    g.base_hand_size += desc.hand_size - g.hand_size();
    g.base_joker_slots += desc.joker_slots - g.joker_slots();
    g.extra_consumable_slots += desc.consumable_slots - g.consumable_slots();

    g.blind = None;
    g.beaten_blind = None;
    g.pending_payout = 0;
    g.chips_scored = 0;
    g.shop = Some(Default::default());
    g.phase = Phase::Shop;
    Ok(g)
}

// ---------------------------------------------------------------------------
// rollout.Policy
// ---------------------------------------------------------------------------

/// rollout._info + native.encode_cards for one rumbot-sim card.
fn info(c: &CardRef) -> kernel::Card {
    let c = c.borrow();
    kernel::Card {
        rank: c.rank as u8,
        suit: match c.suit {
            Suit::Hearts => 1,
            Suit::Diamonds => 2,
            Suit::Clubs => 3,
            Suit::Spades => 4,
        },
        enh: match c.enhancement {
            Enhancement::None => 0,
            Enhancement::Stone => 1,
            Enhancement::Wild => 2,
            Enhancement::Bonus => 3,
            Enhancement::Mult => 4,
            Enhancement::Glass => 5,
            Enhancement::Steel => 6,
            Enhancement::Gold | Enhancement::Lucky => 7,
        },
        edition: match c.edition {
            Edition::None => 0,
            Edition::Foil => 1,
            Edition::Holographic => 2,
            Edition::Polychrome => 3,
            Edition::Negative => 4,
        },
        red: c.seal == Seal::Red,
        debuff: c.debuffed,
    }
}

/// rollout._rv: rank order index, -1 for Stone or rankless.
fn rv(c: &kernel::Card) -> i32 {
    if c.rank != 0 && c.enh != kernel::STONE {
        c.rank as i32 - 2
    } else {
        -1
    }
}

fn straight(infos: &[kernel::Card], live: &[usize]) -> Option<Vec<usize>> {
    const ORDER: [u8; 14] = [14, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14];
    let mut by: HashMap<u8, usize> = HashMap::new();
    for &p in live {
        let r = infos[p].rank;
        if r != 0 && infos[p].enh != kernel::STONE {
            by.entry(r).or_insert(p);
        }
    }
    for top in (4..ORDER.len()).rev() {
        let run = &ORDER[top - 4..=top];
        if run.iter().all(|r| by.contains_key(r)) {
            return Some(run.iter().map(|r| by[r]).collect());
        }
    }
    None
}

/// rollout.candidates
fn candidates(infos: &[kernel::Card]) -> Vec<Vec<usize>> {
    let n = infos.len();
    let ranked: Vec<usize> = (0..n).filter(|&p| infos[p].rank != 0 && infos[p].enh != kernel::STONE).collect();
    let mut groups: Vec<(u8, Vec<usize>)> = Vec::new(); // insertion order, like a dict
    for &p in &ranked {
        match groups.iter_mut().find(|(r, _)| *r == infos[p].rank) {
            Some((_, g)) => g.push(p),
            None => groups.push((infos[p].rank, vec![p])),
        }
    }
    let mut gl: Vec<Vec<usize>> = groups.into_iter().map(|(_, g)| g).collect();
    // sorted(key=(len, rv of first), reverse=True): stable, so equal keys keep insertion order
    gl.sort_by(|a, b| (b.len(), rv(&infos[b[0]])).cmp(&(a.len(), rv(&infos[a[0]]))));
    let mut out: Vec<Vec<usize>> = Vec::new();
    let multi: Vec<&Vec<usize>> = gl.iter().filter(|g| g.len() >= 2).collect();
    for g in multi.iter().take(3) {
        out.push(g.iter().take(5).copied().collect());
    }
    if multi.len() >= 2 {
        let (a, b) = (multi[0], multi[1]);
        let mut c: Vec<usize> = a.iter().take(if a.len() >= 3 { 3 } else { 2 }).copied().collect();
        c.extend(b.iter().take(2));
        out.push(c);
    }
    for s in 1..=4u8 {
        let mut cards: Vec<usize> = ranked.iter().copied().filter(|&p| infos[p].suit == s || infos[p].enh == kernel::WILD).collect();
        if cards.len() >= 5 {
            cards.sort_by_key(|&p| -rv(&infos[p])); // stable
            out.push(cards.into_iter().take(5).collect());
        }
    }
    if let Some(st) = straight(infos, &ranked) {
        out.push(st);
    }
    if !ranked.is_empty() {
        let mut best = ranked[0];
        for &p in &ranked {
            if rv(&infos[p]) > rv(&infos[best]) {
                best = p;
            }
        }
        out.push(vec![best]);
    } else if n > 0 {
        out.push(vec![0]);
    }
    out
}

struct Policy {
    jokers: Vec<(u8, Option<RuleIn>)>,
}

/// kernel hand index (HAND_NAMES order) -> rumbot-sim HandType
fn hand_type(i: usize) -> HandType {
    hand_of(HAND_NAMES[i]).expect("kernel hand names are rumbot-sim labels")
}

impl Policy {
    fn scorer(&self, g: &GameState) -> kernel::Scorer {
        let levels = (0..HAND_NAMES.len())
            .map(|i| {
                let h = hand_type(i);
                let (chips, mult) = g.hand_levels.values(h);
                (chips as f64, mult as f64, g.hand_levels.played(h) as f64, g.hands_played_this_round.contains(&h))
            })
            .collect();
        // rnd = {"hands_left": ..., "discards_left": ...}; score() reads `or 1` / `or 0`
        let hl = if g.hands_left != 0 { g.hands_left } else { 1 } as f64;
        kernel::Scorer::new(levels, self.jokers.clone(), hl, g.discards_left as f64).expect("12 levels")
    }

    /// rollout.Policy.decide: (discard?, hand positions)
    fn decide(&self, g: &GameState) -> (bool, Vec<usize>) {
        let infos: Vec<kernel::Card> = g.hand.iter().map(info).collect();
        let bname = g.boss().map(|b| b.name).unwrap_or("");
        let scorer = self.scorer(g);
        let hands_left = g.hands_left as i64;
        let discards_left = g.discards_left as i64;
        let target = g.blind.as_ref().map(|b| b.target).unwrap_or(0);
        let need = (target - g.chips_scored).max(0);
        let played_round: u32 = (0..HAND_NAMES.len())
            .filter(|&i| g.hands_played_this_round.contains(&hand_type(i)))
            .fold(0, |m, i| m | (1 << i));
        let forced: Vec<usize> = match &g.forced_card {
            Some(f) => {
                let uid = cards::uid_of(f);
                g.hand.iter().position(|c| cards::uid_of(c) == uid).into_iter().collect()
            }
            None => vec![],
        };
        let mut best: Option<(f64, Vec<usize>, usize)> = None;
        for combo in candidates(&infos) {
            let mut combo = combo;
            if bname == "The Psychic" && combo.len() < 5 {
                let mut rest: Vec<usize> = (0..infos.len()).filter(|p| !combo.contains(p)).collect();
                rest.sort_by_key(|&p| rv(&infos[p]));
                let k = 5 - combo.len();
                combo.extend(rest.into_iter().take(k));
            }
            for &f in &forced {
                if !combo.contains(&f) {
                    if combo.len() >= 5 {
                        combo.truncate(4);
                    }
                    combo.push(f);
                }
            }
            combo.sort_unstable();
            combo.dedup();
            let cs: Vec<kernel::Card> = combo.iter().map(|&p| infos[p]).collect();
            let (name, sc) = kernel::classify(&cs);
            let mut s = scorer.score(&infos, &combo, name, &sc);
            let in_round = played_round & (1 << name) != 0;
            if (bname == "The Eye" && in_round) || (bname == "The Mouth" && played_round != 0 && !in_round) {
                s = -1.0;
            }
            if best.as_ref().map_or(true, |b| s > b.0) {
                best = Some((s, combo, name));
            }
        }
        let (score, combo, name) = best.expect("a non-empty hand has a candidate");
        let behind = score * (hands_left as f64) < need as f64;
        if discards_left > 0 && hands_left > 1 && behind {
            let toss = discard(&infos, &forced);
            if !toss.is_empty() {
                return (true, toss);
            }
        }
        if discards_left == 0 && hands_left > 1 && behind && combo.len() < 5 {
            return (false, self.cycle(&scorer, &infos, combo, name, score));
        }
        (false, combo)
    }

    /// rollout.Policy.cycle
    fn cycle(&self, scorer: &kernel::Scorer, infos: &[kernel::Card], combo: Vec<usize>, name: usize, score: f64) -> Vec<usize> {
        let mut spare: Vec<usize> = (0..infos.len()).filter(|p| !combo.contains(p)).collect();
        spare.sort_by_key(|&p| (!infos[p].debuff, rv(&infos[p])));
        let mut out = combo;
        for p in spare {
            if out.len() >= 5 {
                break;
            }
            let mut trial = out.clone();
            trial.push(p);
            trial.sort_unstable();
            let cs: Vec<kernel::Card> = trial.iter().map(|&q| infos[q]).collect();
            let (n2, sc2) = kernel::classify(&cs);
            if n2 == name && scorer.score(infos, &trial, n2, &sc2) >= score {
                out = trial;
            }
        }
        out
    }
}

/// rollout.Policy.discard
fn discard(infos: &[kernel::Card], forced: &[usize]) -> Vec<usize> {
    let n = infos.len();
    let live: Vec<usize> = (0..n).filter(|&p| !infos[p].debuff).collect();
    // Counter.most_common(1): highest count, ties by first insertion
    let mut suits: Vec<(u8, usize)> = Vec::new();
    let bump = |s: u8, suits: &mut Vec<(u8, usize)>| match suits.iter_mut().find(|(k, _)| *k == s) {
        Some((_, c)) => *c += 1,
        None => suits.push((s, 1)),
    };
    for &p in &live {
        let i = infos[p];
        if i.enh == kernel::WILD {
            for s in [1u8, 2, 3, 4] {
                bump(s, &mut suits);
            }
        } else if i.suit != 0 && i.enh != kernel::STONE {
            bump(i.suit, &mut suits);
        }
    }
    let (s, cnt) = suits.iter().fold(None, |best: Option<(u8, usize)>, &(k, c)| match best {
        Some((_, bc)) if bc >= c => best,
        _ => Some((k, c)),
    }).unwrap_or((0, 0));
    let keep: Vec<usize> = if cnt >= 4 {
        live.iter().copied().filter(|&p| infos[p].suit == s || infos[p].enh == kernel::WILD).collect()
    } else {
        let mut ranks: HashMap<u8, usize> = HashMap::new();
        for &p in &live {
            if infos[p].rank != 0 {
                *ranks.entry(infos[p].rank).or_insert(0) += 1;
            }
        }
        let k: Vec<usize> = live.iter().copied().filter(|&p| ranks.get(&infos[p].rank).copied().unwrap_or(0) >= 2).collect();
        if k.is_empty() {
            let mut hi = live.clone();
            hi.sort_by_key(|&p| -rv(&infos[p]));
            hi.into_iter().take(2).collect()
        } else {
            k
        }
    };
    let mut toss: Vec<usize> = (0..n).filter(|p| !keep.contains(p)).collect();
    toss.sort_by_key(|&p| (!infos[p].debuff, rv(&infos[p])));
    toss.truncate(5);
    for &f in forced {
        if !toss.contains(&f) {
            toss.truncate(4);
            toss.push(f);
        }
    }
    toss
}

// ---------------------------------------------------------------------------
// rollout.play_horizon
// ---------------------------------------------------------------------------

type Row = (i32, String, String, i64, i64, bool);

fn act(g: &mut GameState, t: ActionType) {
    g.step(&Action::new(t));
}

/// rollout.use_planets
const SHOP: u8 = 0;
const BLIND_SELECT: u8 = 1;
const SELECTING_HAND: u8 = 2;

#[allow(non_snake_case)]
fn NO_ITEMS() -> ItemsDesc {
    ItemsDesc { targeted: HashMap::new(), safe: HashMap::new(), suit: None }
}

/// build.card_value: how much a hand card matters to the build (debuffed cards are never ranked).
fn card_value(c: &Card, suit: Option<&str>) -> f64 {
    if c.debuffed {
        return -1.0;
    }
    if c.enhancement == Enhancement::Stone {
        return 0.3;
    }
    let mut v = ("23456789TJQKA".find(c.rank.short()).unwrap() + 2) as f64 / 14.0;
    if c.enhancement != Enhancement::None || c.edition != Edition::None || c.seal != Seal::None {
        v += 0.5;
    }
    if let Some(s) = suit {
        v += if c.suit.as_str() == s || c.enhancement == Enhancement::Wild { 2.0 } else { 0.0 };
    }
    v
}

fn enh_code(e: Enhancement) -> &'static str {
    match e {
        Enhancement::None => "",
        Enhancement::Bonus => "BONUS",
        Enhancement::Mult => "MULT",
        Enhancement::Wild => "WILD",
        Enhancement::Glass => "GLASS",
        Enhancement::Steel => "STEEL",
        Enhancement::Stone => "STONE",
        Enhancement::Gold => "GOLD",
        Enhancement::Lucky => "LUCKY",
    }
}

/// tarots.plan: hand positions to target, Ok(None) when no use fits the build, Err when the Python
/// version would raise (best/held with every card debuffed).
fn plan(kind: &str, n: usize, arg: Option<&str>, hand: &[CardRef], suit: Option<&str>) -> Result<Option<Vec<usize>>, ()> {
    let cs: Vec<std::cell::Ref<Card>> = hand.iter().map(|c| c.borrow()).collect();
    if cs.len() < (if kind == "copy" { 2 } else { 1 }) {
        return Ok(None);
    }
    let idx: Vec<usize> = (0..cs.len()).filter(|&i| !cs[i].debuffed).collect();
    let val: Vec<f64> = (0..cs.len()).map(|i| card_value(&cs[i], suit)).collect();
    let mut order = idx.clone();
    order.sort_by(|&a, &b| val[b].partial_cmp(&val[a]).unwrap()); // stable, like sorted(key=-val)
    let some = |p: Vec<usize>| Ok(if p.is_empty() { None } else { Some(p) });
    match kind {
        "enhance" => {
            let arg = arg.unwrap_or("");
            let fresh: Vec<usize> = order.iter().copied().filter(|&i| cs[i].enhancement == Enhancement::None).collect();
            let pick: Vec<usize> = if arg == "WILD" {
                let Some(s) = suit else { return Ok(None) };
                fresh.into_iter().filter(|&i| cs[i].suit.as_str() != s).take(n).collect()
            } else if arg == "STEEL" || arg == "GOLD" || arg == "STONE" {
                fresh.into_iter().rev().take(n).collect()
            } else {
                fresh.into_iter().take(n).collect()
            };
            some(pick)
        }
        "suit" => {
            let arg = arg.unwrap_or("");
            if let Some(s) = suit {
                if s != arg {
                    return Ok(None);
                }
            }
            let pick: Vec<usize> = order.iter().rev().copied()
                .filter(|&i| cs[i].suit.as_str() != arg && !matches!(enh_code(cs[i].enhancement), "WILD" | "STONE"))
                .take(n).collect();
            some(pick)
        }
        "remove" => Ok(Some(order.iter().rev().copied().take(n).collect())),
        "rank_up" => some(order.iter().copied().filter(|&i| cs[i].rank != Rank::Ace).take(n).collect()),
        "copy" => {
            let mut best: Option<(usize, usize)> = None;
            for &i in &idx {
                for &j in &idx {
                    if i < j && best.map_or(true, |(a, b)| val[j] - val[i] > val[b] - val[a]) {
                        best = Some((i, j));
                    }
                }
            }
            match best {
                Some((i, j)) if val[j] - val[i] >= 0.3 => Ok(Some(vec![i, j])),
                _ => Ok(None),
            }
        }
        "best" => order.first().map(|&i| Some(vec![i])).ok_or(()),
        "held" => order.last().map(|&i| Some(vec![i])).ok_or(()),
        _ => Ok(None),
    }
}

/// rollout_items.use_items: up to 8 uses, each the first usable card in slot order (planets always;
/// safe untargeted cards when their condition holds; targeted cards in a round, on tarots.plan's
/// targets). A card the engine refuses is not retried in the same state.
fn use_items(g: &mut GameState, it: &ItemsDesc, state: u8, failed: &mut HashSet<(String, u8)>) -> Result<(), String> {
    for _ in 0..8 {
        if g.consumables.is_empty() {
            return Ok(());
        }
        let free = g.joker_slots() - g.jokers.len() as i32;
        let mut chosen: Option<(usize, String, Vec<usize>)> = None;
        for (i, c) in g.consumables.iter().enumerate() {
            let spec = c.borrow().spec;
            let name = spec.name.to_string();
            if spec.kind == ConsumableKind::Planet {
                chosen = Some((i, name, vec![]));
                break;
            }
            if failed.contains(&(name.clone(), state)) {
                continue;
            }
            if let Some(cond) = it.safe.get(&name) {
                match cond.as_deref() {
                    Some("money4") if g.money < 4 => continue,
                    Some("joker_room") if free < 1 => continue,
                    Some("jokers") if g.jokers.is_empty() => continue,
                    _ => {}
                }
                chosen = Some((i, name, vec![]));
                break;
            }
            if state == SELECTING_HAND {
                if let Some((kind, n, arg)) = it.targeted.get(&name) {
                    match plan(kind, *n, arg.as_deref(), &g.hand, it.suit.as_deref()) {
                        Ok(Some(p)) => {
                            chosen = Some((i, name, p));
                            break;
                        }
                        Ok(None) => {}
                        Err(()) => return Err(STRICT_ABORT.into()),
                    }
                }
            }
        }
        let Some((i, name, mut targets)) = chosen else { return Ok(()) };
        targets.sort_unstable();
        let a = Action { r#type: ActionType::UseConsumable, index: i as i32, cards: targets };
        if g.is_legal(&a) {
            g.step(&a);
        } else {
            failed.insert((name, state));
        }
    }
    Ok(())
}

fn use_planets(g: &mut GameState) {
    let mut i = 0;
    while i < g.consumables.len() {
        let planet = g.consumables[i].borrow().spec.kind == ConsumableKind::Planet;
        if planet {
            let a = Action::at(ActionType::UseConsumable, i as i32);
            if g.is_legal(&a) {
                g.step(&a);
                continue;
            }
        }
        i += 1;
    }
}

fn card_str(c: &CardRef) -> String {
    let c = c.borrow();
    let mut s = format!("{}{}", c.rank.short(), c.suit.as_str());
    if c.seal != Seal::None {
        s.push_str(&format!("[{}]", c.seal.as_str()));
    }
    if c.enhancement != Enhancement::None {
        s.push_str(&format!("[{}]", c.enhancement.as_str()));
    }
    s
}

fn play_horizon(g: &mut GameState, policy: &Policy, bosses: usize, max_steps: usize) -> Result<Vec<Row>, String> {
    play_horizon_traced(g, policy, bosses, max_steps, None, None)
}

/// Returned by a strict rollout that reached a state rollout_items.play_horizon handles differently.
const STRICT_ABORT: &str = "strict: needs the Python rollout";

fn joker_names(g: &GameState) -> Vec<String> {
    g.jokers.iter().map(|j| j.borrow().name().to_string()).collect()
}

/// items (v25, rollout_items): consumables are used like rollout_items.use_items (planets, safe
/// untargeted cards, tarots.plan targets in a round), and the jokers just after each blind select
/// and after each use in a round must be the starting ones (Python rebuilds its policy when they
/// change): otherwise STRICT_ABORT, and Python carries on with that rollout. Without items: the
/// shop forecast's loop (rollout.play_horizon), planets only.
fn play_horizon_traced(g: &mut GameState, policy: &Policy, bosses: usize, max_steps: usize, mut trace: Option<&mut Vec<String>>, items: Option<&ItemsDesc>) -> Result<Vec<Row>, String> {
    let strict = items.is_some();
    let mut failed: HashSet<(String, u8)> = HashSet::new();
    let mut out: Vec<Row> = Vec::new();
    let mut cur: Option<(i32, String, String, i64)> = None;
    let mut done = 0;
    let mut recorded = false;
    let start_jokers = if strict { joker_names(g) } else { Vec::new() };
    for _ in 0..max_steps {
        match g.phase {
            Phase::Shop => {
                match items {
                    Some(it) => use_items(g, it, SHOP, &mut failed)?,
                    None => use_planets(g),
                }
                act(g, ActionType::LeaveShop);
            }
            Phase::BlindSelect => {
                let typ = match g.blind_index {
                    0 => "Small",
                    1 => "Big",
                    _ => "Boss",
                };
                if let Some(it) = items {
                    if !g.consumables.is_empty() {
                        use_items(g, it, BLIND_SELECT, &mut failed)?;
                    }
                }
                act(g, ActionType::SelectBlind);
                failed.retain(|f| f.1 != SELECTING_HAND); // the bot retries targeted cards each round
                // Python rebuilds its policy right after the select (Madness, Riff-raff act in it)
                if strict && joker_names(g) != start_jokers {
                    return Err(STRICT_ABORT.into());
                }
                let b = g.blind.as_ref().ok_or("no blind after select")?;
                cur = Some((g.ante, typ.to_string(), b.name(), b.target));
                recorded = false;
            }
            Phase::Playing => {
                let (ante, typ, boss, target) = cur.clone().ok_or("playing without a blind")?;
                if g.hand.is_empty() {
                    out.push((ante, typ, boss, target, g.chips_scored, false));
                    return Ok(out);
                }
                if let Some(it) = items {
                    if !g.consumables.is_empty() {
                        use_items(g, it, SELECTING_HAND, &mut failed)?;
                        if joker_names(g) != start_jokers {
                            return Err(STRICT_ABORT.into()); // Python rebuilds its policy here
                        }
                    }
                }
                let (is_discard, idx) = policy.decide(g);
                if let Some(t) = trace.as_mut() {
                    let hand: Vec<String> = g.hand.iter().map(card_str).collect();
                    let counters: Vec<String> = g.jokers.iter().map(|j| format!("{:?}", j.borrow().counter)).collect();
                    t.push(format!("{} {:?} hand={} chips={} hl={} dl={} counters=[{}]", if is_discard { "discard" } else { "play" }, idx,
                                   hand.join(","), g.chips_scored, g.hands_left, g.discards_left, counters.join(",")));
                }
                let t = if is_discard { ActionType::Discard } else { ActionType::Play };
                let mut idx = idx;
                idx.sort_unstable(); // rumbot-sim wants positions in order; Jackdaw takes any order
                let a = Action::with_cards(t, idx);
                if g.is_legal(&a) {
                    g.step(&a);
                } else {
                    let forced: Vec<usize> = match &g.forced_card {
                        Some(f) => {
                            let uid = cards::uid_of(f);
                            g.hand.iter().position(|c| cards::uid_of(c) == uid).into_iter().collect()
                        }
                        None => vec![],
                    };
                    g.step(&Action::with_cards(ActionType::Play, if forced.is_empty() { vec![0] } else { forced }));
                }
            }
            Phase::RoundEval | Phase::GameOver | Phase::Won => {
                let (ante, typ, boss, target) = cur.clone().ok_or("round end without a blind")?;
                let cleared = g.phase != Phase::GameOver;
                if !recorded {
                    out.push((ante, typ.clone(), boss, target, g.chips_scored, cleared));
                    recorded = true;
                }
                let is_boss = typ == "Boss";
                if is_boss {
                    done += 1;
                }
                // Jackdaw sets `won` when the final boss is beaten (still in ROUND_EVAL).
                let won = g.phase == Phase::Won || (is_boss && cleared && ante >= 8 && !g.endless);
                if !cleared || done >= bosses || won {
                    return Ok(out);
                }
                act(g, ActionType::CashOut);
            }
            Phase::Pack => act(g, ActionType::SkipPack),
        }
    }
    Err("rollout did not finish".into())
}

// ---------------------------------------------------------------------------
// Python API
// ---------------------------------------------------------------------------

/// Rollouts of one option from a described shop state: one per seed (rollout._seed(k)), each a
/// list of per-blind rows (ante, type, blind name, target, chips, cleared).
#[pyfunction]
fn rollouts(state_json: &str, policy_jokers: Vec<(u8, Option<RuleIn>)>, seeds: Vec<String>, bosses: usize) -> PyResult<Vec<Vec<Row>>> {
    let desc: StateDesc = serde_json::from_str(state_json).map_err(|e| PyValueError::new_err(e.to_string()))?;
    let base = build(&desc)?;
    let policy = Policy { jokers: policy_jokers };
    let mut out = Vec::with_capacity(seeds.len());
    for seed in seeds {
        let mut g = base.deep_clone();
        g.seed = seed.clone();
        g.rng = RunRng::new(seed);
        if let Some(pools) = &desc.rng_pools {
            g.rng.pools = pools.clone(); // v25: an option that drew random numbers continues its streams
        }
        out.push(play_horizon(&mut g, &policy, bosses, 600).map_err(PyRuntimeError::new_err)?);
    }
    Ok(out)
}

/// v25, rollout_items: one strict rollout from a described shop state mid-rollout, its pseudoseed
/// streams continued from `rng_pools`. None when it reaches a state only the Python rollout handles
/// (see play_horizon_traced); the caller then replays it in Python.
#[pyfunction]
fn rollout_strict(state_json: &str, policy_jokers: Vec<(u8, Option<RuleIn>)>, seed: String, bosses: usize) -> PyResult<Option<Vec<Row>>> {
    let desc: StateDesc = serde_json::from_str(state_json).map_err(|e| PyValueError::new_err(e.to_string()))?;
    let mut g = build(&desc)?;
    g.seed = seed.clone();
    g.rng = RunRng::new(seed);
    if let Some(pools) = &desc.rng_pools {
        g.rng.pools = pools.clone();
    }
    let policy = Policy { jokers: policy_jokers };
    let no_items = NO_ITEMS();
    match play_horizon_traced(&mut g, &policy, bosses, 900, None, Some(desc.items.as_ref().unwrap_or(&no_items))) {
        Ok(rows) => Ok(Some(rows)),
        Err(e) if e == STRICT_ABORT => Ok(None),
        Err(e) => Err(PyRuntimeError::new_err(e)),
    }
}

/// One rollout with a decision log: (rows, ["play [..] hand=.. chips=..", ...]).
#[pyfunction]
fn rollout_trace(state_json: &str, policy_jokers: Vec<(u8, Option<RuleIn>)>, seed: String, bosses: usize) -> PyResult<(Vec<Row>, Vec<String>)> {
    let desc: StateDesc = serde_json::from_str(state_json).map_err(|e| PyValueError::new_err(e.to_string()))?;
    let mut g = build(&desc)?;
    g.seed = seed.clone();
    g.rng = RunRng::new(seed);
    let policy = Policy { jokers: policy_jokers };
    let mut log = Vec::new();
    let rows = play_horizon_traced(&mut g, &policy, bosses, 600, Some(&mut log), None).map_err(PyRuntimeError::new_err)?;
    Ok((rows, log))
}

/// Registered names, for the Python side's mapping checks: (jokers, consumables, voucher keys, boss keys).
#[pyfunction]
fn names() -> (Vec<String>, Vec<String>, Vec<String>, Vec<String>) {
    (
        rjokers::all_specs().iter().map(|s| s.name.to_string()).collect(),
        consumables::all_specs().iter().map(|s| s.name.to_string()).collect(),
        rumbot_sim::shop::all_vouchers().iter().map(|v| v.key.to_string()).collect(),
        rumbot_sim::boss_data::BOSS_DATA.iter().map(|b| b.key.to_string()).collect(),
    )
}

#[pymodule]
fn jev_sim(m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_function(wrap_pyfunction!(rollouts, m)?)?;
    m.add_function(wrap_pyfunction!(names, m)?)?;
    m.add_function(wrap_pyfunction!(rollout_trace, m)?)?;
    m.add_function(wrap_pyfunction!(rollout_strict, m)?)?;
    Ok(())
}
