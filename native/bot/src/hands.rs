//! Port of src/hands.py (and the hands parts of src/native.py: call jev_kernel directly). See docs/rust-port-guide.md.
//!
//! Also holds `decisions._boss_levels`, `_boss_allows` and `BossAllow` (decisions.py 200-231): the
//! kernel's allow spec (`native.boss_spec`) lives next to the native glue, so they live here; the
//! decisions port imports them from `hands`.
//!
//! Python's `scorer` argument (a `jokers.Scorer`, any other callable, or None) is `ScorerArg`; its
//! `allowed` argument (None, a `BossAllow`, or any other callable) is `Allowed`. A `jokers.Scorer`
//! with `None`/`BossAllow` runs in the kernel when `native::enabled()` and the cards can be encoded,
//! otherwise (and for plain callables) the pure-Python loop runs, exactly as in Python.

use crate::gs;
use crate::jokers;
use serde_json::{json, Map, Value};
use std::collections::HashMap;

pub const RANK_ORDER: &str = "23456789TJQKA";

/// hand_levels / `gs["hands"]`: a dict of hand name -> {chips, mult, played, played_this_round, ...}.
pub type Levels = Map<String, Value>;

/// hands.RANK_NAMES
pub fn rank_name(r: &str) -> &str {
    match r {
        "T" => "10",
        "J" => "Jack",
        "Q" => "Queen",
        "K" => "King",
        "A" => "Ace",
        _ => r,
    }
}

/// hands.SUIT_NAMES
pub fn suit_name(s: &str) -> &str {
    match s {
        "H" => "Hearts",
        "D" => "Diamonds",
        "C" => "Clubs",
        "S" => "Spades",
        _ => s,
    }
}

/// hands.RANK_CHIPS[r] (KeyError for an unknown rank).
pub fn rank_chips(r: &str) -> f64 {
    match r {
        "2" => 2.0,
        "3" => 3.0,
        "4" => 4.0,
        "5" => 5.0,
        "6" => 6.0,
        "7" => 7.0,
        "8" => 8.0,
        "9" => 9.0,
        "T" | "J" | "Q" | "K" => 10.0,
        "A" => 11.0,
        _ => panic!("KeyError: {r:?}"),
    }
}

/// `RANK_ORDER.index(r)` (ValueError for anything that is not one rank character).
pub fn rank_index(r: &str) -> usize {
    match RANK_ORDER.find(r) {
        Some(i) if r.chars().count() == 1 => i,
        _ => panic!("ValueError: substring not found: {r:?}"),
    }
}

/// hands.card_info(c) as a struct: a dict with rank, suit, enh, edition, seal, debuff.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Info {
    pub rank: Option<String>,
    pub suit: Option<String>,
    pub enh: Option<String>,
    pub edition: Option<String>,
    pub seal: Option<String>,
    pub debuff: bool,
}

impl Info {
    /// The rank when it is truthy (a non-empty string), else None (`not i["rank"]`).
    pub fn rank_truthy(&self) -> Option<&str> {
        self.rank.as_deref().filter(|r| !r.is_empty())
    }
    pub fn enh_is(&self, s: &str) -> bool {
        self.enh.as_deref() == Some(s)
    }
    pub fn suit_is(&self, s: &str) -> bool {
        self.suit.as_deref() == Some(s)
    }
    /// An info dict (fixture / sim) back into the struct.
    pub fn from_json(v: &Value) -> Info {
        let o = |k: &str| gs::get(v, k).as_str().map(str::to_string);
        Info { rank: o("rank"), suit: o("suit"), enh: o("enh"), edition: o("edition"), seal: o("seal"),
               debuff: gs::truthy(gs::get(v, "debuff")) }
    }
    pub fn to_json(&self) -> Value {
        json!({"rank": self.rank, "suit": self.suit, "enh": self.enh, "edition": self.edition,
               "seal": self.seal, "debuff": self.debuff})
    }
}

// hands.py 18-20
pub fn _d(x: &Value) -> &Map<String, Value> {
    gs::d(x)
}

// hands.py 23-32
pub fn card_info(c: &Value) -> Info {
    let v = gs::get(c, "value");
    let m = gs::get(c, "modifier");
    let s = gs::get(c, "state");
    let st = |x: &Value, k: &str| gs::get(x, k).as_str().map(str::to_string);
    Info {
        rank: st(v, "rank"),
        suit: st(v, "suit"),
        enh: st(m, "enhancement"),
        edition: st(m, "edition"),
        seal: st(m, "seal"),
        debuff: gs::truthy(gs::get(s, "debuff")),
    }
}

// hands.py 35-46 (`i = card_info(c) if "value" in c else c`)
pub fn describe(c: &Value) -> String {
    if gs::d(c).contains_key("value") {
        describe_info(&card_info(c))
    } else {
        describe_info(&Info::from_json(c))
    }
}

