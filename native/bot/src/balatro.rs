//! `src/balatro.py`: the BalatroBot JSON-RPC client (http://127.0.0.1:12346) as a `Game`
//! (backend "balatrobot"). Request bodies match `json.dumps` key order (jsonrpc, method, id, params)
//! and ids count from 1 per client, like `itertools.count(1)`.
//!
//! Errors: a JSON-RPC `error` becomes `RpcError`, whose `message` is Python's `str(RPCError)`
//! ("name: message", or "code: message" when there is no name). Transport failures (refused, HTTP
//! status, timeout, bad JSON, missing "result") are Python exceptions that `step()` does not catch,
//! so they panic here too, with the same kind of text.

use crate::gs::truthy;
use crate::iface::{Game, RpcError};
use serde_json::{json, Map, Value};
use std::time::Duration;

pub const DEFAULT_URL: &str = "http://127.0.0.1:12346";
/// `Balatro(timeout=60)`: the panel's "game not responding" hint depends on it.
pub const DEFAULT_TIMEOUT: Duration = Duration::from_secs(60);

pub struct Balatro {
    pub url: String,
    pub timeout: Duration,
    next_id: i64,
}

impl Default for Balatro {
    fn default() -> Self {
        Self::new()
    }
}

impl Balatro {
    /// `Balatro()`: http://127.0.0.1:12346, 60 s timeout.
    pub fn new() -> Self {
        Self::with_url(DEFAULT_URL, DEFAULT_TIMEOUT)
    }

    pub fn with_url(url: &str, timeout: Duration) -> Self {
        Balatro { url: url.to_string(), timeout, next_id: 1 }
    }

    /// The request body `Balatro.call` sends (also takes the next id).
    pub fn request_body(&mut self, method: &str, params: Option<&Value>) -> String {
        let mut body = Map::new();
        body.insert("jsonrpc".into(), json!("2.0"));
        body.insert("method".into(), json!(method));
        body.insert("id".into(), json!(self.next_id));
        self.next_id += 1;
        if let Some(p) = params {
            body.insert("params".into(), p.clone());
        }
        serde_json::to_string(&Value::Object(body)).expect("request body")
    }

    /// `Balatro.state()`.
    pub fn state(&mut self) -> Result<Value, RpcError> {
        self.call("gamestate", None)
    }
}

impl Game for Balatro {
    // balatro.py Balatro.call
    fn call(&mut self, method: &str, params: Option<&Value>) -> Result<Value, RpcError> {
        let body = self.request_body(method, params);
        let sent = ureq::post(&self.url)
            .timeout(self.timeout)
            .set("Content-Type", "application/json")
            .send_string(&body);
        let text = match sent {
            Ok(r) => r.into_string().unwrap_or_else(|e| panic!("balatrobot {method}: {e}")),
            // urllib raises HTTPError / URLError here, not RPCError.
            Err(e) => panic!("balatrobot {method}: {e}"),
        };
        let resp: Value = serde_json::from_str(&text).unwrap_or_else(|e| panic!("balatrobot {method}: bad JSON: {e}"));
        if resp.get("error").is_some() {
            return Err(rpc_error(&resp["error"]));
        }
        // Python: resp["result"] (KeyError escapes when absent).
        match resp.get("result") {
            Some(r) => Ok(r.clone()),
            None => panic!("balatrobot {method}: KeyError: 'result'"),
        }
    }

    fn backend(&self) -> &str {
        "balatrobot"
    }
}

/// `RPCError(e.get("code"), e.get("message"), (e.get("data") or {}).get("name"))` and its
/// `super().__init__(f"{name or code}: {message}")`.
pub fn rpc_error(e: &Value) -> RpcError {
    let name_v = match e.get("data") {
        Some(d) if d.is_object() => d.get("name").cloned().unwrap_or(Value::Null),
        _ => Value::Null,
    };
    let code_v = e.get("code").cloned().unwrap_or(Value::Null);
    let message_v = e.get("message").cloned().unwrap_or(Value::Null);
    let head = if truthy(&name_v) { py_str(&name_v) } else { py_str(&code_v) };
    RpcError {
        code: code_v.as_i64().unwrap_or(0),
        message: format!("{head}: {}", py_str(&message_v)),
        name: name_v.as_str().map(String::from),
    }
}

/// Python `str()` of a JSON value (None, True/False, int, float repr, str). Lists and dicts are
/// not expected here and fall back to JSON text.
pub fn py_str(v: &Value) -> String {
    match v {
        Value::Null => "None".into(),
        Value::Bool(true) => "True".into(),
        Value::Bool(false) => "False".into(),
        Value::String(s) => s.clone(),
        Value::Number(n) => match n.as_i64() {
            Some(i) => i.to_string(),
            None => pycompat::pyfloat(n.as_f64().unwrap_or(f64::NAN)),
        },
        other => other.to_string(),
    }
}
