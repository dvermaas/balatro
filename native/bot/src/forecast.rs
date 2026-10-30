//! Port of src/rollout.py forecast/_forecast/run_options/_run_options_native (lines 585-781): the
//! shop forecast end to end. Options are applied by `forecast_apply` (RunRng set first), the
//! rollouts run on simcore, the result is assembled by `forecast_stats::assemble`.

use crate::forecast_apply::{apply_option_json_on, game_of};
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

/// Lab (`JEVLAB_EXP=value-tail=W`): the learned tail value of the rollout's end, on its last row.
/// `JEVLAB_EXP=money-tail=W`: + W * blinds per dollar at the end ante * money, for a rollout still alive.
/// The weighted sum goes on the last row as "value" (forecast_stats::m_blinds adds it).
pub fn tail_value(rows: &mut [Value], g: &rumbot_sim::game::GameState) {
    let e = crate::experiment::get();
    if e.value_tail <= 0.0 && e.money_tail <= 0.0 {
        return;
    }
    if let Some(last) = rows.last_mut() {
        let cleared = last["cleared"] == json!(true);
        let mut v = 0.0;
        if e.value_tail > 0.0 {
            v += e.value_tail * crate::value_model::tail(g, cleared);
        }
        let alive = cleared && g.phase != rumbot_sim::game::Phase::Won && g.phase != rumbot_sim::game::Phase::GameOver;
        if e.money_tail > 0.0 && alive {
            let a = (g.ante.clamp(0, 9)) as usize;
            v += e.money_tail * crate::experiment::BLINDS_PER_DOLLAR[a] * (g.money.max(0) as f64);
        }
        last["value"] = json!(v);
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
    order: Option<Vec<usize>>,
    ojs: Vec<Value>,
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
    // The state description of pub_ is built once and copied for each option.
    let desc0 = crate::forecast_state::state_json_from_pub(pub_);
    let apply_json = |o: &Value, sd: &str| match &desc0 {
        Ok(d) => apply_option_json_on(d.clone(), pub_, o, sd),
        Err(e) => Err(e.clone()),
    };
    let mut ents: Vec<Ent> = Vec::new();
    let fc_order = crate::experiment::get().fc_order;
    for o in &opts {
        let key = gs::s(gs::get(o, "key")).to_string();
        let ojs = option_jokers(pub_, o);
        // lab (`fc-order`): the rollouts hold the jokers in the order the bot would put them in (v57's search)
        let mut order = if fc_order { joker_order_after(pub_, &ojs) } else { None };
        if crate::experiment::get().dagger_last {
            // v65: the rollouts keep Ceremonial Dagger last too, as the bot's shop does
            let labels: Vec<&str> = ojs.iter().map(|c| gs::get(c, "label").as_str().unwrap_or("")).collect();
            let o = crate::strategy::dagger_last(&labels, order.clone().unwrap_or_else(|| (0..ojs.len()).collect()));
            order = if o == (0..ojs.len()).collect::<Vec<usize>>() { None } else { Some(o) };
        }
        let pol = policy_of(&match &order {
            Some(ord) => ord.iter().map(|&i| ojs[i].clone()).collect(),
            None => ojs.clone(),
        });
        let s0 = seed(0);
        let (error, base) = match crate::prof::time("f.apply0", || apply_json(o, &s0)) {
            Err(_) => (true, None),
            Ok((_, Some(_))) => (false, None),
            Ok((d, None)) => match game_of(d, None, &s0) {
                Ok(mut g) => {
                    reorder(&mut g, order.as_deref(), &ojs);
                    (false, Some(g))
                }
                Err(_) => (true, None),
            },
        };
        ents.push(Ent { key, opt: o, pol, base, error, order, ojs });
    }
    // lab (`power-bonus=W`): the option's own scoring power, before any rollout, vs the skip option's
    let pb = crate::experiment::get().power_bonus;
    let late = gs::num(gs::get(pub_, "ante_num")) as i64 >= crate::experiment::get().power_bonus_ante;
    let bonus: Vec<f64> = if pb > 0.0 && late {
        let p75 = |e: &Ent| -> Option<f64> {
            let g = match &e.base {
                Some(b) => b.deep_clone(),
                None => apply_json(e.opt, &seed(0)).and_then(|(d, pl)| game_of(d, pl, &seed(0))).ok()?,
            };
            let p = crate::value_model::power_log(&g, 32);
            Some(p[3 * p.len() / 4])
        };
        let pw: Vec<Option<f64>> = ents.iter().map(|e| if e.error { None } else { p75(e) }).collect();
        let reference = ents.iter().zip(&pw).find(|(e, _)| e.key == "skip").and_then(|(_, p)| *p)
            .or_else(|| pw.iter().flatten().cloned().reduce(f64::min)).unwrap_or(0.0);
        pw.iter().map(|p| p.map_or(0.0, |p| pb * (p - reference))).collect()
    } else {
        vec![0.0; ents.len()]
    };
    let mut runs: Vec<Vec<Value>> = vec![Vec::new(); ents.len()];
    let shop_policy = crate::experiment::get().rollout_shop_policy;
    let pols: std::cell::RefCell<std::collections::HashMap<Vec<String>, Policy>> = Default::default();
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
                    None => {
                        let mut g = crate::prof::time("f.apply_k", || apply_json(e.opt, &sd).and_then(|(d, pl)| game_of(d, pl, &sd)))
                            .expect("option applied at seed 0");
                        reorder(&mut g, e.order.as_deref(), &e.ojs);
                        g
                    }
                };
                g.seed = sd;
                if i == 0 && ei == 0 && std::env::var_os("VALUE_CHECK").is_some() {
                    crate::value_model::check(pub_, &g);
                }
                let rows = if shop_policy {
                    // v60: rebuild the play policy when a rollout buys a joker (cached per lineup)
                    let mut rebuild = |gm: &rumbot_sim::game::GameState| -> Policy {
                        let sig: Vec<String> = gm.jokers.iter().map(|j| format!("{}|{:?}", j.borrow().spec.name, j.borrow().edition)).collect();
                        if let Some(p) = pols.borrow().get(&sig) {
                            return p.clone();
                        }
                        let p = crate::forecast_items::policy_of_game(gm.deep_clone()).1;
                        pols.borrow_mut().insert(sig, p.clone());
                        p
                    };
                    crate::prof::time("f.play_horizon", || simcore::play_horizon_rebuild(&mut g, &e.pol, bosses_u, 600, None, None, Some(&mut rebuild))).expect("rollout")
                } else {
                    crate::prof::time("f.play_horizon", || play_horizon(&mut g, &e.pol, bosses_u, 600)).expect("rollout")
                };
                let mut rj: Vec<Value> = rows.iter().map(row_json).collect();
                tail_value(&mut rj, &g);
                if bonus[ei] != 0.0 {
                    if let Some(last) = rj.last_mut() {
                        let v = last.get("value").and_then(|v| v.as_f64()).unwrap_or(0.0);
                        last["value"] = json!(v + bonus[ei]);
                    }
                }
                runs[ei].push(Value::Array(rj));
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

/// Lab (`fc-order`): v57's joker order (readiness::best_joker_order) for the jokers an option leaves, as
/// indices into `ojs`; None to keep them as they are.
fn joker_order_after(pub_: &Value, ojs: &[Value]) -> Option<Vec<usize>> {
    let mut g = pub_.clone();
    let area = g.get_mut("jokers")?.as_object_mut()?;
    area.insert("cards".into(), Value::Array(ojs.to_vec()));
    crate::readiness::best_joker_order(&g, 12)
}

/// Put the rollout game's jokers in `order` (indices into `ojs`), when the game holds exactly `ojs` by name
/// (case aside: rumbot's "Riff-Raff" is the game's "Riff-raff").
fn reorder(g: &mut rumbot_sim::game::GameState, order: Option<&[usize]>, ojs: &[Value]) {
    let Some(order) = order else { return };
    let names: Vec<String> = g.jokers.iter().map(|j| j.borrow().spec.name.to_string()).collect();
    let want: Vec<&str> = ojs.iter().map(|c| gs::get(c, "label").as_str().unwrap_or("")).collect();
    if names.len() != want.len() || names.iter().zip(&want).any(|(a, b)| !a.eq_ignore_ascii_case(b)) {
        if std::env::var_os("ORDER_DEBUG").is_some() {
            eprintln!("FC-ORDER mismatch: {names:?} vs {want:?}");
        }
        return;
    }
    g.jokers = order.iter().map(|&i| g.jokers[i].clone()).collect();
}

/// Lab (`skip-forecast`): skip this Small/Big blind for its tag, or play it? From a BLIND_SELECT state: the
/// rollouts start at this blind select with the game's real tags, one branch plays the blind, the other skips it
/// (the skipped blind counted as passed), both over the same seeds; returns (skip - play, paired SE) in blinds
/// (forecast_stats::m_blinds, value tail included). Shopping rollouts (v60) let a tag's free joker count.
pub fn skip_forecast(pub_: &Value, n: usize, bosses: i64) -> Option<(f64, f64)> {
    use rumbot_sim::game::{Action, ActionType};
    let desc = crate::forecast_state::state_json_from_pub(pub_).ok()?;
    let all = options(pub_);
    let noop = all.iter().find(|o| gs::s(gs::get(o, "key")) == "skip")?;
    let pol = policy_of(&option_jokers(pub_, noop));
    let (d, pl) = apply_option_json_on(desc, pub_, noop, &seed(0)).ok()?;
    let base = game_of(d, pl, &seed(0)).ok()?;
    let blinds = gs::get(pub_, "blinds");
    let tag_of = |k: &str| -> (String, String) {
        let name = gs::s(gs::get(gs::get(blinds, k), "tag_name")).to_string();
        let key = rumbot_sim::tag_data::TAG_DATA.iter().find(|t| t.name == name).map_or(String::new(), |t| t.key.to_string());
        (name, key)
    };
    let (small, big) = (tag_of("small"), tag_of("big"));
    let pols: std::cell::RefCell<std::collections::HashMap<Vec<String>, Policy>> = Default::default();
    let mut rebuild = |gm: &rumbot_sim::game::GameState| -> Policy {
        let sig: Vec<String> = gm.jokers.iter().map(|j| format!("{}|{:?}", j.borrow().spec.name, j.borrow().edition)).collect();
        if let Some(p) = pols.borrow().get(&sig) {
            return p.clone();
        }
        let p = crate::forecast_items::policy_of_game(gm.deep_clone()).1;
        pols.borrow_mut().insert(sig, p.clone());
        p
    };
    let (mut play, mut skip): (Vec<Vec<Value>>, Vec<Vec<Value>>) = (Vec::new(), Vec::new());
    for i in 0..n {
        for skipping in [false, true] {
            let sd = seed(i);
            let mut g = base.deep_clone();
            g.rng = rumbot_sim::rng::RunRng::new(sd.clone());
            g.rng.seeded("skip_forecast"); // a tag that opens a pack draws math.random before anything else seeds it
            g.seed = sd;
            g.step(&Action::new(ActionType::LeaveShop));
            g.ante_tags = vec![small.0.clone(), big.0.clone()];
            g.ante_tag_keys = vec![small.1.clone(), big.1.clone()];
            let mut lead: Vec<Value> = Vec::new();
            if skipping {
                let a = Action::new(ActionType::SkipBlind);
                if !g.is_legal(&a) {
                    return None;
                }
                let typ = if g.blind_index == 0 { "Small" } else { "Big" };
                lead.push(json!({"ante": g.ante, "type": typ, "boss": Value::Null, "target": 0, "chips": 0, "cleared": true}));
                g.step(&a);
            }
            let rows = simcore::play_horizon_rebuild(&mut g, &pol, bosses as usize, 600, None, None, Some(&mut rebuild)).ok()?;
            let mut rj: Vec<Value> = lead;
            rj.extend(rows.iter().map(row_json));
            tail_value(&mut rj, &g);
            if skipping { skip.push(rj) } else { play.push(rj) }
        }
    }
    Some(crate::forecast_stats::paired(&skip, &play, crate::forecast_stats::m_blinds))
}
