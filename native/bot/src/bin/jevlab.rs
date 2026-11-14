//! Rust `scripts/policy_lab.py` for arms E (`bot-forecast-long`) and F (`bot-forecast-items`, `--arm F`): the real bot (JevBot + decisions)
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

static ARM_F: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

/// The lab arm name (policy_lab.BOT_ARMS key).
fn arm() -> &'static str {
    if ARM_F.load(Ordering::SeqCst) { "bot-forecast-items" } else { "bot-forecast-long" }
}

fn seed_set(name: &str) -> Option<Vec<String>> {
    jevbot::benchmark::set(name).map(|s| s.iter().map(|x| x.to_string()).collect())
}

fn num(v: &Value) -> f64 {
    v.as_f64().unwrap_or(0.0)
}
fn r1(x: f64) -> f64 {
    (x * 10.0).round() / 10.0
}

/// Wraps the game: records every call as a trace "call" line (method, params, ok/err) into `log`.
struct RecGame {
    inner: Box<dyn jevbot::iface::Game>,
    log: Rc<RefCell<Vec<Value>>>,
    /// record the calls (only needed for JEVLAB_TRACE)
    rec: bool,
}
impl jevbot::iface::Game for RecGame {
    fn call(&mut self, method: &str, params: Option<&Value>) -> Result<Value, jevbot::iface::RpcError> {
        let r = jevbot::prof::time(if method == "gamestate" { "game.gamestate" } else { "game.act" }, || self.inner.call(method, params));
        if !self.rec {
            return r;
        }
        let mut rec = json!({"k": "call", "method": method, "params": params.cloned().unwrap_or(Value::Null)});
        match &r {
            Ok(v) => rec["ok"] = v.clone(),
            Err(e) => rec["err"] = json!({"code": e.code, "message": e.message, "name": e.name}),
        }
        self.log.borrow_mut().push(rec);
        r
    }
    fn backend(&self) -> &str {
        self.inner.backend()
    }
}

/// The rumbot game, shared between the bot (through `RecGame`) and the lab's RL driver.
struct SharedGame(Rc<RefCell<RumbotGame>>);
impl jevbot::iface::Game for SharedGame {
    fn call(&mut self, method: &str, params: Option<&Value>) -> Result<Value, jevbot::iface::RpcError> {
        self.0.borrow_mut().call(method, params)
    }
    fn backend(&self) -> &str {
        jevbot::engine::BACKEND_NAME
    }
}

/// JEVLAB_RLPOLICY=<weights.brlw>: the network (rlcore::net) takes every decision outside a round; JEVLAB_RLTEMP=T
/// samples at temperature T (argmax without it), JEVLAB_RLSEED=K seeds the sampling.
static RL_NET: std::sync::OnceLock<Option<rlcore::net::Net>> = std::sync::OnceLock::new();
fn rl_net() -> Option<&'static rlcore::net::Net> {
    RL_NET
        .get_or_init(|| std::env::var_os("JEVLAB_RLPOLICY").map(|p| rlcore::net::Net::load(std::path::Path::new(&p)).unwrap_or_else(|e| panic!("JEVLAB_RLPOLICY: {e}"))))
        .as_ref()
}

struct RlDriver {
    game: Rc<RefCell<RumbotGame>>,
    net: &'static rlcore::net::Net,
    temp: Option<f32>,
    rng: u64,
    enc: rlcore::obs::Encoder,
    decisions: u64,
    reorders: u64,
    targets_planned: u64,
    targets_fallback: u64,
    /// schema v2: a joker swap is stepped as its sale, then (the next shop decision) this buy
    pending_buy: Option<(String, Value)>,
    swaps: u64,
    /// the JEVLAB_RLLOG logger, told which sales are swaps (rlcore::log::GameLog::explicit)
    log: Option<Rc<RefCell<rlcore::log::GameLog>>>,
}

/// JEVLAB_RLORDER: "v77" (default) runs v77's shop order rule below at every shop decision; "dagger" keeps only
/// its Ceremonial Dagger rule (RL speed: the search was half of config (b)'s CPU).
fn rl_order_dagger_only() -> bool {
    static V: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    *V.get_or_init(|| match std::env::var("JEVLAB_RLORDER").as_deref() {
        Ok("dagger") => true,
        Ok("v77") | Err(_) => false,
        Ok(x) => panic!("JEVLAB_RLORDER={x}: v77 | dagger"),
    })
}

