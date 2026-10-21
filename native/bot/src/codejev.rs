//! Port of the lab's code stand-in for JEV: scripts/policy_lab.py `_code_jev` (CodeJev, with
//! `item_pick`/`item_score`, model "code-top-option") and scripts/sim_run.py `FirstOptionJev`
//! (model "greedy-first-option"). Both answer without any API call.

use crate::gs;
use crate::iface::{Jev, JevResponse, SharedOracle};
use serde_json::{json, Map, Value};

/// policy_lab.FORECAST_N.
pub const FORECAST_N: i64 = 16;

fn choice_answer(keys: &[String], c: &str) -> Value {
    let probs: Map<String, Value> = keys.iter().map(|k| (k.clone(), json!(if k == c { 1.0 } else { 0.0 }))).collect();
    json!({"type": "choice", "choice": c, "confidence": 1.0, "probabilities": Value::Object(probs)})
}

fn score_answer(score: f64) -> Value {
    json!({"type": "score", "score": score, "confidence": 1.0, "legend": {}, "probabilities": {}})
}

fn criteria_keys(q: &Value) -> Vec<String> {
    gs::d(gs::get(q, "criteria")).keys().cloned().collect()
}

fn response(model: &str, answers: Map<String, Value>) -> JevResponse {
    JevResponse { model: model.to_string(), answers, input_tokens: 0, output_tokens: 0 }
}

/// sim_run.FirstOptionJev: Choice -> first option, Noul -> 1.0, Score -> 0.5.
pub struct FirstOptionJev;

impl Jev for FirstOptionJev {
    fn system_one(&mut self, _state: &Value, questions: &Map<String, Value>) -> Result<JevResponse, String> {
        let mut answers = Map::new();
        for (qid, q) in questions {
            let a = match gs::s(gs::get(q, "type")) {
                "choice" => {
                    let keys = criteria_keys(q);
                    choice_answer(&keys, &keys[0])
                }
                "noul" => json!({"type": "noul", "noul": 1.0}),
                _ => score_answer(0.5),
            };
            answers.insert(qid.clone(), a);
        }
        Ok(response("greedy-first-option", answers))
    }
}

#[derive(Default, Debug)]
pub struct CodeJevStats {
    pub item_forecasts: i64,
    pub item_buys: Vec<Value>,
    pub pack_picks: Vec<Value>,
}

/// v55: play now unless the best discard's modeled next play beats the best play by 1 / PLAY_NOW_RATIO (was: by
/// anything). A play also replaces its cards, and a discard kept is worth a lot later in the round; the modeled
/// next play itself is not optimistic (docs/lab-lessons.md). 2 x 2,000 seeds: 862 vs 659 and 825 vs 640 wins.
pub const PLAY_NOW_RATIO: f64 = 0.6;

/// policy_lab `_code_jev(bot_ref, items)`. `last` is the bot's `last_state` (set through `Jev::observe`).
pub struct CodeJev {
    pub items: bool,
    pub oracle: SharedOracle,
    pub last: Value,
    pub stats: CodeJevStats,
}

impl CodeJev {
    pub fn new(items: bool, oracle: SharedOracle) -> Self {
        CodeJev { items, oracle, last: Value::Null, stats: CodeJevStats::default() }
    }

    /// `{qid: score}` for arm F picks on this question set (keys in insertion order).
    fn item_pick(&mut self, questions: &Map<String, Value>) -> Vec<(String, f64)> {
        let g = self.last.clone();
        let st = gs::get(&g, "state").as_str();
        if st == Some("SMODS_BOOSTER_OPENED") && questions.keys().any(|q| q.starts_with("take_")) {
            let p = self.oracle.borrow_mut().pick_pub(&g);
            if gs::truthy(&p) {
                let idx = gs::int(&p.as_array().expect("pick_pub list")[0]);
                let c = &gs::cards_of(&g, "pack")[idx as usize];
                self.stats.pack_picks.push(json!([gs::get(&g, "ante_num"), gs::get(c, "set"), gs::get(c, "label")]));
                return vec![(format!("take_{idx}"), 4.0)];
            }
            return vec![];
        }
        if st != Some("SHOP") {
            return vec![];
        }
        let shop = gs::cards_of(&g, "shop");
        let mut keys: Vec<String> = questions
            .keys()
            .filter(|q| {
                q.starts_with("buy_pack_")
                    || (q.starts_with("buy_card_")
                        && matches!(
                            gs::get(&shop[q.rsplit('_').next().unwrap().parse::<usize>().unwrap()], "set").as_str(),
                            Some("TAROT") | Some("SPECTRAL")
                        ))
            })
            .cloned()
            .collect();
        if keys.is_empty() {
            return vec![];
        }
        keys.sort();
        let fc = self.oracle.borrow_mut().items_forecast(&g, FORECAST_N, 3, &keys, None);
        self.stats.item_forecasts += 1;
        // rollout_items.best(fc): the option beating skip by more than 1 paired SE (v31; was 2) in blinds cleared
        // (largest gain, first on ties).
        let options = gs::get(&fc, "options");
        let mut best: Option<(&String, f64)> = None;
        for (k, r) in gs::d(options) {
            if k == "skip" {
                continue;
            }
            let Some(vs) = r.get("vs_skip") else { continue };
            let pair = gs::list(gs::get(vs, "blinds"));
            let (d, s) = (gs::num(&pair[0]), gs::num(&pair[1]));
            if d > 1.0 * s.max(1e-9) && best.map_or(true, |(_, bd)| d > bd) {
                best = Some((k, d));
            }
        }
        let Some((k, _)) = best else { return vec![] };
        let o = gs::get(options, k);
        self.stats.item_buys.push(json!([gs::get(&g, "ante_num"), gs::get(o, "desc"), gs::get(gs::get(o, "vs_skip"), "blinds")]));
        vec![(k.clone(), 3.0)]
    }

