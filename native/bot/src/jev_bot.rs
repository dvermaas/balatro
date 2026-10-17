//! Port of src/jev_bot.py: the game loop (state -> handler -> one JEV question -> action) and its
//! plumbing (`emit`, `act`, `query`, `ask`, `record_decision`). The decision handlers live in
//! `crate::decisions`. See docs/port-handoff/jevbot.md for the interface the handlers code against.

use crate::benchmark;
use crate::forecast_stats::pystr;
use crate::gs::{self, cards_of};
use crate::hands::describe;
use crate::iface::{Game, Jev, RpcError, SharedOracle};
use crate::prompts::GAME_RULES;
use crate::recorder::{Clock, RunRecorder};
use crate::{strategy, tarots};
use pycompat::json::dumps;
use serde_json::{json, Map, Value};
use std::collections::{HashSet, VecDeque};
use std::path::PathBuf;

pub const SHOP_ACTION_CAP: i64 = 8;
pub const TRANSITIONAL_WAIT: f64 = 0.25;
pub const EARLY_STOP_AFTER: i64 = 10;
/// By half-way, trailing by more than 1 SE means it is very unlikely to end up ahead.
pub const FUTILITY_AFTER: i64 = 15;

/// `(method, params)`: what `act` sends. `None` params is Python `None` (JSON null).
pub type Action = (String, Option<Value>);
/// `{key: (description, (method, params))}` in insertion order (Python dict order matters: it is the
/// order JEV and the code stand-in see). Keys are unique; build entries with `opt()`.
pub type Options = Vec<(String, (String, Action))>;

/// One `Options` entry: `key: (desc, (method, params))`.
pub fn opt(key: &str, desc: impl Into<String>, method: &str, params: Option<Value>) -> (String, (String, Action)) {
    (key.to_string(), (desc.into(), (method.to_string(), params)))
}

/// An action as the JSON list Python's tuple becomes in events.
pub fn action_value(a: &Action) -> Value {
    json!([a.0, a.1.clone().unwrap_or(Value::Null)])
}

/// Why `step` stopped the loop (Python exceptions that are not caught inside the handler try).
#[derive(Debug)]
pub enum Stop {
    /// An `RPCError`. Inside the handler it is counted and retried by `step`; it only escapes from
    /// the gamestate read or the `menu` act of a finished run.
    Rpc(RpcError),
    /// `RuntimeError(msg)`.
    Runtime(String),
    /// `BenchmarkComplete`.
    BenchmarkComplete,
    /// `TypeSafeError`: the JEV call failed (the text carries TypeSafe's HTTP status, e.g. 402).
    Jev(String),
}

impl From<RpcError> for Stop {
    fn from(e: RpcError) -> Self {
        Stop::Rpc(e)
    }
}

/// time.time(), time.perf_counter(), time.sleep(). Replay uses zeros and never sleeps.
pub trait BotClock {
    fn time(&self) -> f64;
    fn perf(&self) -> f64;
    fn sleep(&self, secs: f64);
}

pub struct RealClock(std::time::Instant);

impl RealClock {
    pub fn new() -> Self {
        RealClock(std::time::Instant::now())
    }
}

impl Default for RealClock {
    fn default() -> Self {
        Self::new()
    }
}

impl BotClock for RealClock {
    fn time(&self) -> f64 {
        std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs_f64()).unwrap_or(0.0)
    }
    fn perf(&self) -> f64 {
        self.0.elapsed().as_secs_f64()
    }
    fn sleep(&self, secs: f64) {
        std::thread::sleep(std::time::Duration::from_secs_f64(secs));
    }
}

pub struct ReplayClock;

impl BotClock for ReplayClock {
    fn time(&self) -> f64 {
        0.0
    }
    fn perf(&self) -> f64 {
        0.0
    }
    fn sleep(&self, _secs: f64) {}
}

// ---- question builders (SDK `model_dump()` shape, key order as in the traces) -------------------

/// `Choice(instructions=, criteria={key: desc})` from ordered options.
pub fn choice_q(instructions: &str, criteria: &Options) -> Value {
    let crit: Map<String, Value> = criteria.iter().map(|(k, v)| (k.clone(), json!(v.0))).collect();
    choice_q_map(instructions, crit)
}

