//! Gamestate JSON from a rumbot `GameState`: `sim_backend.serialize` / `serialize_card` / `_area` /
//! `_hands` / `_blinds` (BalatroBot's utils/gamestate.lua shape).
//!
//! Text fields (phase 2, `sim_text.py`): every `value.effect` (cards, jokers, consumables,
//! vouchers, packs), `blinds.*.effect` (`sim_text.blind_effect`) and `blinds.*.tag_effect`
//! (`sim_text.tag_effect`). They are emitted as `""` (`PLACEHOLDER`).

use super::RumbotGame;
use rumbot_sim::blinds::{make_blind, BlindKind};
use rumbot_sim::cards::{CardRef, Edition, Enhancement, Seal};
use rumbot_sim::consumable_data::consumable_row;
use rumbot_sim::consumables::{ConsumableKind, ConsumableRef};
use rumbot_sim::game::{GameState, PackChoice, Phase};
use rumbot_sim::hands::HandType;
use rumbot_sim::joker_data::joker_row;
use rumbot_sim::jokers::JokerRef;
use serde_json::{json, Map, Value};
use std::sync::OnceLock;

/// Where the localized card text goes in phase 2.
pub const PLACEHOLDER: &str = "";

fn tables() -> &'static Value {
    static T: OnceLock<Value> = OnceLock::new();
    T.get_or_init(|| serde_json::from_str(include_str!("tables.json")).expect("tables.json"))
}

/// BalatroBot (Lua) serialises an empty table as `[]`.
fn lua(m: Map<String, Value>) -> Value {
    if m.is_empty() {
        Value::Array(Vec::new())
    } else {
        Value::Object(m)
    }
}

fn edition_str(e: Edition) -> Option<&'static str> {
    match e {
        Edition::None => None,
        Edition::Foil => Some("FOIL"),
        Edition::Holographic => Some("HOLO"),
        Edition::Polychrome => Some("POLYCHROME"),
        Edition::Negative => Some("NEGATIVE"),
    }
}

fn enh_key(e: Enhancement) -> (&'static str, &'static str) {
    // (center key, modifier.enhancement)
    match e {
        Enhancement::None => ("c_base", ""),
        Enhancement::Bonus => ("m_bonus", "BONUS"),
        Enhancement::Mult => ("m_mult", "MULT"),
        Enhancement::Wild => ("m_wild", "WILD"),
        Enhancement::Glass => ("m_glass", "GLASS"),
        Enhancement::Steel => ("m_steel", "STEEL"),
        Enhancement::Stone => ("m_stone", "STONE"),
        Enhancement::Gold => ("m_gold", "GOLD"),
        Enhancement::Lucky => ("m_lucky", "LUCKY"),
    }
}

fn seal_str(s: Seal) -> Option<&'static str> {
    match s {
        Seal::None => None,
        Seal::Gold => Some("GOLD"),
        Seal::Red => Some("RED"),
        Seal::Blue => Some("BLUE"),
        Seal::Purple => Some("PURPLE"),
    }
}

pub fn center_name(key: &str) -> String {
    tables()["centers"][key]["name"].as_str().unwrap_or_default().to_string()
}

fn center_effect(key: &str) -> Option<String> {
    tables()["centers"][key]["effect"].as_str().map(String::from)
}

#[allow(clippy::too_many_arguments)]
fn card_json(
    id: u64,
    key: &str,
    set: &str,
    label: &str,
    value: Map<String, Value>,
    modifier: Map<String, Value>,
    debuff: bool,
    hidden: bool,
    sell: i64,
    buy: i64,
) -> Value {
    let mut state = Map::new();
    if debuff {
        state.insert("debuff".into(), json!(true));
    }
    if hidden {
        state.insert("hidden".into(), json!(true));
    }
    json!({
        "id": id,
        "key": key,
        "set": set,
        "label": match label { "Riff-Raff" => "Riff-raff", "Séance" => "Seance", l => l }, // names as Jackdaw/Balatro localisation prints them
        "value": value,
        "modifier": lua(modifier),
        "state": lua(state),
        "cost": {"sell": sell, "buy": buy},
    })
}

