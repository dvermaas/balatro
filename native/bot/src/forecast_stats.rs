//! The stats and result assembly of the forecasts, ported line by line from rollout.py and
//! rollout_items.py (the rollouts themselves stay in the Python/rumbot path):
//! rollout.py 295-357 (options, option_jokers), 589-642 (summarize, m_*, paired), 774-781
//! (paired_delta), 752-788 (_forecast result assembly, minus the rollouts);
//! rollout_items.py 56-70 (usable), 379-397 (options), 472-482 (best).
//! Option keys, descs, costs and action lists are built in Python insertion order (serde_json
//! preserve_order), and every float goes through pycompat so it prints and rounds as Python does.

use crate::gs;
use pycompat::statistics as st;
use serde_json::{json, Map, Value};

/// `_d(gs.get(area)).get("cards") or []`.
fn cards<'a>(p: &'a Value, area: &str) -> &'a [Value] {
    gs::cards_of(p, area)
}

/// `m.get(k, default)`: the default only when the key is missing (a present null stays null).
fn get_or(m: &Map<String, Value>, k: &str, dflt: Value) -> Value {
    m.get(k).cloned().unwrap_or(dflt)
}

/// `x or default`.
fn or_default(v: &Value, dflt: Value) -> Value {
    if gs::truthy(v) {
        v.clone()
    } else {
        dflt
    }
}

/// Python `str()` of a JSON value: None, True/False, ints, floats via pyfloat, strings as is.
pub fn pystr(v: &Value) -> String {
    match v {
        Value::Null => "None".to_string(),
        Value::Bool(b) => (if *b { "True" } else { "False" }).to_string(),
        Value::Number(n) => match n.as_i64() {
            Some(i) => i.to_string(),
            None => pycompat::pyfloat(n.as_f64().expect("finite number")),
        },
        Value::String(s) => s.clone(),
        other => other.to_string(),
    }
}

/// Python `a - b` on JSON numbers: int when both are ints, else float.
fn sub(a: &Value, b: &Value) -> Value {
    match (a.as_i64(), b.as_i64()) {
        (Some(x), Some(y)) => json!(x - y),
        _ => json!(gs::num(a) - gs::num(b)),
    }
}

