//! Port of src/decisions.py. Lines 1-157 (constants, _round_info, _deck_remaining, _levels,
//! _run_context, blind_select) live here; the rest is split by handler.

use crate::iface::RpcError;
use crate::jev_bot::JevBot;
use serde_json::Value;

pub mod booster;
pub mod play;
pub mod shop;

pub use booster::booster;
pub use play::selecting_hand;
pub use shop::shop;

/// decisions.py `blind_select(bot, gs)`.
pub fn blind_select(_bot: &mut JevBot, _gs: &Value) -> Result<(), RpcError> {
    todo!("decisions::blind_select")
}
