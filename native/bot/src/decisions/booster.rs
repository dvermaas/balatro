//! Port of src/decisions.py lines 851-987: booster, _gain. See docs/port-handoff/decisions.md.

use crate::iface::RpcError;
use crate::jev_bot::JevBot;
use serde_json::Value;

/// decisions.py 851-947 `booster(bot, gs)`.
pub fn booster(_bot: &mut JevBot, _gs: &Value) -> Result<(), RpcError> {
    todo!("decisions booster")
}

// decisions.py 950-987: `{measured_gain, _gain}`-style dict (may be empty); `remove` = joker index or None
pub(crate) fn gain(_gs: &Value, _recent: &[Value], _c: &Value, _remove: Option<usize>) -> Value {
    todo!("decisions gain")
}
