//! The observation: a fixed-size, mostly sparse f32 vector built from a `GameState` outside the
//! Playing phase (it also encodes in Playing, without the hand).
//!
//! Layout: named blocks, each a `rows x cols` grid (`index = offset + row * cols + col`); `schema_json()`
//! names every row and column. Item blocks are keyed by identity (joker id, consumable id, ...), never by
//! slot, except the playing cards a shop or a Standard pack offers (by slot: they have no small identity).
//!
//! ORDER-FREE / NO FUTURE: everything here is something the real game shows. The deck enters only as
//! counts over the full deck (`full_deck`); the draw pile, its order, the RNG state and anything rolled for
//! later (the next shop, the round's voucher before the shop shows it) never do. Joker order is not
//! encoded either (code owns ordering). Tests (`tests.rs`) shuffle the draw pile and reseed the RNG and
//! check the vector does not move.
//!
//! Scaling: unbounded quantities use `slog(x) = sign(x) * ln(1 + |x|)`; small counts are divided by a
//! typical size.

use crate::ids::*;
use rumbot_sim::cards::{self, CardRef, Edition};
use rumbot_sim::game::{tag_key, GameState, PackChoice, Phase};
use rumbot_sim::hands::HandType;
use rumbot_sim::jokers::JokerRef;
use std::sync::OnceLock;

pub fn slog(x: f64) -> f32 {
    (x.signum() * (1.0 + x.abs()).ln()) as f32
}

/// Max shop main-row slots whose playing cards get their own slot features (2 base + 2 Overstock vouchers).
pub const SHOP_CARD_SLOTS: usize = 6;
/// Max options in an open pack (Mega Arcana / Standard show 5).
pub const PACK_SLOTS: usize = 5;

pub struct Block {
    pub name: &'static str,
    pub offset: usize,
    pub rows: Vec<String>,
    pub cols: Vec<String>,
}

impl Block {
    pub fn len(&self) -> usize {
        self.rows.len() * self.cols.len()
    }
}

pub struct Layout {
    pub blocks: Vec<Block>,
    pub dim: usize,
}

const SCALARS: &[&str] = &[
    "ante_div8", "ante_1", "ante_2", "ante_3", "ante_4", "ante_5", "ante_6", "ante_7", "ante_8plus",
    "blind_small", "blind_big", "blind_boss",
    "ph_blind_select", "ph_round_eval", "ph_shop", "ph_pack", "ph_playing",
    "pack_arcana", "pack_celestial", "pack_standard", "pack_buffoon", "pack_spectral",
    "pack_normal", "pack_jumbo", "pack_mega", "pack_picks_left", "pack_options_left", "pack_from_tag",
    "money_slog", "money_div25", "interest_now_div5", "interest_cap_div5", "bankrupt_at_div20",
    "joker_slots_div5", "jokers_held_div5", "joker_free_div5",
    "consumable_slots_div2", "consumables_held_div2",
    "hand_size_div8", "hands_per_round_div4", "discards_per_round_div3",
    "reroll_cost_div5", "shop_rerolls_div5", "free_rerolls", "shop_coupon",
    "can_reroll_boss", "boss_rerolled",
    "deck_size_div52", "round_div24", "blinds_skipped_div5", "hands_played_slog",
    "unknown_items",
];
const CARD_COLS_N: usize = 13 + 4 + 9 + 5 + 5;

fn card_cols(prefix: &[&str]) -> Vec<String> {
    let mut v: Vec<String> = prefix.iter().map(|s| s.to_string()).collect();
    v.extend(RANK_NAMES.iter().map(|r| format!("rank_{r}")));
    v.extend(SUIT_NAMES.iter().map(|s| format!("suit_{s}")));
    v.extend(ENH_NAMES.iter().map(|s| format!("enh_{s}")));
    v.extend(EDITION_NAMES.iter().map(|s| format!("ed_{s}")));
    v.extend(SEAL_NAMES.iter().map(|s| format!("seal_{s}")));
    v
}

