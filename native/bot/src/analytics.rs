//! Port of src/analytics.py: metrics derived from a run's decisions.jsonl, and the per-version and
//! per-benchmark summaries built from summary.json. Key order, int/float types and round() match
//! the Python dicts (serde_json preserve_order; pycompat::pyround). Python's sum() over floats is
//! compensated (CPython 3.12+), so py_sum reproduces it. The mtime cache is not ported (speed only).

use crate::benchmark;
use pycompat::pyround;
use serde_json::{json, Map, Value};
use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};

fn get<'a>(v: &'a Value, k: &str) -> &'a Value {
    v.get(k).unwrap_or(&Value::Null)
}

fn s<'a>(v: &'a Value, k: &str) -> &'a str {
    v.get(k).and_then(Value::as_str).unwrap_or("")
}

/// Python truthiness.
fn truthy(v: &Value) -> bool {
    match v {
        Value::Null => false,
        Value::Bool(b) => *b,
        Value::Number(n) => n.as_f64().map_or(true, |f| f != 0.0),
        Value::String(x) => !x.is_empty(),
        Value::Array(a) => !a.is_empty(),
        Value::Object(o) => !o.is_empty(),
    }
}

/// Python `a or b`.
fn or(v: &Value, d: Value) -> Value {
    if truthy(v) {
        v.clone()
    } else {
        d
    }
}

/// CPython 3.12+ sum() over floats: Neumaier-compensated.
pub fn py_sum(xs: &[f64]) -> f64 {
    let (mut f, mut c) = (0.0f64, 0.0f64);
    for &x in xs {
        let t = f + x;
        if f.abs() >= x.abs() {
            c += (f - t) + x;
        } else {
            c += (x - t) + f;
        }
        f = t;
    }
    if c != 0.0 && c.is_finite() {
        f += c;
    }
    f
}

/// Python `_avg`: mean of the non-None values, rounded to 2; None when there are none.
fn avg(xs: &[Value]) -> Value {
    let v: Vec<f64> = xs.iter().filter_map(Value::as_f64).collect();
    if v.is_empty() {
        return Value::Null;
    }
    json!(pyround(py_sum(&v) / v.len() as f64, 2))
}

/// Python `_se`: standard error of the mean, rounded to 2; None for fewer than two values.
fn se(xs: &[Value]) -> Value {
    let v: Vec<f64> = xs.iter().filter_map(Value::as_f64).collect();
    if v.len() < 2 {
        return Value::Null;
    }
    let n = v.len() as f64;
    let mean = py_sum(&v) / n;
    let sq: Vec<f64> = v.iter().map(|x| (x - mean).powf(2.0)).collect();
    let var = py_sum(&sq) / (v.len() - 1) as f64;
    json!(pyround(var.powf(0.5) / n.powf(0.5), 2))
}

fn field(xs: &[&Value], k: &str) -> Vec<Value> {
    xs.iter().map(|r| get(r, k).clone()).collect()
}

fn mfield(xs: &[&Value], k: &str) -> Vec<Value> {
    xs.iter().map(|r| get(get(r, "metrics"), k).clone()).collect()
}

/// `{**base, **extra}` with Python's key semantics (an existing key keeps its position).
fn merge(base: &Value, extra: Vec<(&str, Value)>) -> Map<String, Value> {
    let mut m = base.as_object().cloned().unwrap_or_default();
    for (k, v) in extra {
        m.insert(k.to_string(), v);
    }
    m
}

pub fn load_decisions(run_dir: &Path) -> Vec<Value> {
    let path = run_dir.join("decisions.jsonl");
    if !path.exists() {
        return vec![];
    }
    let text = fs::read_to_string(&path).expect("read decisions.jsonl");
    text.lines()
        .filter(|l| !l.trim().is_empty())
        .map(|l| serde_json::from_str(l).expect("decisions.jsonl line"))
        .collect()
}

