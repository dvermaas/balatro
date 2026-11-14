//! rlenv: play fast-environment games, or check network inference.
//!
//!   rlenv play --policy random|shopper|net [--weights W.brlw] [--temp T] --seeds FILE [--start S] [--count N]
//!              [--jobs J] [--sample-seed K] [--traj DIR] [--out results.jsonl] [--power] [--max-decisions N]
//!              [--order search|xmult|none]   (joker order: v77's search (default), v56's xMult-right rule, none)
//!     (--policy net plays argmax unless --temp is given, then samples at temperature T)
//!   rlenv netcheck --weights W.brlw --obs OBS.bin --out OUT.bin
//!     OBS.bin: u32 n, then n x (u16 nnz, u16[nnz] index, f32[nnz] value); OUT.bin: u32 n, u32 n_actions,
//!     u32 n_value, then n x (f32[n_actions] logits, f32[n_value] raw value outputs).

use rlenv::env::{play_game, EnvCfg};
use rlenv::net::Net;
use rlenv::policy::{NetMode, NetPolicy, Policy, RandomPolicy, ShopperPolicy};
use serde_json::json;
use std::io::Write;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

fn main() {
    let a: Vec<String> = std::env::args().skip(1).collect();
    let opt = |k: &str| a.iter().position(|x| x == k).and_then(|i| a.get(i + 1)).cloned();
    let flag = |k: &str| a.iter().any(|x| x == k);
    if std::env::var("JEVLAB_EXP").is_ok_and(|s| !s.is_empty()) {
        eprintln!("rlenv: JEVLAB_EXP is set; the fast env runs simcore / bot defaults only");
        std::process::exit(2);
    }
    let _ = jevbot::experiment::get(); // the defaults (v77), set once before any thread starts
    match a.first().map(String::as_str) {
        Some("play") => play(&opt, &flag),
        Some("netcheck") => netcheck(&opt),
        _ => {
            eprintln!("usage: rlenv play ... | rlenv netcheck ... (see src/main.rs)");
            std::process::exit(2);
        }
    }
}

fn netcheck(opt: &dyn Fn(&str) -> Option<String>) {
    let net = Net::load(std::path::Path::new(&opt("--weights").expect("--weights"))).unwrap_or_else(|e| panic!("{e}"));
    let b = std::fs::read(opt("--obs").expect("--obs")).expect("read obs");
    let mut p = 0usize;
    let u16_at = |p: usize| u16::from_le_bytes([b[p], b[p + 1]]);
    let n = u32::from_le_bytes(b[0..4].try_into().unwrap()) as usize;
    p += 4;
    let mut out = Vec::new();
    out.extend_from_slice(&(n as u32).to_le_bytes());
    out.extend_from_slice(&(net.n_actions as u32).to_le_bytes());
    out.extend_from_slice(&(net.n_value as u32).to_le_bytes());
    let t0 = std::time::Instant::now();
    for _ in 0..n {
        let nnz = u16_at(p) as usize;
        p += 2;
        let idx: Vec<u16> = (0..nnz).map(|k| u16_at(p + 2 * k)).collect();
        p += 2 * nnz;
        let val: Vec<f32> = (0..nnz).map(|k| f32::from_le_bytes(b[p + 4 * k..p + 4 * k + 4].try_into().unwrap())).collect();
        p += 4 * nnz;
        let t = net.trunk(&idx, &val);
        for l in net.logits(&t) {
            out.extend_from_slice(&l.to_le_bytes());
        }
        for v in net.value(&t) {
            out.extend_from_slice(&v.to_le_bytes());
        }
    }
    eprintln!("netcheck: {n} records, {:.1} us per full forward", t0.elapsed().as_secs_f64() * 1e6 / n.max(1) as f64);
    std::fs::write(opt("--out").expect("--out"), out).expect("write out");
}

