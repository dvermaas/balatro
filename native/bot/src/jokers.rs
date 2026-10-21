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
use serde_json::{json, Map, Value};
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
    Card(char, char), // v28: one rank of one suit (The Idol)
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
    SeeingDouble,
    /// no face card (not debuffed) among the scoring cards
    NoFaceScored,
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
        Pred::Card(r, s) => json!(["card", r.to_string(), s.to_string()]),
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
                    Cond::SeeingDouble => ("seeing_double", Value::Null),
                    Cond::NoFaceScored => ("no_face_scored", Value::Null),
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
    if let Some(m) = rx!(r"\b(Ace|King|Queen|Jack|10|[2-9]) of (Diamond|Heart|Club|Spade)s?\b").captures(s) {
        // v28: The Idol's one card ("2 of Spades"), not every card of the suit
        return Some(Pred::Card(word_rank(&m[1]).unwrap(), word_suit(&m[2]).unwrap()));
    }
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
        Pred::Card(r, s) => {
            i.rank.as_deref().is_some_and(|x| x.chars().eq(std::iter::once(*r))) && (suit_eq(&i.suit, *s) || i.enh_is("WILD"))
        }
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
    // v28: Bloodstone. A chance-based xMult per scoring card counts at its expected factor
    if let Some(m) = rx!(concat!(r"^(\d+) in (\d+) chance for played cards with (\w+?) suit to give X", num!(), r" Mult when scored")).captures(e) {
        if let Some(suit) = word_suit(&m[3]) {
            let p = (m[1].parse::<i64>().unwrap() as f64 / m[2].parse::<i64>().unwrap() as f64).min(1.0);
            let x: f64 = m[4].parse().unwrap();
            rule.card.push((Pred::Suit(suit), 0.0, 0.0, 1.0 + p * (x - 1.0)));
            return Some(rule);
        }
    }
    if rx!(r"1 in \d+ chance").is_match(e) && !rx!(r"^[+X]\d").is_match(e) {
        return None; // chance-based side effects (money, tarots, destruction)
    }

    // Retriggers
    if let Some(m) = rx!(concat!(r"^Retrigger (.+?)(?: used in scoring)?(?: (\d+) additional times?)?", end!())).captures(e) {
        // v37: Seltzer's "for the next 10 hands" is a duration, not the rank 10
        let subj_owned = rx!(r" for the next \d+ hands?").replace(m.get(1).unwrap().as_str(), "").to_string();
        let subj = subj_owned.as_str();
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

    // v28: Driver's License ("(Currently 2)" is a card count, not +2 Mult)
    if let Some(m) = rx!(concat!("^X", num!(), r" Mult if you have at least (\d+) Enhanced cards in your full deck \(Currently (\d+)\)")).captures(e) {
        let x = if m[3].parse::<i64>().unwrap() >= m[2].parse::<i64>().unwrap() { m[1].parse().unwrap() } else { 1.0 };
        rule.hand = Some(HandSpec::Const((0.0, 0.0, x)));
        return Some(rule);
    }
    // v28: Seeing Double
    if let Some(m) = rx!(concat!("^X", num!(), " Mult if played hand has a scoring Club card and a scoring card of any other suit")).captures(e) {
        return cond_hand(Cond::SeeingDouble, (0.0, 0.0, m[1].parse().unwrap()));
    }

    // v37: "(Currently $4)" is money (Cloud 9, Rocket...), not +4 Mult
    if rx!(r"\(Currently \$").is_match(e) {
        return None;
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
    // v28: Baseball Card gives X1.5 per Uncommon joker; rarity isn't in the gamestate, so look it up by key
    for (j, c) in cards.iter().enumerate() {
        if let Some(m) = rx!(concat!("^Uncommon Jokers each give X", num!(), " Mult")).captures(&effect_or_empty(gs::mget(gs::d(gs::get(c, "value")), "effect"))) {
            let n = cards.iter().enumerate().filter(|&(o, oc)| o != j && _rarity(gs::get(oc, "key")) == Some(2)).count();
            let x: f64 = m[1].parse().unwrap();
            out[j].rule = Some(Rule { hand: Some(HandSpec::Const((0.0, 0.0, x.powf(n as f64)))), ..Default::default() });
        }
    }
    // v42: Misprint's exported effect is only "+" (its text is a random dynatext, v16 note), so it scored 0; count
    // the average of its +0-23 Mult
    for (j, c) in cards.iter().enumerate() {
        if gs::get(c, "key").as_str() == Some("j_misprint") && !gs::truthy(gs::mget(gs::d(gs::get(c, "state")), "debuff")) {
            out[j].rule = Some(Rule { hand: Some(HandSpec::Const((0.0, 11.5, 1.0))), ..Default::default() });
            out[j].scoring = true;
        }
    }
    if !crate::experiment::get().no_scorer_fix {
        // v58: jokers the game updates before the hand scores (rumbot update_before_scoring)
        let played = |h: &str| gs::num(gs::mget(gs::d(gs::get(gs::get(gs_, "hands"), h)), "played"));
        for (j, c) in cards.iter().enumerate() {
            if gs::truthy(gs::mget(gs::d(gs::get(c, "state")), "debuff")) {
                continue;
            }
            let eff = effect_or_empty(gs::mget(gs::d(gs::get(c, "value")), "effect"));
            match gs::get(c, "key").as_str() {
                Some("j_obelisk") => {
                    // gains X0.2 first, or resets to X1 when the hand (counted with this play) is the most played
                    if let Some(m) = rx!(r"Currently X(\d+(?:\.\d+)?)").captures(&eff) {
                        let cur: f64 = m[1].parse().unwrap();
                        let names: Vec<String> = HAND_NAMES.iter().map(|s| s.to_string()).collect();
                        let reset: Vec<String> = names.iter().filter(|h| {
                            let mine = played(h) + 1.0;
                            names.iter().all(|o| o == *h || played(o) < mine)
                        }).cloned().collect();
                        out[j].rule = Some(Rule { hand: Some(HandSpec::Cond { cond: Cond::Names(reset), yes: (0.0, 0.0, 1.0), no: (0.0, 0.0, cur + 0.2) }), ..Default::default() });
                        out[j].scoring = true;
                    }
                }
                Some("j_ride_the_bus") => {
                    // +1 first when no face card scores, else it resets to 0 (Pareidolia: every card is a face)
                    if let Some(m) = rx!(r"Currently \+(\d+(?:\.\d+)?) Mult").captures(&eff) {
                        let cur: f64 = m[1].parse().unwrap();
                        let pareidolia = cards.iter().any(|o| gs::get(o, "key").as_str() == Some("j_pareidolia"));
                        let spec = if pareidolia {
                            HandSpec::Const((0.0, 0.0, 1.0))
                        } else {
                            HandSpec::Cond { cond: Cond::NoFaceScored, yes: (0.0, cur + 1.0, 1.0), no: (0.0, 0.0, 1.0) }
                        };
                        out[j].rule = Some(Rule { hand: Some(spec), ..Default::default() });
                        out[j].scoring = true;
                    }
                }
                Some("j_green_joker") => {
                    // +1 Mult for this hand before it scores
                    if let Some(m) = rx!(r"Currently \+(\d+(?:\.\d+)?) Mult").captures(&eff) {
                        let cur: f64 = m[1].parse().unwrap();
                        out[j].rule = Some(Rule { hand: Some(HandSpec::Const((0.0, cur + 1.0, 1.0))), ..Default::default() });
                        out[j].scoring = true;
                    }
                }
                _ => {}
            }
        }
    }
    // Blueprint copies the joker to its right, Brainstorm the leftmost.
    for n in 0..out.len() {
        let label = out[n].label.as_deref();
        if label == Some("Blueprint") || label == Some("Brainstorm") {
            let src = if label == Some("Blueprint") && n + 1 < out.len() {
                Some(n + 1)
            } else if label == Some("Blueprint") && !crate::experiment::get().no_scorer_fix {
                // v58: the rightmost Blueprint copies nothing (card.lua: G.jokers.cards[i+1] is nil)
                None
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
// jokers.py 247-263
/// Joker rarity (1 common .. 4 legendary); rumbot's table matches Jackdaw's centers.json key for key.
fn _rarity(key: &Value) -> Option<u8> {
    key.as_str().and_then(rumbot_sim::joker_data::joker_row_by_key).map(|r| r.rarity)
}

pub fn unmodelled(jokers: &[JokerEntry]) -> Vec<Option<String>> {
    jokers.iter().filter(|j| j.rule.is_none() && j.scoring).map(|j| j.label.clone()).collect()
}

fn label_of(c: &Value) -> Option<&str> {
    gs::get(c, "label").as_str()
}

/// Python `x.get(k) or ""` for an effect string.
fn effect_or_empty(v: &Value) -> String {
    match v {
        Value::String(s) => s.clone(),
        _ => String::new(),
    }
}

/// `{**card, "value": {**card["value"], "effect": effect}}`.
fn with_effect(card: &Value, effect: String) -> Value {
    let mut c = card.as_object().cloned().unwrap_or_default();
    let mut v = gs::d(gs::get(card, "value")).clone();
    v.insert("effect".to_string(), Value::String(effect));
    c.insert("value".to_string(), Value::Object(v));
    Value::Object(c)
}

// jokers.py 222-233
/// Forecast known discard-sensitive scoring values without mutating the live snapshot.
/// Banner/Mystic Summit use the decremented round counter in score(). Other event-driven
/// effects remain approximations, just as in the normal scorer.
pub fn after_discard(gs_: &Value, discarded: &[Value], drawn: i64) -> Vec<JokerEntry> {
    let mut cards: Vec<Value> = Vec::new();
    for card in gs::cards_of(gs_, "jokers") {
        let mut effect = effect_or_empty(gs::mget(gs::d(gs::get(card, "value")), "effect"));
        let label = label_of(card);
        if label == Some("Blue Joker") {
            let remaining = (gs::cards_of(gs_, "cards").len() as i64 - drawn).max(0);
            effect = format!("+{} Chips", 2 * remaining);
        } else if label == Some("Ramen") {
            if let Some(m) = rx!(r"^X(\d+(?:\.\d+)?) Mult").captures(&effect) {
                let v = m[1].parse::<f64>().unwrap() - 0.01 * (discarded.len() as f64);
                let v = if v > 1.0 { v } else { 1.0 }; // max(1, v) is the int 1 when v <= 1; both print 1.00000000
                effect = format!("X{} Mult", pycompat::fixed(v, 8));
            }
        } else if label == Some("Green Joker") {
            if let Some(m) = rx!(r"Currently \+(\d+(?:\.\d+)?) Mult").captures(&effect) {
                let v = m[1].parse::<f64>().unwrap() - 1.0;
                // max(0, v): the int 0 (prints "0") when v <= 0, else the float
                let shown = if v > 0.0 { pycompat::pyfloat(v) } else { "0".to_string() };
                effect = format!("+{} Mult", shown);
            }
        }
        cards.push(with_effect(card, effect));
    }
    build(gs_, Some(&cards))
}

// jokers.py 236-240
/// Only effects whose transition is modeled below or which stay fixed between plays.
/// Unknown, scaling, destructive, random and copy jokers fall back to normal hand play.
const PLAY_STABLE: &[&str] = &[
    "Joker", "Greedy Joker", "Lusty Joker", "Wrathful Joker", "Gluttonous Joker",
    "Jolly Joker", "Zany Joker", "Mad Joker", "Crazy Joker", "Droll Joker",
    "Sly Joker", "Wily Joker", "Clever Joker", "Devious Joker", "Crafty Joker",
    "Half Joker", "Banner", "Mystic Summit", "Dusk", "Raised Fist", "Fibonacci",
    "Scary Face", "Abstract Joker", "Even Steven", "Odd Todd", "Scholar",
    "Supernova", "Blackboard", "Blue Joker", "Baron", "Photograph", "Acrobat",
    "Sock and Buskin", "Hanging Chad", "Smiley Face", "Walkie Talkie",
    "Shoot the Moon", "Stuntman", "The Duo", "The Trio", "The Family", "The Order", "The Tribe",
    "Arrowhead", "Onyx Agate",
];
const PLAY_UTILITY: &[&str] = &["Burglar", "Riff-raff", "Golden Joker", "Showman", "Juggler", "Drunkard", "Troubadour"];

// jokers.py 243-251
pub fn play_forecast_supported(gs_: &Value) -> bool {
    for c in gs::cards_of(gs_, "jokers") {
        if gs::truthy(gs::mget(gs::d(gs::get(c, "state")), "debuff")) {
            return false;
        }
        let label = label_of(c);
        if label.is_some_and(|l| PLAY_UTILITY.contains(&l)) {
            continue;
        }
        if !label.is_some_and(|l| PLAY_STABLE.contains(&l)) || entry(c).rule.is_none() {
            return false;
        }
    }
    true
}

// jokers.py 254-263
/// Supported joker state after a play; hand counters are updated by the caller.
pub fn after_play(gs_: &Value, drawn: i64) -> Vec<JokerEntry> {
    let mut cards: Vec<Value> = Vec::new();
    for c in gs::cards_of(gs_, "jokers") {
        if label_of(c) == Some("Blue Joker") {
            let remaining = (gs::cards_of(gs_, "cards").len() as i64 - drawn).max(0);
            cards.push(with_effect(c, format!("+{} Chips", 2 * remaining)));
        } else {
            cards.push(c.clone());
        }
    }
    build(gs_, Some(&cards))
}

// jokers.py 396-416
/// One hand to measure on: infos, combo (positions), scoring name, scoring positions, round dict.
pub struct Sample {
    pub infos: Vec<Info>,
    pub combo: Vec<usize>,
    pub name: &'static str,
    pub sc: Vec<usize>,
    pub rnd: Value,
}

fn or_num(v: &Value, default: f64) -> f64 {
    if gs::truthy(v) { gs::num(v) } else { default }
}

/// Hands to measure items on: this run's recent plays, else the example hands of our likely hands.
pub fn sample_plays(gs_: &Value, recent: &[Value]) -> Vec<Sample> {
    let mut out: Vec<Sample> = Vec::new();
    for r in recent {
        let infos: Vec<Info> = r["infos"].as_array().unwrap().iter().map(Info::from_json).collect();
        let combo: Vec<usize> = r["combo"].as_array().unwrap().iter().map(|i| i.as_u64().unwrap() as usize).collect();
        let played: Vec<Info> = combo.iter().map(|&i| infos[i].clone()).collect();
        let (name, sc) = hands::classify(&played);
        out.push(Sample { infos, combo, name, sc, rnd: r["rnd"].clone() });
    }
    if out.len() >= 4 {
        // fewer recent hands are too noisy alone: mix in typical ones
        return out;
    }
    let hands_map = gs::d(gs::get(gs_, "hands"));
    // sorted(hands, key=lambda n: (-(played or 0), -(level or 1)))[:2] or ["Pair"]
    let mut keyed: Vec<(&String, f64, f64)> = hands_map
        .iter()
        .map(|(n, h)| (n, -or_num(gs::mget(gs::d(h), "played"), 0.0), -or_num(gs::mget(gs::d(h), "level"), 1.0)))
        .collect();
    keyed.sort_by(|a, b| a.1.partial_cmp(&b.1).unwrap().then(a.2.partial_cmp(&b.2).unwrap()));
    let mut names: Vec<&str> = keyed.iter().take(2).map(|k| k.0.as_str()).collect();
    if names.is_empty() {
        names.push("Pair");
    }
    for n in names {
        let ex_v = gs::get(hands_map.get(n).unwrap_or(&gs::NULL), "example");
        let ex: Vec<Value> = if gs::truthy(ex_v) { ex_v.as_array().cloned().unwrap_or_default() } else { vec![] };
        let infos: Vec<Info> = ex
            .iter()
            .map(|pair| {
                let k = pair[0].as_str().unwrap();
                Info {
                    rank: Some(k.split('_').nth(1).unwrap().to_string()),
                    suit: Some(k.chars().next().unwrap().to_string()),
                    enh: None,
                    edition: None,
                    seal: None,
                    debuff: false,
                    extra: 0,
                }
            })
            .collect();
        if !infos.is_empty() {
            let (name, sc) = hands::classify(&infos);
            let combo: Vec<usize> = (0..infos.len()).collect();
            out.push(Sample { infos, combo, name, sc, rnd: json!({"hands_left": 2, "discards_left": 1}) });
        }
    }
    out
}

/// `levels.get(h, {}).get(k, default) + c`, keeping int or float as Python does.
fn add_level(cur: Option<&Value>, default: i64, c: i64) -> Value {
    match cur {
        None => json!(default + c),
        Some(Value::Number(n)) if n.is_i64() => json!(n.as_i64().unwrap() + c),
        Some(v) => json!(gs::num(v) + c as f64),
    }
}

// jokers.py 419-441
/// Score ratio - 1 over sample hands after adding a joker card, removing joker index `remove`,
/// or levelling a hand (level_up=(hand, chips, mult)). None when the joker's effect can't be modelled.
pub fn gain(gs_: &Value, recent: &[Value], add: Option<&Value>, remove: Option<usize>, level_up: Option<(&str, i64, i64)>) -> Option<f64> {
    let owned = gs::cards_of(gs_, "jokers");
    if let Some(a) = add {
        if entry(a).rule.is_none() {
            return if entry(a).scoring { None } else { Some(0.0) };
        }
    }
    let base_js = build(gs_, Some(owned));
    let mut new_cards: Vec<Value> = owned.iter().enumerate().filter(|(n, _)| Some(*n) != remove).map(|(_, c)| c.clone()).collect();
    if let Some(a) = add {
        new_cards.push(a.clone());
    }
    let new_js = build(gs_, Some(&new_cards));
    let levels: Levels = gs::d(gs::get(gs_, "hands")).clone();
    let mut new_levels = levels.clone();
    if let Some((h, c, m)) = level_up {
        let mut cur: Map<String, Value> = levels.get(h).map_or_else(Map::new, |v| gs::d(v).clone());
        let chips = add_level(cur.get("chips"), 0, c);
        let mult = add_level(cur.get("mult"), 1, m);
        cur.insert("chips".to_string(), chips);
        cur.insert("mult".to_string(), mult);
        new_levels.insert(h.to_string(), Value::Object(cur));
    }
    let (mut before, mut after) = (0i64, 0i64);
    for s in sample_plays(gs_, recent) {
        let rnd = Rnd::from_value(&s.rnd);
        before += score(&s.infos, &s.combo, s.name, &s.sc, &levels, &base_js, &rnd);
        after += score(&s.infos, &s.combo, s.name, &s.sc, &new_levels, &new_js, &rnd);
    }
    if before != 0 { Some(after as f64 / before as f64 - 1.0) } else { None }
}

// jokers.py 442-444
/// (hand, chips, mult) of a planet's "Level up" text, else None.
pub fn planet_level(effect: &str) -> Option<(String, i64, i64)> {
    static R: OnceLock<Regex> = OnceLock::new();
    let re = R.get_or_init(|| Regex::new(&format!(r"Level up ({}) \+(\d+) Mult and \+(\d+) chips", hand_re())).unwrap());
    re.captures(effect).map(|m| (m[1].to_string(), m[3].parse().unwrap(), m[2].parse().unwrap()))
}

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
                    || (matches!(pred, Pred::Suit(_) | Pred::Ranks(_) | Pred::Card(..) | Pred::All) && _pred(pred, c))
                {
                    reps += times;
                }
            }
        }
        for _ in 0..reps {
            chips += if c.enh_is("STONE") || c.rank_truthy().is_none() { 50.0 } else { hands::rank_chips(c.rank_truthy().unwrap()) } + c.extra as f64;
            chips += (if c.enh_is("BONUS") { 30.0 } else { 0.0 }) + (if is_ed(c, "FOIL") { 50.0 } else { 0.0 });
            mult += (if c.enh_is("MULT") { 4.0 } else { 0.0 }) + (if is_ed(c, "HOLO") { 10.0 } else { 0.0 });
            mult *= (if c.enh_is("GLASS") { 2.0 } else { 1.0 }) * (if is_ed(c, "POLYCHROME") { 1.5 } else { 1.0 });
            for r in &rules {
                for (pred, cc, mm, xx) in &r.card {
                    if (*pred == Pred::FirstFace && Some(p) == first_face)
                        || (matches!(pred, Pred::Suit(_) | Pred::Ranks(_) | Pred::Card(..) | Pred::All) && _pred(pred, c))
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
        club: scoring_suits.contains(&Some("C")),
        other_suit: scoring_suits.iter().any(|s| *s != Some("C")),
        wild: sc.iter().filter(|&&p| played[p].enh_is("WILD")).count(),
        held_retrigger: held_reps - 1,
        face_scored: first_face.is_some(),
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
    club: bool,
    other_suit: bool,
    wild: usize,
    held_retrigger: i64,
    face_scored: bool,
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
                // Wild cards fill Clubs first, then the other suits (joker_main)
                Cond::SeeingDouble => {
                    let (c, o, w) = (ctx.club, ctx.other_suit, ctx.wild);
                    (c || w > 0) && (o || (c && w > 0) || w >= 2)
                }
                Cond::NoFaceScored => !ctx.face_scored,
            };
            if ok { *yes } else { *no }
        }
        HandSpec::TimesPlayed => (0.0, ctx.times_played + 1.0, 1.0),
        HandSpec::PerDiscard(v) => (v * ctx.discards_left, 0.0, 1.0),
        HandSpec::LowestHeld => {
            // v42 Raised Fist (rumbot lowest_held): the rightmost held card of the lowest rank, Stone cards skipped;
            // nothing when it is debuffed, and it repeats with held retriggers (Mime) and its Red Seal
            let order = |i: &Info| "23456789TJQKA".find(i.rank_truthy().unwrap_or("")).unwrap_or(0);
            let mut low: Option<&Info> = None;
            for &i in ctx.held {
                if i.rank_truthy().is_some() && !i.enh_is("STONE") && low.map_or(true, |b| order(i) <= order(b)) {
                    low = Some(i);
                }
            }
            match low {
                Some(i) if !i.debuff => {
                    let reps = 1 + ctx.held_retrigger + i.seal.as_deref().is_some_and(|s| s == "RED") as i64;
                    (0.0, 2.0 * _nominal(i) * reps as f64, 1.0)
                }
                _ => (0.0, 0.0, 1.0),
            }
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