/// v77's joker order in the shop (strategy::auto_action's SHOP branch): Ceremonial Dagger last, then the searched
/// order (readiness::best_joker_order) when it keeps the Dagger last.
fn v77_shop_order(gs_: &Value) -> Option<Vec<usize>> {
    let jokers = jevbot::gs::cards_of(gs_, "jokers");
    let labels: Vec<&str> = jokers.iter().map(|j| jevbot::gs::get(j, "label").as_str().unwrap_or("")).collect();
    let ident: Vec<usize> = (0..jokers.len()).collect();
    let dl = jevbot::strategy::dagger_last(&labels, ident.clone());
    if dl != ident {
        return Some(dl);
    }
    if rl_order_dagger_only() {
        // JEVLAB_RLORDER=dagger: no order search in the shop; the round's own reorder (v75 round_order, at the
        // first SELECTING_HAND) puts the searched order back before anything scores
        return None;
    }
    let best = jevbot::readiness::best_joker_order(gs_, 12);
    let best = if labels.contains(&"Ceremonial Dagger") { best.filter(|o| jevbot::strategy::dagger_last(&labels, o.clone()) == *o) } else { best };
    best.filter(|o| *o != ident)
}

/// The BalatroBot call that steps rumbot action `a` through the bot's engine (engine/actions.rs, inverted).
fn rpc_of(a: &rumbot_sim::game::Action) -> (String, Value) {
    use rumbot_sim::game::ActionType as T;
    let i = a.index;
    let (m, p) = match a.r#type {
        T::SelectBlind => ("select", json!({})),
        T::SkipBlind => ("skip", json!({})),
        T::RerollBoss => ("reroll_boss", json!({})),
        T::CashOut => ("cash_out", json!({})),
        T::Reroll => ("reroll", json!({})),
        T::LeaveShop => ("next_round", json!({})),
        T::SkipPack => ("pack", json!({"skip": true})),
        T::Buy => ("buy", json!({"card": i})),
        T::BuyVoucher => ("buy", json!({"voucher": i})),
        T::BuyPack => ("buy", json!({"pack": i})),
        T::SellJoker => ("sell", json!({"joker": i})),
        T::SellConsumable => ("sell", json!({"consumable": i})),
        T::UseConsumable => ("use", json!({"consumable": i, "cards": a.cards})),
        T::PickPack => ("pack", json!({"card": i, "targets": a.cards})),
        t => panic!("no RPC for {t:?}"),
    };
    (m.to_string(), p)
}

impl RlDriver {
    fn uniform(&mut self) -> f64 {
        self.rng = self.rng.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.rng;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        ((z ^ (z >> 31)) >> 11) as f64 / (1u64 << 53) as f64
    }

