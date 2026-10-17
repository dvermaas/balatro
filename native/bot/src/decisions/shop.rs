//! Port of src/decisions.py lines 453-850: shop, its forecasts and _shop_act. The Python nested `obj`
//! inside `shop` stays a private closure/helper of `shop`. See docs/port-handoff/decisions.md.

use crate::iface::RpcError;
use crate::jev_bot::{Action, JevBot};
use serde_json::{Map, Value};
use std::collections::HashSet;

/// `_score_items` input: `{key: (desc, item_obj, action)}` in insertion order.
pub(crate) type ShopItems = Vec<(String, (String, Value, Action))>;

// decisions.py 453-465
pub(crate) fn discard_obj(_d: &Value) -> Value {
    todo!("decisions discard_obj")
}

// decisions.py 468-484: (scores {key: score}, meta)
pub(crate) fn score_items(
    _bot: &mut JevBot, _gs: &Value, _qname: &str, _state: &Value, _items: &ShopItems,
) -> (Map<String, Value>, Value) {
    todo!("decisions score_items")
}

// decisions.py 487-496: indices of the n weakest jokers (set; used for membership only)
pub(crate) fn weakest_jokers(_gs: &Value, _recent: &[Value], _n: usize) -> HashSet<usize> {
    todo!("decisions weakest_jokers")
}

// decisions.py 499-526: rollout.forecast result or None on failure; cached on bot._forecast_cache
pub(crate) fn shop_forecast(_bot: &mut JevBot, _gs: &Value, _recent: &[Value], _jokers_full: bool) -> Option<Value> {
    todo!("decisions shop_forecast")
}

// decisions.py 529-545
pub(crate) fn log_forecast_error(_bot: &mut JevBot, _gs: &Value, _what: &str) {
    todo!("decisions log_forecast_error")
}

// decisions.py 548-574: rollout_items.forecast result or None; cached on bot._item_forecast_cache
pub(crate) fn item_forecast(_bot: &mut JevBot, _gs: &Value, _fc: &Value) -> Option<Value> {
    todo!("decisions item_forecast")
}

// decisions.py 577-578
pub(crate) fn pct(_p: f64) -> String {
    todo!("decisions pct")
}

// decisions.py 581-594
pub(crate) fn forecast_fields(_fc: &Value, _key: &str) -> Value {
    todo!("decisions forecast_fields")
}

// decisions.py 597-606
pub(crate) fn forecast_note(_gs: &Value, _fc: &Value) -> String {
    todo!("decisions forecast_note")
}

// decisions.py 609-613
pub(crate) fn economic(_obj: &Value, _c: &Value) -> bool {
    todo!("decisions economic")
}

/// decisions.py 616-823 `shop(bot, gs)` (nested `obj` at 647 is a helper inside).
pub fn shop(_bot: &mut JevBot, _gs: &Value) -> Result<(), RpcError> {
    todo!("decisions shop")
}

// decisions.py 826-848: runs the action; on RPCError adds json.dumps(action) to bot._failed and returns Err
pub(crate) fn shop_act(_bot: &mut JevBot, _gs: &Value, _action: &Action) -> Result<(), RpcError> {
    todo!("decisions shop_act")
}
