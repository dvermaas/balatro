//! Schema checks on random rumbot games: every legal strategic action has a vocabulary entry (joker swaps
//! aside), both mapping directions agree, and the observation ignores the draw pile's order, the RNG and
//! the joker order.

use crate::obs::Encoder;
use crate::vocab::*;
use rumbot_sim::game::{Action, ActionType, GameState, Phase};
use rumbot_sim::rng::RunRng;

struct Xs(u64);
impl Xs {
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        self.0
    }
    fn pick(&mut self, n: usize) -> usize {
        (self.next() % n as u64) as usize
    }
}

fn perturbed(g: &GameState, r: &mut Xs) -> GameState {
    let mut h = g.deep_clone();
    let n = h.draw_pile.len();
    for i in (1..n).rev() {
        let j = r.pick(i + 1);
        h.draw_pile.swap(i, j);
    }
    h.rng = RunRng::new(format!("PERTURB{}", r.next() % 1000));
    h.jokers.reverse();
    h.consumables.reverse();
    h
}

fn check_state(g: &GameState, enc: &mut Encoder, r: &mut Xs, stats: &mut [usize; 7]) {
    let legal = g.legal_actions();
    let with = legal_with_ids(g);
    for a in &legal {
        match vocab_of(g, a) {
            Ok(v) => {
                let b = to_action(g, v, Some(&a.cards)).expect("legal id resolves");
                assert!(g.is_legal(&b), "resolved action legal: {b}");
                assert_eq!(b.r#type, a.r#type, "same type for {a}");
                assert_eq!(vocab_of(g, &b), Ok(v), "same item for {a} -> {b}");
                assert_eq!(b.cards, a.cards, "targets kept for {a}");
                if b.index != a.index {
                    stats[2] += 1;
                }
            }
            Err(Unexpressible::JokerOrder) => {}
            Err(e) => panic!("unexpressible legal action {a} in {:?}: {e:?}", g.phase),
        }
    }
    let mark = rumbot_sim::cards::sort_id_now();
    let swaps = legal_replaces(g);
    assert_eq!(rumbot_sim::cards::sort_id_now(), mark, "legal_replaces left the sort ids alone");
    for (v, sell, buy) in &swaps {
        assert!(matches!(decode(*v), Some((Family::ReplaceJoker, _))));
        assert!(g.is_legal(sell) && !g.is_legal(buy), "swap: sale legal, buy not yet");
        let mut h = g.deep_clone();
        h.step(sell);
        assert!(h.is_legal(buy), "swap: buy legal after the sale");
        rumbot_sim::cards::set_sort_id(mark);
    }
    let mask = legal_mask_ids(g);
    for &v in &mask {
        if matches!(decode(v as usize), Some((Family::ReplaceJoker, _))) {
            assert!(swaps.iter().any(|(w, _, _)| *w == v as usize));
            continue;
        }
        let b = to_action(g, v as usize, None).expect("mask id resolves");
        assert!(g.is_legal(&b));
        assert_eq!(vocab_of(g, &b), Ok(v as usize));
    }
    stats[0] += 1;
    stats[1] += mask.len();
    for a in &legal {
        if !a.cards.is_empty() {
            stats[4] += 1;
        }
        if a.r#type == ActionType::PickPack && matches!(g.pack_options.get(a.index as usize), Some(rumbot_sim::game::PackChoice::Card(_))) {
            stats[5] += 1;
        }
    }
    assert!(with.len() + swaps.len() >= mask.len());
    let (i1, v1) = enc.encode(g);
    assert!(v1.iter().all(|x| x.is_finite()));
    assert!(i1.windows(2).all(|w| w[0] < w[1]));
    let h = perturbed(g, r);
    let (i2, v2) = enc.encode(&h);
    assert_eq!(i1, i2, "obs support moved under draw-pile shuffle / reseed / reorder in {:?}", g.phase);
    assert_eq!(v1, v2, "obs values moved under draw-pile shuffle / reseed / reorder");
    let mut m2 = legal_mask_ids(&h);
    m2.sort_unstable();
    assert_eq!(mask, m2, "mask moved");
    stats[3] = stats[3].max(i1.len());
}

#[test]
fn schema_dims() {
    assert!(crate::obs_dim() < 65535);
    assert!(crate::n_actions() < 65535);
    for id in 0..crate::n_actions() {
        if let Some((f, item)) = decode(id) {
            assert_eq!(family_start(f) + item, id);
        }
    }
    let s = crate::schema_json();
    assert!(s.contains("\"obs_dim\""));
}

