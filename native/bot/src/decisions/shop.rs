//! Port of src/decisions.py lines 453-850: shop, its forecasts and _shop_act. The Python nested `obj`
//! inside `shop` stays a private closure/helper of `shop`. See docs/port-handoff/decisions.md.
//! Helpers are `pub` (not pub(crate)) so tests/dec_shop_a.rs can check them against the Python fixture.

use super::{instr, FORECAST_ERROR_LOG, FORECAST_TIMEOUT, ITEM_MIN_BUDGET, SWAP_SELLS};
use crate::forecast_state::py_repr;
use crate::forecast_stats::{items_options, options, pystr};
use crate::gs::cards_of;
use crate::hands::_d;
use crate::iface::RpcError;
use crate::jev_bot::{choice_q_map, score_q, Action, JevBot};
use crate::jokers;
use crate::prompts::{CASH_ITEM_QUESTION, CASH_STEP_QUESTION, ITEM_VALUE_LEVELS};
use crate::trace;
use pycompat::json::dumps;
use pycompat::{fixed, pyfloat, pyround_int, signed};
use serde_json::{json, Map, Value};
use std::collections::{BTreeSet, HashSet};
use std::io::Write;
use std::path::{Path, PathBuf};

/// `_score_items` input: `{key: (desc, item_obj, action)}` in insertion order.
pub type ShopItems = Vec<(String, (String, Value, Action))>;

/// Python `v.get(k)` on a dict-like value (missing or non-dict gives null).
fn g(v: &Value, k: &str) -> Value {
    _d(v).get(k).cloned().unwrap_or(Value::Null)
}

/// Python truthiness of an optional value.
fn truthy(v: Option<&Value>) -> bool {
    match v {
        None | Some(Value::Null) => false,
        Some(Value::Bool(b)) => *b,
        Some(Value::Number(n)) => n.as_f64().map_or(true, |x| x != 0.0),
        Some(Value::String(s)) => !s.is_empty(),
        Some(Value::Array(a)) => !a.is_empty(),
        Some(Value::Object(o)) => !o.is_empty(),
    }
}

/// Python float(v) for JSON numbers; null and missing read as 0.
fn num(v: &Value) -> f64 {
    v.as_f64().or_else(|| v.as_i64().map(|i| i as f64)).unwrap_or(0.0)
}

/// `{k: v for k, v in obj.items() if not k.startswith("_")}`.
fn public(obj: &Value) -> Value {
    Value::Object(_d(obj).iter().filter(|(k, _)| !k.starts_with('_')).map(|(k, v)| (k.clone(), v.clone())).collect())
}

/// Python `f"{x:+}"` on a JSON number: ints print as ints, floats as repr with a sign.
fn sign_repr(v: &Value) -> String {
    match v.as_i64() {
        Some(i) if i >= 0 => format!("+{i}"),
        Some(i) => format!("{i}"),
        None => {
            let s = pyfloat(num(v));
            if s.starts_with('-') {
                s
            } else {
                format!("+{s}")
            }
        }
    }
}

fn trunc120(s: &str) -> String {
    s.chars().take(120).collect()
}

fn nonnull(v: Value) -> Option<Value> {
    if v.is_null() {
        None
    } else {
        Some(v)
    }
}

// decisions.py 453-465
pub fn discard_obj(d: &Value) -> Value {
    let dm = _d(d);
    let mut o = Map::new();
    o.insert("plan".into(), g(d, "label"));
    o.insert("throws_away".into(), g(d, "tossed"));
    o.insert("fits_our_build".into(), dm.get("fits_build").cloned().unwrap_or(json!(false)));
    if dm.contains_key("expected_score") {
        let basis_kind = if truthy(dm.get("exact_draws")) { "all possible" } else { "sampled" };
        o.insert("modeled_average_next_play".into(), g(d, "expected_score"));
        o.insert("poor_draw_next_play".into(), g(d, "low_score"));
        o.insert("poor_draw_definition".into(), json!("20th percentile, not a guaranteed minimum"));
        o.insert("average_change_vs_playing_now".into(), json!(format!("{}%", sign_repr(&g(d, "gain_percent")))));
        o.insert("chance_next_play_clears".into(), json!(pct(num(&g(d, "chance_to_clear")))));
        o.insert("chance_next_play_meets_pace".into(), json!(pct(num(&g(d, "chance_to_meet_pace")))));
        o.insert("cards_drawn".into(), g(d, "drawn"));
        o.insert(
            "basis".into(),
            json!(format!("{} {} unordered draws; scoring model estimate", pystr(&g(d, "samples")), basis_kind)),
        );
    }
    Value::Object(o)
}

