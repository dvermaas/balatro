#![allow(dead_code)]
//! The kernel as it was before the allocation-free rewrite (classify_into, flattened rules), with the
//! joker rules of v31 (pred kind 6, Seeing Double), kept as the reference the fast version must match
//! bit for bit.
use jev_kernel::{rank_chips, Card, HandIn, RuleIn, HAND_NAMES, STONE, WILD};
pub type Eff = (f64, f64, f64);
const BONUS: u8 = 3;
const MULT: u8 = 4;
const GLASS: u8 = 5;
const STEEL: u8 = 6;
const FOIL: u8 = 1;
const HOLO: u8 = 2;
const POLYCHROME: u8 = 3;
const CLUBS: u32 = 3;
const FLUSH_FIVE: usize = 0;
const FLUSH_HOUSE: usize = 1;
const FIVE_KIND: usize = 2;
const STRAIGHT_FLUSH: usize = 3;
const FOUR_KIND: usize = 4;
const FULL_HOUSE: usize = 5;
const FLUSH: usize = 6;
const STRAIGHT: usize = 7;
const THREE_KIND: usize = 8;
const TWO_PAIR: usize = 9;
const PAIR: usize = 10;
const HIGH_CARD: usize = 11;

fn nominal(c: &Card) -> f64 {
    if c.enh == STONE || c.rank == 0 {
        0.0
    } else {
        rank_chips(c.rank)
    }
}

// ---------------------------------------------------------------------------
// hands._classify
// ---------------------------------------------------------------------------

fn is_straight(ranks: &[u8]) -> bool {
    let mut idx: Vec<u8> = ranks.iter().map(|r| r - 2).collect();
    idx.sort_unstable();
    idx.dedup();
    if idx.len() != 5 {
        return false;
    }
    idx[4] - idx[0] == 4 || idx == [0, 1, 2, 3, 12]
}

/// (hand index, scoring positions into `cs`): the same name and order as hands._classify.
pub fn classify(cs: &[Card]) -> (usize, Vec<usize>) {
    let ranked: Vec<usize> = (0..cs.len()).filter(|&p| cs[p].enh != STONE && cs[p].rank != 0).collect();
    let stones: Vec<usize> = (0..cs.len()).filter(|p| !ranked.contains(p)).collect();
    let mut by_rank = [0usize; 15];
    for &p in &ranked {
        by_rank[cs[p].rank as usize] += 1;
    }
    let mut counts: Vec<usize> = by_rank.iter().copied().filter(|&n| n > 0).collect();
    counts.sort_unstable_by(|a, b| b.cmp(a));
    if counts.is_empty() {
        counts.push(0);
    }
    let first_two = |a: usize, b: usize| counts.len() >= 2 && counts[0] == a && counts[1] == b;
    let five = cs.len() == 5 && ranked.len() == 5;
    let flush = five
        && (1..=4u8).any(|s| ranked.iter().all(|&p| cs[p].suit == s || cs[p].enh == WILD));
    let straight = five && is_straight(&ranked.iter().map(|&p| cs[p].rank).collect::<Vec<_>>());
    let with_count = |n: usize| -> Vec<usize> {
        ranked.iter().copied().filter(|&p| by_rank[cs[p].rank as usize] >= n).collect()
    };
    let (name, mut scoring) = if counts[0] == 5 {
        (if flush { FLUSH_FIVE } else { FIVE_KIND }, ranked.clone())
    } else if flush && first_two(3, 2) {
        (FLUSH_HOUSE, ranked.clone())
    } else if straight && flush {
        (STRAIGHT_FLUSH, ranked.clone())
    } else if counts[0] == 4 {
        (FOUR_KIND, with_count(4))
    } else if first_two(3, 2) {
        (FULL_HOUSE, ranked.clone())
    } else if flush {
        (FLUSH, ranked.clone())
    } else if straight {
        (STRAIGHT, ranked.clone())
    } else if counts[0] == 3 {
        (THREE_KIND, with_count(3))
    } else if first_two(2, 2) {
        (TWO_PAIR, with_count(2))
    } else if counts[0] == 2 {
        (PAIR, with_count(2))
    } else {
        // max(ranked, key=_rank_value): the first card of the highest rank
        let mut best: Option<usize> = None;
        for &p in &ranked {
            if best.map_or(true, |b| cs[p].rank > cs[b].rank) {
                best = Some(p);
            }
        }
        (HIGH_CARD, best.into_iter().collect())
    };
    scoring.extend(stones);
    (name, scoring)
}

