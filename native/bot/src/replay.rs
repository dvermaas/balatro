//! Replay harness: `TraceGame`, `TraceJev` and `TraceOracle` serve one recorded Python game
//! (scripts/port/trace_game.py) to a `JevBot`, asserting at every step that the Rust bot asks for
//! exactly what the Python bot asked for. `replay_game` drives a whole game and reports how many
//! records matched before the first mismatch or panic.

use crate::codejev::CodeJev;
use crate::iface::{Game, Jev, JevResponse, Oracle, RpcError, SharedOracle};
use crate::jev_bot::{EventHook, JevBot, ReplayClock, Stop};
use crate::recorder::Clock;
use crate::trace;
use serde_json::{json, Map, Value};
use std::cell::RefCell;
use std::collections::VecDeque;
use std::path::Path;
use std::rc::Rc;

/// The recorded streams of one game, consumed in order per kind.
pub struct Replay {
    calls: VecDeque<Value>,
    jevs: VecDeque<Value>,
    oracles: VecDeque<Value>,
    events: VecDeque<Value>,
    /// Records consumed and found identical so far.
    pub matched: usize,
}

pub type Shared = Rc<RefCell<Replay>>;

impl Replay {
    pub fn new(records: Vec<Value>) -> Self {
        let mut r = Replay {
            calls: VecDeque::new(),
            jevs: VecDeque::new(),
            oracles: VecDeque::new(),
            events: VecDeque::new(),
            matched: 0,
        };
        for rec in records {
            match rec.get("k").and_then(Value::as_str) {
                Some("call") => r.calls.push_back(rec),
                Some("jev") => r.jevs.push_back(rec),
                Some("oracle") => r.oracles.push_back(rec),
                Some("event") => r.events.push_back(rec),
                _ => {}
            }
        }
        r
    }
    pub fn remaining(&self) -> usize {
        self.calls.len() + self.jevs.len() + self.oracles.len() + self.events.len()
    }
}

fn js(v: &Value) -> String {
    serde_json::to_string(v).unwrap()
}

fn check(what: &str, got: &Value, want: &Value) {
    if js(got) != js(want) {
        panic!("{what} mismatch\n  rust:   {}\n  python: {}", trace::short(got, 400), trace::short(want, 400));
    }
}

/// Drop timing keys (`t`, `ms`, `*_seconds`, `*_s`) at every depth.
pub fn strip_timing(v: &Value) -> Value {
    match v {
        Value::Object(m) => Value::Object(
            m.iter()
                .filter(|(k, _)| !(k.as_str() == "t" || k.as_str() == "ms" || k.ends_with("_seconds") || k.ends_with("_s")))
                .map(|(k, v)| (k.clone(), strip_timing(v)))
                .collect::<Map<_, _>>(),
        ),
        Value::Array(a) => Value::Array(a.iter().map(strip_timing).collect()),
        o => o.clone(),
    }
}

pub struct TraceGame {
    pub r: Shared,
}

impl Game for TraceGame {
    fn call(&mut self, method: &str, params: Option<&Value>) -> Result<Value, RpcError> {
        let mut r = self.r.borrow_mut();
        let rec = r.calls.pop_front().unwrap_or_else(|| panic!("trace exhausted: bot called {method}"));
        check("call method", &json!(method), &rec["method"]);
        check(&format!("call {method} params"), &params.cloned().unwrap_or(Value::Null), &rec["params"]);
        r.matched += 1;
        if let Some(e) = rec.get("err") {
            return Err(RpcError {
                code: e["code"].as_i64().unwrap_or(0),
                message: e["message"].as_str().unwrap_or("").to_string(),
                name: e["name"].as_str().map(String::from),
            });
        }
        Ok(rec["ok"].clone())
    }
    fn backend(&self) -> &str {
        "jackdaw"
    }
}

/// Checks the request against the recording and returns the recorded answers, or, with `code`, the
/// code stand-in's answers after asserting they equal the recorded ones.
pub struct TraceJev {
    pub r: Shared,
    pub code: Option<Box<dyn Jev>>,
}

impl Jev for TraceJev {
    fn observe(&mut self, last_state: &Value) {
        if let Some(c) = self.code.as_mut() {
            c.observe(last_state);
        }
    }
    fn system_one(&mut self, state: &Value, questions: &Map<String, Value>) -> Result<JevResponse, String> {
        // run the stand-in first: its oracle calls precede the "jev" record in the Python trace too
        let code_resp = match self.code.as_mut() {
            Some(c) => Some(c.system_one(state, questions)?),
            None => None,
        };
        let mut r = self.r.borrow_mut();
        let rec = r.jevs.pop_front().unwrap_or_else(|| panic!("trace exhausted: bot asked JEV"));
        check("jev state", state, &rec["state"]);
        check("jev questions", &Value::Object(questions.clone()), &rec["questions"]);
        let want = rec["answers"].as_object().cloned().unwrap_or_default();
        if let Some(c) = &code_resp {
            check("codejev answers", &Value::Object(c.answers.clone()), &Value::Object(want.clone()));
        }
        r.matched += 1;
        Ok(code_resp.unwrap_or(JevResponse {
            model: "code-top-option".into(),
            answers: want,
            input_tokens: 0,
            output_tokens: 0,
        }))
    }
}

pub struct TraceOracle {
    pub r: Shared,
}

