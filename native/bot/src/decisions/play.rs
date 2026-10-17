//! Port of src/decisions.py lines 158-452: selecting_hand and its helpers. See docs/rust-port-guide.md.

use crate::iface::RpcError;
use crate::jev_bot::JevBot;
use serde_json::Value;

/// decisions.py `selecting_hand(bot, gs)`.
pub fn selecting_hand(_bot: &mut JevBot, _gs: &Value) -> Result<(), RpcError> {
    todo!("decisions::selecting_hand")
}