pub fn describe_info(i: &Info) -> String {
    let name = match i.rank_truthy() {
        Some(r) if !i.enh_is("STONE") => {
            // f"{SUIT_NAMES.get(i['suit'], i['suit'])}": suit None prints "None"
            let s = match &i.suit {
                Some(s) => suit_name(s).to_string(),
                None => "None".to_string(),
            };
            format!("{} of {}", rank_name(r), s)
        }
        _ => "Stone card".to_string(),
    };
    let mut extras: Vec<String> = Vec::new();
    let enh = if i.enh_is("STONE") { None } else { i.enh.as_ref() };
    for x in [enh, i.edition.as_ref()].into_iter().flatten() {
        if !x.is_empty() {
            extras.push(x.to_lowercase());
        }
    }
    if let Some(s) = i.seal.as_ref().filter(|s| !s.is_empty()) {
        extras.push(format!("{} seal", s.to_lowercase()));
    }
    if i.debuff {
        extras.push("debuffed".to_string());
    }
    if extras.is_empty() { name } else { format!("{} ({})", name, extras.join(", ")) }
}

// hands.py 49-50
pub fn _rank_value(i: &Info) -> i64 {
    match i.rank_truthy() {
        Some(r) if !i.enh_is("STONE") => rank_index(r) as i64 + 2,
        _ => 10,
    }
}

// hands.py 53-57
pub fn _is_straight(ranks: &[&str]) -> bool {
    let mut idx: Vec<usize> = ranks.iter().map(|r| rank_index(r)).collect();
    idx.sort();
    idx.dedup();
    if idx.len() != 5 {
        return false;
    }
    idx[4] - idx[0] == 4 || idx == [0, 1, 2, 3, 12] // A-2-3-4-5
}

// hands.py 63-72 (the memo is a pure cache: not ported)
/// Return (hand name, positions of scoring cards) for 1-5 card infos.
pub fn classify(cards: &[Info]) -> (&'static str, Vec<usize>) {
    _classify(cards)
}

// hands.py 75-110
pub fn _classify(cards: &[Info]) -> (&'static str, Vec<usize>) {
    let ranked: Vec<usize> = (0..cards.len()).filter(|&p| !cards[p].enh_is("STONE") && cards[p].rank_truthy().is_some()).collect();
    let stones: Vec<usize> = (0..cards.len()).filter(|p| !ranked.contains(p)).collect();
    let rk = |p: usize| cards[p].rank.as_deref().unwrap();
    // Counter(...).values(): only the sorted counts and per-rank counts are read
    let by_rank = |r: &str| ranked.iter().filter(|&&p| rk(p) == r).count();
    let mut counts: Vec<usize> = Vec::new();
    let mut seen: Vec<&str> = Vec::new();
    for &p in &ranked {
        if !seen.contains(&rk(p)) {
            seen.push(rk(p));
            counts.push(by_rank(rk(p)));
        }
    }
    counts.sort_by(|a, b| b.cmp(a));
    if counts.is_empty() {
        counts = vec![0];
    }
    let five = cards.len() == 5 && ranked.len() == 5;
    let flush = five
        && ["H", "D", "C", "S"].iter().any(|s| ranked.iter().all(|&p| cards[p].suit_is(s) || cards[p].enh_is("WILD")));
    let straight = five && _is_straight(&ranked.iter().map(|&p| rk(p)).collect::<Vec<_>>());
    let with_count = |n: usize| -> Vec<usize> { ranked.iter().copied().filter(|&p| by_rank(rk(p)) >= n).collect() };
    let c2 = |a: usize, b: usize| counts.len() >= 2 && counts[0] == a && counts[1] == b;

    let (name, scoring): (&'static str, Vec<usize>) = if counts[0] == 5 {
        (if flush { "Flush Five" } else { "Five of a Kind" }, ranked.clone())
    } else if flush && c2(3, 2) {
        ("Flush House", ranked.clone())
    } else if straight && flush {
        ("Straight Flush", ranked.clone())
    } else if counts[0] == 4 {
        ("Four of a Kind", with_count(4))
    } else if c2(3, 2) {
        ("Full House", ranked.clone())
    } else if flush {
        ("Flush", ranked.clone())
    } else if straight {
        ("Straight", ranked.clone())
    } else if counts[0] == 3 {
        ("Three of a Kind", with_count(3))
    } else if c2(2, 2) {
        ("Two Pair", with_count(2))
    } else if counts[0] == 2 {
        ("Pair", with_count(2))
    } else {
        // max(ranked, key=_rank_value): the first maximal element
        let mut best: Option<usize> = None;
        for &p in &ranked {
            if best.map_or(true, |b| _rank_value(&cards[p]) > _rank_value(&cards[b])) {
                best = Some(p);
            }
        }
        ("High Card", best.into_iter().collect())
    };
    let mut scoring = scoring;
    scoring.extend(stones);
    (name, scoring)
}

