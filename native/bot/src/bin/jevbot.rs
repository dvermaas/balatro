//! `jevbot play`: the Rust bot (JevBot) on a real backend, with TypeSafe JEV or the code stand-in.
//!
//!   jevbot play [--backend balatrobot|rumbot] [--seed S] [--deck RED] [--stake WHITE]
//!               [--model jev-latest] [--runs N] [--code-jev] [--env PATH] [--root DIR]
//!
//! balatrobot plays the modded game at 127.0.0.1:12346 (logs under <root>/runs); rumbot plays the
//! headless simulator (logs under <root>/runs/sim, like Python's JEV_BACKEND=sim). JEV's key comes
//! from `.env` (JEV_API_KEY or TYPESAFE_API_KEY, already set in the environment wins). `--code-jev`
//! uses CodeJev instead: no key, no tokens. `--seed` starts the first run with that seed; later runs
//! take the game's own seeds. The bot keeps its recorder and events as in Python.

use jevbot::balatro::Balatro;
use jevbot::codejev::CodeJev;
use jevbot::decisions::DecisionCfg;
use jevbot::engine::RumbotGame;
use jevbot::iface::{Game, Jev, SharedOracle};
use jevbot::jev_bot::{JevBot, RealClock, Stop};
use jevbot::jev_client::{api_key_from_env, load_env, TypeSafeJev};
use jevbot::oracle::RustOracle;
use jevbot::recorder::{Clock, SystemClock};
use serde_json::json;
use std::cell::RefCell;
use std::path::PathBuf;
use std::rc::Rc;

/// The Rust prompt version: sha1 of the behaviour-defining Rust sources.
fn version() -> String {
    jevbot::recorder::bot_version(&jevbot::recorder::native_dir())
}

struct Args {
    backend: String,
    seed: Option<String>,
    deck: String,
    stake: String,
    model: String,
    runs: i64,
    code_jev: bool,
    env: PathBuf,
    root: PathBuf,
}

fn usage() -> ! {
    eprintln!(
        "usage: jevbot play [--backend balatrobot|rumbot] [--seed S] [--deck RED] [--stake WHITE] \
         [--model jev-latest] [--runs N] [--code-jev] [--env PATH] [--root DIR]
       jevbot panel [--port 8765] [--backend balatrobot|rumbot] [--code-jev] [--model M] [--root DIR] [--env PATH]
       jevbot sim (--set NAME | --seeds A,B) [--jobs N] [--code-jev] [--model M] [--deck RED] [--stake WHITE] [--root DIR] [--env PATH]
       jevbot extract-loc [--exe Balatro.exe] [--smods DIR] [--out DIR]"
    );
    std::process::exit(2)
}

fn parse(a: &[String]) -> Args {
    if a.first().map(String::as_str) != Some("play") {
        usage();
    }
    let repo = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let mut out = Args {
        backend: "balatrobot".into(),
        seed: None,
        deck: "RED".into(),
        stake: "WHITE".into(),
        model: "jev-latest".into(),
        runs: 1,
        code_jev: false,
        env: repo.join(".env"),
        root: repo.clone(),
    };
    let mut i = 1;
    while i < a.len() {
        let flag = a[i].as_str();
        if flag == "--code-jev" {
            out.code_jev = true;
            i += 1;
            continue;
        }
        let val = a.get(i + 1).cloned().unwrap_or_else(|| usage());
        match flag {
            "--backend" => out.backend = val,
            "--seed" => out.seed = Some(val),
            "--deck" => out.deck = val,
            "--stake" => out.stake = val,
            "--model" => out.model = val,
            "--runs" => out.runs = val.parse().unwrap_or_else(|_| usage()),
            "--env" => out.env = PathBuf::from(val),
            "--root" => out.root = PathBuf::from(val),
            _ => usage(),
        }
        i += 2;
    }
    if !matches!(out.backend.as_str(), "balatrobot" | "rumbot") {
        eprintln!("unknown --backend {} (use balatrobot or rumbot)", out.backend);
        std::process::exit(2);
    }
    out
}

