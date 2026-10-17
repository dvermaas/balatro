//! The bot's three outside interfaces. The replay harness (`trace.rs`) implements all three from a
//! recorded Python trace; production implements them with BalatroBot/rumbot, TypeSafe and the
//! Rust forecasts.

use serde_json::{Map, Value};

/// `balatro.RPCError`.
#[derive(Clone, Debug, PartialEq)]
pub struct RpcError {
    pub code: i64,
    pub message: String,
    pub name: Option<String>,
}

impl std::fmt::Display for RpcError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.message)
    }
}

/// `balatro.Balatro` / `sim_backend.SimBalatro`: `call(method, params)`. `state()` is
/// `call("gamestate", None)`.
pub trait Game {
    fn call(&mut self, method: &str, params: Option<&Value>) -> Result<Value, RpcError>;
    /// "balatrobot", "jackdaw", "rumbot", ...
    fn backend(&self) -> &str;
}

/// `TypeSafeClient.system_one(state=..., questions=...)`. Questions and answers are the SDK's
/// `model_dump()` dicts, keyed by question id:
///   choice: {"type": "choice", "instructions": str, "criteria": {key: desc}}
///     -> {"type": "choice", "choice": key, "confidence": f, "probabilities": {key: f}}
///   score:  {"type": "score", "instructions": str | dict, ...} -> {"type": "score", "score": f, ...}
///   noul:   {"type": "noul", "instructions": str, ...} -> {"type": "noul", "noul": f}
pub trait Jev {
    fn system_one(&mut self, state: &Value, questions: &Map<String, Value>) -> Result<JevResponse, String>;
}

#[derive(Clone, Debug)]
pub struct JevResponse {
    pub model: String,
    pub answers: Map<String, Value>,
    pub input_tokens: i64,
    pub output_tokens: i64,
}

/// The random or expensive calls the Python bot makes (recorded as `oracle` lines in a trace), so
/// decision code can be checked before the forecasts are ported. `pub_gs` is always the bot's
/// current state. Return values have exactly the Python return's JSON shape.
pub trait Oracle {
    /// `rollout.forecast(pub, n, bosses, keys, budget_s)`; keys sorted.
    fn forecast(&mut self, pub_gs: &Value, n: i64, bosses: i64, keys: &[String], budget_s: Option<f64>) -> Value;
    /// `rollout_items.forecast(pub, n, bosses, keys, budget_s)`; keys sorted.
    fn items_forecast(&mut self, pub_gs: &Value, n: i64, bosses: i64, keys: &[String], budget_s: Option<f64>) -> Value;
    /// `rollout_items.Picker(pub).values(pack, hand, money, jokers, joker_limit)`.
    fn picker_values(&mut self, pub_gs: &Value, args: &Value) -> Value;
    /// `rollout_items.Picker(pub).pick_pub(pub)`.
    fn pick_pub(&mut self, pub_gs: &Value) -> Value;
}
