//! Port of src/decisions.py. Lines 1-157 + 181-199 (constants, _round_info, _deck_remaining, _levels,
//! _run_context, blind_select, _remember, _recent, _play) live here; the rest is split by handler:
//! play.rs 158-452, shop.rs 453-850, booster.rs 851-987.
//!
//! Naming convention: Python names with the leading underscore dropped (`_round_info` -> `round_info`),
//! all `pub(crate)`; the handlers (`blind_select`, `selecting_hand`, `shop`, `booster`) are `pub`.
//! The `hands` re-exports (`_boss_levels`, `_boss_allows`, `BossAllow`) keep their hands.rs names.
//! Functions that can raise RPCError through `bot.act` return `Result<_, RpcError>`.

use crate::forecast_stats::pystr;
use crate::gs::{self, area, cards_of};
use crate::hands::{self, card_info};
use crate::iface::RpcError;
use crate::jev_bot::{current_blind, item_desc, opt, JevBot, Stop};
use crate::prompts::{GAME_RULES, INSTRUCTIONS, OPTIONS};
use crate::strategy;
use pycompat::fmt::pyround_int;
use serde_json::{json, Map, Value};
use std::collections::HashSet;

pub mod booster;
pub mod play;
pub mod shop;

pub use booster::booster;
pub use play::selecting_hand;
pub use shop::shop;

// decisions.py 28-34, 69-92
pub const BUY_SCORE: f64 = 2.5;
pub const BUY_SCORE_LOOSE: i64 = 2;
pub const EARLY_JOKERS: i64 = 4;
pub const EARLY_ANTE: i64 = 3;
pub const SURE_GAIN: f64 = 0.5;
pub const RECENT_PLAYS: usize = 10;
pub const NEAR_PACE: f64 = 0.8;
pub const PACK_SCORE: f64 = 1.5;
pub const INTEREST_BUFFER: i64 = 25;
/// v44: never bought. -1 ante makes the forecast's horizon of blinds cleared easier, but a win needs that ante
/// again with a hand (Petroglyph: a discard) fewer every round.
pub const NEVER_VOUCHERS: &[&str] = &["Hieroglyph", "Petroglyph"];
/// The money kept for interest at this ante: INTEREST_BUFFER (lab `late-floor=N`: $N less per ante after 4).
pub fn interest_buffer(ante: i64) -> i64 {
    let n = crate::experiment::get().late_floor;
    if n > 0 && ante > 4 { (INTEREST_BUFFER - n * (ante - 4)).max(0) } else { INTEREST_BUFFER }
}
pub const MAX_REROLLS: i64 = 2;
pub const INVEST_BELOW: f64 = 0.6;
pub const BANK_ABOVE: f64 = 1.2;
pub const INVEST_FLOOR: i64 = 5;
pub const INVEST_REROLL_FLOOR: i64 = 10;
pub const FORECAST_TIMEOUT: f64 = 10.0;
pub const SWAP_SELLS: usize = 2;
pub const SURE_SE: i64 = 1; // v31 (v20: 2)
pub const HIDE_SE: i64 = 2;
pub const REROLL_SE: i64 = 1;
pub const ITEM_MIN_BUDGET: f64 = 1.0;
pub const PICK_CLEAR: f64 = 1.25;
pub const PLANET_FLOOR: i64 = 4; // v28: shop planets for the deck's top hand and Celestial packs are bought while $4 stays
/// Forecast exceptions are appended to `<JevBot.runs_root>/_forecast_errors.log` (default root "runs").
pub const FORECAST_ERROR_LOG: &str = "_forecast_errors.log";

/// The module constants the lab overrides (policy_lab.py `_play_bot`): read them from `bot.dec`, never
/// from a const. `production()` = the Python defaults; `lab()` = what `_play_bot` sets for the arms the
/// port is verified on (bot-forecast-long / bot-forecast-items): FORECAST_N 16 (policy_lab's own
/// FORECAST_N), FORECAST_BUDGET None, FORECAST_BOSSES 3, VOUCHERS strong flags all False.
#[derive(Clone, Debug)]
pub struct DecisionCfg {
    pub forecast_n: i64,
    pub forecast_budget: Option<f64>,
    pub forecast_bosses: i64,
    /// false = every VOUCHERS entry's strong flag reads False (`voucher()` applies it).
    pub strong_vouchers: bool,
}