fn main() {
    let argv: Vec<String> = std::env::args().skip(1).collect();
    match argv.first().map(String::as_str) {
        Some("panel") => run_panel(&argv[1..]),
        Some("extract-loc") => extract_loc(&argv[1..]),
        Some("sim") => run_sim(&argv[1..]),
        _ => {}
    }
    let a = parse(&argv);
    if let Err(e) = load_env(&a.env) {
        eprintln!("cannot read {}: {e}", a.env.display());
        std::process::exit(1);
    }
    let oracle: SharedOracle = Rc::new(RefCell::new(RustOracle));
    let sim = a.backend == "rumbot";
    let game: Box<dyn Game> = if sim { Box::new(RumbotGame::new()) } else { Box::new(Balatro::new()) };
    let (jev, model): (Box<dyn Jev>, String) = if a.code_jev {
        (Box::new(CodeJev::new(false, oracle.clone())), "code-jev".into())
    } else {
        let key = api_key_from_env().unwrap_or_else(|| {
            eprintln!("JEV_API_KEY missing from .env");
            std::process::exit(1)
        });
        let jev = TypeSafeJev::new(&key, &a.model).unwrap_or_else(|e| {
            eprintln!("{e}");
            std::process::exit(1)
        });
        (Box::new(jev), a.model.clone())
    };
    let log_dir = if sim { a.root.join("runs").join("sim") } else { a.root.join("runs") };
    let mut bot = JevBot::new(
        &a.deck, &a.stake, &model, log_dir, game, jev, oracle, Box::new(RealClock::new()), &version(), &version(), &version(),
        Box::new(SystemClock),
    );
    bot.dec = DecisionCfg::production();
    jevbot::recorder::snapshot_prompts(&a.root.join("runs").join("_prompts"), &version(), SystemClock.time());
    if let Some(seed) = &a.seed {
        if let Err(e) = bot.act("start", Some(&json!({"deck": a.deck, "stake": a.stake, "seed": seed}))) {
            eprintln!("start: {e}");
            std::process::exit(1);
        }
    }
    let code = loop {
        match bot.step() {
            Ok(_) => {}
            Err(Stop::BenchmarkComplete) => break 0,
            Err(Stop::Rpc(e)) => {
                eprintln!("game error: {e}");
                break 1;
            }
            Err(Stop::Jev(m)) => {
                eprintln!("Jev API: {m}");
                break 1;
            }
            Err(Stop::Runtime(m)) => {
                eprintln!("stopped: {m}");
                break 1;
            }
        }
        if bot.stats.runs >= a.runs {
            break 0;
        }
    };
    println!("{:?}", bot.stats);
    std::process::exit(code);
}