fn s(v: &[&str]) -> Vec<String> {
    v.iter().map(|x| x.to_string()).collect()
}

fn composition_rows() -> Vec<String> {
    let mut v = Vec::new();
    for r in RANK_NAMES {
        for su in SUIT_NAMES {
            v.push(format!("{r}{su}"));
        }
    }
    v.extend(ENH_NAMES.iter().map(|x| format!("enh_{x}")));
    v.extend(EDITION_NAMES.iter().map(|x| format!("ed_{x}")));
    v.extend(SEAL_NAMES.iter().map(|x| format!("seal_{x}")));
    v.extend(s(&["size_div52", "extra_chips_mean_div10", "extra_chips_share"]));
    v
}

static LAYOUT: OnceLock<Layout> = OnceLock::new();

pub fn layout() -> &'static Layout {
    LAYOUT.get_or_init(build_layout)
}

fn build_layout() -> Layout {
    let ids = ids();
    let jokers: Vec<String> = ids.joker_keys.iter().map(|k| k.to_string()).collect();
    let cons: Vec<String> = ids.consumable_keys.iter().map(|k| k.to_string()).collect();
    let vouchers: Vec<String> = ids.voucher_keys.iter().map(|k| k.to_string()).collect();
    let tags: Vec<String> = ids.tag_keys.iter().map(|k| k.to_string()).collect();
    let blinds: Vec<String> = ids.blind_keys.iter().map(|k| k.to_string()).collect();
    let packs: Vec<String> = (0..N_PACK_KINDS).map(pack_name).collect();
    let hands: Vec<String> = HandType::ALL.iter().map(|h| h.label().to_string()).collect();
    let one = s(&["v"]);
    let ed4 = ["foil", "holo", "polychrome", "negative"];
    let mut held_j = s(&["count"]);
    held_j.extend(s(&ed4));
    held_j.extend(s(&["eternal", "perishable", "rental", "debuffed", "sell_slog", "counter_slog", "secondary_slog", "perish_tally_div5"]));
    let mut shop_j = s(&["count"]);
    shop_j.extend(s(&ed4));
    shop_j.extend(s(&["eternal", "perishable", "rental", "cost_slog", "buyable"]));
    let mut pack_j = s(&["count"]);
    pack_j.extend(s(&ed4));
    pack_j.extend(s(&["eternal", "perishable", "rental", "pickable"]));
    let slots = |n: usize| (0..n).map(|i| format!("slot{i}")).collect::<Vec<_>>();
    let targets = {
        let mut v: Vec<String> = RANK_NAMES.iter().map(|r| format!("idol_rank_{r}")).collect();
        v.extend(SUIT_NAMES.iter().map(|x| format!("idol_suit_{x}")));
        v.extend(SUIT_NAMES.iter().map(|x| format!("ancient_suit_{x}")));
        v.extend(RANK_NAMES.iter().map(|r| format!("mail_rank_{r}")));
        v.extend(SUIT_NAMES.iter().map(|x| format!("castle_suit_{x}")));
        v
    };
    let spec: Vec<(&'static str, Vec<String>, Vec<String>)> = vec![
        ("scalars", s(SCALARS), one.clone()),
        ("boss", blinds, one.clone()),
        ("tag_small", tags.clone(), one.clone()),
        ("tag_big", tags.clone(), one.clone()),
        ("tags_held", tags, s(&["count"])),
        ("vouchers_owned", vouchers.clone(), one.clone()),
        ("shop_vouchers", vouchers, s(&["offered", "buyable"])),
        ("hand_levels", hands, s(&["level_m1_div10", "level_slog", "played_slog"])),
        ("joker_targets", targets, one.clone()),
        ("deck", composition_rows(), one.clone()),
        ("pack_hand", composition_rows(), one.clone()),
        ("held_jokers", jokers.clone(), held_j),
        ("held_consumables", cons.clone(), s(&["count", "negative"])),
        ("shop_jokers", jokers.clone(), shop_j),
        ("shop_consumables", cons.clone(), s(&["count", "cost_slog", "buyable"])),
        ("shop_cards", slots(SHOP_CARD_SLOTS), card_cols(&["present", "buyable", "cost_slog"])),
        ("shop_packs", packs, s(&["count", "cost_slog", "buyable"])),
        ("pack_jokers", jokers, pack_j),
        ("pack_consumables", cons, s(&["count", "pickable"])),
        ("pack_cards", slots(PACK_SLOTS), card_cols(&["present"])),
    ];
    let mut blocks = Vec::new();
    let mut off = 0;
    for (name, rows, cols) in spec {
        let b = Block { name, offset: off, rows, cols };
        off += b.len();
        blocks.push(b);
    }
    Layout { blocks, dim: off }
}

