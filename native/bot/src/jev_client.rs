//! The real JEV: the `typesafe` crate (native/typesafe, same behaviour as typesafe-sdk 0.7.4) behind
//! `iface::Jev`, plus `jev_bot.load_env` / the key lookup from `JevBot.__init__`.

use crate::iface::{Jev, JevResponse};
use serde_json::{Map, Value};
use std::path::Path;
use typesafe::{Transport, TypeSafeClient, UreqTransport};

/// `TypeSafeClient(api_key=key, model=model)` used through `Jev::system_one`.
pub struct TypeSafeJev<T: Transport = UreqTransport> {
    client: TypeSafeClient<T>,
}

impl TypeSafeJev<UreqTransport> {
    /// `JevBot.__init__` without a `jev`: key from `JEV_API_KEY` / `TYPESAFE_API_KEY` (see
    /// `api_key_from_env`), model `jev-latest` by default.
    pub fn new(api_key: &str, model: &str) -> Result<Self, String> {
        TypeSafeClient::new(Some(api_key), Some(model)).map(|client| TypeSafeJev { client }).map_err(|e| e.to_string())
    }
}

impl<T: Transport> TypeSafeJev<T> {
    pub fn with_transport(api_key: &str, model: &str, transport: T) -> Result<Self, String> {
        TypeSafeClient::with_transport(Some(api_key), Some(model), transport)
            .map(|client| TypeSafeJev { client })
            .map_err(|e| e.to_string())
    }

    /// The model name the client was made with (`TypeSafeClient.model`).
    pub fn model(&self) -> &str {
        self.client.model()
    }
}

impl<T: Transport> Jev for TypeSafeJev<T> {
    // jev_bot.query: `resp.usage.input_tokens or 0` (0 when usage is missing or the count is None).
    fn system_one(&mut self, state: &Value, questions: &Map<String, Value>) -> Result<JevResponse, String> {
        let r = self.client.system_one(state, questions).map_err(|e| e.to_string())?;
        Ok(JevResponse {
            model: r.model,
            answers: r.answers,
            input_tokens: r.input_tokens.unwrap_or(0),
            output_tokens: r.output_tokens.unwrap_or(0),
        })
    }
}

/// `jev_bot.load_env` parsing: one `KEY=VALUE` per line, `#` lines skipped, value stripped of
/// surrounding quotes. A line without `=` is a key with an empty value, as in Python's `partition`.
pub fn parse_env_file(text: &str) -> Vec<(String, String)> {
    let mut out = Vec::new();
    for line in text.lines() {
        let line = line.trim();
        let (k, v) = match line.split_once('=') {
            Some((k, v)) => (k, v),
            None => (line, ""),
        };
        if !k.is_empty() && !k.starts_with('#') {
            out.push((k.to_string(), v.trim().trim_matches('"').trim_matches('\'').to_string()));
        }
    }
    out
}

/// `jev_bot.load_env(path)`: sets each variable only when it is not already in the environment
/// (`os.environ.setdefault`). A missing file does nothing.
pub fn load_env(path: &Path) -> std::io::Result<()> {
    if !path.exists() {
        return Ok(());
    }
    for (k, v) in parse_env_file(&std::fs::read_to_string(path)?) {
        if std::env::var_os(&k).is_none() {
            std::env::set_var(&k, &v);
        }
    }
    Ok(())
}

/// `os.environ.get("JEV_API_KEY") or os.environ.get("TYPESAFE_API_KEY")`.
pub fn api_key_from_env() -> Option<String> {
    ["JEV_API_KEY", "TYPESAFE_API_KEY"]
        .iter()
        .filter_map(|k| std::env::var(k).ok())
        .find(|v| !v.is_empty())
}
