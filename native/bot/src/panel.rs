//! Port of src/panel.py: the local control panel server (std::net, hand-written HTTP/1.1) and the
//! bot controller thread. `JevBot` is not `Send` (Rc oracle), so the worker thread builds the bot
//! itself and publishes a snapshot (events, last state, stats, current run) after every step; HTTP
//! threads only read the snapshot and the runs directory, and queue commands for the worker.
//!
//! Not ported: benchmark play (`POST /api/benchmark` answers 500, the JevBot queue is parked),
//! `--legacy`, the Python traceback in error events (`trace` is omitted).

use crate::analytics;
use crate::iface::{Game, Jev, SharedOracle};
use crate::jev_bot::{summarize, JevBot, RealClock, Stop};
use crate::recorder::{Clock, SystemClock};
use serde_json::{json, Map, Value};
use std::collections::VecDeque;
use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::panic::{catch_unwind, AssertUnwindSafe};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

/// Version tag the Rust bot stamps on its runs (no prompt hash in the port yet).
/// The Rust bot's prompt version id: sha1 of the Rust sources that define behaviour (recorder::bot_version).
pub fn version() -> &'static str {
    static V: std::sync::OnceLock<String> = std::sync::OnceLock::new();
    V.get_or_init(|| crate::recorder::bot_version(&crate::recorder::native_dir()))
}

#[derive(Clone)]
pub struct PanelConfig {
    /// "rumbot" (headless sim, runs under <root>/runs/sim) or "balatrobot" (real game).
    pub backend: String,
    pub code_jev: bool,
    pub model: String,
    pub deck: String,
    pub stake: String,
    /// Repo root: panel.html is read from <root>/src/panel.html, runs from <root>/runs.
    pub root: PathBuf,
    /// Pause between steps while running (Python 0.3 s).
    pub delay: f64,
}

impl PanelConfig {
    pub fn new(root: impl Into<PathBuf>) -> Self {
        PanelConfig {
            backend: "balatrobot".into(), code_jev: false, model: "jev-latest".into(), deck: "RED".into(),
            stake: "WHITE".into(), root: root.into(), delay: 0.3,
        }
    }
    fn sim(&self) -> bool {
        self.backend == "rumbot"
    }
    fn runs_dir(&self) -> PathBuf {
        if self.sim() { self.root.join("runs").join("sim") } else { self.root.join("runs") }
    }
    fn prompts_dir(&self) -> PathBuf {
        self.root.join("runs").join("_prompts")
    }
    fn backend_name(&self) -> &'static str {
        if self.sim() { "jackdaw" } else { "balatrobot" }
    }
}

#[derive(Default, Clone)]
struct Snap {
    events: Vec<Value>,
    last_state: Option<Value>,
    stats: Value,
    run: Value,
    auto_new_run: bool,
    bench_current: Value,
    bench_remaining: usize,
    bench_name: String,
}

struct Shared {
    mode: String,
    pending: i64,
    delay: f64,
    cmds: VecDeque<(String, Value)>,
    snap: Snap,
}

type Sh = Arc<Mutex<Shared>>;

fn now() -> f64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_secs_f64()).unwrap_or(0.0)
}

fn build_bot(cfg: &PanelConfig) -> Result<JevBot, String> {
    use crate::balatro::Balatro;
    use crate::codejev::CodeJev;
    use crate::engine::RumbotGame;
    use crate::jev_client::{api_key_from_env, TypeSafeJev};
    use crate::oracle::RustOracle;
    use std::cell::RefCell;
    use std::rc::Rc;
    let oracle: SharedOracle = Rc::new(RefCell::new(RustOracle));
    let game: Box<dyn Game> = if cfg.sim() { Box::new(RumbotGame::new()) } else { Box::new(Balatro::new()) };
    let (jev, model): (Box<dyn Jev>, String) = if cfg.code_jev {
        (Box::new(CodeJev::new(false, oracle.clone())), "code-jev".into())
    } else {
        let key = api_key_from_env().ok_or("JEV_API_KEY missing (use --code-jev or set it in .env)")?;
        (Box::new(TypeSafeJev::new(&key, &cfg.model)?), cfg.model.clone())
    };
    let mut bot = JevBot::new(
        &cfg.deck, &cfg.stake, &model, cfg.runs_dir(), game, jev, oracle, Box::new(RealClock::new()), version(), version(),
        version(), Box::new(SystemClock),
    );
    bot.dec = crate::decisions::DecisionCfg::production();
    crate::recorder::snapshot_prompts(&cfg.prompts_dir(), version(), SystemClock.time());
    Ok(bot)
}