/// rollout.py 295-329: candidate shop options from the public state. actions are engine-level:
/// ("buy", i) / ("voucher", i) / ("sell", j), kept as [kind, i] JSON arrays.
pub fn options(pub_: &Value) -> Vec<Value> {
    let money = or_default(gs::get(pub_, "money"), json!(0));
    let jok = gs::area(pub_, "jokers");
    let cons = gs::area(pub_, "consumables");
    let free_j = (cards(pub_, "jokers").len() as f64) < gs::num(&or_default(gs::mget(jok, "limit"), json!(5)));
    let free_c = (cards(pub_, "consumables").len() as f64) < gs::num(&or_default(gs::mget(cons, "limit"), json!(2)));
    let money_f = gs::num(&money);
    let mut out = vec![json!({"key": "skip", "desc": format!("buy nothing (keep ${})", pystr(&money)),
        "actions": [], "supported": true, "cost": 0})];
    for (i, c) in cards(pub_, "shop").iter().enumerate() {
        let cost = get_or(gs::d(gs::get(c, "cost")), "buy", json!(0));
        let neg = gs::get(gs::get(c, "modifier"), "edition").as_str() == Some("NEGATIVE");
        let st_v = gs::get(c, "set").clone();
        let st_s = st_v.as_str();
        if gs::num(&cost) > money_f
            || (st_s == Some("JOKER") && !free_j && !neg)
            || (matches!(st_s, Some("TAROT" | "PLANET" | "SPECTRAL")) && !free_c && !neg)
        {
            continue;
        }
        out.push(json!({"key": format!("buy_card_{i}"),
            "desc": format!("${} {}", pystr(&cost), pystr(gs::get(c, "label"))),
            "actions": [["buy", i]],
            "supported": matches!(st_s, Some("JOKER" | "PLANET" | "DEFAULT" | "ENHANCED")),
            "cost": cost, "set": st_v}));
    }
    for (i, c) in cards(pub_, "vouchers").iter().enumerate() {
        let cost = get_or(gs::d(gs::get(c, "cost")), "buy", json!(0));
        if gs::num(&cost) <= money_f && !crate::decisions::NEVER_VOUCHERS.contains(&gs::s(gs::get(c, "label"))) {
            out.push(json!({"key": format!("buy_voucher_{i}"),
                "desc": format!("${} {}", pystr(&cost), pystr(gs::get(c, "label"))),
                "actions": [["voucher", i]], "supported": true, "cost": cost, "set": "VOUCHER"}));
        }
    }
    if !free_j {
        for (j, owned) in cards(pub_, "jokers").iter().enumerate() {
            let m = gs::d(gs::get(owned, "modifier"));
            if gs::truthy(gs::mget(m, "eternal")) || gs::mget(m, "edition").as_str() == Some("NEGATIVE") {
                continue;
            }
            let sell = get_or(gs::d(gs::get(owned, "cost")), "sell", json!(0));
            for (i, c) in cards(pub_, "shop").iter().enumerate() {
                let cost = get_or(gs::d(gs::get(c, "cost")), "buy", json!(0));
                if gs::get(c, "set").as_str() == Some("JOKER") && gs::num(&cost) <= money_f + gs::num(&sell) {
                    out.push(json!({"key": format!("replace_{j}_{i}"),
                        "desc": format!("sell {} for {}", pystr(gs::get(owned, "label")), pystr(gs::get(c, "label"))),
                        "actions": [["sell", j], ["buy", i]], "supported": true,
                        "cost": sub(&cost, &sell), "set": "JOKER"}));
                }
            }
        }
    }
    out
}

/// rollout_items.py 379-396: skip, each affordable pack, each affordable shop tarot/spectral the
/// bot would use. No "supported" key (every entry is supported).
pub fn items_options(pub_: &Value) -> Vec<Value> {
    let money = or_default(gs::get(pub_, "money"), json!(0));
    let cons = gs::area(pub_, "consumables");
    let free_c = (cards(pub_, "consumables").len() as f64) < gs::num(&or_default(gs::mget(cons, "limit"), json!(2)));
    let money_f = gs::num(&money);
    let mut out = vec![json!({"key": "skip", "desc": format!("buy nothing (keep ${})", pystr(&money)),
        "actions": [], "cost": 0})];
    for (i, c) in cards(pub_, "shop").iter().enumerate() {
        let cost = get_or(gs::d(gs::get(c, "cost")), "buy", json!(0));
        let neg = gs::get(gs::get(c, "modifier"), "edition").as_str() == Some("NEGATIVE");
        let st_v = gs::get(c, "set").clone();
        if matches!(st_v.as_str(), Some("TAROT" | "SPECTRAL"))
            && gs::num(&cost) <= money_f
            && (free_c || neg)
            && usable(gs::get(c, "key").as_str())
        {
            out.push(json!({"key": format!("buy_card_{i}"),
                "desc": format!("${} {}", pystr(&cost), pystr(gs::get(c, "label"))),
                "actions": [["buy", i]], "cost": cost, "set": st_v}));
        }
    }
    for (i, c) in cards(pub_, "packs").iter().enumerate() {
        let cost = get_or(gs::d(gs::get(c, "cost")), "buy", json!(0));
        if gs::num(&cost) <= money_f {
            out.push(json!({"key": format!("buy_pack_{i}"),
                "desc": format!("${} {}", pystr(&cost), pystr(gs::get(c, "label"))),
                "actions": [["pack", i]], "cost": cost, "set": "BOOSTER"}));
        }
    }
    out
}