/// `Choice(...)` for criteria not built from `Options`.
pub fn choice_q_map(instructions: &str, criteria: Map<String, Value>) -> Value {
    json!({"type": "choice", "instructions": instructions, "criteria": Value::Object(criteria)})
}

/// `Score(instructions=<str or dict>, criteria=<list of level strings>)` (decisions pass `ITEM_VALUE_LEVELS`).
pub fn score_q(instructions: Value, criteria: Value) -> Value {
    json!({"type": "score", "instructions": instructions, "criteria": criteria})
}

/// `Noul(instructions=, criteria={"true": .., "false": ..})`.
pub fn noul_q(instructions: &str, criteria: Value) -> Value {
    json!({"type": "noul", "instructions": instructions, "criteria": criteria})
}

// ---- helpers ---------------------------------------------------------------------------------

/// Python `(a or 0) - (b or 0)`: int when both are ints, else float.
fn sub_or0(a: &Value, b: &Value) -> Value {
    let (a, b) = (if gs::truthy(a) { a.clone() } else { json!(0) }, if gs::truthy(b) { b.clone() } else { json!(0) });
    match (a.as_i64(), b.as_i64()) {
        (Some(x), Some(y)) => json!(x - y),
        _ => json!(gs::num(&a) - gs::num(&b)),
    }
}

// jev_bot.py 49-72. `mains` is None when the caller passes no main hands.
pub fn item_desc(c: &Value, mains: Option<&[String]>) -> String {
    let v = gs::get(c, "value");
    let m = gs::get(c, "modifier");
    let set = gs::get(c, "set").as_str();
    let mut text;
    if matches!(set, Some("DEFAULT") | Some("ENHANCED")) {
        text = describe(c);
    } else {
        let label = gs::get(c, "label");
        text = gs::s(if gs::truthy(label) { label } else { gs::get(c, "key") }).to_string();
        if gs::truthy(gs::get(v, "effect")) {
            text += &format!(": {}", pystr(gs::get(v, "effect")));
        }
    }
    if gs::truthy(gs::get(m, "edition")) {
        text += &format!(" [{}]", gs::s(gs::get(m, "edition")).to_lowercase());
    }
    for flag in ["eternal", "rental"] {
        if gs::truthy(gs::get(m, flag)) {
            text += &format!(" [{flag}]");
        }
    }
    if gs::truthy(gs::get(m, "perishable")) {
        text += " [perishable]";
    }
    if let (Some(mains), Some("JOKER")) = (mains, set) {
        text += &format!(" [{}]", strategy::joker_tags(gs::get(v, "effect"), mains).join("; "));
    } else if let (Some(mains), Some("PLANET")) = (mains, set) {
        if !mains.is_empty() {
            let eff = gs::s(gs::get(v, "effect"));
            let hit = mains.iter().find(|h| eff.contains(&format!("Level up {h}")));
            if let Some(h) = hit {
                text += &format!(" [levels {h}, a main hand]");
            } else {
                text += " [levels a hand we rarely play]";
            }
        }
    }
    text
}

// jev_bot.py 75-79. `{}` when there is none.
pub fn current_blind(gs_: &Value) -> Value {
    for b in gs::d(gs::get(gs_, "blinds")).values() {
        if matches!(gs::get(b, "status").as_str(), Some("CURRENT") | Some("SELECT")) {
            return b.clone();
        }
    }
    json!({})
}

