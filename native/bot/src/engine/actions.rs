//! BalatroBot methods -> rumbot actions, with Jackdaw's error behaviour
//! (`bridge/backend.py` SimBackend.handle, `bridge/deserializer.py` rpc_to_action, the
//! `IllegalActionError`s of `engine/game.py`, `sim_backend.py` `_with_forced` / `_rearrange`).

use super::{rpc_error, RumbotGame};
use crate::iface::RpcError;
use rumbot_sim::game::{Action, ActionType, GameState, Phase};
use serde_json::Value;
use std::rc::Rc;

const BAD_REQUEST: i64 = -32001;
const INVALID_STATE: i64 = -32002;
const NOT_ALLOWED: i64 = -32003;

fn illegal(msg: impl Into<String>) -> RpcError {
    rpc_error(NOT_ALLOWED, msg)
}

fn bad(msg: impl Into<String>) -> RpcError {
    rpc_error(BAD_REQUEST, msg)
}

/// Jackdaw's phase names in `Action not valid in phase ...` (best guess at its repr).
fn phase_name(p: Phase) -> &'static str {
    match p {
        Phase::BlindSelect => "blind_select",
        Phase::Playing => "selecting_hand",
        Phase::RoundEval => "round_eval",
        Phase::Shop => "shop",
        Phase::Pack => "pack_opening",
        Phase::GameOver => "game_over",
        Phase::Won => "round_eval",
    }
}

/// `SimBalatro` state names, for `_rearrange`'s INVALID_STATE messages.
fn state_name(p: Phase) -> &'static str {
    match p {
        Phase::BlindSelect => "BLIND_SELECT",
        Phase::Playing => "SELECTING_HAND",
        Phase::RoundEval | Phase::Won => "ROUND_EVAL",
        Phase::Shop => "SHOP",
        Phase::Pack => "SMODS_BOOSTER_OPENED",
        Phase::GameOver => "GAME_OVER",
    }
}

const SEED_CHARS: &[u8] = b"123456789ABCDEFGHIJKLMNPQRSTUVWXYZ";

// sim_backend.py random_seed
fn random_seed() -> String {
    let mut x = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos() as u64)
        .unwrap_or(1)
        | 1;
    (0..8)
        .map(|_| {
            x ^= x << 13;
            x ^= x >> 7;
            x ^= x << 17;
            SEED_CHARS[(x % SEED_CHARS.len() as u64) as usize] as char
        })
        .collect()
}

fn deck_name(s: &str) -> &'static str {
    // backend.py DECK_FROM_BOT (unknown decks fall back to Red)
    match s {
        "BLUE" => "Blue Deck",
        "YELLOW" => "Yellow Deck",
        "GREEN" => "Green Deck",
        "BLACK" => "Black Deck",
        "MAGIC" => "Magic Deck",
        "NEBULA" => "Nebula Deck",
        "GHOST" => "Ghost Deck",
        "ABANDONED" => "Abandoned Deck",
        "CHECKERED" => "Checkered Deck",
        "ZODIAC" => "Zodiac Deck",
        "PAINTED" => "Painted Deck",
        "ANAGLYPH" => "Anaglyph Deck",
        "PLASMA" => "Plasma Deck",
        "ERRATIC" => "Erratic Deck",
        _ => "Red Deck",
    }
}

fn stake_num(s: &str) -> i32 {
    match s {
        "RED" => 2,
        "GREEN" => 3,
        "BLACK" => 4,
        "BLUE" => 5,
        "PURPLE" => 6,
        "ORANGE" => 7,
        "GOLD" => 8,
        _ => 1,
    }
}