impl DecisionCfg {
    pub fn production() -> Self {
        DecisionCfg { forecast_n: 24, forecast_budget: Some(4.0), forecast_bosses: 3, strong_vouchers: true }
    }
    pub fn lab() -> Self {
        Self::lab_with(16, 3)
    }
    /// A lab arm with `n` / `bosses` options (BOT_ARMS "n" / "bosses").
    pub fn lab_with(n: i64, bosses: i64) -> Self {
        DecisionCfg { forecast_n: n, forecast_budget: None, forecast_bosses: bosses, strong_vouchers: false }
    }
}

/// decisions.VOUCHERS in dict order: (name, description, strong).
pub const VOUCHERS: &[(&str, &str, bool)] = &[
    ("Grabber", "+1 hand every round for the rest of the run (~+25% chips per round)", true),
    ("Nacho Tong", "+1 more hand every round for the rest of the run", true),
    ("Wasteful", "+1 discard every round for the rest of the run", true),
    ("Recyclomancy", "+1 more discard every round for the rest of the run", true),
    ("Paint Brush", "+1 hand size: more cards to build hands from, every round", true),
    ("Palette", "+1 more hand size, every round", true),
    ("Clearance Sale", "everything in the shop costs 25% less for the rest of the run", true),
    ("Liquidation", "everything in the shop costs 50% less for the rest of the run", true),
    ("Overstock", "+1 card slot in every shop: more jokers to choose from", true),
    ("Overstock Plus", "+1 more card slot in every shop", true),
    ("Antimatter", "+1 joker slot", true),
    ("Reroll Surplus", "rerolls cost $2 less", false),
    ("Reroll Glut", "rerolls cost $2 less again", false),
    ("Crystal Ball", "+1 consumable slot", false),
    ("Telescope", "Celestial packs always contain the planet for our most played hand", false),
    ("Observatory", "useless for us: we use planets right away instead of holding them", false),
    ("Hone", "Foil, Holo and Polychrome cards appear 2x more often", false),
    ("Glow Up", "Foil, Holo and Polychrome cards appear 4x more often", false),
    ("Tarot Merchant", "Tarot cards appear more often in the shop", false),
    ("Tarot Tycoon", "Tarot cards appear much more often in the shop", false),
    ("Planet Merchant", "Planet cards appear more often in the shop", false),
    ("Planet Tycoon", "Planet cards appear much more often in the shop", false),
    ("Seed Money", "interest cap $5 -> $10: only pays if we hold $50+", false),
    ("Money Tree", "interest cap -> $20: only pays if we hold $100+", false),
    ("Blank", "does nothing by itself (unlocks Antimatter, +1 joker slot)", false),
    ("Magic Trick", "playing cards can appear in the shop", false),
    ("Illusion", "shop playing cards may have editions or seals", false),
    ("Hieroglyph", "-1 ante (easier blinds) but -1 hand every round", false),
    ("Petroglyph", "-1 ante but -1 discard every round", false),
    ("Director's Cut", "reroll the boss blind once per ante for $10", false),
    ("Retcon", "reroll the boss blind any number of times for $10", false),
];

/// `VOUCHERS.get(name)` -> (description, strong) with the cfg's strong-flag override applied.
pub(crate) fn voucher(cfg: &DecisionCfg, name: &str) -> Option<(&'static str, bool)> {
    VOUCHERS.iter().find(|v| v.0 == name).map(|v| (v.1, v.2 && cfg.strong_vouchers))
}

/// prompts.INSTRUCTIONS[key] (KeyError panics).
pub(crate) fn instr(key: &str) -> &'static str {
    INSTRUCTIONS.iter().find(|(k, _)| *k == key).unwrap_or_else(|| panic!("KeyError: {key}")).1
}

