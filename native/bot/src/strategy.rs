//! Port of src/strategy.py. See docs/rust-port-guide.md.
//!
//! Code-level strategy: things that are always right, so Jev doesn't spend a decision on them,
//! plus joker tags that give Jev the build context community guides say matters.

use crate::gs;
use regex::Regex;
use serde_json::{json, Map, Value};
use std::sync::OnceLock;

/// Compile a regex once per call site (`re.search(r"...")` in the Python).
macro_rules! rx {
    ($p:expr) => {{
        static R: OnceLock<Regex> = OnceLock::new();
        R.get_or_init(|| Regex::new($p).expect("valid regex"))
    }};
}

// strategy.py 11-12
pub const HAND_NAMES: &[&str] = &[
    "Flush Five", "Flush House", "Five of a Kind", "Straight Flush", "Four of a Kind", "Full House",
    "Flush", "Straight", "Three of a Kind", "Two Pair", "Pair", "High Card",
];

// "played hand contains a X" also fires for bigger hands that include X.
// strategy.py 13-23 (sets listed in Python order)
pub const CONTAINED_IN: &[(&str, &[&str])] = &[
    (
        "Pair",
        &["Pair", "Two Pair", "Three of a Kind", "Full House", "Four of a Kind", "Five of a Kind", "Flush House", "Flush Five"],
    ),
    ("Two Pair", &["Two Pair", "Full House", "Flush House"]),
    (
        "Three of a Kind",
        &["Three of a Kind", "Full House", "Four of a Kind", "Five of a Kind", "Flush House", "Flush Five"],
    ),
    ("Straight", &["Straight", "Straight Flush"]),
    ("Flush", &["Flush", "Straight Flush", "Flush House", "Flush Five"]),
    ("Four of a Kind", &["Four of a Kind", "Five of a Kind", "Flush Five"]),
];

// strategy.py 24
pub const COPIERS: &[&str] = &["Blueprint", "Brainstorm"]; // these copy a neighbour, so never reorder around them

/// `CONTAINED_IN` as JSON: each set as a sorted list (sets have no order in Python, so the
/// fixture sorts them too).
pub fn contained_in_json() -> Value {
    let mut m = Map::new();
    for (k, set) in CONTAINED_IN {
        let mut v: Vec<&str> = set.to_vec();
        v.sort();
        m.insert(k.to_string(), json!(v));
    }
    Value::Object(m)
}

/// `effect or ""` as a str. A non-string truthy effect would make Python's `re` raise: panic.
fn eff(v: &Value) -> &str {
    if !gs::truthy(v) {
        return "";
    }
    match v {
        Value::String(s) => s,
        _ => panic!("joker effect is not a string: {v}"),
    }
}

/// Python `str(x)` for the label values f-strings print (None, bool, int, float, str).
fn py_str(v: &Value) -> String {
    match v {
        Value::Null => "None".into(),
        Value::Bool(b) => (if *b { "True" } else { "False" }).to_string(),
        Value::String(s) => s.clone(),
        Value::Number(n) if n.is_f64() => pycompat::pyfloat(n.as_f64().unwrap_or(0.0)),
        Value::Number(n) => n.to_string(),
        _ => panic!("py_str: unsupported value {v}"),
    }
}

// strategy.py 27-40
pub fn joker_kind(effect: &Value) -> &'static str {
    let e = eff(effect);
    if rx!(r"\bX\d").is_match(e) {
        return "xmult";
    }
    if rx!(r"(?i)\bgains\b|permanently|per hand played").is_match(e) && !e.contains('$') {
        return "scaling";
    }
    if rx!(r"\+\d+ Mult|to Mult|Mult for each").is_match(e) {
        return "mult";
    }
    if rx!(r"\+\d+ Chips").is_match(e) {
        return "chips";
    }
    if rx!(r"\$\d|Earn \$|sell value").is_match(e) {
        return "economy";
    }
    "utility"
}

