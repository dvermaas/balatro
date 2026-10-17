//! Rust `scripts/policy_lab.py` for arms E (`bot-forecast-long`) and F (`bot-forecast-items`, `--arm F`): the real bot (JevBot + decisions)
//! with CodeJev standing in for JEV, on rumbot, RED deck / WHITE stake. One game per worker thread.
//!
//!   jevlab --set sim30-a [--limit 8] --jobs 8 [--name rust-E] [--out runs/_policy_lab]
//!   jevlab --seeds A,B,C --jobs 3
//! Writes `<out>/<name>/results.jsonl` with the keys of policy_lab.py `_play_bot`.

use jevbot::codejev::CodeJev;
use jevbot::engine::RumbotGame;
use jevbot::iface::SharedOracle;
use jevbot::jev_bot::{JevBot, RealClock, Stop};
use jevbot::oracle::RustOracle;
use jevbot::recorder::SystemClock;
use serde_json::{json, Map, Value};
use std::cell::RefCell;
use std::io::Write;
use std::rc::Rc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Mutex;

static ARM_F: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

/// The lab arm name (policy_lab.BOT_ARMS key).
fn arm() -> &'static str {
    if ARM_F.load(Ordering::SeqCst) { "bot-forecast-items" } else { "bot-forecast-long" }
}

fn seed_set(name: &str) -> Option<Vec<String>> {
    jevbot::benchmark::set(name).map(|s| s.iter().map(|x| x.to_string()).collect())
}

fn num(v: &Value) -> f64 {
    v.as_f64().unwrap_or(0.0)
}
fn r1(x: f64) -> f64 {
    (x * 10.0).round() / 10.0
}

/// Wraps the game: records every call as a trace "call" line (method, params, ok/err) into `log`.
struct RecGame {
    inner: Box<dyn jevbot::iface::Game>,
    log: Rc<RefCell<Vec<Value>>>,
    /// record the calls (only needed for JEVLAB_TRACE)
    rec: bool,
}
impl jevbot::iface::Game for RecGame {
    fn call(&mut self, method: &str, params: Option<&Value>) -> Result<Value, jevbot::iface::RpcError> {
        let r = jevbot::prof::time(if method == "gamestate" { "game.gamestate" } else { "game.act" }, || self.inner.call(method, params));
        if !self.rec {
            return r;
        }
        let mut rec = json!({"k": "call", "method": method, "params": params.cloned().unwrap_or(Value::Null)});
        match &r {
            Ok(v) => rec["ok"] = v.clone(),
            Err(e) => rec["err"] = json!({"code": e.code, "message": e.message, "name": e.name}),
        }
        self.log.borrow_mut().push(rec);
        r
    }
    fn backend(&self) -> &str {
        self.inner.backend()
    }
}

/// Strip card `id` fields and timing keys, for comparison.
fn norm(v: &Value) -> Value {
    match jevbot::replay::strip_timing(v) {
        Value::Object(m) => Value::Object(m.into_iter().filter(|(k, _)| k != "id" && k != "sort_id").map(|(k, v)| (k, norm(&v))).collect()),
        Value::Array(a) => Value::Array(a.iter().map(norm).collect()),
        o => o,
    }
}

fn first_diff(a: &Value, b: &Value, path: &str) -> Option<String> {
    match (a, b) {
        (Value::Object(x), Value::Object(y)) => {
            for (k, v) in x {
                match y.get(k) {
                    None => return Some(format!("{path}.{k}: rust {} vs python MISSING", jevbot::trace::short(v, 120))),
                    Some(w) => if let Some(d) = first_diff(v, w, &format!("{path}.{k}")) { return Some(d) },
                }
            }
            y.keys().find(|k| !x.contains_key(*k)).map(|k| format!("{path}.{k}: rust MISSING vs python {}", jevbot::trace::short(&y[k], 120)))
        }
        (Value::Array(x), Value::Array(y)) => {
            for i in 0..x.len().min(y.len()) {
                if let Some(d) = first_diff(&x[i], &y[i], &format!("{path}[{i}]")) { return Some(d) }
            }
            if x.len() != y.len() { Some(format!("{path}: len rust {} vs python {}", x.len(), y.len())) } else { None }
        }
        _ => if a == b { None } else { Some(format!("{path}: rust {} vs python {}", jevbot::trace::short(a, 120), jevbot::trace::short(b, 120))) },
    }
}

/// First differing call between the live log and the Python trace of `seed`.
fn diff_trace(seed: &str, log: &[Value]) -> String {
    let p = jevbot::trace::trace_files(arm()).into_iter().find(|p| p.to_string_lossy().contains(seed));
    let Some(p) = p else { return "no python trace".into() };
    let py: Vec<Value> = jevbot::trace::load_trace(&p).into_iter().filter(|r| r["k"] == "call").collect();
    for (i, r) in log.iter().enumerate() {
        let Some(w) = py.get(i) else { return format!("call {i}: python trace ended ({} calls), rust continues with {}", py.len(), r["method"]) };
        for key in ["method", "params", "ok", "err"] {
            let (a, b) = (norm(r.get(key).unwrap_or(&Value::Null)), norm(w.get(key).unwrap_or(&Value::Null)));
            if let Some(d) = first_diff(&a, &b, key) {
                let prev = if i > 0 { format!("{} {}", log[i - 1]["method"], jevbot::trace::short(&log[i - 1]["params"], 80)) } else { String::new() };
                return format!("call {i} ({} {}) after [{prev}]: {d}", r["method"], jevbot::trace::short(&r["params"], 80));
            }
        }
    }
    if py.len() > log.len() { format!("rust ended at call {} of {}", log.len(), py.len()) } else { "identical".into() }
}