/// prompts.OPTIONS[key].
pub(crate) fn option_tpl(key: &str) -> &'static str {
    OPTIONS.iter().find(|(k, _)| *k == key).unwrap_or_else(|| panic!("KeyError: {key}")).1
}

/// `x or 0` for a gamestate value (kept as the original int/float Value).
fn or0(v: &Value) -> Value {
    if gs::truthy(v) { v.clone() } else { json!(0) }
}

fn or1(v: &Value) -> Value {
    if gs::truthy(v) { v.clone() } else { json!(1) }
}

// decisions.py 95-104
pub(crate) fn round_info(gs_: &Value) -> Value {
    let rnd = gs::d(gs::get(gs_, "round"));
    let blind = current_blind(gs_);
    let (score, chips) = (or0(gs::get(&blind, "score")), or0(gs::mget(rnd, "chips")));
    // max(0, score - chips): int when both ints (max keeps the first of equal values, i.e. int 0)
    let need = match (score.as_i64(), chips.as_i64()) {
        (Some(a), Some(b)) => json!((a - b).max(0)),
        _ => {
            let x = gs::num(&score) - gs::num(&chips);
            if x > 0.0 { json!(x) } else { json!(0) }
        }
    };
    let hands_left = or0(gs::mget(rnd, "hands_left"));
    let per = if gs::truthy(&hands_left) { json!(pyround_int(gs::num(&need) / gs::num(&hands_left))) } else { Value::Null };
    let effect = gs::get(&blind, "effect");
    json!({
        "blind": gs::get(&blind, "name"), "boss_effect": if gs::truthy(effect) { effect.clone() } else { json!("none") },
        "chip_target": gs::get(&blind, "score"), "chips_scored": gs::mget(rnd, "chips"), "chips_still_needed": need,
        "hands_left": hands_left, "discards_left": gs::mget(rnd, "discards_left"),
        "chips_needed_per_remaining_hand": per,
    })
}

// decisions.py 107-116
pub(crate) fn deck_remaining(gs_: &Value) -> Value {
    let infos: Vec<hands::Info> = cards_of(gs_, "cards").iter().map(card_info).collect();
    // fixed suit order: Counter order would reveal the bottom card of the pile
    let mut by_suit = Map::new();
    for s in ["H", "D", "C", "S"] {
        let n = infos.iter().filter(|i| i.rank_truthy().is_some() && i.suit_is(s)).count();
        if n > 0 {
            by_suit.insert(hands::suit_name(s).to_string(), json!(n));
        }
    }
    let mut counts: Vec<(&str, usize)> = Vec::new();
    for i in &infos {
        if let Some(r) = i.rank_truthy() {
            match counts.iter_mut().find(|(k, _)| *k == r) {
                Some(e) => e.1 += 1,
                None => counts.push((r, 1)),
            }
        }
    }
    counts.sort_by_key(|(r, _)| hands::rank_index(r));
    let mut by_rank = Map::new();
    for (r, n) in counts {
        by_rank.insert(hands::rank_name(r).to_string(), json!(n));
    }
    json!({"cards": infos.len(), "by_suit": by_suit, "by_rank": by_rank})
}

// decisions.py 119-121: `names` is only used for membership
pub(crate) fn levels(gs_: &Value, names: &HashSet<String>) -> Value {
    let mut o = Map::new();
    for (n, h) in gs::d(gs::get(gs_, "hands")) {
        if names.contains(n) {
            let h = gs::d(h);
            o.insert(
                n.clone(),
                json!(format!("level {}: {} chips x {} mult", pystr(gs::mget(h, "level")),
                    pystr(gs::mget(h, "chips")), pystr(gs::mget(h, "mult")))),
            );
        }
    }
    Value::Object(o)
}