// hands.py 113-125
pub fn estimate(cards: &[Info], name: &str, scoring: &[usize], hand_levels: &Levels) -> i64 {
    let h = gs::mget(hand_levels, name);
    let lv = |k: &str, default: f64| match gs::d(h).get(k) {
        Some(v) => gs::num(v),
        None => default,
    };
    let (mut chips, mut mult, mut xmult) = (lv("chips", 0.0), lv("mult", 1.0), 1.0f64);
    for &p in scoring {
        let c = &cards[p];
        if c.debuff {
            continue;
        }
        let ed = |s: &str| c.edition.as_deref() == Some(s);
        for _ in 0..(if c.seal.as_deref() == Some("RED") { 2 } else { 1 }) {
            chips += match c.rank_truthy() {
                Some(r) if !c.enh_is("STONE") => rank_chips(r),
                _ => 50.0,
            };
            let bonus = if c.enh_is("BONUS") { 30 } else { 0 };
            let foil = if ed("FOIL") { 50 } else { 0 };
            chips += (bonus + foil) as f64;
            let m4 = if c.enh_is("MULT") { 4 } else { 0 };
            let holo = if ed("HOLO") { 10 } else { 0 };
            mult += (m4 + holo) as f64;
            let glass = if c.enh_is("GLASS") { 2.0 } else { 1.0 };
            let poly = if ed("POLYCHROME") { 1.5 } else { 1.0 };
            xmult *= glass * poly;
        }
    }
    (chips * mult * xmult) as i64
}

// ---- scorer / allowed kinds ---------------------------------------------------------------

