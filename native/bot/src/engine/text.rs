//! `src/sim_text.py` lines 349-376 and 612-846: the `value.effect` of playing cards, consumables,
//! vouchers and boosters, `blinds.*.effect` and `blinds.*.tag_effect`. Joker text is a later task.

use crate::simtext::{variable, 
    desc, dictionary, localize_desc, localize_other, misc, name_text, poker_hand, raw_lines, signed, signed_dollars,
};
use rumbot_sim::cards::{Card, Edition, Enhancement};
use rumbot_sim::game::GameState;
use rumbot_sim::hands::{level_gain, HandType};
use rumbot_sim::jokers::JokerRef;
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

// ---------------------------------------------------------------------------
// Jokers (`sim_text._joker_vars`). Jackdaw's card.ability is rebuilt from the center config
// (set_ability) and overridden with the rumbot joker's counter / secondary where it grows.
// ---------------------------------------------------------------------------

fn num(f: f64) -> Value {
    if f.fract() == 0.0 && f.abs() < 1e15 {
        json!(f as i64)
    } else {
        json!(f)
    }
}

fn nv(v: &Value) -> f64 {
    v.as_f64().unwrap_or(0.0)
}

fn full_word(h: Option<HandType>) -> Value {
    h.map_or(Value::Null, |h| poker_hand(h.label()))
}

fn rank_word(r: rumbot_sim::cards::Rank) -> String {
    crate::hands::rank_name(r.short()).to_string()
}

fn suit_word(s: rumbot_sim::cards::Suit) -> String {
    crate::hands::suit_name(s.as_str()).to_string()
}

type JokerVars = (Vec<Value>, Option<Vec<String>>, Option<Vec<String>>);