impl Layout {
    pub fn block(&self, name: &str) -> &Block {
        self.blocks.iter().find(|b| b.name == name).unwrap_or_else(|| panic!("no block {name}"))
    }
}

/// Dense scratch + sparse output.
pub struct Encoder {
    buf: Vec<f32>,
    off: Offsets,
}

#[derive(Clone, Copy)]
struct B {
    off: usize,
    cols: usize,
}

struct Offsets {
    scalars: B,
    boss: B,
    tag_small: B,
    tag_big: B,
    tags_held: B,
    vouchers_owned: B,
    shop_vouchers: B,
    hand_levels: B,
    joker_targets: B,
    deck: B,
    pack_hand: B,
    held_jokers: B,
    held_consumables: B,
    shop_jokers: B,
    shop_consumables: B,
    shop_cards: B,
    shop_packs: B,
    pack_jokers: B,
    pack_consumables: B,
    pack_cards: B,
}

fn sc(name: &str) -> usize {
    SCALARS.iter().position(|x| *x == name).unwrap_or_else(|| panic!("no scalar {name}"))
}

impl Default for Encoder {
    fn default() -> Self {
        Self::new()
    }
}

impl Encoder {
    pub fn new() -> Self {
        let l = layout();
        let b = |n: &str| {
            let x = l.block(n);
            B { off: x.offset, cols: x.cols.len() }
        };
        Encoder {
            buf: vec![0.0; l.dim],
            off: Offsets {
                scalars: b("scalars"),
                boss: b("boss"),
                tag_small: b("tag_small"),
                tag_big: b("tag_big"),
                tags_held: b("tags_held"),
                vouchers_owned: b("vouchers_owned"),
                shop_vouchers: b("shop_vouchers"),
                hand_levels: b("hand_levels"),
                joker_targets: b("joker_targets"),
                deck: b("deck"),
                pack_hand: b("pack_hand"),
                held_jokers: b("held_jokers"),
                held_consumables: b("held_consumables"),
                shop_jokers: b("shop_jokers"),
                shop_consumables: b("shop_consumables"),
                shop_cards: b("shop_cards"),
                shop_packs: b("shop_packs"),
                pack_jokers: b("pack_jokers"),
                pack_consumables: b("pack_consumables"),
                pack_cards: b("pack_cards"),
            },
        }
    }

    fn set(&mut self, b: B, row: usize, col: usize, v: f32) {
        self.buf[b.off + row * b.cols + col] = v;
    }
    fn add(&mut self, b: B, row: usize, col: usize, v: f32) {
        self.buf[b.off + row * b.cols + col] += v;
    }
    fn max(&mut self, b: B, row: usize, col: usize, v: f32) {
        let x = &mut self.buf[b.off + row * b.cols + col];
        if v > *x {
            *x = v;
        }
    }
    fn scalar(&mut self, name: &str, v: f32) {
        let b = self.off.scalars;
        self.set(b, sc(name), 0, v);
    }

    /// Encode `g`; returns (indices, values) of the nonzero entries, indices ascending.
    pub fn encode(&mut self, g: &GameState) -> (Vec<u16>, Vec<f32>) {
        self.fill(g);
        let mut idx = Vec::with_capacity(256);
        let mut val = Vec::with_capacity(256);
        for (i, v) in self.buf.iter_mut().enumerate() {
            if *v != 0.0 {
                idx.push(i as u16);
                val.push(*v);
                *v = 0.0;
            }
        }
        (idx, val)
    }

