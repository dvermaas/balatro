//! Port of src/odds.py. See docs/rust-port-guide.md.
//!
//! Uses only order-free facts about the draw pile (which cards remain, never their order: the pile is
//! in draw order, see CLAUDE.md). Forecasts score complete future hands after uniform unordered draws.
//!
//! The forecast's scorer_factory is a Rust closure: the callers' jokers.after_discard / after_play are
//! not ported yet (jokers.rs TODO), so the test passes the scorer decisions builds without them.

use crate::hands::{self, Allowed, Goal, Info, ScorerArg};
use crate::jokers;
use pycompat::{comb, fsum, pyround, pyround_int, Random};
use serde_json::{json, Value};
use std::collections::BTreeSet;

// odds.py 15-17
/// All observable scoring properties as Python strings: str(c.get(k) or "") for each key.
fn card_key(c: &Info) -> [String; 6] {
    let s = |o: &Option<String>| o.clone().unwrap_or_default();
    [
        s(&c.rank),
        s(&c.suit),
        s(&c.enh),
        s(&c.edition),
        s(&c.seal),
        if c.debuff { "True".to_string() } else { String::new() },
    ]
}

fn sort_by_card_key(cards: &mut [Info]) {
    cards.sort_by_cached_key(card_key);
}

/// Python int / int (true division). Exact for both operands below 2^53, which covers every
/// deck size the bot sees; anything larger would need a correctly rounded big-int division.
fn int_div(a: u128, b: u128) -> f64 {
    const EXACT: u128 = 1 << 53;
    assert!(a < EXACT && b < EXACT, "odds: int/int above 2^53 is not reproduced exactly");
    a as f64 / b as f64
}

/// How draw_count is passed: None (default: as many as were tossed), a fixed int, or a callable of
/// the number tossed.
pub enum DrawCount<'a> {
    Default,
    Fixed(i64),
    Func(&'a dyn Fn(usize) -> i64),
}

// odds.py 22-61
/// Model the best next play over unordered draws, including misses and held cards.
///
/// Small draw spaces are enumerated exactly; otherwise use reproducible uniform samples (Random(12012)
/// per plan, common random numbers). Each plan dict gets expected_score, low_score, chance_to_clear,
/// chance_to_meet_pace, samples, exact_draws and drawn, in that order. scorer_factory(discarded, count)
/// accounts for the spent discard and joker changes.
pub fn forecast(
    discards: &mut [Value],
    hand_infos: &[Info],
    deck_infos: &[Info],
    scorer_factory: &dyn Fn(&[Info], usize) -> jokers::Scorer,
    allowed: Allowed,
    need: f64,
    pace: f64,
    draw_count: DrawCount,
    samples: usize,
) {
    assert!(samples >= 1, "samples must be positive");
    let mut deck: Vec<Info> = deck_infos.to_vec();
    sort_by_card_key(&mut deck);
    for plan in discards.iter_mut() {
        let cards: Vec<usize> = plan["cards"]
            .as_array()
            .expect("plan cards")
            .iter()
            .map(|v| v.as_u64().expect("card index") as usize)
            .collect();
        let tossed: BTreeSet<usize> = cards.iter().copied().collect();
        let kept: Vec<Info> = hand_infos
            .iter()
            .enumerate()
            .filter(|(i, _)| !tossed.contains(i))
            .map(|(_, c)| c.clone())
            .collect();
        let discarded: Vec<Info> = tossed.iter().map(|&i| hand_infos[i].clone()).collect();
        let requested: Option<i64> = match &draw_count {
            DrawCount::Default => None,
            DrawCount::Fixed(x) => Some(*x),
            DrawCount::Func(f) => Some(f(tossed.len())),
        };
        let want = requested.unwrap_or(tossed.len() as i64).max(0) as usize;
        let count = deck.len().min(want);
        let possibilities = comb(deck.len() as u64, count as u64);
        let exact = possibilities <= samples as u128;
        let draws: Vec<Vec<usize>> = if exact {
            hands::combinations(deck.len(), count)
        } else {
            // common random numbers reduce noise between competing plans
            let mut rng = Random::new_int(12012);
            (0..samples).map(|_| rng.sample_indices(deck.len(), count)).collect()
        };
        let scorer = scorer_factory(&discarded, count);
        let mut scores: Vec<i64> = Vec::with_capacity(draws.len());
        for draw in &draws {
            let mut future = kept.clone();
            future.extend(draw.iter().map(|&i| deck[i].clone()));
            // The live bot uses rank-sorted hands. Stable modifier ties also avoid pile-order leaks.
            future.sort_by_cached_key(future_key);
            scores.push(hands::best_score(&future, ScorerArg::Jokers(&scorer), allowed));
        }
        scores.sort();
        let n = scores.len();
        let total = fsum(&scores.iter().map(|&s| s as f64).collect::<Vec<f64>>());
        let expected_score = pyround_int(total / n as f64);
        let low_score = scores[((n - 1) as f64 * 0.2) as usize];
        let chance_to_clear = scores.iter().filter(|&&s| s as f64 >= need).count() as f64 / n as f64;
        let chance_to_meet_pace = scores.iter().filter(|&&s| s as f64 >= pace).count() as f64 / n as f64;
        let obj = plan.as_object_mut().expect("plan dict");
        obj.insert("expected_score".into(), json!(expected_score));
        obj.insert("low_score".into(), json!(low_score));
        obj.insert("chance_to_clear".into(), json!(chance_to_clear));
        obj.insert("chance_to_meet_pace".into(), json!(chance_to_meet_pace));
        obj.insert("samples".into(), json!(n));
        obj.insert("exact_draws".into(), json!(exact));
        obj.insert("drawn".into(), json!(count));
    }
}