fn publish(sh: &Sh, bot: &JevBot) {
    let run = bot
        .recorder
        .dir()
        .and_then(|d| std::fs::read_to_string(d.join("summary.json")).ok())
        .and_then(|t| serde_json::from_str::<Value>(&t).ok())
        .unwrap_or(Value::Null);
    let s = &bot.stats;
    let snap = Snap {
        events: bot.events.iter().cloned().collect(),
        last_state: bot.last_state.clone(),
        stats: json!({"decisions": s.decisions, "input_tokens": s.input_tokens, "output_tokens": s.output_tokens,
                      "jev_ms": s.jev_ms, "runs": s.runs, "wins": s.wins, "best_ante": s.best_ante, "errors": s.errors}),
        run,
        auto_new_run: bot.auto_new_run,
        bench_current: bot.bench_current.clone(),
        bench_remaining: bot.bench_queue.len(),
        bench_name: if bot.bench_name.is_empty() { crate::benchmark::NAME.into() } else { bot.bench_name.clone() },
    };
    sh.lock().unwrap().snap = snap;
}

fn pause(sh: &Sh) {
    let mut g = sh.lock().unwrap();
    g.mode = "paused".into();
    g.pending = 0;
}

fn panic_text(p: &Box<dyn std::any::Any + Send>) -> String {
    p.downcast_ref::<String>().cloned().or_else(|| p.downcast_ref::<&str>().map(|s| s.to_string())).unwrap_or_default()
}

fn run_cmd(bot: &mut JevBot, cmd: &str, body: &Value) {
    match cmd {
        "settings" => {
            if let Some(a) = body.get("auto_new_run") {
                bot.auto_new_run = crate::gs::truthy(a);
            }
        }
        "abandon" => {
            bot.emit("info", json!({"msg": "Abandoning run (back to menu)"}));
            bot.bench_queue.clear();
            bot.bench_done_pending = false;
            bot.recorder.finish("abandoned", None);
            if let Err(e) = bot.game.call("menu", None) {
                bot.emit("error", json!({"msg": format!("RPCError: {e}")}));
            }
        }
        "benchmark" => bot.start_benchmark(body.get("set").and_then(Value::as_str)),
        "cancel_benchmark" => {
            bot.bench_queue.clear();
            bot.bench_done_pending = false;
            bot.emit("info", json!({"msg": "Benchmark cancelled; the current run will finish normally"}));
        }
        _ => {}
    }
}

/// panel.py: `except TypeSafeError as e: emit("error", msg=f"Jev API: {e}")` (the text carries TypeSafe's
/// HTTP status, e.g. the 402 "no usable credits" message).
pub fn jev_error_text(e: &str) -> String {
    format!("Jev API: {e}")
}