fn play(seed: &str) -> Result<Value, String> {
    let t0 = std::time::Instant::now();
    let tmp = std::env::temp_dir().join(format!("plab_{}_{}", seed, std::process::id()));
    let _ = std::fs::remove_dir_all(&tmp);
    let oracle: SharedOracle = Rc::new(RefCell::new(RustOracle));
    let calls: Rc<RefCell<Vec<Value>>> = Rc::new(RefCell::new(Vec::new()));
    let want_trace = std::env::var_os("JEVLAB_TRACE").is_some();
    let jev = CodeJev::new(ARM_F.load(Ordering::SeqCst), oracle.clone());
    let mut bot = JevBot::new(
        "RED", "WHITE", "code-top-option", &tmp, Box::new(RecGame { inner: Box::new(RumbotGame::new()), log: calls.clone(), rec: want_trace }), Box::new(jev), oracle,
        Box::new(RealClock::new()), "rust", "rust", "rust", Box::new(SystemClock),
    );
    bot.dec = jevbot::decisions::DecisionCfg::lab();
    bot.act("start", Some(&json!({"seed": seed, "deck": "RED", "stake": "WHITE"}))).map_err(|e| format!("start: {e:?}"))?;
    let mut engine_won = false;
    for _ in 0..20000 {
        match bot.step() {
            Ok(_) => {}
            Err(Stop::BenchmarkComplete) => break,
            Err(Stop::Runtime(m)) if m.contains("Victory reported") => {
                let g = bot.game.call("gamestate", None).map_err(|e| e.message)?;
                if g["won"] == json!(true) {
                    engine_won = true;
                    break;
                }
                return Err(m);
            }
            Err(e) => return Err(format!("{e:?}")),
        }
        if bot.stats.runs >= 1 && bot.last_state.as_ref().map_or(false, |g| g["state"] == "MENU") {
            break;
        }
    }
    let dir = bot.recorder.dir().ok_or("no run dir")?.to_path_buf();
    let summ: Value = serde_json::from_str(&std::fs::read_to_string(dir.join("summary.json")).map_err(|e| e.to_string())?).map_err(|e| e.to_string())?;
    let ev: Vec<Value> = std::fs::read_to_string(dir.join("decisions.jsonl")).map_err(|e| e.to_string())?
        .lines().filter(|l| !l.trim().is_empty()).map(|l| serde_json::from_str(l).unwrap()).collect();
    let kind = |e: &Value| e["kind"].as_str().unwrap_or("").to_string();
    let blinds = ev.iter().filter(|e| kind(e) == "round_end" && num(&e["scored"]) >= if num(&e["target"]) != 0.0 { num(&e["target"]) } else { 1.0 }).count();
    let rerolls = ev.iter().filter(|e| kind(e) == "action" && e["method"] == "reroll").count();
    let mut buys: Vec<Value> = ev.iter().filter(|e| kind(e) == "auto" && e["question"] == "shop")
        .map(|e| json!([e["ante"], "auto", e["desc"]])).collect();
    buys.extend(ev.iter().filter(|e| kind(e) == "decision" && e["question"] == "shop"
        && { let c = e["choice"].as_str().unwrap_or(""); c.starts_with("buy") || c.starts_with("replace") })
        .map(|e| json!([e["ante"], "jev", e["choice"]])));
    let buys: Vec<Value> = buys.iter().map(|b| {
        let d = b[2].as_str().map(String::from).unwrap_or_else(|| b[2].to_string());
        json!([b[0], if d.contains("replace") { "swap" } else { "joker" }, b[2]])
    }).collect();
    let fs: Vec<f64> = ev.iter().map(|e| num(&e["forecast_seconds"])).filter(|x| *x != 0.0).collect();
    let ifs: Vec<f64> = ev.iter().map(|e| num(&e["item_forecast_seconds"])).filter(|x| *x != 0.0).collect();
    let won = summ["result"] == "won" || engine_won;
    let mut death = Value::Null;
    if !won {
        let ends: Vec<&Value> = ev.iter().filter(|e| kind(e) == "round_end").collect();
        let plays: Vec<&Value> = ev.iter().filter(|e| kind(e) == "outcome" && e["method"] == "play").collect();
        let last_end = ends.last().map_or(-1.0, |e| num(&e["id"]));
        let fatal: Vec<&&Value> = plays.iter().filter(|p| num(&p["id"]) > last_end).collect();
        let b = &bot._round_blind;
        let hands: Vec<f64> = fatal.iter().map(|p| num(&p["chips_delta"])).collect();
        let best = hands.iter().cloned().fold(0.0, f64::max);
        let jokers = plays.last().map(|p| p["jokers"].clone()).filter(|j| !j.is_null()).unwrap_or(json!([]));
        let eff = b.get("effect").cloned().filter(|e| e.as_str().map_or(!e.is_null(), |s| !s.is_empty())).unwrap_or(Value::Null);
        death = json!({"blind": b.get("name"), "type": b.get("type"), "effect": eff, "target": b.get("score").cloned().unwrap_or(json!(0)),
            "final": fatal.last().map_or(0.0, |p| num(&p["chips"])), "best_hand": best, "hands": hands.len(), "jokers": jokers});
    }
    let mut row = Map::new();
    let mut put = |k: &str, v: Value| { row.insert(k.into(), v); };
    put("seed", json!(seed));
    put("arm", json!(arm()));
    put("won", json!(won));
    put("ante", if won { json!(8) } else { summ["ante"].clone() });
    put("blinds", json!(blinds));
    put("round", summ["round"].clone());
    put("rerolls", json!(rerolls));
    put("reroll_spent", json!(0));
    put("spent", json!(0));
    put("buys", Value::Array(buys));
    put("forecasts", json!(fs.len()));
    put("forecast_errors", json!(0));
    put("forecast_s", json!(r1(fs.iter().sum())));
    put("game_errors", json!(bot.stats.errors));
    put("seconds", json!(r1(t0.elapsed().as_secs_f64())));
    put("death", death);
    put("prompt_version", summ["prompt_version"].clone());
    put("item_fc", json!(ifs.len()));
    put("item_fc_s", json!(r1(ifs.iter().sum())));
    if !want_trace { let _ = std::fs::remove_dir_all(&tmp); }
    if want_trace {
        let log = calls.borrow();
        let d = std::path::PathBuf::from(std::env::var("JEVLAB_TRACE").unwrap());
        let _ = std::fs::create_dir_all(&d);
        use flate2::{write::GzEncoder, Compression};
        let mut gz = GzEncoder::new(std::fs::File::create(d.join(format!("{seed}.jsonl.gz"))).map_err(|e| e.to_string())?, Compression::fast());
        for r in log.iter() { writeln!(gz, "{r}").map_err(|e| e.to_string())?; }
        let _ = writeln!(gz, "{}", json!({"k": "end", "result": row}));
        eprintln!("{seed}: first divergence: {}", diff_trace(seed, &log));
    }
    Ok(Value::Object(row))
}