// decisions.py 468-484: (scores {key: score}, meta)
pub fn score_items(bot: &mut JevBot, gs: &Value, qname: &str, state: &Value, items: &ShopItems) -> (Map<String, Value>, Value) {
    let mut questions = Map::new();
    for (k, (_, obj, _)) in items {
        let question = if _d(obj).contains_key("cash_outcome") { CASH_ITEM_QUESTION } else { instr("item_value") };
        questions.insert(k.clone(), score_q(json!({"question": question, "item": public(obj)}), json!(ITEM_VALUE_LEVELS)));
    }
    let cash: Vec<(String, Value)> = items
        .iter()
        .filter(|it| qname == "shop" && num(&g(&g(&it.1 .1, "cash_outcome"), "net_cash_gain")) > 0.0)
        .map(|it| (it.0.clone(), it.1 .1.clone()))
        .collect();
    let has_cash = !cash.is_empty();
    if has_cash {
        let mut options = Map::new();
        for (k, obj) in &cash {
            options.insert(k.clone(), public(obj));
        }
        options.insert("continue_shopping".into(), json!({"action": "Pass these cash transactions and consider the other shop actions"}));
        questions.insert("cash_step".into(), choice_q_map(CASH_STEP_QUESTION, options));
    }
    let (answers, mut meta) = bot.query(gs, state, &questions);
    if has_cash {
        let probs = _d(&answers.get("cash_step").map(|a| g(a, "probabilities")).unwrap_or(Value::Null)).clone();
        // Python max(probabilities, key=probabilities.get): first maximal key in dict order.
        let mut best: Option<(String, f64)> = None;
        for (k, v) in &probs {
            let p = num(v);
            if best.as_ref().map_or(true, |(_, bp)| p > *bp) {
                best = Some((k.clone(), p));
            }
        }
        let m = meta.as_object_mut().expect("meta object");
        m.insert("cash_step_choice".into(), json!(best.map(|(k, _)| k)));
        m.insert("cash_step_probabilities".into(), Value::Object(probs));
    }
    let scores: Map<String, Value> = items
        .iter()
        .map(|(k, _)| (k.clone(), answers.get(k).map(|a| g(a, "score")).unwrap_or(Value::Null)))
        .collect();
    (scores, meta)
}

// decisions.py 487-496: indices of the n weakest jokers (set; used for membership only)
pub fn weakest_jokers(gs: &Value, recent: &[Value], n: usize) -> HashSet<usize> {
    let mut loss: Vec<(f64, f64, usize)> = Vec::new();
    for (j, c) in cards_of(gs, "jokers").iter().enumerate() {
        let modifier = g(c, "modifier");
        let m = _d(&modifier);
        if truthy(m.get("eternal")) || m.get("edition") == Some(&json!("NEGATIVE")) {
            continue;
        }
        let gain = jokers::gain(gs, recent, None, Some(j), None).unwrap_or(0.0);
        let cost = g(c, "cost");
        let sell = _d(&cost).get("sell").map(num).unwrap_or(0.0);
        loss.push((-gain, sell, j));
    }
    loss.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap().then(a.1.partial_cmp(&b.1).unwrap()).then(a.2.cmp(&b.2)));
    loss.into_iter().take(n).map(|(_, _, j)| j).collect()
}