/// Python `run_metrics` (without the mtime cache).
pub fn run_metrics(run_dir: &Path) -> Value {
    let (mut plays, mut discards, mut pack_picks, mut pack_takes) = (0i64, 0i64, 0i64, 0i64);
    let (mut packs_bought, mut cards_bought, mut jokers_at_end) = (0i64, 0i64, 0i64);
    let mut conf: Vec<(String, Vec<f64>)> = Vec::new();
    let mut last_move: Option<Value> = None;
    let mut final_chips = Value::Null;
    let mut rounds_won = 0i64;
    for e in load_decisions(run_dir) {
        let kind = s(&e, "kind");
        if kind == "round_end" {
            rounds_won += 1;
        }
        if kind == "outcome" && s(&e, "method") == "play" {
            final_chips = get(&e, "chips").clone();
        }
        if kind == "forced" && s(&e, "question") == "move" {  // v6 plays a sure clear without asking
            if s(&e, "choice").starts_with("play") {
                plays += 1
            } else {
                discards += 1
            }
        }
        if kind != "decision" {
            continue;
        }
        let (q, c) = (s(&e, "question"), s(&e, "choice"));
        let cv = or(get(&e, "confidence"), json!(0)).as_f64().unwrap_or(0.0);
        match conf.iter_mut().find(|x| x.0 == q) {
            Some(x) => x.1.push(cv),
            None => conf.push((q.to_string(), vec![cv])),
        }
        let sent = or(get(&e, "state_sent"), json!({}));
        let game = get(&sent, "game");
        let g = if truthy(game) { game.clone() } else { sent.clone() };  // v1-v5 nest under "game"
        jokers_at_end = match or(get(&g, "jokers"), json!([])) {
            Value::Array(a) => a.len() as i64,
            Value::Object(o) => o.len() as i64,
            Value::String(x) => x.chars().count() as i64,
            _ => 0,
        };
        if q == "move" {
            let round_obj = get(&g, "round");
            let fallback = if round_obj.is_object() { get(round_obj, "discards_left").clone() } else { Value::Null };
            let dl = g.get("discards_left").cloned().unwrap_or(fallback);
            last_move = Some(json!({ "discards_left": dl }));
            if c.starts_with("play") {
                plays += 1
            } else {
                discards += 1
            }
        } else if q == "pack" {
            pack_picks += 1;
            pack_takes += c.starts_with("take") as i64;
        } else if q == "shop" {
            packs_bought += c.starts_with("buy_pack") as i64;
            cards_bought += c.starts_with("buy_card") as i64;
        }
    }
    let mut m = Map::new();
    m.insert("plays".into(), json!(plays));
    m.insert("discards".into(), json!(discards));
    m.insert("pack_picks".into(), json!(pack_picks));
    m.insert("pack_takes".into(), json!(pack_takes));
    m.insert("packs_bought".into(), json!(packs_bought));
    m.insert("cards_bought".into(), json!(cards_bought));
    m.insert("wasted_discards".into(), Value::Null);
    m.insert("jokers_at_end".into(), json!(jokers_at_end));
    if let Some(lm) = &last_move {
        m.insert("wasted_discards".into(), get(lm, "discards_left").clone());
        // Mirrors the Python exactly: last_move only holds discards_left, so target is always 0 here.
        let target = or(get(&or(get(lm, "blind"), json!({})), "chip_target"), json!(0));
        let scored = if !final_chips.is_null() {
            final_chips.clone()
        } else {
            or(get(lm, "chips_scored_this_round"), json!(0))
        };
        let cleared = std::cmp::max(rounds_won, or(get(lm, "round"), json!(1)).as_i64().unwrap_or(1) - 1);
        let progress = if truthy(&target) {
            let x = scored.as_f64().unwrap_or(0.0) / target.as_f64().unwrap_or(0.0);
            let frac = if x < 1.0 { x } else { 1.0 };  // Python min(1.0, x)
            json!(pyround(cleared as f64 + frac, 2))
        } else {
            json!(cleared)  // int + 0 stays int; round(int, 2) is int
        };
        m.insert("progress".into(), progress);
    }
    let mut confidence = Map::new();
    for (q, v) in &conf {
        confidence.insert(q.clone(), json!(pyround(py_sum(v) / v.len() as f64, 3)));
    }
    m.insert("confidence".into(), Value::Object(confidence));
    Value::Object(m)
}

/// Python `run_backend`: "balatrobot" (real game, and runs from before the field existed) or "jackdaw".
pub fn run_backend(r: &Value) -> String {
    match r.get("backend") {
        Some(Value::String(b)) if !b.is_empty() => b.clone(),
        _ => "balatrobot".to_string(),
    }
}

/// Python `all_runs`: every summary.json one level under root, newest dir name first.
pub fn all_runs(root: &Path) -> Vec<Value> {
    let mut dirs: Vec<PathBuf> = match fs::read_dir(root) {
        Ok(rd) => rd.filter_map(|e| e.ok().map(|e| e.path())).filter(|p| p.join("summary.json").is_file()).collect(),
        Err(_) => return vec![],
    };
    // Windows paths compare case-insensitively.
    dirs.sort_by_key(|p| p.file_name().map(|n| n.to_string_lossy().to_lowercase()).unwrap_or_default());
    dirs.reverse();
    let mut out = Vec::new();
    for d in dirs {
        let Ok(text) = fs::read_to_string(d.join("summary.json")) else { continue };
        let Ok(sum) = serde_json::from_str::<Value>(&text) else { continue };
        let metrics = run_metrics(&d);
        out.push(Value::Object(merge(&sum, vec![("metrics", metrics)])));
    }
    out
}