// SimBalatro.call "start"
pub fn start(eng: &mut RumbotGame, params: &Value) -> Result<(), RpcError> {
    let seed = match params.get("seed") {
        Some(Value::String(s)) => s.clone(),
        Some(v) if !v.is_null() => v.to_string(),
        _ => random_seed(),
    };
    let deck = deck_name(params.get("deck").and_then(Value::as_str).unwrap_or("RED"));
    let stake = stake_num(params.get("stake").and_then(Value::as_str).unwrap_or("WHITE"));
    let g = GameState::new(seed, deck, stake);
    eng.deck_limit = g.full_deck.len();
    eng.g = Some(g);
    eng.round_hands_played = 0;
    eng.round_started = false;
    eng.played_this_round.clear();
    eng.pack_from_shop = false;
    eng.shop_frozen = None;
    Ok(())
}

fn idx_param(params: &Value, key: &str) -> Result<i64, RpcError> {
    match params.get(key) {
        Some(Value::Number(n)) if n.is_i64() => Ok(n.as_i64().unwrap()),
        Some(v) => Err(bad(format!("{key}: expected an integer, got {v}"))),
        None => Err(bad(format!("missing '{key}'"))),
    }
}

fn index_list(v: Option<&Value>) -> Result<Vec<i64>, RpcError> {
    match v {
        None | Some(Value::Null) => Ok(Vec::new()),
        Some(Value::Array(a)) => a
            .iter()
            .map(|x| x.as_i64().ok_or_else(|| bad(format!("bad card index {x}"))))
            .collect(),
        Some(x) => Err(bad(format!("bad card list {x}"))),
    }
}

fn phase_check(g: &GameState, ok: &[Phase]) -> Result<(), RpcError> {
    if ok.contains(&g.phase) {
        Ok(())
    } else {
        let names: Vec<_> = ok.iter().map(|p| phase_name(*p)).collect();
        Err(illegal(format!(
            "Action not valid in phase '{}' (expected {})",
            phase_name(g.phase),
            names.join(" or ")
        )))
    }
}

// sim_backend.py _with_forced: Cerulean Bell's card is added to the selection (up to 5 cards).
fn with_forced(g: &GameState, cards: Vec<i64>) -> Vec<i64> {
    let forced = match g.held_forced_card() {
        Some(c) => g.hand.iter().position(|h| Rc::ptr_eq(h, &c)),
        None => None,
    };
    let forced = match forced {
        Some(f) => f as i64,
        None => return cards,
    };
    if cards.contains(&forced) || cards.iter().any(|&i| i < 0 || i as usize >= g.hand.len()) {
        return cards;
    }
    let mut v = vec![forced];
    v.extend(cards);
    v.truncate(5);
    v.sort();
    v
}

fn truthy(v: Option<&Value>) -> bool {
    match v {
        None | Some(Value::Null) => false,
        Some(Value::Bool(b)) => *b,
        Some(Value::Number(n)) => n.as_f64() != Some(0.0),
        Some(Value::String(s)) => !s.is_empty(),
        Some(Value::Array(a)) => !a.is_empty(),
        Some(Value::Object(o)) => !o.is_empty(),
    }
}

// bridge/backend.py _handle_action + deserializer.rpc_to_action + game.py step
fn d_size_total(g: &GameState) -> i32 {
    g.jokers.iter().map(|j| match j.borrow().spec.name { "Drunkard" => 1, "Merry Andy" => 3, _ => 0 }).sum()
}

// Jackdaw card.py:762-775/835-844 (card.lua:591): add_to_deck/remove_from_deck ease the CURRENT round's discards_left by d_size;
// rumbot only changes it at blind start, so mirror the delta into the stale-round snapshot outside Playing.
pub fn act(eng: &mut RumbotGame, method: &str, params: &Value) -> Result<(), RpcError> {
    let before = eng.g.as_ref().map(|g| (g.phase, d_size_total(g)));
    let r = act_inner(eng, method, params);
    if let (Some((ph, d0)), Some(g)) = (before, eng.g.as_ref()) {
        if r.is_ok() && ph != Phase::Playing && g.phase != Phase::Playing {
            let d1 = d_size_total(g);
            if d1 != d0 {
                eng.last_round.1 = (eng.last_round.1 + d1 - d0).max(0);
            }
        }
    }
    r
}