/// Python try-body of `_shop_forecast`: Ok((sig, f)) or Err((exception type, message)).
fn joker_forecast(bot: &mut JevBot, gs: &Value, weak: &HashSet<usize>) -> Result<(Value, Value), (String, String)> {
    let mut keys: BTreeSet<String> = BTreeSet::new();
    for o in options(gs) {
        if !truthy(o.get("supported")) {
            continue;
        }
        let k = g(&o, "key").as_str().unwrap_or("").to_string();
        if k.starts_with("replace_") {
            let part = k.split('_').nth(1).unwrap_or("").to_string();
            let idx = part
                .parse::<usize>()
                .map_err(|_| ("ValueError".to_string(), format!("invalid literal for int() with base 10: '{part}'")))?;
            if !weak.contains(&idx) {
                continue;
            }
        }
        keys.insert(k);
    }
    let keys_v: Vec<String> = keys.iter().cloned().collect();
    let buy = |c: &Value| _d(&g(c, "cost")).get("buy").cloned().unwrap_or(Value::Null);
    let sig = Value::String(dumps(&json!([
        g(gs, "seed"),
        g(gs, "ante_num"),
        g(gs, "round_num"),
        g(gs, "money"),
        cards_of(gs, "shop").iter().chain(cards_of(gs, "vouchers").iter()).map(|c| json!([g(c, "key"), buy(c)])).collect::<Vec<_>>(),
        cards_of(gs, "jokers").iter().map(|c| g(c, "key")).collect::<Vec<_>>(),
        keys_v,
    ])));
    if let Some((csig, cf)) = bot._forecast_cache.as_ref() {
        if *csig == sig {
            return Ok((sig.clone(), cf.clone()));
        }
    }
    // v21: one shop budget for both forecasts; the item forecast keeps at least ITEM_MIN_BUDGET of it
    let budget = bot.dec.forecast_budget.map(|b| b - ITEM_MIN_BUDGET);
    let (n, bosses) = (bot.dec.forecast_n, bot.dec.forecast_bosses);
    let mut f = bot.oracle.borrow_mut().forecast(gs, n, bosses, &keys_v, budget);
    if let Some(o) = f.as_object_mut() {
        o.remove("_runs");
    }
    if num(&g(&f, "seconds")) > FORECAST_TIMEOUT || !_d(&g(&f, "options")).contains_key("skip") {
        return Err(("TimeoutError".into(), format!("{}s", pystr(&g(&f, "seconds")))));
    }
    let weak_sorted: Vec<usize> = weak.iter().copied().collect::<BTreeSet<_>>().into_iter().collect();
    f.as_object_mut().expect("forecast object").insert("weak".into(), json!(weak_sorted));
    let msg = format!(
        "Forecast {}s: {} rollouts x {} options",
        pystr(&g(&f, "seconds")),
        pystr(&g(&f, "rollouts")),
        _d(&g(&f, "options")).len()
    );
    bot.emit("info", json!({"msg": msg}));
    Ok((sig, f))
}

// decisions.py 499-526: rollout.forecast result or None on failure; cached on bot._forecast_cache
pub fn shop_forecast(bot: &mut JevBot, gs: &Value, recent: &[Value], jokers_full: bool) -> Option<Value> {
    let weak = if jokers_full { weakest_jokers(gs, recent, SWAP_SELLS) } else { HashSet::new() };
    let (sig, f) = match joker_forecast(bot, gs, &weak) {
        Ok(x) => x,
        Err((t, m)) => {
            let msg = format!("Forecast unavailable ({t}: {}); using measured gains", trunc120(&m));
            bot.emit("info", json!({"msg": msg}));
            log_forecast_error(bot, gs, "joker forecast");
            (Value::Null, Value::Null)
        }
    };
    bot._forecast_cache = Some((sig, f.clone()));
    nonnull(f)
}