/// Python `prompt_versions`: the snapshots in runs/_prompts/index.json order, labelled v1, v2, ...
/// `current` is the bot's PROMPT_VERSION (recorder.PROMPT_VERSION in Python).
pub fn prompt_versions(prompts_dir: &Path, current: &str) -> Vec<Value> {
    let index_path = prompts_dir.join("index.json");
    if !index_path.exists() {
        return vec![];
    }
    let idx: Value = serde_json::from_str(&fs::read_to_string(&index_path).expect("index.json")).expect("index json");
    let order = get(&idx, "order").as_array().cloned().unwrap_or_default();
    order
        .iter()
        .enumerate()
        .map(|(n, v)| {
            let id = v.as_str().expect("version id");
            let text = fs::read_to_string(prompts_dir.join(format!("{id}.json"))).expect("snapshot");
            let snap: Value = serde_json::from_str(&text).expect("snapshot json");
            Value::Object(merge(
                &snap,
                vec![("label", json!(format!("v{}", n + 1))), ("current", json!(id == current))],
            ))
        })
        .collect()
}

/// Python `version_stats`.
pub fn version_stats(backend: &str, root: &Path, prompts_dir: &Path, current: &str) -> Vec<Value> {
    let runs: Vec<Value> = all_runs(root).into_iter().filter(|r| run_backend(r) == backend).collect();
    let mut out = Vec::new();
    for v in prompt_versions(prompts_dir, current) {
        let vid = get(&v, "version");
        let rs: Vec<&Value> = runs.iter().filter(|r| get(r, "prompt_version") == vid).collect();
        let done: Vec<&Value> = rs.iter().copied().filter(|r| matches!(s(r, "result"), "won" | "lost")).collect();
        let lost: Vec<&Value> = done.iter().copied().filter(|r| s(r, "result") == "lost").collect();
        let ms: Vec<&Value> = rs.iter().map(|r| get(r, "metrics")).collect();
        let qs: BTreeSet<String> = ms
            .iter()
            .flat_map(|m| m["confidence"].as_object().into_iter().flat_map(|o| o.keys().cloned()))
            .collect();
        let picks: i64 = ms.iter().map(|m| get(m, "pack_picks").as_i64().unwrap_or(0)).sum();
        let takes: i64 = ms.iter().map(|m| get(m, "pack_takes").as_i64().unwrap_or(0)).sum();
        // max((ante or 0 for r in rs), default=None): first maximal element.
        let mut best = Value::Null;
        for r in &rs {
            let a = or(get(r, "ante"), json!(0));
            if best.is_null() || a.as_f64().unwrap_or(0.0) > best.as_f64().unwrap_or(0.0) {
                best = a;
            }
        }
        let mut by_q = Map::new();
        for q in &qs {
            let xs: Vec<Value> = ms.iter().map(|m| get(&m["confidence"], q).clone()).collect();
            by_q.insert(q.clone(), avg(&xs));
        }
        let discard_share: Vec<Value> = ms
            .iter()
            .filter(|m| get(m, "plays").as_i64().unwrap_or(0) + get(m, "discards").as_i64().unwrap_or(0) != 0)
            .map(|m| {
                let d = get(m, "discards").as_f64().unwrap_or(0.0);
                json!(d / (get(m, "plays").as_f64().unwrap_or(0.0) + d))
            })
            .collect();
        let tokens: Vec<Value> = rs
            .iter()
            .map(|r| json!(get(r, "input_tokens").as_f64().unwrap_or(0.0) + get(r, "output_tokens").as_f64().unwrap_or(0.0)))
            .collect();
        let pack_take_rate = if picks != 0 { json!(pyround(takes as f64 / picks as f64, 2)) } else { Value::Null };
        out.push(Value::Object(merge(
            &v,
            vec![
                ("runs", json!(rs.len())),
                ("finished", json!(done.len())),
                ("wins", json!(done.iter().filter(|r| s(r, "result") == "won").count())),
                ("avg_ante", avg(&field(&done, "ante"))),
                ("best_ante", if rs.is_empty() { Value::Null } else { best }),
                ("avg_round", avg(&field(&done, "round"))),
                ("avg_decisions", avg(&field(&rs, "decisions"))),
                ("avg_tokens", avg(&tokens)),
                ("avg_confidence", avg(&field(&rs, "avg_confidence"))),
                ("confidence_by_question", Value::Object(by_q)),
                ("pack_take_rate", pack_take_rate),
                ("discard_share", avg(&discard_share)),
                ("avg_wasted_discards_on_loss", avg(&mfield(&lost, "wasted_discards"))),
                ("avg_jokers_at_end", avg(&field(&ms, "jokers_at_end"))),
            ],
        )));
    }
    out
}