    fn decide(&mut self, gs_: &Value, handler: &str) -> Option<(String, Value)> {
        use rumbot_sim::game::{ActionType, PackChoice};
        if handler == "shop" {
            if let Some(b) = self.pending_buy.take() {
                return Some(b);
            }
            if let Some(o) = jevbot::prof::time("rl.v77_order", || v77_shop_order(gs_)) {
                self.reorders += 1;
                return Some(("rearrange".into(), json!({"jokers": o})));
            }
        }
        let game = self.game.clone();
        let eng = game.borrow();
        let g = eng.g.as_ref()?;
        if !rlcore::vocab::strategic_phase(g.phase) {
            return None;
        }
        let (oi, ov) = jevbot::prof::time("rl.encode", || self.enc.encode(g));
        let legal = jevbot::prof::time("rl.legal", || rlcore::vocab::legal_with_ids(g));
        let swaps = jevbot::prof::time("rl.swaps", || rlcore::vocab::legal_replaces(g));
        let mut mask: Vec<u16> = legal.iter().map(|(v, _)| *v as u16).chain(swaps.iter().map(|(v, _, _)| *v as u16)).collect();
        mask.sort_unstable();
        mask.dedup();
        if mask.is_empty() {
            return None;
        }
        let u = if self.temp.is_some() { self.uniform() } else { 0.0 };
        let (id, _, _) = jevbot::prof::time("rl.net", || self.net.choose(&oi, &ov, &mask, self.temp, u));
        if let Some((_, sell, buy)) = swaps.iter().find(|(v, _, _)| *v == id) {
            self.swaps += 1;
            self.decisions += 1;
            if let Some(l) = &self.log {
                l.borrow_mut().swap_next = true;
            }
            self.pending_buy = Some(rpc_of(buy));
            return Some(rpc_of(sell));
        }
        let cands: Vec<rumbot_sim::game::Action> = legal.iter().filter(|(v, _)| *v == id).map(|(_, a)| a.clone()).collect();
        let mut a = rlcore::vocab::to_action_from(g, &cands, None)?;
        if !a.cards.is_empty() {
            // a targeted consumable: v77's own target chooser (tarots::plan on the build, as decisions/booster.rs)
            let name = match a.r#type {
                ActionType::PickPack => match &g.pack_options[a.index as usize] {
                    PackChoice::Consumable(spec) => Some(spec.name),
                    _ => None,
                },
                ActionType::UseConsumable => Some(g.consumables[a.index as usize].borrow().spec.name),
                _ => None,
            };
            let key = name.and_then(rumbot_sim::shop_pool::key_by_consumable_name);
            let planned = jevbot::prof::time("rl.targets", || key.and_then(|k| jevbot::tarots::plan(k, jevbot::gs::cards_of(gs_, "hand"), &jevbot::build::decide(gs_))));
            match planned.map(|(mut t, _)| {
                t.sort_unstable();
                t
            }) {
                Some(t) => match rlcore::vocab::to_action_from(g, &cands, Some(&t)) {
                    Some(b) if b.cards == t => {
                        a = b;
                        self.targets_planned += 1;
                    }
                    _ => self.targets_fallback += 1,
                },
                None => self.targets_fallback += 1,
            }
        }
        self.decisions += 1;
        Some(rpc_of(&a))
    }
}

/// Strip card `id` fields and timing keys, for comparison.
fn norm(v: &Value) -> Value {
    match jevbot::replay::strip_timing(v) {
        Value::Object(m) => Value::Object(m.into_iter().filter(|(k, _)| k != "id" && k != "sort_id").map(|(k, v)| (k, norm(&v))).collect()),
        Value::Array(a) => Value::Array(a.iter().map(norm).collect()),
        o => o,
    }
}

fn first_diff(a: &Value, b: &Value, path: &str) -> Option<String> {
    match (a, b) {
        (Value::Object(x), Value::Object(y)) => {
            for (k, v) in x {
                match y.get(k) {
                    None => return Some(format!("{path}.{k}: rust {} vs python MISSING", jevbot::trace::short(v, 120))),
                    Some(w) => if let Some(d) = first_diff(v, w, &format!("{path}.{k}")) { return Some(d) },
                }
            }
            y.keys().find(|k| !x.contains_key(*k)).map(|k| format!("{path}.{k}: rust MISSING vs python {}", jevbot::trace::short(&y[k], 120)))
        }
        (Value::Array(x), Value::Array(y)) => {
            for i in 0..x.len().min(y.len()) {
                if let Some(d) = first_diff(&x[i], &y[i], &format!("{path}[{i}]")) { return Some(d) }
            }
            if x.len() != y.len() { Some(format!("{path}: len rust {} vs python {}", x.len(), y.len())) } else { None }
        }
        _ => if a == b { None } else { Some(format!("{path}: rust {} vs python {}", jevbot::trace::short(a, 120), jevbot::trace::short(b, 120))) },
    }
}