// decisions.py 529-545. Appends to runs/_forecast_errors.log under the repo root (Python's `src/..`).
// Differences from Python: no traceback (Rust has none to print), a unix-seconds stamp instead of
// strftime, and run= is empty (the recorder keeps its run dir private).
pub fn log_forecast_error(_bot: &mut JevBot, gs: &Value, what: &str) {
    let shop: Map<String, Value> = ["shop", "packs", "vouchers", "consumables", "jokers"]
        .iter()
        .map(|a| (a.to_string(), Value::Array(cards_of(gs, a).iter().map(|c| g(c, "key")).collect())))
        .collect();
    let root: PathBuf = trace::data_dir().parent().and_then(Path::parent).unwrap_or(Path::new(".")).to_path_buf();
    let path = root.join(FORECAST_ERROR_LOG);
    let stamp = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0);
    let line = format!(
        "--- {stamp} {what} run= seed={} ante={} round={} money={} {}\n(no traceback: Rust port)\n",
        pystr(&g(gs, "seed")),
        pystr(&g(gs, "ante_num")),
        pystr(&g(gs, "round_num")),
        pystr(&g(gs, "money")),
        dumps(&Value::Object(shop))
    );
    // Logging must never break the shop: ignore I/O errors, as Python does.
    let _ = (|| -> std::io::Result<()> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let mut f = std::fs::OpenOptions::new().create(true).append(true).open(&path)?;
        f.write_all(line.as_bytes())
    })();
}

/// rollout_items.forecast call of `_item_forecast`: Ok(f) or Err((exception type, message)).
fn item_forecast_run(bot: &mut JevBot, gs: &Value, fc: &Value, keys: &[String]) -> Result<Value, (String, String)> {
    let budget = bot.dec.forecast_budget.map(|b| ITEM_MIN_BUDGET.max(b - num(&g(fc, "seconds"))));
    let (n, bosses) = (bot.dec.forecast_n, bot.dec.forecast_bosses);
    let f = bot.oracle.borrow_mut().items_forecast(gs, n, bosses, keys, budget);
    if truthy(Some(&g(&f, "errors"))) {
        let msg = format!("Item forecast option errors: {}", py_repr(&g(&f, "errors")));
        bot.emit("info", json!({"msg": msg}));
    }
    if !_d(&g(&f, "options")).contains_key("skip") {
        return Err(("RuntimeError".into(), "item forecast has no skip baseline".into()));
    }
    let msg = format!(
        "Item forecast {}s: {} rollouts x {} options",
        pystr(&g(&f, "seconds")),
        pystr(&g(&f, "rollouts")),
        _d(&g(&f, "options")).len()
    );
    bot.emit("info", json!({"msg": msg}));
    Ok(f)
}

// decisions.py 548-574: rollout_items.forecast result or None; cached on bot._item_forecast_cache
pub fn item_forecast(bot: &mut JevBot, gs: &Value, fc: &Value) -> Option<Value> {
    let keys: BTreeSet<String> = items_options(gs)
        .iter()
        .map(|o| g(o, "key").as_str().unwrap_or("").to_string())
        .filter(|k| k != "skip")
        .collect();
    if keys.is_empty() {
        return None;
    }
    let keys_v: Vec<String> = keys.into_iter().collect();
    let buy = |c: &Value| _d(&g(c, "cost")).get("buy").cloned().unwrap_or(Value::Null);
    let sig = Value::String(dumps(&json!([
        g(gs, "seed"),
        g(gs, "ante_num"),
        g(gs, "round_num"),
        g(gs, "money"),
        cards_of(gs, "shop").iter().chain(cards_of(gs, "packs").iter()).map(|c| json!([g(c, "key"), buy(c)])).collect::<Vec<_>>(),
        cards_of(gs, "jokers").iter().chain(cards_of(gs, "consumables").iter()).map(|c| g(c, "key")).collect::<Vec<_>>(),
        keys_v,
    ])));
    if let Some((csig, cf)) = bot._item_forecast_cache.as_ref() {
        if *csig == sig {
            return nonnull(cf.clone());
        }
    }
    match item_forecast_run(bot, gs, fc, &keys_v) {
        Ok(f) => {
            bot._item_forecast_cache = Some((sig, f.clone()));
            nonnull(f)
        }
        Err((t, m)) => {
            let msg = format!("Item forecast unavailable ({t}: {})", trunc120(&m));
            bot.emit("info", json!({"msg": msg}));
            log_forecast_error(bot, gs, "item forecast");
            bot._item_forecast_cache = Some((Value::Null, Value::Null));
            None
        }
    }
}

