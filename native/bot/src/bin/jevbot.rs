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
       jevbot panel [--port 8765] [--backend balatrobot|rumbot] [--code-jev] [--model M] [--root DIR] [--env PATH]"
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
    if argv.first().map(String::as_str) == Some("panel") {
        run_panel(&argv[1..]);
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

/// `jevbot panel [--port 8765] [--backend balatrobot|rumbot] [--code-jev] [--model M] [--root DIR] [--env PATH]`
fn run_panel(a: &[String]) -> ! {
    // The v1 bot (`src/panel.py --legacy`) is not ported; say where it lives instead of ignoring the flag.
    if a.iter().any(|x| x == "--legacy") {
        eprintln!("the v1 bot is Python-only: run src/panel.py --legacy");
        std::process::exit(2);
    }
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