/// First differing call between the live log and the Python trace of `seed`.
fn diff_trace(seed: &str, log: &[Value]) -> String {
    let p = jevbot::trace::trace_files(arm()).into_iter().find(|p| p.to_string_lossy().contains(seed));
    let Some(p) = p else { return "no python trace".into() };
    let py: Vec<Value> = jevbot::trace::load_trace(&p).into_iter().filter(|r| r["k"] == "call").collect();
    for (i, r) in log.iter().enumerate() {
        let Some(w) = py.get(i) else { return format!("call {i}: python trace ended ({} calls), rust continues with {}", py.len(), r["method"]) };
        for key in ["method", "params", "ok", "err"] {
            let (a, b) = (norm(r.get(key).unwrap_or(&Value::Null)), norm(w.get(key).unwrap_or(&Value::Null)));
            if let Some(d) = first_diff(&a, &b, key) {
                let prev = if i > 0 { format!("{} {}", log[i - 1]["method"], jevbot::trace::short(&log[i - 1]["params"], 80)) } else { String::new() };
                return format!("call {i} ({} {}) after [{prev}]: {d}", r["method"], jevbot::trace::short(&r["params"], 80));
            }
        }
    }
    if py.len() > log.len() { format!("rust ended at call {} of {}", log.len(), py.len()) } else { "identical".into() }
}

/// JEVLAB_SNAPS=1: one snapshot per shop visit (value-model training data): money, jokers [key, edition],
/// hand levels {hand: [level, played]}, deck size and enhanced cards, hand size, vouchers.
fn snap(g: &Value) -> Value {
    let cards = |a: &str| g[a]["cards"].as_array().cloned().unwrap_or_default();
    let deck: Vec<Value> = cards("cards").into_iter().chain(cards("hand")).collect();
    let enh = deck.iter().filter(|c| c["modifier"]["enhancement"].as_str().is_some_and(|e| !e.is_empty())).count();
    let levels: Map<String, Value> = g["hands"].as_object().map(|h| h.iter()
        .map(|(k, v)| (k.clone(), json!([v["level"], v["played"]]))).collect()).unwrap_or_default();
    let vouchers: Vec<Value> = match &g["used_vouchers"] {
        Value::Object(m) => m.keys().map(|k| json!(k)).collect(),
        Value::Array(a) => a.clone(),
        _ => vec![],
    };
    let boss = g["blinds"].as_object().and_then(|b| b.values().find(|x| x["type"] == "BOSS")).map(|x| x["name"].clone()).unwrap_or(Value::Null);
    json!({"ante": g["ante_num"], "round": g["round_num"], "money": g["money"], "boss": boss,
           "jokers": cards("jokers").iter().map(|c| json!([c["key"], c["modifier"]["edition"]])).collect::<Vec<_>>(),
           "levels": levels, "deck": deck.len(), "enh": enh, "hand_size": g["hand"]["limit"], "vouchers": vouchers})
}