/// `jevbot sim (--set NAME | --seeds A,B) [--jobs N] [--code-jev] ...`: whole games on rumbot with the
/// production bot (the former scripts/sim_run.py), one game per worker thread. JEV games are recorded
/// under <root>/runs/sim, `--code-jev` ones under runs/sim_smoke (so no analysis mistakes them for JEV
/// runs). `--set` plays a benchmark set and marks each summary's `benchmark`. Prints one JSON line per
/// game, then a summary line; exits 1 if any game failed.
fn run_sim(a: &[String]) -> ! {
    let repo = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let (mut set, mut seeds, mut jobs) = (None::<String>, Vec::<String>::new(), 1usize);
    let (mut code_jev, mut model, mut deck, mut stake) = (false, "jev-latest".to_string(), "RED".to_string(), "WHITE".to_string());
    let (mut root, mut env, mut max_steps) = (repo.clone(), repo.join(".env"), 20000usize);
    let mut i = 0;
    while i < a.len() {
        if a[i] == "--code-jev" {
            code_jev = true;
            i += 1;
            continue;
        }
        let v = a.get(i + 1).cloned().unwrap_or_else(|| usage());
        match a[i].as_str() {
            "--set" => set = Some(v),
            "--seeds" | "--seed" => seeds.extend(v.split(',').map(str::trim).filter(|s| !s.is_empty()).map(String::from)),
            "--jobs" => jobs = v.parse().unwrap_or_else(|_| usage()),
            "--model" => model = v,
            "--deck" => deck = v,
            "--stake" => stake = v,
            "--root" => root = PathBuf::from(v),
            "--env" => env = PathBuf::from(v),
            "--max-steps" => max_steps = v.parse().unwrap_or_else(|_| usage()),
            _ => usage(),
        }
        i += 2;
    }
    // (seed, summary.benchmark); a set may repeat a seed as "SEED#n"
    let mut tasks: Vec<(String, serde_json::Value)> = Vec::new();
    if let Some(name) = &set {
        let s = jevbot::benchmark::set(name).unwrap_or_else(|| {
            eprintln!("unknown set {name}");
            std::process::exit(2)
        });
        for (idx, seed) in s.iter().enumerate() {
            let seed = seed.split('#').next().unwrap_or(seed).to_string();
            tasks.push((seed, json!({"name": name, "index": idx, "of": s.len()})));
        }
    }
    tasks.extend(seeds.into_iter().map(|s| (s, serde_json::Value::Null)));
    if tasks.is_empty() {
        usage();
    }
    let key = if code_jev {
        None
    } else {
        if let Err(e) = load_env(&env) {
            eprintln!("cannot read {}: {e}", env.display());
            std::process::exit(1);
        }
        Some(api_key_from_env().unwrap_or_else(|| {
            eprintln!("JEV_API_KEY missing from .env");
            std::process::exit(1)
        }))
    };
    // once, before the workers start (they would race on runs/_prompts/index.json)
    jevbot::recorder::snapshot_prompts(&root.join("runs").join("_prompts"), &version(), SystemClock.time());
    let log_dir = root.join("runs").join(if code_jev { "sim_smoke" } else { "sim" });
    let next = std::sync::atomic::AtomicUsize::new(0);
    let results = std::sync::Mutex::new(Vec::new());
    let t0 = std::time::Instant::now();
    std::thread::scope(|s| {
        for _ in 0..jobs.clamp(1, tasks.len()) {
            s.spawn(|| loop {
                let i = next.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
                let Some((seed, bench)) = tasks.get(i) else { break };
                let r = std::panic::catch_unwind(|| sim_game(seed, bench, key.as_deref(), &model, &deck, &stake, &log_dir, max_steps))
                    .unwrap_or_else(|p| json!({"seed": seed, "error": jevbot::replay::panic_msg(&*p)}));
                println!("{r}");
                results.lock().unwrap().push(r);
            });
        }
    });
    let results = results.into_inner().unwrap();
    let done: Vec<&serde_json::Value> = results.iter().filter(|r| r["result"] == "won" || r["result"] == "lost").collect();
    let sum = |k: &str| results.iter().map(|r| r[k].as_i64().unwrap_or(0)).sum::<i64>();
    let errors = results.iter().filter(|r| r.get("error").is_some()).count();
    println!("{}", json!({
        "games": results.len(), "finished": done.len(), "won": done.iter().filter(|r| r["result"] == "won").count(),
        "avg_ante": if done.is_empty() { serde_json::Value::Null } else {
            json!(pycompat::pyround(done.iter().map(|r| r["ante"].as_f64().unwrap_or(0.0)).sum::<f64>() / done.len() as f64, 2))
        },
        "errors": errors, "input_tokens": sum("input_tokens"), "output_tokens": sum("output_tokens"),
        "wall_seconds": pycompat::pyround(t0.elapsed().as_secs_f64(), 1),
    }));
    std::process::exit(if errors == 0 { 0 } else { 1 })
}

/// One seeded `jevbot sim` game to the end: a result row, never a panic of its own.
#[allow(clippy::too_many_arguments)]
fn sim_game(seed: &str, bench: &serde_json::Value, key: Option<&str>, model: &str, deck: &str, stake: &str, log_dir: &std::path::Path, max_steps: usize) -> serde_json::Value {
    let t0 = std::time::Instant::now();
    let oracle: SharedOracle = Rc::new(RefCell::new(RustOracle));
    let (jev, model): (Box<dyn Jev>, String) = match key {
        None => (Box::new(CodeJev::new(false, oracle.clone())), "code-jev".into()),
        Some(k) => match TypeSafeJev::new(k, model) {
            Ok(j) => (Box::new(j), model.to_string()),
            Err(e) => return json!({"seed": seed, "error": e.to_string()}),
        },
    };
    let mut bot = JevBot::new(
        deck, stake, &model, log_dir, Box::new(RumbotGame::new()), jev, oracle, Box::new(RealClock::new()), &version(), &version(),
        &version(), Box::new(SystemClock),
    );
    bot.dec = DecisionCfg::production();
    bot.bench_current = bench.clone();
    let mut row = json!({"seed": seed});
    let mut steps = 0;
    let err = match bot.act("start", Some(&json!({"seed": seed, "deck": deck, "stake": stake}))) {
        Err(e) => Some(format!("start: {e}")),
        Ok(_) => loop {
            if steps >= max_steps {
                bot.recorder.finish("abandoned", bot.last_state.clone().as_ref());
                break Some(format!("no game end after {max_steps} steps"));
            }
            steps += 1;
            match bot.step() {
                Ok(_) => {}
                Err(Stop::BenchmarkComplete) => break None,
                Err(e) => {
                    bot.recorder.finish("abandoned", bot.last_state.clone().as_ref());
                    break Some(format!("{e:?}"));
                }
            }
            if bot.stats.runs >= 1 && bot.last_state.as_ref().map_or(false, |g| g["state"] == "MENU") {
                break None;
            }
        },
    };
    if let Some(dir) = bot.recorder.dir() {
        if let Ok(t) = std::fs::read_to_string(dir.join("summary.json")) {
            let summ: serde_json::Value = serde_json::from_str(&t).unwrap_or_default();
            for k in ["run_id", "result", "ante", "round"] {
                row[k] = summ[k].clone();
            }
        }
    }
    let st = &bot.stats;
    row["steps"] = json!(steps);
    row["decisions"] = json!(st.decisions);
    row["input_tokens"] = json!(st.input_tokens);
    row["output_tokens"] = json!(st.output_tokens);
    row["jev_seconds"] = json!(pycompat::pyround(st.jev_ms as f64 / 1000.0, 1));
    row["game_errors"] = json!(st.errors);
    row["seconds"] = json!(pycompat::pyround(t0.elapsed().as_secs_f64(), 1));
    if let Some(e) = err {
        row["error"] = json!(e);
    }
    row
}

