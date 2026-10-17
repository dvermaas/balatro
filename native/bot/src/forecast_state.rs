//! Public gamestate -> simcore state description, in one step: `sim_bridge.describe(rollout.build_state(pub))`
//! (rollout.py build_state 210-292, playing_card 87-104, item_card 196-207, _edition 81-84, and the ctx
//! part of _joker_dynamic 112-174; sim_bridge.py describe 101-145, _playing, _edition, _rank, _suit, _name).
//! rng=False, no items. EVERYTHING except `jokers` (task 5 fills it: see JOKER HOOK) and the RNG pools.
//!
//! What Jackdaw's `initialize_run(.., "ROLLOUT")` contributes is constant for the verified Red/White runs and
//! hardcoded below (first boss bl_window, idol J of Spades, mail 7, ancient and castle Diamonds, starting
//! limits 8/5/2). Only Red Deck / White stake is verified (fc_state: every forecast pub in the traces).
//! sort_ids: Jackdaw's process-wide counter has no meaning, only uniqueness; deck cards get 0..N-1, then the
//! jokers N..N+nj-1 (task 5), then the consumables (Jackdaw creation order, measured identical offsets).

use crate::gs;
use fancy_regex::Regex as FRegex;
use pycompat::fmt::pyround_int;
use pycompat::json::pyrepr;
use regex::Regex;
use rumbot_sim::consumable_data::CONSUMABLE_DATA;
use serde_json::{json, Map, Value};
use simcore::StateDesc;
use std::sync::OnceLock;

const EDITIONS: [&str; 4] = ["foil", "holo", "polychrome", "negative"];
const ENHANCEMENTS: [&str; 8] = ["bonus", "mult", "wild", "glass", "steel", "stone", "gold", "lucky"];
/// `HandType` iteration order (jackdaw.engine.data.hands).
const HAND_TYPES: [&str; 12] = [
    "Flush Five", "Flush House", "Five of a Kind", "Straight Flush", "Four of a Kind", "Full House", "Flush",
    "Straight", "Three of a Kind", "Two Pair", "Pair", "High Card",
];
/// `rollout.BOSS_KEY` (BLINDS name -> key).
const BOSS_KEY: [(&str, &str); 30] = [
    ("The Arm", "bl_arm"), ("Big Blind", "bl_big"), ("The Club", "bl_club"), ("The Eye", "bl_eye"),
    ("Amber Acorn", "bl_final_acorn"), ("Cerulean Bell", "bl_final_bell"), ("Crimson Heart", "bl_final_heart"),
    ("Verdant Leaf", "bl_final_leaf"), ("Violet Vessel", "bl_final_vessel"), ("The Fish", "bl_fish"),
    ("The Flint", "bl_flint"), ("The Goad", "bl_goad"), ("The Head", "bl_head"), ("The Hook", "bl_hook"),
    ("The House", "bl_house"), ("The Manacle", "bl_manacle"), ("The Mark", "bl_mark"), ("The Mouth", "bl_mouth"),
    ("The Needle", "bl_needle"), ("The Ox", "bl_ox"), ("The Pillar", "bl_pillar"), ("The Plant", "bl_plant"),
    ("The Psychic", "bl_psychic"), ("The Serpent", "bl_serpent"), ("Small Blind", "bl_small"),
    ("The Tooth", "bl_tooth"), ("The Wall", "bl_wall"), ("The Water", "bl_water"), ("The Wheel", "bl_wheel"),
    ("The Window", "bl_window"),
];
/// `init_game_object` bosses_used keys (blinds with boss is not None), in dict order.
const BOSSES_USED: [&str; 28] = [
    "bl_arm", "bl_club", "bl_eye", "bl_final_acorn", "bl_final_bell", "bl_final_heart", "bl_final_leaf",
    "bl_final_vessel", "bl_fish", "bl_flint", "bl_goad", "bl_head", "bl_hook", "bl_house", "bl_manacle", "bl_mark",
    "bl_mouth", "bl_needle", "bl_ox", "bl_pillar", "bl_plant", "bl_psychic", "bl_serpent", "bl_tooth", "bl_wall",
    "bl_water", "bl_wheel", "bl_window",
];
/// build_state's `planets` list (the first `planets_used` of these count as used).
const PLANETS: [&str; 12] = [
    "c_pluto", "c_mercury", "c_uranus", "c_venus", "c_saturn", "c_jupiter", "c_earth", "c_mars", "c_neptune",
    "c_planet_x", "c_ceres", "c_eris",
];

