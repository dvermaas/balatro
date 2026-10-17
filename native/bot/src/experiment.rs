//! Lab experiments that are not production policy (scripts/policy_lab.py BOT_ARMS, in Rust). Read once from
//! `JEVLAB_EXP` (comma-separated flags); unset = production behaviour. Used by jevlab only.
//!
//! Pair commitment (docs/lab-lessons.md "Open ideas"): the 5k-games teacher's highest hand at ante 6 is Pair
//! in 77% of its wins, with Pair jokers and Pair levels together. Flags:
//!   pair-planets  the shop planet rule always targets Pair; the pack Picker adds PAIR_PLANET to Mercury
//!   pair-jokers   Pair jokers clear the auto-buy bar PAIR_SE paired SEs earlier, and win early slot fills
//!   pair-keep     Pair jokers are never among the weakest jokers (swaps, Verdant Leaf sales)
//!   pair-play     a play containing a Pair within PAIR_PLAY of the best score is played instead

use std::sync::OnceLock;

pub const PAIR_PLANET: f64 = 0.5;
pub const PAIR_SE: f64 = 1.0;
pub const PAIR_PLAY: f64 = 0.85;
/// Jokers that pay for playing Pairs (or for repeating one hand, which Pair commitment does).
pub const PAIR_JOKERS: &[&str] = &["Jolly Joker", "Sly Joker", "The Duo", "Half Joker", "Supernova", "Card Sharp", "Spare Trousers", "Mad Joker", "Clever Joker"];

#[derive(Debug, Default)]
pub struct Exp {
    pub pair_planets: bool,
    pub pair_jokers: bool,
    pub pair_keep: bool,
    pub pair_play: bool,
}

pub fn get() -> &'static Exp {
    static E: OnceLock<Exp> = OnceLock::new();
    E.get_or_init(|| {
        let s = std::env::var("JEVLAB_EXP").unwrap_or_default();
        let has = |f: &str| s.split(',').any(|x| x.trim() == f || (x.trim() == "pair" && f.starts_with("pair-")));
        Exp { pair_planets: has("pair-planets"), pair_jokers: has("pair-jokers"), pair_keep: has("pair-keep"), pair_play: has("pair-play") }
    })
}

pub fn is_pair_joker(label: Option<&str>) -> bool {
    label.is_some_and(|l| PAIR_JOKERS.contains(&l))
}
