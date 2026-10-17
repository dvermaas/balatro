//! Port of src/jokers.py (Scorer via jev_kernel; the encoding in src/native.py). See docs/rust-port-guide.md.
//!
//! Joker-aware hand scoring, parsed from each joker's live effect text. Ported here: the constants,
//! parse/entry/build/unmodelled and score/_hand/Scorer (jokers.py 1-221, 286-392).
//! Python's rule dicts and tuples become the enums below. Numbers inside effect tuples are f64 (Python
//! mixes ints and floats there, but they are only ever added or multiplied into float totals).

use crate::gs;
use crate::hands::{self, Info, Levels};
use crate::strategy::{CONTAINED_IN, HAND_NAMES};
use regex::Regex;
use serde_json::{json, Value};
use std::cell::OnceCell;
use std::sync::OnceLock;

// Python `$` also matches before a final newline: end!() does the same.
macro_rules! num { () => { r"(\d+(?:\.\d+)?)" }; }
macro_rules! end { () => { r"\n?\z" }; }
macro_rules! rx {
    ($p:expr) => {{
        static R: OnceLock<Regex> = OnceLock::new();
        R.get_or_init(|| Regex::new($p).unwrap())
    }};
}

const FACES: &str = "JQK";
const WORD_RANK: &[(&str, char)] = &[
    ("Ace", 'A'), ("King", 'K'), ("Queen", 'Q'), ("Jack", 'J'), ("10", 'T'),
    ("2", '2'), ("3", '3'), ("4", '4'), ("5", '5'), ("6", '6'), ("7", '7'), ("8", '8'), ("9", '9'),
];
const WORD_SUIT: &[(&str, char)] = &[("Diamond", 'D'), ("Heart", 'H'), ("Club", 'C'), ("Spade", 'S')];

fn word_rank(w: &str) -> Option<char> {
    WORD_RANK.iter().find(|(k, _)| *k == w).map(|(_, v)| *v)
}
fn word_suit(w: &str) -> Option<char> {
    WORD_SUIT.iter().find(|(k, _)| *k == w).map(|(_, v)| *v)
}
fn is_ed(c: &Info, e: &str) -> bool {
    c.edition.as_deref() == Some(e)
}

/// Who a per-card effect applies to (jokers.py `_card_pred` tuples and the strings "first_face", "first", "final").
#[derive(Clone, Debug, PartialEq)]
pub enum Pred {
    FirstFace,
    First,
    Final,
    Suit(char),
    Ranks(Vec<char>), // sorted, distinct
    All,
}

#[derive(Clone, Debug, PartialEq)]
pub enum Cond {
    Names(Vec<String>),
    Le3,
    Final,
    NoDiscards,
    HeldSuits(Vec<Option<char>>), // distinct (a Python set); None = a suit word we don't know
    FourSuits,
    PlayedThisRound,
}

#[derive(Clone, Debug, PartialEq)]
pub enum HandSpec {
    Const((f64, f64, f64)),
    Cond { cond: Cond, yes: (f64, f64, f64), no: (f64, f64, f64) },
    TimesPlayed,
    PerDiscard(f64),
    LowestHeld,
}

#[derive(Clone, Debug, PartialEq, Default)]
pub struct Rule {
    pub card: Vec<(Pred, f64, f64, f64)>,
    pub retrigger: Vec<(Pred, i64)>,
    pub held: Vec<(Pred, f64, f64)>,
    pub held_retrigger: Option<i64>,
    pub hand: Option<HandSpec>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct JokerEntry {
    pub label: Option<String>,
    pub rule: Option<Rule>,
    pub edition: Option<String>,
    pub scoring: bool,
}

/// The round dict: hands_left `or 1`, discards_left `or 0`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Rnd {
    pub hands_left: f64,
    pub discards_left: f64,
}

impl Rnd {
    pub fn from_value(rnd: &Value) -> Rnd {
        let hl = gs::num(gs::get(rnd, "hands_left"));
        Rnd { hands_left: if hl != 0.0 { hl } else { 1.0 }, discards_left: gs::num(gs::get(rnd, "discards_left")) }
    }
}