// decisions.py 577-578
pub fn pct(p: f64) -> String {
    format!("{}%", pyround_int(100.0 * p))
}

// decisions.py 581-594
pub fn forecast_fields(fc: &Value, key: &str) -> Value {
    let o = &fc["options"][key];
    let s = &fc["options"]["skip"];
    let nb = &o["vs_skip"]["next_boss"];
    let bl = &o["vs_skip"]["blinds"];
    let mut out = Map::new();
    out.insert(
        "next_boss_clear".into(),
        json!(format!(
            "{} (skip {}, {} ± {} pts)",
            pct(num(&o["p_next_boss"])),
            pct(num(&s["p_next_boss"])),
            signed(100.0 * num(&nb[0]), 0),
            fixed(100.0 * num(&nb[1]), 0)
        )),
    );
    out.insert(
        "survive_horizon".into(),
        json!(format!(
            "{} clear all {} next bosses (skip {})",
            pct(num(&o["p_survive"])),
            pystr(&fc["bosses"]),
            pct(num(&s["p_survive"]))
        )),
    );
    out.insert(
        "blinds_cleared_horizon".into(),
        json!(format!(
            "{} of {} (skip {}, {} ± {})",
            pystr(&o["exp_blinds_cleared"]),
            pystr(&o["horizon_blinds"]),
            pystr(&s["exp_blinds_cleared"]),
            signed(num(&bl[0]), 2),
            fixed(num(&bl[1]), 2)
        )),
    );
    out.insert("_fc_delta".into(), bl[0].clone());
    out.insert("_fc_se".into(), bl[1].clone());
    if !o["next_boss_margin"].is_null() {
        let next_boss = match _d(o).get("next_boss") {
            Some(v) => pystr(v),
            None => "next boss".to_string(),
        };
        let mut m = format!("median {}x of the {} target", pystr(&o["next_boss_margin"]), next_boss);
        if !s["next_boss_margin"].is_null() {
            m += &format!(" (skip {}x)", pystr(&s["next_boss_margin"]));
        }
        out.insert("boss_margin".into(), json!(m));
    }
    Value::Object(out)
}

// decisions.py 597-606
pub fn forecast_note(gs: &Value, fc: &Value) -> String {
    let s = &fc["options"]["skip"];
    let boss = _d(&g(gs, "blinds")).values().find(|b| g(b, "type") == json!("BOSS")).cloned().unwrap_or(json!({}));
    let ante = if truthy(Some(&g(gs, "ante_num"))) { g(gs, "ante_num") } else { json!(1) };
    format!(
        "Item numbers simulate {} futures each: take the item, then play the next {} blinds through {} boss blinds with the build frozen (no other purchases, planets used, scaling jokers grow, our usual play). The ante {} boss is known ({}); later bosses are sampled. Buying nothing: next boss {}, survive all {}. Packs are opened with sampled contents and our usual pick; tarots are used on our usual targets. Not simulated: rerolls, later shops.",
        pystr(&fc["rollouts"]),
        pystr(&s["horizon_blinds"]),
        pystr(&fc["bosses"]),
        pystr(&ante),
        pystr(&g(&boss, "name")),
        pct(num(&s["p_next_boss"])),
        pct(num(&s["p_survive"]))
    )
}

// decisions.py 609-613
pub fn economic(obj: &Value, c: &Value) -> bool {
    if _d(obj).contains_key("cash_outcome") || g(c, "set") == json!("VOUCHER") {
        return true;
    }
    g(c, "set") == json!("JOKER") && !jokers::entry(c).scoring
}

/// decisions.py 616-823 `shop(bot, gs)` (nested `obj` at 647 is a helper inside).
pub fn shop(_bot: &mut JevBot, _gs: &Value) -> Result<(), RpcError> {
    todo!("decisions shop")
}

// decisions.py 826-848: runs the action; on RPCError adds json.dumps(action) to bot._failed and returns Err
pub(crate) fn shop_act(_bot: &mut JevBot, _gs: &Value, _action: &Action) -> Result<(), RpcError> {
    todo!("decisions shop_act")
}