    /// Encode `g` densely (length `layout().dim`).
    pub fn encode_dense(&mut self, g: &GameState) -> Vec<f32> {
        self.fill(g);
        let out = self.buf.clone();
        self.buf.iter_mut().for_each(|v| *v = 0.0);
        out
    }

    fn fill(&mut self, g: &GameState) {
        let o = &self.off;
        let (scalars, boss, tag_small, tag_big, tags_held) = (o.scalars, o.boss, o.tag_small, o.tag_big, o.tags_held);
        let (vouchers_owned, shop_vouchers, hand_levels, joker_targets, deck) = (o.vouchers_owned, o.shop_vouchers, o.hand_levels, o.joker_targets, o.deck);
        let (pack_hand, held_jokers, held_consumables, shop_jokers, shop_consumables) = (o.pack_hand, o.held_jokers, o.held_consumables, o.shop_jokers, o.shop_consumables);
        let (shop_cards, shop_packs, pack_jokers, pack_consumables, pack_cards) = (o.shop_cards, o.shop_packs, o.pack_jokers, o.pack_consumables, o.pack_cards);
        let _ = scalars;
        let ids = ids();
        let mut unknown = 0;

        // ---- run scalars
        let ante = g.ante;
        self.scalar("ante_div8", ante as f32 / 8.0);
        let a = ante.clamp(1, 8);
        self.scalar(["ante_1", "ante_2", "ante_3", "ante_4", "ante_5", "ante_6", "ante_7", "ante_8plus"][(a - 1) as usize], 1.0);
        let bi = g.blind_index.clamp(0, 2) as usize;
        self.scalar(["blind_small", "blind_big", "blind_boss"][bi], 1.0);
        let ph = match g.phase {
            Phase::BlindSelect => "ph_blind_select",
            Phase::RoundEval => "ph_round_eval",
            Phase::Shop => "ph_shop",
            Phase::Pack => "ph_pack",
            _ => "ph_playing",
        };
        self.scalar(ph, 1.0);
        if let (Phase::Pack, Some(p)) = (g.phase, g.pack.as_ref()) {
            self.scalar(["pack_arcana", "pack_celestial", "pack_standard", "pack_buffoon", "pack_spectral"][pack_kind_index(p.kind)], 1.0);
            self.scalar(["pack_normal", "pack_jumbo", "pack_mega"][pack_size_index(p.size)], 1.0);
            self.scalar("pack_picks_left", g.pack_picks_left as f32);
            self.scalar("pack_options_left", g.pack_options.len() as f32 / 5.0);
            if g.shop.is_none() {
                self.scalar("pack_from_tag", 1.0);
            }
        }
        let money = g.money;
        self.scalar("money_slog", slog(money as f64));
        self.scalar("money_div25", (money as f32 / 25.0).clamp(-2.0, 8.0));
        let cap = g.interest_cap();
        self.scalar("interest_now_div5", ((money.max(0) / 5).min(cap / 5)) as f32 / 5.0);
        self.scalar("interest_cap_div5", (cap / 5) as f32 / 5.0);
        self.scalar("bankrupt_at_div20", g.bankrupt_at() as f32 / 20.0);
        let (js, jh) = (g.joker_slots(), g.jokers.len() as i32);
        self.scalar("joker_slots_div5", js as f32 / 5.0);
        self.scalar("jokers_held_div5", jh as f32 / 5.0);
        self.scalar("joker_free_div5", (js - jh) as f32 / 5.0);
        self.scalar("consumable_slots_div2", g.consumable_slots() as f32 / 2.0);
        self.scalar("consumables_held_div2", g.consumables.len() as f32 / 2.0);
        self.scalar("hand_size_div8", g.hand_size() as f32 / 8.0);
        let (hpr, dpr) = g.round_allowance(false);
        self.scalar("hands_per_round_div4", hpr as f32 / 4.0);
        self.scalar("discards_per_round_div3", dpr as f32 / 3.0);
        let discount: i32 = g.vouchers.iter().map(|v| v.reroll_discount).sum();
        let reroll_cost = match &g.shop {
            Some(shop) => shop.reroll_cost(discount),
            None => {
                if g.free_rerolls_carried > 0 {
                    0
                } else {
                    g.reroll_price_carried
                }
            }
        };
        self.scalar("reroll_cost_div5", reroll_cost as f32 / 5.0);
        if let Some(shop) = &g.shop {
            self.scalar("shop_rerolls_div5", shop.rerolls as f32 / 5.0);
            self.scalar("free_rerolls", shop.free_rerolls as f32);
        }
        if g.shop_free {
            self.scalar("shop_coupon", 1.0);
        }
        if g.can_reroll_boss() {
            self.scalar("can_reroll_boss", 1.0);
        }
        if g.boss_rerolled {
            self.scalar("boss_rerolled", 1.0);
        }
        self.scalar("deck_size_div52", g.full_deck.len() as f32 / 52.0);
        self.scalar("round_div24", g.round_number as f32 / 24.0);
        self.scalar("blinds_skipped_div5", g.blinds_skipped as f32 / 5.0);
        self.scalar("hands_played_slog", slog(g.hands_played as f64));

        // ---- boss and tags
        match ids.blind(&g.ante_boss) {
            Some(b) => self.set(boss, b, 0, 1.0),
            None => unknown += (!g.ante_boss.is_empty()) as i32,
        }
        // the skip tags still on offer this ante (the small one only while the small blind is next)
        for (i, key) in g.ante_tag_keys.iter().enumerate().take(2) {
            if (i as i32) < g.blind_index {
                continue;
            }
            if let Some(t) = ids.tag(key) {
                self.set(if i == 0 { tag_small } else { tag_big }, t, 0, 1.0);
            } else if !key.is_empty() {
                unknown += 1;
            }
        }
        for t in &g.tags {
            match ids.tag(tag_key(*t)) {
                Some(t) => self.add(tags_held, t, 0, 1.0),
                None => unknown += 1,
            }
        }

        // ---- vouchers
        for v in &g.vouchers {
            match ids.voucher(v.key) {
                Some(i) => self.set(vouchers_owned, i, 0, 1.0),
                None => unknown += 1,
            }
        }
        if let Some(shop) = &g.shop {
            for v in shop.vouchers_on_offer() {
                if let Some(i) = ids.voucher(v.key) {
                    self.set(shop_vouchers, i, 0, 1.0);
                    if g.affords(g.price(v.cost)) {
                        self.set(shop_vouchers, i, 1, 1.0);
                    }
                } else {
                    unknown += 1;
                }
            }
        }

        // ---- hand levels
        for (i, h) in HandType::ALL.iter().enumerate() {
            let lv = g.hand_levels.level(*h);
            self.set(hand_levels, i, 0, (lv - 1) as f32 / 10.0);
            self.set(hand_levels, i, 1, slog(lv as f64));
            self.set(hand_levels, i, 2, slog(g.hand_levels.played(*h) as f64));
        }

        // ---- the round targets jokers name (shown on the joker)
        if let Some(r) = g.idol_rank {
            self.set(joker_targets, rank_index(r), 0, 1.0);
        }
        if let Some(su) = g.idol_suit {
            self.set(joker_targets, 13 + suit_index(su), 0, 1.0);
        }
        if let Some(su) = g.ancient_suit {
            self.set(joker_targets, 17 + suit_index(su), 0, 1.0);
        }
        if let Some(r) = g.mail_rank {
            self.set(joker_targets, 21 + rank_index(r), 0, 1.0);
        }
        if let Some(su) = g.castle_suit {
            self.set(joker_targets, 34 + suit_index(su), 0, 1.0);
        }

        // ---- deck composition (order-free counts over every card owned)
        self.composition(deck, &g.full_deck, 4.0, 52.0);
        // the hand an Arcana / Spectral pack deals is on screen
        if g.phase == Phase::Pack && !g.hand.is_empty() {
            self.composition(pack_hand, &g.hand, 1.0, 8.0);
        }

        // ---- held jokers and consumables
        for j in &g.jokers {
            let name = j.borrow().spec.name;
            match ids.joker(name) {
                Some(i) => self.held_joker(g, held_jokers, i, j),
                None => unknown += 1,
            }
        }
        for c in &g.consumables {
            let c = c.borrow();
            match ids.consumable(c.spec.name) {
                Some(i) => {
                    self.add(held_consumables, i, 0, 1.0);
                    if c.edition == Edition::Negative {
                        self.add(held_consumables, i, 1, 1.0);
                    }
                }
                None => unknown += 1,
            }
        }

        // ---- shop
        if let Some(shop) = &g.shop {
            for (slot_i, slot) in shop.slots.iter().enumerate() {
                let price = g.slot_price(slot);
                let afford = g.affords(price);
                if let Some(j) = &slot.joker {
                    let Some(i) = ids.joker(j.borrow().spec.name) else {
                        unknown += 1;
                        continue;
                    };
                    self.add(shop_jokers, i, 0, 1.0);
                    self.joker_flags(shop_jokers, i, j);
                    self.set(shop_jokers, i, 8, slog(price as f64));
                    if afford && g.room_for_joker(j) {
                        self.set(shop_jokers, i, 9, 1.0);
                    }
                } else if let Some(spec) = slot.consumable {
                    let Some(i) = ids.consumable(spec.name) else {
                        unknown += 1;
                        continue;
                    };
                    self.add(shop_consumables, i, 0, 1.0);
                    self.set(shop_consumables, i, 1, slog(price as f64));
                    if afford && (g.consumables.len() as i32) < g.consumable_slots() {
                        self.set(shop_consumables, i, 2, 1.0);
                    }
                } else if let Some(card) = &slot.card {
                    if slot_i >= SHOP_CARD_SLOTS {
                        unknown += 1;
                        continue;
                    }
                    self.set(shop_cards, slot_i, 0, 1.0);
                    if afford {
                        self.set(shop_cards, slot_i, 1, 1.0);
                    }
                    self.set(shop_cards, slot_i, 2, slog(price as f64));
                    self.card_slot(shop_cards, slot_i, 3, card);
                }
            }
            for p in &shop.packs {
                let i = pack_id(p);
                let price = g.pack_price(p);
                self.add(shop_packs, i, 0, 1.0);
                // the cheapest copy's price
                let c = slog(price as f64);
                let cur = self.buf[shop_packs.off + i * shop_packs.cols + 1];
                if self.buf[shop_packs.off + i * shop_packs.cols] == 1.0 || c < cur {
                    self.set(shop_packs, i, 1, c);
                }
                if g.affords(price) {
                    self.set(shop_packs, i, 2, 1.0);
                }
            }
        }

        // ---- open pack
        if g.phase == Phase::Pack {
            for (slot_i, opt) in g.pack_options.iter().enumerate() {
                match opt {
                    PackChoice::Joker(j) => {
                        let Some(i) = ids.joker(j.borrow().spec.name) else {
                            unknown += 1;
                            continue;
                        };
                        self.add(pack_jokers, i, 0, 1.0);
                        self.joker_flags(pack_jokers, i, j);
                        if g.room_for_joker(j) {
                            self.set(pack_jokers, i, 8, 1.0);
                        }
                    }
                    PackChoice::Consumable(spec) => {
                        let Some(i) = ids.consumable(spec.name) else {
                            unknown += 1;
                            continue;
                        };
                        self.add(pack_consumables, i, 0, 1.0);
                        if crate::vocab::pack_pickable(g, slot_i) {
                            self.set(pack_consumables, i, 1, 1.0);
                        }
                    }
                    PackChoice::Card(card) => {
                        if slot_i >= PACK_SLOTS {
                            unknown += 1;
                            continue;
                        }
                        self.set(pack_cards, slot_i, 0, 1.0);
                        self.card_slot(pack_cards, slot_i, 1, card);
                    }
                }
            }
        }
        self.scalar("unknown_items", unknown as f32);
    }