fn worker(cfg: PanelConfig, sh: Sh) {
    let mut bot = match build_bot(&cfg) {
        Ok(b) => b,
        Err(e) => {
            eprintln!("panel: cannot build the bot: {e}");
            return;
        }
    };
    publish(&sh, &bot);
    loop {
        let (cmds, go, delay) = {
            let mut g = sh.lock().unwrap();
            let cmds: Vec<_> = g.cmds.drain(..).collect();
            (cmds, g.mode == "running" || g.pending > 0, g.delay)
        };
        if !cmds.is_empty() {
            for (c, b) in &cmds {
                run_cmd(&mut bot, c, b);
            }
            publish(&sh, &bot);
        }
        if !go {
            std::thread::sleep(Duration::from_millis(if cmds.is_empty() { 100 } else { 1 }));
            continue;
        }
        let r = catch_unwind(AssertUnwindSafe(|| bot.step()));
        match r {
            Ok(Ok(acted)) => {
                if acted {
                    {
                        let mut g = sh.lock().unwrap();
                        g.pending = (g.pending - 1).max(0);
                    }
                    publish(&sh, &bot);
                    std::thread::sleep(Duration::from_secs_f64(delay));
                } else {
                    publish(&sh, &bot);
                }
                continue;
            }
            Ok(Err(Stop::BenchmarkComplete)) => {}
            Ok(Err(Stop::Jev(e))) => bot.emit("error", json!({"msg": jev_error_text(&e)})),
            Ok(Err(Stop::Rpc(e))) => bot.emit("error", json!({"msg": format!("RPCError: {e}")})),
            Ok(Err(Stop::Runtime(m))) => bot.emit("error", json!({"msg": format!("RuntimeError: {m}")})),
            Err(p) => {
                let m = panic_text(&p);
                let lm = m.to_lowercase();
                let msg = if lm.contains("timed out") {
                    "Balatro didn't respond for 60s. Usually a popup (e.g. 'Joker unlocked') is open in the game: click it away, then press Run.".to_string()
                } else if lm.contains("connect") || lm.contains("refused") {
                    format!("Can't reach Balatro ({m}). Is the game running?")
                } else {
                    format!("Panic: {m}")
                };
                bot.emit("error", json!({"msg": msg}));
            }
        }
        pause(&sh);
        publish(&sh, &bot);
    }
}

// ---------------------------------------------------------------- HTTP

fn command(sh: &Sh, cmd: &str, body: &Value) -> Result<(), String> {
    let mut g = sh.lock().unwrap();
    match cmd {
        "run" => g.mode = "running".into(),
        "pause" => {
            g.mode = "paused".into();
            g.pending = 0;
        }
        "step" => {
            g.mode = "paused".into();
            g.pending += 1;
        }
        "settings" => {
            if let Some(d) = body.get("delay") {
                g.delay = d
                    .as_f64()
                    .or_else(|| d.as_str().and_then(|s| s.parse().ok()))
                    .ok_or("could not convert delay to float")?;
            }
            if let Some(a) = body.get("auto_new_run") {
                g.snap.auto_new_run = crate::gs::truthy(a);
            }
            g.cmds.push_back((cmd.into(), body.clone()));
        }
        "abandon" => {
            g.mode = "paused".into();
            g.pending = 0;
            g.cmds.push_back((cmd.into(), body.clone()));
        }
        "benchmark" => {
            g.mode = "running".into();
            g.cmds.push_back((cmd.into(), body.clone()));
        }
        "cancel_benchmark" => g.cmds.push_back((cmd.into(), body.clone())),
        _ => return Err(format!("unknown command {cmd}")),
    }
    Ok(())
}

