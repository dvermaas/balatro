//! The kernel's allocation-free classify/score against the original implementation (tests/reference),
//! on random hands and random joker rules: every subset of up to 5 cards, every score bit for bit.
mod reference;

use jev_kernel::{classify, each_play, Card, RuleIn, Scorer};
use reference::OldScorer;

/// Small deterministic generator (64-bit LCG), so failures reproduce.
struct Lcg(u64);
impl Lcg {
    fn next(&mut self) -> u64 {
        self.0 = self.0.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
        self.0 >> 33
    }
    fn below(&mut self, n: u64) -> u64 {
        self.next() % n
    }
    fn chance(&mut self, pct: u64) -> bool {
        self.below(100) < pct
    }
    fn f(&mut self, options: &[f64]) -> f64 {
        options[self.below(options.len() as u64) as usize]
    }
}

fn card(r: &mut Lcg) -> Card {
    Card {
        rank: if r.chance(5) { 0 } else { 2 + r.below(13) as u8 },
        // few suits make flushes common
        suit: if r.chance(3) {
            0
        } else {
            let suits = if r.chance(50) { 2 } else { 4 };
            1 + r.below(suits) as u8
        },
        enh: if r.chance(60) { 0 } else { r.below(7) as u8 },
        edition: if r.chance(80) { 0 } else { r.below(4) as u8 },
        red: r.chance(10),
        debuff: r.chance(5),
    }
}

fn pred_arg(r: &mut Lcg, kind: u8) -> u32 {
    match kind {
        1 => 1 + r.below(4) as u32,
        2 => (r.next() as u32) & 0x7ffc,
        6 => (2 + r.below(13) as u32) | ((1 + r.below(4) as u32) << 8),
        _ => 0,
    }
}

fn rule(r: &mut Lcg) -> RuleIn {
    let mut card = Vec::new();
    for _ in 0..r.below(3) {
        let kind = r.below(7) as u8;
        card.push((kind, pred_arg(r, kind), r.f(&[0.0, 10.0, 31.0]), r.f(&[0.0, 3.0, 4.5]), r.f(&[1.0, 1.5, 2.0])));
    }
    let mut retrigger = Vec::new();
    for _ in 0..r.below(2) {
        let kind = r.below(7) as u8;
        retrigger.push((kind, pred_arg(r, kind), 1 + r.below(2) as i64));
    }
    let mut held = Vec::new();
    for _ in 0..r.below(2) {
        let kind = [0, 1, 2, 6][r.below(4) as usize];
        held.push((kind, pred_arg(r, kind), r.f(&[0.0, 13.0]), r.f(&[1.0, 1.5])));
    }
    let hand = if r.chance(70) {
        let kind = r.below(12) as u8;
        let arg = match kind {
            1 => (r.next() as u32) & 0xfff,
            5 => ((r.next() as u32) & 0xf) << 1,
            _ => 0,
        };
        let eff = |r: &mut Lcg| (r.f(&[0.0, 50.0, 100.0]), r.f(&[0.0, 4.0, 12.0]), r.f(&[1.0, 2.0, 3.0]));
        Some((kind, arg, r.f(&[0.0, 2.0, 20.0]), eff(r), eff(r)))
    } else {
        None
    };
    (card, retrigger, held, if r.chance(10) { 1 } else { 0 }, hand)
}

fn case(r: &mut Lcg) -> (Vec<Card>, Vec<(f64, f64, f64, bool)>, Vec<(u8, Option<RuleIn>)>, f64, f64) {
    let cards: Vec<Card> = (0..1 + r.below(10)).map(|_| card(r)).collect();
    let levels = (0..12).map(|_| (r.f(&[5.0, 30.0, 160.0]), r.f(&[1.0, 4.0, 16.0]), r.below(4) as f64, r.chance(30))).collect();
    let jokers = (0..r.below(6)).map(|_| (r.below(4) as u8, if r.chance(85) { Some(rule(r)) } else { None })).collect();
    (cards, levels, jokers, r.f(&[1.0, 2.0, 4.0]), r.f(&[0.0, 1.0, 3.0]))
}

#[test]
fn classify_and_score_match_reference() {
    let mut r = Lcg(0x5eed);
    let mut subsets = 0usize;
    for _ in 0..3000 {
        let (cards, levels, jokers, hands_left, discards_left) = case(&mut r);
        let new = Scorer::new(levels.clone(), jokers.clone(), hands_left, discards_left).unwrap();
        let old = OldScorer::new(levels, jokers, hands_left, discards_left).unwrap();
        each_play(&new, &cards, (0, 0, false), |score, name, combo, sc| {
            let played: Vec<Card> = combo.iter().map(|&i| cards[i]).collect();
            let (want_name, want_sc) = reference::classify(&played);
            assert_eq!((name, sc), (want_name, &want_sc[..]), "classify of combo {combo:?}");
            assert_eq!(classify(&played), (want_name, want_sc.clone()));
            let want = old.score(&cards, combo, name, sc);
            assert_eq!(score.to_bits(), want.to_bits(), "score of combo {combo:?}: {score} vs {want}");
            // scoring positions in any order give the same score
            let rev: Vec<usize> = sc.iter().rev().copied().collect();
            assert_eq!(new.score(&cards, combo, name, &rev).to_bits(), want.to_bits());
            subsets += 1;
        });
    }
    assert!(subsets > 100_000, "only {subsets} subsets");
}
