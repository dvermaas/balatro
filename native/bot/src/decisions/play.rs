//! Port of src/decisions.py lines 158-452 (except 181-199, which are in mod.rs): the hand handler and
//! its helpers. Plays are Python dicts, kept as `serde_json::Value` objects (the helpers add keys such
//! as "outlook"). See docs/port-handoff/decisions.md.

use crate::hands::{Info, Levels};
use crate::iface::RpcError;
use crate::jev_bot::JevBot;
use serde_json::Value;

// decisions.py 200-231 live in hands.rs; re-exported under their hands.rs names.
#[allow(unused_imports)]
pub(crate) use crate::hands::{BossAllow, _boss_allows, _boss_levels};

/// `scorer(infos, combo, name, scoring)` of any kind (a jokers.Scorer is wrapped by the caller).
pub(crate) type ScorerFn<'a> = &'a dyn Fn(&[Info], &[usize], &str, &[usize]) -> i64;
/// `allowed(combo, name)`.
pub(crate) type AllowedFn<'a> = &'a dyn Fn(&[usize], &str) -> bool;

// decisions.py 158-159
pub(crate) const CLEAR_MARGIN: f64 = 1.1;
pub(crate) const PLAY_NOW_UNDER_PACE: f64 = 0.85;

// decisions.py 257-262
pub(crate) const PLAY_FORECAST_BOSSES: &[&str] = &[
    "Small Blind", "Big Blind", "The Water", "The Goad", "The Club", "The Head",
    "The Window", "The Plant", "The Psychic", "The Eye", "The Mouth", "The Flint",
    "The Needle", "The Serpent", "The Wall", "The Manacle", "Violet Vessel",
];

// decisions.py 162-163
pub(crate) fn clears(_p: &Value, _info: &Value, _margin: f64) -> bool {
    todo!("decisions clears")
}

// decisions.py 166-178
pub(crate) fn play_obj(_p: &Value, _info: &Value, _margin: f64) -> Value {
    todo!("decisions play_obj")
}

// decisions.py 232-254. Returns the discard-plan dict.
pub(crate) fn preserve_discard(_hand: &[Value], _play: &Value, _scorer: ScorerFn, _max_discard: usize) -> Value {
    todo!("decisions preserve_discard")
}

// decisions.py 264-332. Returns the (possibly extended) list of plays.
#[allow(clippy::too_many_arguments)]
pub(crate) fn setup_plays(
    _gs: &Value, _hand: &[Value], _plays: &[Value], _levels: &Levels, _scorer: ScorerFn, _allowed: AllowedFn,
    _info: &Value, _margin: f64,
) -> Vec<Value> {
    todo!("decisions setup_plays")
}

/// decisions.py 335-452 `selecting_hand(bot, gs)`.
pub fn selecting_hand(_bot: &mut JevBot, _gs: &Value) -> Result<(), RpcError> {
    todo!("decisions selecting_hand")
}