// sim_backend.serialize_card, playing cards
fn playing_card(c: &CardRef, area: &str, live: bool) -> Value {
    let c = c.borrow();
    let (ckey, enh) = enh_key(c.enhancement);
    let set = if c.enhancement != Enhancement::None { "ENHANCED" } else { "DEFAULT" };
    let label = tables()["enh_label"][ckey].as_str().unwrap_or("Base Card").to_string();
    let mut value = Map::new();
    value.insert("suit".into(), json!(c.suit.as_str()));
    value.insert("rank".into(), json!(c.rank.short()));
    value.insert("effect".into(), json!(PLACEHOLDER));
    let mut m = Map::new();
    if let Some(s) = seal_str(c.seal) {
        m.insert("seal".into(), json!(s));
    }
    if let Some(e) = edition_str(c.edition) {
        m.insert("edition".into(), json!(e));
    }
    if !enh.is_empty() {
        m.insert("enhancement".into(), json!(enh));
    }
    let key = format!("{}_{}", c.suit.as_str(), c.rank.short());
    card_json(c.uid, &key, set, &label, value, m, c.debuffed && live, c.face_down || area == "cards", 0, 0)
}

fn effect_value() -> Map<String, Value> {
    let mut value = Map::new();
    value.insert("effect".into(), json!(PLACEHOLDER));
    value
}

fn joker_json(g: &GameState, j: &JokerRef) -> Value {
    let jb = j.borrow();
    let name = jb.spec.name;
    let key = joker_row(name).map_or("", |r| r.key);
    let mut m = Map::new();
    if let Some(e) = edition_str(jb.edition) {
        m.insert("edition".into(), json!(e));
    }
    if let Some(eff) = center_effect(key) {
        if eff != "Base" {
            m.insert("enhancement".into(), json!(eff.replace(" Card", "").to_uppercase()));
        }
    }
    if jb.eternal {
        m.insert("eternal".into(), json!(true));
    }
    if jb.perishable && jb.perish_tally > 0 {
        m.insert("perishable".into(), json!(jb.perish_tally));
    }
    if jb.rental {
        m.insert("rental".into(), json!(true));
    }
    let buy = if jb.rental { 1 } else { g.card_cost(jb.spec.cost, jb.edition) };
    let (debuff, hidden, uid) = (jb.debuffed, jb.face_down, jb.uid);
    drop(jb);
    let sell = g.sell_value(j);
    card_json(uid, key, "JOKER", name, effect_value(), m, debuff, hidden, sell as i64, buy as i64)
}

fn consumable_set(k: ConsumableKind) -> &'static str {
    match k {
        ConsumableKind::Tarot => "TAROT",
        ConsumableKind::Planet => "PLANET",
        ConsumableKind::Spectral => "SPECTRAL",
    }
}

fn consumable_spec_json(
    spec: &rumbot_sim::consumables::ConsumableSpec,
    id: u64,
    edition: Edition,
    sell: i64,
    buy: i64,
) -> Value {
    let key = consumable_row(spec.name).map_or("", |r| r.key);
    let mut m = Map::new();
    if let Some(e) = edition_str(edition) {
        m.insert("edition".into(), json!(e));
    }
    if let Some(eff) = center_effect(key) {
        if eff != "Base" {
            m.insert("enhancement".into(), json!(eff.replace(" Card", "").to_uppercase()));
        }
    }
    card_json(id, key, consumable_set(spec.kind), spec.name, effect_value(), m, false, false, sell, buy)
}

fn consumable_json(g: &GameState, c: &ConsumableRef) -> Value {
    let cb = c.borrow();
    let astronomer = g.active_jokers().iter().any(|j| j.borrow().spec.free_planets);
    let buy = if cb.spec.kind == ConsumableKind::Planet && astronomer { 0 } else { g.card_cost(cb.spec.cost, cb.edition) };
    let (spec, uid, ed) = (cb.spec, cb.uid, cb.edition);
    drop(cb);
    consumable_spec_json(spec, uid, ed, g.consumable_sell_value(c) as i64, buy as i64)
}

fn area(cards: Vec<Value>, limit: i64, highlighted_limit: i64) -> Value {
    json!({"count": cards.len(), "limit": limit, "cards": cards, "highlighted_limit": highlighted_limit})
}