/// rollout.py 343-355: the public joker dicts the build has after the option (the policy input).
pub fn option_jokers(pub_: &Value, opt: &Value) -> Vec<Value> {
    let mut js: Vec<Value> = cards(pub_, "jokers").to_vec();
    for act in gs::list(gs::get(opt, "actions")) {
        let kind = gs::list(act).first().map_or("", gs::s);
        let i = gs::int(&gs::list(act)[1]) as usize;
        if kind == "sell" {
            js = js.into_iter().enumerate().filter(|(n, _)| *n != i).map(|(_, c)| c).collect();
        } else if kind == "buy" && gs::get(&cards(pub_, "shop")[i], "set").as_str() == Some("JOKER") {
            js.push(cards(pub_, "shop")[i].clone());
        }
    }
    js
}

/// rollout.py 623-625: 1.0 when the horizon's last blind was cleared and it held at least
/// `bosses` boss blinds.
pub fn m_survive(r: &[Value], bosses: i64) -> f64 {
    let last_cleared = r.last().map_or(false, |b| gs::truthy(gs::get(b, "cleared")));
    if last_cleared && boss_count(r) >= bosses {
        1.0
    } else {
        0.0
    }
}

fn boss_count(r: &[Value]) -> i64 {
    r.iter().filter(|b| gs::get(b, "type").as_str() == Some("Boss")).count() as i64
}

/// rollout.py 627-629: the first boss blind of the horizon was cleared.
pub fn m_next_boss(r: &[Value]) -> f64 {
    if r.iter().any(|b| gs::get(b, "type").as_str() == Some("Boss") && gs::truthy(gs::get(b, "cleared"))) {
        1.0
    } else {
        0.0
    }
}

/// rollout.py 632-633: blinds cleared.
pub fn m_blinds(r: &[Value]) -> f64 {
    let n = r.iter().filter(|b| gs::truthy(gs::get(b, "cleared"))).count() as f64;
    let e = crate::experiment::get();
    if e.value_tail > 0.0 || e.money_tail > 0.0 {
        // lab: + the weighted value of the run after the horizon (forecast::tail_value)
        return n + r.last().map_or(0.0, |b| gs::num(gs::get(b, "value")));
    }
    n
}

/// rollout.py 636-641: common-random-numbers mean of fn(a) - fn(b) and its paired standard error.
pub fn paired<F: Fn(&[Value]) -> f64>(runs_a: &[Vec<Value>], runs_b: &[Vec<Value>], f: F) -> (f64, f64) {
    let d: Vec<f64> = runs_a.iter().zip(runs_b).map(|(a, b)| f(a.as_slice()) - f(b.as_slice())).collect();
    if d.len() < 2 {
        return (0.0, 0.0);
    }
    (st::mean_f64(&d), st::stdev_f64(&d) / (d.len() as f64).sqrt())
}

/// rollout.py 774-781: the paired difference in P(survive) with its SE.
pub fn paired_delta(runs_a: &[Vec<Value>], runs_b: &[Vec<Value>], bosses: i64) -> (f64, f64) {
    paired(runs_a, runs_b, |r| m_survive(r, bosses))
}

/// `dict(Counter(x["boss"] for x in reached).most_common(4))`: ties keep first-seen order.
fn bosses_seen(reached: &[&Value]) -> Value {
    let mut counts: Vec<(String, i64)> = Vec::new();
    for x in reached {
        let key = match gs::get(x, "boss") {
            Value::String(s) => s.clone(),
            Value::Null => "null".to_string(),
            other => other.to_string(),
        };
        match counts.iter_mut().find(|e| e.0 == key) {
            Some(e) => e.1 += 1,
            None => counts.push((key, 1)),
        }
    }
    counts.sort_by(|a, b| b.1.cmp(&a.1));
    counts.truncate(4);
    Value::Object(counts.into_iter().map(|(k, c)| (k, json!(c))).collect())
}

