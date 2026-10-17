//! rollout_items.py 56-212: `_cards`, `usable`, `synth_plays` and `Picker` (which pack card to take).
//! `usable` lives in forecast_stats.rs (it uses tarots.rs's key lists); `_cards` is `gs::cards_of`.

use crate::hands::{card_info, classify, rank_index, Info};
use crate::jokers::{self, Rnd};
use crate::{build, gs, tarots};
use pycompat::json::pyrepr;
use pycompat::Random;
use regex::Regex;
use serde_json::{json, Value};
use std::collections::HashMap;

const MONEY: f64 = 0.01;
pub const SYNTH_HANDS: usize = 12;

fn flat(key: &str) -> Option<f64> {
    Some(match key {
        "c_high_priestess" => 0.08, "c_emperor" => 0.04, "c_fool" => 0.02, "c_judgement" => 0.10,
        "c_wheel_of_fortune" => 0.03, "c_black_hole" => 0.30, "c_lovers" => 0.02, "c_tower" => 0.0,
        "c_devil" => 0.03, "c_hanged_man" => 0.02, "c_strength" => 0.015, "c_death" => 0.03, "c_star" => 0.03,
        "c_moon" => 0.03, "c_sun" => 0.03, "c_world" => 0.03, "c_aura" => 0.08, "c_deja_vu" => 0.03,
        "c_talisman" => 0.03, "c_medium" => 0.01, "c_trance" => 0.03, "c_cryptid" => 0.04,
        _ => return None,
    })
}
fn edition_v(e: Option<&str>) -> Option<f64> {
    match e { Some("POLYCHROME") => Some(0.04), Some("HOLO") => Some(0.02), Some("FOIL") => Some(0.01), _ => None }
}
fn enh_v(e: Option<&str>) -> Option<f64> {
    match e {
        Some("GLASS") => Some(0.03), Some("STEEL") => Some(0.02), Some("LUCKY") => Some(0.015),
        Some("MULT") => Some(0.015), Some("BONUS") => Some(0.01), Some("GOLD") => Some(0.01),
        Some("WILD") => Some(0.01), _ => None,
    }
}
fn seal_v(e: Option<&str>) -> Option<f64> {
    match e { Some("RED") => Some(0.02), Some("GOLD") => Some(0.015), Some("BLUE") => Some(0.01), Some("PURPLE") => Some(0.005), _ => None }
}
/// Python `max(a, b)`: b only when strictly greater.
fn pmax(a: f64, b: f64) -> f64 {
    if b > a { b } else { a }
}

fn py_str(v: &Value) -> String {
    match v { Value::String(s) => s.clone(), o => pyrepr(o) }
}

/// rollout.candidates on the bot's hand infos (structural play candidates, same logic as simcore).
fn candidates(infos: &[Info]) -> Vec<Vec<usize>> {
    let cards: Vec<jev_kernel::Card> = infos.iter().map(kernel_card).collect();
    simcore::candidates(&cards)
}
/// hands.card_info as the kernel's card: rank 0 for none or Stone, suit 1-4 (H, D, C, S).
fn kernel_card(i: &Info) -> jev_kernel::Card {
    let stone = i.enh_is("STONE");
    jev_kernel::Card {
        rank: if stone { 0 } else { i.rank_truthy().map_or(0, |r| rank_index(r) as u8 + 2) },
        suit: match i.suit.as_deref() { Some("H") => 1, Some("D") => 2, Some("C") => 3, Some("S") => 4, _ => 0 },
        enh: if stone { jev_kernel::STONE } else if i.enh_is("WILD") { jev_kernel::WILD } else { 0 },
        edition: 0,
        red: false,
        debuff: i.debuff,
    }
}