// ---- canonical JSON of parse() results (used by the identity fixtures; sets sorted) ----

fn pred_json(p: &Pred) -> Value {
    match p {
        Pred::FirstFace => json!("first_face"),
        Pred::First => json!("first"),
        Pred::Final => json!("final"),
        Pred::Suit(s) => json!(["suit", s.to_string()]),
        Pred::Ranks(r) => json!(["ranks", r.iter().map(|c| c.to_string()).collect::<Vec<_>>()]),
        Pred::All => json!(["all"]),
    }
}
fn eff_json(e: &(f64, f64, f64)) -> Value {
    json!([e.0, e.1, e.2])
}
impl HandSpec {
    pub fn to_json(&self) -> Value {
        match self {
            HandSpec::Const(e) => json!(["const", eff_json(e)]),
            HandSpec::Cond { cond, yes, no } => {
                let (name, arg) = match cond {
                    Cond::Names(n) => {
                        let mut n = n.clone();
                        n.sort();
                        ("names", json!(n))
                    }
                    Cond::Le3 => ("le3", Value::Null),
                    Cond::Final => ("final", Value::Null),
                    Cond::NoDiscards => ("no_discards", Value::Null),
                    Cond::HeldSuits(s) => {
                        let mut s: Vec<String> = s.iter().map(|c| c.map_or("None".to_string(), |c| c.to_string())).collect();
                        s.sort();
                        ("held_suits", json!(s))
                    }
                    Cond::FourSuits => ("four_suits", Value::Null),
                    Cond::PlayedThisRound => ("played_this_round", Value::Null),
                };
                json!(["cond", name, arg, eff_json(yes), eff_json(no)])
            }
            HandSpec::TimesPlayed => json!(["times_played"]),
            HandSpec::PerDiscard(v) => json!(["per_discard", v]),
            HandSpec::LowestHeld => json!(["lowest_held"]),
        }
    }
}
impl Rule {
    pub fn to_json(&self) -> Value {
        json!({
            "card": self.card.iter().map(|(p, c, m, x)| json!([pred_json(p), c, m, x])).collect::<Vec<_>>(),
            "retrigger": self.retrigger.iter().map(|(p, t)| json!([pred_json(p), t])).collect::<Vec<_>>(),
            "held": self.held.iter().map(|(p, m, x)| json!([pred_json(p), m, x])).collect::<Vec<_>>(),
            "held_retrigger": self.held_retrigger,
            "hand": self.hand.as_ref().map(|h| h.to_json()),
        })
    }
}
impl JokerEntry {
    pub fn to_json(&self) -> Value {
        json!({"label": self.label, "rule": self.rule.as_ref().map(|r| r.to_json()),
               "edition": self.edition, "scoring": self.scoring})
    }
}

// jokers.py 28-36
/// '+20 Chips and +4 Mult' / 'X1.5 Mult' -> (chips, mult, xmult).
fn _parts(text: &str) -> (f64, f64, f64) {
    let mut c = 0.0;
    for m in rx!(concat!(r"\+", num!(), " Chips")).captures_iter(text) {
        c += m[1].parse::<f64>().unwrap();
    }
    let mut mu = 0.0;
    for m in rx!(concat!(r"\+", num!(), " Mult")).captures_iter(text) {
        mu += m[1].parse::<f64>().unwrap();
    }
    let mut x = 1.0;
    for m in rx!(concat!("X", num!(), " Mult")).captures_iter(text) {
        x *= m[1].parse::<f64>().unwrap();
    }
    (c, mu, x)
}

fn ranks(s: &str) -> Pred {
    let mut v: Vec<char> = s.chars().collect();
    v.sort();
    v.dedup();
    Pred::Ranks(v)
}

