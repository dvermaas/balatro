//! Port of src/decisions.py lines 453-850: shop, its forecasts and _shop_act. See docs/rust-port-guide.md.

use crate::iface::RpcError;
use crate::jev_bot::JevBot;
use serde_json::Value;

/// decisions.py `shop(bot, gs)`.
pub fn shop(_bot: &mut JevBot, _gs: &Value) -> Result<(), RpcError> {
    todo!("decisions::shop")
}