fn joker_vars(g: &GameState, j: &JokerRef, in_jokers: bool) -> JokerVars {
    let jb = j.borrow();
    let name = jb.spec.name;
    let key = rumbot_sim::joker_data::joker_row(name).map_or("", |r| r.key);
    let cfgv = cfg(key);
    let cget = |k: &str| cfgv.get(k).cloned().unwrap_or(Value::Null);
    let name = match name { "Riff-Raff" => "Riff-raff", "Séance" => "Seance", n => n };
    let c = jb.counter;
    let mut mult = nv(&cget("mult"));
    let mut x_mult = cget("Xmult").as_f64().unwrap_or(1.0);
    let t_mult = cget("t_mult");
    let t_chips = cget("t_chips");
    let h_size = cget("h_size");
    let d_size = cget("d_size");
    let typ = cget("type");
    let x = cget("extra");
    let mut e: serde_json::Map<String, Value> = x.as_object().cloned().unwrap_or_default();
    let mut caino = Value::Null;
    let mut yorick_d = Value::Null;
    match name {
        "Green Joker" | "Ceremonial Dagger" | "Ride the Bus" | "Flash Card" | "Popcorn" | "Spare Trousers" | "Red Card" => mult = c,
        "Constellation" | "Glass Joker" | "Hit the Road" | "Madness" | "Vampire" | "Hologram" | "Obelisk" | "Lucky Cat"
        | "Campfire" | "Ramen" => x_mult = c,
        "Throwback" => x_mult = 1.0 + nv(&x) * g.blinds_skipped as f64,
        "Yorick" => {
            x_mult = c;
            yorick_d = num(jb.secondary);
        }
        "Caino" => caino = num(c),
        "Runner" | "Ice Cream" | "Wee Joker" | "Square Joker" | "Castle" => {
            e.insert("chips".into(), num(c));
        }
        "Rocket" => {
            e.insert("dollars".into(), num(c));
        }
        "Turtle Bean" => {
            e.insert("h_size".into(), num(c));
        }
        _ => {}
    }
    let ev = |k: &str| e.get(k).cloned().unwrap_or(Value::Null);
    let amult = num(mult);
    let axm = num(x_mult);
    let nplaying = (g.draw_pile.len() + g.hand.len() + g.discard_pile.len()) as i64;
    let tally = |f: &dyn Fn(&rumbot_sim::cards::Card) -> bool| {
        g.draw_pile.iter().chain(g.hand.iter()).chain(g.discard_pile.iter()).filter(|c| f(&c.borrow())).count() as i64
    };
    let sw = |s: Option<rumbot_sim::cards::Suit>, cat: &str| s.map_or(misc(cat, ""), |s| misc(cat, &suit_word(s)));
    let rw = |r: Option<rumbot_sim::cards::Rank>| r.map_or(misc("ranks", ""), |r| misc("ranks", &rank_word(r)));
    let mut ms: Option<Vec<String>> = None;
    let mut me: Option<Vec<String>> = None;
    let xx = &x;
    let lv: Vec<Value> = match name {
        "Joker" => vec![amult.clone()],
        "Jolly Joker" | "Zany Joker" | "Mad Joker" | "Crazy Joker" | "Droll Joker" => {
            vec![t_mult, poker_hand(typ.as_str().unwrap_or(""))]
        }
        "Sly Joker" | "Wily Joker" | "Clever Joker" | "Devious Joker" | "Crafty Joker" => {
            vec![t_chips, poker_hand(typ.as_str().unwrap_or(""))]
        }
        "Half Joker" => vec![ev("mult"), ev("size")],
        "Fortune Teller" => vec![xx.clone(), json!(g.tarots_used)],
        "Steel Joker" => {
            let t = tally(&|c| c.enhancement == Enhancement::Steel);
            vec![xx.clone(), num(1.0 + nv(xx) * t as f64)]
        }
        "Stone Joker" => {
            let t = tally(&|c| c.enhancement == Enhancement::Stone);
            vec![xx.clone(), num(nv(xx) * t as f64)]
        }
        "Space Joker" | "8 Ball" | "Business Card" | "Hallucination" => prob(g, 1, xx),
        "Chaos the Clown" | "Credit Card" | "Banner" | "Fibonacci" | "Scary Face" | "Delayed Gratification" | "Even Steven"
        | "Odd Todd" | "Superposition" | "Egg" | "Burglar" | "DNA" | "Hiker" | "Astronomer" | "Golden Ticket" | "Acrobat"
        | "Certificate" | "Hanging Chad" | "Rough Gem" | "Arrowhead" | "Onyx Agate" | "Flower Pot" | "Seeing Double"
        | "Matador" | "Riff-raff" | "Vagabond" | "Baron" | "Photograph" | "Gift Card" | "To the Moon" | "Baseball Card"
        | "Trading Card" | "Smiley Face" | "Shoot the Moon" | "Triboulet" | "Perkeo" | "Golden Joker" => {
            vec![xx.clone()]
        }
        "Seltzer" => vec![num(c)],
        "Drunkard" => vec![d_size],
        "Green Joker" => vec![ev("hand_add"), ev("discard_sub"), amult.clone()],
        "Greedy Joker" | "Lusty Joker" | "Wrathful Joker" | "Gluttonous Joker" => {
            vec![ev("s_mult"), misc("suits_singular", ev("suit").as_str().unwrap_or(""))]
        }
        "Blue Joker" => vec![xx.clone(), num(nv(xx) * g.draw_pile.len() as f64)],
        "Hack" | "Dusk" | "Sock and Buskin" => vec![num(nv(xx) + 1.0)],
        "Faceless Joker" => vec![ev("dollars"), ev("faces")],
        "Juggler" => vec![h_size],
        "Joker Stencil" => {
            let n = g.jokers.len() as i64;
            let st = g.jokers.iter().filter(|q| q.borrow().spec.name == "Joker Stencil").count() as i64;
            vec![json!(g.joker_slots() as i64 - n + st)]
        }
        "Ceremonial Dagger" => vec![amult.clone()],
        "Misprint" => {
            ms = Some(vec!["  +".into()]);
            vec![]
        }
        "Mystic Summit" => vec![ev("mult"), ev("d_remaining")],
        "Loyalty Card" => {
            let every = e.get("every").and_then(|v| v.as_i64()).unwrap_or(5);
            let at_create = if g.jokers.iter().any(|q| std::rc::Rc::ptr_eq(q, j)) { jb.hands_at_create as i64 } else { g.hands_played as i64 };
            let rem = (every - (g.hands_played as i64 - at_create)).rem_euclid(every + 1);
            vec![
                ev("Xmult"),
                json!(every + 1),
                json!(variable(if rem == 0 { "loyalty_active" } else { "loyalty_inactive" }, &[json!(rem)])),
            ]
        }
        "Abstract Joker" => vec![xx.clone(), num(g.jokers.len() as f64 * nv(xx))],
        "Gros Michel" | "Cavendish" => {
            let mut v = if name == "Gros Michel" { vec![ev("mult")] } else { vec![ev("Xmult")] };
            v.extend(prob(g, 1, &ev("odds")));
            v
        }
        "Scholar" => vec![ev("mult"), ev("chips")],
        "Spare Trousers" => vec![xx.clone(), poker_hand("Two Pair"), amult.clone()],
        "Ride the Bus" | "Flash Card" | "Red Card" => vec![xx.clone(), amult.clone()],
        "Blackboard" => vec![xx.clone(), misc("suits_plural", "Spades"), misc("suits_plural", "Clubs")],
        "Runner" | "Ice Cream" | "Wee Joker" | "Square Joker" => vec![ev("chips"), ev("chip_mod")],
        "Constellation" | "Throwback" | "Glass Joker" | "Hit the Road" | "Madness" | "Vampire" | "Hologram" | "Obelisk"
        | "Lucky Cat" | "Campfire" => vec![xx.clone(), axm.clone()],
        "To Do List" => vec![ev("dollars"), full_word(jb.named_hand)],
        "Blueprint" | "Brainstorm" | "Luchador" => {
            if in_jokers {
                me = Some(vec![String::new()]);
            }
            vec![]
        }
        "Swashbuckler" => {
            let s: i64 = g.jokers.iter().filter(|q| !std::rc::Rc::ptr_eq(q, j)).map(|q| g.sell_value(q) as i64).sum();
            vec![json!(s)]
        }
        "Troubadour" => vec![ev("h_size"), json!(-ev("h_plays").as_i64().unwrap_or(0))],
        "Bloodstone" => {
            let mut v = prob(g, 1, &ev("odds"));
            v.push(ev("Xmult"));
            v
        }
        "Merry Andy" => vec![d_size, h_size],
        "The Idol" => vec![xx.clone(), rw(g.idol_rank), sw(g.idol_suit, "suits_plural")],
        "The Duo" | "The Trio" | "The Family" | "The Order" | "The Tribe" => vec![axm.clone(), poker_hand(typ.as_str().unwrap_or(""))],
        "Card Sharp" => vec![ev("Xmult")],
        "Seance" => vec![poker_hand(ev("poker_hand").as_str().unwrap_or(""))],
        "Cloud 9" => {
            let t = tally(&|c| c.rank == rumbot_sim::cards::Rank::Nine);
            vec![xx.clone(), num(nv(xx) * t as f64)]
        }
        "Rocket" => vec![ev("dollars"), ev("increase")],
        "Turtle Bean" => vec![ev("h_size"), ev("h_mod")],
        "Erosion" => {
            let start = g.starting_deck_size as i64;
            vec![xx.clone(), num((nv(xx) * (start - nplaying) as f64).max(0.0)), json!(start)]
        }
        "Reserved Parking" => {
            let mut v = vec![ev("dollars")];
            v.extend(prob(g, 1, &ev("odds")));
            v
        }
        "Mail-In Rebate" => vec![xx.clone(), rw(g.mail_rank)],
        "Bull" => vec![xx.clone(), num(nv(xx) * (g.money.max(0)) as f64)],
        "Diet Cola" => vec![name_text("Tag", "tag_double")],
        "Popcorn" => vec![amult.clone(), xx.clone()],
        "Ramen" => vec![axm.clone(), xx.clone()],
        "Ancient Joker" => vec![xx.clone(), sw(g.ancient_suit, "suits_singular")],
        "Walkie Talkie" => vec![ev("chips"), ev("mult")],
        "Castle" => vec![ev("chip_mod"), sw(g.castle_suit, "suits_singular"), ev("chips")],
        "Stuntman" => vec![ev("chip_mod"), ev("h_size")],
        "Invisible Joker" => {
            if g.jokers.iter().any(|q| q.borrow().edition == Edition::Negative) {
                me = Some(localize_other("remove_negative", &[]));
            }
            vec![xx.clone(), num(jb.secondary)]
        }
        "Satellite" => vec![xx.clone(), num(0.0)], // sim_text counts consumeable_usage entries; traces always show $0
        "Driver's License" => vec![xx.clone(), json!(tally(&|c| c.enhancement != Enhancement::None))],
        "Bootstraps" => {
            let d = nv(&ev("dollars"));
            vec![ev("mult"), ev("dollars"), num(nv(&ev("mult")) * (g.money as f64 / d).floor())]
        }
        "Caino" => vec![xx.clone(), caino.clone()],
        "Yorick" => vec![ev("xmult"), ev("discards"), yorick_d.clone(), axm.clone()],
        _ => vec![],
    };
    (lv, ms, me)
}

/// card_effect for a joker (`debuff`: the card is debuffed)
pub fn joker_effect(g: &GameState, j: &JokerRef, in_jokers: bool, debuff: bool) -> String {
    if debuff {
        return join(localize_other("debuffed_default", &[]));
    }
    let (lv, ms, me) = joker_vars(g, j, in_jokers);
    let key = rumbot_sim::joker_data::joker_row(j.borrow().spec.name).map_or("", |r| r.key);
    let mut lines = ms.unwrap_or_default();
    lines.extend(localize_desc("Joker", key, &lv));
    lines.extend(me.unwrap_or_default());
    join(lines)
}