fn live(cfg: &PanelConfig, sh: &Sh, since: i64) -> Value {
    let (mode, delay, snap) = {
        let g = sh.lock().unwrap();
        (g.mode.clone(), g.delay, g.snap.clone())
    };
    let t = now();
    let events = &snap.events;
    let recent = events
        .iter()
        .filter(|e| e["kind"] == "decision" && t - e["t"].as_f64().unwrap_or(0.0) <= 60.0)
        .count();
    let backend = cfg.backend_name();
    let labels = analytics::prompt_versions(&cfg.prompts_dir(), version());
    let label = labels
        .iter()
        .find(|v| v["version"] == version())
        .and_then(|v| v["label"].as_str())
        .unwrap_or("?")
        .to_string();
    let on_version: Vec<Value> = analytics::all_runs(&cfg.runs_dir())
        .into_iter()
        .filter(|r| r["prompt_version"] == version() && analytics::run_backend(r) == backend)
        .collect();
    let durations: Vec<f64> = on_version
        .iter()
        .filter(|r| crate::gs::truthy(&r["ended"]))
        .map(|r| r["ended"].as_f64().unwrap_or(0.0) - r["started"].as_f64().unwrap_or(0.0))
        .collect();
    let avg = if durations.is_empty() {
        Value::Null
    } else {
        json!((durations.iter().sum::<f64>() / durations.len() as f64).round_ties_even() as i64)
    };
    let finished = on_version.iter().filter(|r| matches!(r["result"].as_str(), Some("won") | Some("lost"))).count();
    let last_id = events.last().map(|e| e["id"].clone()).unwrap_or(json!(0));
    let mut sel: Vec<Value> = events
        .iter()
        .filter(|e| e["id"].as_i64().unwrap_or(0) > since)
        .map(|e| {
            let mut m: Map<String, Value> = e.as_object().cloned().unwrap_or_default();
            m.remove("state_sent");
            Value::Object(m)
        })
        .collect();
    if sel.len() > 150 {
        sel.drain(..sel.len() - 150);
    }
    let gs = snap.last_state.as_ref();
    json!({
        "mode": mode, "backend": backend, "delay": delay, "auto_new_run": snap.auto_new_run, "stats": snap.stats,
        "game_state": gs.and_then(|g| g.get("state")).cloned().unwrap_or(Value::Null),
        "game": gs.map(summarize).unwrap_or(Value::Null),
        "current_run": snap.run,
        "version": {"id": version(), "label": label, "legacy": false, "runs": on_version.len(), "finished": finished},
        "rate": {"decisions_per_min": recent, "avg_run_seconds": avg},
        "benchmark": {"name": snap.bench_name, "current": snap.bench_current, "remaining": snap.bench_remaining},
        "last_id": last_id, "events": sel,
    })
}

