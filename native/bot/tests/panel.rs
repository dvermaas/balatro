//! Panel server on the rumbot backend with CodeJev (no real game, no TypeSafe): stepping, /api/live
//! field names, and /api/runs + /api/runs/<id> identical to the Python Handler on a copy of 3 run
//! dirs (fixture: scripts/port/fixtures/panel.py -> runs/_port/fixtures/panel.json).

use jevbot::panel::{start, PanelConfig};
use serde_json::{json, Value};
use std::io::{Read, Write};
use std::net::TcpListener;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

fn http(port: u16, method: &str, path: &str, body: &str) -> (u16, Value) {
    let mut s = std::net::TcpStream::connect(("127.0.0.1", port)).unwrap();
    write!(s, "{method} {path} HTTP/1.1\r\nHost: x\r\nContent-Length: {}\r\n\r\n{body}", body.len()).unwrap();
    let mut out = String::new();
    s.read_to_string(&mut out).unwrap();
    let code = out[9..12].parse().unwrap();
    let b = out.split_once("\r\n\r\n").unwrap().1;
    (code, serde_json::from_str(b).unwrap())
}

fn serve(root: &Path) -> u16 {
    let l = TcpListener::bind(("127.0.0.1", 0)).unwrap();
    let port = l.local_addr().unwrap().port();
    let mut cfg = PanelConfig::new(root);
    cfg.backend = "rumbot".into();
    cfg.code_jev = true;
    cfg.delay = 0.0;
    start(cfg, l);
    port
}

fn tmp(name: &str) -> PathBuf {
    let p = std::env::temp_dir().join(format!("jevpanel-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&p);
    std::fs::create_dir_all(&p).unwrap();
    p
}

#[test]
fn step_and_live() {
    let root = tmp("live");
    let port = serve(&root);
    assert_eq!(http(port, "POST", "/api/settings", r#"{"delay": 0}"#).1, json!({"ok": true}));
    for _ in 0..6 {
        assert_eq!(http(port, "POST", "/api/step", "{}").0, 200);
    }
    let t0 = Instant::now();
    let live = loop {
        let (c, v) = http(port, "GET", "/api/live?since=0", "");
        assert_eq!(c, 200);
        if v["events"].as_array().map_or(0, |e| e.len()) >= 5 || t0.elapsed() > Duration::from_secs(60) {
            break v;
        }
        std::thread::sleep(Duration::from_millis(50));
    };
    for k in ["mode", "backend", "delay", "auto_new_run", "stats", "game_state", "game", "current_run", "version", "rate",
              "benchmark", "last_id", "events"] {
        assert!(live.get(k).is_some(), "missing {k}");
    }
    assert_eq!(live["backend"], "rumbot");
    assert_eq!(live["mode"], "paused");
    let ids: Vec<i64> = live["events"].as_array().unwrap().iter().map(|e| e["id"].as_i64().unwrap()).collect();
    assert!(ids.len() >= 5 && ids.windows(2).all(|w| w[0] < w[1]), "{ids:?}");
    assert_eq!(live["last_id"], *ids.last().unwrap());
    for e in live["events"].as_array().unwrap() {
        assert!(e.get("t").is_some() && e.get("kind").is_some() && e.get("state_sent").is_none());
    }
    let since = ids[1];
    let (_, v2) = http(port, "GET", &format!("/api/live?since={since}"), "");
    assert!(v2["events"].as_array().unwrap().iter().all(|e| e["id"].as_i64().unwrap() > since));
    assert_eq!(http(port, "POST", "/api/nope", "{}").0, 500);
    assert_eq!(http(port, "GET", "/api/runs/..", "").0, 404);
}

#[test]
fn runs_identical_to_python() {
    let fx = Path::new("C:/Users/admin/Documents/Repos/balatro-rust/runs/_port/fixtures/panel.json");
    let exp: Value = serde_json::from_str(&std::fs::read_to_string(fx).expect("runs/_port fixture panel (recorded from the Python v27 bot; see trace::data_dir)")).unwrap();
    let root = tmp("runs");
    let runs = root.join("runs").join("sim");
    for n in exp["dirs"].as_array().unwrap() {
        let n = n.as_str().unwrap();
        let src = Path::new("C:/Users/admin/Documents/Repos/balatro/runs").join(n);
        std::fs::create_dir_all(runs.join(n)).unwrap();
        for f in std::fs::read_dir(src).unwrap() {
            let f = f.unwrap();
            if f.path().is_file() {
                std::fs::copy(f.path(), runs.join(n).join(f.file_name())).unwrap();
            }
        }
    }
    let port = serve(&root);
    let (c, got) = http(port, "GET", "/api/runs", "");
    assert_eq!(c, 200);
    assert_eq!(got, exp["runs"]);
    for n in exp["dirs"].as_array().unwrap() {
        let n = n.as_str().unwrap();
        let (c, got) = http(port, "GET", &format!("/api/runs/{n}"), "");
        assert_eq!(c, 200);
        assert_eq!(got, exp["run"][n], "run {n}");
    }
    let (c, got) = http(port, "GET", "/api/runs/nope", "");
    assert_eq!(json!([c, got]), exp["missing"]);
}