// decisions.py 124-140: (mains, context)
pub(crate) fn run_context(gs_: &Value) -> (Vec<String>, Value) {
    let mains = strategy::main_hands(gs_, 2);
    let (jok, cons) = (area(gs_, "jokers"), area(gs_, "consumables"));
    let mut names: HashSet<String> = mains.iter().cloned().collect();
    for (n, h) in gs::d(gs::get(gs_, "hands")) {
        if gs::num(&or1(gs::mget(gs::d(h), "level"))) > 1.0 {
            names.insert(n.clone());
        }
    }
    let effects: Vec<Value> = cards_of(gs_, "jokers").iter()
        .map(|c| gs::mget(gs::d(gs::get(c, "value")), "effect").clone()).collect();
    let cnt = |m: &Map<String, Value>, k: &str, dflt: i64| -> String {
        match m.get(k) { Some(v) => pystr(v), None => dflt.to_string() }
    };
    let ctx = json!({
        "game_rules": GAME_RULES, "ante": gs::get(gs_, "ante_num"), "round": gs::get(gs_, "round_num"),
        "money": gs::get(gs_, "money"), "interest_per_round": strategy::interest(gs::get(gs_, "money")),
        "main_hands": if mains.is_empty() { json!("none yet") } else { json!(mains) },
        "engine": strategy::engine(&effects),
        "jokers": cards_of(gs_, "jokers").iter().map(|c| item_desc(c, Some(&mains))).collect::<Vec<_>>(),
        "joker_slots": format!("{}/{}", cnt(jok, "count", 0), cnt(jok, "limit", 5)),
        "consumables": cards_of(gs_, "consumables").iter().map(|c| item_desc(c, None)).collect::<Vec<_>>(),
        "consumable_slots": format!("{}/{}", cnt(cons, "count", 0), cnt(cons, "limit", 2)),
        "hand_levels": levels(gs_, &names),
    });
    (mains, ctx)
}

// ---- blind ------------------------------------------------------------------

// decisions.py 143-155
pub fn blind_select(bot: &mut JevBot, gs_: &Value) -> Result<(), Stop> {
    let b = current_blind(gs_);
    if gs::get(&b, "type").as_str() == Some("BOSS") {
        bot.act("select", None)?;
        return Ok(());
    }
    let fmt = |tpl: &str| -> String {
        let mut s = tpl.to_string();
        for k in ["name", "score", "tag_name", "tag_effect"] {
            s = s.replace(&format!("{{{k}}}"), &pystr(gs::get(&b, k)));
        }
        s
    };
    let tag = gs::s(gs::get(&b, "tag_name")).to_string();
    if crate::experiment::get().skip_tags.contains(&tag) && gs::int(gs::get(gs_, "ante_num")) <= crate::experiment::SKIP_ANTE {
        bot.emit("auto", json!({"question": "blind", "choice": "skip", "desc": format!("skip for {tag}"), "reason": "lab skip-tags"}));
        bot.act("skip", None)?;
        return Ok(());
    }
    let opts = vec![
        opt("play", fmt(option_tpl("blind_play")), "select", None),
        opt("skip", fmt(option_tpl("blind_skip")), "skip", None),
    ];
    let (m, p) = bot.ask("blind", gs_, instr("blind"), &opts)?;
    bot.act(&m, p.as_ref())?;
    Ok(())
}

// ---- recent plays (decisions.py 181-198) --------------------------------------

/// Keep this run's recent plays: the shop measures items on them.
pub(crate) fn remember(bot: &mut JevBot, gs_: &Value, hand: &[Value], combo: &[usize]) {
    if bot._recent_seed != *gs::get(gs_, "seed") {
        bot._recent_seed = gs::get(gs_, "seed").clone();
        bot._recent = vec![];
    }
    let rnd = gs::d(gs::get(gs_, "round"));
    bot._recent.push(json!({
        "infos": hand.iter().map(|c| card_info(c).to_json()).collect::<Vec<_>>(),
        "combo": combo,
        "rnd": {"hands_left": gs::mget(rnd, "hands_left"), "discards_left": gs::mget(rnd, "discards_left")},
    }));
    if bot._recent.len() > RECENT_PLAYS {
        let cut = bot._recent.len() - RECENT_PLAYS;
        bot._recent.drain(..cut);
    }
}

pub(crate) fn recent(bot: &JevBot, gs_: &Value) -> Vec<Value> {
    if bot._recent_seed == *gs::get(gs_, "seed") { bot._recent.clone() } else { vec![] }
}