// jokers.py 39-60
/// Which scoring cards a per-card effect applies to, from e.g. 'face cards', 'Aces', '10 or 4'.
fn _card_pred(s: &str) -> Option<Pred> {
    if let Some(suit) = WORD_SUIT.iter().find(|(k, _)| s.contains(k)).map(|(_, v)| *v) {
        return Some(Pred::Suit(suit));
    }
    if s.contains("face") {
        return Some(ranks(FACES));
    }
    if s.contains("odd rank") {
        return Some(ranks("A9753"));
    }
    if s.contains("even rank") {
        return Some(ranks("T8642"));
    }
    let found: String = rx!(r"Ace|King|Queen|Jack|10|[2-9]").find_iter(s).map(|m| word_rank(m.as_str()).unwrap()).collect();
    if !found.is_empty() {
        return Some(ranks(&found));
    }
    if s.contains("all") || s.contains("cards") {
        return Some(Pred::All);
    }
    None
}

fn suit_eq(suit: &Option<String>, c: char) -> bool {
    suit.as_deref().is_some_and(|x| x.chars().eq(std::iter::once(c)))
}

// jokers.py 63-69
fn _pred(pred: &Pred, i: &Info) -> bool {
    match pred {
        Pred::Suit(s) => suit_eq(&i.suit, *s) || i.enh_is("WILD"),
        Pred::Ranks(r) => i.rank.as_deref().is_some_and(|x| {
            let mut it = x.chars();
            matches!((it.next(), it.next()), (Some(c), None) if r.contains(&c))
        }),
        _ => true,
    }
}

// jokers.py 156-158
fn _effect(sign: &str, val: f64, unit: &str) -> (f64, f64, f64) {
    if sign == "X" {
        return (0.0, 0.0, val);
    }
    if unit == "Chips" { (val, 0.0, 1.0) } else { (0.0, val, 1.0) }
}

fn hand_re() -> &'static str {
    static S: OnceLock<String> = OnceLock::new();
    S.get_or_init(|| {
        let mut v: Vec<&str> = HAND_NAMES.to_vec();
        v.sort_by(|a, b| b.chars().count().cmp(&a.chars().count())); // sorted(key=len, reverse=True): stable
        v.join("|")
    })
}

fn contained(need: &str) -> Vec<String> {
    match CONTAINED_IN.iter().find(|(k, _)| *k == need) {
        Some((_, v)) => v.iter().map(|s| s.to_string()).collect(),
        None => vec![need.to_string()],
    }
}

fn cond_hand(cond: Cond, eff: (f64, f64, f64)) -> Option<Rule> {
    Some(Rule { hand: Some(HandSpec::Cond { cond, yes: eff, no: (0.0, 0.0, 1.0) }), ..Default::default() })
}