/// rollout.py 589-620: aggregate one option's rollouts (non-empty) into the forecast dict.
pub fn summarize(runs: &[Vec<Value>], bosses: i64) -> Value {
    let n = runs.len();
    assert!(n > 0, "summarize: no runs (Python max() of an empty sequence)");
    let surv: Vec<f64> = runs.iter().map(|r| m_survive(r, bosses)).collect();
    let p = surv.iter().fold(0.0, |a, b| a + b) / n as f64;
    let v = p * (1.0 - p);
    let v = if 1e-9 > v { 1e-9 } else { v };
    let mut longest = &runs[0];
    for r in runs {
        if r.len() > longest.len() {
            longest = r;
        }
    }
    let cleared = |x: &Value| gs::truthy(gs::get(x, "cleared"));
    let mut blinds: Vec<Value> = Vec::new();
    for (k, b) in longest.iter().enumerate() {
        let reached: Vec<&Value> = runs.iter().filter(|r| r.len() > k).map(|r| &r[k]).collect();
        let n_cleared = reached.iter().filter(|x| cleared(x)).count();
        let label = format!("ante {} {}", pystr(gs::get(b, "ante")), pystr(gs::get(b, "type")).to_lowercase());
        let mut row = Map::new();
        row.insert("blind".into(), json!(label));
        row.insert("p_clear".into(), json!(pycompat::pyround(n_cleared as f64 / n as f64, 3)));
        row.insert(
            "p_clear_given_reached".into(),
            json!(pycompat::pyround(n_cleared as f64 / reached.len() as f64, 3)),
        );
        if gs::get(b, "type").as_str() == Some("Boss") {
            let mut margins: Vec<f64> = reached
                .iter()
                .filter(|x| gs::truthy(gs::get(x, "target")))
                .map(|x| gs::num(gs::get(x, "chips")) / gs::num(gs::get(x, "target")))
                .collect();
            margins.sort_by(|a, b| a.partial_cmp(b).unwrap());
            if !margins.is_empty() {
                row.insert("median_margin".into(), json!(pycompat::pyround(st::median_f64(&margins), 2)));
                row.insert("bosses_seen".into(), bosses_seen(&reached));
            }
        }
        blinds.push(Value::Object(row));
    }
    let blind_survival: Vec<Value> = blinds.iter().map(|r| r["p_clear"].clone()).collect();
    let total_cleared: usize = runs.iter().map(|r| r.iter().filter(|b| cleared(b)).count()).sum();
    let p_next_boss = runs.iter().map(|r| m_next_boss(r)).fold(0.0, |a, b| a + b) / n as f64;
    let boss = blinds.iter().find(|b| b["blind"].as_str().map_or(false, |s| s.ends_with("boss")));
    let mut out = Map::new();
    out.insert("n".into(), json!(n));
    out.insert("p_survive".into(), json!(pycompat::pyround(p, 3)));
    out.insert("se_survive".into(), json!(pycompat::pyround((v / n as f64).sqrt(), 3)));
    out.insert("blinds".into(), json!(blinds));
    out.insert("blind_survival".into(), json!(blind_survival));
    out.insert("exp_blinds_cleared".into(), json!(pycompat::pyround(total_cleared as f64 / n as f64, 2)));
    out.insert("horizon_blinds".into(), json!(longest.len()));
    out.insert("p_next_boss".into(), json!(pycompat::pyround(p_next_boss, 3)));
    if let Some(b) = boss {
        let row = b.as_object().expect("blind row");
        out.insert("next_boss".into(), b["blind"].clone());
        out.insert("next_boss_margin".into(), get_or(row, "median_margin", Value::Null));
        out.insert("next_boss_names".into(), get_or(row, "bosses_seen", Value::Null));
    }
    Value::Object(out)
}

/// A recorded `_runs` value (per option: a list of rollouts, each a list of blind dicts).
pub fn runs_list(v: &Value) -> Vec<Vec<Value>> {
    gs::list(v).iter().map(|r| gs::list(r).to_vec()).collect()
}