/// Sort key of the future hand: rank-sorted (descending rank, stone cards unranked), then card key.
fn future_key(c: &Info) -> (i64, [String; 6]) {
    let first = match c.rank_truthy() {
        Some(r) if !c.enh_is("STONE") => -((hands::rank_index(r) + 2) as i64),
        _ => 0,
    };
    (first, card_key(c))
}

// odds.py 64-65
/// P(no card from a group of k) when drawing d of n.
fn p_none(n: u64, k: u64, d: u64) -> f64 {
    if d <= n {
        int_div(comb(n - k, d), comb(n, d))
    } else {
        0.0
    }
}

// odds.py 68-72
/// Hypergeometric P(X >= need): X = cards from a group of k among d drawn from n.
pub fn p_at_least(n: u64, k: u64, d: u64, need: i64) -> f64 {
    let d = d.min(n);
    if d == 0 {
        return if need <= 0 { 1.0 } else { 0.0 };
    }
    let hi = k.min(d);
    let mut total: u128 = 0;
    let lo = need.max(0) as u64;
    for x in lo..=hi {
        total += comb(k, x) * comb(n - k, d - x);
    }
    int_div(total, comb(n, d))
}

// odds.py 75-80
/// P(drawing at least one of each listed rank group) by inclusion-exclusion.
pub fn p_all_ranks(n: u64, counts: &[u64], d: u64) -> f64 {
    let d = d.min(n);
    let mut total = 0.0f64;
    for r in 0..=counts.len() {
        for sub in hands::combinations(counts.len(), r) {
            let s: u64 = sub.iter().map(|&i| counts[i]).sum();
            let sign = if r % 2 == 0 { 1.0 } else { -1.0 };
            total += sign * p_none(n, s, d);
        }
    }
    if total > 0.0 {
        total
    } else {
        0.0
    }
}

/// The goal a discard plan carries (plan["goal"], a JSON pair), or None when absent.
fn goal_of(plan: &Value) -> Option<Goal> {
    let g = plan.get("goal").filter(|g| !g.is_null())?;
    let arg = &g[1];
    let strs = |v: &Value| -> Vec<String> {
        v.as_array().expect("goal list").iter().map(|x| x.as_str().expect("rank").to_string()).collect()
    };
    let opts = |v: &Value| -> Vec<Option<String>> {
        v.as_array().expect("goal list").iter().map(|x| x.as_str().map(|s| s.to_string())).collect()
    };
    match g[0].as_str()? {
        "flush" => Some(Goal::Flush(arg.as_str().expect("suit").to_string())),
        "straight" => Some(Goal::Straight(strs(arg))),
        "sets" => Some(Goal::Sets(opts(arg))),
        "high" => Some(Goal::High(opts(arg))),
        _ => None,
    }
}

