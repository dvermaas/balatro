//! `src/sim_text.py` lines 349-376 and 612-846: the `value.effect` of playing cards, consumables,
//! vouchers and boosters, `blinds.*.effect` and `blinds.*.tag_effect`. Joker text is a later task.

use crate::simtext::{
    desc, dictionary, localize_desc, localize_other, misc, name_text, poker_hand, raw_lines, signed, signed_dollars,
};
use rumbot_sim::cards::{Card, Edition, Enhancement};
use rumbot_sim::game::GameState;
use rumbot_sim::hands::{level_gain, HandType};
use serde_json::{json, Value};
use std::sync::OnceLock;

fn tables() -> &'static Value {
    static T: OnceLock<Value> = OnceLock::new();
    T.get_or_init(|| serde_json::from_str(include_str!("tables.json")).expect("tables.json"))
}

/// `center(key)`
fn center(key: &str) -> &'static Value {
    &tables()["centers"][key]
}

/// `_cfg(card)`: the center's config
fn cfg(key: &str) -> &'static Value {
    &center(key)["config"]
}

fn cname(key: &str) -> &'static str {
    center(key)["name"].as_str().unwrap_or("")
}

/// `_prob`: SMODS.get_probability_vars
fn prob(g: &GameState, base: i64, odds: &Value) -> Vec<Value> {
    let n = base as f64 * g.probability_scale();
    vec![if n.fract() == 0.0 { json!(n as i64) } else { json!(n) }, odds.clone()]
}

fn join(lines: Vec<String>) -> String {
    lines.join(" ")
}

fn tarot_vars(g: &GameState, key: &str) -> (Vec<Value>, Option<Vec<String>>) {
    let c = cfg(key);
    let name = cname(key);
    let mut main_end = None;
    let lv: Vec<Value> = match name {
        "The Fool" => {
            let ltp = g.last_tarot_planet.as_str();
            main_end = Some(vec![String::new()]);
            if !ltp.is_empty() && !center(ltp).is_null() {
                vec![name_text(center(ltp)["set"].as_str().unwrap_or(""), ltp)]
            } else {
                vec![dictionary("k_none")]
            }
        }
        "The Magician" | "The Empress" | "The Hierophant" | "The Lovers" | "The Chariot" | "Justice" | "The Devil"
        | "The Tower" => vec![c["max_highlighted"].clone(), name_text("Enhanced", c["mod_conv"].as_str().unwrap_or(""))],
        "The High Priestess" => vec![c["planets"].clone()],
        "The Emperor" => vec![c["tarots"].clone()],
        "The Hermit" => vec![c["extra"].clone()],
        "The Wheel of Fortune" => prob(g, 1, &c["extra"]),
        "Strength" | "The Hanged Man" | "Death" => vec![c["max_highlighted"].clone()],
        "Temperance" => {
            let money: i64 = g.jokers.iter().map(|j| g.sell_value(j) as i64).sum();
            let extra = c["extra"].as_i64().unwrap_or(0);
            vec![c["extra"].clone(), json!(extra.min(money))]
        }
        "The Star" | "The Moon" | "The Sun" | "The World" => {
            vec![c["max_highlighted"].clone(), misc("suits_plural", c["suit_conv"].as_str().unwrap_or(""))]
        }
        _ => vec![],
    };
    (lv, main_end)
}

fn spectral_vars(g: &GameState, key: &str) -> (Vec<Value>, Option<Vec<String>>) {
    let c = cfg(key);
    let name = cname(key);
    let ex = &c["extra"];
    let mut lv = vec![];
    let mut main_end = None;
    match name {
        "Familiar" | "Grim" | "Incantation" | "Cryptid" => lv = vec![ex.clone()],
        "Immolate" => lv = vec![ex["destroy"].clone(), ex["dollars"].clone()],
        "Ankh" => {
            if g.jokers.iter().any(|j| j.borrow().edition == Edition::Negative) {
                main_end = Some(localize_other("remove_negative", &[]));
            }
        }
        _ => {}
    }
    if name == "Ectoplasm" {
        lv = vec![json!(if g.ecto_minus != 0 { g.ecto_minus } else { 1 })];
    }
    (lv, main_end)
}

fn planet_vars(g: &GameState, key: &str) -> Vec<Value> {
    let ht = cfg(key)["hand_type"].as_str().unwrap_or("");
    let h: HandType = super::view::HANDS.iter().find(|(n, _)| *n == ht).expect("planet hand").1;
    let (l_chips, l_mult) = level_gain(h);
    vec![json!(g.hand_levels.level(h)), poker_hand(ht), json!(l_mult), json!(l_chips)]
}

fn voucher_vars(key: &str) -> Vec<Value> {
    let c = cfg(key);
    match cname(key) {
        "Tarot Merchant" | "Tarot Tycoon" | "Planet Merchant" | "Planet Tycoon" => vec![c["extra_disp"].clone()],
        "Seed Money" | "Money Tree" => vec![json!(c["extra"].as_f64().unwrap_or(0.0) / 5.0)],
        "Overstock" | "Overstock Plus" | "Blank" | "Antimatter" => vec![],
        _ => vec![c["extra"].clone()],
    }
}

fn booster(key: &str) -> (String, Vec<Value>) {
    let c = cfg(key);
    // p_arcana_normal_2 -> p_arcana_normal
    let t = key.trim_end_matches(|ch: char| ch.is_ascii_digit());
    let k = if t.len() < key.len() && t.ends_with('_') { &t[..t.len() - 1] } else { key };
    (k.to_string(), vec![c["choose"].clone(), c["extra"].clone()])
}

