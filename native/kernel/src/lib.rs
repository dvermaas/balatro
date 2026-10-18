//! The bot's joker-aware hand scorer, shared by `jev_native` (PyO3 module, ../src) and `jev_sim`
//! (../sim, rollouts on rumbot-sim).
//!
//! A line-by-line port of `hands._classify`, `hands.evaluated_plays` and `jokers.score` over the
//! encoded inputs that `src/native.py` builds. Floating-point operations run in the same order as the
//! Python, so every score is bit-identical (tests/test_native.py compares every subset).
//! Part of the prompt version hash (see recorder.PROMPT_VERSION).

// Card codes (src/native.py): rank 0 = none/falsy, 2..=14 = 2..A; suit 0 = none, 1..=4 = H D C S.
pub const STONE: u8 = 1;
pub const WILD: u8 = 2;
const BONUS: u8 = 3;
const MULT: u8 = 4;
const GLASS: u8 = 5;
const STEEL: u8 = 6;
const FOIL: u8 = 1;
const HOLO: u8 = 2;
const POLYCHROME: u8 = 3;
const CLUBS: u32 = 3; // native._SUIT["C"]

// Hand names in strategy.HAND_NAMES order.
pub const HAND_NAMES: [&str; 12] = [
    "Flush Five", "Flush House", "Five of a Kind", "Straight Flush", "Four of a Kind", "Full House",
    "Flush", "Straight", "Three of a Kind", "Two Pair", "Pair", "High Card",
];
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

#[derive(Clone, Copy)]
pub struct Card {
    pub rank: u8,
    pub suit: u8,
    pub enh: u8,
    pub edition: u8,
    pub red: bool,
    pub debuff: bool,
}

pub type CardIn = (u8, u8, u8, u8, bool, bool);

pub fn cards_of(v: &[CardIn]) -> Vec<Card> {
    v.iter()
        .map(|&(rank, suit, enh, edition, red, debuff)| Card { rank, suit, enh, edition, red, debuff })
        .collect()
}

/// hands.RANK_CHIPS
pub fn rank_chips(rank: u8) -> f64 {
    match rank {
        2..=10 => rank as f64,
        11..=13 => 10.0,
        _ => 11.0,
    }
}

/// jokers._nominal
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

fn ranked(c: &Card) -> bool {
    c.enh != STONE && c.rank != 0
}

/// (hand index, scoring positions into `cs`): the same name and order as hands._classify.
pub fn classify(cs: &[Card]) -> (usize, Vec<usize>) {
    let mut scoring = Vec::with_capacity(cs.len());
    let name = classify_into(cs, &mut scoring);
    (name, scoring)
}

/// `classify` into a reused buffer: returns the hand index and leaves the scoring positions in `out`.
/// Ranked cards are the non-Stone cards with a rank; Stones always score, after the ranked ones.
pub fn classify_into(cs: &[Card], out: &mut Vec<usize>) -> usize {
    out.clear();
    let mut by_rank = [0usize; 15];
    let mut n_ranked = 0;
    for c in cs.iter().filter(|c| ranked(c)) {
        by_rank[c.rank as usize] += 1;
        n_ranked += 1;
    }
    // the two largest rank counts (sorted descending; 0 when absent) and how many ranks occur
    let (mut top, mut second, mut distinct) = (0usize, 0usize, 0usize);
    for &n in by_rank.iter().filter(|&&n| n > 0) {
        distinct += 1;
        if n > top {
            second = top;
            top = n;
        } else if n > second {
            second = n;
        }
    }
    let first_two = |a: usize, b: usize| distinct >= 2 && top == a && second == b;
    let five = cs.len() == 5 && n_ranked == 5;
    let flush = five && (1..=4u8).any(|s| cs.iter().filter(|c| ranked(c)).all(|c| c.suit == s || c.enh == WILD));
    // five distinct ranks in a row, or the wheel A-2-3-4-5
    let straight = five && distinct == 5 && {
        let lo = (2..15).find(|&r| by_rank[r] > 0).unwrap_or(0);
        by_rank[lo..(lo + 5).min(15)].iter().all(|&n| n > 0) || [2, 3, 4, 5, 14].iter().all(|&r| by_rank[r] > 0)
    };
    let all_ranked = |out: &mut Vec<usize>| out.extend((0..cs.len()).filter(|&p| ranked(&cs[p])));
    let with_count = |out: &mut Vec<usize>, n: usize| {
        out.extend((0..cs.len()).filter(|&p| ranked(&cs[p]) && by_rank[cs[p].rank as usize] >= n))
    };
    let name = if top == 5 {
        all_ranked(out);
        if flush { FLUSH_FIVE } else { FIVE_KIND }
    } else if flush && first_two(3, 2) {
        all_ranked(out);
        FLUSH_HOUSE
    } else if straight && flush {
        all_ranked(out);
        STRAIGHT_FLUSH
    } else if top == 4 {
        with_count(out, 4);
        FOUR_KIND
    } else if first_two(3, 2) {
        all_ranked(out);
        FULL_HOUSE
    } else if flush {
        all_ranked(out);
        FLUSH
    } else if straight {
        all_ranked(out);
        STRAIGHT
    } else if top == 3 {
        with_count(out, 3);
        THREE_KIND
    } else if first_two(2, 2) {
        with_count(out, 2);
        TWO_PAIR
    } else if top == 2 {
        with_count(out, 2);
        PAIR
    } else {
        // max(ranked, key=_rank_value): the first card of the highest rank
        let mut best: Option<usize> = None;
        for p in (0..cs.len()).filter(|&p| ranked(&cs[p])) {
            if best.map_or(true, |b| cs[p].rank > cs[b].rank) {
                best = Some(p);
            }
        }
        out.extend(best);
        HIGH_CARD
    };
    out.extend((0..cs.len()).filter(|&p| !ranked(&cs[p])));
    name
}

