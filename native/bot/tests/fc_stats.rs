//! Identity checks for src/forecast_stats.rs against the Python fixture scripts/port/fixtures/fc_stats.py
//! (runs/_port/fixtures/forecast_stats.jsonl.gz). Options are checked against the public states of
//! the reference traces (the pub is rebuilt from each oracle line's preceding call, as load_trace does).

use jevbot::forecast_stats as fs;
use jevbot::trace::{fixtures_dir, load_trace, read_jsonl, short, trace_files};
use serde_json::{json, Value};

fn same(a: &Value, b: &Value) -> bool {
    serde_json::to_string(a).unwrap() == serde_json::to_string(b).unwrap()
}

fn rows() -> Vec<Value> {
    read_jsonl(&fixtures_dir().join("forecast_stats.jsonl.gz"))
}

struct Tally {
    name: &'static str,
    n: usize,
    bad: Vec<String>,
}

impl Tally {
    fn new(name: &'static str) -> Tally {
        Tally { name, n: 0, bad: Vec::new() }
    }
    fn check(&mut self, label: &str, got: &Value, want: &Value) {
        self.n += 1;
        if !same(got, want) {
            self.bad.push(format!("{label}: got {} want {}", short(got, 300), short(want, 300)));
        }
    }
    fn report(&self) {
        println!("{}: {}/{} identical", self.name, self.n - self.bad.len(), self.n);
        for b in self.bad.iter().take(5) {
            println!("  {b}");
        }
        assert!(self.bad.is_empty(), "{}: {} mismatches", self.name, self.bad.len());
    }
}

fn keys_arg(v: &Value) -> Option<Vec<String>> {
    v.as_array().map(|a| a.iter().map(|k| k.as_str().unwrap().to_string()).collect())
}

#[test]
fn options_and_filtered_keys_from_traces() {
    let fx = rows();
    let mut want_opts = fx.iter().filter(|r| r["fn"] == "options");
    let mut want_filt = fx.iter().filter(|r| r["fn"] == "filtered");
    let mut t_opt = Tally::new("options(pub) vs trace pub");
    let mut t_filt = Tally::new("filtered option keys vs recorded ret.options");
    for arm in ["bot-forecast-items", "bot-forecast-long"] {
        for path in trace_files(arm) {
            let mut pub_ = Value::Null;
            for r in load_trace(&path) {
                if r["k"] == "call" {
                    pub_ = r.get("ok").cloned().unwrap_or(Value::Null);
                    continue;
                }
                if r["k"] != "oracle" {
                    continue;
                }
                let mode = match r["fn"].as_str() {
                    Some("rollout.forecast") => "rollout",
                    Some("rollout_items.forecast") => "items",
                    _ => continue,
                };
                let o = want_opts.next().expect("fixture options row");
                let label = format!("{arm} {mode} {}", path.display());
                t_opt.check(&format!("{label} args.pub"), &o["args"]["pub"], &pub_);
                let opts = if mode == "rollout" { fs::options(&pub_) } else { fs::items_options(&pub_) };
                t_opt.check(&label, &Value::Array(opts.clone()), &o["ret"]);

                let f = want_filt.next().expect("fixture filtered row");
                let keys = keys_arg(&r["args"]["keys"]);
                let mut kept: Vec<String> = opts
                    .iter()
                    .filter(|o| mode == "items" || o["supported"] == true)
                    .map(|o| o["key"].as_str().unwrap().to_string())
                    .filter(|k| keys.as_ref().map_or(true, |ks| ks.contains(k) || k == "skip"))
                    .collect();
                let errors: Vec<String> = f["ret"]["errors"].as_array().unwrap().iter().map(|e| e.as_str().unwrap().to_string()).collect();
                kept.retain(|k| !errors.contains(k));
                t_filt.check(&label, &json!(kept), &f["ret"]["recorded"]);
            }
        }
    }
    assert!(want_opts.next().is_none() && want_filt.next().is_none(), "fixture has extra rows");
    t_opt.report();
    t_filt.report();
}

#[test]
fn summarize_paired_and_assembly_from_recorded_runs() {
    let fx = rows();
    let mut t_stats = Tally::new("summarize + paired/paired_delta vs skip (rollout _runs)");
    let mut t_asm = Tally::new("assemble (forecast result minus seconds/_runs)");
    let mut t_items = Tally::new("best(fc) (rollout_items ret)");
    let mut t_jok = Tally::new("option_jokers");
    let mut t_use = Tally::new("usable");
    for r in &fx {
        match r["fn"].as_str().unwrap() {
            "stats" => {
                let bosses = r["args"]["bosses"].as_i64().unwrap();
                let runs = r["args"]["runs"].as_object().unwrap();
                let base = fs::runs_list(&runs["skip"]);
                for (key, rs) in runs {
                    let rs = fs::runs_list(rs);
                    let label = format!("summarize {key}");
                    t_stats.check(&label, &fs::summarize(&rs, bosses), &r["ret"]["summarize"][key]);
                    let v = &r["ret"]["vs_skip"][key];
                    let pr = |(m, s): (f64, f64)| json!([m, s]);
                    t_stats.check(&format!("{label} next_boss"), &pr(fs::paired(&rs, &base, fs::m_next_boss)), &v["next_boss"]);
                    t_stats.check(&format!("{label} survive"), &pr(fs::paired(&rs, &base, |x| fs::m_survive(x, bosses))), &v["survive"]);
                    t_stats.check(&format!("{label} blinds"), &pr(fs::paired(&rs, &base, fs::m_blinds)), &v["blinds"]);
                    t_stats.check(&format!("{label} delta"), &pr(fs::paired_delta(&rs, &base, bosses)), &v["delta"]);
                }
            }
            "assemble" => {
                let pub_ = &r["args"]["pub"];
                let runs = r["args"]["runs"].as_object().unwrap();
                let keys = keys_arg(&r["args"]["keys"]);
                let got = fs::assemble(
                    &fs::options(pub_),
                    keys.as_deref(),
                    runs,
                    r["args"]["bosses"].as_i64().unwrap(),
                    r["args"]["rollouts"].as_i64().unwrap(),
                );
                t_asm.check("assemble", &got, &r["ret"]);
            }
            "best" => {
                let got = fs::best(&r["args"]["fc"], 2.0);
                t_items.check("best", &json!(got), &r["ret"]);
            }
            "option_jokers" => {
                let got = fs::option_jokers(&r["args"]["pub"], &r["args"]["opt"]);
                t_jok.check("option_jokers", &Value::Array(got), &r["ret"]);
            }
            "usable" => {
                let got = fs::usable(r["args"]["key"].as_str());
                t_use.check(&format!("usable {}", r["args"]["key"]), &json!(got), &r["ret"]);
            }
            _ => {}
        }
    }
    t_stats.report();
    t_asm.report();
    t_items.report();
    t_jok.report();
    t_use.report();
}