    /// edition (cols 1-4) and stickers (cols 5-7), summed over copies
    fn joker_flags(&mut self, b: B, i: usize, j: &JokerRef) {
        let j = j.borrow();
        let e = edition_index(j.edition);
        if e > 0 {
            self.add(b, i, e, 1.0);
        }
        if j.eternal {
            self.add(b, i, 5, 1.0);
        }
        if j.perishable {
            self.add(b, i, 6, 1.0);
        }
        if j.rental {
            self.add(b, i, 7, 1.0);
        }
    }

    fn held_joker(&mut self, g: &GameState, b: B, i: usize, j: &JokerRef) {
        self.add(b, i, 0, 1.0);
        self.joker_flags(b, i, j);
        let sell = g.sell_value(j);
        let jj = j.borrow();
        if jj.debuffed {
            self.add(b, i, 8, 1.0);
        }
        // copies: the larger value (order-free)
        self.max(b, i, 9, slog(sell as f64));
        self.max(b, i, 10, slog(jj.counter));
        self.max(b, i, 11, slog(jj.secondary));
        if jj.perishable {
            self.max(b, i, 12, jj.perish_tally as f32 / 5.0);
        }
    }

    fn card_slot(&mut self, b: B, slot: usize, base: usize, c: &CardRef) {
        self.set(b, slot, base + rank_index(cards::rank_of(c)), 1.0);
        self.set(b, slot, base + 13 + suit_index(cards::suit_of(c)), 1.0);
        self.set(b, slot, base + 17 + enhancement_index(cards::enhancement_of(c)), 1.0);
        self.set(b, slot, base + 26 + edition_index(cards::edition_of(c)), 1.0);
        self.set(b, slot, base + 31 + seal_index(cards::seal_of(c)), 1.0);
        debug_assert_eq!(base + 36, base + CARD_COLS_N);
    }