fn deck_name(pub_deck: &str) -> &'static str {
    match pub_deck {
        "RED" => "Red Deck", "BLUE" => "Blue Deck", "YELLOW" => "Yellow Deck", "GREEN" => "Green Deck",
        "BLACK" => "Black Deck", "MAGIC" => "Magic Deck", "NEBULA" => "Nebula Deck", "GHOST" => "Ghost Deck",
        "ABANDONED" => "Abandoned Deck", "CHECKERED" => "Checkered Deck", "ZODIAC" => "Zodiac Deck",
        "PAINTED" => "Painted Deck", "ANAGLYPH" => "Anaglyph Deck", "PLASMA" => "Plasma Deck",
        "ERRATIC" => "Erratic Deck",
        _ => "Red Deck",
    }
}

fn stake_num(s: &str) -> i64 {
    match s {
        "WHITE" => 1, "RED" => 2, "GREEN" => 3, "BLACK" => 4, "BLUE" => 5, "PURPLE" => 6, "ORANGE" => 7,
        "GOLD" => 8,
        _ => 1,
    }
}

/// `str(x)`: strings as they are, everything else as its repr (`str(None)` = "None").
fn py_str(v: &Value) -> String {
    match v {
        Value::String(s) => s.clone(),
        o => pyrepr(o),
    }
}

// ---------------------------------------------------------------------------
// Cards
// ---------------------------------------------------------------------------

/// `_edition(card, mod)` then `sim_bridge._edition`: foil/holo/polychrome/negative or None.
fn edition(m: &Value) -> Value {
    let e = gs::get(m, "edition");
    let ed = if gs::truthy(e) { gs::s(e).to_lowercase() } else { String::new() };
    if EDITIONS.contains(&ed.as_str()) {
        Value::String(ed)
    } else {
        Value::Null
    }
}

fn rank_code(r: &str) -> Result<&str, String> {
    match r {
        "A" | "K" | "Q" | "J" | "T" | "2" | "3" | "4" | "5" | "6" | "7" | "8" | "9" => Ok(r),
        o => Err(format!("unknown rank {o:?}")),
    }
}

fn suit_code(s: &str) -> Result<&str, String> {
    match s {
        "H" | "D" | "S" | "C" => Ok(s),
        o => Err(format!("unknown suit {o:?}")),
    }
}

fn extra_chips_re() -> &'static Regex {
    static R: OnceLock<Regex> = OnceLock::new();
    R.get_or_init(|| Regex::new(r"(?i)\+(\d+) extra chips").unwrap())
}

/// `sim_bridge._playing(rollout.playing_card(src))`, sort_id supplied by the caller.
pub(crate) fn playing(src: &Value, sort_id: usize) -> Result<Value, String> {
    let v = gs::get(src, "value");
    let m = gs::get(src, "modifier");
    let rank = rank_code(gs::s(gs::get(v, "rank")))?;
    let suit = suit_code(gs::s(gs::get(v, "suit")))?;
    let enh = gs::s(gs::get(m, "enhancement")).to_lowercase();
    let enh = if ENHANCEMENTS.contains(&enh.as_str()) { Some(enh) } else { None };
    let mut extra = 0i64;
    if let Some(c) = extra_chips_re().captures(gs::s(gs::get(v, "effect"))) {
        let n: i64 = c[1].parse().map_err(|e| format!("extra chips: {e}"))?;
        extra = n - if enh.as_deref() == Some("bonus") { 30 } else { 0 };
    }
    let seal = gs::get(m, "seal");
    let seal = if gs::truthy(seal) { Value::String(gs::s(seal).to_lowercase()) } else { Value::Null };
    Ok(json!({
        "sort_id": sort_id, "rank": rank, "suit": suit, "enh": enh, "edition": edition(m), "seal": seal,
        "extra_chips": extra, "debuff": gs::truthy(gs::get(gs::get(src, "state"), "debuff")),
        "played_this_ante": false,
    }))
}