// sim_backend._hands: Jackdaw's HandType order and `order` numbers
const HANDS: [(&str, HandType); 12] = [
    ("Flush Five", HandType::FlushFive),
    ("Flush House", HandType::FlushHouse),
    ("Five of a Kind", HandType::FiveOfAKind),
    ("Straight Flush", HandType::StraightFlush),
    ("Four of a Kind", HandType::FourOfAKind),
    ("Full House", HandType::FullHouse),
    ("Flush", HandType::Flush),
    ("Straight", HandType::Straight),
    ("Three of a Kind", HandType::ThreeOfAKind),
    ("Two Pair", HandType::TwoPair),
    ("Pair", HandType::Pair),
    ("High Card", HandType::HighCard),
];

fn hands(eng: &RumbotGame, g: &GameState) -> Value {
    let mut out = Map::new();
    for (i, (name, ht)) in HANDS.iter().enumerate() {
        let (chips, mult) = g.hand_levels.values(*ht);
        out.insert(
            (*name).into(),
            json!({
                "order": i + 1,
                "level": g.hand_levels.level(*ht),
                "chips": chips,
                "mult": mult,
                "played": g.hand_levels.played(*ht),
                "played_this_round": eng.played_this_round.get(ht).copied().unwrap_or(0),
                "example": tables()["hand_examples"][*name].clone(),
            }),
        );
    }
    Value::Object(out)
}

fn tag_name(key: Option<&String>) -> String {
    key.and_then(|k| tables()["tags"][k.as_str()].as_str()).unwrap_or("").to_string()
}

// sim_backend._blinds
fn blinds(g: &GameState) -> Value {
    let cur = g.blind_index.clamp(0, 2) as usize;
    let preview = if g.phase == Phase::RoundEval && g.beaten_was_boss {
        let mut c = g.deep_clone();
        c._roll_ante_tags();
        c._roll_boss();
        Some(c)
    } else {
        None
    };
    let tags: Vec<String> = preview.as_ref().map_or_else(|| g.ante_tag_keys.clone(), |c| c.ante_tag_keys.clone());
    let boss = match &preview { Some(c) => c._pick_boss(), None => g._pick_boss().or_else(|| g.blind.as_ref().and_then(|b| b.boss)) };
    let ante_scaling = g.deck_config().ante_scaling;
    // Jackdaw rolls the next ante's tags and boss the moment the boss falls; rumbot rolls them at cash_out.
    // The streams are keyed, so rolling on a clone shows what the real roll will give.
    let mut out = Map::new();
    for (i, (typ, kind)) in [("SMALL", BlindKind::Small), ("BIG", BlindKind::Big), ("BOSS", BlindKind::Boss)]
        .iter()
        .enumerate()
    {
        let b_boss = if *kind == BlindKind::Boss { boss } else { None };
        let blind = make_blind(*kind, g.ante, b_boss, ante_scaling, g.blind_scaling(), false);
        let name = match (kind, b_boss) {
            (BlindKind::Boss, None) => "Boss Blind".to_string(),
            _ => blind.name(),
        };
        let skipped = g.skipped_this_ante.contains(&(i as i32));
        let past = if skipped { "SKIPPED" } else { "DEFEATED" };
        let status = match g.phase {
            Phase::Playing | Phase::GameOver => {
                if i < cur { past } else if i == cur { "CURRENT" } else { "UPCOMING" }
            }
            Phase::BlindSelect => {
                if i < cur { past } else if i == cur { "SELECT" } else { "UPCOMING" }
            }
            // the blind after the one just beaten stays UPCOMING until the select screen, except a
            // new ante's small blind (Jackdaw: blind_states are rolled when the boss falls)
            _ => {
                if i < cur { past } else if i == cur && cur == 0 { "SELECT" } else { "UPCOMING" }
            }
        };
        let tag_key = if i < 2 { tags.get(i) } else { None };
        out.insert(
            typ.to_lowercase(),
            json!({
                "type": typ,
                "status": status,
                "name": name,
                "effect": super::view::PLACEHOLDER,
                "score": blind.target,
                "tag_name": tag_name(tag_key),
                "tag_effect": super::view::PLACEHOLDER,
            }),
        );
    }
    Value::Object(out)
}

fn state_name(g: &GameState) -> &'static str {
    match g.phase {
        Phase::BlindSelect => "BLIND_SELECT",
        Phase::Playing => "SELECTING_HAND",
        Phase::RoundEval | Phase::Won => "ROUND_EVAL",
        Phase::Shop => "SHOP",
        Phase::Pack => "SMODS_BOOSTER_OPENED",
        Phase::GameOver => "GAME_OVER",
    }
}