fn pct_decode(s: &str) -> String {
    let b = s.as_bytes();
    let mut out = Vec::new();
    let mut i = 0;
    while i < b.len() {
        if b[i] == b'%' && i + 2 < b.len() {
            if let Ok(v) = u8::from_str_radix(&s[i + 1..i + 3], 16) {
                out.push(v);
                i += 3;
                continue;
            }
        }
        out.push(if b[i] == b'+' { b' ' } else { b[i] });
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

fn query_get(q: &str, key: &str) -> Option<String> {
    q.split('&').filter_map(|kv| kv.split_once('=')).find(|(k, _)| *k == key).map(|(_, v)| pct_decode(v))
}

/// (status, content type, body)
fn route_get(cfg: &PanelConfig, sh: &Sh, target: &str) -> (u16, &'static str, Vec<u8>) {
    let (path, query) = target.split_once('?').unwrap_or((target, ""));
    let json_resp = |code: u16, v: Value| (code, "application/json", pycompat::json::dumps(&v).into_bytes());
    let runs = cfg.runs_dir();
    let backend = cfg.backend_name();
    match path {
        "/" => match std::fs::read(cfg.root.join("src").join("panel.html")) {
            Ok(b) => (200, "text/html; charset=utf-8", b),
            Err(e) => json_resp(500, json!({"error": e.to_string()})),
        },
        "/api/live" => {
            let since = query_get(query, "since").and_then(|s| s.parse().ok()).unwrap_or(0);
            json_resp(200, live(cfg, sh, since))
        }
        "/api/runs" => {
            let labels = analytics::prompt_versions(&cfg.prompts_dir(), version());
            let out: Vec<Value> = analytics::all_runs(&runs)
                .into_iter()
                .map(|r| {
                    let l = labels.iter().find(|v| v["version"] == r["prompt_version"] && !r["prompt_version"].is_null());
                    let mut m = r.as_object().cloned().unwrap_or_default();
                    m.insert("prompt_label".into(), l.map(|v| v["label"].clone()).unwrap_or(json!("?")));
                    Value::Object(m)
                })
                .collect();
            json_resp(200, Value::Array(out))
        }
        "/api/benchmark" => json_resp(
            200,
            analytics::benchmark_matrix(query_get(query, "set").as_deref(), backend, &runs, &cfg.prompts_dir(), version()),
        ),
        "/api/prompts" => json_resp(200, Value::Array(analytics::version_stats(backend, &runs, &cfg.prompts_dir(), version()))),
        p if p.starts_with("/api/runs/") => {
            let id = pct_decode(&p["/api/runs/".len()..]);
            let dir = runs.join(&id);
            let bad = id.is_empty()
                || id.contains('/')
                || id.contains('\\')
                || id == "."
                || id == ".."
                || !dir.join("summary.json").is_file();
            if bad {
                return json_resp(404, json!({"error": "no such run"}));
            }
            let summary: Value = std::fs::read_to_string(dir.join("summary.json"))
                .ok()
                .and_then(|t| serde_json::from_str(&t).ok())
                .unwrap_or(Value::Null);
            json_resp(
                200,
                json!({"summary": summary, "metrics": analytics::run_metrics(&dir), "events": analytics::load_decisions(&dir)}),
            )
        }
        _ => json_resp(404, json!({"error": "not found"})),
    }
}

fn handle(mut s: TcpStream, cfg: &PanelConfig, sh: &Sh) -> std::io::Result<()> {
    s.set_read_timeout(Some(Duration::from_secs(10)))?;
    let mut buf = Vec::new();
    let mut tmp = [0u8; 4096];
    let head_end = loop {
        if let Some(p) = buf.windows(4).position(|w| w == b"\r\n\r\n") {
            break p;
        }
        let n = s.read(&mut tmp)?;
        if n == 0 {
            return Ok(());
        }
        buf.extend_from_slice(&tmp[..n]);
    };
    let head = String::from_utf8_lossy(&buf[..head_end]).into_owned();
    let mut lines = head.split("\r\n");
    let mut rl = lines.next().unwrap_or("").split(' ');
    let (method, target) = (rl.next().unwrap_or(""), rl.next().unwrap_or("/").to_string());
    let clen: usize = lines
        .filter_map(|l| l.split_once(':'))
        .find(|(k, _)| k.eq_ignore_ascii_case("content-length"))
        .and_then(|(_, v)| v.trim().parse().ok())
        .unwrap_or(0);
    let mut body = buf[head_end + 4..].to_vec();
    while body.len() < clen {
        let n = s.read(&mut tmp)?;
        if n == 0 {
            break;
        }
        body.extend_from_slice(&tmp[..n]);
    }
    let (code, ctype, data) = match method {
        "GET" => route_get(cfg, sh, &target),
        "POST" if target.starts_with("/api/") => {
            let v: Value = if body.is_empty() { json!({}) } else { serde_json::from_slice(&body).unwrap_or(json!({})) };
            let r = command(sh, &target["/api/".len()..], &v);
            let j = match &r {
                Ok(()) => json!({"ok": true}),
                Err(e) => json!({"error": e}),
            };
            (if r.is_ok() { 200 } else { 500 }, "application/json", pycompat::json::dumps(&j).into_bytes())
        }
        _ => (404, "application/json", br#"{"error": "not found"}"#.to_vec()),
    };
    let reason = match code {
        200 => "OK",
        404 => "Not Found",
        _ => "Internal Server Error",
    };
    write!(
        s,
        "HTTP/1.1 {code} {reason}\r\nContent-Type: {ctype}\r\nContent-Length: {}\r\nCache-Control: no-store\r\nConnection: close\r\n\r\n",
        data.len()
    )?;
    s.write_all(&data)?;
    s.flush()
}

/// Start the controller thread and serve on `listener` (accept loop on its own thread). Returns at once.
pub fn start(cfg: PanelConfig, listener: TcpListener) {
    let sh: Sh = Arc::new(Mutex::new(Shared {
        mode: "paused".into(),
        pending: 0,
        delay: cfg.delay,
        cmds: VecDeque::new(),
        snap: Snap {
            auto_new_run: true,
            stats: json!({}),
            run: Value::Null,
            bench_current: Value::Null,
            bench_name: crate::benchmark::NAME.into(),
            ..Default::default()
        },
    }));
    {
        let (c, s) = (cfg.clone(), sh.clone());
        std::thread::spawn(move || worker(c, s));
    }
    std::thread::spawn(move || {
        for conn in listener.incoming().flatten() {
            let (c, s) = (cfg.clone(), sh.clone());
            std::thread::spawn(move || {
                let _ = handle(conn, &c, &s);
            });
        }
    });
}

/// The repo root (two levels above this crate): default for `--root`.
pub fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}
