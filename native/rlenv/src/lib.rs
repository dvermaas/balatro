//! Fast RL environment on rumbot-sim (see `env`), policies (`policy`), network inference (`net`) and
//! trajectory files (`traj`).

pub mod env;
pub use rlcore::net;
pub mod policy;
pub mod traj;
