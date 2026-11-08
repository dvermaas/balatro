//! Four Fingers / Shortcut / Smeared Joker classification (lab `hand-mods`), against the game's rules as
//! rumbot's hands::evaluate implements them.
use jev_kernel::*;

fn c(rank: u8, suit: u8) -> Card {
    Card { rank, suit, enh: 0, edition: 0, red: false, debuff: false, extra: 0 }
}

fn cls(cs: &[Card], m: u8) -> (&'static str, Vec<usize>) {
    set_mods(m);
    let (n, sc) = classify(cs);
    set_mods(0);
    (HAND_NAMES[n], sc)
}

#[test]
fn four_fingers() {
    // A A 4 3 2: a 4-card straight (Ace low); both Aces score
    let h = [c(14, 4), c(14, 3), c(4, 2), c(3, 1), c(2, 3)];
    assert_eq!(cls(&h, 0).0, "Pair");
    assert_eq!(cls(&h, MOD_FOUR_FINGERS), ("Straight", vec![0, 1, 2, 3, 4]));
    // four Hearts and a Spade: a flush of the four Hearts
    let h = [c(2, 1), c(9, 1), c(11, 1), c(5, 4), c(7, 1)];
    assert_eq!(cls(&h, MOD_FOUR_FINGERS), ("Flush", vec![0, 1, 2, 4]));
    // a 4-card play
    assert_eq!(cls(&[c(9, 2), c(8, 1), c(7, 3), c(6, 4)], MOD_FOUR_FINGERS).0, "Straight");
    // 2S 3S 4S 5H 9S: a 4-card straight and a 4-card flush make a Straight Flush, all five scoring
    let h = [c(2, 4), c(3, 4), c(4, 4), c(5, 1), c(9, 4)];
    assert_eq!(cls(&h, MOD_FOUR_FINGERS), ("Straight Flush", vec![0, 1, 2, 3, 4]));
    // a pair still beats nothing: 3 cards
    assert_eq!(cls(&[c(5, 1), c(5, 2), c(9, 3)], MOD_FOUR_FINGERS), ("Pair", vec![0, 1]));
}

#[test]
fn shortcut() {
    let h = [c(10, 1), c(8, 2), c(6, 3), c(4, 4), c(2, 1)];
    assert_eq!(cls(&h, 0).0, "High Card");
    assert_eq!(cls(&h, MOD_SHORTCUT).0, "Straight");
    // one-rank gaps anywhere; a two-rank gap breaks it
    let h = [c(10, 1), c(8, 2), c(7, 3), c(6, 4), c(5, 1)];
    assert_eq!(cls(&h, MOD_SHORTCUT).0, "Straight");
    let h = [c(10, 1), c(7, 2), c(6, 3), c(5, 4), c(4, 1)];
    assert_eq!(cls(&h, MOD_SHORTCUT).0, "High Card");
    let h = [c(13, 1), c(10, 2), c(6, 3), c(5, 4), c(4, 1)];
    assert_eq!(cls(&h, MOD_SHORTCUT).0, "High Card");
}

#[test]
fn smeared() {
    let h = [c(2, 1), c(9, 2), c(11, 1), c(5, 2), c(7, 1)];
    assert_eq!(cls(&h, 0).0, "High Card");
    assert_eq!(cls(&h, MOD_SMEARED), ("Flush", vec![0, 1, 2, 3, 4]));
    let h = [c(2, 3), c(9, 4), c(11, 3), c(5, 2), c(7, 4)];
    assert_eq!(cls(&h, MOD_SMEARED).0, "High Card");
}

#[test]
fn unmodded_matches() {
    // with flags set but irrelevant, ordinary hands classify as before
    for h in [vec![c(5, 1), c(5, 2), c(9, 3), c(9, 4), c(9, 1)], vec![c(14, 1), c(13, 1), c(12, 1), c(11, 1), c(10, 1)],
              vec![c(2, 1), c(2, 2), c(3, 3), c(3, 4), c(7, 1)], vec![c(8, 1)]] {
        let a = cls(&h, 0);
        let b = cls(&h, MOD_SMEARED | MOD_SHORTCUT | MOD_FOUR_FINGERS);
        assert_eq!(a.0, b.0, "{:?}", h.iter().map(|x| (x.rank, x.suit)).collect::<Vec<_>>());
    }
}
