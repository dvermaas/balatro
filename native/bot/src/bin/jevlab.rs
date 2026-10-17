//! Rust `scripts/policy_lab.py` for arm E (`bot-forecast-long`): the real bot (JevBot + decisions)
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

const SIM30_A: [&str; 30] = [
    "TU9KANLF", "MGBZKPLC", "YZAI217Z", "2YLF77B5", "1YJ4DQ2R", "XFIKZ36U", "665WDGX8", "UDBU5JA1", "11BLBC1T", "6CLKMFPY",
    "1SHJTVG7", "EV2CS4TX", "Z9DXS1JX", "6L3CIXAF", "R8ZXJN6L", "MZ5KZ3JH", "AEQ18EUN", "HXGTTFCH", "3XFPWBMH", "61QF3Z7R",
    "SXYS1LUB", "QNCJLKQ3", "X4JSXYEF", "KGA553W7", "XT9RAZDH", "4R5NYC8E", "68RZS4XR", "F1PE5WEY", "SKP9DVE3", "WKPGXL14",
];
// benchmark.py SETS["sim60-a"] = SETS["sim30-a"] + these 30
const SIM60_B: [&str; 30] = [
    "GY2PWF7D", "Z88P7IDQ", "BIA2QXW3", "8Y1ZLY8X", "57R27KQ2", "GMJWI7Q1", "D172P6BH", "DUSJQ1TN", "MHVZ1JEC", "K792ZEJU",
    "A7MZDVCC", "Z3GELG7B", "DRMQY3JS", "IDXTTNVK", "PCL2K7FC", "XYK41G5M", "JU76QQT6", "USJXYQLN", "2HKM8YZ8", "BAF8SB49",
    "6MV2BE26", "54ATPSF9", "NSMRHDU9", "7NULGXWV", "3F5ILH3G", "D54JXDEK", "GTYIS5F4", "RS718JG4", "RHPJ6FZH", "QQ7KIM4A",
];
const GAP4_A: [&str; 4] = ["XYK41G5M", "68RZS4XR", "R8ZXJN6L", "54ATPSF9"];
const ARM: &str = "bot-forecast-long";

fn seed_set(name: &str) -> Option<Vec<String>> {
    let v: Vec<&str> = match name {
        "sim30-a" => SIM30_A.to_vec(),
        "sim60-a" => SIM30_A.iter().chain(SIM60_B.iter()).copied().collect(),
        "gap4-a" => GAP4_A.to_vec(),
        _ => return None,
    };
    Some(v.into_iter().map(String::from).collect())
}

fn num(v: &Value) -> f64 {
    v.as_f64().unwrap_or(0.0)
}
fn r1(x: f64) -> f64 {
    (x * 10.0).round() / 10.0
}

fn play(seed: &str) -> Result<Value, String> {
    let t0 = std::time::Instant::now();
    let tmp = std::env::temp_dir().join(format!("plab_{}_{}", seed, std::process::id()));
    let _ = std::fs::remove_dir_all(&tmp);
    let oracle: SharedOracle = Rc::new(RefCell::new(RustOracle));
    let jev = CodeJev::new(false, oracle.clone());
    let mut bot = JevBot::new(
        "RED", "WHITE", "code-top-option", &tmp, Box::new(RumbotGame::new()), Box::new(jev), oracle,
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
    put("arm", json!(ARM));
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
    let _ = std::fs::remove_dir_all(&tmp);
    Ok(Value::Object(row))
}

fn main() {
    let a: Vec<String> = std::env::args().skip(1).collect();
    let opt = |k: &str| a.iter().position(|x| x == k).and_then(|i| a.get(i + 1)).cloned();
    let mut seeds: Vec<String> = match (opt("--seeds"), opt("--set")) {
        (Some(s), _) => s.split(',').map(String::from).collect(),
        (None, Some(n)) => seed_set(&n).unwrap_or_else(|| panic!("unknown seed set {n} (sim30-a, sim60-a, gap4-a)")),
        _ => panic!("usage: jevlab --set NAME | --seeds A,B  [--limit N] [--jobs N] [--name NAME] [--out DIR]"),
    };
    if let Some(l) = opt("--limit").and_then(|l| l.parse::<usize>().ok()) {
        seeds.truncate(l);
    }
    let jobs: usize = opt("--jobs").and_then(|j| j.parse().ok()).unwrap_or(1).min(8).max(1);
    let name = opt("--name").unwrap_or_else(|| "rust-bot-forecast-long".into());
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
                let r = std::panic::catch_unwind(|| play(seed)).unwrap_or_else(|p| Err(jevbot::replay::panic_msg(&*p)));
                let row = r.unwrap_or_else(|e| json!({"seed": seed, "arm": ARM, "error": e}));
                eprintln!("{seed}: ante {} won {} {}s", row["ante"], row["won"], row["seconds"]);
                writeln!(file.lock().unwrap(), "{row}").unwrap();
            });
        }
    });
    println!("{} games, {jobs} threads, {:.1}s wall, {:.2}s/game wall -> {}", seeds.len(), t0.elapsed().as_secs_f64(), t0.elapsed().as_secs_f64() / seeds.len() as f64, out.display());
}