    fn item_score(&self, q: &Value) -> f64 {
        let instr = gs::get(q, "instructions");
        let item = gs::get(instr, "item"); // {} unless instructions is a dict
        // max(hands.items(), key=(played or 0, level or 0)): first maximum
        let mut top: Option<(&String, &Value, (f64, f64))> = None;
        for (name, h) in gs::d(gs::get(&self.last, "hands")) {
            let key = (gs::num(gs::get(h, "played")), gs::num(gs::get(h, "level")));
            if top.map_or(true, |(_, _, tk)| key.0 > tk.0 || (key.0 == tk.0 && key.1 > tk.1)) {
                top = Some((name, h, key));
            }
        }
        if let Some((name, h, _)) = top {
            if gs::get(item, "kind").as_str() == Some("planet")
                && gs::num(gs::get(h, "played")) > 0.0
                && gs::s(gs::get(item, "what")).contains(&format!("Level up {name} "))
                && gs::num(gs::get(item, "money_after")) >= 4.0
            {
                return 4.0;
            }
        }
        0.0
    }
}

/// Lab (`setup-play=avg|clear`): out of discards with hands to spare, the plays carry a next-hand outlook; pick the
/// best two-play average (`avg`), or the best chance that the two plays clear, then the average
/// (`clear`). Default: the best immediate score (the first option).
fn setup_pick(q: &Value) -> Option<String> {
    let mode = crate::experiment::get().setup_play.as_deref()?;
    let crit = gs::d(gs::get(q, "criteria"));
    let mut best: Option<(&String, (f64, f64, f64))> = None;
    for (k, o) in crit {
        let ol = gs::get(o, "next_hand_outlook");
        if !gs::truthy(ol) {
            continue;
        }
        let avg = gs::num(gs::get(ol, "average_two_play_total"));
        let poor = gs::num(gs::get(ol, "poor_draw_two_play_total"));
        let clear: f64 = gs::s(gs::get(ol, "chance_two_plays_clear")).trim_end_matches('%').parse().unwrap_or(0.0);
        let key = if mode == "clear" { (clear, avg, poor) } else { (avg, poor, 0.0) };
        if best.map_or(true, |(_, b)| key > b) {
            best = Some((k, key));
        }
    }
    best.map(|(k, _)| k.clone())
}

impl Jev for CodeJev {
    fn observe(&mut self, last_state: &Value) {
        self.last = last_state.clone();
    }

    fn system_one(&mut self, state: &Value, questions: &Map<String, Value>) -> Result<JevResponse, String> {
        let mut answers = Map::new();
        let picks = if self.items { self.item_pick(questions) } else { vec![] };
        for (qid, q) in questions {
            let a = match gs::s(gs::get(q, "type")) {
                "choice" => {
                    let keys = criteria_keys(q);
                    let has_cs = keys.iter().any(|k| k == "continue_shopping");
                    let mut c = if has_cs { "continue_shopping".to_string() } else { keys[0].clone() };
                    if qid == "best_play" {
                        if let Some(k) = setup_pick(q) {
                            c = k;
                        }
                    }
                    let cash: Vec<&String> = picks.iter().map(|p| &p.0).filter(|k| keys.contains(k)).collect();
                    if has_cs && !cash.is_empty() {
                        c = cash[0].clone();
                    }
                    choice_answer(&keys, &c)
                }
                "noul" => {
                    let sp = gs::get(state, "strongest_play");
                    let bd = gs::get(state, "best_draw_by_measurement");
                    let e = crate::experiment::get();
                    let rnd = gs::get(state, "round");
                    let spare = gs::num(gs::get(rnd, "discards_left")) >= gs::num(gs::get(rnd, "hands_left"));
                    let ratio = match e.play_now_spare {
                        Some(r) if spare => r,
                        _ => e.play_now_ratio.unwrap_or(PLAY_NOW_RATIO),
                    };
                    let play = gs::truthy(gs::get(sp, "clears_blind_alone"))
                        || gs::d(bd).get("modeled_average_next_play").is_none()
                        || gs::num(gs::get(sp, "estimated_score")) >= ratio * gs::num(gs::get(bd, "modeled_average_next_play"));
                    json!({"type": "noul", "noul": if play { 1.0 } else { 0.0 }})
                }
                _ => {
                    let pick = picks.iter().find(|p| &p.0 == qid).map_or(0.0, |p| p.1);
                    score_answer(self.item_score(q).max(pick))
                }
            };
            answers.insert(qid.clone(), a);
        }
        Ok(response("code-top-option", answers))
    }
}
