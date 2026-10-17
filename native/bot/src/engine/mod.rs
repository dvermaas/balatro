//! `Game` backed by rumbot-sim: BalatroBot's JSON-RPC shape on the Rust engine
//! (`src/sim_backend.py` + Jackdaw's `bridge/backend.py` / `deserializer.py`).
//! `actions.rs` maps the RPC methods to rumbot actions, `view.rs` builds the gamestate JSON.
//! Card effect text (`sim_text.py`) is phase 2: see docs/port-handoff/engine.md.

use crate::iface::{Game, RpcError};
use rumbot_sim::game::{GameState, Phase};
use rumbot_sim::hands::HandType;
use serde_json::{json, Value};
use std::collections::HashMap;

pub mod actions;
pub mod view;

pub const BACKEND_NAME: &str = "rumbot";

pub fn rpc_error(code: i64, message: impl Into<String>) -> RpcError {
    let name = match code {
        -32001 => Some("BAD_REQUEST"),
        -32002 => Some("INVALID_STATE"),
        -32003 => Some("NOT_ALLOWED"),
        -32000 => Some("INTERNAL_ERROR"),
        _ => None,
    };
    RpcError { code, message: message.into(), name: name.map(String::from) }
}

/// `SimBalatro`: the rumbot game plus the few things BalatroBot reports that rumbot does not keep.
pub struct RumbotGame {
    pub g: Option<GameState>,
    pub calls: u64,
    /// `current_round.hands_played` (rumbot's `hands_played` is the run's total).
    pub(crate) round_hands_played: i32,
    /// a round has been started since `start` (before that Python reports 0 hands/discards).
    pub(crate) round_started: bool,
    /// (hands_left, discards_left, discards_used) as the last round left them (Python keeps them through ROUND_EVAL/SHOP; rumbot resets).
    pub(crate) last_round: (i32, i32, i32),
    /// `chips` as the last play left it (Python keeps it through ROUND_EVAL/SHOP until the next blind starts).
    pub(crate) last_chips: i64,
    /// `hands[..].played_this_round` (rumbot keeps only a set).
    pub(crate) played_this_round: HashMap<HandType, i32>,
    /// The pack was bought in the shop (not opened by a tag): the shop areas stay in the view.
    pub(crate) pack_from_shop: bool,
    /// `_bb_shop_frozen`: (count, limit) of shop, vouchers, packs as of the last shop view.
    pub(crate) shop_frozen: Option<[(usize, usize); 3]>,
    pub(crate) deck_limit: usize,
    pub(crate) last_phase: Option<Phase>,
}

impl RumbotGame {
    pub fn new() -> Self {
        RumbotGame {
            g: None,
            calls: 0,
            round_hands_played: 0,
            round_started: false,
            last_round: (0, 0, 0),
            last_chips: 0,
            played_this_round: HashMap::new(),
            pack_from_shop: false,
            shop_frozen: None,
            deck_limit: 52,
            last_phase: None,
        }
    }

    pub fn state(&mut self) -> Result<Value, RpcError> {
        self.call("gamestate", None)
    }

    fn view(&mut self) -> Value {
        match &self.g {
            None => json!({"state": "MENU", "round_num": 0, "ante_num": 0, "money": 0}),
            Some(_) => view::serialize(self),
        }
    }
}

impl Default for RumbotGame {
    fn default() -> Self {
        Self::new()
    }
}

impl Game for RumbotGame {
    // sim_backend.py SimBalatro.call
    fn call(&mut self, method: &str, params: Option<&Value>) -> Result<Value, RpcError> {
        self.calls += 1;
        let empty = Value::Object(Default::default());
        let params = match params {
            Some(Value::Object(_)) => params.unwrap(),
            _ => &empty,
        };
        match method {
            "health" => Ok(json!({"status": "ok"})),
            "gamestate" => Ok(self.view()),
            "menu" => {
                self.g = None;
                Ok(self.view())
            }
            "start" => {
                actions::start(self, params)?;
                Ok(self.view())
            }
            _ => {
                if self.g.is_none() {
                    return Err(rpc_error(-32002, format!("Method '{method}' requires an active run")));
                }
                actions::act(self, method, params)?;
                Ok(self.view())
            }
        }
    }

    fn backend(&self) -> &str {
        BACKEND_NAME
    }
}
