//! RL schema on rumbot-sim, shared by the bot's behaviour-cloning logger (`jevlab`, `JEVLAB_RLLOG`) and
//! the fast environment: an order-free observation (`obs`), an item-keyed action vocabulary (`vocab`)
//! with its legal mask and both directions of the mapping to `rumbot_sim::game::Action`, and the game
//! log format (`log`). Depends on rumbot only, so neither the bot nor an env crate forms a cycle, and it
//! is outside the bot's version hash (`recorder::behaviour_sources`).
//!
//! Datasets and weights depend on the schema: bump `SCHEMA_VERSION` on any change to the observation
//! layout or the vocabulary. `schema_hash()` (FNV-1a of `schema_json()`) is written into every log file
//! and catches a change that forgot the bump.

pub mod ids;
pub mod log;
pub mod net;
pub mod obs;
pub mod vocab;

#[cfg(test)]
mod tests;

pub const SCHEMA_VERSION: u32 = 1;

pub fn obs_dim() -> usize {
    obs::layout().dim
}

pub fn n_actions() -> usize {
    vocab::n_actions()
}

/// The whole schema as JSON: version, dims, observation blocks, vocabulary, enum codes.
pub fn schema_json() -> String {
    let phases = ["blind_select", "playing", "round_eval", "shop", "pack", "game_over", "won"];
    let types: Vec<String> = log::ACTION_TYPES.iter().map(|t| format!("\"{}\"", t.as_str())).collect();
    format!(
        "{{\"schema_version\":{},\"obs_dim\":{},\"n_actions\":{},\"phases\":[{}],\"action_types\":[{}],\"blocks\":{},\"vocab\":{}}}",
        SCHEMA_VERSION,
        obs_dim(),
        n_actions(),
        phases.iter().map(|p| format!("\"{p}\"")).collect::<Vec<_>>().join(","),
        types.join(","),
        obs::schema_blocks_json(),
        vocab::vocab_json()
    )
}

pub fn schema_hash() -> u64 {
    rumbot_sim::state::fnv1a64(schema_json().as_bytes())
}
