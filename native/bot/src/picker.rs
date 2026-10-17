//! rollout_items.py 56-212: `_cards`, `usable`, `synth_plays` and `Picker` (which pack card to take).
//! `usable` lives in forecast_stats.rs (it uses tarots.rs's key lists); `_cards` is `gs::cards_of`.

use crate::hands::{best_score, card_info, classify, rank_index, Allowed, Info, ScorerArg};
use crate::jokers::{self, Rnd};
use crate::{build, gs, tarots};
use pycompat::json::pyrepr;
use pycompat::Random;
use regex::Regex;
use serde_json::{json, Value};
use std::collections::HashMap;

const MONEY: f64 = 0.01;
pub const SYNTH_HANDS: usize = 12;
pub const PLANET_DEALS: usize = 24; // v28: deals for a planet's value (hand size + PLANET_EXTRA cards each)
pub const PLANET_EXTRA: i64 = 3; // roughly what a discard adds, as in readiness.play_mix

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

// rollout_items.py 75-80
/// The deck multiset (cards + hand), canonically sorted so the real draw order is never read.
fn _deck_infos(pub_: &Value) -> Vec<Info> {
    let mut deck: Vec<Value> = gs::cards_of(pub_, "cards").to_vec();
    deck.extend(gs::cards_of(pub_, "hand").iter().cloned());
    let key = |c: &Value| {
        let v = gs::get(c, "value");
        (py_str(gs::get(v, "suit")), py_str(gs::get(v, "rank")), pyrepr(&Value::Object(gs::d(gs::get(c, "modifier")).clone())))
    };
    deck.sort_by_key(key);
    deck.iter().map(|c| { let mut i = card_info(c); i.debuff = false; i }).collect()
}

// rollout_items.py 83-104
/// v28: mean best-play score ratio - 1 after levelling lv=(hand, chips, mult), with the best play
/// re-chosen in each deal (Rust kernel), over k deals of hand size + `extra` cards from the deck multiset.
pub fn planet_gain(pub_: &Value, lv: (&str, i64, i64), k: usize, extra: i64, seed: &str) -> f64 {
    let infos = _deck_infos(pub_);
    let lim = gs::get(gs::get(pub_, "hand"), "limit");
    let size = ((if gs::truthy(lim) { gs::int(lim) } else { 8 }) + extra).min(infos.len() as i64);
    if size < 1 {
        return 0.0;
    }
    let js = jokers::build(pub_, None);
    let levels = gs::d(gs::get(pub_, "hands")).clone();
    let (h, c, m) = lv;
    let mut new = levels.clone();
    let mut cur = gs::d(levels.get(h).unwrap_or(&Value::Null)).clone();
    let chips = add_num(cur.get("chips"), 0, c);
    let mult = add_num(cur.get("mult"), 1, m);
    cur.insert("chips".to_string(), chips);
    cur.insert("mult".to_string(), mult);
    new.insert(h.to_string(), Value::Object(cur));
    let rnd = Rnd { hands_left: 2.0, discards_left: 1.0 };
    let (s0, s1) = (jokers::Scorer::new(levels, js.clone(), rnd), jokers::Scorer::new(new, js, rnd));
    let mut rng = Random::new_str(seed);
    let (mut before, mut after) = (0.0f64, 0.0f64);
    for _ in 0..k {
        let hand: Vec<Info> = rng.sample_indices(infos.len(), size as usize).into_iter().map(|i| infos[i].clone()).collect();
        before += best_score(&hand, ScorerArg::Jokers(&s0), Allowed::None) as f64;
        after += best_score(&hand, ScorerArg::Jokers(&s1), Allowed::None) as f64;
    }
    if before != 0.0 { after / before - 1.0 } else { 0.0 }
}

/// `planet_gain` for many planets at the cost of about one: the deals are scored once at the current
/// levels, keeping each deal's best score per hand type and its plays grouped by type. Levelling hand h
/// changes only the scores of plays of type h (the levels row of the played hand is the only one a score
/// reads, and joker rules do not depend on levels), so a planet's "after" for a deal is the larger of the
/// other types' best and the rescored type-h plays. Same floats, same order as `planet_gain`; it falls
/// back to `planet_gain` where the native kernel cannot score (JEV_NATIVE=0, unmodelled joker text).
pub struct PlanetDeals {
    native: Option<NativeDeals>,
    k: usize,
    extra: i64,
    seed: String,
}

struct NativeDeals {
    js: Vec<jokers::JokerEntry>,
    levels: crate::hands::Levels,
    /// sum over deals of the best score at the current levels (`before` in planet_gain)
    before: f64,
    deals: Vec<Deal>,
}

struct Deal {
    cards: Vec<jev_kernel::Card>,
    /// best score per hand type at the current levels
    best: [Option<f64>; 12],
    /// the plays that can be best, grouped by hand type: (positions, scoring positions)
    plays: [Vec<(Small, Small)>; 12],
}

/// Up to 5 positions, inline.
#[derive(Clone, Copy)]
struct Small([usize; 5], usize);
impl Small {
    fn of(v: &[usize]) -> Small {
        let mut a = [0; 5];
        a[..v.len()].copy_from_slice(v);
        Small(a, v.len())
    }
    fn get(&self) -> &[usize] {
        &self.0[..self.1]
    }
}