fn play(seed: &str) -> Result<Value, String> {
    let t0 = std::time::Instant::now();
    let tmp = std::env::temp_dir().join(format!("plab_{}_{}", seed, std::process::id()));
    let _ = std::fs::remove_dir_all(&tmp);
    let oracle: SharedOracle = Rc::new(RefCell::new(RustOracle));
    let calls: Rc<RefCell<Vec<Value>>> = Rc::new(RefCell::new(Vec::new()));
    let want_trace = std::env::var_os("JEVLAB_TRACE").is_some();
    let jev = CodeJev::new(ARM_F.load(Ordering::SeqCst), oracle.clone());
    // JEVLAB_RLLOG=<dir>: log every strategic rumbot action of this game for behaviour cloning (rlcore::log)
    let rl_dir = std::env::var_os("JEVLAB_RLLOG").map(std::path::PathBuf::from);
    let rl_log = rl_dir.as_ref().map(|_| Rc::new(RefCell::new(rlcore::log::GameLog::new(seed))));
    let mut rumbot = RumbotGame::new();
    if let Some(l) = &rl_log {
        let l = l.clone();
        rumbot.on_step = Some(Box::new(move |g, a| l.borrow_mut().on_step(g, a)));
    }
    let rumbot = Rc::new(RefCell::new(rumbot));
    let mut bot = JevBot::new(
        "RED", "WHITE", "code-top-option", &tmp, Box::new(RecGame { inner: Box::new(SharedGame(rumbot.clone())), log: calls.clone(), rec: want_trace }), Box::new(jev), oracle,
        Box::new(RealClock::new()), "rust", "rust", "rust", Box::new(SystemClock),
    );
    if let (Some(l), Some(_)) = (&rl_log, rl_net()) {
        l.borrow_mut().explicit = true;
    }
    let rl_driver = rl_net().map(|net| {
        let k: u64 = std::env::var("JEVLAB_RLSEED").ok().and_then(|x| x.parse().ok()).unwrap_or(0);
        Rc::new(RefCell::new(RlDriver {
            game: rumbot.clone(),
            net,
            temp: std::env::var("JEVLAB_RLTEMP").ok().and_then(|x| x.parse().ok()),
            rng: rumbot_sim::state::fnv1a64(seed.as_bytes()) ^ k.wrapping_mul(0x9E37_79B9_7F4A_7C15),
            enc: rlcore::obs::Encoder::new(),
            decisions: 0,
            reorders: 0,
            targets_planned: 0,
            targets_fallback: 0,
            pending_buy: None,
            swaps: 0,
            log: rl_log.clone(),
        }))
    });
    if let Some(d) = &rl_driver {
        let d = d.clone();
        bot.strategic = Some(Box::new(move |gs_, h| jevbot::prof::time("rl.hook", || d.borrow_mut().decide(gs_, h))));
    }
    let e = jevbot::experiment::get();
    bot.dec = jevbot::decisions::DecisionCfg::lab_with(e.fc_n.unwrap_or(64), e.fc_bosses.unwrap_or(3));
    bot.recorder.raw_states = want_trace; // only decisions.jsonl is read back
    bot.act("start", Some(&json!({"seed": seed, "deck": "RED", "stake": "WHITE"}))).map_err(|e| format!("start: {e:?}"))?;
    let mut engine_won = false;
    let want_snaps = std::env::var_os("JEVLAB_SNAPS").is_some();
    let (mut snaps, mut snapped) = (Vec::new(), std::collections::HashSet::new());
    for _ in 0..20000 {
        if want_snaps {
            if let Some(g) = bot.last_state.as_ref().filter(|g| g["state"] == "SHOP") {
                if snapped.insert(g["round_num"].to_string()) {
                    let mut s = snap(g);
                    if let Some(p) = jevbot::value_model::power_of_view(g) {
                        s["power"] = json!(p.iter().map(|x| (x * 1000.0).round() / 1000.0).collect::<Vec<f64>>());
                    }
                    snaps.push(s);
                }
            }
        }
        match jevbot::prof::time("bot.step", || bot.step()) {
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
    // every event that ran a forecast (policy_lab.py counted only nonzero times, so a forecast that rounded
    // to 0.00 s went missing and the count depended on machine speed)
    let fs: Vec<f64> = ev.iter().filter_map(|e| e.get("forecast_seconds")).map(num).collect();
    let ifs: Vec<f64> = ev.iter().filter_map(|e| e.get("item_forecast_seconds")).map(num).collect();
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
    put("arm", json!(arm()));
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
    if let Some(f) = ev.iter().find(|e| kind(e) == "flip") {
        put("flip", f.clone());
    }
    let ag: Vec<Value> = ev.iter().filter(|e| kind(e) == "agent").cloned().collect();
    if !ag.is_empty() {
        put("agent", Value::Array(ag));
    }
    if let Some(d) = jevbot::experiment::get().agent_dir.as_ref() {
        let dd = std::path::Path::new(d).join("done");
        let _ = std::fs::create_dir_all(&dd);
        let _ = std::fs::write(dd.join(seed), "");
    }
    if want_snaps {
        put("snaps", Value::Array(snaps));
    }
    if !want_trace { let _ = std::fs::remove_dir_all(&tmp); }
    if want_trace {
        let log = calls.borrow();
        let d = std::path::PathBuf::from(std::env::var("JEVLAB_TRACE").unwrap());
        let _ = std::fs::create_dir_all(&d);
        use flate2::{write::GzEncoder, Compression};
        let mut gz = GzEncoder::new(std::fs::File::create(d.join(format!("{seed}.jsonl.gz"))).map_err(|e| e.to_string())?, Compression::fast());
        for r in log.iter() { writeln!(gz, "{r}").map_err(|e| e.to_string())?; }
        let _ = writeln!(gz, "{}", json!({"k": "end", "result": row}));
        eprintln!("{seed}: first divergence: {}", diff_trace(seed, &log));
    }
    if let Some(d) = &rl_driver {
        let d = d.borrow();
        row.insert("rl_decisions".into(), json!(d.decisions));
        row.insert("rl_reorders".into(), json!(d.reorders));
        row.insert("rl_targets_planned".into(), json!(d.targets_planned));
        row.insert("rl_targets_fallback".into(), json!(d.targets_fallback));
        row.insert("rl_swaps".into(), json!(d.swaps));
    }
    if let (Some(d), Some(l)) = (&rl_dir, &rl_log) {
        let t = std::time::Instant::now();
        let l = l.borrow();
        let i = |k: &str| row.get(k).and_then(Value::as_i64).unwrap_or(0) as i32;
        let (_, bytes) = l.write(d, won, i("ante"), i("blinds"), i("round")).map_err(|e| format!("rl log: {e}"))?;
        row.insert("rl_records".into(), json!(l.records()));
        row.insert("rl_unexpressible".into(), json!(l.counters.get("unexpressible").copied().unwrap_or(0)));
        row.insert("rl_bytes".into(), json!(bytes));
        row.insert("rl_s".into(), json!(((l.secs + t.elapsed().as_secs_f64()) * 1000.0).round() / 1000.0));
    }
    Ok(Value::Object(row))
}

fn main() {
    let _ = jevbot::experiment::get(); // JEVLAB_EXP flags, including simcore's FREE_DISCARD for the rollouts
    let a: Vec<String> = std::env::args().skip(1).collect();
    let opt = |k: &str| a.iter().position(|x| x == k).and_then(|i| a.get(i + 1)).cloned();
    let mut seeds: Vec<String> = match (opt("--seeds"), opt("--set")) {
        (Some(s), _) => s.split(',').map(String::from).collect(),
        (None, Some(n)) => seed_set(&n).unwrap_or_else(|| panic!("unknown seed set {n} (sim30-a, sim60-a, gap4-a)")),
        _ => panic!("usage: jevlab --set NAME | --seeds A,B  [--arm E|F] [--limit N] [--jobs N] [--name NAME] [--out DIR]"),
    };
    if let Some(l) = opt("--limit").and_then(|l| l.parse::<usize>().ok()) {
        seeds.truncate(l);
    }
    if let Some(x) = opt("--arm") {
        assert!(x == "E" || x == "F", "--arm E | F");
        ARM_F.store(x == "F", Ordering::SeqCst);
    }
    // games are independent (every cache is per game), so any number of threads gives the same rows
    let cores = std::thread::available_parallelism().map_or(1, |n| n.get());
    // agent-dir games spend most of their time waiting on the agent, so they may outnumber the cores
    let jobs: usize = opt("--jobs").and_then(|j| j.parse().ok()).unwrap_or(1).clamp(1, if jevbot::experiment::get().agent_dir.is_some() { 64 } else { cores });
    let name = opt("--name").unwrap_or_else(|| format!("rust-{}", arm()));
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
                let r = std::panic::catch_unwind(|| jevbot::prof::time("GAME", || play(seed))).unwrap_or_else(|p| Err(jevbot::replay::panic_msg(&*p)));
                let row = r.unwrap_or_else(|e| json!({"seed": seed, "arm": arm(), "error": e}));
                // JEVLAB_RMDIRS=1 (RL training): also drop the run dir of a game that errored (a finished game's
                // is dropped anyway unless JEVLAB_TRACE)
                if std::env::var_os("JEVLAB_RMDIRS").is_some() && std::env::var_os("JEVLAB_TRACE").is_none() {
                    let _ = std::fs::remove_dir_all(std::env::temp_dir().join(format!("plab_{}_{}", seed, std::process::id())));
                }
                eprintln!("{seed}: ante {} won {} {}s", row["ante"], row["won"], row["seconds"]);
                writeln!(file.lock().unwrap(), "{row}").unwrap();
            });
        }
    });
    jevbot::prof::dump();
    if jevbot::experiment::get().free_discard {
        eprintln!("rollout free discards: {}", simcore::FREE_DISCARDS_TAKEN.load(Ordering::SeqCst));
    }
    println!("{} games, {jobs} threads, {:.1}s wall, {:.2}s/game wall -> {}", seeds.len(), t0.elapsed().as_secs_f64(), t0.elapsed().as_secs_f64() / seeds.len() as f64, out.display());
}