impl TraceOracle {
    fn next(&self, f: &str, args: &Value) -> Value {
        let mut r = self.r.borrow_mut();
        let rec = r.oracles.pop_front().unwrap_or_else(|| panic!("trace exhausted: oracle {f}"));
        check("oracle fn", &json!(f), &rec["fn"]);
        let want = &rec["args"];
        // compare only the args the Python call recorded (an unset budget_s is not recorded)
        let mut got = Map::new();
        for k in crate::gs::d(want).keys() {
            got.insert(k.clone(), args.get(k).cloned().unwrap_or(Value::Null));
        }
        check(&format!("oracle {f} args"), &Value::Object(got), want);
        r.matched += 1;
        rec["ret"].clone()
    }
}

impl Oracle for TraceOracle {
    fn forecast(&mut self, _pub_gs: &Value, n: i64, bosses: i64, keys: &[String], budget_s: Option<f64>) -> Value {
        self.next("rollout.forecast", &json!({"n": n, "bosses": bosses, "keys": keys, "budget_s": budget_s}))
    }
    fn items_forecast(&mut self, _pub_gs: &Value, n: i64, bosses: i64, keys: &[String], budget_s: Option<f64>) -> Value {
        self.next("rollout_items.forecast", &json!({"n": n, "bosses": bosses, "keys": keys, "budget_s": budget_s}))
    }
    fn picker_values(&mut self, _pub_gs: &Value, args: &Value) -> Value {
        self.next("Picker.values", args)
    }
    fn pick_pub(&mut self, _pub_gs: &Value) -> Value {
        self.next("Picker.pick_pub", &json!({}))
    }
}

/// Compares every emitted event with the next recorded one (timing keys ignored).
pub fn event_hook(r: Shared) -> EventHook {
    Box::new(move |kind, data| {
        let mut r = r.borrow_mut();
        let rec = r.events.pop_front().unwrap_or_else(|| panic!("trace exhausted: bot emitted {kind}"));
        check("event kind", &json!(kind), &rec["kind"]);
        check(&format!("event {kind} data"), &strip_timing(data), &strip_timing(&rec["data"]));
        r.matched += 1;
    })
}

struct FixedClock;

impl Clock for FixedClock {
    fn time(&self) -> f64 {
        0.0
    }
    fn stamp(&self) -> String {
        "19700101-000000".into()
    }
}

#[derive(Debug)]
pub struct Outcome {
    pub matched: usize,
    pub total: usize,
    pub identical: bool,
    /// Panic message and location, or the stop reason.
    pub error: Option<String>,
}

pub static LAST_PANIC: std::sync::Mutex<String> = std::sync::Mutex::new(String::new());

/// Record the location of the last panic (call once before `replay_game`).
pub fn install_panic_hook() {
    std::panic::set_hook(Box::new(|info| {
        *LAST_PANIC.lock().unwrap() = info.location().map(|l| format!("{}:{}", l.file(), l.line())).unwrap_or_default();
    }));
}

/// Replay one trace file with `JevBot` + `CodeJev` (arm F: `items`). Run dirs go under `log_dir`.
pub fn replay_game(path: &Path, items: bool, log_dir: &Path) -> Outcome {
    let records = trace::load_trace(path);
    let start = records.iter().find(|r| r["k"] == "call").expect("trace has no calls")["params"].clone();
    let replay: Shared = Rc::new(RefCell::new(Replay::new(records)));
    let total = replay.borrow().remaining();
    let res = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let oracle: SharedOracle = Rc::new(RefCell::new(TraceOracle { r: replay.clone() }));
        let code = CodeJev::new(items, oracle.clone());
        let jev = TraceJev { r: replay.clone(), code: Some(Box::new(code)) };
        let mut bot = JevBot::new(
            "RED",
            "WHITE",
            "jev-latest",
            log_dir,
            Box::new(TraceGame { r: replay.clone() }),
            Box::new(jev),
            oracle,
            Box::new(ReplayClock),
            "replay",
            "python",
            "python",
            Box::new(FixedClock),
        );
        bot.model = "code-top-option".into();
        bot.event_hook = Some(event_hook(replay.clone()));
        bot.act("start", Some(&start)).expect("start");
        for _ in 0..20000 {
            match bot.step() {
                Ok(_) => {}
                Err(Stop::BenchmarkComplete) => break,
                Err(Stop::Runtime(m)) if m.contains("Victory reported") => break,
                Err(e) => return Some(format!("{e:?}")),
            }
            if bot.stats.runs >= 1 && bot.last_state.as_ref().map_or(false, |g| g["state"] == "MENU") {
                break;
            }
        }
        None
    }));
    let r = replay.borrow();
    let (identical, error) = match res {
        Ok(None) if r.remaining() == 0 => (true, None),
        Ok(None) => (false, Some(format!("game ended with {} unconsumed records", r.remaining()))),
        Ok(Some(e)) => (false, Some(format!("stopped: {e}"))),
        Err(p) => {
            let msg = p
                .downcast_ref::<String>()
                .cloned()
                .or_else(|| p.downcast_ref::<&str>().map(|s| s.to_string()))
                .unwrap_or_default();
            let loc = LAST_PANIC.lock().unwrap().clone();
            (false, Some(format!("panic at {loc}: {}", msg.lines().take(3).collect::<Vec<_>>().join(" | "))))
        }
    };
    Outcome { matched: r.matched, total, identical, error }
}