impl PlanetDeals {
    pub fn new(pub_: &Value, k: usize, extra: i64, seed: &str) -> PlanetDeals {
        PlanetDeals { native: Self::native_deals(pub_, k, extra, seed), k, extra, seed: seed.to_string() }
    }

    fn native_deals(pub_: &Value, k: usize, extra: i64, seed: &str) -> Option<NativeDeals> {
        let infos = _deck_infos(pub_);
        let lim = gs::get(gs::get(pub_, "hand"), "limit");
        let size = ((if gs::truthy(lim) { gs::int(lim) } else { 8 }) + extra).min(infos.len() as i64);
        if size < 1 {
            return None;
        }
        let js = jokers::build(pub_, None);
        let levels = gs::d(gs::get(pub_, "hands")).clone();
        let s0 = jokers::Scorer::new(levels.clone(), js.clone(), Rnd { hands_left: 2.0, discards_left: 1.0 });
        let ns = s0.native()?;
        let mut rng = Random::new_str(seed);
        let (mut before, mut deals) = (0.0f64, Vec::with_capacity(k));
        for _ in 0..k {
            let hand: Vec<Info> = rng.sample_indices(infos.len(), size as usize).into_iter().map(|i| infos[i].clone()).collect();
            let cards = jev_kernel::cards_of(&crate::hands::native::encode_cards(&hand)?);
            let mut d = Deal { cards: Vec::new(), best: [None; 12], plays: Default::default() };
            // the best play per type, and the type-h plays a planet rescores, are among these (a kicker never
            // matters where they are pruned, at either level)
            jev_kernel::each_best_candidate(ns, &cards, (0, 0, false), |score, name, combo, sc| {
                if d.best[name].map_or(true, |b| score > b) {
                    d.best[name] = Some(score);
                }
                d.plays[name].push((Small::of(combo), Small::of(sc)));
            });
            d.cards = cards;
            // best_score: max over every play, 0 without one, truncated to an int
            before += d.best.iter().flatten().fold(None, |m: Option<f64>, &s| Some(m.map_or(s, |m| m.max(s)))).unwrap_or(0.0) as i64 as f64;
            deals.push(d);
        }
        Some(NativeDeals { js, levels, before, deals })
    }

    /// `planet_gain(pub_, lv, k, extra, seed)`.
    pub fn gain(&self, pub_: &Value, lv: (&str, i64, i64)) -> f64 {
        let (h, c, m) = lv;
        let (Some(nd), Some(hi)) = (self.native.as_ref(), jev_kernel::HAND_NAMES.iter().position(|n| *n == h)) else {
            return planet_gain(pub_, lv, self.k, self.extra, &self.seed);
        };
        let mut new = nd.levels.clone();
        let mut cur = gs::d(nd.levels.get(h).unwrap_or(&Value::Null)).clone();
        let chips = add_num(cur.get("chips"), 0, c);
        let mult = add_num(cur.get("mult"), 1, m);
        cur.insert("chips".to_string(), chips);
        cur.insert("mult".to_string(), mult);
        new.insert(h.to_string(), Value::Object(cur));
        let s1 = jokers::Scorer::new(new, nd.js.clone(), Rnd { hands_left: 2.0, discards_left: 1.0 });
        let Some(ns1) = s1.native() else { return planet_gain(pub_, lv, self.k, self.extra, &self.seed) };
        let mut after = 0.0f64;
        for d in &nd.deals {
            let mut best: Option<f64> = None;
            for (n, b) in d.best.iter().enumerate() {
                if let Some(&s) = b.as_ref().filter(|_| n != hi) {
                    best = Some(best.map_or(s, |m: f64| m.max(s)));
                }
            }
            for (combo, sc) in &d.plays[hi] {
                let s = ns1.score(&d.cards, combo.get(), hi, sc.get());
                best = Some(best.map_or(s, |m: f64| m.max(s)));
            }
            after += best.unwrap_or(0.0) as i64 as f64;
        }
        if nd.before != 0.0 { after / nd.before - 1.0 } else { 0.0 }
    }
}

/// `d.get(k, default) + c`, keeping int or float as Python does.
fn add_num(cur: Option<&Value>, default: i64, c: i64) -> Value {
    match cur {
        None => json!(default + c),
        Some(Value::Number(n)) if n.is_i64() => json!(n.as_i64().unwrap() + c),
        Some(v) => json!(gs::num(v) + c as f64),
    }
}

/// Recent-play stand-ins: `{"infos", "combo", "rnd"}` dicts (jokers::sample_plays input).
pub fn synth_plays(pub_: &Value, k: usize, seed: &str) -> Vec<Value> {
    let infos = _deck_infos(pub_);
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
    /// the planet deals, built at the first planet this Picker values
    planets: Option<PlanetDeals>,
}

impl<'a> Picker<'a> {
    pub fn new(pub_: &'a Value) -> Picker<'a> {
        Picker { pub_, b: build::decide(pub_), synth: synth_plays(pub_, SYNTH_HANDS, "SYNTH"), memo: HashMap::new(), planets: None }
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
            // v28: planet_gain re-chooses the best play per deal (gain kept each sample's play fixed)
            let (pub_, planets) = (self.pub_, &mut self.planets);
            let g = lv.as_ref().map(|(h, a, b)| {
                planets.get_or_insert_with(|| PlanetDeals::new(pub_, PLANET_DEALS, PLANET_EXTRA, "PLANET")).gain(pub_, (h.as_str(), *a, *b))
            });
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