// jokers.py 72-155
/// Rule for one joker, or None when its text has no scoring effect we can read.
pub fn parse(effect: &str) -> Option<Rule> {
    let e = effect;
    let mut rule = Rule::default();
    if rx!(r"1 in \d+ chance").is_match(e) && !rx!(r"^[+X]\d").is_match(e) {
        return None; // chance-based side effects (money, tarots, destruction)
    }

    // Retriggers
    if let Some(m) = rx!(concat!(r"^Retrigger (.+?)(?: used in scoring)?(?: (\d+) additional times?)?", end!())).captures(e) {
        let subj = m.get(1).unwrap().as_str();
        let times: i64 = m.get(2).map_or(1, |t| t.as_str().parse().unwrap());
        if subj.contains("first played card") {
            rule.retrigger.push((Pred::First, times));
        } else if subj.contains("held in hand") {
            rule.held_retrigger = Some(times);
        } else if subj.contains("final hand of round") {
            rule.retrigger.push((Pred::Final, times));
        } else if let Some(pred) = _card_pred(&subj.replace("each played", "").replace("all played", "")) {
            rule.retrigger.push((pred, times));
        }
        return Some(rule);
    }

    // Per scoring card: "Played Aces give +20 Chips and +4 Mult when scored", "Each played 10 or 4 gives ..."
    if let Some(m) = rx!(r"^(First played face card|(?:Each )?[Pp]layed (.+?)) (?:each )?gives? (.+?) when scored").captures(e) {
        if !e.contains("chance") {
            let (c, mu, x) = _parts(m.get(3).unwrap().as_str());
            if c != 0.0 || mu != 0.0 || x != 1.0 {
                let pred = if m.get(1).unwrap().as_str().starts_with("First") {
                    Some(Pred::FirstFace)
                } else {
                    _card_pred(m.get(2).unwrap().as_str())
                };
                if let Some(pred) = pred {
                    rule.card.push((pred, c, mu, x));
                    return Some(rule);
                }
            }
        }
    }

    // Held in hand: Baron, Shoot the Moon, Raised Fist
    if let Some(m) = rx!(concat!(r"^Each (\w+) held in hand gives (.+)", end!())).captures(e) {
        if let Some(r) = word_rank(m.get(1).unwrap().as_str()) {
            let (_c, mu, x) = _parts(m.get(2).unwrap().as_str());
            rule.held.push((Pred::Ranks(vec![r]), mu, x));
            return Some(rule);
        }
    }
    if e.starts_with("Adds double the rank of lowest ranked card held in hand") {
        rule.hand = Some(HandSpec::LowestHeld);
        return Some(rule);
    }

    let cur = rx!(concat!(r"\(Currently ([+X]?)\$?", num!(), r"(?: (Mult|Chips))?\)")).captures(e);
    let unit_guess = rx!(r"(Mult|Chips)").captures(e).map_or("Mult", |m| m.get(1).unwrap().as_str());

    // Conditional on the played hand
    static RE_NAMES: OnceLock<Regex> = OnceLock::new();
    let re_names = RE_NAMES.get_or_init(|| {
        Regex::new(&format!(r"^([+X]){} (Mult|Chips) if played hand contains an? ({})", num!(), hand_re())).unwrap()
    });
    if let Some(m) = re_names.captures(e) {
        let eff = _effect(&m[1], m[2].parse::<f64>().unwrap(), &m[3]);
        return cond_hand(Cond::Names(contained(&m[4])), eff);
    }
    if let Some(m) = rx!(concat!(r"^([+X])", num!(), r" (Mult|Chips) if played hand contains 3 or fewer cards")).captures(e) {
        return cond_hand(Cond::Le3, _effect(&m[1], m[2].parse().unwrap(), &m[3]));
    }
    if let Some(m) = rx!(concat!(r"^([+X])", num!(), r" (Mult|Chips) on final hand of round")).captures(e) {
        return cond_hand(Cond::Final, _effect(&m[1], m[2].parse().unwrap(), &m[3]));
    }
    if let Some(m) = rx!(concat!(r"^([+X])", num!(), r" (Mult|Chips) when 0 discards remaining")).captures(e) {
        return cond_hand(Cond::NoDiscards, _effect(&m[1], m[2].parse().unwrap(), &m[3]));
    }
    if let Some(m) = rx!(concat!("^X", num!(), r" Mult every \d+ hands played")).captures(e) {
        let x: f64 = m[1].parse().unwrap();
        let active = rx!(r"Active|\b0 remaining").is_match(e);
        rule.hand = Some(HandSpec::Const(if active { (0.0, 0.0, x) } else { (0.0, 0.0, 1.0) }));
        return Some(rule);
    }
    if e.starts_with("Adds the number of times poker hand has been played") {
        rule.hand = Some(HandSpec::TimesPlayed);
        return Some(rule);
    }
    if let Some(m) = rx!(concat!(r"^\+", num!(), r" Chips for each remaining discard")).captures(e) {
        rule.hand = Some(HandSpec::PerDiscard(m[1].parse().unwrap()));
        return Some(rule);
    }
    if let Some(m) = rx!(concat!("^X", num!(), r" Mult if all cards held in hand are (\w+?)s? or (\w+?)s?", end!())).captures(e) {
        let x: f64 = m[1].parse().unwrap();
        let mut suits = vec![word_suit(&m[2])];
        let s2 = word_suit(&m[3]);
        if !suits.contains(&s2) {
            suits.push(s2);
        }
        return cond_hand(Cond::HeldSuits(suits), (0.0, 0.0, x));
    }
    if let Some(m) = rx!(concat!("^X", num!(), " Mult if poker hand contains a Diamond card, Club card, Heart card, and Spade")).captures(e) {
        return cond_hand(Cond::FourSuits, (0.0, 0.0, m[1].parse().unwrap()));
    }
    if let Some(m) = rx!(concat!("^X", num!(), " Mult if played poker hand has already been played this round")).captures(e) {
        return cond_hand(Cond::PlayedThisRound, (0.0, 0.0, m[1].parse().unwrap()));
    }
    if e.starts_with("+0-23 Mult") {
        // Misprint: random, count the average
        rule.hand = Some(HandSpec::Const((0.0, 11.5, 1.0)));
        return Some(rule);
    }

    // Scaling jokers that also gain during this hand: count the gain when its condition holds
    if let Some(cur) = cur {
        let sign = cur.get(1).map_or("", |s| s.as_str());
        let sign = if sign.is_empty() { "+" } else { sign };
        let val: f64 = cur[2].parse().unwrap();
        let unit = cur.get(3).map_or(unit_guess, |u| u.as_str());
        let base = _effect(sign, val, unit);
        static RE_GAIN: OnceLock<Regex> = OnceLock::new();
        let re_gain = RE_GAIN.get_or_init(|| {
            Regex::new(&format!(r"[Gg]ains ([+X]){} (Mult|Chips) if played hand contains an? ({})", num!(), hand_re())).unwrap()
        });
        if let Some(g) = re_gain.captures(e) {
            let gain: f64 = g[2].parse().unwrap();
            rule.hand = Some(HandSpec::Cond { cond: Cond::Names(contained(&g[4])), yes: _effect(sign, val + gain, unit), no: base });
        } else {
            rule.hand = Some(HandSpec::Const(base));
        }
        return Some(rule);
    }

    // Plain values: "+4 Mult", "X1.44 Mult, loses ...", "+60 Chips -5 Chips for every hand played"
    if let Some(m) = rx!(concat!(r"^([+X])", num!(), r" (Mult|Chips)(.*)", end!())).captures(e) {
        if !rx!(r"^\s*(if|on|when|every|for each|per)\b").is_match(&m[4]) {
            rule.hand = Some(HandSpec::Const(_effect(&m[1], m[2].parse().unwrap(), &m[3])));
            return Some(rule);
        }
    }
    None
}

