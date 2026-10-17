//! The JEV Balatro bot in Rust. Each module ports one Python module, line by line; see
//! docs/rust-port-guide.md for the conventions and the identity checks.

pub mod gs;
pub mod iface;
pub mod trace;

pub mod build;
pub mod codejev;
pub mod decisions;
pub mod hands;
pub mod jev_bot;
pub mod jokers;
pub mod odds;
pub mod prompts;
pub mod readiness;
pub mod strategy;
pub mod tarots;