    fn composition(&mut self, b: B, cs: &[CardRef], per_card: f32, size_div: f32) {
        let mut extra = 0i64;
        let mut extra_n = 0;
        for c in cs {
            let r = rank_index(cards::rank_of(c));
            let su = suit_index(cards::suit_of(c));
            self.add(b, r * 4 + su, 0, 1.0 / per_card);
            self.add(b, 52 + enhancement_index(cards::enhancement_of(c)), 0, 1.0 / 10.0);
            self.add(b, 61 + edition_index(cards::edition_of(c)), 0, 1.0 / 5.0);
            self.add(b, 66 + seal_index(cards::seal_of(c)), 0, 1.0 / 5.0);
            let x = cards::extra_chips_of(c) as i64;
            extra += x;
            extra_n += (x != 0) as i32;
        }
        let n = cs.len().max(1) as f32;
        self.set(b, 71, 0, cs.len() as f32 / size_div);
        self.set(b, 72, 0, extra as f32 / n / 10.0);
        self.set(b, 73, 0, extra_n as f32 / n);
    }
}

/// The layout as JSON (block name, offset, row names, column names).
pub fn schema_blocks_json() -> String {
    let l = layout();
    let q = |x: &str| format!("\"{}\"", x.replace('\\', "\\\\").replace('"', "\\\""));
    let blocks: Vec<String> = l
        .blocks
        .iter()
        .map(|b| {
            format!(
                "{{\"name\":{},\"offset\":{},\"rows\":[{}],\"cols\":[{}]}}",
                q(b.name),
                b.offset,
                b.rows.iter().map(|r| q(r)).collect::<Vec<_>>().join(","),
                b.cols.iter().map(|c| q(c)).collect::<Vec<_>>().join(",")
            )
        })
        .collect();
    format!("[{}]", blocks.join(","))
}