fn act_inner(eng: &mut RumbotGame, method: &str, params: &Value) -> Result<(), RpcError> {
    if method == "rearrange" {
        return rearrange(eng, params);
    }
    let g = eng.g.as_mut().unwrap();
    match method {
        "select" => {
            phase_check(g, &[Phase::BlindSelect])?;
            g.step(&Action::new(ActionType::SelectBlind));
            eng.round_started = true;
            eng.last_chips = 0;
            eng.last_round = (g.hands_left, g.discards_left, g.discards_used);
            eng.round_hands_played = 0;
            eng.played_this_round.clear();
        }
        "skip" => {
            phase_check(g, &[Phase::BlindSelect])?;
            if !g.is_legal(&Action::new(ActionType::SkipBlind)) {
                return Err(illegal("Cannot skip Boss blind"));
            }
            g.step(&Action::new(ActionType::SkipBlind));
        }
        "play" | "discard" => {
            let play = method == "play";
            phase_check(g, &[Phase::Playing])?;
            let mut cards = index_list(params.get("cards"))?;
            if play && g.hands_left <= 0 {
                return Err(illegal("No hands remaining"));
            }
            if !play && g.discards_left <= 0 {
                return Err(illegal("No discards remaining"));
            }
            if cards.is_empty() {
                return Err(illegal("Must select at least 1 card"));
            }
            cards = with_forced(g, cards);
            if cards.len() > 5 {
                return Err(illegal(format!("Cannot {method} more than 5 cards")));
            }
            let n = g.hand.len() as i64;
            if cards.iter().any(|&i| i < 0 || i >= n) {
                return Err(illegal("Card index out of range"));
            }
            let mut idx: Vec<usize> = cards.iter().map(|&i| i as usize).collect();
            if !play {
                idx.sort_unstable(); // rumbot wants discards ascending; order is irrelevant to the discard
            }
            let a = Action::with_cards(if play { ActionType::Play } else { ActionType::Discard }, idx.clone());
            if !g.is_legal(&a) {
                return Err(illegal("Forced card (Cerulean Bell) must be in the selection"));
            }
            if play {
                // played_this_round counts: rumbot only keeps a set
                let chosen: Vec<_> = idx.iter().map(|&i| g.hand[i].clone()).collect();
                let hand = g.evaluate_selection(&chosen).hand;
                *eng.played_this_round.entry(hand).or_insert(0) += 1;
                eng.round_hands_played += 1;
            }
            let (hl, dl, du) = (g.hands_left, g.discards_left, g.discards_used);
            g.step(&a);
            eng.last_chips = g.chips_scored;
            eng.last_round = if g.phase == Phase::Playing {
                (g.hands_left, g.discards_left, g.discards_used)
            } else if play {
                (hl - 1, dl, du)
            } else {
                (hl, dl - 1, du + 1)
            };
        }
        "buy" => {
            if let Some(i) = params.get("card") {
                let i = i.as_i64().ok_or_else(|| bad("buy: bad card index"))?;
                phase_check(g, &[Phase::Shop])?;
                let n = g.shop.as_ref().map_or(0, |s| s.slots.len()) as i64;
                if i < 0 || i >= n {
                    return Err(illegal(format!("Invalid shop index {i}")));
                }
                let a = Action::at(ActionType::Buy, i as i32);
                if !g.is_legal(&a) {
                    let slot = &g.shop.as_ref().unwrap().slots[i as usize];
                    if !g.affords(g.slot_price(slot)) {
                        return Err(illegal("Cannot afford card"));
                    }
                    return Err(illegal(if slot.kind == "joker" { "No room for joker" } else { "No room for consumable" }));
                }
                g.step(&a);
            } else if let Some(i) = params.get("voucher") {
                let i = i.as_i64().ok_or_else(|| bad("buy: bad voucher index"))?;
                phase_check(g, &[Phase::Shop])?;
                let n = g.shop.as_ref().map_or(0, |s| s.vouchers_on_offer().len()) as i64;
                if i < 0 || i >= n {
                    return Err(illegal(format!("Invalid voucher index {i}")));
                }
                let a = Action::at(ActionType::BuyVoucher, i as i32);
                if !g.is_legal(&a) {
                    return Err(illegal("Cannot afford voucher"));
                }
                g.step(&a);
            } else if let Some(i) = params.get("pack") {
                let i = i.as_i64().ok_or_else(|| bad("buy: bad pack index"))?;
                if i == 0 && eng.first_shop == 1 { eng.first_pack_gone = true; }
                phase_check(g, &[Phase::Shop])?;
                let n = g.shop.as_ref().map_or(0, |s| s.packs.len()) as i64;
                if i < 0 || i >= n {
                    return Err(illegal(format!("Invalid booster index {i}")));
                }
                let a = Action::at(ActionType::BuyPack, i as i32);
                if !g.is_legal(&a) {
                    return Err(illegal("Cannot afford booster"));
                }
                g.step(&a);
                eng.pack_from_shop = true;
            } else {
                return Err(bad(format!("buy: unrecognized params {params}")));
            }
        }
        "sell" => {
            let (area, i, ty) = if params.get("joker").is_some() {
                ("jokers", idx_param(params, "joker")?, ActionType::SellJoker)
            } else if params.get("consumable").is_some() {
                ("consumables", idx_param(params, "consumable")?, ActionType::SellConsumable)
            } else {
                return Err(bad(format!("sell: unrecognized params {params}")));
            };
            if !matches!(g.phase, Phase::Shop | Phase::Playing | Phase::RoundEval | Phase::BlindSelect | Phase::Pack) {
                return Err(illegal(format!("Cannot sell in phase {}", phase_name(g.phase))));
            }
            let n = if area == "jokers" { g.jokers.len() } else { g.consumables.len() } as i64;
            if i < 0 || i >= n {
                return Err(illegal(format!("Invalid {area} index {i}")));
            }
            if area == "jokers" && g.jokers[i as usize].borrow().eternal {
                return Err(illegal("Cannot sell eternal card"));
            }
            let a = Action::at(ty, i as i32);
            if !g.is_legal(&a) {
                return Err(illegal(format!("Cannot sell in phase {}", phase_name(g.phase))));
            }
            g.step(&a);
        }
        "use" => {
            if params.get("consumable").is_none() {
                return Err(bad(format!("use: missing 'consumable' in params {params}")));
            }
            let i = idx_param(params, "consumable")?;
            let cards = index_list(params.get("cards"))?;
            if i < 0 || i as usize >= g.consumables.len() {
                return Err(illegal(format!("Invalid consumable index {i}")));
            }
            let n = g.hand.len() as i64;
            let cannot = |g: &GameState| {
                illegal(format!(
                    "Consumable {:?} cannot be used at this time",
                    g.consumables[i as usize].borrow().spec.name
                ))
            };
            if cards.iter().any(|&c| c < 0 || c >= n) {
                return Err(cannot(g));
            }
            let a = Action {
                r#type: ActionType::UseConsumable,
                index: i as i32,
                cards: { let mut v: Vec<usize> = cards.iter().map(|&c| c as usize).collect(); v.sort_unstable(); v }, // Jackdaw targets are hand positions, order-free
            };
            if !g.is_legal(&a) {
                return Err(cannot(g));
            }
            g.step(&a);
        }
        "reroll" => {
            phase_check(g, &[Phase::Shop])?;
            let a = Action::new(ActionType::Reroll);
            if !g.is_legal(&a) {
                return Err(illegal("Cannot afford reroll"));
            }
            g.step(&a);
        }
        "next_round" => {
            phase_check(g, &[Phase::Shop])?;
            g.step(&Action::new(ActionType::LeaveShop));
            eng.pack_from_shop = false;
        }
        "cash_out" => {
            phase_check(g, &[Phase::RoundEval])?;
            g.step(&Action::new(ActionType::CashOut));
        }
        "pack" => {
            if truthy(params.get("skip")) {
                phase_check(g, &[Phase::Pack])?;
                g.step(&Action::new(ActionType::SkipPack));
            } else if params.get("card").is_some() {
                let i = idx_param(params, "card")?;
                let targets = index_list(params.get("targets"))?;
                phase_check(g, &[Phase::Pack])?;
                if g.pack_options.is_empty() {
                    return Err(illegal("No pack choices remaining"));
                }
                if i < 0 || i as usize >= g.pack_options.len() {
                    return Err(illegal(format!("Invalid pack card index {i}")));
                }
                let a = Action {
                    r#type: ActionType::PickPack,
                    index: i as i32,
                    cards: { let mut v: Vec<usize> = targets.iter().map(|&c| c.max(0) as usize).collect(); v.sort_unstable(); v },
                };
                if targets.iter().any(|&c| c < 0) || !g.is_legal(&a) {
                    return Err(illegal("Cannot pick that pack card (no room, or wrong targets)"));
                }
                g.step(&a);
            } else {
                return Err(bad(format!("pack: unrecognized params {params}")));
            }
        }
        // Jackdaw's "add"/"set" debug methods are not something the bot sends
        _ => return Err(bad(format!("Unknown method: {method:?}"))),
    }
    Ok(())
}

