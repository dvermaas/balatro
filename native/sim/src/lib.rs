//! PyO3 module `jev_sim`: shop-forecast rollouts on rumbot-sim (stage B of the Rust port).
//! Only the Python glue lives here; the rollout code is in the `simcore` crate.

use jev_kernel::RuleIn;
use pyo3::exceptions::{PyRuntimeError, PyValueError};
use pyo3::prelude::*;
use rumbot_sim::consumables;
use rumbot_sim::jokers as rjokers;
use rumbot_sim::rng::RunRng;
use simcore::*;

fn verr(e: String) -> PyErr {
    PyValueError::new_err(e)
}

// Python API
// ---------------------------------------------------------------------------

/// Rollouts of one option from a described shop state: one per seed (rollout._seed(k)), each a
/// list of per-blind rows (ante, type, blind name, target, chips, cleared).
#[pyfunction]
fn rollouts(state_json: &str, policy_jokers: Vec<(u8, Option<RuleIn>)>, seeds: Vec<String>, bosses: usize) -> PyResult<Vec<Vec<Row>>> {
    let desc: StateDesc = serde_json::from_str(state_json).map_err(|e| PyValueError::new_err(e.to_string()))?;
    let base = build(&desc).map_err(verr)?;
    let policy = Policy { jokers: policy_jokers };
    let mut out = Vec::with_capacity(seeds.len());
    for seed in seeds {
        let mut g = base.deep_clone();
        g.seed = seed.clone();
        g.rng = RunRng::new(seed);
        if let Some(pools) = &desc.rng_pools {
            g.rng.pools = pools.clone(); // v25: an option that drew random numbers continues its streams
        }
        out.push(play_horizon(&mut g, &policy, bosses, 600).map_err(PyRuntimeError::new_err)?);
    }
    Ok(out)
}

/// v25, rollout_items: one strict rollout from a described shop state mid-rollout, its pseudoseed
/// streams continued from `rng_pools`. None when it reaches a state only the Python rollout handles
/// (see play_horizon_traced); the caller then replays it in Python.
#[pyfunction]
fn rollout_strict(state_json: &str, policy_jokers: Vec<(u8, Option<RuleIn>)>, seed: String, bosses: usize) -> PyResult<Option<Vec<Row>>> {
    let desc: StateDesc = serde_json::from_str(state_json).map_err(|e| PyValueError::new_err(e.to_string()))?;
    let mut g = build(&desc).map_err(verr)?;
    g.seed = seed.clone();
    g.rng = RunRng::new(seed);
    if let Some(pools) = &desc.rng_pools {
        g.rng.pools = pools.clone();
    }
    let policy = Policy { jokers: policy_jokers };
    let no_items = NO_ITEMS();
    match play_horizon_traced(&mut g, &policy, bosses, 900, None, Some(desc.items.as_ref().unwrap_or(&no_items))) {
        Ok(rows) => Ok(Some(rows)),
        Err(e) if e == STRICT_ABORT => Ok(None),
        Err(e) => Err(PyRuntimeError::new_err(e)),
    }
}

/// One rollout with a decision log: (rows, ["play [..] hand=.. chips=..", ...]).
#[pyfunction]
fn rollout_trace(state_json: &str, policy_jokers: Vec<(u8, Option<RuleIn>)>, seed: String, bosses: usize) -> PyResult<(Vec<Row>, Vec<String>)> {
    let desc: StateDesc = serde_json::from_str(state_json).map_err(|e| PyValueError::new_err(e.to_string()))?;
    let mut g = build(&desc).map_err(verr)?;
    g.seed = seed.clone();
    g.rng = RunRng::new(seed);
    let policy = Policy { jokers: policy_jokers };
    let mut log = Vec::new();
    let rows = play_horizon_traced(&mut g, &policy, bosses, 600, Some(&mut log), None).map_err(PyRuntimeError::new_err)?;
    Ok((rows, log))
}

/// Registered names, for the Python side's mapping checks: (jokers, consumables, voucher keys, boss keys).
#[pyfunction]
fn names() -> (Vec<String>, Vec<String>, Vec<String>, Vec<String>) {
    (
        rjokers::all_specs().iter().map(|s| s.name.to_string()).collect(),
        consumables::all_specs().iter().map(|s| s.name.to_string()).collect(),
        rumbot_sim::shop::all_vouchers().iter().map(|v| v.key.to_string()).collect(),
        rumbot_sim::boss_data::BOSS_DATA.iter().map(|b| b.key.to_string()).collect(),
    )
}

#[pymodule]
fn jev_sim(m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_function(wrap_pyfunction!(rollouts, m)?)?;
    m.add_function(wrap_pyfunction!(names, m)?)?;
    m.add_function(wrap_pyfunction!(rollout_trace, m)?)?;
    m.add_function(wrap_pyfunction!(rollout_strict, m)?)?;
    Ok(())
}