// jokers.py 161-165
fn _nominal(i: &Info) -> f64 {
    match i.rank_truthy() {
        Some(r) if !i.enh_is("STONE") => hands::rank_chips(r),
        _ => 0.0,
    }
}

// jokers.py 168-171
pub fn entry(c: &Value) -> JokerEntry {
    let v = gs::d(gs::get(c, "value"));
    let eff = match gs::mget(v, "effect") {
        x if gs::truthy(x) => gs::s(x),
        _ => "",
    };
    let label = match gs::get(c, "label") {
        Value::String(s) => Some(s.clone()),
        _ => None,
    };
    let edition = match gs::get(gs::get(c, "modifier"), "edition") {
        Value::String(s) => Some(s.clone()),
        _ => None,
    };
    JokerEntry { label, rule: parse(eff), edition, scoring: rx!(r"Mult|Chips|Retrigger").is_match(eff) }
}

// jokers.py 174-185
/// Parse the run's jokers once per decision. `cards`: Python's optional override list.
pub fn build(gs_: &Value, cards: Option<&[Value]>) -> Vec<JokerEntry> {
    let cards = match cards {
        Some(c) => c,
        None => gs::cards_of(gs_, "jokers"),
    };
    let mut out: Vec<JokerEntry> = cards.iter().map(entry).collect();
    // Blueprint copies the joker to its right, Brainstorm the leftmost.
    for n in 0..out.len() {
        let label = out[n].label.as_deref();
        if label == Some("Blueprint") || label == Some("Brainstorm") {
            let src = if label == Some("Blueprint") && n + 1 < out.len() {
                Some(n + 1)
            } else if n > 0 {
                Some(0)
            } else {
                None
            };
            if let Some(s) = src {
                if !matches!(out[s].label.as_deref(), Some("Blueprint") | Some("Brainstorm")) {
                    let (r, sc) = (out[s].rule.clone(), out[s].scoring);
                    out[n].rule = r;
                    out[n].scoring = sc;
                }
            }
        }
    }
    out
}

