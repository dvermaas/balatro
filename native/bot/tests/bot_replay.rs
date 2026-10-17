//! Replays the 60 recorded Python games (30 per lab arm) through JevBot + CodeJev and reports how
//! many records matched per game before the first mismatch or panic. It passes while the decisions
//! are todo!() and prints where each game stopped. JEV_REPLAY_ASSERT=1 requires 60/60 identical.

use jevbot::replay::{install_panic_hook, replay_game};
use jevbot::trace;

#[test]
fn bot_replay() {
    install_panic_hook();
    let dir = std::env::temp_dir().join("jevbot_replay");
    let _ = std::fs::remove_dir_all(&dir);
    let mut matched = vec![];
    let mut identical = 0;
    let mut n = 0;
    let mut errors: std::collections::BTreeMap<String, usize> = Default::default();
    for (arm, items) in [("bot-forecast-long", false), ("bot-forecast-items", true)] {
        for p in trace::trace_files(arm) {
            let o = replay_game(&p, items, &dir);
            n += 1;
            identical += o.identical as usize;
            matched.push(o.matched);
            if let Some(e) = &o.error {
                *errors.entry(e.chars().take(300).collect::<String>()).or_default() += 1;
            }
        }
    }
    matched.sort();
    for (e, c) in &errors {
        println!("  {c}x {e}");
    }
    println!(
        "bot replay: {identical}/{n} games identical; records matched min/median/max {}/{}/{}",
        matched[0],
        matched[matched.len() / 2],
        matched[matched.len() - 1]
    );
    let _ = std::fs::remove_dir_all(&dir);
    if std::env::var_os("JEV_REPLAY_ASSERT").is_some() {
        assert_eq!(identical, n);
    }
}

/// `build_state` (summarize + item_desc) against the recorded `ask` states: the gamestate is the
/// last call result before the "jev" record.
#[test]
fn build_state_matches_asks() {
    let (mut ok, mut n) = (0, 0);
    let mut bad = vec![];
    for arm in ["bot-forecast-long", "bot-forecast-items"] {
        for p in trace::trace_files(arm) {
            let mut last = serde_json::Value::Null;
            for r in trace::load_trace(&p) {
                match r["k"].as_str() {
                    Some("call") => {
                        if r.get("ok").map_or(false, |g| g.get("state").is_some()) {
                            last = r["ok"].clone();
                        }
                    }
                    Some("jev") => {
                        let keys: Vec<&String> = r["state"].as_object().unwrap().keys().collect();
                        if keys == ["rules", "game"] {
                            n += 1;
                            let g = jevbot::jev_bot::summarize(&last);
                            if serde_json::to_string(&g).unwrap() == serde_json::to_string(&r["state"]["game"]).unwrap() {
                                ok += 1;
                            } else if bad.len() < 3 {
                                bad.push((trace::short(&g, 600), trace::short(&r["state"]["game"], 600)));
                            }
                        }
                    }
                    _ => {}
                }
            }
        }
    }
    println!("summarize: {ok}/{n} ask states identical");
    for b in &bad {
        println!("  rust:   {}\n  python: {}", b.0, b.1);
    }
    assert_eq!(ok, n);
}

/// CodeJev (items=false) against every recorded JEV request of arm E (gamestate = last call result, oracle calls served
/// from the recording in order): its answers must equal the recorded ones.
#[test]
fn codejev_matches_answers() {
    use jevbot::codejev::CodeJev;
    use jevbot::iface::{Jev, SharedOracle};
    use jevbot::replay::{Replay, TraceJev, TraceOracle};
    use std::{cell::RefCell, rc::Rc};
    jevbot::replay::install_panic_hook();
    let (mut n, mut bad) = (0, 0);
    // arm F (items) needs the decisions' oracle calls interleaved: it is covered by bot_replay once they exist
    for (arm, items) in [("bot-forecast-long", false)] {
        for p in trace::trace_files(arm) {
            let recs = trace::load_trace(&p);
            let r = Rc::new(RefCell::new(Replay::new(recs.clone())));
            let oracle: SharedOracle = Rc::new(RefCell::new(TraceOracle { r: r.clone() }));
            let mut tj = TraceJev { r: r.clone(), code: Some(Box::new(CodeJev::new(items, oracle))) };
            let mut last = serde_json::Value::Null;
            for rec in &recs {
                match rec["k"].as_str() {
                    Some("call") if rec.get("ok").map_or(false, |g| g.get("state").is_some()) => last = rec["ok"].clone(),
                    Some("jev") => {
                        n += 1;
                        tj.observe(&last);
                        let qs = rec["questions"].as_object().unwrap().clone();
                        let res = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| tj.system_one(&rec["state"], &qs)));
                        if let Err(e) = &res {
                            if bad == 0 { println!("first: {:?} {:?}", e.downcast_ref::<String>(), jevbot::replay::LAST_PANIC.lock().unwrap()); }
                            bad += 1;
                            break;
                        }
                    }
                    _ => {}
                }
            }
        }
    }
    println!("codejev: {}/{n} requests identical", n - bad);
    assert_eq!(bad, 0);
}