/// Lab (`JEVLAB_EXP=free-discard`): before a play, the spare cards a held joker pays to have discarded
/// (simcore::free_discard, the rule the forecast's rollouts follow too). Empty when off or nothing to gain.
fn discard_goals(gs_: &Value) -> simcore::DiscardGoals {
    let js: Vec<&Value> = cards_of(gs_, "jokers").iter().filter(|c| !gs::truthy(gs::get(gs::get(c, "state"), "debuff"))).collect();
    let effect = |label: &str| js.iter().find(|c| gs::get(c, "label").as_str() == Some(label))
        .map(|c| gs::s(gs::get(gs::get(c, "value"), "effect")).to_string()).unwrap_or_default();
    let castle = regex::Regex::new(r"per discarded (Club|Diamond|Heart|Spade) card").unwrap().captures(&effect("Castle"))
        .map(|m| match &m[1] { "Heart" => 1, "Diamond" => 2, "Club" => 3, _ => 4 });
    let mail = regex::Regex::new(r"for each discarded (Ace|King|Queen|Jack|10|[2-9])\b").unwrap().captures(&effect("Mail-In Rebate"))
        .map(|m| match &m[1] { "Ace" => 14, "King" => 13, "Queen" => 12, "Jack" => 11, n => n.parse::<u8>().unwrap() });
    simcore::DiscardGoals::from_names(js.iter().filter_map(|c| gs::get(c, "label").as_str()), castle, mail)
}

/// Lab (`JEVLAB_EXP=free-discard`): a regular discard as the round's first, cut to one card for Trading Card.
pub(crate) fn adjust_discard(gs_: &Value, hand: &[Value], cards: Vec<usize>) -> Vec<usize> {
    if !crate::experiment::get().free_discard {
        return cards;
    }
    let infos: Vec<jev_kernel::Card> = hand.iter().map(|c| crate::picker::kernel_card(&card_info(c))).collect();
    let first = gs::num(gs::get(gs::get(gs_, "round"), "discards_used")) == 0.0;
    simcore::first_discard_adjust(cards, &infos, &discard_goals(gs_), first)
}

fn free_discard(gs_: &Value, hand: &[Value], cards: &[usize]) -> Vec<usize> {
    if !crate::experiment::get().free_discard {
        return vec![];
    }
    let rnd = gs::get(gs_, "round");
    if gs::num(gs::get(rnd, "discards_left")) <= 0.0 {
        return vec![];
    }
    let goals = discard_goals(gs_);
    let infos: Vec<jev_kernel::Card> = hand.iter().map(|c| crate::picker::kernel_card(&card_info(c))).collect();
    let played: Vec<jev_kernel::Card> = cards.iter().map(|&p| infos[p]).collect();
    let main = if played.is_empty() { jev_kernel::HAND_NAMES.len() - 1 } else { jev_kernel::classify(&played).0 };
    let first = gs::num(gs::get(rnd, "discards_used")) == 0.0;
    let out = simcore::free_discard(&infos, cards, &[], &goals, first, main);
    if std::env::var_os("FD_DEBUG").is_some() && goals.wants() {
        eprintln!("FD {} first={first} dl={} keep={cards:?} hand={} goals={goals:?} -> {out:?}", gs::get(gs_, "seed"), gs::get(rnd, "discards_left"), hand.len());
    }
    out
}

pub(crate) fn play(bot: &mut JevBot, gs_: &Value, hand: &[Value], cards: &[usize]) -> Result<(), RpcError> {
    let toss = free_discard(gs_, hand, cards);
    if !toss.is_empty() {
        let names: Vec<String> = toss.iter().map(|&p| hands::describe(&hand[p])).collect();
        bot.emit("auto", json!({"question": "move", "choice": "free_discard", "desc": format!("discard {}", names.join(", ")),
            "reason": "lab free-discard: a held joker pays for these; the planned play keeps its cards"}));
        bot.act("discard", Some(&json!({"cards": toss})))?;
        return Ok(());
    }
    remember(bot, gs_, hand, cards);
    bot.act("play", Some(&json!({"cards": cards})))?;
    Ok(())
}