// strategy.py 43-59
pub fn main_hands(gs_: &Value, n: usize) -> Vec<String> {
    // picks entries: (-level, HAND_NAMES index, -played, name)
    let mut picks: Vec<(f64, usize, f64, &str)> = Vec::new();
    for (name, h) in gs::d(gs::get(gs_, "hands")).iter() {
        let one = json!(1);
        let zero = json!(0);
        let level = gs::num(gs::or(gs::get(h, "level"), &one));
        let played = gs::num(gs::or(gs::get(h, "played"), &zero));
        // v15: a level alone no longer makes a main hand (one Jupiter made Flush "main", which then drew more
        // Jupiters while we kept playing pairs); we must actually be playing it.
        let idx = HAND_NAMES.iter().position(|x| *x == name.as_str());
        let idx = match idx {
            Some(i) if played >= 2.0 => i,
            _ => continue,
        };
        if (name == "High Card" || name == "Pair") && level < 2.0 {
            continue;
        }
        picks.push((-level, idx, -played, name.as_str()));
    }
    picks.sort_by(|a, b| {
        a.0.partial_cmp(&b.0)
            .unwrap()
            .then(a.1.cmp(&b.1))
            .then(a.2.partial_cmp(&b.2).unwrap())
            .then(a.3.cmp(b.3))
    });
    picks.iter().take(n).map(|p| p.3.to_string()).collect()
}

// strategy.py 62-73
pub fn engine(jokers_effects: &[Value]) -> Value {
    let kinds: Vec<&str> = jokers_effects.iter().map(joker_kind).collect();
    let mut has = Map::new();
    for k in ["chips", "mult", "xmult", "scaling", "economy", "utility"] {
        let count = kinds.iter().filter(|x| **x == k).count();
        if count > 0 {
            has.insert(k.to_string(), json!(count));
        }
    }
    let mut missing: Vec<&str> = Vec::new();
    if !has.contains_key("mult") && !has.contains_key("scaling") {
        missing.push("a +Mult source for a reliable scoring base");
    }
    if !has.contains_key("chips") {
        missing.push("a chips source");
    }
    if !has.contains_key("xmult") {
        missing.push("xMult that activates with our current jokers and hands");
    }
    let has_v = if has.is_empty() { json!("nothing yet") } else { Value::Object(has) };
    let missing_v = if missing.is_empty() {
        json!("nothing major: upgrade weak or off-plan jokers")
    } else {
        json!(missing)
    };
    json!({"has": has_v, "missing": missing_v})
}

// strategy.py 76-92
pub fn joker_tags(effect: &Value, mains: &[String]) -> Vec<String> {
    let e = eff(effect);
    let kind = joker_kind(effect);
    let first = match kind {
        "xmult" => "xMult",
        "scaling" => "scales over the run",
        "mult" => "+Mult",
        "chips" => "+Chips",
        "economy" => "economy",
        _ => "utility",
    };
    let mut tags: Vec<String> = vec![first.to_string()];
    if kind == "xmult" && rx!(r"\bgains\b").is_match(e) {
        tags[0] = "xMult that scales over the run".to_string();
    }
    if e.contains("without playing your most played poker hand") && !mains.is_empty() {
        tags.push(format!("resets whenever we play our most played hand (likely {})", mains[0]));
    }
    if rx!(r"(?i)destroy (a random )?Joker|destroy Joker|this card is destroyed|is destroyed").is_match(e)
        && !e.contains("1 in 1000")
    {
        // Cavendish: negligible risk
        tags.push("risky: destroys jokers or itself".to_string());
    }
    static CONTAINS: OnceLock<Regex> = OnceLock::new();
    let contains = CONTAINS.get_or_init(|| {
        Regex::new(&format!("contains an? ({})", HAND_NAMES.join("|"))).expect("valid regex")
    });
    if let Some(m) = contains.captures(e) {
        let hand = m.get(1).map_or("", |g| g.as_str());
        if !mains.is_empty() {
            let set: Vec<&str> = match CONTAINED_IN.iter().find(|(k, _)| *k == hand) {
                Some((_, s)) => s.to_vec(),
                None => vec![hand],
            };
            let hit: Vec<&str> = mains
                .iter()
                .map(|h| h.as_str())
                .filter(|h| set.contains(h))
                .collect();
            if !hit.is_empty() {
                let plural = if hit.len() > 1 { "s" } else { "" };
                tags.push(format!("boosts {}, our main hand{}", hit.join(" and "), plural));
            } else {
                tags.push(format!("only helps hands containing a {hand}, which we rarely play"));
            }
        }
    }
    tags
}