// ---------------------------------------------------------------------------
// Joker rules (jokers.parse data tuples, encoded by native._rule)
// ---------------------------------------------------------------------------


struct Rule {
    card: Vec<(u8, u32, f64, f64, f64)>,
    retrigger: Vec<(u8, u32, i64)>,
    held: Vec<(u8, u32, f64, f64)>,
    hand: Option<HandIn>,
}

/// jokers._pred: 0 all, 1 suit (or Wild), 2 rank set, 6 one rank of one suit (rank | suit << 8; Wild
/// matches the suit). Kinds 3-5 (first_face, first, final) are position rules handled by the caller
/// and never match here.
fn pred(kind: u8, arg: u32, c: &Card) -> bool {
    match kind {
        0 => true,
        1 => c.suit as u32 == arg || c.enh == WILD,
        2 => c.rank != 0 && arg & (1 << c.rank) != 0,
        6 => c.rank != 0 && c.rank as u32 == arg & 0xff && (c.suit as u32 == arg >> 8 || c.enh == WILD),
        _ => false,
    }
}

pub struct OldScorer {
    levels: Vec<(f64, f64, f64, bool)>,
    editions: Vec<u8>,
    rules: Vec<Option<Rule>>,
    held_retrigger: i64,
    hands_left: f64,
    discards_left: f64,
}

impl OldScorer {
    pub fn new(levels: Vec<(f64, f64, f64, bool)>, jokers: Vec<(u8, Option<RuleIn>)>, hands_left: f64, discards_left: f64) -> Result<Self, &'static str> {
        if levels.len() != HAND_NAMES.len() {
            return Err("levels must have one row per hand name");
        }
        let mut held_retrigger = 0;
        let mut editions = Vec::new();
        let mut rules = Vec::new();
        for (edition, rule) in jokers {
            editions.push(edition);
            rules.push(rule.map(|(card, retrigger, held, hr, hand)| {
                held_retrigger += hr;
                Rule { card, retrigger, held, hand }
            }));
        }
        Ok(OldScorer { levels, editions, rules, held_retrigger, hands_left, discards_left })
    }
}

impl OldScorer {
    fn active(&self) -> impl Iterator<Item = &Rule> {
        self.rules.iter().flatten()
    }

    fn hand_effect(&self, h: &HandIn, ctx: &Ctx) -> Eff {
        let &(kind, arg, v, yes, no) = h;
        let cond = |ok: bool| if ok { yes } else { no };
        match kind {
            0 => yes,
            1 => cond(arg & (1 << ctx.name) != 0),
            2 => cond(ctx.n_played <= 3),
            3 => cond(self.hands_left == 1.0),
            4 => cond(self.discards_left == 0.0),
            5 => cond(ctx.held.iter().all(|c| arg & (1 << c.suit) != 0)),
            6 => cond((ctx.scoring_suits.count_ones() as usize) + ctx.wild >= 4),
            7 => cond(ctx.played_this_round),
            8 => (0.0, ctx.times_played + 1.0, 1.0),
            9 => (v * self.discards_left, 0.0, 1.0),
            11 => {
                // Seeing Double: Wild cards fill Clubs first, then the other suits
                let c = ctx.scoring_suits & (1 << CLUBS) != 0;
                let o = ctx.scoring_suits & !(1 << CLUBS) != 0;
                let w = ctx.wild;
                cond((c || w > 0) && (o || (c && w > 0) || w >= 2))
            }
            _ => {
                let low = ctx.held.iter().map(nominal).fold(None, |m: Option<f64>, x| Some(m.map_or(x, |m| m.min(x))));
                (0.0, 2.0 * low.unwrap_or(0.0), 1.0)
            }
        }
    }