// jokers.py 188-189
pub fn unmodelled(jokers: &[JokerEntry]) -> Vec<Option<String>> {
    jokers.iter().filter(|j| j.rule.is_none() && j.scoring).map(|j| j.label.clone()).collect()
}

// TODO(jokers-rest): jokers.py 190-285 (play_forecast_supported, PLAY_STABLE/PLAY_UTILITY, after_discard,
// after_play) and 395-441 (sample_plays, gain, planet_level).

// jokers.py 286-358
/// Expected score of playing hand positions `combo` (scoring = indices into combo).
pub fn score(infos: &[Info], combo: &[usize], name: &str, scoring: &[usize], hand_levels: &Levels, jokers: &[JokerEntry], rnd: &Rnd) -> i64 {
    let h = hand_levels.get(name).unwrap_or(&gs::NULL); // hand_levels.get(name, {})
    let hnum = |k: &str, d: f64| match gs::d(h).get(k) {
        None => d,
        Some(v) => gs::num(v),
    };
    let (mut chips, mut mult) = (hnum("chips", 0.0), hnum("mult", 1.0));
    let played: Vec<&Info> = combo.iter().map(|&i| &infos[i]).collect();
    let held: Vec<&Info> = (0..infos.len()).filter(|i| !combo.contains(i)).map(|i| &infos[i]).collect();
    let mut sc: Vec<usize> = scoring.to_vec();
    sc.sort();
    let hands_left = rnd.hands_left;
    let rules: Vec<&Rule> = jokers.iter().filter_map(|j| j.rule.as_ref()).collect();

    let is_face = |c: &Info| c.rank.as_deref().is_some_and(|r| r.chars().count() == 1 && FACES.contains(r));
    let first_face = sc.iter().copied().find(|&p| is_face(played[p]) && !played[p].debuff);
    for (k, &p) in sc.iter().enumerate() {
        let c = played[p];
        if c.debuff {
            continue;
        }
        let mut reps: i64 = 1 + c.seal.as_deref().is_some_and(|s| s == "RED") as i64;
        for r in &rules {
            for (pred, times) in &r.retrigger {
                if (*pred == Pred::First && k == 0)
                    || (*pred == Pred::Final && hands_left == 1.0)
                    || (matches!(pred, Pred::Suit(_) | Pred::Ranks(_) | Pred::All) && _pred(pred, c))
                {
                    reps += times;
                }
            }
        }
        for _ in 0..reps {
            chips += if c.enh_is("STONE") || c.rank_truthy().is_none() { 50.0 } else { hands::rank_chips(c.rank_truthy().unwrap()) };
            chips += (if c.enh_is("BONUS") { 30.0 } else { 0.0 }) + (if is_ed(c, "FOIL") { 50.0 } else { 0.0 });
            mult += (if c.enh_is("MULT") { 4.0 } else { 0.0 }) + (if is_ed(c, "HOLO") { 10.0 } else { 0.0 });
            mult *= (if c.enh_is("GLASS") { 2.0 } else { 1.0 }) * (if is_ed(c, "POLYCHROME") { 1.5 } else { 1.0 });
            for r in &rules {
                for (pred, cc, mm, xx) in &r.card {
                    if (*pred == Pred::FirstFace && Some(p) == first_face)
                        || (matches!(pred, Pred::Suit(_) | Pred::Ranks(_) | Pred::All) && _pred(pred, c))
                    {
                        chips += cc;
                        mult = (mult + mm) * xx;
                    }
                }
            }
        }
    }

    let held_reps: i64 = 1 + rules.iter().map(|r| r.held_retrigger.unwrap_or(0)).sum::<i64>();
    for c in &held {
        if c.debuff {
            continue;
        }
        for _ in 0..(held_reps + c.seal.as_deref().is_some_and(|s| s == "RED") as i64) {
            if c.enh_is("STEEL") {
                mult *= 1.5;
            }
            for r in &rules {
                for (pred, mm, xx) in &r.held {
                    if _pred(pred, c) {
                        mult = (mult + mm) * xx;
                    }
                }
            }
        }
    }

    let mut scoring_suits: Vec<Option<&str>> = Vec::new();
    for &p in &sc {
        if !played[p].enh_is("WILD") && !scoring_suits.contains(&played[p].suit.as_deref()) {
            scoring_suits.push(played[p].suit.as_deref());
        }
    }
    let ctx = Ctx {
        name,
        n_played: played.len(),
        held: &held,
        hands_left,
        discards_left: rnd.discards_left,
        times_played: hnum("played", 0.0),
        played_this_round: gs::truthy(gs::mget(gs::d(h), "played_this_round")),
        scoring_suits: scoring_suits.len(),
        wild: sc.iter().filter(|&&p| played[p].enh_is("WILD")).count(),
    };
    for j in jokers {
        let ed = j.edition.as_deref();
        chips += if ed == Some("FOIL") { 50.0 } else { 0.0 };
        mult += if ed == Some("HOLO") { 10.0 } else { 0.0 };
        if let Some(Rule { hand: Some(spec), .. }) = &j.rule {
            let (cc, mm, xx) = _hand(spec, &ctx);
            chips += cc;
            mult = (mult + mm) * xx;
        }
        mult *= if ed == Some("POLYCHROME") { 1.5 } else { 1.0 };
    }
    // int(max(0, chips) * max(1, mult))
    ((if chips > 0.0 { chips } else { 0.0 }) * (if mult > 1.0 { mult } else { 1.0 })) as i64
}

