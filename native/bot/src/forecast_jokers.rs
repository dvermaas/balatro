//! The joker part of the forecast description (port of rollout.joker_card + _joker_dynamic, then
//! sim_bridge._joker + sim_bridge_jokers.joker_fields, composed per joker key: the running state is
//! read from the effect text straight into rumbot's counter / secondary / hands_at_create / named_hand).
//! Checked by tests/fc_jokers.rs against scripts/port/fixtures/fc_jokers.py.

use regex::Regex;
use rumbot_sim::jokers as rjokers;
use serde_json::{json, Map, Value};
use simcore::JokerDesc;
use std::cell::RefCell;
use std::collections::HashMap;

use crate::engine::view::center_name;

const NUM: &str = r"(\d+(?:\.\d+)?)";
const EDITIONS: [&str; 4] = ["foil", "holo", "polychrome", "negative"];
const HANDS: [&str; 12] = [
    "High Card", "Pair", "Two Pair", "Three of a Kind", "Straight", "Flush", "Full House", "Four of a Kind",
    "Straight Flush", "Five of a Kind", "Flush House", "Flush Five",
];

thread_local! {
    static RE: RefCell<HashMap<String, Regex>> = RefCell::new(HashMap::new());
    static NAMES: RefCell<HashMap<String, String>> = RefCell::new(HashMap::new());
}

/// `re.search(pattern, text)[1]`
fn cap(pattern: &str, text: &str) -> Option<String> {
    RE.with(|c| {
        let mut c = c.borrow_mut();
        let re = c.entry(pattern.to_string()).or_insert_with(|| Regex::new(pattern).expect("regex"));
        re.captures(text).and_then(|m| m.get(1)).map(|m| m.as_str().to_string())
    })
}

/// `_num(pattern, text, default)` with cast float
fn numf(pattern: &str, text: &str, default: f64) -> f64 {
    cap(pattern, text).map_or(default, |s| s.parse().expect("float"))
}

/// `_num(..., cast=int)`
fn numi(pattern: &str, text: &str, default: i64) -> i64 {
    cap(pattern, text).map_or(default, |s| s.parse().expect("int"))
}

/// hands._d: BalatroBot's empty table `[]` reads as {}
fn d(v: &Value) -> Value {
    if v.is_object() {
        v.clone()
    } else {
        json!({})
    }
}

fn truthy(v: &Value) -> bool {
    match v {
        Value::Null => false,
        Value::Bool(b) => *b,
        Value::Number(n) => n.as_f64().map_or(true, |f| f != 0.0),
        Value::String(s) => !s.is_empty(),
        Value::Array(a) => !a.is_empty(),
        Value::Object(o) => !o.is_empty(),
    }
}

fn py_int(v: &Value) -> i64 {
    match v {
        Value::Bool(b) => *b as i64,
        Value::Number(n) => n.as_i64().unwrap_or_else(|| n.as_f64().unwrap_or(0.0) as i64),
        Value::String(s) => s.trim().parse().unwrap_or(0),
        _ => 0,
    }
}

fn norm(x: &str) -> String {
    x.chars()
        .map(|c| match c {
            'é' | 'è' | 'ê' => 'e',
            'á' | 'à' => 'a',
            'í' => 'i',
            'ó' | 'ö' => 'o',
            'ú' | 'ü' => 'u',
            c => c,
        })
        .filter(|c| c.is_alphanumeric())
        .flat_map(|c| c.to_lowercase())
        .collect()
}

/// sim_bridge._name for a joker key (rumbot's registered name).
fn rumbot_name(key: &str) -> String {
    if key == "j_caino" {
        return "Canio".into();
    }
    NAMES.with(|n| {
        let mut n = n.borrow_mut();
        if n.is_empty() {
            for s in rjokers::all_specs() {
                n.insert(norm(s.name), s.name.to_string());
            }
        }
        let cn = center_name(key);
        n.get(&norm(&cn)).cloned().unwrap_or_else(|| panic!("no rumbot-sim name for {key}"))
    })
}

type Running = (Option<f64>, Option<f64>, Option<i64>, Option<String>);