// ---------------------------------------------------------------------------
// Joker rules (jokers.parse data tuples, encoded by native._rule)
// ---------------------------------------------------------------------------

pub type Eff = (f64, f64, f64);
pub type HandIn = (u8, u32, f64, Eff, Eff);
pub type RuleIn = (Vec<(u8, u32, f64, f64, f64)>, Vec<(u8, u32, i64)>, Vec<(u8, u32, f64, f64)>, i64, Option<HandIn>);

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

pub struct Scorer {
    levels: Vec<(f64, f64, f64, bool)>,
    /// per joker, in row order: its edition and its hand rule
    editions: Vec<u8>,
    hands: Vec<Option<HandIn>>,
    // every joker's card, retrigger and held rules, concatenated in row order (the order they apply)
    card: Vec<(u8, u32, f64, f64, f64)>,
    retrigger: Vec<(u8, u32, i64)>,
    held: Vec<(u8, u32, f64, f64)>,
    held_retrigger: i64,
    hands_left: f64,
    discards_left: f64,
}

impl Scorer {
    pub fn new(levels: Vec<(f64, f64, f64, bool)>, jokers: Vec<(u8, Option<RuleIn>)>, hands_left: f64, discards_left: f64) -> Result<Self, &'static str> {
        if levels.len() != HAND_NAMES.len() {
            return Err("levels must have one row per hand name");
        }
        let mut s = Scorer {
            levels, editions: Vec::new(), hands: Vec::new(), card: Vec::new(), retrigger: Vec::new(), held: Vec::new(),
            held_retrigger: 0, hands_left, discards_left,
        };
        for (edition, rule) in jokers {
            s.editions.push(edition);
            let hand = rule.map(|(card, retrigger, held, hr, hand)| {
                s.card.extend(card);
                s.retrigger.extend(retrigger);
                s.held.extend(held);
                s.held_retrigger += hr;
                hand
            });
            s.hands.push(hand.flatten());
        }
        Ok(s)
    }
}