fn main() {
    let a: Vec<String> = std::env::args().skip(1).collect();
    let opt = |k: &str| a.iter().position(|x| x == k).and_then(|i| a.get(i + 1)).cloned();
    let mut seeds: Vec<String> = match (opt("--seeds"), opt("--set")) {
        (Some(s), _) => s.split(',').map(String::from).collect(),
        (None, Some(n)) => seed_set(&n).unwrap_or_else(|| panic!("unknown seed set {n} (sim30-a, sim60-a, gap4-a)")),
        _ => panic!("usage: jevlab --set NAME | --seeds A,B  [--arm E|F] [--limit N] [--jobs N] [--name NAME] [--out DIR]"),
    };
    if let Some(l) = opt("--limit").and_then(|l| l.parse::<usize>().ok()) {
        seeds.truncate(l);
    }
    if let Some(x) = opt("--arm") {
        assert!(x == "E" || x == "F", "--arm E | F");
        ARM_F.store(x == "F", Ordering::SeqCst);
    }
    let jobs: usize = opt("--jobs").and_then(|j| j.parse().ok()).unwrap_or(1).min(8).max(1);
    let name = opt("--name").unwrap_or_else(|| format!("rust-{}", arm()));
    let out = std::path::PathBuf::from(opt("--out").unwrap_or_else(|| "runs/_policy_lab".into())).join(&name);
    std::fs::create_dir_all(&out).expect("create out dir");
    let file = Mutex::new(std::fs::OpenOptions::new().create(true).append(true).open(out.join("results.jsonl")).expect("open results.jsonl"));
    let next = AtomicUsize::new(0);
    let t0 = std::time::Instant::now();
    std::thread::scope(|s| {
        for _ in 0..jobs {
            s.spawn(|| loop {
                let i = next.fetch_add(1, Ordering::SeqCst);
                let Some(seed) = seeds.get(i) else { break };
                let r = std::panic::catch_unwind(|| jevbot::prof::time("GAME", || play(seed))).unwrap_or_else(|p| Err(jevbot::replay::panic_msg(&*p)));
                let row = r.unwrap_or_else(|e| json!({"seed": seed, "arm": arm(), "error": e}));
                eprintln!("{seed}: ante {} won {} {}s", row["ante"], row["won"], row["seconds"]);
                writeln!(file.lock().unwrap(), "{row}").unwrap();
            });
        }
    });
    jevbot::prof::dump();
    println!("{} games, {jobs} threads, {:.1}s wall, {:.2}s/game wall -> {}", seeds.len(), t0.elapsed().as_secs_f64(), t0.elapsed().as_secs_f64() / seeds.len() as f64, out.display());
}
