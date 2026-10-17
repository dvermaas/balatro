//! The JEV Balatro bot in Rust. Each module ports one Python module, line by line; see
//! docs/rust-port-guide.md for the conventions and the identity checks.

/// mimalloc for every binary and test that links the bot: the forecasts allocate heavily (JSON states,
/// rumbot clones) and spent about a third of their time in the system allocator.
#[global_allocator]
static GLOBAL: mimalloc::MiMalloc = mimalloc::MiMalloc;

pub mod gs;
pub mod iface;
pub mod trace;
pub mod prof;

pub mod analytics;
pub mod balatro;
pub mod benchmark;
pub mod build;
pub mod codejev;
pub mod decisions;
pub mod engine;
pub mod forecast_jokers;
pub mod forecast_stats;
pub mod forecast_state;
pub mod forecast;
pub mod forecast_apply;
pub mod forecast_items;
pub mod hands;
pub mod jev_bot;
pub mod jev_client;
pub mod jokers;
pub mod odds;
pub mod panel;
pub mod oracle;
pub mod picker;
pub mod prompts;
pub mod readiness;
pub mod recorder;
pub mod replay;
pub mod strategy;
pub mod tarots;
pub mod simtext;