// jev_bot.py 82-120
pub fn summarize(gs_: &Value) -> Value {
    let rnd = gs::get(gs_, "round");
    let blind = current_blind(gs_);
    let mut hs: Vec<(&String, &Value)> = gs::d(gs::get(gs_, "hands")).iter().collect();
    hs.sort_by(|a, b| gs::num(gs::get(a.1, "order")).partial_cmp(&gs::num(gs::get(b.1, "order"))).unwrap());
    let mut levels = Map::new();
    for (name, h) in hs {
        levels.insert(
            name.clone(),
            json!(format!(
                "level {}: {} chips x {} mult",
                pystr(gs::get(h, "level")),
                pystr(gs::get(h, "chips")),
                pystr(gs::get(h, "mult"))
            )),
        );
    }
    let (jokers, cons) = (gs::area(gs_, "jokers"), gs::area(gs_, "consumables"));
    let diff = sub_or0(gs::get(&blind, "score"), gs::get(rnd, "chips"));
    let need = if gs::num(&diff) > 0.0 { diff } else { json!(0) };
    let hands_left = gs::get(rnd, "hands_left");
    let mains = strategy::main_hands(gs_, 2);
    let effects: Vec<Value> =
        cards_of(gs_, "jokers").iter().map(|c| gs::get(gs::get(c, "value"), "effect").clone()).collect();
    let cnt = |m: &Map<String, Value>, k: &str, d: i64| m.get(k).map(pystr).unwrap_or_else(|| d.to_string());
    let effect = gs::get(&blind, "effect");
    json!({
        "ante": gs::get(gs_, "ante_num"),
        "round": gs::get(gs_, "round_num"),
        "money": gs::get(gs_, "money"),
        "interest_per_round": strategy::interest(gs::get(gs_, "money")),
        "engine": strategy::engine(&effects),
        "main_hands": if mains.is_empty() { json!("none yet") } else { json!(mains) },
        "blind": {
            "name": gs::get(&blind, "name"),
            "effect": if gs::truthy(effect) { effect.clone() } else { json!("none") },
            "chip_target": gs::get(&blind, "score"),
        },
        "chips_scored_this_round": gs::get(rnd, "chips"),
        "chips_still_needed": need,
        "chips_needed_per_remaining_hand": if gs::truthy(hands_left) {
            json!(pycompat::pyround_int(gs::num(&need) / gs::num(hands_left)))
        } else { Value::Null },
        "hands_left": gs::get(rnd, "hands_left"),
        "discards_left": gs::get(rnd, "discards_left"),
        "jokers": cards_of(gs_, "jokers").iter().map(|c| item_desc(c, Some(&mains))).collect::<Vec<_>>(),
        "joker_slots": format!("{}/{}", cnt(jokers, "count", 0), cnt(jokers, "limit", 5)),
        "consumables": cards_of(gs_, "consumables").iter().map(|c| item_desc(c, None)).collect::<Vec<_>>(),
        "consumable_slots": format!("{}/{}", cnt(cons, "count", 0), cnt(cons, "limit", 2)),
        "hand_levels": Value::Object(levels),
        "cards_in_hand": cards_of(gs_, "hand").iter().map(describe).collect::<Vec<_>>(),
    })
}

/// `self.stats`.
#[derive(Clone, Debug, Default)]
pub struct Stats {
    pub decisions: i64,
    pub input_tokens: i64,
    pub output_tokens: i64,
    pub jev_ms: i64,
    pub runs: i64,
    pub wins: i64,
    pub best_ante: i64,
    pub errors: i64,
}

/// Called with (kind, data) for every emitted event, before it is recorded (replay comparison).
pub type EventHook = Box<dyn FnMut(&str, &Value)>;

pub struct JevBot {
    pub jev: Box<dyn Jev>,
    pub game: Box<dyn Game>,
    /// Forecast/Picker calls (the code stand-in shares it).
    pub oracle: SharedOracle,
    pub clock: Box<dyn BotClock>,
    pub backend: String,
    pub error_backoff: f64,
    pub deck: String,
    pub stake: String,
    pub model: String,
    pub recorder: RunRecorder,
    pub auto_new_run: bool,
    pub events: VecDeque<Value>,
    pub _event_id: i64,
    pub _last_pack_cost: Value,
    pub bench_queue: Vec<(usize, String)>,
    /// Python None (Null) or {"name","index","of"}.
    pub bench_current: Value,
    pub bench_done_pending: bool,
    pub bench_name: String,
    /// Where the prompt-version snapshots live (`runs/_prompts`, recorder.PROMPTS_DIR).
    pub prompts_dir: PathBuf,
    pub _auto_failed: HashSet<String>,
    /// Python None (Null) or the blind dict.
    pub _round_blind: Value,
    pub _round_had_bones: bool,
    pub _error_streak: i64,
    /// Python None (Null) or the seed.
    pub _finished_seed: Value,
    pub stats: Stats,
    pub last_state: Option<Value>,
    // decisions.py / tarots.py attributes (set lazily in Python; initialised to Python's fallbacks)
    /// `_shop_key`: None until the first shop; compared with `==`.
    pub _shop_key: Value,
    pub _shop_actions: i64,
    /// `json.dumps(action)` strings.
    pub _failed: HashSet<String>,
    pub _rerolls: i64,
    /// `_recent`: list of dicts; `_recent_seed` None (Null) or the seed.
    pub _recent: Vec<Value>,
    pub _recent_seed: Value,
    /// `(sig, forecast)` or None.
    pub _forecast_cache: Option<(Value, Value)>,
    pub _item_forecast_cache: Option<(Value, Value)>,
    pub auto_actions: bool,
    /// decisions.py module constants the lab overrides (FORECAST_N etc.).
    pub dec: crate::decisions::DecisionCfg,
    pub event_hook: Option<EventHook>,
}