fn play(opt: &dyn Fn(&str) -> Option<String>, flag: &dyn Fn(&str) -> bool) {
    let seeds_file = opt("--seeds").expect("--seeds FILE");
    let all: Vec<String> = std::fs::read_to_string(&seeds_file).expect("seeds file").split_whitespace().map(String::from).collect();
    let start: usize = opt("--start").and_then(|x| x.parse().ok()).unwrap_or(0);
    let count: usize = opt("--count").and_then(|x| x.parse().ok()).unwrap_or(all.len());
    let seeds: Vec<String> = all.into_iter().skip(start).take(count).collect();
    let jobs: usize = opt("--jobs").and_then(|x| x.parse().ok()).unwrap_or(1).max(1);
    let sample_seed: u64 = opt("--sample-seed").and_then(|x| x.parse().ok()).unwrap_or(0);
    let pol_name = opt("--policy").unwrap_or_else(|| "shopper".into());
    let net: Option<Arc<Net>> = opt("--weights").map(|w| Arc::new(Net::load(std::path::Path::new(&w)).unwrap_or_else(|e| panic!("{e}"))));
    let mode = match opt("--temp").and_then(|t| t.parse::<f32>().ok()) {
        Some(t) => NetMode::Sample(t),
        None => NetMode::Argmax,
    };
    if pol_name == "net" && net.is_none() {
        panic!("--policy net needs --weights");
    }
    let cfg = EnvCfg {
        max_decisions: opt("--max-decisions").and_then(|x| x.parse().ok()).unwrap_or(3000),
        power: flag("--power"),
        record: opt("--traj").is_some(),
        order: match opt("--order").as_deref() {
            None | Some("search") => rlenv::env::JokerOrder::Search,
            Some("xmult") => rlenv::env::JokerOrder::Xmult,
            Some("none") => rlenv::env::JokerOrder::None,
            Some(x) => panic!("--order search|xmult|none, not {x}"),
        },
    };
    let traj = opt("--traj").map(std::path::PathBuf::from);
    let out = opt("--out").map(|p| Mutex::new(std::fs::OpenOptions::new().create(true).append(true).open(p).expect("open --out")));
    let next = AtomicUsize::new(0);
    let rows: Mutex<Vec<serde_json::Value>> = Mutex::new(Vec::new());
    let t0 = std::time::Instant::now();
    std::thread::scope(|s| {
        for _ in 0..jobs {
            s.spawn(|| {
                let mut pol: Box<dyn Policy> = match pol_name.as_str() {
                    "random" => Box::new(RandomPolicy),
                    "shopper" => Box::new(ShopperPolicy::default()),
                    "net" => Box::new(NetPolicy { net: net.clone().unwrap(), mode }),
                    x => panic!("unknown policy {x}"),
                };
                loop {
                    let i = next.fetch_add(1, Ordering::SeqCst);
                    let Some(seed) = seeds.get(i) else { break };
                    let tg = std::time::Instant::now();
                    let o = play_game(seed, pol.as_mut(), sample_seed, &cfg);
                    let secs = tg.elapsed().as_secs_f64();
                    let mut bytes = 0;
                    if let Some(d) = &traj {
                        bytes = rlenv::traj::write(d, &o).expect("write trajectory").1;
                    }
                    let mut row = json!({"seed": seed, "won": o.won, "ante": o.ante, "blinds": o.blinds, "round": o.round,
                        "decisions": o.decisions, "seconds": (secs * 1e4).round() / 1e4, "targets_planned": o.targets_planned,
                        "targets_fallback": o.targets_fallback, "reorders": o.reorders,
                        "round_s": (o.round_s * 1e4).round() / 1e4, "order_s": (o.order_s * 1e4).round() / 1e4, "policy_s": (o.policy_s * 1e4).round() / 1e4});
                    if o.empty_hand { row["empty_hand"] = json!(true); }
                    if o.truncated { row["truncated"] = json!(true); }
                    if let Some(e) = &o.error { row["error"] = json!(e); }
                    if cfg.power {
                        row["power_s"] = json!((o.power_s * 1e4).round() / 1e4);
                        row["power"] = json!(o.power.iter().map(|(a, p)| json!([a, p.iter().map(|x| (*x as f64 * 1000.0).round() / 1000.0).collect::<Vec<_>>()])).collect::<Vec<_>>());
                    }
                    if traj.is_some() { row["traj_bytes"] = json!(bytes); }
                    if let Some(f) = &out {
                        writeln!(f.lock().unwrap(), "{row}").unwrap();
                    }
                    rows.lock().unwrap().push(row);
                }
            });
        }
    });
    let wall = t0.elapsed().as_secs_f64();
    let rows = rows.into_inner().unwrap();
    let n = rows.len().max(1) as f64;
    let f = |k: &str| rows.iter().map(|r| r[k].as_f64().unwrap_or(0.0)).sum::<f64>();
    let wins = rows.iter().filter(|r| r["won"] == json!(true)).count();
    let mut deaths = [0usize; 10];
    for r in rows.iter().filter(|r| r["won"] != json!(true)) {
        deaths[(r["ante"].as_i64().unwrap_or(0).clamp(0, 9)) as usize] += 1;
    }
    let errors = rows.iter().filter(|r| r.get("error").is_some()).count();
    let trunc = rows.iter().filter(|r| r.get("truncated").is_some()).count();
    let empty = rows.iter().filter(|r| r.get("empty_hand").is_some()).count();
    println!(
        "{} games, policy {pol_name}{}, {jobs} threads: {wall:.2}s wall, {:.2} games/s, {:.1} ms/game CPU, {:.1} decisions/game",
        rows.len(),
        if pol_name == "net" { format!(" ({mode:?})") } else { String::new() },
        rows.len() as f64 / wall,
        1000.0 * f("seconds") / n,
        f("decisions") / n
    );
    println!(
        "wins {wins} ({:.1}%), mean ante {:.2}, mean blinds {:.2}; deaths by ante {:?}; errors {errors}, truncated {trunc}, empty-hand losses {empty}; targets planned {} fallback {}{}",
        100.0 * wins as f64 / n,
        f("ante") / n,
        f("blinds") / n,
        &deaths[1..9],
        f("targets_planned"),
        f("targets_fallback"),
        if cfg.power { format!("; power {:.1} ms/game", 1000.0 * f("power_s") / n) } else { String::new() }
    );
    println!("per game: round player {:.1} ms, joker order + build suit {:.1} ms, policy {:.1} ms", 1000.0 * f("round_s") / n, 1000.0 * f("order_s") / n, 1000.0 * f("policy_s") / n);
}