/// The running state of one joker key: (counter, secondary, hands_at_create, named_hand).
fn running(key: &str, text: &str, hands_played: i64) -> Running {
    let cur_mult = format!(r"Currently \+{NUM} Mult");
    let cur_x = format!(r"Currently X{NUM} Mult");
    match key {
        "j_green_joker" | "j_ceremonial" | "j_trousers" | "j_ride_the_bus" | "j_red_card" | "j_flash" => {
            (Some(numf(&cur_mult, text, 0.0)), None, None, None)
        }
        "j_popcorn" => (Some(numf(&format!(r"^\+{NUM} Mult"), text, 20.0)), None, None, None),
        "j_runner" | "j_wee" | "j_castle" | "j_square" => {
            (Some(numf(&format!(r"Currently \+?{NUM} Chips"), text, 0.0)), None, None, None)
        }
        "j_ice_cream" => (Some(numf(&format!(r"^\+{NUM} Chips"), text, 100.0)), None, None, None),
        "j_constellation" | "j_glass" | "j_hit_the_road" | "j_madness" | "j_vampire" | "j_hologram" | "j_obelisk"
        | "j_lucky_cat" | "j_campfire" | "j_caino" => (Some(numf(&cur_x, text, 1.0)), None, None, None),
        "j_yorick" => {
            let x = numf(&cur_x, text, 1.0);
            let disc = numi(r"\[(\d+)\]", text, 23);
            (Some(x), Some(disc as f64), None, None)
        }
        "j_ramen" => (Some(numf(&format!(r"^X{NUM} Mult"), text, 2.0)), None, None, None),
        "j_loyalty_card" => {
            let every = 5;
            let rem = if text.contains("Active") { 0 } else { numi(r"(\d+) remaining", text, every) };
            (None, None, Some(hands_played - (every - rem)), None)
        }
        "j_invisible" => (Some(numi(r"Currently (\d+)/", text, 0) as f64), None, None, None),
        "j_selzer" => (Some(numi(r"next (\d+) hands", text, 10) as f64), None, None, None),
        "j_turtle_bean" => (Some(numi(r"^\+(\d+) hand size", text, 5) as f64), None, None, None),
        "j_rocket" => (Some(numi(r"Earn \$(\d+) at end", text, 1) as f64), None, None, None),
        "j_todo_list" => {
            let h = cap(r"poker hand is an? (.+?), poker hand", text).filter(|h| HANDS.contains(&h.as_str()));
            (None, None, None, Some(h.unwrap_or_else(|| "High Card".to_string())))
        }
        _ => (None, None, None, None),
    }
}

/// One joker card of a public gamestate -> the description dict, field order as sim_bridge._joker.
/// `sort_id` is the engine's global counter in Python (history dependent), so the caller supplies it.
pub fn joker_value(src: &Value, hands_played: i64, sort_id: u64) -> Value {
    let key = src.get("key").and_then(Value::as_str).unwrap_or("");
    let m = d(src.get("modifier").unwrap_or(&Value::Null));
    let st = d(src.get("state").unwrap_or(&Value::Null));
    let cost = d(src.get("cost").unwrap_or(&Value::Null));
    let buy = cost.get("buy").map_or(0, py_int);
    let sell = cost.get("sell").map_or(0, py_int);
    let edition = m
        .get("edition")
        .and_then(Value::as_str)
        .map(str::to_lowercase)
        .filter(|e| EDITIONS.contains(&e.as_str()));
    let perishable = m.get("perishable").filter(|v| truthy(v)).map(py_int);
    let value = d(src.get("value").unwrap_or(&Value::Null));
    let text = value.get("effect").and_then(Value::as_str).unwrap_or("");
    let extra_sell = (sell - 1.max(buy.div_euclid(2))).max(0);
    let (counter, secondary, hands_at_create, named_hand) = running(key, text, hands_played);

    let mut o = Map::new();
    o.insert("sort_id".into(), json!(sort_id));
    o.insert("name".into(), json!(rumbot_name(key)));
    o.insert("edition".into(), json!(edition));
    o.insert("eternal".into(), json!(m.get("eternal").map_or(false, truthy)));
    o.insert("perishable".into(), json!(perishable.is_some()));
    // Card.perish_tally defaults to 5 (perishable: the pub value)
    o.insert("perish_tally".into(), json!(perishable.unwrap_or(5)));
    o.insert("rental".into(), json!(m.get("rental").map_or(false, truthy)));
    o.insert("debuffed".into(), json!(st.get("debuff").map_or(false, truthy)));
    o.insert("extra_sell_value".into(), json!(extra_sell as f64));
    if let Some(c) = counter {
        o.insert("counter".into(), json!(c));
    }
    if let Some(s) = secondary {
        o.insert("secondary".into(), json!(s));
    }
    if let Some(h) = hands_at_create {
        o.insert("hands_at_create".into(), json!(h));
    }
    if let Some(n) = named_hand {
        o.insert("named_hand".into(), json!(n));
    }
    Value::Object(o)
}

pub fn hands_played(pubv: &Value) -> i64 {
    d(pubv.get("hands").unwrap_or(&Value::Null))
        .as_object()
        .map_or(0, |h| h.values().map(|x| d(x).get("played").filter(|v| truthy(v)).map_or(0, py_int)).sum())
}

/// The joker dicts of `joker_descs_from_pub`, with sort ids first_sort_id, first_sort_id+1, ...
pub fn joker_values_from_pub(pubv: &Value, first_sort_id: u64) -> Vec<Value> {
    let hp = hands_played(pubv);
    let jokers = d(pubv.get("jokers").unwrap_or(&Value::Null));
    let cards: Vec<Value> = jokers.get("cards").and_then(Value::as_array).cloned().unwrap_or_default();
    cards.iter().enumerate().map(|(i, c)| joker_value(c, hp, first_sort_id + i as u64)).collect()
}

/// The jokers of the forecast description (sort ids 1..n; the caller renumbers if it needs the engine's).
pub fn joker_descs_from_pub(pubv: &Value) -> Vec<JokerDesc> {
    joker_values_from_pub(pubv, 1).into_iter().map(|v| serde_json::from_value(v).expect("JokerDesc")).collect()
}