impl JevBot {
    /// `JevBot(deck, stake, model, log_dir, game, jev)`. `version` is PROMPT_VERSION (the recorder
    /// stores it); `rec_clock` is the recorder's wall clock.
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        deck: &str, stake: &str, model: &str, log_dir: impl Into<PathBuf>, game: Box<dyn Game>, jev: Box<dyn Jev>,
        oracle: SharedOracle, clock: Box<dyn BotClock>, version: &str, scorer: &str, rollouts: &str,
        rec_clock: Box<dyn Clock>,
    ) -> Self {
        let backend = game.backend().to_string();
        let error_backoff = if backend == "balatrobot" { 0.5 } else { 0.0 };
        let log_dir: PathBuf = log_dir.into();
        // recorder.PROMPTS_DIR is runs/_prompts even when the sim logs under runs/sim.
        let prompts_dir = match log_dir.file_name().and_then(|n| n.to_str()) {
            Some("sim") => log_dir.parent().unwrap_or(&log_dir).join("_prompts"),
            _ => log_dir.join("_prompts"),
        };
        let recorder = RunRecorder::new(log_dir, &backend, version, scorer, rollouts, rec_clock);
        JevBot {
            jev, game, oracle, clock, backend, error_backoff,
            deck: deck.into(), stake: stake.into(), model: model.into(), recorder,
            auto_new_run: true, events: VecDeque::new(), _event_id: 0, _last_pack_cost: json!(0),
            bench_queue: vec![], bench_current: Value::Null, bench_done_pending: false,
            bench_name: String::new(), prompts_dir, _auto_failed: HashSet::new(), _round_blind: Value::Null,
            _round_had_bones: false, _error_streak: 0, _finished_seed: Value::Null,
            stats: Stats::default(), last_state: None,
            _shop_key: Value::Null, _shop_actions: 0, _failed: HashSet::new(), _rerolls: 0,
            _recent: vec![], _recent_seed: Value::Null, _forecast_cache: None, _item_forecast_cache: None,
            auto_actions: true, dec: crate::decisions::DecisionCfg::production(), event_hook: None,
        }
    }

    // ---- plumbing -------------------------------------------------------

    /// `emit(kind, **data)`: `data` is a JSON object whose key order is the Python kwargs order.
    pub fn emit(&mut self, kind: &str, data: Value) {
        if let Some(h) = self.event_hook.as_mut() {
            h(kind, &data);
        }
        self._event_id += 1;
        let mut ev = Map::new();
        ev.insert("id".into(), json!(self._event_id));
        ev.insert("t".into(), json!(self.clock.time()));
        ev.insert("kind".into(), json!(kind));
        if let Value::Object(m) = data {
            for (k, v) in m {
                ev.insert(k, v);
            }
        }
        let ev = Value::Object(ev);
        self.recorder.write(&ev);
        self.events.push_back(ev);
        if self.events.len() > 400 {
            self.events.pop_front();
        }
    }

    /// `act(method, params)`: emits "action", calls the game, emits "outcome", updates `last_state`.
    pub fn act(&mut self, method: &str, params: Option<&Value>) -> Result<Value, RpcError> {
        self.emit("action", json!({"method": method, "params": params.cloned().unwrap_or(Value::Null)}));
        let before = self.last_state.clone().unwrap_or_else(|| json!({}));
        let result = self.game.call(method, params)?;
        if result.is_object() && result.get("state").is_some() {
            let mut snap = Self::_snapshot(&result, &before);
            if method != "play" {
                snap.remove("chips_delta"); // only a played hand scores chips
            }
            let mut data = Map::new();
            data.insert("method".into(), json!(method));
            data.extend(snap);
            self.emit("outcome", Value::Object(data));
            self.last_state = Some(result.clone());
        }
        Ok(result)
    }

    /// Compact after-action facts: lets analytics measure real hand scores, spending and progress.
    pub fn _snapshot(gs_: &Value, before: &Value) -> Map<String, Value> {
        let (rnd, prev) = (gs::get(gs_, "round"), gs::get(before, "round"));
        let mut out = Map::new();
        out.insert("state".into(), gs::get(gs_, "state").clone());
        out.insert("ante".into(), gs::get(gs_, "ante_num").clone());
        out.insert("round".into(), gs::get(gs_, "round_num").clone());
        out.insert("money".into(), gs::get(gs_, "money").clone());
        out.insert("money_delta".into(), sub_or0(gs::get(gs_, "money"), gs::get(before, "money")));
        out.insert("chips".into(), gs::get(rnd, "chips").clone());
        out.insert("hands_left".into(), gs::get(rnd, "hands_left").clone());
        out.insert("discards_left".into(), gs::get(rnd, "discards_left").clone());
        out.insert(
            "jokers".into(),
            json!(cards_of(gs_, "jokers").iter().map(|c| gs::get(c, "label").clone()).collect::<Vec<_>>()),
        );
        if gs::get(before, "round_num") == gs::get(gs_, "round_num") && !gs::get(prev, "chips").is_null() {
            out.insert("chips_delta".into(), sub_or0(gs::get(rnd, "chips"), gs::get(prev, "chips")));
        }
        out
    }

    /// `query(gs, state, questions)`: one JEV call with several typed questions. Returns the answers
    /// (SDK `model_dump()` dicts by question id) and `meta` for `record_decision`. A JEV error is `Stop::Jev`
    /// (Python lets the TypeSafeError propagate; the panel pauses with "Jev API: ...").
    pub fn query(&mut self, gs_: &Value, state: &Value, questions: &Map<String, Value>) -> Result<(Map<String, Value>, Value), Stop> {
        self.recorder.write_raw(self._event_id + 1, gs_);
        let t0 = self.clock.perf();
        self.jev.observe(self.last_state.as_ref().unwrap_or(&Value::Null));
        let resp = self.jev.system_one(state, questions).map_err(Stop::Jev)?;
        let ms = ((self.clock.perf() - t0) * 1000.0) as i64;
        self.stats.decisions += 1;
        self.stats.jev_ms += ms;
        self.stats.input_tokens += resp.input_tokens;
        self.stats.output_tokens += resp.output_tokens;
        let meta = json!({"state_sent": state, "ms": ms, "input_tokens": resp.input_tokens,
                          "output_tokens": resp.output_tokens, "model": resp.model,
                          "questions": Value::Object(questions.clone()), "answers": Value::Object(resp.answers.clone())});
        Ok((resp.answers, meta))
    }

    /// Log a combined decision in the same shape as `ask()` (`meta` from `query`, spliced last).
    #[allow(clippy::too_many_arguments)]
    pub fn record_decision(
        &mut self, question: &str, gs_: &Value, choice: &Value, action: &Value, options: &Value, probs: &Value,
        confidence: &Value, meta: &Value, display: &str,
    ) {
        let mut data = Map::new();
        data.insert("question".into(), json!(question));
        data.insert("game_state".into(), gs::get(gs_, "state").clone());
        data.insert("ante".into(), gs::get(gs_, "ante_num").clone());
        data.insert("round".into(), gs::get(gs_, "round_num").clone());
        data.insert("choice".into(), choice.clone());
        data.insert("action".into(), action.clone());
        data.insert("confidence".into(), confidence.clone());
        data.insert("probs".into(), probs.clone());
        data.insert("options".into(), options.clone());
        data.insert("display".into(), json!(display));
        data.extend(gs::d(meta).clone());
        self.emit("decision", Value::Object(data));
    }

    pub fn build_state(&self, gs_: &Value) -> Value {
        json!({"rules": GAME_RULES, "game": summarize(gs_)})
    }

    /// One `Choice` over code-built options; returns the chosen action. A single option skips JEV
    /// (emits "forced"). Use: `let (m, p) = bot.ask(..); bot.act(&m, p.as_ref())?`.
    pub fn ask(&mut self, qid: &str, gs_: &Value, instructions: &str, options: &Options) -> Result<Action, Stop> {
        if options.len() == 1 {
            let (k, (desc, action)) = &options[0];
            self.emit("forced", json!({"question": qid, "choice": k, "desc": desc}));
            return Ok(action.clone());
        }
        let state = self.build_state(gs_);
        self.recorder.write_raw(self._event_id + 1, gs_); // id the decision event below will get
        let t0 = self.clock.perf();
        let mut questions = Map::new();
        questions.insert(qid.to_string(), choice_q(instructions, options));
        self.jev.observe(self.last_state.as_ref().unwrap_or(&Value::Null));
        let resp = self.jev.system_one(&state, &questions).map_err(Stop::Jev)?;
        let ms = ((self.clock.perf() - t0) * 1000.0) as i64;
        let ans = resp.answers.get(qid).cloned().unwrap_or(Value::Null);
        self.stats.decisions += 1;
        self.stats.jev_ms += ms;
        self.stats.input_tokens += resp.input_tokens;
        self.stats.output_tokens += resp.output_tokens;
        let choice = gs::s(gs::get(&ans, "choice")).to_string();
        let action = options.iter().find(|o| o.0 == choice).unwrap_or_else(|| panic!("KeyError {choice}")).1 .1.clone();
        let opts: Map<String, Value> = options.iter().map(|(k, v)| (k.clone(), json!(v.0))).collect();
        self.emit(
            "decision",
            json!({"question": qid, "game_state": gs::get(gs_, "state"), "ante": gs::get(gs_, "ante_num"),
                   "round": gs::get(gs_, "round_num"), "choice": choice, "action": action_value(&action),
                   "confidence": gs::get(&ans, "confidence"), "probs": gs::get(&ans, "probabilities"),
                   "options": Value::Object(opts), "instructions": instructions, "state_sent": state,
                   "model": resp.model, "ms": ms, "input_tokens": resp.input_tokens,
                   "output_tokens": resp.output_tokens}),
        );
        Ok(action)
    }

    // ---- one step -------------------------------------------------------

    /// `step()`: Ok(true) when something was done, Ok(false) in a transitional state.
    pub fn step(&mut self) -> Result<bool, Stop> {
        let gs_ = self.game.call("gamestate", None)?;
        self.last_state = Some(gs_.clone());
        let st = gs::get(&gs_, "state").as_str().map(String::from);
        if gs::truthy(&self._finished_seed) {
            if st.as_deref() == Some("MENU") {
                self._finished_seed = Value::Null;
            } else if gs::get(&gs_, "seed") == &self._finished_seed {
                self.act("menu", None)?;
                return Ok(true);
            } else {
                return Err(Stop::Runtime("Unexpected game while returning a finished run to the menu".into()));
            }
        }
        let (deck, stake) = (json!(self.deck), json!(self.stake));
        self.recorder.ensure(&gs_, &deck, &stake, &self.model, &self.bench_current);
        if st.as_deref() == Some("SELECTING_HAND") {
            self._round_blind = current_blind(&gs_);
            self._round_had_bones =
                cards_of(&gs_, "jokers").iter().any(|c| gs::get(c, "label").as_str() == Some("Mr. Bones"));
        }
        self.recorder.progress(&gs_);
        let handler = st.as_deref().map(|s| s.to_lowercase()).unwrap_or_default();
        if !matches!(
            handler.as_str(),
            "blind_select" | "selecting_hand" | "shop" | "smods_booster_opened" | "menu" | "game_over" | "round_eval"
        ) {
            self.clock.sleep(TRANSITIONAL_WAIT); // animation / transitional state
            return Ok(false);
        }
        match self.step_handler(&gs_, st.as_deref(), &handler) {
            Ok(()) => self._error_streak = 0,
            Err(Stop::Rpc(e)) => {
                self.stats.errors += 1;
                self._error_streak += 1;
                let st_s = st.as_deref().unwrap_or("None");
                self.emit("error", json!({"msg": format!("{st_s}: {e}")}));
                if self._error_streak >= 5 {
                    self._error_streak = 0;
                    return Err(Stop::Runtime(format!("5 game errors in a row in {st_s}; pausing")));
                }
                self.clock.sleep(self.error_backoff);
            }
            Err(other) => return Err(other),
        }
        Ok(true)
    }

    fn step_handler(&mut self, gs_: &Value, st: Option<&str>, handler: &str) -> Result<(), Stop> {
        let auto: Option<(String, Value, String)> = if self.auto_actions {
            strategy::auto_action(gs_).or_else(|| tarots::auto_use(gs_).map(|(m, p, r)| (m.to_string(), p, r)))
        } else {
            None
        };
        if let Some((method, params, reason)) = &auto {
            let key = dumps(&json!([method, params, gs::get(gs_, "round_num"), st]));
            if !self._auto_failed.contains(&key) {
                self.emit("auto", json!({"msg": reason}));
                if let Err(e) = self.act(method, Some(params)) {
                    self._auto_failed.insert(key);
                    return Err(e.into());
                }
                return Ok(());
            }
        }
        match handler {
            "blind_select" => Ok(crate::decisions::blind_select(self, gs_)?),
            "selecting_hand" => Ok(crate::decisions::selecting_hand(self, gs_)?),
            "shop" => Ok(crate::decisions::shop(self, gs_)?),
            "smods_booster_opened" => Ok(crate::decisions::booster(self, gs_)?),
            "menu" => self.on_menu(gs_),
            "game_over" => self.on_game_over(gs_),
            "round_eval" => self.on_round_eval(gs_),
            _ => unreachable!(),
        }
    }

    /// `start_benchmark(name)`: queue every seed of the set (an unknown name falls back to the default).
    pub fn start_benchmark(&mut self, name: Option<&str>) {
        self.bench_name = match name {
            Some(n) if benchmark::set(n).is_some() => n.to_string(),
            _ => benchmark::NAME.to_string(),
        };
        let seeds = benchmark::set(&self.bench_name).expect("known set");
        self.bench_queue = seeds.iter().enumerate().map(|(i, s)| (i, s.to_string())).collect();
        self.emit("info", json!({"msg": format!("Benchmark {}: {} seeded runs queued", self.bench_name, self.bench_queue.len())}));
    }

    /// `_clearly_worse()`: after 10 benchmark seeds, compare seed by seed with the best finished version:
    /// the message when the current version trails by more than 2 standard errors (or, from 15 seeds,
    /// by more than 1), else None.
    pub fn clearly_worse(&self) -> Option<String> {
        let name = if self.bench_name.is_empty() { benchmark::NAME } else { self.bench_name.as_str() };
        let m = crate::analytics::benchmark_matrix(
            Some(name), &self.backend, self.recorder.root(), &self.prompts_dir, self.recorder.version(),
        );
        let versions = m["versions"].as_array()?;
        let me = versions.iter().find(|v| v["version"] == self.recorder.version())?;
        let done = |v: &Value| v["done"] == v["of"];
        let finished: Vec<&Value> = versions.iter().filter(|v| v["version"] != me["version"] && done(v)).collect();
        if me["done"].as_i64().unwrap_or(0) < EARLY_STOP_AFTER || finished.is_empty() {
            return None;
        }
        let mut base = finished[0];
        for v in &finished[1..] {
            if v["avg_progress"].as_f64().unwrap_or(0.0) > base["avg_progress"].as_f64().unwrap_or(0.0) {
                base = v;
            }
        }
        let d: Vec<f64> = me["cells"]
            .as_object()?
            .iter()
            .filter_map(|(s, c)| Some(c["progress"].as_f64()? - base["cells"].get(s)?["progress"].as_f64()?))
            .collect();
        if (d.len() as i64) < EARLY_STOP_AFTER {
            return None;
        }
        let mean = pycompat::statistics::mean_f64(&d);
        let se = pycompat::statistics::stdev_f64(&d) / (d.len() as f64).sqrt();
        if mean < -2.0 * se || (d.len() as i64 >= FUTILITY_AFTER && mean < -se) {
            let (a, b) = (me["label"].as_str().unwrap_or(""), base["label"].as_str().unwrap_or(""));
            return Some(format!("{a} trails {b} by {:.2} ± {:.2} blinds over {} seeds", mean.abs(), se, d.len()));
        }
        None
    }

    pub fn on_menu(&mut self, _gs: &Value) -> Result<(), Stop> {
        let mut params = json!({"deck": self.deck, "stake": self.stake});
        self.bench_current = Value::Null;
        if !self.bench_queue.is_empty() && self.clearly_worse().is_some() {
            let why = self.clearly_worse().unwrap_or_default();
            self.emit("info", json!({"msg": format!("Benchmark stopped early: {why}")}));
            self.bench_queue.clear();
            self.bench_done_pending = false;
            return Err(Stop::BenchmarkComplete);
        }
        if !self.bench_queue.is_empty() {
            let (idx, seed) = self.bench_queue.remove(0);
            let n = benchmark::set(&self.bench_name).map_or(0, |s| s.len());
            // "SEED#n" labels a repeat of SEED
            params["seed"] = json!(seed.split('#').next().unwrap_or(""));
            self.bench_current = json!({"name": self.bench_name, "index": idx, "of": n});
            self.emit("info", json!({"msg": format!("Benchmark run {}/{} (seed {})", idx + 1, n, seed)}));
        } else if self.bench_done_pending {
            self.bench_done_pending = false;
            self.emit("info", json!({"msg": format!("Benchmark {} complete", self.bench_name)}));
            return Err(Stop::BenchmarkComplete);
        } else if !self.auto_new_run {
            self.clock.sleep(0.5);
            return Ok(());
        } else {
            self.emit("info", json!({"msg": format!("Starting new run ({} deck, {} stake)", self.deck, self.stake)}));
        }
        self.bench_done_pending = gs::truthy(&self.bench_current) && self.bench_queue.is_empty();
        self.act("start", Some(&params))?;
        Ok(())
    }

    pub fn _finish_run(&mut self, gs_: &Value, won: bool) -> Result<(), Stop> {
        if &self._finished_seed != gs::get(gs_, "seed") {
            self._finished_seed = gs::get(gs_, "seed").clone();
            self.stats.runs += 1;
            self.stats.wins += won as i64;
            self.stats.best_ante = self.stats.best_ante.max(gs::int(gs::get(gs_, "ante_num")));
            self.emit(
                "run_end",
                json!({"won": won, "reported_won": gs::truthy(gs::get(gs_, "won")),
                       "ante": gs::get(gs_, "ante_num"), "round": gs::get(gs_, "round_num")}),
            );
            self.recorder.finish(if won { "won" } else { "lost" }, Some(gs_));
        }
        // menu clears the victory overlay and queued events. Do not cash out,
        // buy from a shop, or play endless rounds after a benchmark victory.
        self.act("menu", None)?;
        Ok(())
    }

    pub fn on_game_over(&mut self, gs_: &Value) -> Result<(), Stop> {
        // The installed game sets won=True even on an ante-8 boss LOSS. Only
        // a prior advance past ante 8 proves an old endless-mode run was won.
        // New runs finish at the successful ROUND_EVAL below.
        let won = gs::truthy(gs::get(gs_, "won")) && gs::int(gs::get(gs_, "ante_num")) > 8;
        self._finish_run(gs_, won)
    }

    pub fn on_round_eval(&mut self, gs_: &Value) -> Result<(), Stop> {
        let rnd = gs::get(gs_, "round");
        let blind = self._round_blind.clone(); // already marked defeated here
        let jokers: Vec<Value> = cards_of(gs_, "jokers").iter().map(|c| gs::get(c, "label").clone()).collect();
        self.emit(
            "round_end",
            json!({"ante": gs::get(gs_, "ante_num"), "round": gs::get(gs_, "round_num"), "blind": gs::get(&blind, "name"),
                   "target": gs::get(&blind, "score"), "scored": gs::get(rnd, "chips"),
                   "hands_left": gs::get(rnd, "hands_left"), "discards_left": gs::get(rnd, "discards_left"),
                   "money": gs::get(gs_, "money"), "jokers": jokers}),
        );
        if gs::truthy(gs::get(gs_, "won")) && gs::int(gs::get(gs_, "ante_num")) > 8 {
            let target = gs::num(gs::get(&blind, "score"));
            // Mr. Bones saving the ante-8 boss is a win in the real game: end_round sets game_won
            // whenever that round ends without a game over (state_events.lua). The save consumes him.
            let bones_save = self._round_had_bones
                && !cards_of(gs_, "jokers").iter().any(|c| gs::get(c, "label").as_str() == Some("Mr. Bones"));
            if gs::get(&blind, "type").as_str() != Some("BOSS")
                || target <= 0.0
                || (gs::num(gs::get(rnd, "chips")) < target && !bones_save)
            {
                return Err(Stop::Runtime("Victory reported without a verified final-boss clear; pausing for review".into()));
            }
            // Preserve the actual winning state even though there is no next Jev question.
            self.recorder.write_raw(self._event_id + 1, gs_);
            return self._finish_run(gs_, true);
        }
        self.act("cash_out", None)?;
        Ok(())
    }
}