/// `jevbot extract-loc [--exe Balatro.exe] [--smods DIR] [--out DIR]`: copy the game's localization
/// (never committed) to where sim games read it, $BALATRO_LOC_DIR or runs/_loc. Run once per machine.
fn extract_loc(a: &[String]) -> ! {
    let appdata = std::env::var_os("APPDATA").map(PathBuf::from).unwrap_or_default();
    let mut exe = PathBuf::from(r"C:\Program Files (x86)\Steam\steamapps\common\Balatro\Balatro.exe");
    let mut smods = appdata.join("Balatro").join("Mods").join("smods");
    let mut out = jevbot::simtext::localization_dir();
    for pair in a.chunks(2) {
        let v = pair.get(1).map(PathBuf::from).unwrap_or_else(|| usage());
        match pair[0].as_str() {
            "--exe" => exe = v,
            "--smods" => smods = v,
            "--out" => out = v,
            _ => usage(),
        }
    }
    match jevbot::simtext::extract_localization(&exe, Some(&smods), &out) {
        Ok(files) => {
            for f in &files {
                println!("wrote {}", f.display());
            }
            if !files.iter().any(|f| f.ends_with("smods-en-us.lua")) {
                eprintln!("no Steamodded localization at {} (optional)", smods.join("localization").join("en-us.lua").display());
            }
            std::process::exit(0)
        }
        Err(e) => {
            eprintln!("{e}");
            std::process::exit(1)
        }
    }
}

/// `jevbot panel [--port 8765] [--backend balatrobot|rumbot] [--code-jev] [--model M] [--root DIR] [--env PATH]`
fn run_panel(a: &[String]) -> ! {
    let mut cfg = jevbot::panel::PanelConfig::new(jevbot::panel::repo_root());
    let (mut port, mut env): (u16, Option<PathBuf>) = (8765, None);
    let mut i = 0;
    while i < a.len() {
        if a[i] == "--code-jev" {
            cfg.code_jev = true;
            i += 1;
            continue;
        }
        let v = a.get(i + 1).cloned().unwrap_or_else(|| usage());
        match a[i].as_str() {
            "--port" => port = v.parse().unwrap_or_else(|_| usage()),
            "--backend" => cfg.backend = v,
            "--model" => cfg.model = v,
            "--root" => cfg.root = PathBuf::from(v),
            "--env" => env = Some(PathBuf::from(v)),
            _ => usage(),
        }
        i += 2;
    }
    if !matches!(cfg.backend.as_str(), "balatrobot" | "rumbot") {
        usage();
    }
    if !cfg.code_jev {
        let _ = load_env(&env.unwrap_or_else(|| cfg.root.join(".env")));
    }
    let l = std::net::TcpListener::bind(("127.0.0.1", port)).unwrap_or_else(|e| {
        eprintln!("cannot bind 127.0.0.1:{port}: {e}");
        std::process::exit(1)
    });
    println!("JEV Balatro control panel: http://127.0.0.1:{port}  (bot starts paused)");
    jevbot::panel::start(cfg, l);
    loop {
        std::thread::sleep(std::time::Duration::from_secs(3600));
    }
}
