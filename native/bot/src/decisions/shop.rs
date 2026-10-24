//! Port of src/decisions.py lines 453-850: shop, its forecasts and _shop_act. The Python nested `obj`
//! inside `shop` stays a private closure/helper of `shop`. See docs/port-handoff/decisions.md.
//! Helpers are `pub` (not pub(crate)) so tests/dec_shop_a.rs can check them against the Python fixture.

use super::{instr, FORECAST_ERROR_LOG, FORECAST_TIMEOUT, ITEM_MIN_BUDGET, SWAP_SELLS};
use pycompat::json::pyrepr;
use crate::forecast_stats::{items_options, options, pystr};
use crate::gs::cards_of;
use crate::hands::_d;
use crate::iface::RpcError;
use crate::jev_bot::{choice_q_map, score_q, Action, JevBot, Stop};
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
pub fn score_items(bot: &mut JevBot, gs: &Value, qname: &str, state: &Value, items: &ShopItems) -> Result<(Map<String, Value>, Value), Stop> {
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
    let (answers, mut meta) = bot.query(gs, state, &questions)?;
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
    Ok((scores, meta))
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
        if crate::experiment::get().pair_keep && crate::experiment::is_pair_joker(g(c, "label").as_str()) {
            continue; // lab: Pair commitment keeps its Pair jokers
        }
        let gain = jokers::gain(gs, recent, None, Some(j), None).unwrap_or(0.0);
        let cost = g(c, "cost");
        let sell = _d(&cost).get("sell").map(num).unwrap_or(0.0);
        loss.push((-gain, sell, j));
    }
    loss.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap().then(a.1.partial_cmp(&b.1).unwrap()).then(a.2.cmp(&b.2)));
    let mut out: HashSet<usize> = loss.iter().take(n).map(|(_, _, j)| *j).collect();
    if crate::experiment::get().prior_sell {
        // lab: also offer to sell the held joker the learned values rate lowest
        let jv = |j: usize| crate::experiment::joker_value(crate::gs::s(&g(&cards_of(gs, "jokers")[j], "key")));
        if let Some(&(_, _, j)) = loss.iter().min_by(|a, b| jv(a.2).partial_cmp(&jv(b.2)).unwrap().then(a.2.cmp(&b.2))) {
            out.insert(j);
        }
    }
    out
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
    let weak = if jokers_full { weakest_jokers(gs, recent, crate::experiment::get().swap_sells.unwrap_or(SWAP_SELLS)) } else { HashSet::new() };
    let (sig, f) = match joker_forecast(bot, gs, &weak) {
        Ok(x) => x,
        Err((t, m)) => {
            let msg = format!("Forecast unavailable ({t}: {}); using measured gains", trunc120(&m));
            bot.emit("info", json!({"msg": msg}));
            log_forecast_error(bot, gs, "joker forecast", &format!("{t}: {m}"));
            (Value::Null, Value::Null)
        }
    };
    bot._forecast_cache = Some((sig, f.clone()));
    nonnull(f)
}