/// Recent-play stand-ins: `{"infos", "combo", "rnd"}` dicts (jokers::sample_plays input).
pub fn synth_plays(pub_: &Value, k: usize, seed: &str) -> Vec<Value> {
    let mut deck: Vec<Value> = gs::cards_of(pub_, "cards").to_vec();
    deck.extend(gs::cards_of(pub_, "hand").iter().cloned());
    let key = |c: &Value| {
        let v = gs::get(c, "value");
        (py_str(gs::get(v, "suit")), py_str(gs::get(v, "rank")), pyrepr(&Value::Object(gs::d(gs::get(c, "modifier")).clone())))
    };
    deck.sort_by_key(key);
    let infos: Vec<Info> = deck.iter().map(|c| { let mut i = card_info(c); i.debuff = false; i }).collect();
    if infos.is_empty() {
        return vec![];
    }
    let mut rng = Random::new_str(seed);
    let lim = gs::get(gs::get(pub_, "hand"), "limit");
    let size = (if gs::truthy(lim) { gs::int(lim) } else { 8 }).min(infos.len() as i64) as usize;
    let js = jokers::build(pub_, None);
    let levels = gs::d(gs::get(pub_, "hands")).clone();
    let rnd = Rnd { hands_left: 3.0, discards_left: 2.0 };
    let mut out = Vec::new();
    for _ in 0..k {
        let h: Vec<Info> = rng.sample_indices(infos.len(), size).into_iter().map(|i| infos[i].clone()).collect();
        let mut best: Option<(i64, Vec<usize>)> = None;
        for mut combo in candidates(&h) {
            combo.sort();
            combo.dedup();
            let played: Vec<Info> = combo.iter().map(|&p| h[p].clone()).collect();
            let (name, sc) = classify(&played);
            let s = jokers::score(&h, &combo, name, &sc, &levels, &js, &rnd);
            if best.as_ref().map_or(true, |b| s > b.0) {
                best = Some((s, combo));
            }
        }
        out.push(json!({"infos": h.iter().map(Info::to_json).collect::<Vec<_>>(), "combo": best.unwrap().1,
                        "rnd": {"hands_left": 3, "discards_left": 2}}));
    }
    out
}

/// One value of Picker.values: (value, index, targets). `int` marks Python's int 0 (card_value with no bonus).
pub struct Val {
    pub v: f64,
    pub int: bool,
    pub i: usize,
    pub targets: Option<Vec<usize>>,
}
impl Val {
    pub fn to_json(&self) -> Value {
        let v = if self.int { json!(self.v as i64) } else { json!(self.v) };
        json!([v, self.i, self.targets])
    }
}

pub struct Picker<'a> {
    pub pub_: &'a Value,
    b: Value,
    synth: Vec<Value>,
    memo: HashMap<String, f64>,
}