    /// jokers.score for hand positions `combo`; `sc` = scoring positions into combo. Truncated like int().
    pub fn score(&self, cards: &[Card], combo: &[usize], name: usize, sc: &[usize]) -> f64 {
        let (lchips, lmult, times_played, played_this_round) = self.levels[name];
        let mut chips = lchips;
        let mut mult = lmult;
        let played: Vec<Card> = combo.iter().map(|&i| cards[i]).collect();
        let held: Vec<Card> = (0..cards.len()).filter(|i| !combo.contains(i)).map(|i| cards[i]).collect();
        let mut sc: Vec<usize> = sc.to_vec();
        sc.sort_unstable();

        let first_face = sc.iter().copied().find(|&p| (11..=13).contains(&played[p].rank) && !played[p].debuff);
        for (k, &p) in sc.iter().enumerate() {
            let c = &played[p];
            if c.debuff {
                continue;
            }
            let mut reps: i64 = 1 + c.red as i64;
            for r in self.active() {
                for &(kind, arg, times) in &r.retrigger {
                    if (kind == 4 && k == 0) || (kind == 5 && self.hands_left == 1.0) || pred(kind, arg, c) {
                        reps += times;
                    }
                }
            }
            for _ in 0..reps.max(0) {
                chips += if c.enh == STONE || c.rank == 0 { 50.0 } else { rank_chips(c.rank) };
                chips += ((if c.enh == BONUS { 30 } else { 0 }) + (if c.edition == FOIL { 50 } else { 0 })) as f64;
                mult += ((if c.enh == MULT { 4 } else { 0 }) + (if c.edition == HOLO { 10 } else { 0 })) as f64;
                mult *= (if c.enh == GLASS { 2.0 } else { 1.0 }) * (if c.edition == POLYCHROME { 1.5 } else { 1.0 });
                for r in self.active() {
                    for &(kind, arg, cc, mm, xx) in &r.card {
                        if (kind == 3 && Some(p) == first_face) || pred(kind, arg, c) {
                            chips += cc;
                            mult = (mult + mm) * xx;
                        }
                    }
                }
            }
        }

        let held_reps = 1 + self.held_retrigger;
        for c in &held {
            if c.debuff {
                continue;
            }
            for _ in 0..(held_reps + c.red as i64).max(0) {
                if c.enh == STEEL {
                    mult *= 1.5;
                }
                for r in self.active() {
                    for &(kind, arg, mm, xx) in &r.held {
                        if pred(kind, arg, c) {
                            mult = (mult + mm) * xx;
                        }
                    }
                }
            }
        }

        let mut scoring_suits = 0u32;
        let mut wild = 0usize;
        for &p in &sc {
            if played[p].enh != WILD {
                scoring_suits |= 1 << played[p].suit;
            } else {
                wild += 1;
            }
        }
        let ctx = Ctx { name, n_played: played.len(), held: &held, times_played, played_this_round, scoring_suits, wild };
        for (edition, rule) in self.editions.iter().zip(&self.rules) {
            chips += if *edition == FOIL { 50.0 } else { 0.0 };
            mult += if *edition == HOLO { 10.0 } else { 0.0 };
            if let Some(h) = rule.as_ref().and_then(|r| r.hand.as_ref()) {
                let (cc, mm, xx) = self.hand_effect(h, &ctx);
                chips += cc;
                mult = (mult + mm) * xx;
            }
            mult *= if *edition == POLYCHROME { 1.5 } else { 1.0 };
        }
        let c = if chips > 0.0 { chips } else { 0.0 };
        let m = if mult > 1.0 { mult } else { 1.0 };
        (c * m).trunc()
    }
}

struct Ctx<'a> {
    name: usize,
    n_played: usize,
    held: &'a [Card],
    times_played: f64,
    played_this_round: bool,
    scoring_suits: u32,
    wild: usize,
}