struct Ctx<'a> {
    name: &'a str,
    n_played: usize,
    held: &'a [&'a Info],
    hands_left: f64,
    discards_left: f64,
    times_played: f64,
    played_this_round: bool,
    scoring_suits: usize,
    wild: usize,
}

// jokers.py 361-392
/// (chips, mult, xmult) of a joker's whole-hand rule in this hand.
fn _hand(spec: &HandSpec, ctx: &Ctx) -> (f64, f64, f64) {
    match spec {
        HandSpec::Const(e) => *e,
        HandSpec::Cond { cond, yes, no } => {
            let ok = match cond {
                Cond::Names(arg) => arg.iter().any(|n| n == ctx.name),
                Cond::Le3 => ctx.n_played <= 3,
                Cond::Final => ctx.hands_left == 1.0,
                Cond::NoDiscards => ctx.discards_left == 0.0,
                Cond::HeldSuits(arg) => ctx.held.iter().all(|i| {
                    arg.iter().any(|a| match a {
                        Some(c) => suit_eq(&i.suit, *c),
                        None => i.suit.is_none(),
                    })
                }),
                Cond::FourSuits => ctx.scoring_suits + ctx.wild >= 4,
                Cond::PlayedThisRound => ctx.played_this_round,
            };
            if ok { *yes } else { *no }
        }
        HandSpec::TimesPlayed => (0.0, ctx.times_played + 1.0, 1.0),
        HandSpec::PerDiscard(v) => (v * ctx.discards_left, 0.0, 1.0),
        HandSpec::LowestHeld => {
            let mut m: Option<f64> = None;
            for i in ctx.held {
                let n = _nominal(i);
                if m.map_or(true, |x| n < x) {
                    m = Some(n);
                }
            }
            (0.0, 2.0 * m.unwrap_or(0.0), 1.0)
        }
    }
}

/// score() bound to one decision's hand levels, jokers and round: scorer(infos, combo, name, scoring).
pub struct Scorer {
    pub hand_levels: Levels,
    pub jokers: Vec<JokerEntry>,
    pub rnd: Rnd,
    native: OnceCell<Option<jev_kernel::Scorer>>, // not built yet; None inside once we know it can't be
}

impl Scorer {
    pub fn new(hand_levels: Levels, jokers: Vec<JokerEntry>, rnd: Rnd) -> Scorer {
        Scorer { hand_levels, jokers, rnd, native: OnceCell::new() }
    }

    pub fn call(&self, infos: &[Info], combo: &[usize], name: &str, scoring: &[usize]) -> i64 {
        score(infos, combo, name, scoring, &self.hand_levels, &self.jokers, &self.rnd)
    }

    pub fn native(&self) -> Option<&jev_kernel::Scorer> {
        self.native.get_or_init(|| hands::native::scorer(&self.hand_levels, &self.jokers, &self.rnd)).as_ref()
    }
}