impl Scorer {
    fn hand_effect(&self, h: &HandIn, ctx: &Ctx) -> Eff {
        let &(kind, arg, v, yes, no) = h;
        let cond = |ok: bool| if ok { yes } else { no };
        match kind {
            0 => yes,
            1 => cond(arg & (1 << ctx.name) != 0),
            2 => cond(ctx.combo.len() <= 3),
            3 => cond(self.hands_left == 1.0),
            4 => cond(self.discards_left == 0.0),
            5 => cond(ctx.held().all(|c| arg & (1 << c.suit) != 0)),
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
                // Raised Fist (v42, rumbot lowest_held): the rightmost held card of the lowest rank, Stone cards
                // skipped; nothing when it is debuffed, and it repeats with held retriggers (Mime) and its Red Seal
                let low = ctx.held().filter(|c| c.enh != STONE && c.rank != 0).fold(None, |m: Option<&Card>, c| match m {
                    Some(b) if c.rank > b.rank => Some(b),
                    _ => Some(c),
                });
                match low {
                    Some(c) if !c.debuff => (0.0, 2.0 * nominal(c) * (1 + self.held_retrigger + c.red as i64) as f64, 1.0),
                    _ => (0.0, 0.0, 1.0),
                }
            }
        }
    }

    /// jokers.score for hand positions `combo`; `sc` = scoring positions into combo. Truncated like int().
    pub fn score(&self, cards: &[Card], combo: &[usize], name: usize, sc: &[usize]) -> f64 {
        let (lchips, lmult, times_played, played_this_round) = self.levels[name];
        let mut chips = lchips;
        let mut mult = lmult;
        let played = |p: usize| &cards[combo[p]];
        // scoring positions in ascending order (at most 5 in a real play; a longer list goes to the heap)
        let mut buf = [0usize; 8];
        let mut long = Vec::new();
        let sc: &mut [usize] = if sc.len() <= buf.len() {
            buf[..sc.len()].copy_from_slice(sc);
            &mut buf[..sc.len()]
        } else {
            long.extend_from_slice(sc);
            &mut long
        };
        sc.sort_unstable();
        let sc: &[usize] = sc;

        let first_face = sc.iter().copied().find(|&p| (11..=13).contains(&played(p).rank) && !played(p).debuff);
        for (k, &p) in sc.iter().enumerate() {
            let c = played(p);
            if c.debuff {
                continue;
            }
            let mut reps: i64 = 1 + c.red as i64;
            for &(kind, arg, times) in &self.retrigger {
                if (kind == 4 && k == 0) || (kind == 5 && self.hands_left == 1.0) || pred(kind, arg, c) {
                    reps += times;
                }
            }
            for _ in 0..reps.max(0) {
                chips += if c.enh == STONE || c.rank == 0 { 50.0 } else { rank_chips(c.rank) };
                chips += ((if c.enh == BONUS { 30 } else { 0 }) + (if c.edition == FOIL { 50 } else { 0 })) as f64;
                mult += ((if c.enh == MULT { 4 } else { 0 }) + (if c.edition == HOLO { 10 } else { 0 })) as f64;
                mult *= (if c.enh == GLASS { 2.0 } else { 1.0 }) * (if c.edition == POLYCHROME { 1.5 } else { 1.0 });
                for &(kind, arg, cc, mm, xx) in &self.card {
                    if (kind == 3 && Some(p) == first_face) || pred(kind, arg, c) {
                        chips += cc;
                        mult = (mult + mm) * xx;
                    }
                }
            }
        }

        let ctx = Ctx { name, cards, combo, times_played, played_this_round, scoring_suits: 0, wild: 0 };
        let held_reps = 1 + self.held_retrigger;
        // without held rules only a held Steel card changes anything
        let only_steel = self.held.is_empty();
        for c in ctx.held() {
            if c.debuff || (only_steel && c.enh != STEEL) {
                continue;
            }
            for _ in 0..(held_reps + c.red as i64).max(0) {
                if c.enh == STEEL {
                    mult *= 1.5;
                }
                for &(kind, arg, mm, xx) in &self.held {
                    if pred(kind, arg, c) {
                        mult = (mult + mm) * xx;
                    }
                }
            }
        }

        let mut scoring_suits = 0u32;
        let mut wild = 0usize;
        for &p in sc {
            if played(p).enh != WILD {
                scoring_suits |= 1 << played(p).suit;
            } else {
                wild += 1;
            }
        }
        let ctx = Ctx { scoring_suits, wild, ..ctx };
        for (edition, hand) in self.editions.iter().zip(&self.hands) {
            chips += if *edition == FOIL { 50.0 } else { 0.0 };
            mult += if *edition == HOLO { 10.0 } else { 0.0 };
            if let Some(h) = hand {
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
    cards: &'a [Card],
    /// the played positions into `cards`; every other card is held
    combo: &'a [usize],
    times_played: f64,
    played_this_round: bool,
    scoring_suits: u32,
    wild: usize,
}

impl<'a> Ctx<'a> {
    /// The held cards, in hand order.
    fn held(&self) -> impl Iterator<Item = &'a Card> + 'a {
        let combo = self.combo;
        self.cards.iter().enumerate().filter(move |(i, _)| !combo.contains(i)).map(|(_, c)| c)
    }
}

// ---------------------------------------------------------------------------
// hands.evaluated_plays / best_score / best_play
// ---------------------------------------------------------------------------

/// decisions._boss_allows: (boss, played-hand mask, any hand played); boss 0 = no filter.
pub fn allows(allow: (u8, u32, bool), combo_len: usize, name: usize) -> bool {
    let (boss, played, any) = allow;
    match boss {
        1 => combo_len == 5,
        2 => played & (1 << name) == 0,
        3 if any => played & (1 << name) != 0,
        _ => true,
    }
}

impl Scorer {
    /// No score reads the cards left in hand: no held rules, and no hand rule on held cards (kinds 5,
    /// 10 and unknown) or on the number of cards played (kind 2).
    fn ignores_held(&self) -> bool {
        self.held.is_empty() && self.hands.iter().flatten().all(|h| matches!(h.0, 0 | 1 | 3 | 4 | 6 | 7 | 8 | 9 | 11))
    }
}

/// The plays that can be the best one: `each_play`'s, minus those with a kicker (a played card that
/// does not score) whenever a kicker cannot matter. Then dropping a play's kickers keeps its hand type
/// and scoring cards (in the same order), and the score reads nothing else: the scorer ignores held
/// cards, no Steel card is in hand (held Steel multiplies), and the boss does not count cards (The
/// Psychic). So the best score, and the best (score, hand) pair, are the same as over every play.
pub fn each_best_candidate(s: &Scorer, cards: &[Card], allow: (u8, u32, bool), mut f: impl FnMut(f64, usize, &[usize], &[usize])) {
    let prune = allow.0 != 1 && s.ignores_held() && !cards.iter().any(|c| c.enh == STEEL);
    if !prune {
        return each_play(s, cards, allow, f);
    }
    let mut cs: Vec<Card> = Vec::with_capacity(5);
    let mut sc: Vec<usize> = Vec::with_capacity(5);
    each_combo(cards.len(), |combo| {
        cs.clear();
        cs.extend(combo.iter().map(|&i| cards[i]));
        if !may_lack_kickers(&cs) {
            return;
        }
        let name = classify_into(&cs, &mut sc);
        if sc.len() == combo.len() && allows(allow, combo.len(), name) {
            f(s.score(cards, combo, name, &sc), name, combo, &sc);
        }
    });
}

/// False when `classify` would surely leave a played card unscored: with 2 to 4 ranked cards only
/// pairs, two pairs, trips and quads score all of them, so a rank seen once means a kicker. (With 0, 1
/// or 5 ranked cards anything goes: Stones, a lone high card, a flush or a straight.)
fn may_lack_kickers(cs: &[Card]) -> bool {
    let mut ranks = [0u8; 5];
    let mut n = 0;
    for c in cs.iter().filter(|c| ranked(c)) {
        ranks[n] = c.rank;
        n += 1;
    }
    let ranks = &ranks[..n];
    !(2..=4).contains(&n) || ranks.iter().all(|r| ranks.iter().filter(|x| *x == r).count() >= 2)
}

/// Every allowed subset of 1..=5 cards in itertools.combinations order: f(score, name, combo, scoring).
pub fn each_play(s: &Scorer, cards: &[Card], allow: (u8, u32, bool), mut f: impl FnMut(f64, usize, &[usize], &[usize])) {
    each_classified(cards, |name, combo, sc| {
        if allows(allow, combo.len(), name) {
            f(s.score(cards, combo, name, sc), name, combo, sc);
        }
    });
}

/// Every subset of 1..=5 cards in itertools.combinations order with its classification: f(name, combo, scoring).
fn each_classified(cards: &[Card], mut f: impl FnMut(usize, &[usize], &[usize])) {
    let mut cs: Vec<Card> = Vec::with_capacity(5);
    let mut sc: Vec<usize> = Vec::with_capacity(5);
    each_combo(cards.len(), |combo| {
        cs.clear();
        cs.extend(combo.iter().map(|&i| cards[i]));
        let name = classify_into(&cs, &mut sc);
        f(name, combo, &sc);
    });
}

/// Every subset of 1..=5 of `n` positions, in itertools.combinations order.
fn each_combo(n: usize, mut f: impl FnMut(&[usize])) {
    let mut combo: Vec<usize> = Vec::with_capacity(5);
    for r in 1..=n.min(5) {
        combo.clear();
        combo.extend(0..r);
        loop {
            f(&combo);
            // next combination
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
    }
}

