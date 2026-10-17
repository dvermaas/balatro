//! Port of src/rollout.py forecast/_forecast/run_options/_run_options_native (lines 585-781): the
//! shop forecast end to end. Options are applied by `forecast_apply` (RunRng set first), the
//! rollouts run on simcore, the result is assembled by `forecast_stats::assemble`.

use crate::forecast_apply::{apply_option, apply_option_json};
use crate::forecast_stats::{assemble, option_jokers, options};
use crate::{gs, hands, jokers};
use serde_json::{json, Map, Value};
use simcore::{play_horizon, Policy, Row};

/// rollout._seed(k)
pub fn seed(k: usize) -> String {
    format!("RO{k:06}")
}

/// sim_bridge.policy_jokers(Policy(joker_dicts))
pub fn policy_of(joker_dicts: &[Value]) -> Policy {
    let js = jokers::build(&json!({}), Some(joker_dicts));
    Policy {
        jokers: js.iter().map(|j| (hands::native::edition_code(j.edition.as_deref()), hands::native::rule(&j.rule))).collect(),
    }
}

pub fn row_json(r: &Row) -> Value {
    json!({"ante": r.0, "type": r.1, "boss": r.2, "target": r.3, "chips": r.4, "cleared": r.5})
}

/// Wall-clock `forecast`.
pub fn forecast(pub_: &Value, n: usize, bosses: i64, keys: Option<&[String]>, budget_s: Option<f64>) -> Value {
    let t0 = std::time::Instant::now();
    forecast_clock(pub_, n, bosses, keys, budget_s, &|| t0.elapsed().as_secs_f64())
}

struct Ent<'a> {
    key: String,
    opt: &'a Value,
    pol: Policy,
    base: Option<rumbot_sim::game::GameState>,
    error: bool,
}

/// `clock` returns seconds since the start of the forecast (time.time() - t0).
pub fn forecast_clock(
    pub_: &Value,
    n: usize,
    bosses: i64,
    keys: Option<&[String]>,
    budget_s: Option<f64>,
    clock: &dyn Fn() -> f64,
) -> Value {
    let all = crate::prof::time("f.options", || options(pub_));
    let opts: Vec<&Value> = all
        .iter()
        .filter(|o| {
            let key = gs::s(gs::get(o, "key"));
            gs::truthy(gs::get(o, "supported")) && keys.map_or(true, |ks| ks.iter().any(|k| k == key) || key == "skip")
        })
        .collect();
    let budget = budget_s.filter(|b| *b != 0.0);
    let bosses_u = bosses as usize;
    // Apply each option once at rollout 0 seed. No RNG drawn: convert once; else apply per rollout seed.
    let mut ents: Vec<Ent> = Vec::new();
    for o in &opts {
        let key = gs::s(gs::get(o, "key")).to_string();
        let pol = policy_of(&option_jokers(pub_, o));
        let s0 = seed(0);
        let (error, base) = match crate::prof::time("f.apply0", || apply_option_json(pub_, o, &s0)) {
            Err(_) => (true, None),
            Ok((_, Some(_))) => (false, None),
            Ok((_, None)) => match apply_option(pub_, o, &s0) {
                Ok(g) => (false, Some(g)),
                Err(_) => (true, None),
            },
        };
        ents.push(Ent { key, opt: o, pol, base, error });
    }
    let mut runs: Vec<Vec<Value>> = vec![Vec::new(); ents.len()];
    let mut k = 0usize;
    while k < n {
        let step_n = if budget.is_some() { 8.min(n - k) } else { n - k };
        for (ei, e) in ents.iter().enumerate() {
            if e.error {
                continue;
            }
            for i in k..k + step_n {
                let sd = seed(i);
                let mut g = match &e.base {
                    Some(b) => {
                        let mut g = crate::prof::time("f.deep_clone", || b.deep_clone());
                        g.rng = rumbot_sim::rng::RunRng::new(sd.clone());
                        g
                    }
                    None => crate::prof::time("f.apply_k", || apply_option(pub_, e.opt, &sd)).expect("option applied at seed 0"),
                };
                g.seed = sd;
                let rows = crate::prof::time("f.play_horizon", || play_horizon(&mut g, &e.pol, bosses_u, 600)).expect("rollout");
                runs[ei].push(Value::Array(rows.iter().map(row_json).collect()));
            }
        }
        k += step_n;
        if let Some(b) = budget {
            if clock() > b && k >= 8 {
                break;
            }
        }
    }
    let mut rmap = Map::new();
    for (e, r) in ents.iter().zip(runs) {
        rmap.insert(e.key.clone(), Value::Array(r));
    }
    let mut out = crate::prof::time("f.assemble", || assemble(&all, keys, &rmap, bosses, k as i64));
    let m = out.as_object_mut().unwrap();
    m.insert("seconds".into(), json!(pycompat::pyround(clock(), 2)));
    m.insert("_runs".into(), Value::Object(rmap));
    out
}