/// `_playing_card_lines`
fn playing_card_lines(g: &GameState, card: &Card, k: &str) -> Vec<String> {
    let c = cfg(k);
    // ability.bonus: the enhancement's config bonus + Hiker's perma_bonus
    let enh_bonus = c["bonus"].as_i64().unwrap_or(0);
    let bonus = enh_bonus + card.extra_chips as i64;
    let nominal = if card.enhancement == Enhancement::Stone { 0 } else { card.rank.chips() as i64 };
    let chips_line = if nominal != 0 {
        localize_other(if nominal < 0 { "card_chips_minus" } else { "card_chips" }, &[json!(nominal)])
    } else {
        vec![]
    };
    let mut out = vec![];
    if k == "c_base" {
        out.extend(chips_line);
        if bonus != 0 {
            out.extend(localize_other("card_extra_chips", &[json!(signed(&json!(bonus)))]));
        }
    } else if k == "m_lucky" {
        out.extend(chips_line);
        let mut v = prob(g, 1, &json!(5));
        let (nm, dm) = (v.remove(0), v.remove(0));
        let mut v = prob(g, 1, &json!(15));
        let (nd, dd) = (v.remove(0), v.remove(0));
        out.extend(localize_desc("Enhanced", k, &[nm, c["mult"].clone(), dm, c["p_dollars"].clone(), dd, nd]));
        let rem = bonus - enh_bonus;
        if rem != 0 {
            out.extend(localize_other("card_extra_chips", &[json!(signed(&json!(rem)))]));
        }
    } else {
        if k != "m_stone" {
            out.extend(chips_line);
        }
        let lv: Vec<Value> = match k {
            "m_mult" => vec![json!(signed(&c["mult"]))],
            "m_glass" => {
                let mut v = vec![c["Xmult"].clone()];
                v.extend(prob(g, 1, &c["extra"]));
                v
            }
            "m_steel" => vec![c["h_x_mult"].clone()],
            "m_stone" => vec![json!(if bonus != 0 { signed(&json!(bonus)) } else { signed(&c["bonus"]) })],
            "m_gold" => vec![json!(signed_dollars(&c["h_dollars"]))],
            _ => vec![],
        };
        out.extend(localize_desc("Enhanced", k, &lv));
        if k != "m_stone" && bonus != 0 {
            out.extend(localize_other("card_extra_chips", &[json!(signed(&json!(bonus)))]));
        }
    }
    out
}

/// card_effect for a playing card (`ckey`: its enhancement's center key)
pub fn playing_effect(g: &GameState, card: &Card, ckey: &str, debuff: bool) -> String {
    if debuff {
        return join(localize_other("debuffed_playing_card", &[]));
    }
    join(playing_card_lines(g, card, ckey))
}

/// card_effect for a consumable / voucher / booster, by center key
pub fn center_effect(g: &GameState, key: &str) -> String {
    let set = center(key)["set"].as_str().unwrap_or("");
    let mut end: Option<Vec<String>> = None;
    let mut lines = match set {
        "Tarot" => {
            let (lv, e) = tarot_vars(g, key);
            end = e;
            localize_desc("Tarot", key, &lv)
        }
        "Planet" => localize_desc("Planet", key, &planet_vars(g, key)),
        "Spectral" => {
            let (lv, e) = spectral_vars(g, key);
            end = e;
            localize_desc("Spectral", key, &lv)
        }
        "Voucher" => localize_desc("Voucher", key, &voucher_vars(key)),
        "Booster" => {
            let (k, lv) = booster(key);
            localize_other(&k, &lv)
        }
        _ => localize_desc(set, key, &[]),
    };
    lines.extend(end.unwrap_or_default());
    join(lines)
}

/// `blind_effect(key)`
pub fn blind_effect(key: &str) -> String {
    if key.is_empty() || key == "bl_small" || key == "bl_big" {
        return String::new();
    }
    match crate::simtext::loc()["descriptions"]["Blind"].get(key).and_then(|e| e.get("text")).and_then(|t| t.as_array()) {
        Some(t) => t.iter().filter_map(|s| s.as_str()).filter(|s| !s.is_empty()).collect::<Vec<_>>().join(" "),
        None => String::new(),
    }
}

pub fn blind_key(name: &str) -> &'static str {
    tables()["blinds"]
        .as_object()
        .and_then(|m| m.iter().find(|(_, v)| v.as_str() == Some(name)).map(|(k, _)| k.as_str()))
        .unwrap_or("")
}

/// `tag_effect(key, gs)`
pub fn tag_effect(g: &GameState, key: &str) -> String {
    if key.is_empty() {
        return String::new();
    }
    let t = &tables()["tag_config"][key];
    let c = &t["config"];
    let n = |k: &str| c[k].as_i64().unwrap_or(0);
    let lv: Vec<Value> = match t["name"].as_str().unwrap_or("") {
        "Investment Tag" => vec![json!(n("dollars"))],
        "Handy Tag" => vec![json!(n("dollars_per_hand")), json!(n("dollars_per_hand") * g.hands_played as i64)],
        "Garbage Tag" => vec![json!(n("dollars_per_discard")), json!(n("dollars_per_discard") * g.unused_discards as i64)],
        "Juggle Tag" => vec![json!(n("h_size"))],
        "Top-up Tag" => vec![json!(n("spawn_jokers"))],
        "Skip Tag" => vec![json!(n("skip_bonus")), json!(n("skip_bonus") * (g.blinds_skipped as i64 + 1))],
        "Orbital Tag" => vec![json!("Poker Hand"), json!(n("levels"))],
        "Economy Tag" => vec![json!(n("max"))],
        _ => vec![],
    };
    match desc("Tag", key) {
        Some(lines) if !lines.is_empty() => join(raw_lines(lines, &lv)),
        _ => String::new(),
    }
}