/// rollout.py 752-788 (`_forecast`) minus the rollouts: given the options(pub) list, the requested
/// keys (None = all), the per-option rollout rows, bosses and the rollout count, build
/// {options, unsupported, rollouts, bosses}. The caller adds "seconds" and "_runs".
pub fn assemble(
    pub_opts: &[Value],
    keys: Option<&[String]>,
    runs: &Map<String, Value>,
    bosses: i64,
    rollouts: i64,
) -> Value {
    let runs_of = |k: &str| runs_list(gs::mget(runs, k));
    let opts: Vec<&Value> = pub_opts
        .iter()
        .filter(|o| {
            let key = gs::s(gs::get(o, "key"));
            gs::truthy(gs::get(o, "supported"))
                && keys.map_or(true, |ks| ks.iter().any(|k| k.as_str() == key) || key == "skip")
        })
        .collect();
    let mut res = Map::new();
    for o in &opts {
        let key = gs::s(gs::get(o, "key"));
        let rs = runs_of(key);
        if rs.is_empty() {
            continue;
        }
        let mut row = Map::new();
        row.insert("desc".into(), gs::get(o, "desc").clone());
        row.insert("cost".into(), gs::get(o, "cost").clone());
        if let Value::Object(s) = summarize(&rs, bosses) {
            for (k, v) in s {
                row.insert(k, v);
            }
        }
        res.insert(key.to_string(), Value::Object(row));
    }
    if res.contains_key("skip") {
        let base = runs_of("skip");
        let res_keys: Vec<String> = res.keys().cloned().collect();
        for key in res_keys {
            let r = runs_of(&key);
            let pair = |(m, s): (f64, f64)| json!([pycompat::pyround(m, 3), pycompat::pyround(s, 3)]);
            let vs = json!({
                "next_boss": pair(paired(&r, &base, m_next_boss)),
                "survive": pair(paired(&r, &base, |x| m_survive(x, bosses))),
                "blinds": pair(paired(&r, &base, m_blinds)),
            });
            if let Some(Value::Object(row)) = res.get_mut(&key) {
                row.insert("vs_skip".into(), vs);
            }
        }
    }
    let skipped: Vec<Value> = pub_opts
        .iter()
        .filter(|o| !gs::truthy(gs::get(o, "supported")))
        .map(|o| gs::get(o, "key").clone())
        .collect();
    let mut out = Map::new();
    out.insert("options".into(), Value::Object(res));
    out.insert("unsupported".into(), json!(skipped));
    out.insert("rollouts".into(), json!(rollouts));
    out.insert("bosses".into(), json!(bosses));
    Value::Object(out)
}

/// rollout_items.py 64-70: consumables the bot ever uses. None (a missing key) is False.
pub fn usable(key: Option<&str>) -> bool {
    key.map_or(false, |k| crate::tarots::targeted(k).is_some() || crate::tarots::safe_untargeted(k))
}

/// rollout_items.py 472-482: the option that beats skip by more than `se` paired SEs in blinds
/// cleared (largest gain; the first of equal gains), or None.
pub fn best(fc: &Value, se: f64) -> Option<String> {
    let mut best: Option<(String, f64)> = None;
    for (k, r) in gs::d(gs::get(fc, "options")) {
        if k.as_str() == "skip" || gs::get(r, "vs_skip").is_null() {
            continue;
        }
        let pair = gs::list(gs::get(gs::get(r, "vs_skip"), "blinds"));
        let d = gs::num(&pair[0]);
        let s = gs::num(&pair[1]);
        let sm = if 1e-9 > s { 1e-9 } else { s };
        if d > se * sm {
            let better = match &best {
                Some((_, bd)) => d > *bd,
                None => true,
            };
            if better {
                best = Some((k.clone(), d));
            }
        }
    }
    best.map(|(k, _)| k)
}