impl<'a> Picker<'a> {
    pub fn new(pub_: &'a Value) -> Picker<'a> {
        Picker { pub_, b: build::decide(pub_), synth: synth_plays(pub_, SYNTH_HANDS, "SYNTH"), memo: HashMap::new() }
    }

    fn gain(&self, add: Option<&Value>, level_up: Option<(&str, i64, i64)>) -> Option<f64> {
        // Python also swallows exceptions here (unparseable text); the port has none to swallow.
        jokers::gain(self.pub_, &self.synth, add, None, level_up)
    }

    fn joker_value(&mut self, c: &Value) -> f64 {
        let m = gs::d(gs::get(c, "modifier"));
        let eff = gs::get(gs::get(c, "value"), "effect");
        let eff = if gs::truthy(eff) { gs::s(eff) } else { "" };
        let re = Regex::new(r"(?i)destroys? (a random )?Joker").unwrap();
        if gs::truthy(gs::mget(m, "rental")) || gs::truthy(gs::mget(m, "eternal")) || re.is_match(eff) {
            return 0.0;
        }
        let k = format!("j|{}|{}", gs::get(c, "key"), py_str(gs::mget(m, "edition")));
        if !self.memo.contains_key(&k) {
            let base = match self.gain(Some(c), None) { None => 0.05, Some(g) => pmax(g, 0.0) };
            self.memo.insert(k.clone(), base + 0.01);
        }
        self.memo[&k]
    }

    fn planet_value(&mut self, c: &Value) -> f64 {
        let k = format!("p|{}", gs::get(c, "key"));
        if !self.memo.contains_key(&k) {
            let eff = gs::get(gs::get(c, "value"), "effect");
            let lv = jokers::planet_level(if gs::truthy(eff) { gs::s(eff) } else { "" });
            let g = lv.as_ref().and_then(|(h, a, b)| self.gain(None, Some((h.as_str(), *a, *b))));
            let g = g.unwrap_or(0.0); // `g or 0.0`
            let g = if g == 0.0 { 0.0 } else { g };
            self.memo.insert(k.clone(), pmax(g, 0.0) + 0.001);
        }
        self.memo[&k]
    }

    fn consumable_value(&mut self, c: &Value, hand: &[Value], money: &Value, jokers_: &[Value], room: i64) -> (f64, Option<Vec<usize>>) {
        let kv = gs::get(c, "key");
        let key = if gs::truthy(kv) { gs::s(kv) } else { "" };
        if let Some((kind, _n, arg)) = tarots::targeted(key) {
            let p = if hand.is_empty() { None } else { tarots::plan(key, hand, &self.b) };
            let p = match p { None => return (0.0, None), Some(p) => p };
            let mut v = flat(key).unwrap_or(0.02);
            if kind == "enhance" && matches!(arg, Some("MULT" | "BONUS" | "GLASS" | "LUCKY" | "STEEL")) {
                let mk = format!("m|{key}");
                if !self.memo.contains_key(&mk) {
                    let m = tarots::measured(c, self.pub_, &self.synth, &self.b);
                    self.memo.insert(mk.clone(), m.map_or(0.02, |m| pmax(m.1, 0.0)));
                }
                v = self.memo[&mk];
            }
            return (pmax(v, 0.001), Some(p.0));
        }
        if tarots::safe_untargeted(key) {
            if key == "c_hermit" {
                return (pmax(0.001, MONEY * gs::num(money).min(20.0).max(0.0)), None);
            }
            if key == "c_temperance" {
                let s: f64 = jokers_.iter().map(|j| {
                    let x = gs::get(gs::get(j, "cost"), "sell");
                    if gs::truthy(x) { gs::num(x) } else { 0.0 }
                }).sum();
                return (pmax(0.001, MONEY * s.min(50.0)), None);
            }
            if key == "c_judgement" && room < 1 {
                return (0.0, None);
            }
            if key == "c_wheel_of_fortune" && jokers_.is_empty() {
                return (0.0, None);
            }
            return (flat(key).unwrap_or(0.01), None);
        }
        (0.0, None)
    }

    /// (value, is_int): Python's int 0 when the card carries no bonus.
    fn card_value(&self, c: &Value) -> (f64, bool) {
        let i = card_info(c);
        let (e, h, s) = (edition_v(i.edition.as_deref()), enh_v(i.enh.as_deref()), seal_v(i.seal.as_deref()));
        let mut is_int = e.is_none() && h.is_none() && s.is_none();
        let mut v = e.unwrap_or(0.0) + h.unwrap_or(0.0) + s.unwrap_or(0.0);
        let bsuit = gs::get(&self.b, "suit");
        if i.enh_is("WILD") && !gs::truthy(bsuit) {
            v -= 0.01;
            is_int = false;
        }
        if gs::truthy(bsuit) && i.suit.as_deref() != bsuit.as_str() && !i.enh_is("WILD") {
            v *= 0.3;
            is_int = false;
        }
        (v, is_int)
    }

    pub fn values(&mut self, pack: &[Value], hand: &[Value], money: &Value, jokers_: &[Value], joker_limit: i64) -> Vec<Val> {
        let room = joker_limit - jokers_.len() as i64;
        let mut out = Vec::new();
        for (i, c) in pack.iter().enumerate() {
            let mut targets = None;
            let (v, int) = match gs::get(c, "set").as_str() {
                Some("JOKER") => {
                    if room < 1 && gs::get(gs::get(c, "modifier"), "edition").as_str() != Some("NEGATIVE") {
                        continue;
                    }
                    (self.joker_value(c), false)
                }
                Some("PLANET") => (self.planet_value(c), false),
                Some("TAROT") | Some("SPECTRAL") => {
                    let (mut v, t) = self.consumable_value(c, hand, money, jokers_, room);
                    targets = t;
                    if gs::get(c, "key").as_str() == Some("c_black_hole") {
                        v = flat("c_black_hole").unwrap();
                    }
                    (v, false)
                }
                _ => self.card_value(c),
            };
            out.push(Val { v, int, i, targets });
        }
        out
    }

    pub fn pick(&mut self, pack: &[Value], hand: &[Value], money: &Value, jokers_: &[Value], joker_limit: i64) -> Option<(usize, Option<Vec<usize>>)> {
        let mut best: Option<Val> = None;
        for x in self.values(pack, hand, money, jokers_, joker_limit) {
            if x.v > 0.0 && best.as_ref().map_or(true, |b| x.v > b.v) {
                best = Some(x);
            }
        }
        best.map(|b| (b.i, b.targets))
    }

    pub fn pick_pub(&mut self, pub_: &Value) -> Option<(usize, Option<Vec<usize>>)> {
        let lim = gs::get(gs::get(pub_, "jokers"), "limit");
        let m = gs::get(pub_, "money");
        let zero = json!(0);
        self.pick(gs::cards_of(pub_, "pack"), gs::cards_of(pub_, "hand"), if gs::truthy(m) { m } else { &zero },
                  gs::cards_of(pub_, "jokers"), if gs::truthy(lim) { gs::int(lim) } else { 5 })
    }
}

pub fn pick_json(r: Option<(usize, Option<Vec<usize>>)>) -> Value {
    match r { Some((i, t)) => json!([i, t]), None => Value::Null }
}