/// `sim_bridge._name(key)` for a consumable key.
pub(crate) fn consumable_name(key: &str) -> Result<&'static str, String> {
    CONSUMABLE_DATA
        .iter()
        .find(|r| r.key == key)
        .map(|r| r.name)
        .ok_or_else(|| format!("no rumbot-sim name for {key}"))
}

// ---------------------------------------------------------------------------
// ctx from the jokers' effect text (rollout._joker_dynamic, the ctx[...] assignments only)
// ---------------------------------------------------------------------------

#[derive(Default)]
struct Ctx {
    hands_played: i64,
    skips: i64,
    tarots_used: i64,
    planets_used: i64,
    ancient: Option<String>,
    idol: Option<(String, String)>,
    mail: Option<String>,
    castle: Option<String>,
}

fn re(p: &'static str, cell: &'static OnceLock<FRegex>) -> &'static FRegex {
    cell.get_or_init(|| FRegex::new(p).unwrap())
}

/// Group 1 of the first match (`_num(pattern, text, ...)`).
fn cap1(r: &FRegex, text: &str) -> Option<String> {
    r.captures(text).ok().flatten().map(|c| c[1].to_string())
}

fn joker_ctx(key: &str, text: &str, ctx: &mut Ctx) {
    static THROW: OnceLock<FRegex> = OnceLock::new();
    static TEL: OnceLock<FRegex> = OnceLock::new();
    static SAT: OnceLock<FRegex> = OnceLock::new();
    static ANC: OnceLock<FRegex> = OnceLock::new();
    static IDOL: OnceLock<FRegex> = OnceLock::new();
    static MAIL: OnceLock<FRegex> = OnceLock::new();
    static CAS: OnceLock<FRegex> = OnceLock::new();
    match key {
        "j_throwback" => {
            // x_mult from "Currently X<n> Mult", else the center's default 1
            let x = cap1(re(r"Currently X(\d+(?:\.\d+)?) Mult", &THROW), text)
                .map(|s| s.parse::<f64>().unwrap())
                .unwrap_or(1.0);
            ctx.skips = pyround_int((x - 1.0) / 0.25);
        }
        "j_fortune_teller" => {
            ctx.tarots_used = cap1(re(r"Currently \+(\d+)", &TEL), text).map(|s| s.parse().unwrap()).unwrap_or(0)
        }
        "j_satellite" => {
            ctx.planets_used = cap1(re(r"Currently \$(\d+)", &SAT), text).map(|s| s.parse().unwrap()).unwrap_or(0)
        }
        "j_ancient" => {
            if let Some(s) = cap1(re(r"with (Heart|Diamond|Club|Spade) suit", &ANC), text) {
                ctx.ancient = Some(format!("{s}s"));
            }
        }
        "j_idol" => {
            if let Ok(Some(c)) = re(r"Each played (\w+) of (Hearts|Diamonds|Clubs|Spades)", &IDOL).captures(text) {
                ctx.idol = Some((c[1].to_string(), c[2].to_string()));
            }
        }
        "j_mail" => {
            if let Some(r) = cap1(re(r"discarded (\w+), rank", &MAIL), text) {
                ctx.mail = Some(r);
            }
        }
        "j_castle" => {
            if let Some(s) = cap1(re(r"discarded (Heart|Diamond|Club|Spade) card", &CAS), text) {
                ctx.castle = Some(format!("{s}s"));
            }
        }
        _ => {}
    }
}

/// `sim_bridge._rank` of a full rank word ("Ace" -> "A", "10" -> "T"); unknown words pass through.
fn rank_word(r: &str) -> String {
    match r {
        "Ace" => "A",
        "King" => "K",
        "Queen" => "Q",
        "Jack" => "J",
        "10" => "T",
        o => o,
    }
    .to_string()
}

/// `sim_bridge._suit` of a full suit word.
fn suit_word(s: &str) -> String {
    match s {
        "Hearts" => "H",
        "Diamonds" => "D",
        "Clubs" => "C",
        "Spades" => "S",
        o => o,
    }
    .to_string()
}