/// Python `benchmark_matrix`: seed x version grid for one benchmark set, plus a row per version.
pub fn benchmark_matrix(name: Option<&str>, backend: &str, root: &Path, prompts_dir: &Path, current: &str) -> Value {
    let name = match name {
        Some(n) if benchmark::set(n).is_some() => n,
        _ => benchmark::NAME,
    };
    let seeds = benchmark::set(name).expect("known set");
    let mut runs: Vec<Value> = all_runs(root)
        .into_iter()
        .filter(|r| {
            get(&or(get(r, "benchmark"), json!({})), "name").as_str() == Some(name)
                && matches!(s(r, "result"), "won" | "lost")
                && run_backend(r) == backend
        })
        .collect();
    runs.sort_by(|a, b| {  // stable, like Python's sorted
        get(a, "started").as_f64().unwrap_or(0.0).partial_cmp(&get(b, "started").as_f64().unwrap_or(0.0)).unwrap()
    });
    let mut versions = Vec::new();
    for v in prompt_versions(prompts_dir, current) {
        // Latest attempt per seed wins; cells keep first-insertion order like the Python dict.
        let mut rs: Vec<(String, Value)> = Vec::new();
        for r in runs.iter().filter(|r| get(r, "prompt_version") == get(&v, "version")) {
            let b = or(get(r, "benchmark"), json!({}));
            let seed = s(r, "seed").to_string();
            let key = match get(&b, "index").as_i64() {
                Some(i) if i >= 0 && (i as usize) < seeds.len() && seeds[i as usize].split('#').next() == Some(seed.as_str()) => {
                    seeds[i as usize].to_string()
                }
                _ => seed.clone(),
            };
            match rs.iter_mut().find(|e| e.0 == key) {
                Some(e) => e.1 = r.clone(),
                None => rs.push((key, r.clone())),
            }
        }
        if rs.is_empty() && !truthy(get(&v, "current")) {
            continue;
        }
        let done: Vec<&Value> = rs.iter().map(|(_, r)| r).collect();
        let mut antes: Vec<Value> = done.iter().map(|r| or(get(r, "ante"), json!(0))).collect();
        antes.sort_by(|a, b| a.as_f64().unwrap_or(0.0).partial_cmp(&b.as_f64().unwrap_or(0.0)).unwrap());
        let mut cells = Map::new();
        for (k, r) in &rs {
            cells.insert(
                k.clone(),
                json!({
                    "result": get(r, "result"), "ante": get(r, "ante"), "round": get(r, "round"),
                    "run_id": get(r, "run_id"), "progress": get(get(r, "metrics"), "progress"),
                }),
            );
        }
        let secs: Vec<Value> = done
            .iter()
            .filter(|r| truthy(get(r, "ended")))
            .map(|r| json!(get(r, "ended").as_f64().unwrap_or(0.0) - get(r, "started").as_f64().unwrap_or(0.0)))
            .collect();
        let tokens: Vec<Value> = done
            .iter()
            .map(|r| json!(get(r, "input_tokens").as_f64().unwrap_or(0.0) + get(r, "output_tokens").as_f64().unwrap_or(0.0)))
            .collect();
        versions.push(Value::Object(merge(
            &Value::Null,
            vec![
                ("version", get(&v, "version").clone()),
                ("label", get(&v, "label").clone()),
                ("current", get(&v, "current").clone()),
                ("note", get(&v, "note").clone()),
                ("done", json!(done.len())),
                ("of", json!(seeds.len())),
                ("ante_se", se(&field(&done, "ante"))),
                ("avg_progress", avg(&mfield(&done, "progress"))),
                ("progress_se", se(&mfield(&done, "progress"))),
                ("wins", json!(done.iter().filter(|r| s(r, "result") == "won").count())),
                ("avg_ante", avg(&field(&done, "ante"))),
                ("median_ante", if antes.is_empty() { Value::Null } else { antes[antes.len() / 2].clone() }),
                ("avg_round", avg(&field(&done, "round"))),
                ("avg_confidence", avg(&field(&done, "avg_confidence"))),
                ("avg_tokens", avg(&tokens)),
                ("avg_seconds", avg(&secs)),
                ("cells", Value::Object(cells)),
            ],
        )));
    }
    json!({
        "name": name,
        "backend": backend,
        "sets": benchmark::SETS.iter().map(|(n, _)| *n).collect::<Vec<_>>(),
        "default": benchmark::NAME,
        "seeds": seeds,
        "versions": versions,
    })
}