// decisions.py 529-545. Appends to <bot.runs_root>/_forecast_errors.log (a relative runs_root resolves under the
// repo root, Python's `src/..`). Python writes the exception's traceback; here `err` is the error message and
// the location is the caller's file:line:col (`#[track_caller]`, no backtrace). run= is the current run dir name
// (empty before the first `ensure`, like Python's getattr default).
#[track_caller]
pub fn log_forecast_error(bot: &mut JevBot, gs: &Value, what: &str, err: &str) {
    let loc = std::panic::Location::caller();
    let shop: Map<String, Value> = ["shop", "packs", "vouchers", "consumables", "jokers"]
        .iter()
        .map(|a| (a.to_string(), Value::Array(cards_of(gs, a).iter().map(|c| g(c, "key")).collect())))
        .collect();
    let run = bot.recorder.dir().and_then(Path::file_name).map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
    let path = if bot.runs_root.is_absolute() {
        bot.runs_root.join(FORECAST_ERROR_LOG)
    } else {
        let repo: PathBuf = trace::data_dir().parent().and_then(Path::parent).unwrap_or(Path::new(".")).to_path_buf();
        repo.join(&bot.runs_root).join(FORECAST_ERROR_LOG)
    };
    let stamp = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0);
    let line = format!(
        "--- {stamp} {what} run={run} seed={} ante={} round={} money={} {}
{err}
  at {}:{}:{}
",
        pystr(&g(gs, "seed")),
        pystr(&g(gs, "ante_num")),
        pystr(&g(gs, "round_num")),
        pystr(&g(gs, "money")),
        dumps(&Value::Object(shop)),
        loc.file(),
        loc.line(),
        loc.column(),
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
        let msg = format!("Item forecast option errors: {}", pyrepr(&g(&f, "errors")));
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
            log_forecast_error(bot, gs, "item forecast", &format!("{t}: {m}"));
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

/// Python `repr`/`f"{x}"` of a JSON number (int stays int, float via repr).
fn rep(v: &Value) -> String {
    match v {
        Value::Number(n) if n.is_i64() => n.to_string(),
        Value::Number(_) => pyfloat(num(v)),
        Value::Null => "None".into(),
        Value::String(s) => s.clone(),
        other => other.to_string(),
    }
}

fn chars_to(s: &str, n: usize) -> String {
    s.chars().take(n).collect()
}

/// One entry of shop()'s `items`: (key, desc, obj, action, cost).
struct Item {
    key: String,
    desc: String,
    obj: Value,
    action: Action,
    cost: i64,
}

fn flag(o: &Value, k: &str) -> bool {
    truthy(_d(o).get(k))
}

fn fnum(o: &Value, k: &str, default: f64) -> f64 {
    _d(o).get(k).map_or(default, num)
}

/// decisions.py 616-823 `shop(bot, gs)` (nested `obj` at 647 is a helper inside).
// decisions.py 865-881
/// v28: the shop planet for the hand our deck scores most with (readiness.play_mix, levels and jokers
/// included), else the biggest Celestial pack, while PLANET_FLOOR dollars stay; None otherwise.
fn planet_rule(gs: &Value, items: &[Item], money: i64) -> Option<usize> {
    let mix = crate::readiness::play_mix(gs, 24);
    let mut top: Option<(&String, f64)> = None; // max(mix, key=mix.get): the first of equal shares
    for (k, v) in _d(&mix) {
        if top.map_or(true, |(_, b)| num(v) > b) {
            top = Some((k, num(v)));
        }
    }
    let (shop, packs) = (cards_of(gs, "shop"), cards_of(gs, "packs"));
    let idx = |k: &str| k.rsplit('_').next().unwrap().parse::<usize>().unwrap();
    // lab: JEVLAB_EXP=pair-planets commits every shop planet to Pair
    let top: Option<String> = if crate::experiment::get().pair_planets { Some("Pair".into()) } else { top.map(|(k, _)| k.clone()) };
    if let Some(top) = top {
        let needle = format!("Level up {top} ");
        for (n, it) in items.iter().enumerate() {
            if it.key.starts_with("buy_card_") && money - it.cost >= super::planet_floor() {
                let c = &shop[idx(&it.key)];
                let eff = g(&g(c, "value"), "effect");
                if g(c, "set") == json!("PLANET") && eff.as_str().unwrap_or("").contains(&needle) {
                    return Some(n);
                }
            }
        }
    }
    let mut best: Option<usize> = None;
    for (n, it) in items.iter().enumerate() {
        if it.key.starts_with("buy_pack_") && money - it.cost >= super::planet_floor()
            && g(&packs[idx(&it.key)], "label").as_str().unwrap_or("").contains("Celestial")
            && best.map_or(true, |b| it.cost > items[b].cost)
        {
            best = Some(n);
        }
    }
    best
}

pub fn shop(bot: &mut JevBot, gs: &Value) -> Result<(), Stop> {
    let key = json!([g(gs, "ante_num"), g(gs, "round_num")]);
    if key != bot._shop_key {
        bot._shop_key = key;
        bot._shop_actions = 0;
        bot._failed = HashSet::new();
        bot._rerolls = 0;
    }
    let money_v = g(gs, "money");
    let money: i64 = if truthy(Some(&money_v)) { crate::gs::int(&money_v) } else { 0 };
    let jok = crate::gs::area(gs, "jokers");
    let cons = crate::gs::area(gs, "consumables");
    let jok_count = jok.get("count").map_or(0, crate::gs::int);
    let jokers_full = jok_count >= jok.get("limit").map_or(5, crate::gs::int);
    let cons_full = cons.get("count").map_or(0, crate::gs::int) >= cons.get("limit").map_or(2, crate::gs::int);
    let (mains, mut state) = super::run_context(gs);
    let boss = _d(&g(gs, "blinds")).values().find(|b| g(b, "type") == json!("BOSS")).cloned().unwrap_or_else(|| json!({}));
    state["upcoming_boss"] = json!({"name": g(&boss, "name"), "effect": g(&boss, "effect"), "chip_target": g(&boss, "score")});
    // v38: Verdant Leaf debuffs every card until a joker is sold. Before it, buy the cheapest sellable joker
    // as bait (a free slot only) so the sale costs nothing; play::_leaf_sale sells it first.
    let next_is_boss = _d(&g(gs, "blinds")).values().any(|b| g(b, "type") == json!("BIG") && matches!(g(b, "status").as_str(), Some("DEFEATED") | Some("SKIPPED")));
    if g(&boss, "name") == json!("Verdant Leaf") && next_is_boss && !jokers_full && bot._shop_actions < crate::jev_bot::SHOP_ACTION_CAP {
        let held = cards_of(gs, "jokers").iter().any(|c| !bot._leaf_bait.is_null() && g(c, "id") == bot._leaf_bait);
        if !held {
            let cheapest = cards_of(gs, "shop").iter().enumerate()
                .filter(|(_, c)| g(c, "set") == json!("JOKER") && !truthy(_d(&g(c, "modifier")).get("eternal")))
                .map(|(i, c)| (crate::gs::int(&g(&g(c, "cost"), "buy")), i, c))
                .filter(|(cost, _, _)| *cost <= money)
                .min_by_key(|(cost, i, _)| (*cost, *i));
            if let Some((cost, i, c)) = cheapest {
                bot._leaf_bait = g(c, "id");
                let action: Action = ("buy".into(), Some(json!({"card": i})));
                bot.emit("auto", json!({"question": "shop", "choice": format!("buy_card_{i}"), "action": crate::jev_bot::action_value(&action),
                    "desc": format!("${cost} {}", crate::gs::s(&g(c, "label"))), "reason": "Verdant Leaf bait: a cheap joker to sell when the boss starts"}));
                return Ok(shop_act(bot, gs, &action)?);
            }
        }
    }
    let b = crate::prof::time("s.build", || crate::build::decide(gs));
    state["our_build"] = json!(crate::build::describe(&b));

    let recent = crate::prof::time("s.recent", || super::recent(bot, gs));
    let under_cap = bot._shop_actions < crate::jev_bot::SHOP_ACTION_CAP;
    let fc: Option<Value> = if under_cap { crate::prof::time("s.shop_forecast", || shop_forecast(bot, gs, &recent, jokers_full)) } else { None };
    let fci: Option<Value> = if under_cap { crate::prof::time("s.item_forecast", || item_forecast(bot, gs, fc.as_ref().unwrap_or(&Value::Null))) } else { None };
    if let Some(f) = &fc {
        state["forecast"] = json!(forecast_note(gs, f));
    }
    let ready = crate::prof::time("s.readiness", || crate::readiness::assess(gs, None, 24));
    let worst: Option<f64> = ready.as_ref().map(|r| num(&g(r, "worst_ratio")));
    let worst_v: Value = ready.as_ref().map(|r| g(r, "worst_ratio")).unwrap_or(Value::Null);
    let mode = match worst {
        None => "normal",
        Some(w) if w < super::INVEST_BELOW => "invest",
        Some(w) if w >= super::BANK_ABOVE => "bank",
        _ => "normal",
    };
    let buffer = if mode == "invest" { super::INVEST_FLOOR } else { super::interest_buffer(crate::gs::int(&g(gs, "ante_num")).max(1)) };
    if let Some(r) = &ready {
        let blinds: Vec<String> = g(r, "blinds").as_array().cloned().unwrap_or_default().iter()
            .map(|x| format!("{} ({} chips): {}", rep(&g(x, "blind")), rep(&g(x, "target")), rep(&g(x, "ratio")))).collect();
        let w = rep(&worst_v);
        let plan = match mode {
            "invest" => format!("priority: scoring power, we are behind the next blinds (readiness {w})"),
            "bank" => format!("priority: economy, we are ahead (readiness {w}); buy only clear upgrades"),
            _ => format!("priority: balanced (readiness {w}); keep ~$25 for interest"),
        };
        state["readiness"] = json!({"next_blinds": blinds, "typical_hand": g(r, "typical_hand")});
        state["plan"] = json!(plan);
        bot.emit("info", json!({"msg": format!("Readiness {w} ({mode}): {}", blinds.join("; "))}));
    }

    let shop_cards = cards_of(gs, "shop");
    let voucher_cards = cards_of(gs, "vouchers");
    let pack_cards = cards_of(gs, "packs");
    let joker_cards = cards_of(gs, "jokers");

    let obj = |c: &Value, cost: i64, kind: &str, remove: Option<usize>, key: &str, extra: Vec<(&str, Value)>| -> Value {
        let left = money - cost;
        let mut o = Map::new();
        o.insert("kind".into(), json!(kind));
        o.insert("what".into(), json!(crate::jev_bot::item_desc(c, Some(&mains))));
        o.insert("cost".into(), json!(cost));
        o.insert("money_after".into(), json!(left));
        o.insert("interest_per_round_after".into(), json!(crate::strategy::interest(&json!(left))));
        for (k, v) in extra {
            o.insert(k.into(), v);
        }
        let mut o = Value::Object(o);
        let upd = |o: &mut Value, add: &Value| {
            for (k, v) in _d(add) {
                o.as_object_mut().unwrap().insert(k.clone(), v.clone());
            }
        };
        if let Some(f) = fc.as_ref().filter(|f| _d(&g(f, "options")).contains_key(key)) {
            upd(&mut o, &forecast_fields(f, key));
            if g(c, "key") == json!("j_misprint") {
                o["known_effect"] = json!("Adds a random 0 to 23 Mult each hand; not guaranteed scoring");
            }
            return o;
        }
        if let Some(f) = fci.as_ref().filter(|f| _d(&g(f, "options")).contains_key(key)) {
            // v21: packs, tarots, spectrals
            upd(&mut o, &forecast_fields(f, key));
            o["_fc_item"] = json!(true);
            if kind.starts_with("booster") {
                return o;
            }
        }
        upd(&mut o, &crate::prof::time("s.gain", || super::booster::gain(gs, &recent, c, remove)));
        if let Some(w) = worst {
            let gn = g(&o, "_gain");
            if truthy(Some(&gn)) && matches!(kind, "joker" | "joker swap" | "planet") {
                o["readiness_after"] = json!(format!("{} -> {}", rep(&worst_v), pyfloat(pycompat::pyround(w * (1.0 + num(&gn)), 2))));
            }
        }
        o
    };

    let mut items: Vec<Item> = Vec::new();
    if under_cap {
        for (i, c) in shop_cards.iter().enumerate() {
            let cost = crate::gs::int(&g(&g(c, "cost"), "buy"));
            let set = g(c, "set");
            let neg = g(&g(c, "modifier"), "edition") == json!("NEGATIVE"); // v29: a Negative card brings its own slot
            if cost > money || (set == json!("JOKER") && jokers_full && !neg)
                || (matches!(set.as_str(), Some("TAROT" | "PLANET" | "SPECTRAL")) && cons_full && !neg) {
                continue;
            }
            let kind = if truthy(Some(&set)) { crate::gs::s(&set).to_lowercase() } else { "card".to_string() };
            let mut o = obj(c, cost, &kind, None, &format!("buy_card_{i}"), vec![]);
            let n = crate::tarots::note(c, &b, &json!(money - cost), joker_cards);
            let cash = crate::tarots::cash_outcome(c, &json!(money), &json!(cost), joker_cards);
            if let Some(cash) = cash {
                let mut co = cash.clone();
                co["interest_after_use"] = json!(crate::strategy::interest(&g(&cash, "cash_after_use")));
                o["cash_outcome"] = co;
            }
            if let Some(n) = n.filter(|n| !n.is_empty()) {
                o["use"] = json!(n);
            }
            items.push(Item { key: format!("buy_card_{i}"), desc: crate::jev_bot::item_desc(c, Some(&mains)), obj: o,
                              action: ("buy".into(), Some(json!({"card": i}))), cost });
        }
        for (i, c) in voucher_cards.iter().enumerate() {
            let cost = crate::gs::int(&g(&g(c, "cost"), "buy"));
            if cost <= money && !super::NEVER_VOUCHERS.contains(&crate::gs::s(&g(c, "label"))) {
                let v = super::voucher(&bot.dec, crate::gs::s(&g(c, "label")));
                let mut o = obj(c, cost, "voucher (permanent upgrade for the rest of the run)", None, &format!("buy_voucher_{i}"), vec![]);
                if let Some((worth, strong)) = v {
                    let e = crate::experiment::get();
                    let label_v = g(c, "label"); let label = crate::gs::s(&label_v);
                    let tbl = super::VOUCHERS.iter().any(|v| v.0 == label && v.2);
                    o["worth"] = json!(worth);
                    o["_strong"] = json!(strong || (e.strong_vouchers && tbl) || e.buy_vouchers.iter().any(|b| b == label));
                }
                items.push(Item { key: format!("buy_voucher_{i}"), desc: crate::jev_bot::item_desc(c, None), obj: o,
                                  action: ("buy".into(), Some(json!({"voucher": i}))), cost });
            }
        }
        for (i, c) in pack_cards.iter().enumerate() {
            let cost = crate::gs::int(&g(&g(c, "cost"), "buy"));
            if cost <= money {
                let o = obj(c, cost, "booster pack (pick from its cards)", None, &format!("buy_pack_{i}"), vec![]);
                items.push(Item { key: format!("buy_pack_{i}"), desc: crate::jev_bot::item_desc(c, None), obj: o,
                                  action: ("buy".into(), Some(json!({"pack": i}))), cost });
            }
        }
        if jokers_full {
            for (j, owned) in joker_cards.iter().enumerate() {
                let m = g(owned, "modifier");
                if flag(&m, "eternal") || g(&m, "edition") == json!("NEGATIVE") {
                    continue; // selling a Negative frees no slot
                }
                if let Some(f) = &fc {
                    // v19: swaps only sell our weakest jokers
                    if !g(f, "weak").as_array().map_or(false, |w| w.iter().any(|x| x.as_i64() == Some(j as i64))) {
                        continue;
                    }
                }
                let sell = crate::gs::int(&g(&g(owned, "cost"), "sell"));
                for (i, c) in shop_cards.iter().enumerate() {
                    let cost = crate::gs::int(&g(&g(c, "cost"), "buy"));
                    if g(c, "set") == json!("JOKER") && cost <= money + sell {
                        let k = format!("replace_{j}_{i}");
                        let o = obj(c, cost - sell, "joker swap", Some(j), &k,
                                    vec![("sells", json!(crate::jev_bot::item_desc(owned, Some(&mains)))), ("sell_value", json!(sell))]);
                        items.push(Item { key: k, desc: format!("sell {} for {}", rep(&g(owned, "label")), rep(&g(c, "label"))),
                                          obj: o, action: ("replace".into(), Some(json!({"joker": j, "card": i}))), cost: cost - sell });
                    }
                }
            }
        }
    }
    items.retain(|it| !bot._failed.contains(&dumps(&crate::jev_bot::action_value(&it.action))));
    // lab: JEVLAB_OFFERS=<file> appends each forecast shop's items with their forecast numbers (joker valuation study)
    if let (Some(path), true) = (std::env::var_os("JEVLAB_OFFERS"), fc.is_some()) {
        let card_key = |k: &str| -> Value {
            if k.starts_with("buy_card_") || k.starts_with("replace_") {
                let i: usize = k.rsplit('_').next().unwrap().parse().unwrap();
                g(&shop_cards[i], "key")
            } else {
                Value::Null
            }
        };
        let held: Vec<Value> = crate::gs::cards_of(gs, "jokers").iter().map(|c| g(c, "key")).collect();
        let row = json!({"seed": g(gs, "seed"), "ante": g(gs, "ante_num"), "round": g(gs, "round_num"), "money": money, "held": held,
            "rerolls": bot._rerolls, "items": items.iter().map(|it| json!({"key": it.key, "card": card_key(&it.key), "desc": it.desc, "cost": it.cost,
                "delta": g(&it.obj, "_fc_delta"), "se": g(&it.obj, "_fc_se")})).collect::<Vec<_>>()});
        use std::io::Write;
        if let Ok(mut f) = std::fs::OpenOptions::new().create(true).append(true).open(path) {
            let _ = f.write_all(format!("{row}\n").as_bytes());
        }
    }
    // v61: add the joker's learned value (minus the sold joker's) to its forecast delta: the 3-boss rollouts, whose skip
    // option fills the open slot at the next shop, rated the scaling jokers that win runs (Constellation, Hologram)
    // below buying nothing
    let jp = crate::experiment::get().joker_prior;
    let ante_now = crate::gs::int(&g(gs, "ante_num"));
    if jp != 0.0 && fc.is_some() && ante_now <= 6 && ante_now >= crate::experiment::get().joker_prior_from {
        let held: Vec<String> = crate::gs::cards_of(gs, "jokers").iter().map(|c| crate::gs::s(&g(c, "key")).to_string()).collect();
        for it in items.iter_mut() {
            let parts: Vec<&str> = it.key.split('_').collect();
            let (new_i, sold) = match parts.as_slice() {
                ["buy", "card", i] => (i.parse::<usize>().ok(), None),
                ["replace", j, i] => (i.parse::<usize>().ok(), j.parse::<usize>().ok()),
                _ => (None, None),
            };
            let Some(i) = new_i else { continue };
            let k = crate::gs::s(&g(&shop_cards[i], "key")).to_string();
            if !k.starts_with("j_") || !_d(&it.obj).contains_key("_fc_delta") {
                continue;
            }
            // lab (JOKER_PAIRS): + the pair terms with the jokers kept (the sold one's terms with them go)
            let kept: Vec<&String> = held.iter().enumerate().filter(|(j, _)| Some(*j) != sold).map(|(_, h)| h).collect();
            let with = |x: &str| crate::experiment::joker_value(x) + kept.iter().map(|h| crate::experiment::joker_pair(x, h)).sum::<f64>();
            let mut b = jp * (with(&k) - sold.and_then(|j| held.get(j)).map_or(0.0, |h| with(h)));
            let ps = crate::experiment::get().prior_safe;
            if ps != 0.0 {
                let o = &fc.as_ref().unwrap()["options"];
                let (po, pk) = (fnum(&o[&it.key], "p_survive", 1.0), fnum(&o["skip"], "p_survive", 1.0));
                if pk > 0.0 {
                    b *= (po / pk).min(1.0).powf(ps);
                }
            }
            let d = fnum(&it.obj, "_fc_delta", 0.0) + b;
            it.obj["_fc_delta"] = json!(d);
        }
    }
    if fc.is_some() {
        // v19: hide items the forecast finds clearly worse than buying nothing, unless economic
        let src = |k: &str| -> Value {
            let idx = || k.rsplit('_').next().unwrap().parse::<usize>().unwrap();
            if k.starts_with("buy_voucher_") {
                voucher_cards[idx()].clone()
            } else if k.starts_with("buy_card_") || k.starts_with("replace_") {
                shop_cards[idx()].clone()
            } else {
                json!({})
            }
        };
        let hidden: Vec<String> = items.iter()
            .filter(|it| _d(&it.obj).contains_key("_fc_delta")
                && fnum(&it.obj, "_fc_delta", 0.0) < -(super::HIDE_SE as f64) * fnum(&it.obj, "_fc_se", 0.0).max(1e-9)
                && !economic(&it.obj, &src(&it.key)))
            .map(|it| it.key.clone()).collect();
        if !hidden.is_empty() {
            let descs: Vec<String> = hidden.iter().map(|k| chars_to(&items.iter().find(|i| &i.key == k).unwrap().desc, 30)).collect();
            bot.emit("info", json!({"msg": format!("Forecast hides {}", descs.join("; "))}));
        }
        items.retain(|it| !hidden.contains(&it.key));
    }

    let mut shown: Vec<String> = Vec::new();
    for cards in [shop_cards, voucher_cards, pack_cards] {
        for c in cards {
            let cv = _d(&g(c, "cost")).get("buy").cloned().unwrap_or(json!(0));
            shown.push(format!("${} {}", rep(&cv), crate::jev_bot::item_desc(c, None)));
        }
    }
    bot.emit("info", json!({"msg": format!("Shop (${money}): {}", shown.join("; "))}));

    let rc = g(&g(gs, "round"), "reroll_cost");
    let reroll: i64 = if truthy(Some(&rc)) { crate::gs::int(&rc) } else { 5 };
    // v11: back to v9's rule. v10's early rerolls down to $5 cost ~$7.50 a run in ante 1-2, led to a joker
    // buy only 1 time in 5, and left ante 2-3 balances at $7-12 instead of $16-23 (lost interest).
    // v13: when readiness says we're behind, rerolls hunt for power down to a low floor.
    let mut can_reroll = money - reroll >= (if mode == "invest" { super::INVEST_REROLL_FLOOR } else { super::interest_buffer(crate::gs::int(&g(gs, "ante_num")).max(1)) })
        && bot._rerolls < crate::experiment::get().max_rerolls.unwrap_or(super::MAX_REROLLS);
    if fc.is_some() {
        // v19: reroll when nothing here beats buying nothing by 1 SE, from money above the interest floor
        can_reroll = money - reroll >= super::interest_buffer(crate::gs::int(&g(gs, "ante_num")).max(1)) && bot._rerolls < crate::experiment::get().max_rerolls.unwrap_or(super::MAX_REROLLS)
            && !items.iter().any(|it| !flag(&it.obj, "_fc_item")
                && fnum(&it.obj, "_fc_delta", 0.0) > (super::REROLL_SE as f64) * fnum(&it.obj, "_fc_se", 0.0).max(1e-9));
    }
    let fallback: (&str, Action) = if can_reroll { ("reroll", ("reroll".into(), None)) } else { ("leave", ("next_round".into(), None)) };
    if items.is_empty() {
        bot.emit("forced", json!({"question": "shop", "choice": fallback.0, "desc": "nothing affordable to rate"}));
        return Ok(shop_act(bot, gs, &fallback.1)?);
    }

    let ante = { let a = g(gs, "ante_num"); if truthy(Some(&a)) { crate::gs::int(&a) } else { 1 } };
    if fc.is_some() || fci.is_some() {
        // lab: JEVLAB_EXP=pair-jokers moves Pair jokers PAIR_SE paired SEs up (bar and ranking)
        let eff = |i: usize| -> f64 {
            let o = &items[i].obj;
            let d = fnum(o, "_fc_delta", 0.0);
            let k = &items[i].key;
            if crate::experiment::get().pair_jokers && (k.starts_with("buy_card_") || k.starts_with("replace_"))
                && crate::experiment::is_pair_joker(g(&shop_cards[k.rsplit('_').next().unwrap().parse::<usize>().unwrap()], "label").as_str())
            {
                d + crate::experiment::PAIR_SE * fnum(o, "_fc_se", 0.0).max(1e-9)
            } else {
                d
            }
        };
        // v20: a clear forecast winner is bought by code (policy lab arm C); JEV gets the close calls
        let mut auto: Vec<usize> = (0..items.len()).filter(|&i| {
            let o = &items[i].obj;
            _d(o).contains_key("_fc_delta") && eff(i) > crate::experiment::get().sure_se.unwrap_or(super::SURE_SE as f64) * fnum(o, "_fc_se", 0.0).max(1e-9)
        }).collect();
        // v21: jokers/vouchers/planets first (arm F's order), then packs, tarots and spectrals
        let non_item: Vec<usize> = auto.iter().cloned().filter(|&i| !flag(&items[i].obj, "_fc_item")).collect();
        if !non_item.is_empty() {
            auto = non_item;
        }
        let mut why = "forecast";
        if auto.is_empty() && ante <= super::early_ante() && jok_count < super::early_jokers() {
            // v23: early slots are filled by code. Ante-1/2 blinds are easy enough that no single joker
            // clears the 2 SE bar, so every early joker was a close call, and the code-only bot (top
            // option = buy nothing) died with empty slots on seeds where JEV filled them and went deep.
            auto = (0..items.len()).filter(|&i| {
                let it = &items[i];
                it.key.starts_with("buy_card") && crate::gs::s(&g(&it.obj, "kind")).contains("joker") && eff(i) > 0.0
            }).collect();
            why = "early slot fill";
        }
        if !auto.is_empty() {
            let mut k = auto[0];
            for &i in &auto[1..] {
                if eff(i) > eff(k) {
                    k = i;
                }
            }
            let it = &items[k];
            let o = &it.obj;
            let mut numbers = Map::new();
            for f in ["next_boss_clear", "survive_horizon", "boss_margin", "blinds_cleared_horizon"] {
                if let Some(v) = _d(o).get(f) {
                    numbers.insert(f.into(), v.clone());
                }
            }
            let is_item = flag(o, "_fc_item");
            let src_fc = if is_item { fci.as_ref().unwrap() } else { fc.as_ref().unwrap() };
            let delta = num(&g(o, "_fc_delta"));
            let tail = if why == "forecast" {
                format!(" (> {} SE = {})", super::SURE_SE, fixed(super::SURE_SE as f64 * num(&g(o, "_fc_se")), 2))
            } else {
                format!(" (ante <= {}, {} of {} early jokers)", super::EARLY_ANTE, jok_count, super::EARLY_JOKERS)
            };
            let reason = format!("{}{why}: beats buying nothing by {} blinds{tail}", if is_item { "item " } else { "" }, signed(delta, 2));
            let (ik, idesc, icost, iaction) = (it.key.clone(), it.desc.clone(), it.cost, it.action.clone());
            let msg = format!("Forecast buy {}: {} blinds", chars_to(&idesc, 40), rep(&g(o, "blinds_cleared_horizon")));
            bot.emit("auto", json!({"question": "shop", "choice": ik, "action": crate::jev_bot::action_value(&iaction),
                "desc": format!("${icost} {idesc}"), "reason": reason, "msg": msg, "forecast": Value::Object(numbers),
                "forecast_seconds": g(src_fc, "seconds"), "forecast_rollouts": g(src_fc, "rollouts"),
                "forecast_note": g(&state, "forecast")}));
            return Ok(shop_act(bot, gs, &iaction)?);
        }
    }
    if let Some(k) = planet_rule(gs, &items, money) {
        let it = &items[k];
        let (ik, idesc, icost, iaction) = (it.key.clone(), it.desc.clone(), it.cost, it.action.clone());
        bot.emit("auto", json!({"question": "shop", "choice": ik, "action": crate::jev_bot::action_value(&iaction),
            "desc": format!("${icost} {idesc}"),
            "reason": format!("planet rule: level our top hand / open Celestial packs while ${} stays", super::PLANET_FLOOR)}));
        return Ok(shop_act(bot, gs, &iaction)?);
    }
    let sitems: ShopItems = items.iter().map(|it| (it.key.clone(), (it.desc.clone(), it.obj.clone(), it.action.clone()))).collect();
    let (scores, mut meta) = crate::prof::time("s.score_items", || score_items(bot, gs, "shop", &state, &sitems))?;
    if let Some(f) = &fc {
        meta["forecast_seconds"] = g(f, "seconds");
        meta["forecast_rollouts"] = g(f, "rollouts");
    }
    if let Some(f) = &fci {
        meta["item_forecast_seconds"] = g(f, "seconds");
        meta["item_forecast_rollouts"] = g(f, "rollouts");
    }
    let find = |k: &str| items.iter().find(|i| i.key == k);
    let cash_pick = g(&meta, "cash_step_choice");
    if let Some(it) = cash_pick.as_str().and_then(find) {
        let net = _d(&g(&it.obj, "cash_outcome")).get("net_cash_gain").map_or(0.0, num);
        if net > 0.0 {
            let index = it.action.1.as_ref().unwrap()["card"].clone();
            let ci = index.as_u64().unwrap() as usize;
            let action: Action = ("buy_cash".into(), Some(json!({"card": index, "key": shop_cards[ci]["key"]})));
            let probabilities = g(&meta, "cash_step_probabilities");
            let mut options = Map::new();
            for k in _d(&probabilities).keys() {
                if let Some(i) = find(k) {
                    options.insert(k.clone(), json!(format!("buy and use {}", i.desc)));
                }
            }
            options.insert("continue_shopping".into(), json!("Continue shopping without this cash transaction"));
            let conf = g(&probabilities, crate::gs::s(&cash_pick));
            bot.record_decision("shop", gs, &cash_pick, &crate::jev_bot::action_value(&action), &Value::Object(options),
                                &probabilities, &conf, &meta, "prob");
            return Ok(shop_act(bot, gs, &action)?);
        }
    }
    // A JEV decision to continue shopping passes the explicit cash transactions.
    let cash_probs = g(&meta, "cash_step_probabilities");
    let sc = |k: &str| -> f64 { scores.get(k).map_or(0.0, num) };
    let selectable: Vec<&String> = scores.keys().filter(|k| !_d(&cash_probs).contains_key(*k)).collect();
    let mut best = "";
    for k in &selectable {
        if best.is_empty() || sc(k) > sc(best) {
            best = k;
        }
    }
    let (s, cost) = if !best.is_empty() { (sc(best), find(best).unwrap().cost) } else { (-1.0, 0) };
    let early = ante <= super::early_ante() && jok_count < super::early_jokers();
    let early_joker = early && (best.starts_with("buy_card") || best.starts_with("replace"))
        && crate::gs::s(&g(&find(best).unwrap().obj, "kind")).contains("joker");
    // A joker that measurably lifts our recent hands a lot is bought whenever a slot is free.
    let strong_v: Vec<&Item> = items.iter()
        .filter(|it| flag(&it.obj, "_strong") && mode != "invest"
            && money - it.cost >= crate::experiment::get().voucher_floor.unwrap_or_else(|| super::interest_buffer(ante)))
        .collect(); // v11: from surplus only, never during the joker phase
    let is_joker_buy = |it: &Item| (it.key.starts_with("buy_card") || it.key.starts_with("replace")) && crate::gs::s(&g(&it.obj, "kind")).contains("joker");
    let mut sure: Vec<&Item> = items.iter()
        .filter(|it| is_joker_buy(it) && fnum(&it.obj, "_gain", 0.0) >= super::SURE_GAIN && sc(&it.key) >= 1.0
            && (it.key.starts_with("buy_card") || mode == "invest")) // v13: behind, a big measured swap is sure too
        .collect();
    if fc.is_some() {
        // v19: sure = beats buying nothing by more than 2 SE in blinds cleared over the horizon
        sure = items.iter()
            .filter(|it| _d(&it.obj).contains_key("_fc_delta")
                && fnum(&it.obj, "_fc_delta", 0.0) > crate::experiment::get().sure_se.unwrap_or(super::SURE_SE as f64) * fnum(&it.obj, "_fc_se", 0.0).max(1e-9) && sc(&it.key) >= 1.0)
            .collect();
    }
    let mut loose_ok = money - cost >= buffer || early_joker;
    if mode == "bank" && !early_joker {
        loose_ok = false; // ahead: only clear upgrades (BUY_SCORE) or sure jokers
    }
    let (choice, action): (String, Action);
    let mut infos: Vec<String> = Vec::new();
    if s >= super::BUY_SCORE || (s >= super::BUY_SCORE_LOOSE as f64 && loose_ok) {
        choice = best.to_string();
        action = find(best).unwrap().action.clone();
    } else if !strong_v.is_empty() {
        let it = strong_v[0];
        choice = it.key.clone();
        action = it.action.clone();
        infos.push(format!("Buying voucher {}: {}", chars_to(&it.desc, 40), rep(&g(&it.obj, "worth"))));
    } else if !sure.is_empty() {
        let mut pick = sure[0];
        let metric = if fc.is_some() { "_fc_delta" } else { "_gain" };
        for it in &sure[1..] {
            if fnum(&it.obj, metric, 0.0) > fnum(&pick.obj, metric, 0.0) {
                pick = it;
            }
        }
        choice = pick.key.clone();
        if fc.is_some() {
            infos.push(format!("Buying {}: {} blinds", chars_to(&pick.desc, 40), rep(&g(&pick.obj, "blinds_cleared_horizon"))));
        } else {
            infos.push(format!("Buying {}: +{}% on recent hands", chars_to(&pick.desc, 40), pyround_int(100.0 * num(&g(&pick.obj, "_gain")))));
        }
        action = pick.action.clone();
    } else {
        choice = fallback.0.to_string();
        action = fallback.1.clone();
    }
    for m in infos {
        bot.emit("info", json!({"msg": m}));
    }
    let mut options = Map::new();
    for it in &items {
        options.insert(it.key.clone(), json!(format!("${} {}", it.cost, it.desc)));
    }
    let mut probs = Map::new();
    for (k, v) in &scores {
        probs.insert(k.clone(), json!(num(v) / 4.0));
    }
    bot.record_decision("shop", gs, &json!(choice), &crate::jev_bot::action_value(&action), &Value::Object(options),
                        &Value::Object(probs), &json!(s / 4.0), &meta, "score");
    Ok(shop_act(bot, gs, &action)?)
}

// decisions.py 826-848: runs the action; on RPCError adds json.dumps(action) to bot._failed and returns Err
pub(crate) fn shop_act(bot: &mut JevBot, gs: &Value, action: &Action) -> Result<(), RpcError> {
    bot._shop_actions += 1;
    if action.0 == "reroll" {
        bot._rerolls += 1;
    }
    let params = action.1.clone().unwrap_or(Value::Null);
    if action.0 == "buy" && _d(&params).contains_key("pack") {
        let idx = crate::gs::int(&params["pack"]) as usize;
        bot._last_pack_cost = _d(&g(&cards_of(gs, "packs")[idx], "cost")).get("buy").cloned().unwrap_or(json!(0));
    }
    let r: Result<(), RpcError> = (|| {
        match action.0.as_str() {
            "buy_cash" => {
                let after = bot.act("buy", Some(&json!({"card": params["card"]})))?;
                // Locate the acquired card in the returned state; do not assume its slot.
                let want = &params["key"];
                let matches: Vec<usize> = cards_of(&after, "consumables").iter().enumerate()
                    .filter(|(_, c)| g(c, "key") == *want).map(|(i, _)| i).collect();
                if matches.is_empty() {
                    return Err(RpcError { code: -1, message: "cash_tarot_missing: Purchased cash tarot is absent from the returned state".into(),
                                          name: Some("cash_tarot_missing".into()) });
                }
                bot.act("use", Some(&json!({"consumable": matches[matches.len() - 1]})))?;
            }
            "replace" => {
                bot.act("sell", Some(&json!({"joker": params["joker"]})))?;
                bot.act("buy", Some(&json!({"card": params["card"]})))?;
            }
            _ => {
                bot.act(&action.0, action.1.as_ref())?;
            }
        }
        Ok(())
    })();
    if let Err(e) = r {
        bot._failed.insert(dumps(&crate::jev_bot::action_value(action)));
        return Err(e);
    }
    Ok(())
}