// strategy.py 95-96. BalatroBot money is always an int, so the int path is the one used.
pub fn interest(money: &Value) -> i64 {
    let m = if gs::truthy(money) { gs::int(money) } else { 0 };
    m.div_euclid(5).clamp(0, 5)
}

// strategy.py 99-101
pub fn _cards<'a>(gs_: &'a Value, area: &str) -> &'a [Value] {
    gs::cards_of(gs_, area)
}

/// Lab (`dagger-last`): `order` with Ceremonial Dagger moved to the end, so no joker sits to its right (it destroys
/// that one each time a blind is selected).
pub fn dagger_last(labels: &[&str], order: Vec<usize>) -> Vec<usize> {
    let (mut rest, daggers): (Vec<usize>, Vec<usize>) = order.into_iter().partition(|&i| labels.get(i) != Some(&"Ceremonial Dagger"));
    rest.extend(daggers);
    rest
}

// strategy.py 104-119
pub fn auto_action(gs_: &Value) -> Option<(String, Value, String)> {
    let state = gs::get(gs_, "state").as_str();
    if matches!(state, Some("SHOP") | Some("BLIND_SELECT") | Some("SELECTING_HAND")) {
        for (i, c) in _cards(gs_, "consumables").iter().enumerate() {
            if gs::get(c, "set").as_str() == Some("PLANET") {
                return Some((
                    "use".to_string(),
                    json!({"consumable": i}),
                    format!("auto-use planet {}", py_str(gs::get(c, "label"))),
                ));
            }
        }
    }
    if matches!(state, Some("SHOP") | Some("SELECTING_HAND")) {
        let jokers = _cards(gs_, "jokers");
        if !crate::experiment::get().no_order_search {
            // v57: in the shop, the joker order with the best sampled score (editions apply at their joker's place,
            // per-card effects run in joker order, copiers copy by place); elsewhere keep the order
            if gs::get(gs_, "state").as_str() != Some("SHOP") {
                return None;
            }
            let found = crate::readiness::best_joker_order(gs_, 12);
            if crate::experiment::get().dagger_last {
                let labels: Vec<&str> = jokers.iter().map(|j| gs::get(j, "label").as_str().unwrap_or("")).collect();
                let order = dagger_last(&labels, found.unwrap_or_else(|| (0..jokers.len()).collect()));
                if order == (0..jokers.len()).collect::<Vec<usize>>() {
                    return None;
                }
                return Some(("rearrange".to_string(), json!({"jokers": order}), "joker order with the best sampled best-play score, Ceremonial Dagger last".to_string()));
            }
            return found.map(|order| {
                ("rearrange".to_string(), json!({"jokers": order}), "joker order with the best sampled best-play score".to_string())
            });
        }
        // lab (`no-order-search`): the v56 rule, xMult jokers to the right and nothing around a copier
        if jokers.iter().any(|j| COPIERS.contains(&gs::get(j, "label").as_str().unwrap_or("\u{0}"))) {
            return None;
        }
        let kinds: Vec<bool> = jokers
            .iter()
            .map(|j| joker_kind(gs::get(gs::get(j, "value"), "effect")) == "xmult")
            .collect();
        let mut order: Vec<usize> = (0..kinds.len()).filter(|&i| !kinds[i]).collect();
        order.extend((0..kinds.len()).filter(|&i| kinds[i]));
        if order != (0..jokers.len()).collect::<Vec<usize>>() {
            return Some((
                "rearrange".to_string(),
                json!({"jokers": order}),
                "move xMult jokers to the right so they apply last".to_string(),
            ));
        }
    }
    None
}