/// Python's `scorer` argument.
#[derive(Clone, Copy)]
pub enum ScorerArg<'a> {
    None,
    /// a `jokers.Scorer` (kernel-capable)
    Jokers(&'a jokers::Scorer),
    /// any other callable `scorer(infos, combo, name, scoring)`
    Func(&'a dyn Fn(&[Info], &[usize], &str, &[usize]) -> i64),
}

/// Python's `allowed` argument.
#[derive(Clone, Copy)]
pub enum Allowed<'a> {
    None,
    /// a `decisions.BossAllow` (kernel-capable)
    Boss(&'a BossAllow),
    /// any other callable `allowed(combo, name)`
    Func(&'a dyn Fn(&[usize], &str) -> bool),
}

impl<'a> ScorerArg<'a> {
    pub fn call(&self, infos: &[Info], combo: &[usize], name: &str, sc: &[usize]) -> i64 {
        match self {
            ScorerArg::None => panic!("no scorer"),
            ScorerArg::Jokers(s) => s.call(infos, combo, name, sc),
            ScorerArg::Func(f) => f(infos, combo, name, sc),
        }
    }
    pub fn is_some(&self) -> bool {
        !matches!(self, ScorerArg::None)
    }
}

impl<'a> Allowed<'a> {
    pub fn call(&self, combo: &[usize], name: &str) -> bool {
        match self {
            Allowed::None => true,
            Allowed::Boss(b) => b.call(combo, name),
            Allowed::Func(f) => f(combo, name),
        }
    }
}

// decisions.py 200-205
pub fn _boss_levels(levels: &Levels, boss: &str) -> Levels {
    if boss == "The Flint" {
        // base chips and mult are halved
        let mut out = Map::new();
        for (n, h) in levels {
            let mut h2 = gs::d(h).clone();
            let chips = h2.get("chips").map_or(0.0, gs::num);
            let mult = h2.get("mult").map_or(1.0, gs::num);
            h2.insert("chips".into(), json!((chips / 2.0 + 0.5) as i64));
            h2.insert("mult".into(), json!(std::cmp::max(1, (mult / 2.0 + 0.5) as i64)));
            out.insert(n.clone(), Value::Object(h2));
        }
        return out;
    }
    levels.clone()
}

// decisions.py 208-216
/// Apply boss restrictions before candidate pruning and to every forecasted hand.
pub fn _boss_allows(combo: &[usize], name: &str, levels: &Levels, boss: &str) -> bool {
    let played: Vec<&String> =
        levels.iter().filter(|(_, h)| gs::truthy(gs::get(h, "played_this_round"))).map(|(n, _)| n).collect();
    if boss == "The Psychic" {
        return combo.len() == 5;
    }
    if boss == "The Eye" {
        return !played.iter().any(|n| n.as_str() == name);
    }
    if boss == "The Mouth" && !played.is_empty() {
        return played.iter().any(|n| n.as_str() == name);
    }
    true
}

// decisions.py 219-229
/// `_boss_allows` bound to one state: allowed(combo, name).
#[derive(Clone, Debug)]
pub struct BossAllow {
    pub levels: Levels,
    pub boss: String,
}

impl BossAllow {
    pub fn new(levels: &Levels, boss: &str) -> BossAllow {
        BossAllow { levels: levels.clone(), boss: boss.to_string() }
    }
    pub fn call(&self, combo: &[usize], name: &str) -> bool {
        _boss_allows(combo, name, &self.levels, &self.boss)
    }
    pub fn native_spec(&self) -> Option<(u8, u32, bool)> {
        native::boss_spec(&self.levels, &self.boss)
    }
}

// ---- hands.py 128-159 ---------------------------------------------------------------------

/// One row of `evaluated_plays`: (score, hand name, combo, scoring set as sorted positions).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Play {
    pub score: i64,
    pub name: &'static str,
    pub combo: Vec<usize>,
    pub scoring: Vec<usize>,
}

/// itertools.combinations(range(n), r), in order.
pub fn combinations(n: usize, r: usize) -> Vec<Vec<usize>> {
    let mut out = Vec::new();
    if r > n {
        return out;
    }
    let mut combo: Vec<usize> = (0..r).collect();
    loop {
        out.push(combo.clone());
        let mut i = r;
        while i > 0 && combo[i - 1] == i - 1 + n - r {
            i -= 1;
        }
        if i == 0 {
            break;
        }
        combo[i - 1] += 1;
        for j in i..r {
            combo[j] = combo[j - 1] + 1;
        }
    }
    out
}

// hands.py 128-143
/// Score every subset before pruning; held cards and play length can change its value.
pub fn evaluated_plays(infos: &[Info], hand_levels: &Levels, scorer: ScorerArg, allowed: Allowed) -> Vec<Play> {
    if let Some(rows) = native::evaluate(scorer, infos, allowed) {
        return rows;
    }
    let mut out = Vec::new();
    for r in 1..=std::cmp::min(5, infos.len()) {
        for combo in combinations(infos.len(), r) {
            let cs: Vec<Info> = combo.iter().map(|&i| infos[i].clone()).collect();
            let (name, sc) = classify(&cs);
            if !allowed.call(&combo, name) {
                continue;
            }
            let score = if scorer.is_some() { scorer.call(infos, &combo, name, &sc) } else { estimate(&cs, name, &sc, hand_levels) };
            let mut scoring: Vec<usize> = sc.iter().map(|&p| combo[p]).collect();
            scoring.sort();
            scoring.dedup();
            out.push(Play { score, name, combo, scoring });
        }
    }
    out
}

// hands.py 146-151
/// Best modeled score in a complete future hand, including cards left held.
pub fn best_score(infos: &[Info], scorer: ScorerArg, allowed: Allowed) -> i64 {
    if let Some(best) = native::best_score(scorer, infos, allowed) {
        return best;
    }
    // max(..., default=0)
    let mut best: Option<i64> = None;
    for row in evaluated_plays(infos, &Levels::new(), scorer, allowed) {
        if best.map_or(true, |b| row.score > b) {
            best = Some(row.score);
        }
    }
    best.unwrap_or(0)
}

// hands.py 154-159
/// max((score, hand name)) over the plays, (0, None) without any: ties go to the larger name.
pub fn best_play(infos: &[Info], hand_levels: &Levels, scorer: ScorerArg, allowed: Allowed) -> (i64, Option<&'static str>) {
    if let Some(best) = native::best_play(scorer, infos, allowed) {
        return best;
    }
    let mut best: Option<(i64, &'static str)> = None;
    for r in evaluated_plays(infos, hand_levels, scorer, allowed) {
        if best.map_or(true, |(s, n)| (r.score, r.name) > (s, n)) {
            best = Some((r.score, r.name));
        }
    }
    best.map_or((0, None), |(s, n)| (s, Some(n)))
}

// ---- hands.py 162-198 ---------------------------------------------------------------------

/// One entry of `play_candidates`' result.
#[derive(Clone, Debug, PartialEq)]
pub struct PlayCand {
    pub hand: &'static str,
    pub cards: Vec<usize>,
    pub estimate: i64,
    pub scoring: Vec<String>,
    pub kicked: Vec<String>,
}

impl PlayCand {
    pub fn to_json(&self) -> Value {
        json!({"hand": self.hand, "cards": self.cards, "estimate": self.estimate,
               "scoring": self.scoring, "kicked": self.kicked})
    }
}

type Row = (i64, &'static str, Vec<usize>, Vec<usize>);
type Best = Vec<((&'static str, Vec<usize>), (i64, (i64, usize, i64), Vec<usize>))>;

// hands.py 162-198
/// Best play per (hand type, scoring cards); cycle low junk only when scores tie.
pub fn play_candidates(hand_cards: &[Value], hand_levels: &Levels, limit: usize, scorer: ScorerArg, allowed: Allowed) -> Vec<PlayCand> {
    let infos: Vec<Info> = hand_cards.iter().map(card_info).collect();
    // best: dict keyed by (name, scoring) in insertion order -> (est, pref, combo)
    let mut best: Best = Vec::new();
    let mut index: HashMap<(&'static str, Vec<usize>), usize> = HashMap::new();
    for Play { score: est, name, combo, scoring } in evaluated_plays(&infos, hand_levels, scorer, allowed) {
        let junk_sum: i64 = combo.iter().filter(|i| !scoring.contains(i)).map(|&i| _rank_value(&infos[i])).sum();
        let pref = (est, combo.len(), -junk_sum);
        let key = (name, scoring);
        match index.get(&key) {
            Some(&at) => {
                if pref > best[at].1 .1 {
                    best[at].1 = (est, pref, combo);
                }
            }
            None => {
                index.insert(key.clone(), best.len());
                best.push((key, (est, pref, combo)));
            }
        }
    }
    let mut ranked: Vec<Row> = best.into_iter().map(|(k, v)| (v.0, k.0, v.2, k.1)).collect();
    ranked.sort_by(|a, b| b.0.cmp(&a.0)); // sorted(key=-x[0]): stable
    // One best play per hand type first, then fill with the next best overall.
    let mut picked: Vec<Row> = Vec::new();
    let mut seen: Vec<&str> = Vec::new();
    for row in &ranked {
        if !seen.contains(&row.1) {
            seen.push(row.1);
            picked.push(row.clone());
        }
    }
    let mut per_type: HashMap<&str, usize> = HashMap::new();
    for row in &picked {
        *per_type.entry(row.1).or_insert(0) += 1;
    }
    for row in &ranked {
        if picked.len() >= limit {
            break;
        }
        if !picked.contains(row) && per_type.get(row.1).copied().unwrap_or(0) < 2 {
            *per_type.entry(row.1).or_insert(0) += 1;
            picked.push(row.clone());
        }
    }
    picked.sort_by(|a, b| b.0.cmp(&a.0));
    picked.truncate(limit);
    picked
        .into_iter()
        .map(|(score, name, idxs, scoring)| PlayCand {
            hand: name,
            estimate: score,
            scoring: idxs.iter().filter(|i| scoring.contains(i)).map(|&i| describe(&hand_cards[i])).collect(),
            kicked: idxs.iter().filter(|i| !scoring.contains(i)).map(|&i| describe(&hand_cards[i])).collect(),
            cards: idxs,
        })
        .collect()
}

// ---- hands.py 201-250 ---------------------------------------------------------------------

/// The goal a discard plan carries for odds.annotate(): ("flush", s) / ("sets", ranks) /
/// ("straight", missing ranks) / ("high", ranks). Python sets are sorted vectors here (None first).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Goal {
    Flush(String),
    Sets(Vec<Option<String>>),
    Straight(Vec<String>),
    High(Vec<Option<String>>),
}

impl Goal {
    pub fn to_json(&self) -> Value {
        match self {
            Goal::Flush(s) => json!(["flush", s]),
            Goal::Sets(r) => json!(["sets", r]),
            Goal::Straight(m) => json!(["straight", m]),
            Goal::High(r) => json!(["high", r]),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DiscardPlan {
    pub label: String,
    pub cards: Vec<usize>,
    pub goal: Goal,
    pub tossed: Vec<String>,
}

impl DiscardPlan {
    pub fn to_json(&self) -> Value {
        json!({"label": self.label, "cards": self.cards, "goal": self.goal.to_json(), "tossed": self.tossed})
    }
}

fn rank_set(ranks: impl Iterator<Item = Option<String>>) -> Vec<Option<String>> {
    let mut v: Vec<Option<String>> = ranks.collect();
    v.sort(); // None sorts first
    v.dedup();
    v
}

// hands.py 201-250
/// Discard plans. Debuffed cards (boss effects) count as junk: never kept for a draw, tossed first.
/// Each plan carries a goal for odds.annotate().
pub fn discard_candidates(hand_cards: &[Value], max_discard: usize) -> Vec<DiscardPlan> {
    let infos: Vec<Info> = hand_cards.iter().map(card_info).collect();
    let allidx: Vec<usize> = (0..infos.len()).collect();
    let live: Vec<usize> = allidx.iter().copied().filter(|&i| !infos[i].debuff).collect();

    let toss = |keep: &[usize], label: String, goal: Goal| -> (String, Vec<usize>, Goal) {
        let mut others: Vec<usize> = allidx.iter().copied().filter(|i| !keep.contains(i)).collect();
        others.sort_by_key(|&i| (!infos[i].debuff, _rank_value(&infos[i])));
        others.truncate(max_discard);
        (label, others, goal)
    };

    let mut out: Vec<(String, Vec<usize>, Goal)> = Vec::new();
    for s in ["H", "D", "C", "S"] {
        let keep: Vec<usize> = live.iter().copied().filter(|&i| infos[i].suit_is(s) || infos[i].enh_is("WILD")).collect();
        if 3 <= keep.len() && keep.len() < 5 {
            // 5+ means the flush is already playable
            out.push(toss(&keep, format!("Chase a {} flush (holding {} of 5)", suit_name(s), keep.len()), Goal::Flush(s.to_string())));
        }
    }

    let rk = |i: usize| infos[i].rank_truthy();
    let by_rank = |r: &str| live.iter().filter(|&&i| rk(i) == Some(r)).count();
    let sets: Vec<usize> = live.iter().copied().filter(|&i| rk(i).map_or(false, |r| by_rank(r) >= 2)).collect();
    if !sets.is_empty() {
        let goal = Goal::Sets(rank_set(sets.iter().map(|&i| infos[i].rank.clone())));
        out.push(toss(&sets, "Keep pairs/sets, dig for more of a kind".to_string(), goal));
    }

    // Straight draw: the 5-rank window holding the most distinct ranks (A counts low too).
    let mut best_keep: Vec<usize> = Vec::new();
    let mut best_window: Vec<String> = Vec::new();
    let ro: Vec<char> = RANK_ORDER.chars().collect();
    for lo in -1i32..9 {
        let window: Vec<String> = if lo >= 0 {
            (0..5).map(|k| ro[(lo as usize + k) % 13].to_string()).collect()
        } else {
            "A2345".chars().map(|c| c.to_string()).collect()
        };
        let mut keep: Vec<usize> = Vec::new();
        let mut used: Vec<&str> = Vec::new();
        for &i in &live {
            if let Some(r) = infos[i].rank.as_deref() {
                if window.iter().any(|w| w == r) && !used.contains(&r) {
                    keep.push(i);
                    used.push(r);
                }
            }
        }
        if keep.len() > best_keep.len() {
            best_keep = keep;
            best_window = window;
        }
    }
    if 3 <= best_keep.len() && best_keep.len() < 5 {
        // 5 means the straight is already playable
        let have: Vec<&str> = best_keep.iter().filter_map(|&i| infos[i].rank.as_deref()).collect();
        let mut missing: Vec<String> = Vec::new();
        for w in &best_window {
            if !have.contains(&w.as_str()) && !missing.contains(w) {
                missing.push(w.clone());
            }
        }
        missing.sort_by_key(|r| rank_index(r));
        out.push(toss(&best_keep, format!("Chase a straight (holding {} of 5 ranks)", best_keep.len()), Goal::Straight(missing)));
    }

    let mut high = live.clone();
    high.sort_by(|&a, &b| _rank_value(&infos[b]).cmp(&_rank_value(&infos[a]))); // key=-rank value: stable
    high.truncate(allidx.len().saturating_sub(max_discard));
    let goal = Goal::High(rank_set(high.iter().map(|&i| infos[i].rank.clone())));
    out.push(toss(&high, "Keep high cards, throw the lowest".to_string(), goal));

    let mut seen: Vec<Vec<usize>> = Vec::new();
    let mut result = Vec::new();
    for (label, cards, goal) in out {
        let mut key = cards.clone();
        key.sort();
        key.dedup();
        if !cards.is_empty() && !seen.contains(&key) {
            seen.push(key);
            let tossed = cards.iter().map(|&i| describe(&hand_cards[i])).collect();
            result.push(DiscardPlan { label, cards, goal, tossed });
        }
    }
    result
}

// ---- native.py ----------------------------------------------------------------------------

/// src/native.py: encodes inputs for `jev_kernel` and calls it. Every function returns None when
/// the Python path must run instead (disabled, or an input the kernel can't take exactly).
pub mod native {
    use super::*;
    use jev_kernel::{CardIn, HAND_NAMES};
    use std::sync::atomic::{AtomicU8, Ordering};

    // 0 = unset (read JEV_NATIVE), 1 = enabled, 2 = disabled
    static STATE: AtomicU8 = AtomicU8::new(0);

    /// native.ENABLED (the kernel is always built in Rust; JEV_NATIVE=0 turns it off).
    pub fn enabled() -> bool {
        match STATE.load(Ordering::Relaxed) {
            1 => true,
            2 => false,
            _ => std::env::var("JEV_NATIVE").map_or(true, |v| v != "0"),
        }
    }

    /// Force the kernel on or off (the Python path is the reference check).
    pub fn set_enabled(on: bool) {
        STATE.store(if on { 1 } else { 2 }, Ordering::Relaxed);
    }

    fn rank_code(r: &str) -> Option<u8> {
        RANK_ORDER.find(r).filter(|_| r.chars().count() == 1).map(|i| i as u8 + 2)
    }
    pub fn suit_code(s: Option<&str>) -> Option<u8> {
        match s {
            None => Some(0),
            Some("H") => Some(1),
            Some("D") => Some(2),
            Some("C") => Some(3),
            Some("S") => Some(4),
            _ => None,
        }
    }
    pub fn enh_code(e: Option<&str>) -> u8 {
        match e {
            None => 0,
            Some("STONE") => 1,
            Some("WILD") => 2,
            Some("BONUS") => 3,
            Some("MULT") => 4,
            Some("GLASS") => 5,
            Some("STEEL") => 6,
            _ => 7,
        }
    }
    pub fn edition_code(e: Option<&str>) -> u8 {
        match e {
            None => 0,
            Some("FOIL") => 1,
            Some("HOLO") => 2,
            Some("POLYCHROME") => 3,
            _ => 4,
        }
    }
    pub fn hand_index(name: &str) -> Option<usize> {
        HAND_NAMES.iter().position(|n| *n == name)
    }

    // native.py 47-61
    /// [(rank, suit, enh, edition, red seal, debuff)] or None when a card can't be encoded exactly.
    pub fn encode_cards(infos: &[Info]) -> Option<Vec<CardIn>> {
        let mut out = Vec::new();
        for i in infos {
            let rank = match i.rank_truthy() {
                None => 0, // `0 if not r`
                Some(r) => rank_code(r)?,
            };
            let suit = suit_code(i.suit.as_deref())?;
            out.push((rank, suit, enh_code(i.enh.as_deref()), edition_code(i.edition.as_deref()), i.seal.as_deref() == Some("RED"), i.debuff));
        }
        Some(out)
    }

    // native.py 64-68
    fn mask_ranks(values: &[char]) -> u32 {
        values.iter().fold(0, |m, c| m | 1 << rank_code(&c.to_string()).unwrap())
    }

    // native.py 71-83
    fn pred(p: &jokers::Pred) -> (u8, u32) {
        match p {
            jokers::Pred::FirstFace => (3, 0),
            jokers::Pred::First => (4, 0),
            jokers::Pred::Final => (5, 0),
            jokers::Pred::Suit(s) => (1, suit_code(Some(&s.to_string())).unwrap() as u32),
            jokers::Pred::Ranks(r) => (2, mask_ranks(r)),
            // v28: one rank of one suit (The Idol)
            jokers::Pred::Card(r, s) => (6, rank_code(&r.to_string()).unwrap() as u32 | (suit_code(Some(&s.to_string())).unwrap() as u32) << 8),
            jokers::Pred::All => (0, 0),
        }
    }

    // native.py 93-105
    fn hand(spec: &jokers::HandSpec) -> jev_kernel::HandIn {
        use jokers::{Cond, HandSpec};
        let one = (0.0, 0.0, 1.0);
        match spec {
            HandSpec::Const(e) => (0, 0, 0.0, *e, *e),
            HandSpec::Cond { cond, yes, no } => {
                let (k, mask) = match cond {
                    Cond::Names(names) => (1, names.iter().fold(0u32, |m, n| m | 1 << hand_index(n).unwrap())),
                    Cond::Le3 => (2, 0),
                    Cond::Final => (3, 0),
                    Cond::NoDiscards => (4, 0),
                    Cond::HeldSuits(suits) => (
                        5,
                        suits.iter().fold(0u32, |m, s| m | 1 << suit_code(s.map(|c| c.to_string()).as_deref()).unwrap()),
                    ),
                    Cond::FourSuits => (6, 0),
                    Cond::PlayedThisRound => (7, 0),
                    Cond::SeeingDouble => (11, 0),
                };
                (k, mask, 0.0, *yes, *no)
            }
            HandSpec::TimesPlayed => (8, 0, 0.0, one, one),
            HandSpec::PerDiscard(v) => (9, 0, *v, one, one),
            HandSpec::LowestHeld => (10, 0, 0.0, one, one),
        }
    }

    // native.py 108-115
    pub fn rule(r: &Option<jokers::Rule>) -> Option<jev_kernel::RuleIn> {
        let r = r.as_ref()?;
        Some((
            r.card.iter().map(|(p, c, m, x)| { let (k, a) = pred(p); (k, a, *c, *m, *x) }).collect(),
            r.retrigger.iter().map(|(p, t)| { let (k, a) = pred(p); (k, a, *t) }).collect(),
            r.held.iter().map(|(p, m, x)| { let (k, a) = pred(p); (k, a, *m, *x) }).collect(),
            r.held_retrigger.unwrap_or(0),
            r.hand.as_ref().map(hand),
        ))
    }

    /// float(v) for a levels value (number or bool); None where Python raises TypeError.
    fn pyfloat_of(v: Option<&Value>, default: f64) -> Option<f64> {
        match v {
            None => Some(default),
            Some(Value::Number(n)) => n.as_f64(),
            Some(Value::Bool(b)) => Some(*b as i64 as f64),
            _ => None,
        }
    }

    // native.py 118-133
    /// A native jokers.Scorer, or None (disabled, or inputs it can't take exactly).
    pub fn scorer(hand_levels: &Levels, jokers: &[jokers::JokerEntry], rnd: &jokers::Rnd) -> Option<jev_kernel::Scorer> {
        if !enabled() {
            return None;
        }
        let mut levels = Vec::new();
        for n in HAND_NAMES {
            let h = hand_levels.get(n).unwrap_or(&gs::NULL); // hand_levels.get(n, {})
            let g = |k: &str, d: f64| pyfloat_of(gs::d(h).get(k), d);
            levels.push((g("chips", 0.0)?, g("mult", 1.0)?, g("played", 0.0)?, gs::truthy(gs::get(h, "played_this_round"))));
        }
        let js = jokers.iter().map(|j| (edition_code(j.edition.as_deref()), rule(&j.rule))).collect();
        jev_kernel::Scorer::new(levels, js, rnd.hands_left, rnd.discards_left).ok()
    }

    // native.py 136-142
    /// (boss code, played-hand mask, any hand played) for a BossAllow, (0, 0, False) for no filter,
    /// None for a filter the kernel can't evaluate.
    pub fn allow_spec(allowed: Allowed) -> Option<(u8, u32, bool)> {
        match allowed {
            Allowed::None => Some((0, 0, false)),
            Allowed::Boss(b) => b.native_spec(),
            Allowed::Func(_) => None,
        }
    }

    // native.py 145-150
    pub fn boss_spec(levels: &Levels, boss: &str) -> Option<(u8, u32, bool)> {
        let played: Vec<&String> =
            levels.iter().filter(|(_, h)| gs::truthy(gs::get(h, "played_this_round"))).map(|(n, _)| n).collect();
        let code = match boss {
            "The Psychic" => 1,
            "The Eye" => 2,
            "The Mouth" => 3,
            _ => 0,
        };
        let mask = played.iter().filter_map(|n| hand_index(n)).fold(0u32, |m, i| m | 1 << i);
        Some((code, mask, !played.is_empty()))
    }

    // native.py 153-162
    fn inputs<'a>(scorer: ScorerArg<'a>, infos: &[Info], allowed: Allowed) -> Option<(&'a jev_kernel::Scorer, Vec<jev_kernel::Card>, (u8, u32, bool))> {
        if !enabled() {
            return None;
        }
        let ns = match scorer {
            ScorerArg::Jokers(s) => s.native()?,
            _ => return None,
        };
        let cards = encode_cards(infos);
        let allow = allow_spec(allowed);
        let (cards, allow) = (cards?, allow?);
        Some((ns, jev_kernel::cards_of(&cards), allow))
    }

    // native.py 165-170
    /// hands.evaluated_plays rows, or None for the Python path.
    pub fn evaluate(scorer: ScorerArg, infos: &[Info], allowed: Allowed) -> Option<Vec<Play>> {
        let (ns, cards, allow) = inputs(scorer, infos, allowed)?;
        let mut out = Vec::new();
        jev_kernel::each_play(ns, &cards, allow, |s, n, combo, sc| {
            let mut scoring: Vec<usize> = sc.iter().map(|&p| combo[p]).collect();
            scoring.sort();
            scoring.dedup();
            out.push(Play { score: s as i64, name: HAND_NAMES[n], combo: combo.to_vec(), scoring });
        });
        Some(out)
    }

    // native.py 173-175
    pub fn best_score(scorer: ScorerArg, infos: &[Info], allowed: Allowed) -> Option<i64> {
        let (ns, cards, allow) = inputs(scorer, infos, allowed)?;
        let mut best: Option<f64> = None;
        jev_kernel::each_play(ns, &cards, allow, |score, _, _, _| {
            if best.map_or(true, |b| score > b) {
                best = Some(score);
            }
        });
        Some(best.unwrap_or(0.0) as i64)
    }

    // native.py 178-184
    /// max((score, name)) over the allowed plays, default (0, None); None for the Python path.
    pub fn best_play(scorer: ScorerArg, infos: &[Info], allowed: Allowed) -> Option<(i64, Option<&'static str>)> {
        let (ns, cards, allow) = inputs(scorer, infos, allowed)?;
        let mut best: Option<(f64, usize)> = None;
        jev_kernel::each_play(ns, &cards, allow, |score, name, _, _| {
            let better = match best {
                None => true,
                Some((bs, bn)) => score > bs || (score == bs && HAND_NAMES[name] > HAND_NAMES[bn]),
            };
            if better {
                best = Some((score, name));
            }
        });
        Some(best.map_or((0, None), |(s, n)| (s as i64, Some(HAND_NAMES[n]))))
    }

    // native.py 187-196
    /// [jokers.score(infos, combo, name, scoring)] for plays [(combo, name, scoring)], or None.
    pub fn score_many(scorer: ScorerArg, infos: &[Info], plays: &[(Vec<usize>, &str, Vec<usize>)]) -> Option<Vec<i64>> {
        let (ns, cards, _) = inputs(scorer, infos, Allowed::None)?;
        let mut enc = Vec::new();
        for (combo, name, sc) in plays {
            enc.push((combo, hand_index(name)?, sc));
        }
        Some(enc.iter().map(|(combo, name, sc)| ns.score(&cards, combo, *name, sc) as i64).collect())
    }
}