// ---------------------------------------------------------------------------
// build_state + describe
// ---------------------------------------------------------------------------

/// The description as the JSON `sim_bridge.describe` makes (keys in its order); `jokers` is `[]` until task 5.
pub fn state_json_from_pub(p: &Value) -> Result<Value, String> {
    let hands = gs::get(p, "hands");
    let zero = Value::from(0);
    let mut ctx = Ctx::default();
    for (_, h) in gs::d(hands) {
        ctx.hands_played += gs::int(gs::or(gs::get(h, "played"), &zero));
    }
    let red = json!("RED");
    let white = json!("WHITE");
    let deck = deck_name(gs::s(gs::or(gs::get(p, "deck"), &red)));
    // vouchers: starting vouchers of the deck, then the used ones (dict order), each once
    let mut vouchers: Vec<String> = Vec::new();
    match deck {
        "Magic Deck" => vouchers.push("v_crystal_ball".into()),
        "Nebula Deck" => vouchers.push("v_telescope".into()),
        "Zodiac Deck" => {
            vouchers.extend(["v_tarot_merchant", "v_planet_merchant", "v_overstock_norm"].map(String::from))
        }
        _ => {}
    }
    for k in gs::d(gs::get(p, "used_vouchers")).keys() {
        if !vouchers.contains(k) {
            vouchers.push(k.clone());
        }
    }
    // deck: order-free multiset in the canonical sort (risk 4): (str(suit), str(rank), str(modifier)), stable
    let mut deck_src: Vec<&Value> = gs::cards_of(p, "cards").iter().collect();
    deck_src.extend(gs::cards_of(p, "hand").iter());
    let mut keyed: Vec<((String, String, String), &Value)> = deck_src
        .iter()
        .map(|c| {
            let v = gs::get(c, "value");
            (// key uses _d(modifier): an empty Lua table arrives as [] and sorts as "{}"
            (py_str(gs::get(v, "suit")), py_str(gs::get(v, "rank")), pyrepr(&Value::Object(gs::d(gs::get(c, "modifier")).clone()))), *c)
        })
        .collect();
    keyed.sort_by(|a, b| a.0.cmp(&b.0)); // stable, like list.sort
    let cards: Vec<Value> = keyed.iter().enumerate().map(|(i, (_, c))| playing(c, i)).collect::<Result<_, _>>()?;

    // JOKER HOOK (task 5): the jokers in pub order get sort_ids N..N+nj-1 and their descs go into `jokers`.
    // Here only their effect text feeds ctx (skips, tarots, planets, targets), exactly as _joker_dynamic does.
    let jokers_src = gs::cards_of(p, "jokers");
    for src in jokers_src {
        joker_ctx(gs::s(gs::get(src, "key")), gs::s(gs::get(gs::get(src, "value"), "effect")), &mut ctx);
    }
    let jokers: Vec<Value> = crate::forecast_jokers::joker_values_from_pub(p, cards.len() as u64);
    let mut next_id = cards.len() + jokers_src.len();

    let mut consumables = Vec::new();
    for src in gs::cards_of(p, "consumables") {
        let set = gs::s(gs::get(src, "set"));
        if matches!(set, "DEFAULT" | "ENHANCED" | "JOKER") {
            return Err(format!("consumable area holds a {set} card"));
        }
        let ed = edition(gs::get(src, "modifier"));
        let name = consumable_name(gs::s(gs::get(src, "key")))?;
        consumables.push(json!({"sort_id": next_id, "name": name, "negative": ed == "negative"}));
        next_id += 1;
    }

    // limits: pub limits, else the run's starting values plus the vouchers' effects
    let has = |k: &str| vouchers.iter().any(|v| v == k);
    let base_hand = 8 + has("v_paint_brush") as i64 + has("v_palette") as i64;
    let base_jokers = 5 + has("v_antimatter") as i64;
    let base_cons = 2 + has("v_crystal_ball") as i64;
    let lim = |a: &str, dflt: i64| {
        let l = gs::get(gs::get(p, a), "limit");
        if gs::truthy(l) {
            gs::int(l)
        } else {
            dflt
        }
    };

    // hand levels
    let mut levels = Map::new();
    for name in gs::d(hands).keys() {
        if !HAND_TYPES.contains(&name.as_str()) {
            return Err(format!("{name:?} is not a valid HandType"));
        }
    }
    for name in HAND_TYPES {
        let h = gs::d(hands).get(name).unwrap_or(&Value::Null);
        let level = gs::d(h).get("level").map(gs::int).unwrap_or(1);
        let played = gs::d(h).get("played").map(gs::int).unwrap_or(0);
        let ptr = gs::d(h).get("played_this_round").map(gs::truthy).unwrap_or(false);
        levels.insert(name.into(), json!({"level": level, "played": played, "played_this_round": ptr}));
    }

    // blinds: the known boss (the init boss bl_window otherwise), which blinds are done
    let blinds = gs::get(p, "blinds");
    let boss_name = gs::s(gs::get(gs::get(blinds, "boss"), "name"));
    let boss_key = BOSS_KEY.iter().find(|(n, _)| *n == boss_name).map(|(_, k)| *k);
    let mut bosses_used: Map<String, Value> = BOSSES_USED.iter().map(|k| (k.to_string(), json!(0))).collect();
    bosses_used.insert("bl_window".into(), json!(1)); // assign_ante_blinds(1, "ROLLOUT")
    let boss = match boss_key {
        Some(k) => {
            let n = bosses_used.get(k).and_then(|v| v.as_i64()).unwrap_or(0);
            bosses_used.insert(k.into(), json!(n + 1));
            k
        }
        None => "bl_window",
    };
    let mut on_deck: Option<usize> = None;
    for (i, typ) in ["small", "big", "boss"].iter().enumerate() {
        let st = gs::s(gs::get(gs::get(blinds, typ), "status"));
        if !(st == "DEFEATED" || st == "SKIPPED") && on_deck.is_none() {
            on_deck = Some(i);
        }
    }

    // planets used (Satellite): the first n of PLANETS, one use each
    let n_pl = (ctx.planets_used.max(0) as usize).min(PLANETS.len());
    let unique_planets: Vec<&str> = PLANETS[..n_pl].iter().map(|k| consumable_name(k)).collect::<Result<_, _>>()?;

    let idol = match &ctx.idol {
        Some((r, s)) => json!([rank_word(r), suit_word(s)]),
        None => json!(["J", "S"]),
    };
    let mail = ctx.mail.as_deref().map(rank_word).unwrap_or_else(|| "7".into());
    let ancient = ctx.ancient.as_deref().map(suit_word).unwrap_or_else(|| "D".into());
    let castle = ctx.castle.as_deref().map(suit_word).unwrap_or_else(|| "D".into());
    let ante_v = gs::get(p, "ante_num");

    Ok(json!({
        "deck": deck,
        "stake": stake_num(gs::s(gs::or(gs::get(p, "stake"), &white))),
        "ante": if gs::truthy(ante_v) { gs::int(ante_v) } else { 1 },
        "blind_index": on_deck.unwrap_or(0),
        "round": gs::int(gs::get(p, "round_num")),
        "money": gs::int(gs::get(p, "money")),
        "hands_played": ctx.hands_played,
        "skips": ctx.skips,
        "boss": boss,
        "bosses_used": bosses_used,
        "cards": cards,
        "jokers": jokers,
        "consumables": consumables,
        "vouchers": vouchers,
        "levels": levels,
        "idol": idol,
        "mail": mail,
        "ancient": ancient,
        "castle": castle,
        "tarots_used": ctx.tarots_used,
        "planets_used": n_pl,
        "unique_planets": unique_planets,
        "pool_flags": Vec::<String>::new(),
        "most_played": "High Card",
        "hand_size": lim("hand", base_hand),
        "joker_slots": lim("jokers", base_jokers),
        "consumable_slots": lim("consumables", base_cons),
        "last_tarot_planet": Value::Null,
    }))
}

/// The same, as the simcore description (`build(&desc)` makes the rumbot game).
pub fn state_desc_from_pub(p: &Value) -> Result<StateDesc, String> {
    serde_json::from_value(state_json_from_pub(p)?).map_err(|e| format!("desc: {e}"))
}