fn stake_name(s: i32) -> &'static str {
    match s {
        2 => "RED",
        3 => "GREEN",
        4 => "BLACK",
        5 => "BLUE",
        6 => "PURPLE",
        7 => "ORANGE",
        8 => "GOLD",
        _ => "WHITE",
    }
}

fn price_card(g: &GameState, base: i32, ed: Edition, extra_sell: i64, buy: i64) -> (i64, i64) {
    let _ = (g, base, ed);
    ((buy / 2).max(1) + extra_sell, buy)
}

fn shop_slot_json(g: &GameState, slot: &rumbot_sim::shop::ShopSlot) -> Value {
    let buy = g.slot_price(slot) as i64;
    if let Some(j) = &slot.joker {
        let mut v = joker_json(g, j);
        let (jb_sell, jb_buy) = {
            let jb = j.borrow();
            price_card(g, jb.spec.cost, jb.edition, jb.extra_sell_value as i64, buy)
        };
        v["cost"] = json!({"sell": jb_sell, "buy": jb_buy});
        return v;
    }
    if let Some(spec) = slot.consumable {
        let (s, b) = price_card(g, spec.cost, Edition::None, 0, buy);
        return consumable_spec_json(spec, slot.sort_id, Edition::None, s, b);
    }
    let card = slot.card.as_ref().expect("a shop slot holds a joker, a consumable or a card");
    let mut v = playing_card(card, "shop", true);
    let (s, b) = price_card(g, 0, Edition::None, 0, buy);
    v["cost"] = json!({"sell": s, "buy": b});
    v
}

fn voucher_json(g: &GameState, v: &rumbot_sim::shop::Voucher) -> Value {
    let buy = g.price(v.cost) as i64;
    // no per-voucher card ids in rumbot
    card_json(0, v.key, "VOUCHER", v.name, effect_value(), Map::new(), false, false, (buy / 2).max(1), buy)
}

fn pack_json(g: &GameState, p: &rumbot_sim::shop::PackSpec, first_shop: bool) -> Value {
    let buy = g.pack_price(p) as i64;
    card_json(0, if first_shop && p.key == "p_buffoon_normal_2" { "p_buffoon_normal_1" } else { p.key }, "BOOSTER", &p.name(), effect_value(), Map::new(), false, false, (buy / 2).max(1), buy)
}

fn pack_choice_json(g: &GameState, c: &PackChoice) -> Value {
    match c {
        PackChoice::Joker(j) => joker_json(g, j),
        PackChoice::Card(c) => playing_card(c, "pack", true),
        // no per-card id in rumbot for a pack's consumables: id 0
        PackChoice::Consumable(spec) => {
            let buy = g.card_cost(spec.cost, Edition::None) as i64;
            consumable_spec_json(spec, 0, Edition::None, (buy / 2).max(1), buy)
        }
    }
}