// sim_backend.py _rearrange (BalatroBot endpoints/rearrange.lua)
fn rearrange(eng: &mut RumbotGame, params: &Value) -> Result<(), RpcError> {
    let g = eng.g.as_mut().unwrap();
    let keys = ["hand", "jokers", "consumables"];
    let n = keys.iter().filter(|k| params.get(**k).is_some()).count();
    if n != 1 {
        return Err(bad(if n == 0 {
            "Must provide exactly one of: hand, jokers, or consumables"
        } else {
            "Can only rearrange one type at a time"
        }));
    }
    let phase = g.phase;
    if !matches!(phase, Phase::Playing | Phase::Shop | Phase::Pack) {
        return Err(rpc_error(
            INVALID_STATE,
            format!("Method 'rearrange' not allowed in state {}", state_name(phase)),
        ));
    }
    let k = *keys.iter().find(|k| params.get(**k).is_some()).unwrap();
    if k == "hand" {
        if !matches!(phase, Phase::Playing | Phase::Pack) {
            return Err(rpc_error(INVALID_STATE, "Can only rearrange hand during hand selection"));
        }
        if phase == Phase::Pack && g.hand.is_empty() {
            return Err(illegal(
                "No cards to rearrange. You can only rearrange hand in Arcana and Spectral packs.",
            ));
        }
    }
    let len = match k {
        "hand" => g.hand.len(),
        "jokers" => g.jokers.len(),
        _ => g.consumables.len(),
    };
    let perm: Vec<Value> = params.get(k).and_then(Value::as_array).cloned().unwrap_or_default();
    if perm.len() != len {
        return Err(bad(format!("Must provide exactly {len} indices for {k}")));
    }
    let mut seen = std::collections::HashSet::new();
    let mut p = Vec::new();
    for v in &perm {
        let i = match v.as_i64() {
            Some(i) if i >= 0 && (i as usize) < len => i as usize,
            _ => return Err(bad(format!("Index out of range for {k}: {}", pyrepr(v)))),
        };
        if !seen.insert(i) {
            return Err(bad(format!("Duplicate index in {k}: {i}")));
        }
        p.push(i);
    }
    match k {
        "hand" => g.hand = p.iter().map(|&i| g.hand[i].clone()).collect(),
        "jokers" => g.jokers = p.iter().map(|&i| g.jokers[i].clone()).collect(),
        _ => g.consumables = p.iter().map(|&i| g.consumables[i].clone()).collect(),
    }
    Ok(())
}

fn pyrepr(v: &Value) -> String {
    match v {
        Value::String(s) => format!("'{s}'"),
        Value::Bool(b) => (if *b { "True" } else { "False" }).into(),
        Value::Null => "None".into(),
        o => o.to_string(),
    }
}