#[test]
fn random_games_round_trip_and_leak_free() {
    let mut r = Xs(0x9E37_79B9_7F4A_7C15);
    let mut enc = Encoder::new();
    let mut stats = [0usize; 7];
    let mut max_ante = 0;
    for game in 0..400 {
        let mut g = GameState::new(format!("RLT{game:05}"), "Red Deck", 1);
        for _ in 0..3000 {
            if g.is_over() {
                break;
            }
            if strategic_phase(g.phase) {
                check_state(&g, &mut enc, &mut r, &mut stats);
                let with = legal_with_ids(&g);
                // lean towards leaving / skipping so games move on, but take items often
                let a: Action = if r.pick(3) == 0 || with.len() == 1 {
                    with[0].1.clone()
                } else {
                    with[r.pick(with.len())].1.clone()
                };
                // now and then a joker swap (sale then buy) when one is legal
                let sw = legal_replaces(&g);
                if !sw.is_empty() && r.pick(3) == 0 {
                    let (_, sell, buy) = sw[r.pick(sw.len())].clone();
                    g.step(&sell);
                    assert!(g.is_legal(&buy));
                    g.step(&buy);
                    stats[6] += 1;
                    continue;
                }
                // a random reroll loop can starve the run; leave the shop instead now and then
                let a = if a.r#type == ActionType::Reroll && r.pick(2) == 0 { Action::new(ActionType::LeaveShop) } else { a };
                g.step(&a);
                // tiny blind targets so random shopping still reaches the late antes
                if a.r#type == ActionType::SelectBlind && r.pick(4) != 0 {
                    if let Some(b) = g.blind.as_mut() {
                        b.target = 1;
                    }
                }
            } else if g.phase == Phase::Playing {
                // a cheap round player: the play with the most cards, first such
                let la = g.legal_actions();
                let a = la.iter().filter(|a| a.r#type == ActionType::Play).max_by_key(|a| a.cards.len()).unwrap().clone();
                g.step(&a);
            } else {
                break;
            }
            max_ante = max_ante.max(g.ante);
        }
    }
    eprintln!("states {} mean mask {:.1} other-slot resolutions {} max nnz {} max ante {max_ante} targeted legal actions {} standard-card picks {} swaps taken {}", stats[0], stats[1] as f64 / stats[0] as f64, stats[2], stats[3], stats[4], stats[5], stats[6]);
    assert!(stats[0] > 1000);
}

/// v2 joker swaps on rich random games (money topped up, jokers bought whenever there is room), so the row
/// fills and swaps come up: checks come from `check_state`, and the swaps are taken often.
#[test]
fn swaps_on_full_rows() {
    let mut r = Xs(0xD1B5_4A32_D192_ED03);
    let mut enc = Encoder::new();
    let mut stats = [0usize; 7];
    let mut offered = 0;
    for game in 0..150 {
        let mut g = GameState::new(format!("RLS{game:05}"), "Red Deck", 1);
        for _ in 0..3000 {
            if g.is_over() {
                break;
            }
            if strategic_phase(g.phase) {
                if g.phase == Phase::Shop && r.pick(2) == 0 {
                    g.money = g.money.max(40);
                }
                check_state(&g, &mut enc, &mut r, &mut stats);
                let sw = legal_replaces(&g);
                offered += !sw.is_empty() as usize;
                if !sw.is_empty() && r.pick(2) == 0 {
                    let (_, sell, buy) = sw[r.pick(sw.len())].clone();
                    g.step(&sell);
                    assert!(g.is_legal(&buy));
                    g.step(&buy);
                    stats[6] += 1;
                    continue;
                }
                let with = legal_with_ids(&g);
                let buy = with.iter().find(|(v, _)| matches!(decode(*v), Some((Family::BuyJoker, _)))).map(|x| x.1.clone());
                let a = match buy {
                    Some(b) => b,
                    None if r.pick(3) == 0 || with.len() == 1 => with[0].1.clone(),
                    None => with[r.pick(with.len())].1.clone(),
                };
                let a = if a.r#type == ActionType::Reroll && r.pick(2) == 0 { Action::new(ActionType::LeaveShop) } else { a };
                g.step(&a);
                if a.r#type == ActionType::SelectBlind {
                    if let Some(b) = g.blind.as_mut() {
                        b.target = 1;
                    }
                }
            } else if g.phase == Phase::Playing {
                let la = g.legal_actions();
                let a = la.iter().filter(|a| a.r#type == ActionType::Play).max_by_key(|a| a.cards.len()).unwrap().clone();
                g.step(&a);
            } else {
                break;
            }
        }
    }
    eprintln!("swap test: states {} with swaps offered {offered}, swaps taken {}", stats[0], stats[6]);
    assert!(stats[6] > 100);
}