// odds.py 83-130
/// Add chance_to_hit, goal_hand and score_if_hit to each discard plan from hands.discard_candidates.
/// score_fn(cards) scores a list of card infos played as a hand (jokers included).
pub fn annotate(discards: &mut [Value], hand_infos: &[Info], deck_infos: &[Info], score_fn: &dyn Fn(&[Info]) -> i64) {
    let mut deck: Vec<Info> = deck_infos.to_vec();
    sort_by_card_key(&mut deck);
    let n = deck.len() as u64;
    for dsc in discards.iter_mut() {
        let cards: Vec<usize> = dsc["cards"]
            .as_array()
            .expect("plan cards")
            .iter()
            .map(|v| v.as_u64().expect("index") as usize)
            .collect();
        let keep: Vec<usize> = (0..hand_infos.len()).filter(|i| !cards.contains(i)).collect();
        let d = cards.len() as u64;
        let (chance, hit): (f64, Vec<Info>) = match goal_of(dsc) {
            None => continue,
            Some(Goal::Flush(suit)) => {
                let held: Vec<usize> = keep
                    .iter()
                    .copied()
                    .filter(|&i| hand_infos[i].suit_is(&suit) || hand_infos[i].enh_is("WILD"))
                    .collect();
                let mut pool: Vec<Info> = deck
                    .iter()
                    .filter(|c| c.suit_is(&suit) && c.rank_truthy().is_some())
                    .cloned()
                    .collect();
                pool.sort_by_key(|c| hands::rank_index(c.rank_truthy().expect("rank")));
                let need = 5usize.saturating_sub(held.len());
                let chance = p_at_least(n, pool.len() as u64, d, need as i64);
                let mid = pool.len() / 2;
                let start = mid.saturating_sub(need / 2);
                let mut hit: Vec<Info> = held.iter().take(5).map(|&i| hand_infos[i].clone()).collect();
                hit.extend(pool[start..].iter().take(need).cloned());
                (chance, hit)
            }
            Some(Goal::Straight(missing)) => {
                let groups: Vec<Vec<Info>> = missing
                    .iter()
                    .map(|r| deck.iter().filter(|c| c.rank.as_deref() == Some(r.as_str())).cloned().collect())
                    .collect();
                let counts: Vec<u64> = groups.iter().map(|g| g.len() as u64).collect();
                let chance = p_all_ranks(n, &counts, d);
                let filler: Vec<Info> = groups.iter().filter(|g| !g.is_empty()).map(|g| g[0].clone()).collect();
                let window = arg_window(&missing, hand_infos, &keep);
                let mut ranks: Vec<Option<String>> = Vec::new();
                let mut hit: Vec<Info> = Vec::new();
                for &i in &keep {
                    let r = hand_infos[i].rank.clone();
                    if window.contains(&r) && !ranks.contains(&r) {
                        ranks.push(r);
                        hit.push(hand_infos[i].clone());
                    }
                }
                hit.extend(filler);
                (chance, hit)
            }
            Some(Goal::Sets(ranks)) | Some(Goal::High(ranks)) => {
                let pool: Vec<&Info> = deck.iter().filter(|c| ranks.contains(&c.rank)).collect();
                let chance = 1.0 - p_none(n, pool.len() as u64, d.min(n));
                // max(pool, key=RANK_ORDER.index, default=None): the first maximum wins
                let mut best: Option<&Info> = None;
                for c in &pool {
                    let key = hands::rank_index(c.rank.as_deref().expect("rank"));
                    if best.map_or(true, |b| key > hands::rank_index(b.rank.as_deref().expect("rank"))) {
                        best = Some(c);
                    }
                }
                let mut hit: Vec<Info> = match best {
                    Some(_) => keep
                        .iter()
                        .filter(|&&i| ranks.contains(&hand_infos[i].rank))
                        .take(4)
                        .map(|&i| hand_infos[i].clone())
                        .collect(),
                    None => Vec::new(),
                };
                if let Some(b) = best {
                    hit.push(b.clone());
                }
                (chance, hit)
            }
        };
        let obj = dsc.as_object_mut().expect("plan dict");
        obj.insert("chance_to_hit".into(), json!(pyround(chance, 2)));
        if !hit.is_empty() && chance != 0.0 {
            let top = &hit[..hit.len().min(5)];
            let (name, _) = hands::classify(top);
            obj.insert("goal_hand".into(), json!(name));
            obj.insert("score_if_hit".into(), json!(score_fn(top)));
        }
    }
}

// odds.py 133-134
/// The 5-rank window a straight plan targets: kept ranks plus the missing ones.
/// Returned as a sorted set (None last), as the fixture writes Python sets.
pub fn arg_window(missing: &[String], hand_infos: &[Info], keep: &[usize]) -> Vec<Option<String>> {
    let mut w: Vec<Option<String>> = keep.iter().map(|&i| hand_infos[i].rank.clone()).collect();
    w.extend(missing.iter().map(|m| Some(m.clone())));
    w.sort_by(|a, b| (a.is_none(), a.as_deref().unwrap_or("")).cmp(&(b.is_none(), b.as_deref().unwrap_or(""))));
    w.dedup();
    w
}

