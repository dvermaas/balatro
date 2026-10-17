//! Shell gaps: benchmark early stop (`clearly_worse` vs Python's `_clearly_worse`, fixture
//! scripts/port/fixtures/clearly_worse.py), JEV errors as `Stop::Jev`, local-time stamps, Rust prompt version.

use jevbot::codejev::CodeJev;
use jevbot::engine::RumbotGame;
use jevbot::iface::{Jev, JevResponse, SharedOracle};
use jevbot::jev_bot::{JevBot, RealClock, Stop};
use jevbot::oracle::RustOracle;
use jevbot::recorder::{self, Clock, SystemClock};
use serde_json::{json, Map, Value};
use std::cell::RefCell;
use std::path::{Path, PathBuf};
use std::rc::Rc;

fn tmp(name: &str) -> PathBuf {
    let p = std::env::temp_dir().join(format!("jevshell-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&p);
    std::fs::create_dir_all(&p).unwrap();
    p
}

fn bot_with(root: &Path, version: &str, jev: Option<Box<dyn Jev>>) -> JevBot {
    let oracle: SharedOracle = Rc::new(RefCell::new(RustOracle));
    let jev = jev.unwrap_or_else(|| Box::new(CodeJev::new(false, oracle.clone())));
    let mut b = JevBot::new(
        "Red Deck", "White Stake", "m", root, Box::new(RumbotGame::new()), jev, oracle, Box::new(RealClock::new()),
        version, "x", "x", Box::new(SystemClock),
    );
    b.backend = "balatrobot".into();
    b
}

#[test]
fn clearly_worse_matches_python() {
    let cases: Vec<Value> = serde_json::from_str(&std::fs::read_to_string("tests/data/clearly_worse.json").unwrap()).unwrap();
    let seeds = jevbot::benchmark::set("bench30-a").unwrap();
    for (n, c) in cases.iter().enumerate() {
        let root = tmp(&format!("cw{n}")).join("runs");
        let pd = root.join("_prompts");
        std::fs::create_dir_all(&pd).unwrap();
        let ids: Vec<&str> = c["versions"].as_array().unwrap().iter().map(|v| v["id"].as_str().unwrap()).collect();
        std::fs::write(pd.join("index.json"), json!({"order": ids}).to_string()).unwrap();
        for v in c["versions"].as_array().unwrap() {
            let id = v["id"].as_str().unwrap();
            std::fs::write(pd.join(format!("{id}.json")), json!({"version": id, "created": 1.0, "note": "n"}).to_string()).unwrap();
            for (i, p) in v["progress"].as_array().unwrap().iter().enumerate() {
                let p = p.as_i64().unwrap();
                let d = root.join(format!("20260101-{id}-{i:02}"));
                std::fs::create_dir_all(&d).unwrap();
                std::fs::write(d.join("summary.json"), json!({
                    "run_id": d.file_name().unwrap().to_str(), "seed": seeds[i].split('#').next().unwrap(), "prompt_version": id,
                    "result": "lost", "ante": 1 + p / 3, "round": p + 1, "started": 1000.0 + i as f64, "ended": 1100.0 + i as f64,
                    "input_tokens": 10, "avg_confidence": 0.5, "output_tokens": 1,
                    "benchmark": {"name": "bench30-a", "index": i, "of": seeds.len()}}).to_string()).unwrap();
                let mut lines = vec![r#"{"kind": "round_end"}"#.to_string(); p as usize];
                lines.push(json!({"kind": "decision", "question": "move", "choice": "play_1",
                                  "state_sent": {"discards_left": 1, "round": 1}}).to_string());
                std::fs::write(d.join("decisions.jsonl"), lines.join("\n") + "\n").unwrap();
            }
        }
        let mut bot = bot_with(&root, c["me"].as_str().unwrap(), None);
        bot.bench_name = c["set"].as_str().unwrap().into();
        assert_eq!(bot.clearly_worse(), c["expected"].as_str().map(String::from), "case {n}");
    }
}

#[test]
fn start_benchmark_queues_seeds() {
    let mut bot = bot_with(&tmp("sb").join("runs"), "v", None);
    bot.start_benchmark(Some("nope"));
    assert_eq!(bot.bench_name, "bench30-a");
    assert_eq!(bot.bench_queue.len(), 30);
    bot.start_benchmark(Some("gap4-a"));
    assert_eq!((bot.bench_name.as_str(), bot.bench_queue.len()), ("gap4-a", 4));
}

struct Failing;
impl Jev for Failing {
    fn system_one(&mut self, _s: &Value, _q: &Map<String, Value>) -> Result<JevResponse, String> {
        Err("HTTP 402: no usable credits".into())
    }
}

#[test]
fn jev_error_is_a_result() {
    let mut bot = bot_with(&tmp("je").join("runs"), "v", Some(Box::new(Failing)));
    let mut q = Map::new();
    q.insert("a".into(), json!({}));
    match bot.query(&json!({}), &json!({}), &q) {
        Err(Stop::Jev(m)) => assert_eq!(jevbot::panel::jev_error_text(&m), "Jev API: HTTP 402: no usable credits"),
        other => panic!("{other:?}"),
    }
}

#[test]
fn local_stamp_format() {
    let s = SystemClock.stamp();
    assert_eq!(s.len(), 15);
    assert_eq!(&s[8..9], "-");
    let (h, d): (i64, i64) = (s[9..11].parse().unwrap(), s[6..8].parse().unwrap());
    let utc = SystemClock.stamp_utc_for_test();
    let (uh, ud): (i64, i64) = (utc[9..11].parse().unwrap(), utc[6..8].parse().unwrap());
    let diff = ((d * 24 + h) - (ud * 24 + uh)).rem_euclid(24 * 31); // whole-hour (or half-hour) offsets, < 15 h
    assert!(diff <= 14 || diff >= 24 * 31 - 12, "{s} vs {utc}");
}

#[test]
fn rust_version_and_snapshots() {
    let native = tmp("ver");
    for (f, t) in [("bot/src/a.rs", "x"), ("kernel/src/lib.rs", "y"), ("other/src/z.rs", "ignored")] {
        let p = native.join(f);
        std::fs::create_dir_all(p.parent().unwrap()).unwrap();
        std::fs::write(p, t).unwrap();
    }
    assert_eq!(recorder::behaviour_sources(&native), vec!["bot/src/a.rs", "kernel/src/lib.rs"]);
    let v1 = recorder::bot_version(&native);
    assert_eq!(v1, recorder::version_hash(&["bot/src/a.rs", "kernel/src/lib.rs"], &native));
    std::fs::write(native.join("bot/src/a.rs"), "x2").unwrap();
    let v2 = recorder::bot_version(&native);
    assert_ne!(v1, v2);
    let pd = native.join("_prompts");
    recorder::snapshot_prompts(&pd, &v1, 1.0);
    recorder::snapshot_prompts(&pd, &v2, 2.0);
    recorder::snapshot_prompts(&pd, &v2, 3.0); // once per version
    let vs = jevbot::analytics::prompt_versions(&pd, &v2);
    assert_eq!(vs.len(), 2);
    assert_eq!((vs[0]["label"].as_str(), vs[1]["label"].as_str()), (Some("v1"), Some("v2")));
    assert_eq!(vs[1]["current"], true);
    assert_eq!(vs[1]["created"], 2.0);
    assert!(vs[1]["rules"].as_str().unwrap().starts_with("Balatro:"));
    assert!(vs[1]["instructions"]["blind"].is_string() && vs[1]["options"]["blind_play"].is_string());
}