// sim_backend.serialize
pub fn serialize(eng: &mut RumbotGame) -> Value {
    let g = eng.g.as_ref().expect("serialize needs a run");
    let phase = g.phase;
    let live = g.blind.is_some(); // Jackdaw clears card debuffs when the blind ends; rumbot keeps them until the next deal
    let (lr_h, lr_d, lr_u) = if phase == Phase::Playing { (g.hands_left, g.discards_left, g.discards_used) } else if eng.round_started { eng.last_round } else { (0, 0, 0) };
    // Jackdaw always deals p_buffoon_normal_1 in the first shop (shop.py:180); rumbot rolls math.random(1, 2) like the game
    if in_shop_now(phase, eng.pack_from_shop) && eng.first_shop == 0 { eng.first_shop = 1; }
    if (phase == Phase::BlindSelect || phase == Phase::Playing) && eng.first_shop == 1 { eng.first_shop = 2; }
    let in_shop = phase == Phase::Shop || (phase == Phase::Pack && eng.pack_from_shop);
    let reroll_cost = match &g.shop {
        Some(s) => s.reroll_cost(g.vouchers.iter().map(|v| v.reroll_discount).sum()) as i64,
        None if phase == Phase::Playing && g.jokers.iter().any(|j| j.borrow().spec.name == "Chaos the Clown") => 0, // free_rerolls > 0 at round start -> calculate_reroll_cost gives 0
        None if phase == Phase::Playing => (5 - g.vouchers.iter().map(|v| v.reroll_discount).sum::<i32>() as i64).max(0), // round start resets the increase
        None => eng.last_reroll, // Python keeps current_round.reroll_cost after the shop closes
    };
    eng.last_reroll = reroll_cost;
    let mut used = Map::new();
    for v in &g.vouchers {
        used.insert(v.key.to_string(), json!(""));
    }
    let deck: Vec<Value> = g.draw_pile.iter().map(|c| playing_card(c, "cards", live)).collect();
    let mut out = Map::new();
    out.insert("state".into(), json!(state_name(g)));
    out.insert("round_num".into(), json!(g.round_number));
    out.insert("ante_num".into(), json!(g.ante));
    out.insert("money".into(), json!(g.money));
    out.insert("won".into(), json!(phase == Phase::Won));
    out.insert("deck".into(), json!(g.deck.split(' ').next().unwrap_or("").to_uppercase()));
    out.insert("stake".into(), json!(stake_name(g.stake)));
    out.insert("seed".into(), json!(g.seed));
    out.insert("used_vouchers".into(), lua(used));
    out.insert("hands".into(), hands(eng, g));
    out.insert(
        "round".into(),
        json!({
            "hands_left": lr_h,
            "hands_played": eng.round_hands_played,
            "discards_left": lr_d,
            "discards_used": lr_u,
            "reroll_cost": reroll_cost,
            "chips": if phase == Phase::Playing { g.chips_scored } else { eng.last_chips },
        }),
    );
    out.insert("blinds".into(), blinds(g));
    out.insert(
        "jokers".into(),
        area(g.jokers.iter().map(|j| joker_json(g, j)).collect(), g.joker_slots() as i64, 1),
    );
    out.insert(
        "consumables".into(),
        area(g.consumables.iter().map(|c| consumable_json(g, c)).collect(), g.consumable_slots() as i64, 1),
    );
    out.insert("cards".into(), area(deck, eng.deck_limit as i64, 5));
    out.insert(
        "hand".into(),
        area(g.hand.iter().map(|c| playing_card(c, "hand", live)).collect(), g.hand_size() as i64, 5),
    );
    if in_shop {
        let empty = rumbot_sim::shop::Shop::default();
        let shop = g.shop.as_ref().unwrap_or(&empty);
        let slots: Vec<Value> = shop.slots.iter().map(|s| shop_slot_json(g, s)).collect();
        let vouchers: Vec<Value> = shop.vouchers_on_offer().iter().map(|v| voucher_json(g, v)).collect();
        let packs: Vec<Value> = shop.packs.iter().enumerate().map(|(i, p)| pack_json(g, p, eng.first_shop == 1 && i == 0 && !eng.first_pack_gone)).collect();
        let a = area(slots, g._shop_slot_count() as i64, 1);
        let b = area(vouchers.clone(), vouchers.len().max(1) as i64, 1);
        let c = area(packs, 2, 1);
        eng.shop_frozen = Some([
            (a["count"].as_u64().unwrap() as usize, a["limit"].as_u64().unwrap() as usize),
            (b["count"].as_u64().unwrap() as usize, b["limit"].as_u64().unwrap() as usize),
            (c["count"].as_u64().unwrap() as usize, c["limit"].as_u64().unwrap() as usize),
        ]);
        out.insert("shop".into(), a);
        out.insert("vouchers".into(), b);
        out.insert("packs".into(), c);
    } else if let Some(fz) = &eng.shop_frozen {
        for (k, (count, limit)) in ["shop", "vouchers", "packs"].iter().zip(fz.iter()) {
            out.insert(
                (*k).into(),
                json!({"count": count, "limit": limit, "cards": [], "highlighted_limit": 1}),
            );
        }
    }
    if phase == Phase::Pack {
        let cards: Vec<Value> = g.pack_options.iter().map(|c| pack_choice_json(g, c)).collect();
        let limit = 2; // Jackdaw gs["pack_size"] is a constant 2 (run_init), never updated
        out.insert("pack".into(), area(cards, limit, 1));
    }
    Value::Object(out)
}

fn in_shop_now(phase: Phase, from_shop: bool) -> bool {
    phase == Phase::Shop || (phase == Phase::Pack && from_shop)
}