/// A first move for `round_search`: play these cards (scoring this much) or discard them.
pub enum RootAct {
    Play(Vec<usize>, i64),
    Discard(Vec<usize>),
}

/// The round as `round_search` sees it.
pub struct RoundState<'a> {
    pub hand: &'a [Info],
    pub deck: &'a [Info],
    pub need: f64,
    pub hands_left: i64,
    pub discards_left: i64,
    pub hand_size: usize,
    pub cap: usize,
    pub serpent: bool,
}

fn take(h: &mut Vec<Info>, cards: &[usize]) {
    let mut cs = cards.to_vec();
    cs.sort();
    for &c in cs.iter().rev() {
        if c < h.len() {
            h.remove(c);
        }
    }
}

/// The rollout's discard: with no flush in hand, keep 4+ cards of one suit; else keep the best play. Toss up to
/// `cap` of the rest, lowest first.
fn sim_discard(h: &[Info], best: &[usize], name: &str, cap: usize) -> Vec<usize> {
    let mut keep: Vec<usize> = best.to_vec();
    if !name.contains("Flush") {
        for s in ["Hearts", "Diamonds", "Clubs", "Spades"] {
            let of: Vec<usize> = (0..h.len()).filter(|&i| h[i].suit_is(s) && !h[i].enh_is("STONE")).collect();
            if of.len() >= 4 {
                keep = of;
                break;
            }
        }
    }
    let mut rest: Vec<usize> = (0..h.len()).filter(|i| !keep.contains(i)).collect();
    rest.sort_by_key(|&i| (hands::_rank_value(&h[i]), i));
    rest.truncate(cap);
    rest
}

/// lab (`round-search=N`): P(reaching `need` this round) after each first move, playing on with a simple policy
/// (play the best hand; while behind pace with discards left, or on the last hand, discard around it) over N
/// shuffles of the draw pile. The pile is sorted before shuffling, so its real order never matters; the same
/// shuffles score every move. The scorer is the current one (no joker growth within the round).
pub fn round_search(acts: &[RootAct], st: &RoundState, scorer: &jokers::Scorer, allowed: Allowed, samples: usize) -> Vec<f64> {
    let mut deck: Vec<Info> = st.deck.to_vec();
    sort_by_card_key(&mut deck);
    let mut wins = vec![0usize; acts.len()];
    let mut rng = Random::new_int(24024);
    for _ in 0..samples {
        let mut order: Vec<usize> = (0..deck.len()).collect();
        rng.shuffle(&mut order);
        for (a, act) in acts.iter().enumerate() {
            let mut h: Vec<Info> = st.hand.to_vec();
            let mut next = 0usize;
            let (mut r, mut hl, mut dl) = (st.need, st.hands_left, st.discards_left);
            let mut first = Some(act);
            loop {
                let (play, cards, score) = match first.take() {
                    Some(RootAct::Play(c, s)) => (true, c.clone(), *s),
                    Some(RootAct::Discard(c)) => (false, c.clone(), 0),
                    None => {
                        let (s, name, best) = hands::best_play_cards(&h, ScorerArg::Jokers(scorer), allowed);
                        if s as f64 >= r || dl <= 0 || best.is_empty() || (hl > 1 && (s * hl) as f64 >= r) {
                            (true, best, s)
                        } else {
                            let d = sim_discard(&h, &best, name, st.cap);
                            if d.is_empty() { (true, best, s) } else { (false, d, 0) }
                        }
                    }
                };
                take(&mut h, &cards);
                if play {
                    r -= score as f64;
                    hl -= 1;
                    if r <= 0.0 {
                        wins[a] += 1;
                        break;
                    }
                    if hl <= 0 {
                        break;
                    }
                } else {
                    dl -= 1;
                }
                let want = if st.serpent { 3 } else { st.hand_size.saturating_sub(h.len()) };
                for _ in 0..want {
                    if next >= order.len() {
                        break;
                    }
                    h.push(deck[order[next]].clone());
                    next += 1;
                }
                h.sort_by_cached_key(future_key);
                if h.is_empty() {
                    break;
                }
            }
        }
    }
    wins.iter().map(|&w| w as f64 / samples as f64).collect()
}
